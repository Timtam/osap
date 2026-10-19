---
title: "host.window — windows, controls and focus"
sidebar_position: 20
toc_max_heading_level: 2
---

Finds windows and the surfaces inside them, reports where the keyboard is, and raises an event when either changes. In short: which window is in front, what surfaces sit inside it, where the keyboard actually is, and whether a point on screen still belongs to the window you think it does.

Most modules never call any of it — an overlay's binding does the finding, and the overlay runtime registers `onTrigger` and `onFocus` once on everyone's behalf — so you come here for the question a binding cannot settle by itself: Kontakt walks `controls()` for the Qt container that says which Kontakt this is and whether a Komplete Kontrol wraps it, the overlay runtime asks `focusChain()` whether the keyboard is on a DAW's own chrome or inside the plug-in's panel on a Mac, and daw-hosts pairs `find` with `focus` to build the "put me back in the plug-in window" key that macOS otherwise has no command for.

Nothing here touches the screen, so one call is cheap — but every overlay makes several on every foreground and focus change. Kontakt measured a single cell's context match at 15 to 96 ms with every underlying OS answer already served from cache, which is thousands of cheap calls rather than one expensive one, and why its six cells derive the whole scene once per epoch instead of each walking the control list.

The same call answers with different things per platform, so read the platform sections: `controls()` yields every visible child window on Windows and container elements only on macOS, and a focus chain's links are windows there and accessibility elements here — test what a chain contains rather than how deep it is. Believe what you are told rather than what you assume: `focus` can be declined by either platform, and `ownsPoint` answers `nil` on macOS, which a caller must read as permission rather than refusal.

**Some answers are kept for the rest of the epoch.** `active()`, `controls()` and `focusChain()` — like `host.element.find`, `findAny` and `pluginLocate` — ask the operating system once per [epoch](./timer.md#host-epoch) and hand every later call in the same epoch the same answer, a `nil` included. The epoch turns over on OS events (a hotkey, a captured key, a foreground or focus change, a controller event), on a [`host.timer.after`](./timer.md#host-timer-after) callback coming due, on an image result arriving, on input the platform drives, and on `host.window.focus`, so a callback normally sees the world as it was when the callback started. It does **not** turn over on a [`host.timer.every`](./timer.md#host-timer-every) tick: a poll is handed whatever was asked in the current epoch, which may be from before the last tick, unless something else turned it over. Nor does [`host.input.post`](./input.md#host-input-post) turn it over: a `focusChain()` read after a post in the same callback is the answer from before the key. Nor does a window that hides or closes without an event. `list`, `find`, `findAll`, `apps`, `windowsOf` and [`foreground`](#host-window-foreground) are asked afresh on every call.

All coordinates are physical device pixels on Windows and **points** on macOS (i32 → Luau `number`) unless noted; the two agree only at 100 % scaling — see [`host.screen.size()`](screen#host-screen-size) for why that distinction costs a day when it is missed. `id` fields are native window handles on Windows (a Win32 `HWND` as a Luau integer) and interned counters on macOS; see the platform sections under [Table shapes](#table-shapes).

`list`, `apps`, `windowsOf`, `active`, `foreground`, `focus`, `controls`, `focusChain`, `ownsPoint` and `recheck` are native bindings; `find`, `findAll`, `test`, `onTrigger` and `onFocus` are added on top of them by the Luau prelude. The prelude also defines `W._hasTriggers()`, `W._triggerSeq()`, `W._wantsInitial(reprime)`, `W._dispatchActivate(win, beforeFirst, waited, upto)`, `W._dispatchInitial(win, reprime, beforeFirst, waited)` and `W._dispatchFocus(waited)`, and the host adds `W._requestInitial()`, which `onTrigger { initial = true }` calls to queue its module for the report. The host's event loop calls the dispatchers to deliver foreground and focus changes, and the initial report, into the registered `onTrigger` and `onFocus` callbacks; a module calls none of them.

**What a call answers.** Only [`focus`](#host-window-focus) answers an [error value](./errors.md#error-values): `false, err` when it could not bring the window forward, its codes under its **Errors**. Every other call here answers what there is, and never an error value: each entry's **Returns** says what its `nil`, `false` or empty list means. Compare `err.code`, never the message: a message may change between versions, and the code does not. Besides the mistakes each entry lists under **Raises**, two raises can meet any call here: reaching for `host.window` in a module that did not declare `window` ([What to declare](#declare)), and the host's guard stopping a callback that ran too long (`stopped by the host: …`, [the limits](../module-runtime-and-lifecycle.md#limits)). An argument of the wrong type raises with the host's conversion message, such as `bad argument #1: error converting Lua nil to isize (expected number or string coercible to number)`; a number with a fraction is cut toward zero where a whole number goes.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["window"]
```

See [what that list is and is not](./index.md#capabilities).

## Table shapes {#table-shapes}

### Window table

Returned by `host.window.list()`, `host.window.active()`, `host.window.find()`, `host.window.findAll()`, and passed to trigger/test callbacks. Built by `win_to_table`:

```luau
{
  id    = 0x000A12,        -- native window handle (HWND), integer
  title = "REAPER",        -- window title
  class = "REAPERwnd",     -- window class name
  own   = false,           -- a window of this application's own process
  app = {
    name = "reaper",       -- exe stem (filename without extension)
    exe  = "reaper.exe",   -- executable file name
    pid  = 12345,          -- process id
  },
  bounds = { x = 0, y = 0, w = 1920, h = 1040 },  -- window rect (screen px)
  client = { x = 8, y = 31, w = 1904, h = 1001 },  -- client area (screen px)
}
```

`own` is `true` for a window of this application's own process — the module manager, a dialog of the host's — and `false` for every other: the host compares the window's process id with its own, on both platforms, and asks nothing else. The [overlay runtime](./overlay.md#bindings) lets go of the front on it.

`client` is the window's client area in screen pixels: `x`/`y` its origin, which overlay regions are expressed relative to, and `w`/`h` its size. It is also what a window region reads: `{ window = w, fraction = { x1, y1, x2, y2 } }` is a rectangle in fractions of `client.w` and `client.h`, from `client.x` and `client.y` (see [the Region form](./screen.md#region-form)), so the table must come from `host.window`, or at least carry a `client` with those four whole numbers. Every `host.screen` and `host.ocr` call that takes a region takes that form, and [`host.screen.pixel`](./screen.md#host-screen-pixel) a point `{ window = w, fraction = { x, y } }`; an [overlay](./overlay.md) control's `region` and `ocrLabel` options take only corners, relative to the overlay's origin. The fractions resolve against the `client` the table carries at the call, never a fresh one: a table from `active()` is the one kept for the epoch (above), so in a [`host.timer.every`](./timer.md#host-timer-every) poll the rectangle is the one the window had when the epoch last turned over, and a window moved or resized since then without an event in between is read at its old place. `find` asks the operating system afresh on every call.

### Control table

Returned by `host.window.controls()` and `host.window.focusChain()`. Built by `control_to_table`:

```luau
{
  id     = 0x001B44,                            -- control handle (HWND), integer
  class  = "Qt5152QWindowIcon",                 -- control window class
  bounds = { x = 100, y = 200, w = 640, h = 480 }, -- control rect (screen px)
  client = { x = 108, y = 231, w = 624, h = 441 }, -- control client area (screen px)
}
```

Note: a control table has no `title` / `app` fields — only `id`, `class`, `bounds`, `client`.

### Windows {#table-shapes-windows}

`class` is the real Win32 class from `GetClassNameW` — `"REAPERwnd"`, `"Qt5152QWindowIcon"`, `"#32770"`. `app.exe` is the executable's file name (`"reaper.exe"`) and `app.name` its stem; `app.bundleId` is always `""`. `id` is a real `HWND`, which the operating system **reuses**, so a stale id can quietly come to mean a different window. `client` is `GetClientRect` translated to the screen.

### macOS {#table-shapes-macos}

`class` is a synthesised triple, `AXRole/AXSubrole/AXIdentifier`, with both separators always present and missing parts left empty: `"AXWindow/AXStandardWindow/"`, `"AXGroup//NI.Kontakt.Main"`. It is never the empty string — an element with no role at all yields `"//"`. A Windows class pattern therefore cannot match here, and the overlay simply never activates with nothing to say why; `host.os.pick` is how a matcher carries both.

`app.exe` is the binary *inside* the bundle, named by the vendor's build system — Ableton is `Ableton Live 11 Suite.exe` on Windows and `Live` here. With no extension to strip, `app.name` equals `app.exe`, and if the executable URL cannot be read it falls back to the **localised** application name, which changes with the user's system language. `app.bundleId` carries the stable identity (`"com.native-instruments.Kontakt8"`) and is the field to match on.

`id` is a small interned counter, meaningful only to this platform and **never reused** — nothing outside it accepts one, and a handle whose process has exited stays permanently unmatched rather than silently matching something else. The host forgets the handles of an application when the workspace reports that it quit. It also sweeps its table of handles: first when the table reaches 4,096 entries, after that each time it reaches twice what the last sweep kept (and never below 4,096). A sweep runs on the event loop after that turn's events have been delivered, never inside a call, and drops the handles of processes that have exited and of windows that no longer exist, with every control inside them: a window paired with a window-server window number that the window server still lists is kept without asking anybody, and any other window is asked of its application — at most 256 windows and 150 ms a sweep, though one application that stops answering can hold one question for its one-second accessibility timeout. A forgotten handle is unmatched too — [`controls`](#host-window-controls), for one, answers it with nothing. The window table's executable name and bundle id of an application are forgotten when the workspace reports it quit, so a later process given the same process id is then described as itself. `client` is derived rather than read: a titled window's content rect is worked out from its own geometry, so on a borderless plug-in window it equals the frame.

**This application's own windows** — the module manager, a dialog, a [page](./gui.md#host-gui-webview) — are described by AppKit, never by an accessibility question to this application, which would be the event loop asking itself. Their `id` is 2⁴⁰ (1099511627776) plus the window server's number for the window — the `id` [`windowsOf`](#host-window-windowsof) lists it under — so it never meets the counter above and stays the same for the life of the window. `class` is the window's own accessibility role, subrole and identifier, read inside the application (`"AXWindow/AXStandardWindow/"`); `bounds` and `client` are AppKit's frame and content rectangle, flipped to the top-left of the primary display, so `client` is exact here; `app` is this application. A hidden or minimised one is left out, as any other is. [`controls`](#host-window-controls) and `host.element` find nothing inside one.

## Matchers {#matchers}

A *matcher* is a declarative table passed to `host.window.find/findAll/test/onTrigger`, and to the overlay's bindings. It is a plain Luau table — there is no constructor for it — and these are **all the keys that are read** (all optional; all must match):

- `title` — a **field matcher** tested against the window title.
- `app` — table with any of `name` / `exe` / `bundleId` (field matchers) and `pid` (exact number). `bundleId` is empty on Windows, so asking for it there never matches; put it in the `macos` block.
- `os` — a list of platform names, `{ "windows", "macos" }`; on any other platform the matcher fails.
- `windows` / `macos` / `linux` — per-OS blocks. If any platform block is present but the current OS is not among them, the matcher fails. The current-OS block may contain `title` and `app` (as above), `class` (a field matcher against the window class), and on macOS `axRole`, `axSubrole` and `axIdentifier` (field matchers against the three parts of the macOS `class` string).
- `where` — a `function(win) -> boolean` escape-hatch predicate, evaluated last.

**Every other key is ignored without an error.** That includes a `class` at the top level, outside a platform block: `{ app = { name = "reaper" }, class = "REAPERwnd" }` matches every window REAPER has. The window class lives in the `windows` or `macos` block, where AutoHotkey's `ahk_class` would be. Keys from other designs — `controlClass`, `wmClass`, `exe` at the top level — are ignored the same way. A window table is plain data too: it has fields and no methods.

**The keys above of the wrong type** are checked only by [`onTrigger`](#host-window-ontrigger), which raises when it registers. `find`, `findAll`, `test` and the overlay's bindings do not check: there an `app` given as a string constrains nothing — `{ app = "game.exe" }` matches every window — and neither does a string block for the platform in use (a block for another platform still makes the matcher fail, as any would); a `where` that is not a function is skipped; and an `os` that is not a table, an `app` that is a number or a boolean, or such a block for the platform in use, raises from inside the call.

A **field matcher** is either a string (case-insensitive equality) or a table with one of:

- `exact` — case-insensitive equality
- `contains` / `prefix` / `suffix` — case-insensitive substring/edge match
- `pattern` / `regex` — Luau string pattern (both evaluated as Luau patterns; **case-sensitive**)
- `not` — negates a nested field matcher

A table with none of these keys — a misspelt `{ equals = "REAPER" }` — matches nothing, silently. A table with several uses the first one present in the order `not`, `exact`, `contains`, `prefix`, `suffix`, `pattern`, `regex`, and ignores the rest. A field matcher may itself be keyed by platform — `title = { windows = "REAPER", macos = { prefix = "REAPER" } }` — and then matches nothing on a platform it does not name.

```luau
-- Match REAPER's main window on Windows only, title containing "REAPER".
local matcher = {
  title = { contains = "REAPER" },
  app = { name = "reaper" },
  windows = { class = "REAPERwnd" },
  where = function(w) return w.bounds.w > 400 end,
}
```

---

## host.window.list(filter?) {#host-window-list}

**Signature:** `host.window.list(filter: { pids: { number }? }?) -> { Window }`

Returns an array of [window tables](#window-table) for all enumerable top-level windows — or, with `{ pids = {…} }`, only the windows of those processes. `find` and `findAll` pass the pids of the applications a matcher names, so a module rarely calls this directly; when it does, and it knows the application, it should narrow the same way.

```luau
for _, w in ipairs(host.window.list()) do
  host.log.info(w.title .. " [" .. w.class .. "]")
end

-- Only REAPER's windows, without asking any other application anything.
local reaper = host.window.list({ pids = { reaperPid } })
```

### Returns {#host-window-list-returns}

- `{ Window }` — a list of [window tables](#window-table), in the order the platform lists them. Empty when nothing is listed: no window, no window of the processes in `pids`, or `pids = {}`. Never `nil`.
- Which windows are listed differs by platform (below). On macOS an application that does not answer in time is left out, and the list does not say so: the log does.

### Errors {#host-window-list-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: a `filter` that is neither a table nor `nil`; a `pids` that is neither a table nor `nil`; an entry of `pids` that is not a number from 0 to 4294967295. Every other key of `filter` is ignored.

### Windows {#host-window-list-windows}

`EnumWindows` — local, microseconds, and the filter merely drops entries. There is no cost to leaving it out.

Only **visible windows with a title** are listed, so `find` and `findAll` never return an untitled window — Komplete Kontrol's untitled `#32770` save dialog, for one. `active()` is the one call that also returns an untitled window as a window table; [`foreground()`](#host-window-foreground) names the foreground window by id whether it has a title or not, and whether it is visible or not.

### macOS {#host-window-list-macos}

Every application listed is a call into that process for its `AXWindows`, each with its own timeout, on the thread that carries the event tap. Unfiltered, the listing asks every running application that could own a window, and gives each of them **a quarter of a second** rather than the process-wide second — the third Mac session measured 2.0–2.3 s of every 3 s probe stall going to one unrelated process that never answered, and the tap was switched off each time. An application that misses that short deadline is left out of the listing and **named in the log, but not quarantined**: only a miss at the full timeout marks an application as not answering for the paths that consult that. With `pids`, the named applications are asked at the **full** timeout, because those are the ones the caller is waiting on and a slow-but-alive plugin must not read as absent. An application that takes 100 ms or more is named in the log either way, with how many windows it listed. One listing asks at most 256 applications — with `pids`, the first 256 named — and reads at most 64 windows of each. An application in the five-second not-answering quarantine (see [`active()`](#host-window-active)) is not asked at all, and the log says so.

## host.window.apps() {#host-window-apps}

**Signature:** `host.window.apps() -> { { pid: number, name: string, exe: string, bundleId: string } }`

The running applications, described the way a window table's `app` field describes them — so a matcher's `app` clause tests against either. Asks none of them anything: this is what lets `find` decide **which** applications to list windows from before a single cross-process question is put.

```luau
-- Which pid is Kontakt's, without listing a window.
for _, a in ipairs(host.window.apps()) do
  if a.bundleId == "com.native-instruments.Kontakt 7" then return a.pid end
end
```

### Returns {#host-window-apps-returns}

- `{ { pid, name, exe, bundleId } }` — one entry per running application the platform lists (below), `name` the stem of `exe` as a window table's `app.name` is. Empty when there is none. Never `nil`.

### Errors {#host-window-apps-errors}

None. It never answers an error value. It never raises: arguments are ignored.

### Windows {#host-window-apps-windows}

Derived from the visible top-level windows: their owning processes, one entry per pid. A process with no window is not a place a window search could find anything, so it is not listed.

### macOS {#host-window-apps-macos}

The workspace's own list of running applications, its first 512, minus those whose activation policy is *prohibited* (helpers, agents, XPC services — more of them than everything else together, and none can own a window). Hidden applications (Command-H) are included, as they are in `list()`: a module that finds a hidden DAW's plugin window and focuses it is how that DAW comes back.

## host.window.windowsOf(pid) {#host-window-windowsof}

**Signature:** `host.window.windowsOf(pid: number) -> { { id: number, layer: number, class: string, x: number, y: number, w: number, h: number } }`

Every on-screen window a process owns, as the **window manager** lists them rather than as accessibility does. The difference is the point: a popup menu drawn as a window of its own is in this list from the moment it opens to the moment it closes, whether or not the application posts a notification about it or exposes it as a menu element. The overlay runtime's menu test `O.menuTests.newWindow` takes this list when a control that opens a menu is activated and compares on every tick after — a window that is there now and was not then is the menu, and its going is the menu closing (see [`O.menuTests`](overlay.md#o-menutests)). It works for a plugin whose menu neither the native-menu notification nor the accessibility tree can see, provided the menu is a window at all. A module needs it directly only for the same kind of question.

Cheap: one system-wide list, no message to the application, so it may be asked on the tick that carries the keyboard. Nothing here needs a window's title, and none is read.

```luau
-- Before opening the menu:
local before = {}
for _, w in ipairs(host.window.windowsOf(pid)) do before[w.id] = true end
host.input.click(x, y)
-- A tick later: anything new is the popup.
for _, w in ipairs(host.window.windowsOf(pid)) do
  if not before[w.id] then host.log.info(("popup %dx%d at layer %d"):format(w.w, w.h, w.layer)) end
end
```

### Returns {#host-window-windowsof-returns}

- `{ { id, layer, class, x, y, w, h } }` — the process's on-screen windows, with their rectangles in screen coordinates. Empty when it has none on screen, or no process has that `pid`. Never `nil`.

### Errors {#host-window-windowsof-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: a `pid` that is not a number from 0 to 4294967295.

### Windows {#host-window-windowsof-windows}

`EnumWindows` filtered to visible windows owned by the process, with the Win32 class in `class`. A `#32768` popup menu is a top-level window owned by the thread that opened it, so it appears here; so does a toolkit's self-drawn popup, which is usually a tool window of its own. `layer` is always 0.

### macOS {#host-window-windowsof-macos}

`CGWindowListCopyWindowInfo` for on-screen windows, filtered by owning pid; only the first 512 windows of the system's list, front to back, are read, and a window of the process past them is left out. `id` is the `CGWindowID`, `layer` the window server's level — an `NSMenu` sits at 101, an ordinary window at 0. `class` is empty. Whether a given plugin's self-drawn menu is a window of its own or painted inside the plugin's window is exactly what this exists to find out; the runtime's `newWindow` test logs each window that appeared after a menu-opening press, with its level and size. sforzando's menus are windows here, at level 101 each. VPS Avenger's in Logic are not (the first Mac test of its page, 2026-10-14): after its zoom field was clicked no new window of Logic's process came on screen in 8 seconds, while the same list of the window server's paired Logic's plug-in window by frame and owner in that session — JUCE paints a popup that has a parent component inside the plug-in's editor. Such a popup is seen by the pixels only: [`O.menuTests.drawn`](overlay.md#o-menutests).

## host.window.active() {#host-window-active}

**Signature:** `host.window.active() -> Window?`

Returns the [window table](#window-table) for the foreground window, or `nil` if there is none or it is hidden: on Windows not visible (`IsWindowVisible`), on macOS minimised. A window table is something to match and place things against, and a window nobody can see is neither. Keys still go to such a window; [`foreground()`](#host-window-foreground) is the call that says so. Its `shown` is a different, stricter test on Windows: a minimised or cloaked foreground window is a window table here and `shown = false` there.

**The answer is the epoch's.** The first call in an [epoch](./timer.md#host-epoch) asks the operating system, and every later call in the same epoch — from any module — gets that answer back, `nil` included (the note at the top of this page lists what turns it over). So it is current as of whatever last turned the epoch over, not as of the call: in a [`host.timer.every`](./timer.md#host-timer-every) poll it can name a window that has since hidden, closed or lost the foreground without an event.

```luau
local w = host.window.active()
if w then host.log.info("front: " .. w.title) end
```

### Returns {#host-window-active-returns}

- A [window table](#window-table) — the foreground window as the epoch last saw it.
- `nil` — no window is in the foreground, or it is hidden: on Windows not visible, on macOS minimised. On macOS also when its application does not answer and no window of it is remembered (below). The `nil` is kept for the epoch too.

### Errors {#host-window-active-errors}

None. It never answers an error value. It never raises: arguments are ignored.

### macOS {#host-window-active-macos}

**This application's own window** is AppKit's answer: while this application is the frontmost one,
or a panel of it has the keyboard without activating it (see [`foreground()`](#host-window-foreground)),
the answer is its key window, else its main window, and `nil` when that is hidden or minimised.
Never from memory, and nothing is asked of this application through accessibility.

**An answer can be up to five seconds old.** Every reading here is a synchronous call into
another application, and a plugin that is mid-repaint does not answer one. When that happens
the application is left alone for five seconds, and during those five seconds this returns
the window it last reported rather than `nil`.

That is deliberate: the application is still in front, it is simply not talking, and `nil`
means "no foreground window" — which is what an overlay gates its own activation on. Without
the memory an overlay would switch itself off, and then back on, every time a plugin was busy
for a moment.

What it costs is that `bounds` and `client` may be stale for those five seconds. In practice
an application that is not answering is also not moving its window, but a module that acts on
a coordinate far from where the user last saw it has no way to tell. Every served answer is
in the log, so a session can be explained afterwards.

A minimised window reads as `nil` — minimised is this platform's hidden — and the call does not
go on to another window of the application instead.

### Windows {#host-window-active-windows}

`GetForegroundWindow`, a local question answered without asking the application anything — but
asked once per epoch and kept, as above, so "local" is not "current": the answer is refreshed
only when the epoch turns over (an OS event dispatched into a module, a one-shot timer coming due,
an image, text or snapshot result arriving, input the host drives, `host.window.focus`,
`host.window.recheck` — the full list is under [`host.epoch`](./timer.md#host-epoch)).

A foreground window that is not visible (`IsWindowVisible` false: hidden) reads as `nil`, and so
does a window with nothing in the foreground. A minimised or cloaked foreground window is still
reported: it is visible to that test. An untitled window is reported too, unlike in `list()`.

## host.window.foreground() {#host-window-foreground}

**Signature:** `host.window.foreground() -> { id: number, pid: number, shown: boolean, own: boolean }?`

The foreground window right now, whether or not anybody can see it: its `id`, its process's `pid`, and whether it is `shown`. `own` says whether it is one of this application's own windows, as a [window table](#window-table)'s `own` does. It is the window the keys the user presses go to, unless a hotkey or a keyboard hook takes them first — this host's captured keys among them — and apart from two platform cases: a Windows Store app, and on macOS a panel that takes the keys without its application coming to the front (see the platform sections). `nil` when there is no foreground window, or when the platform cannot say which it is.

It answers the two things [`active()`](#host-window-active) cannot. `active()` drops a window that
is not shown, although keys still go to it — a program that hides a popup instead of closing it can
leave the keyboard there, and every key then vanishes into a window nobody sees. And `active()` is
kept for the epoch, while this is **read afresh on every call**, so it also sees a change that
came with no event: a window hidden since the epoch began.

Only identity and visibility — no title, class or geometry. `id` is in the id space of a
[window table](#window-table)'s `id`, so it compares with `active().id` and is what
[`focus`](#host-window-focus) takes; for the rest, look the window up with
`list({ pids = { f.pid } })`, which lists shown, titled windows only.

- **Cost.** Windows: five local calls, no message to any application — 7 to 8 µs a call,
  measured in a debug build with the table it returns. macOS: two accessibility reads of the
  frontmost application, the kind `active()` makes (see below). On the main thread,
  like every host call, and it returns when those have.
- **Turns nothing over.** It is a reading: the epoch and every answer kept in it stay as they are.
  On macOS a read that times out does leave one mark behind: the application is put in the
  not-answering quarantine, which other calls consult (see the macOS section).

```luau
-- Warn when keys go into a window that is not on screen.
local f = host.window.foreground()
if f and not f.shown then
  host.speech.output("The keyboard is in a window that is not on screen")
end

-- The window keys go to is not the one active() reported earlier in this epoch.
local a = host.window.active()
if f and a and f.id ~= a.id then
  host.log.info(("keys go to window %d of process %d, not to '%s'"):format(f.id, f.pid, a.title))
end
```

### Returns {#host-window-foreground-returns}

- `{ id, pid, shown, own }` — the foreground window now: its id, its process, whether it is shown, and whether it is this application's own.
- `nil` — no window is in the foreground, or the platform cannot say which. On Windows that is a moment between two windows, or a window destroyed during the call. On macOS it is a frontmost application that names no focused window, this application in front with none of its windows holding the keyboard, a read that timed out, or an application in the not-answering quarantine (below). `nil` is the only "no answer".

### Errors {#host-window-foreground-errors}

None. It never answers an error value. It never raises: it takes no arguments, and arguments passed are ignored.

### Windows {#host-window-foreground-windows}

`GetForegroundWindow` — the window whose thread gets keyboard input — then its process
(`GetWindowThreadProcessId`) and three local tests. `shown` is `true` only when the window is
visible (`IsWindowVisible`), not minimised (`IsIconic`) and not cloaked (DWM's `DWMWA_CLOAKED`:
a window on another virtual desktop, a suspended Store app's, the Start menu's and search's hosts
while they are closed — visible to the first test, drawn by nobody). A cloaked question that DWM
refuses counts as not cloaked. `nil` when Windows reports no foreground window, which it does for
a moment while one window loses activation to another, and when the window was destroyed between
the calls.

Nothing is sent to the window, so a hung application cannot hold the call up, and a window of an
elevated application is read like any other: none of the five calls is refused across integrity
levels. Where `active()` answers `nil` for a hidden foreground window, this answers it with
`shown = false`; a minimised or cloaked one is a window to both, and `shown = false` here.

A Windows Store (UWP) app's foreground window is its frame, an `ApplicationFrameWindow` of
`ApplicationFrameHost.exe`, so `pid` is that process's — as `active()`'s `app.pid` is — while the
keys go to the app's own window inside the frame, in the app's process.

### macOS {#host-window-foreground-macos}

The frontmost application — the workspace's own answer, local — and its `AXFocusedWindow`. Only
that attribute: `active()` falls back to the application's main window and then its first window
when none is focused, and this does not, so an application that answers with no focused window
gives `nil` here where `active()` may still name one. For the same window the `id` is the handle
`active()` reports. `shown` is `false` only when the window reports itself minimised
(`AXMinimized`); the window server is not asked whether the window is drawn.

The two accessibility reads are made at the call; the frontmost application is the workspace's
answer as of the main thread's last run-loop turn, so an application that came to the front since
is named once that turn has been taken. A panel that takes the keys without activating its
application — Spotlight, a launcher, a password manager's quick-access window — is not named: the
frontmost application, and so this answer, stays the one before it.

**This application's own window** is the exception, and is AppKit's answer: while one of its windows
has the keyboard, that window — its `id` as a [window table](#window-table) gives it, `own = true`,
`shown` unless it is hidden or minimised. That holds while the workspace names this application the
frontmost one, and while it names another only for a panel of this application's that takes the
keyboard without activating it: any other window of this application's with the keyboard then is
not the answer, and the application the workspace names is asked, as it is when that application's
activation is reported. The workspace and AppKit tell the application of one switch separately, and
which comes first is not known; each is a focus change, so the answer is asked again once both have
come. With this application the frontmost one and none of its windows holding the keyboard, `nil`:
no window gets the keys then. Which of its windows has the keyboard is followed from AppKit's
notifications and read again at every turn of the event loop (see "Keys in this application's own
windows" under [`host.keys.capture`](./keys.md#host-keys-capture)); nothing is asked of this
application through accessibility.

**Never from memory.** While the frontmost application is in the five-second not-answering
quarantine described under [`active()`](#host-window-active), it is not asked, and the answer is
`nil` — not the window `active()` serves from memory then. A read that times out puts the
application in that quarantine and answers `nil` as well, and the quarantine then holds for every
call that consults it, for five seconds: `active()` serves its remembered window, and other calls
put no accessibility question to that application. Each read is a call into the application on the
main thread, bounded by the accessibility timeout of one second, so a call against an application
that has just stopped answering can take that long once; the quarantine makes the calls after it
free until it ends.

## host.window.focus(id) {#host-window-focus}

**Signature:** `host.window.focus(id: number) -> true | (false, Error)`

Brings the window with that handle to the front and gives it the keyboard. `id` may also be a
**control's**, on Windows — one of [`controls()`](#host-window-controls) or
[`focusChain()`](#host-window-focuschain), or an embedded overlay's
[`O:hwnd()`](./overlay.md#o-hwnd): its window is brought to the front, and the keyboard is then
landed on the control (Windows, below). Returns `true` when the system accepted it, and `false, err` when it did not, `err.code` saying why ([Errors](#host-window-focus-errors)). Like `host.input.*`, it acts at once, also while the calling module has a
[`host.ocr.recognize`](./ocr.md#host-ocr-recognize) or a snapshot request out: a picture asked for
just before and not taken yet may show the window in front. Read first and focus once the read is
answered, and the log notes a module that does not, once a session (see
[Acting does not wait for a read](./input.md)). The picture's `inputEpoch` does not tell a focus:
[`host.inputEpoch()`](./timer.md#host-inputepoch) turns over only when the window has come
forward and the event loop has heard of it, a turn of the loop later at the earliest, so a picture
taken right after the call can carry the value from before it. To know whether a picture came after
a focus, note `host.now()` after the call and compare the picture's `time` with it, or focus in the
read's callback.

**Believe the answer.** Both platforms can decline — Windows refuses a foreground change
under conditions it does not explain, and on macOS the window's own application has to be
activated as well as the window raised, either of which can fail. A module that announces
"back in the plugin" when the focus did not move has told somebody who cannot check that
they are somewhere they are not, which is the one failure this project treats as worse than
doing nothing.

```luau
local w = host.window.find({ title = { contains = "sforzando" } })
if not (w and host.window.focus(w.id)) then
    host.speech.output("could not get back to sforzando")
    return
end
-- Accepted is not the same as arrived: read where the keyboard is before saying so. The
-- identity test is the portable half — an empty chain, or one ending in another window,
-- means this read says nothing about our window on either platform.
local chain = host.window.focusChain()
if #chain == 0 or chain[#chain].id ~= w.id then
    host.speech.output("could not tell whether the keyboard is in sforzando")
else
    host.speech.output("sforzando")
end
```

A call also turns the observation cache over, so a `focusChain()` read straight after it asks
the operating system again rather than being handed the answer kept from before the call. That
answer is the world as of the read, which on Windows can still be from before the change (see
below).

**Why it exists.** On Windows a screen-reader user gets back to a plugin's window with
OSARA's F6. macOS has no equivalent: VoiceOver offers no command for it, so a plugin window
opened inside a DAW can be genuinely hard to return to. Building that command needs this
call, and until now the platform had nothing that could focus a window at all.

On Windows a minimised window is restored first — a window that is made foreground while
still minimised stays invisible, which is the worst of both answers for somebody who cannot
see it happen.

```luau
-- Back into an embedded plug-in from a page: its own control, so that the keyboard lands in it
-- (Windows), not only its window to the front. On a Mac the same call takes the window.
if not host.window.focus(ov:hwnd()) then
  host.speech.output("The plug-in's window would not come to the front.")
end
```

### Returns {#host-window-focus-returns}

- `true` — the system accepted the request. Not that the keyboard is in the window, and often not even that the change has been made yet: read [`focusChain()`](#host-window-focuschain), or wait for the foreground event (see the platform sections).
- `false, err` — it did not, and nothing more is done; `err.code` is one of the codes under Errors.

The epoch turns over either way. To tell the user why, compare the code:

```luau
local w = host.window.find({ title = { contains = "sforzando" } })
if not w then return end
local ok, err = host.window.focus(w.id)
if not ok and err.code == host.errors.GONE then
  host.speech.output("sforzando's window is closed")
elseif not ok then
  host.speech.output("could not get back to sforzando")
end
```

### Errors {#host-window-focus-errors}

None of these is raised. Each comes as the second value, beside `false`.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `gone` | return | No window has that id: on Windows `IsWindow` says no, `0` included; on a Mac an id this backend never issued, one it has forgotten, or one whose application has quit. Nothing is asked. | `no such window` | both |
| `hidden` | return | A hidden window of this application: the module manager once it is closed, a page hidden, or any window of the application while the application is hidden. Nothing is shown; a line in the log says so. | `the window is hidden` | macOS |
| `refused` | return | The system declined. On Windows `SetForegroundWindow` refused. On a Mac the activation request was not taken, the window of an active application took neither the raise nor the focus flag, or a page described `activates = false` is not the key window as the call returns. | `declined by the system` | both |
| `not_supported` | return | The build for other systems, which cannot focus a window. | `not supported on this platform yet` | other |

**Raises** for a mistake in the call: an `id` that is not a number (`bad argument #1: error converting Lua nil to isize (expected number or string coercible to number)`). That is the only mistake it raises for.

### Windows {#host-window-focus-windows}

A minimised window is restored first, then brought to the foreground with `SetForegroundWindow`. `true` means Windows accepted the request — not that the change has been made. For a window of another application the activation can finish only once that window's own thread has processed it, so [`active()`](#host-window-active), [`foreground()`](#host-window-foreground) and `focusChain()` asked straight after the call can still name the window that was in front; the foreground event follows once the change is made, and a module that wants to know where the keyboard went waits for that event rather than reading again at once. `false` with `refused` means Windows refused: it limits which processes may change the foreground (the conditions are listed under `SetForegroundWindow` in Microsoft's documentation), and it does not say which one applied, and nothing more is done. An id that names no window (`IsWindow`) is `gone`, and nothing is asked. Read [`focusChain()`](#host-window-focuschain) afterwards for where the keyboard is inside the window.

**A control's id — the keyboard lands on it.** Which control of a window gets the keyboard is the application's to decide, and REAPER, brought forward, has been seen to leave it on its FX window itself, on its FX list, or nowhere (the HTML probe's W7 and W10) rather than in the plug-in. So for an `id` that is a control, the control's top-level window is brought forward, and an accepted request starts a **landing** on the control, which decides by events and by no timer:

- **It looks** on the call's own turn when the window was in front already, at the window's foreground event, and at every focus event after that. A look reads the window's thread with `GetGUIThreadInfo` — local, no message sent to the application.
- **Its activation not processed yet** (the thread's active window is not the window): it waits for the next event.
- **The keyboard on the control, or inside it:** landed. Said once, and still looked at, since an application can move the keyboard on once more after its activation.
- **Elsewhere in the window, or nowhere:** the keyboard is **put** on the control — UI Automation's `SetFocus`, asked on the host's accessibility thread as [`host.element`](./element.md)'s questions are, never on the event loop, and read back there. At most **two** puts: one for where the activation left it, one for an application that moves it on once (W7). Past that the application keeps it where it wants it, and the log says so. A control that is no longer a visible window inside its window gets none.
- **It ends** at the user's first key-down after the call (a key held down at the call repeating is not one), at any `host.input` call, at another window's foreground event, at the control destroyed or hidden, and at a newer `host.window.focus`, which replaces it. So it never moves the keyboard after the user — or a module — acted: a Shift+Tab to the FX list is the user's.

The key-downs are the ones raw input reports, which the host watches whenever a module holds a key. A plug-in whose process does not answer holds only the accessibility thread's put, up to its one-second timeout, never the event loop. Our own code never attaches input to another thread; for a window with no accessibility provider of its own — a JUCE plug-in without accessibility — UI Automation's window proxy may, on the accessibility thread, for the moment it takes. Whether a put takes there is unmeasured, and is in the log:

`[window] focus of control <id> ('<class>') in '<title>': ` then `waiting for the window's activation`, `the keyboard landed on it`, `the keyboard was on '<class>'` (or `nothing (nowhere)`) `; put on the control: took` or `refused (<why>)`, `the application moved it to '<class>' after 2 puts — left there`, and `ended: a key was pressed`, `ended: a host.input call`, `ended: another window came to the front`, `ended: the control is gone or hidden`, `ended: a newer focus`.

**A top-level window's id** is brought forward as it always was, with no landing — with one exception. A page of this application in front gives the keyboard back, with the front, to the control it took the front from ([`page:hide`](./gui.md#page-hide)); but a module that brings the plug-in's window forward **first**, as [`page:hide`](./gui.md#page-hide) asks, leaves the page nothing to hand back once it is no longer in front. So `host.window.focus` of the window a page in front came from lands on the control that page took the front from.

```luau
-- The way back from a page, in the order gui.md asks for: the plug-in's window first, then the page
-- hidden. Its keyboard lands on the control it was on when the page came.
if host.window.focus(plugin.id) then
  page:hide()
end
```

### macOS {#host-window-focus-macos}

Three things are done: the window's application is made the active one, the window is raised inside it, and its accessibility focus flag is set. When that application is the active one already — and this application is not — the window is raised and its focus flag set at once, and `true` means that the window accepted either. That is also the case while a page described [`activates = false`](./gui.md#host-gui-webview) has the keyboard: the application it took the keyboard from is still the active one, though AppKit says this one is. Whether raising and focusing a window of that application then takes the keyboard from the page is unmeasured (TODO.md, MW1b). Otherwise the application is asked first, with `NSRunningApplication`'s `activateWithOptions` for all of its windows, and this application is left as it is: its module manager's window, say, keeps the keyboard until that application takes it. The window is raised and focused only when the workspace announces that application as the active one — after the call has returned, in the turn of the event loop that announces the activation, before the window in front is read for the foreground event, so that event names this window where the application lets it be raised. Here `true` means that the system took the request: activation is asynchronous, and the system can still decline it without a word. Only events end the wait, never a length of time. If another application becomes the active one first, the window is neither raised nor focused, and the log says `the front went to … instead`. If a key was pressed after the call, the window is not raised or focused when the activation comes, since the user has moved on and that key went somewhere else (`… but a key was pressed since`); the keys are seen by the event tap, so this needs the permission the tap needs. A second call, this application taking the front itself, and this application becoming the active one again after it had stopped being it give up a focus still waiting (`… is given up`). So a request the system declined leaves the window to be raised and focused the next time its application becomes the active one, if none of these came first. [`active()`](#host-window-active), [`foreground()`](#host-window-foreground) and `focusChain()` asked straight after a call that had to ask can still name the window that was in front before, as on Windows: a module that wants to know where the keyboard went waits for the foreground event.

`true` never means "the keyboard is in it". Measured in the first tester session, five presses out of five in REAPER: the raise, the activation and the focus flag were all accepted and the keyboard stayed on REAPER's own FX list, because that plug-in's view exposes no accessibility element there is to hand it to. An overlay gating on the focus being inside the plug-in can still decline to act after this reported success. Read [`focusChain()`](#host-window-focuschain) and take it three ways: **empty** means the backend could not read where the keyboard is; a chain whose last link is **not this window** belongs to another application, since activation is asynchronous; and a chain ending in this window tells you how deep the focus sits inside it. The `daw-hosts` shortcut does that, clicks just inside the plug-in's panel when the focus is deeper than the window itself, and reads the chain again a quarter of a second later, because a posted click has not been processed when the call returns. Whether that click moves REAPER's keyboard is not yet measured.

Minimised windows are additionally dropped from `list()` and `find()` here, so one cannot normally be reached to pass in.

**Pass a window's id.** A plug-in in a DAW publishes no control of its own here, and an embedded overlay's origin is the DAW's panel, whose id is its window's ([`attachEmbedded`](./overlay.md#o-attachembedded-macos)), so `host.window.focus(ov:hwnd())` is the same call on both platforms. Nothing here lands the keyboard on a control: where VoiceOver's keyboard goes inside the window is the application's (TODO.md, MW1b).

A window of this application's own is focused with AppKit's calls instead: brought back if minimised, the application asked to become the active one (`NSApplication`'s `activateIgnoringOtherApps:`), and the window ordered in front and made key. `true` whenever the window is shown or minimised; the change is asynchronous, and the window becoming key is what says it happened. The window of a page described [`activates = false`](./gui.md#host-gui-webview) takes the keyboard without that request, so nothing is on its way and the answer is decided at the call: `true` when the window is the key window as the call returns, `false, err` with `refused` when it is not. A hidden one — the module manager once it is closed, a page hidden, or any window of the application while the application is hidden — answers `false, err` with `hidden`, and a line in the log, as the window tables leave it out: only what hid it shows it again. An id this backend never issued, or one of an application that has quit, is `gone`; any other request the system did not take, `refused`.

## host.window.controls(win?) {#host-window-controls}

**Signature:** `host.window.controls(win: Window?) -> { Control }`

Returns the child [control tables](#control-table) of `win` (its `id` is used), or of the active window when omitted. Returns an empty table if no window applies. Used to detect embedded plugins by control class/geometry.

```luau
for _, c in ipairs(host.window.controls()) do
  if string.match(c.class, "^Qt") then
    host.log.info("plugin control id=" .. c.id)
  end
end
```

### Returns {#host-window-controls-returns}

- `{ Control }` — the [control tables](#control-table) inside the window, which ones differing by platform (below). The list for a window id is kept for the epoch. With `win` left out, the window in front is asked afresh at every call — not the epoch's [`active()`](#host-window-active) answer, so the two can name different windows.
- `{}` — no window applies, or it holds nothing listed: `win` left out with no window in front, an id that names no window, a window with no visible child (Windows) or no container (macOS), and on macOS a window of this application's own. Never `nil`.

### Errors {#host-window-controls-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: a `win` that is neither a table nor `nil`, and a table whose `id` is not a number, a table without `id` included (`error converting Lua nil to isize (expected number or string coercible to number)`). Its other fields are not read.

### Windows {#host-window-controls-windows}

Every **visible child window**, from `EnumChildWindows`, with no node or depth cap. A Win32 dialog therefore yields its buttons, edit fields and labels as well as its containers.

### macOS {#host-window-controls-macos}

**Container roles only** — `AXGroup`, `AXScrollArea`, `AXSplitGroup`, `AXTabGroup`, `AXToolbar`, `AXWindow` and their like. Buttons, labels and text fields are deliberately excluded, and the walk stops at 600 nodes, depth 8, 256 results or 300 ms.

So a module that identifies a plug-in by scanning `controls()` for a *leaf* control class finds it on Windows and comes back empty-handed here. Identify by container, or by `host.element.*`.

Empty for a window of this application's own: nothing inside it is read.

## host.window.focusChain() {#host-window-focuschain}

**Signature:** `host.window.focusChain() -> { Control }`

Returns [control tables](#control-table) from the currently focused element up to its top-level window. Used to detect that focus has entered an embedded plugin.

```luau
local chain = host.window.focusChain()
local focused = chain[1]   -- innermost focused control, if any
```

### Returns {#host-window-focuschain-returns}

- `{ Control }` — the chain, innermost first: `chain[1]` is what has the focus, `chain[#chain]` its top-level window. Kept for the epoch.
- `{}` — the focus could not be read: on Windows when there is no foreground window; on macOS in every state listed below. Never `nil`, and never "the keyboard is in the window".

### Errors {#host-window-focuschain-errors}

None. It never answers an error value. It never raises: arguments are ignored.

### Windows {#host-window-focuschain-windows}

The chain's links are **windows**: it walks from the focused `HWND` up through its parents. A plug-in that draws its own interface is one link, because it is one window.

### macOS {#host-window-focuschain-macos}

The links are **accessibility elements**. Both platforms start at whatever has focus, but the units differ, so the same self-drawn plug-in can be many links deep here and one link there. A gate written as "the chain is at most one deep, therefore we are in the plug-in" is reading a granularity, not a fact about the plug-in — check what the chain actually contains rather than how long it is.

**An empty chain means the focus could not be read**, and is never "the keyboard is in the window". Several states produce it: the application did not answer within the accessibility timeout, it is in the short quarantine that follows such a timeout, there is no frontmost application, the keyboard is in this application — one of its windows has it, or it is the frontmost application, and the read would be answered by this application itself, so it is not made — or the Accessibility permission is not granted at all — in which case every chain is empty for the whole session — or the application answered the read of its focused element with an error, or with none and no window in front. Logic Pro answers so while a plug-in's own drawn view has the keyboard, right after every click into it (the first Mac test of VPS Avenger's page, 2026-10-14; which error it answers was not logged then). The first time an application answers either way, one line says which, once per application and kind for the session: `[macos] accessibility answered <error> reading AXFocusedUIElement in <application> — where the keyboard is cannot be read there (the focus chain is empty)`, or `… answered no focused element and no front window in <application> — …`; the busy and the permission errors have lines of their own. The overlay runtime keeps a plug-in's stay through an empty chain in the same window, and begins none on one ([`O:attachEmbedded`](overlay.md#o-attachembedded-macos)). When the read succeeds and nothing inside the window reports focus (a plug-in view that is not accessible is exactly that), the window alone is returned, and that one-link chain is the honest answer. Until 2026-09-03 a failed read took the same fallback and came out identical to it.

Depth is still not a fact about the plug-in. A one-link chain says the focus is the window; whether that means "the user is in the plug-in" depends on whether the plug-in exposes anything, which differs per plug-in and per platform.

## host.window.ownsPoint(id, x, y, opts?) {#host-window-ownspoint}

**Signature:** `host.window.ownsPoint(id: number, x: number, y: number, opts: { listed: boolean? }?) -> boolean?`

Whether the window `id` belongs to is the one drawn at that screen point.

`opts.listed = true`: `id` is a window as [`host.window.windowsOf`](#host-window-windowsof) lists it, not a `host.window` id — a popup a plug-in draws as a window of its own is known only from that list, and an item in it is clicked only if nothing else is drawn over it (the overlay runtime's [`O:chooseMenuItem`](overlay.md#o-choosemenuitem) asks this way). A listed window is a top-level window already, and the same three answers come back. A negative `id` with `listed` answers `nil`.

Every hotspot in this project clicks a coordinate worked out from a window's own frame, and knowing the point falls *inside* that frame says nothing about what is drawn there. A notification, a tooltip, another application raised over it: the click goes to whichever window owns the pixel. This is how an overlay finds that out before clicking rather than after.

Compared at the **top level**. An overlay's origin is often a child — an embedded plug-in is a control inside its host's window — and the point resolves to whichever child is drawn there, usually a different one. So both sides are taken up to their root, and the question answered is "does this point belong to the same application window".

**`nil` means the platform cannot say, and must be read as permission rather than refusal.** A check with no answer must not block what it cannot judge. Only a definite `false` should stop anything.

`Overlay:addHotspotButton` and `addHotspotToggle` already ask this before every click; a module only needs it directly when it clicks a coordinate itself.

```luau
local o = ov:origin()
if host.window.ownsPoint(o.id, x, y) == false then
  return -- something else is covering it
end
host.input.click(x, y)

-- A popup a plug-in opened, known from the window list: is anything drawn over the item?
for _, w in ipairs(host.window.windowsOf(pid)) do
  if w.id == popupId and host.window.ownsPoint(w.id, ix, iy, { listed = true }) ~= false then
    host.input.click(ix, iy)
  end
end
```

### Returns {#host-window-ownspoint-returns}

- `true` — the window drawn at the point belongs to the same top-level window as `id`.
- `false` — another window is drawn there: another application's, or another top-level window of the same application. Only this should stop a click.
- `nil` — the platform cannot say. On Windows no window is at the point — it is off every screen — or `id` names no window; on macOS any of the cases below; and always in the build for other systems. Read it as permission.

### Errors {#host-window-ownspoint-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: an `id` that is not a number; an `x` or `y` that is not a number within the 32-bit range; an `opts` that is neither a table nor `nil`; a `listed` other than `true`, `false` or `nil` (`host.window.ownsPoint: listed is true or false, not a number`). Every other key of `opts` is ignored.

### Windows {#host-window-ownspoint-windows}

Implemented with `WindowFromPoint` + `GetAncestor(GA_ROOT)` on both sides. `WindowFromPoint` is the window manager's own hit test, so a click-through window (`WS_EX_TRANSPARENT`) is correctly seen through rather than treated as a cover. `windowsOf` lists windows by their `HWND`, so with `listed` the question is the same one about the same number.

### macOS {#host-window-ownspoint-macos}

`NSWindow.windowNumberAtPoint:belowWindowWithWindowNumber:` — the window server's own hit-test, which names the frontmost window that would receive a mouse-down at the point across every application and skips windows that let clicks through. That is the question a click asks, and the reason the cheap route was not taken: `CGWindowListCopyWindowInfo` answers what is *drawn* there, and a screen reader's cursor ring is drawn over the very control being operated while letting clicks pass, so the drawn-there answer would have refused essentially every press on the machines this exists for.

The window is paired with its `CGWindowID` by owner and frame (`window_id`), once per window. Three answers: the pair matches, `true`; the point belongs to another window — another application's, or another of the same application's, such as a popup of a DAW over its own plug-in window — `false`, with a log line naming that window's application and its window level, so a refused press can be explained; and `nil` wherever the question could not be put — no pairing yet, no window at the point, a window of VoiceOver's own that it did not mark click-through, or any window of this application's but a [page](./gui.md#host-gui-webview) (the announcement window sits over the plugin). A page's window at the point answers `false`, as another application's does: it would take the click. Which window of this application is at the point is AppKit's answer, with no window list read. With `listed`, `id` is the `CGWindowID` itself, which is the numbering `windowsOf` lists in and the hit test answers in: no pairing is needed, and the other answers are the same. Main thread only, which the pump is. Unverified on hardware as of 2026-09-10 without `listed`, and not run at all with it: the probe records which answer it got.

## host.window.find(matcher) {#host-window-find}

**Signature:** `host.window.find(matcher: Matcher) -> Window?`

(prelude) Returns the first window that satisfies `matcher`, or `nil`.

Only the applications the matcher names are asked for their windows: the `app` clauses (top level and this platform's block) are run over [`host.window.apps()`](#host-window-apps) first, and [`host.window.list()`](#host-window-list) is then called with those pids. A matcher with no `app` clause lists every application that could own a window, at the short per-application bound described under `list()`; one whose application is not running returns `nil` without asking any process anything.

```luau
local reaper = host.window.find({ app = { name = "reaper" } })
```

### Returns {#host-window-find-returns}

- A [window table](#window-table) — the first window in the listing that the matcher matches.
- `nil` — none matches, or no running application matches the matcher's `app` clauses, in which case nothing was listed.

### Errors {#host-window-find-errors}

None. It never answers an error value.

**Raises** from inside the call, since the matcher is not checked (see [Matchers](#matchers)): a `matcher` that is `nil`, a number or a boolean (`attempt to index nil with 'app'`) — a string raises nothing and matches every window; an `os` that is not a table, an `app` that is a number or a boolean, and such a block for the platform in use; a field matcher whose value is a table where a string goes, or a `pattern` or `regex` that is not a valid Luau pattern; and whatever a `where` function raises. All but the first raise only when there is a window or an application to test the matcher against, so the same mistake can pass while nothing is listed.

## host.window.findAll(matcher) {#host-window-findall}

**Signature:** `host.window.findAll(matcher: Matcher) -> { Window }`

(prelude) Returns all windows that satisfy `matcher`, listed the same narrowed way as `find`.

```luau
local editors = host.window.findAll({ title = { contains = "Notepad" } })
```

### Returns {#host-window-findall-returns}

- `{ Window }` — every window in the listing that the matcher matches, in listing order. Empty when none does. Never `nil`.

### Errors {#host-window-findall-errors}

None. It never answers an error value.

**Raises** from inside the call, as [`find`](#host-window-find-errors) does.

## host.window.test(matcher, win) {#host-window-test}

**Signature:** `host.window.test(matcher: Matcher, win: Window) -> boolean`

(prelude) Returns whether the given window table satisfies `matcher` (the same logic `find`/`findAll` apply). Used by overlay attach contexts.

```luau
local w = host.window.active()
if w and host.window.test({ windows = { class = "REAPERwnd" } }, w) then ... end
```

### Returns {#host-window-test-returns}

- `true` or `false` — whether `win` satisfies `matcher`. Nothing is asked of the system: only the table is read.

### Errors {#host-window-test-errors}

None. It never answers an error value.

**Raises** from inside the call, as [`find`](#host-window-find-errors) does, and for a `win` that is not a table as soon as the matcher reads one of its fields; a matcher that reads none, such as `{}`, answers `true` for anything.

## host.window.onTrigger(matcher, opts, cb) {#host-window-ontrigger}

**Signature:** `host.window.onTrigger(matcher: Matcher?, opts: { on: string?, initial: boolean? }?, cb: (win: Window) -> ()) -> ()`, or `host.window.onTrigger(matcher: Matcher?, cb: (win: Window) -> ()) -> ()`

(prelude) Registers `cb` to fire on every foreground change for which the new active window satisfies `matcher`. The callback receives the matched [window table](#window-table). An empty matcher `{}`, or `nil`, matches every window.

**The options can be left out.** `onTrigger(matcher, cb)` is `onTrigger(matcher, nil, cb)`: it fires on every matching activation and asks for no report of the window already in front. A function in the second place is taken as the callback when there is no third argument.

**A bad registration raises from `onTrigger` itself**, at the caller's line, and nothing is registered: a `cb` that is not a function (`host.window.onTrigger: the callback is a function, not a nil`), a `matcher` that is neither a table nor `nil`, an `opts` that is neither a table nor `nil`, and an `initial` that is not `true` or `false` (below). The matcher's keys whose type decides what they mean are checked too: an `app`, `os` or platform block (`windows`, `macos`, `linux`) that is not a table, an `app` inside a platform block that is not a table, and a `where` that is not a function raise — `{ app = "game.exe" }` raises `host.window.onTrigger: matcher.app is a table, not a string`, where it would otherwise fire for every window. Nothing else about a matcher is checked: a misspelt key is ignored, and a field matcher that is not a string or a table, or a table with none of its keys, matches nothing (see [Matchers](#matchers)).

Registration is plain table work in the prelude, on the main thread, with no system call — except that `initial = true` queues the module for the report described next.

**`initial = true` also reports the window already in front.** Without it, a window that is in front when the module loads, is enabled or is reloaded is not reported — only a foreground change after that. With it, that window is reported too, **once**, on the next pass of the loop:

- **When.** After the module's load has finished — its entry file and `activate` have returned — so the callback can use what the file sets up after the registration. Again every time the module is enabled in the module manager, because to a module that was off whatever is in front is new — though at that moment the window in front is usually the manager itself, so a game behind it is reported at its next activation instead. And for a trigger registered later, from a timer say, on the pass after the registration. Never from inside `onTrigger` itself, and never while the module is disabled.
- **Once per foreground.** An activation dispatched before that pass *is* the report — after an enable as well — so the trigger is not called a second time for the same window. A report that found no match, or no window in front, is used up as well — the matching window coming forward later is an ordinary activation.
- **What it costs.** The foreground window is asked for once per pass, for every module owed a report together, and only when one of them has an `initial` trigger still waiting: enabling a module that has none asks nothing. The callbacks run on the main thread, inside the loop pass whose length the application logs past 250 ms, like every other trigger.
- **The same window an activation hands over.** The callback gets the same window table, and a foreground window an activation never hands over — one without a title — is not reported; the report is used up all the same. Before the first matching callback of the pass, [`host.inputEpoch`](./timer.md#host-inputepoch) turns over once, however many modules the pass reports to, and each module that reads through desktop duplication has its opening started before its own first matching callback — the primary monitor's, and not while duplication is open already, backing off, overdue or stopped — both as an activation does. The opening runs in the background: the callback's event-loop reads do not wait for it, while a read on the image worker does; see [Which picture a read sees](./screen.md#which-picture-a-read-sees).
- **What raises.** `initial` is `true` or `false`; anything else raises from `onTrigger`, as the other mistakes above do.

What it does **not** do:

- **Only `"activate"` is dispatched.** `opts.on` defaults to it; any other value — `"open"`, `"close"`, `"focus"`, `"titleChange"` — is accepted and never fires, with or without `initial`. Focus moves inside a window go to [`onFocus`](#host-window-onfocus).
- **There is no handle.** `onTrigger` returns nothing, and a trigger cannot be removed; it stays until the module is reloaded. While the module is disabled it simply is not called. To stop work a trigger started, stop that work — a poll it armed is [cancelled by its token](./timer.md#host-timer-cancel).
- **One failing callback stops the rest.** A module's triggers are called one after another in registration order, with no protection between them: an error in one skips the module's later triggers for that foreground change, or for that initial report. The error is logged every time and shown to the user once.

**While its module is busy** — one of its handlers [waits](../module-runtime-and-lifecycle.md#a-handler-waits) for a read — an activation, and the `initial` report, wait behind that handler and are never dropped: each runs once the module is free, in the order its events came. So `win` can be a window that is no longer in front when the callback runs — the user went on while the module waited. A callback that acts on the window, a click or a key sent, compares it with [`host.window.active()`](#host-window-active) first. An activation goes only to the triggers that were registered when it came: one that a handler before it registered meanwhile does not get it.

```luau
local REAPER = { app = { name = "reaper" } }
-- Also called for a REAPER window that is already in front when the module loads or is
-- enabled: once, on the next tick, and not at all while the module is disabled.
host.window.onTrigger(REAPER, { initial = true }, function(w)
  host.speech.output("REAPER focused")
end)

-- Without options: only a REAPER window that comes forward from now on.
host.window.onTrigger(REAPER, function(w)
  host.log.info("REAPER in front: " .. w.title)
end)
```

**A trigger belongs to the VM that registers it**, and a `code_module`'s source runs in more than one: once in its own VM and once in every dependent's (see [`host.require`](./require.md#host-require)). A trigger registered at the top level of a shared runtime is therefore registered — and reported — once per VM. Where the trigger is the dependent's business, as a game's is, the runtime exports a function that registers it and each game module's entry calls that function once, in its own VM, with its own data: the trigger, and the poll and hotkey it starts, then belong to the game module, stop while it is disabled and go when it is reloaded. That shape, with a hotkey that works for any number of games on one runtime, is the example under [a runtime shared by game modules](./require.md#a-runtime-shared-by-game-modules). What must exist once per application goes into the runtime's `activate`, which runs in its own VM only. A poll that starts from `initial = true` and stops itself when the game leaves the front is the example under [`host.timer.cancel`](./timer.md#host-timer-cancel).

### Returns {#host-window-ontrigger-returns}

- Nothing. There is no handle; the trigger stays until the module is reloaded.
- `cb(win)` — on each matching activation, and once for `initial = true`, with the [window table](#window-table). Its return value is ignored.

### Errors {#host-window-ontrigger-errors}

None. It never answers an error value. Its callback is handed none either.

**Raises** for a mistake in the call, at the caller's line, with nothing registered: a `cb` that is not a function (`host.window.onTrigger: the callback is a function, not a nil`); a `matcher` or `opts` that is neither a table nor `nil`; an `initial` that is not `true` or `false`; a matcher's `app`, `os`, platform block or a block's `app` that is not a table, and a `where` that is not a function (`host.window.onTrigger: matcher.app is a table, not a string`). An error the callback raises later is logged, and skips the module's later triggers for that change (above).

### Windows {#host-window-ontrigger-windows}

A system-wide foreground hook, registered for **every process**, whether or not it cooperates with accessibility, is what fires this. A foreground window **without a title** is never handed to a trigger: it runs a focus round (the `onFocus` callbacks) instead. The focus and name-change hooks beside it only ever run a focus round, so a window whose title becomes matchable after it came forward does not fire `onTrigger` either — re-check such a window from `onFocus`.

The `initial` report asks the same question as [`host.window.active()`](#host-window-active): the foreground window, its title and class, and the name of its process. Unlike `active()`, it treats a window without a title as no window, the rule the foreground hook follows: an untitled dialog or the taskbar in front at load is not reported, and the report is used up.

No focus round is run when watching starts. The window already in front is seen by `onFocus` only when the foreground or the focus next changes, or when a module calls [`host.window.recheck()`](#host-window-recheck).

**After a sleep, an unlock or a new connection** — the machine resumed, the session was unlocked, or it was connected to the console or a remote client again — the window in front is reported again as an activation, when it has a title, whether or not Windows raised a foreground change of its own, and a focus round follows; so an overlay that was active before the lock looks again at once. When Windows does raise one as well, the window is reported twice, as it is when the same window comes forward twice. After a display, scale or work-area change a focus round runs, and no activation. Each of these writes a `[system]` line to the log.

### macOS {#host-window-ontrigger-macos}

Only *application* activation is system-wide. Focus-within-an-application, window-created and title-changed exist solely as per-process accessibility observers, created lazily the first time that application comes to the front and abandoned after three permanent refusals.

**For an application that is already frontmost, `initial = true` is the only report there is.** A game already in front when its module loads raises no application activation, so without `initial` it stays unreported until the user switches away and back. The report asks the frontmost application for its window through accessibility, as [`host.window.active()`](#host-window-active) does, with the same five-second memory for an application that does not answer; with no answer at all it finds no window, and nothing fires. A window without a title is not reported, as the activation path does not report one.

The consequence lands exactly on the embedded-plug-in case: a plug-in window opening inside a DAW that is **already** frontmost raises no application activation, so the event depends entirely on that per-process observer. In a host that refuses accessibility, the overlay never activates even though the same module works on Windows.

When watching first starts, one focus round is run for the application already in front, so `onFocus` callbacks registered by then see it; `onTrigger` sees it only through `initial = true`.

**After the Mac wakes, the screen is unlocked or this user's session is back at the console**, the window in front is reported again as an activation, when it has a title — the question [`host.window.active()`](#host-window-active) asks — and a focus round follows, as on Windows. Before that, and after the displays wake or the display configuration changes, the host asks macOS again which application is in front. One that came to the front meanwhile without an activation being announced — while the screen was locked, say — is taken as the application in front from then on, and the log says so (`… is in front, and no activation said so — …`). After a wake, an unlock or the session coming back its window is the one reported to `onTrigger` then, once (`— taken over now; the system event reports its window`); one without a title is not reported, and the focus rounds that follow an untitled activation run instead (see [`onFocus`](#host-window-onfocus), macOS). After the displays wake or the configuration changes it is reported to `onTrigger` as an activation is (`— handled as one now`); when the application in front has not changed, a focus round runs and the window already known is not reported again. Each of these writes a `[system]` line to the log.

## host.window.onFocus(cb) {#host-window-onfocus}

**Signature:** `host.window.onFocus(cb: () -> ()) -> ()`

(prelude) Registers `cb` to fire whenever the keyboard focus moves — including within the same top-level window. Takes no arguments; the callback typically re-reads `host.window.active()` / `focusChain()`. Used to catch focus entering an embedded plugin without a foreground change. Like `onTrigger` it returns no handle, stays until the module is reloaded, and a callback that raises skips the module's later `onFocus` callbacks for that focus change. A `cb` that is not a function raises at once (`host.window.onFocus: the callback is a function, not a nil`), rather than when the focus next moves.

**While its module is busy** — one of its handlers [waits](../module-runtime-and-lifecycle.md#a-handler-waits) for a read — focus changes are folded: however many come, one waits behind the events before it, and its callbacks ask the focus as it is when they run.

```luau
host.window.onFocus(function()
  local chain = host.window.focusChain()
  -- re-evaluate which plugin (if any) now has focus
end)
```

### Returns {#host-window-onfocus-returns}

- Nothing. There is no handle; the callback stays until the module is reloaded.
- `cb()` — on each focus round, with no arguments. Its return value is ignored.

### Errors {#host-window-onfocus-errors}

None. It never answers an error value. Its callback is handed none either.

**Raises** for a mistake in the call, at once and with nothing registered: a `cb` that is not a function (`host.window.onFocus: the callback is a function, not a nil`).

### Windows {#host-window-onfocus-windows}

Fired by the system-wide focus and name-change hooks, by a foreground change to a window without a title, and after a sleep, an unlock, a new connection and a display, scale or work-area change (see [`onTrigger`](#host-window-ontrigger)). No focus round is run when watching starts.

### macOS {#host-window-onfocus-macos}

Same caveat as `onTrigger` above: focus changes *within* an application are delivered by a per-process accessibility observer rather than by a system-wide hook, so an application that will not answer accessibility produces no focus events at all.

One focus round is also run after the Mac wakes, the screen is unlocked, this user's session is back at the console, the displays wake or the display configuration changes, whether or not anything in front changed: a window may have moved, or come back at another scale, underneath the module.

When an application comes to the front and its window has no title yet — so no activation is reported — a focus round runs then and again 0.2, 0.5, 1, 2, 5.3, 6.5 and 10.5 seconds later, so a window that takes its title a moment later is still seen; the last rungs lie past the five seconds an application that did not answer accessibility is left alone. A new application activation while these are pending does not start them again. The same rounds follow a wake, an unlock or the session coming back when the application the host then finds in front is one no activation was announced for and its window has no title.

## host.window.recheck() {#host-window-recheck}

**Signature:** `host.window.recheck() -> ()`

Asks the host to run a focus-change round at the end of the current tick — after OS events, timers, image results and any [initial window report](#host-window-ontrigger) have run — exactly as if the OS had reported one: the observation epoch turns over (so cached window answers are re-read) and every enabled module's `host.window.onFocus` callbacks fire — including the one the overlay runtime registers, which re-evaluates each overlay's context match and gate. It exists for the case where a module *itself* changed what is detectable on screen and no OS event will follow: Komplete Kontrol auto-closing its library browser reveals the nested Kontakt underneath, and nothing about that is a focus change, so without this nobody notices until the next `pollMatch` tick (~500 ms) — half a second in which the ring sits on the wrong overlay for somebody who cannot see that it moved. The dispatch is **cross-VM**, which is the whole point here: the module that acted is usually not the module that has to notice.

Calls are coalesced — any number in one tick cost a single round — but the round itself is the same work a real focus change does: every overlay that is not already outranked re-runs its contexts and its gate (the runtime logs any recheck of 15 ms or more). So this is a one-shot for a change you caused and can therefore time; a match that changes on its own, with nothing of yours running, is what [`pollMatch` under `O:attach`](./overlay.md#o-attach) is for, and putting `recheck` on a recurring timer is that poll written twice. Two further things bite: only *focus* is dispatched, so a module's own `onTrigger(..., { on = "activate" })` callbacks do **not** run (overlays are unaffected — the runtime re-checks on both); and nothing guarantees the plug-in has finished redrawing by the next tick, so a reveal that takes an unknown moment is poked more than once rather than once and hopefully late enough.

```luau
-- modules/komplete-kontrol/src/main.luau — the KK overlay became active and its library
-- browser is covering the loaded instrument, so close it.
local p = browserToggle(o:hwnd())
if p then
  host.input.click(p.x, p.y)
  -- The instrument appears a moment later with NO focus event. Poke the re-check a few
  -- times over the next second so the Kontakt overlay — another module, another VM —
  -- takes over promptly instead of waiting on its ~500 ms pollMatch.
  host.timer.after(250, host.window.recheck)
  host.timer.after(600, host.window.recheck)
  host.timer.after(1000, host.window.recheck)
end
```

### Returns {#host-window-recheck-returns}

- Nothing. The round runs at the end of the current tick, once however many calls asked for it.

### Errors {#host-window-recheck-errors}

None. It never answers an error value. It never raises: arguments are ignored.

### Windows {#host-window-recheck-windows}

Answered by the 15 ms tick, in the manager window's timer and in a headless run alike.

### macOS {#host-window-recheck-macos}

Answered by the same 15 ms tick as on Windows, in the wxWidgets timer and in a headless run alike.
