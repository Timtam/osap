---
title: "host.window — windows, controls and focus"
sidebar_position: 20
toc_max_heading_level: 2
---

Finds windows and the surfaces inside them, reports where the keyboard is, and raises an event when either changes. In short: which window is in front, what surfaces sit inside it, where the keyboard actually is, and whether a point on screen still belongs to the window you think it does.

Most modules never call any of it — an overlay's binding does the finding, and the overlay runtime registers `onTrigger` and `onFocus` once on everyone's behalf — so you come here for the question a binding cannot settle by itself: Kontakt walks `controls()` for the Qt container that says which Kontakt this is and whether a Komplete Kontrol wraps it, sforzando reads the depth of `focusChain()` to tell REAPER's own chrome from the plug-in inside it, and daw-hosts pairs `find` with `focus` to build the "put me back in the plug-in window" key that macOS otherwise has no command for.

Nothing here touches the screen, so one call is cheap — but every overlay makes several on every foreground and focus change. Kontakt measured a single cell's context match at 15 to 96 ms with every underlying OS answer already served from cache, which is thousands of cheap calls rather than one expensive one, and why its six cells derive the whole scene once per epoch instead of each walking the control list.

The same call answers with different things per platform, so read the platform sections: `controls()` yields every visible child window on Windows and container elements only on macOS, and a focus chain's links are windows there and accessibility elements here — test what a chain contains rather than how deep it is. Believe what you are told rather than what you assume: `focus` can be declined by either platform, and `ownsPoint` answers `nil` on macOS, which a caller must read as permission rather than refusal.

**Some answers are kept for the rest of the epoch.** `active()`, `controls()` and `focusChain()` — like `host.element.find`, `findAny` and `pluginLocate` — ask the operating system once per [epoch](./timer.md#host-epoch) and hand every later call in the same epoch the same answer, a `nil` included. The epoch turns over on OS events (a hotkey, a captured key, a foreground or focus change, a controller event), on a [`host.timer.after`](./timer.md#host-timer-after) callback coming due, on an image result arriving, on input the platform drives, and on `host.window.focus`, so a callback normally sees the world as it was when the callback started. It does **not** turn over on a [`host.timer.every`](./timer.md#host-timer-every) tick: a poll is handed whatever was asked in the current epoch, which may be from before the last tick, unless something else turned it over. Nor does [`host.input.post`](./input.md#host-input-post) turn it over: a `focusChain()` read after a post in the same callback is the answer from before the key. `list`, `find`, `findAll`, `apps` and `windowsOf` are asked afresh on every call.

All coordinates are physical device pixels on Windows and **points** on macOS (i32 → Luau `number`) unless noted; the two agree only at 100 % scaling — see [`host.screen.size()`](screen#host-screen-size) for why that distinction costs a day when it is missed. `id` fields are native window handles on Windows (a Win32 `HWND` as a Luau integer) and interned counters on macOS; see the platform sections under [Table shapes](#table-shapes).

`list`, `active`, `controls`, `focusChain` and `ownsPoint` are native bindings; `find`, `findAll`, `test`, `onTrigger` and `onFocus` are added on top of them by the Luau prelude.

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
  app = {
    name = "reaper",       -- exe stem (filename without extension)
    exe  = "reaper.exe",   -- executable file name
    pid  = 12345,          -- process id
  },
  bounds = { x = 0, y = 0, w = 1920, h = 1040 },  -- window rect (screen px)
  client = { x = 8, y = 31, w = 1904, h = 1001 },  -- client area (screen px)
}
```

`client` is the window's client area in screen pixels: `x`/`y` its origin, which overlay regions are expressed relative to, and `w`/`h` its size. It is also what a window region reads: `{ window = w, fraction = { x1, y1, x2, y2 } }` is a rectangle in fractions of `client.w` and `client.h`, from `client.x` and `client.y` (see [the Region form](./screen.md#region-form)), so the table must come from `host.window`, or at least carry a `client` with those four whole numbers.

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

### Windows

`class` is the real Win32 class from `GetClassNameW` — `"REAPERwnd"`, `"Qt5152QWindowIcon"`, `"#32770"`. `app.exe` is the executable's file name (`"reaper.exe"`) and `app.name` its stem; `app.bundleId` is always `""`. `id` is a real `HWND`, which the operating system **reuses**, so a stale id can quietly come to mean a different window. `client` is `GetClientRect` translated to the screen.

### macOS

`class` is a synthesised triple, `AXRole/AXSubrole/AXIdentifier`, with both separators always present and missing parts left empty: `"AXWindow/AXStandardWindow/"`, `"AXGroup//NI.Kontakt.Main"`. It is never the empty string — an element with no role at all yields `"//"`. A Windows class pattern therefore cannot match here, and the overlay simply never activates with nothing to say why; `host.os.pick` is how a matcher carries both.

`app.exe` is the binary *inside* the bundle, named by the vendor's build system — Ableton is `Ableton Live 11 Suite.exe` on Windows and `Live` here. With no extension to strip, `app.name` equals `app.exe`, and if the executable URL cannot be read it falls back to the **localised** application name, which changes with the user's system language. `app.bundleId` carries the stable identity (`"com.native-instruments.Kontakt8"`) and is the field to match on.

`id` is a small interned counter, meaningful only to this platform and **never reused** — nothing outside it accepts one, and a handle whose process has exited stays permanently unmatched rather than silently matching something else. `client` is derived rather than read: a titled window's content rect is worked out from its own geometry, so on a borderless plug-in window it equals the frame.

## Matchers {#matchers}

A *matcher* is a declarative table passed to `host.window.find/findAll/test/onTrigger`, and to the overlay's bindings. It is a plain Luau table — there is no constructor for it — and these are **all the keys that are read** (all optional; all must match):

- `title` — a **field matcher** tested against the window title.
- `app` — table with any of `name` / `exe` / `bundleId` (field matchers) and `pid` (exact number). `bundleId` is empty on Windows, so asking for it there never matches; put it in the `macos` block.
- `os` — a list of platform names, `{ "windows", "macos" }`; on any other platform the matcher fails.
- `windows` / `macos` / `linux` — per-OS blocks. If any platform block is present but the current OS is not among them, the matcher fails. The current-OS block may contain `title` and `app` (as above), `class` (a field matcher against the window class), and on macOS `axRole`, `axSubrole` and `axIdentifier` (field matchers against the three parts of the macOS `class` string).
- `where` — a `function(win) -> boolean` escape-hatch predicate, evaluated last.

**Every other key is ignored without an error.** That includes a `class` at the top level, outside a platform block: `{ app = { name = "reaper" }, class = "REAPERwnd" }` matches every window REAPER has. The window class lives in the `windows` or `macos` block, where AutoHotkey's `ahk_class` would be. Keys from other designs — `controlClass`, `wmClass`, `exe` at the top level — are ignored the same way. A window table is plain data too: it has fields and no methods.

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

`host.window.list(filter: { pids: { number }? }?) -> { Window }`

Returns an array of [window tables](#window-table) for all enumerable top-level windows — or, with `{ pids = {…} }`, only the windows of those processes. `find` and `findAll` pass the pids of the applications a matcher names, so a module rarely calls this directly; when it does, and it knows the application, it should narrow the same way.

```luau
for _, w in ipairs(host.window.list()) do
  host.log.info(w.title .. " [" .. w.class .. "]")
end

-- Only REAPER's windows, without asking any other application anything.
local reaper = host.window.list({ pids = { reaperPid } })
```

### Windows

`EnumWindows` — local, microseconds, and the filter merely drops entries. There is no cost to leaving it out.

Only **visible windows with a title** are listed, so `find` and `findAll` never return an untitled window — Komplete Kontrol's untitled `#32770` save dialog, for one. `active()` is the one call that also returns an untitled window.

### macOS

Every application listed is a call into that process for its `AXWindows`, each with its own timeout, on the thread that carries the event tap. Unfiltered, the listing asks every running application that could own a window, and gives each of them **a quarter of a second** rather than the process-wide second — the third Mac session measured 2.0–2.3 s of every 3 s probe stall going to one unrelated process that never answered, and the tap was switched off each time. An application that misses that short deadline is left out of the listing and **named in the log, but not quarantined**: only a miss at the full timeout marks an application as not answering for the paths that consult that. With `pids`, the named applications are asked at the **full** timeout, because those are the ones the caller is waiting on and a slow-but-alive plugin must not read as absent. An application that takes 100 ms or more is named in the log either way, with how many windows it listed.

## host.window.apps() {#host-window-apps}

`host.window.apps() -> { { pid: number, name: string, exe: string, bundleId: string } }`

The running applications, described the way a window table's `app` field describes them — so a matcher's `app` clause tests against either. Asks none of them anything: this is what lets `find` decide **which** applications to list windows from before a single cross-process question is put.

```luau
-- Which pid is Kontakt's, without listing a window.
for _, a in ipairs(host.window.apps()) do
  if a.bundleId == "com.native-instruments.Kontakt 7" then return a.pid end
end
```

### Windows

Derived from the visible top-level windows: their owning processes, one entry per pid. A process with no window is not a place a window search could find anything, so it is not listed.

### macOS

The workspace's own list of running applications, minus those whose activation policy is *prohibited* (helpers, agents, XPC services — more of them than everything else together, and none can own a window). Hidden applications (Command-H) are included, as they are in `list()`: a module that finds a hidden DAW's plugin window and focuses it is how that DAW comes back.

## host.window.windowsOf(pid) {#host-window-windowsof}

`host.window.windowsOf(pid: number) -> { { id: number, layer: number, class: string, x: number, y: number, w: number, h: number } }`

Every on-screen window a process owns, as the **window manager** lists them rather than as accessibility does. The difference is the point: a popup menu drawn as a window of its own is in this list from the moment it opens to the moment it closes, whether or not the application posts a notification about it or exposes it as a menu element. The overlay runtime's menu watch takes this list before clicking a control that opens a menu and compares on every tick while the menu is plausible — a window that is there now and was not then is the menu, and its going is the menu closing. That is the third detector, after the native-menu notification and the accessibility walk, and the one that works for a plugin whose menu neither of those can see, provided the menu is a window at all. A module needs it directly only for the same kind of question.

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

### Windows

`EnumWindows` filtered to visible windows owned by the process, with the Win32 class in `class`. A `#32768` popup menu is a top-level window owned by the thread that opened it, so it appears here; so does a toolkit's self-drawn popup, which is usually a tool window of its own. `layer` is always 0.

### macOS

`CGWindowListCopyWindowInfo` for on-screen windows, filtered by owning pid. `id` is the `CGWindowID`, `layer` the window server's level — an `NSMenu` sits at 101, an ordinary window at 0. `class` is empty. Whether a given plugin's self-drawn menu is a window of its own or painted inside the plugin's window is exactly what this exists to find out; the runtime logs the answer the first time a hold runs with this detector armed.

## host.window.active() {#host-window-active}

`host.window.active() -> Window?`

Returns the [window table](#window-table) for the foreground window, or `nil` if there is none.

```luau
local w = host.window.active()
if w then host.log.info("front: " .. w.title) end
```

### macOS

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

### Windows

Always current: the foreground window is a local question there, answered without asking the
application anything.

## host.window.focus(id) {#host-window-focus}

**Signature:** `host.window.focus(id: number) -> boolean`

Brings the window with that handle to the front and gives it the keyboard. Returns whether
the system accepted it. Like `host.input.*`, it first waits — up to 50 ms, and only while the
calling module has a [`host.ocr.read`](./ocr.md#host-ocr-read) whose picture has not been
taken — until that picture is taken, and puts that picture before every other read's, so a read
asked for just before sees the screen as it was.

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

A call also turns the observation cache over, so a `focusChain()` read straight after it
reports the world after the change rather than before it.

**Why it exists.** On Windows a screen-reader user gets back to a plugin's window with
OSARA's F6. macOS has no equivalent: VoiceOver offers no command for it, so a plugin window
opened inside a DAW can be genuinely hard to return to. Building that command needs this
call, and until now the platform had nothing that could focus a window at all.

On Windows a minimised window is restored first — a window that is made foreground while
still minimised stays invisible, which is the worst of both answers for somebody who cannot
see it happen.

### Windows

A minimised window is restored first, then brought to the foreground. `true` means it is foreground **and has the keyboard**.

### macOS

Three separate things are attempted — raising the window, activating its application, and setting the accessibility focus flag — and the return is `true` if **either of the first two** succeeded; the focus flag's own answer is not part of it. Measured in the first tester session, five presses out of five in REAPER: all three were accepted and the keyboard stayed on REAPER's own FX list, because that plug-in's view exposes no accessibility element there is to hand it to. So `true` here means "raised and in front", never "the keyboard is in it", and an overlay gating on the focus being inside the plug-in can still decline to act after this reported success. Read [`focusChain()`](#host-window-focuschain) and take it three ways: **empty** means the backend could not read where the keyboard is; a chain whose last link is **not this window** belongs to another application, since activation is asynchronous; and a chain ending in this window tells you how deep the focus sits inside it. The `daw-hosts` shortcut does that, clicks just inside the plug-in's panel when the focus is deeper than the window itself, and reads the chain again a quarter of a second later, because a posted click has not been processed when the call returns. Whether that click moves REAPER's keyboard is not yet measured.

Minimised windows are additionally dropped from `list()` and `find()` here, so one cannot normally be reached to pass in.

## host.window.controls(win?) {#host-window-controls}

`host.window.controls(win: Window?) -> { Control }`

Returns the child [control tables](#control-table) of `win` (its `id` is used), or of the active window when omitted. Returns an empty table if no window applies. Used to detect embedded plugins by control class/geometry.

```luau
for _, c in ipairs(host.window.controls()) do
  if string.match(c.class, "^Qt") then
    host.log.info("plugin control id=" .. c.id)
  end
end
```

### Windows

Every **visible child window**, from `EnumChildWindows`, with no node or depth cap. A Win32 dialog therefore yields its buttons, edit fields and labels as well as its containers.

### macOS

**Container roles only** — `AXGroup`, `AXScrollArea`, `AXSplitGroup`, `AXTabGroup`, `AXToolbar`, `AXWindow` and their like. Buttons, labels and text fields are deliberately excluded, and the walk stops at 600 nodes, depth 8, or 256 results.

So a module that identifies a plug-in by scanning `controls()` for a *leaf* control class finds it on Windows and comes back empty-handed here. Identify by container, or by `host.element.*`.

## host.window.focusChain() {#host-window-focuschain}

`host.window.focusChain() -> { Control }`

Returns [control tables](#control-table) from the currently focused element up to its top-level window. Used to detect that focus has entered an embedded plugin.

```luau
local chain = host.window.focusChain()
local focused = chain[1]   -- innermost focused control, if any
```

### Windows

The chain's links are **windows**: it walks from the focused `HWND` up through its parents. A plug-in that draws its own interface is one link, because it is one window.

### macOS

The links are **accessibility elements**. Both platforms start at whatever has focus, but the units differ, so the same self-drawn plug-in can be many links deep here and one link there. A gate written as "the chain is at most one deep, therefore we are in the plug-in" is reading a granularity, not a fact about the plug-in — check what the chain actually contains rather than how long it is.

**An empty chain means the focus could not be read**, and is never "the keyboard is in the window". Several states produce it: the application did not answer within the accessibility timeout, it is in the short quarantine that follows such a timeout, there is no frontmost application, or the Accessibility permission is not granted at all — in which case every chain is empty for the whole session. When the read succeeds and nothing inside the window reports focus (a plug-in view that is not accessible is exactly that), the window alone is returned, and that one-link chain is the honest answer. Until 2026-09-03 a failed read took the same fallback and came out identical to it.

Depth is still not a fact about the plug-in. A one-link chain says the focus is the window; whether that means "the user is in the plug-in" depends on whether the plug-in exposes anything, which differs per plug-in and per platform.

## host.window.ownsPoint(id, x, y) {#host-window-ownspoint}

`host.window.ownsPoint(id: number, x: number, y: number) -> boolean?`

Whether the window `id` belongs to is the one drawn at that screen point.

Every hotspot in this project clicks a coordinate worked out from a window's own frame, and knowing the point falls *inside* that frame says nothing about what is drawn there. A notification, a tooltip, another application raised over it: the click goes to whichever window owns the pixel. This is how an overlay finds that out before clicking rather than after.

Compared at the **top level**. An overlay's origin is often a child — an embedded plug-in is a control inside its host's window — and the point resolves to whichever child is drawn there, usually a different one. So both sides are taken up to their root, and the question answered is "does this point belong to the same application window".

**`nil` means the platform cannot say, and must be read as permission rather than refusal.** A check with no answer must not block what it cannot judge. Only a definite `false` should stop anything.

### Windows

Implemented with `WindowFromPoint` + `GetAncestor(GA_ROOT)` on both sides. `WindowFromPoint` is the window manager's own hit test, so a click-through window (`WS_EX_TRANSPARENT`) is correctly seen through rather than treated as a cover.

### macOS

`NSWindow.windowNumberAtPoint:belowWindowWithWindowNumber:` — the window server's own hit-test, which names the frontmost window that would receive a mouse-down at the point across every application and skips windows that let clicks through. That is the question a click asks, and the reason the cheap route was not taken: `CGWindowListCopyWindowInfo` answers what is *drawn* there, and a screen reader's cursor ring is drawn over the very control being operated while letting clicks pass, so the drawn-there answer would have refused essentially every press on the machines this exists for.

The window is paired with its `CGWindowID` by owner and frame (`window_id`), once per window. Three answers: the pair matches, `true`; the point belongs to another application's window, `false` — with a log line naming that application and its window level, so a refused press can be explained; and `nil` wherever the question could not be put — no pairing yet, no window at the point, a window of VoiceOver's own that it did not mark click-through, or one of this application's (the announcement window sits over the plugin). Main thread only, which the pump is. Unverified on hardware as of 2026-09-10: the probe records which answer it got.

`Overlay:addHotspotButton` and `addHotspotToggle` already ask this before every click; a module only needs it directly when it clicks a coordinate itself.

```luau
local o = ov:origin()
if host.window.ownsPoint(o.id, x, y) == false then
  return -- something else is covering it
end
host.input.click(x, y)
```

## host.window.find(matcher) {#host-window-find}

`host.window.find(matcher: Matcher) -> Window?`

(prelude) Returns the first window that satisfies `matcher`, or `nil`.

Only the applications the matcher names are asked for their windows: the `app` clauses (top level and this platform's block) are run over [`host.window.apps()`](#host-window-apps) first, and [`host.window.list()`](#host-window-list) is then called with those pids. A matcher with no `app` clause lists every application that could own a window, at the short per-application bound described under `list()`; one whose application is not running returns `nil` without asking any process anything.

```luau
local reaper = host.window.find({ app = { name = "reaper" } })
```

## host.window.findAll(matcher) {#host-window-findall}

`host.window.findAll(matcher: Matcher) -> { Window }`

(prelude) Returns all windows that satisfy `matcher`, listed the same narrowed way as `find`.

```luau
local editors = host.window.findAll({ title = { contains = "Notepad" } })
```

## host.window.test(matcher, win) {#host-window-test}

`host.window.test(matcher: Matcher, win: Window) -> boolean`

(prelude) Returns whether the given window table satisfies `matcher` (the same logic `find`/`findAll` apply). Used by overlay attach contexts.

```luau
local w = host.window.active()
if w and host.window.test({ windows = { class = "REAPERwnd" } }, w) then ... end
```

## host.window.onTrigger(matcher, opts, cb) {#host-window-ontrigger}

`host.window.onTrigger(matcher: Matcher, opts: { on: string?, initial: boolean? }?, cb: (win: Window) -> ()) -> ()`

(prelude) Registers `cb` to fire on every foreground change for which the new active window satisfies `matcher`. The callback receives the matched [window table](#window-table). An empty matcher `{}` matches every window.

**`initial = true` also reports the window already in front.** Without it, a window that is in front when the module loads, is enabled or is reloaded is not reported — only a foreground change after that. With it, that window is reported too, **once**, on the next pass of the loop:

- **When.** After the module's load has finished — its entry file and `activate` have returned — so the callback can use what the file sets up after the registration. Again every time the module is enabled in the module manager, because to a module that was off whatever is in front is new — though at that moment the window in front is usually the manager itself, so a game behind it is reported at its next activation instead. And for a trigger registered later, from a timer say, on the pass after the registration. Never from inside `onTrigger` itself, and never while the module is disabled.
- **Once per foreground.** An activation dispatched before that pass *is* the report — after an enable as well — so the trigger is not called a second time for the same window. A report that found no match, or no window in front, is used up as well — the matching window coming forward later is an ordinary activation.
- **What it costs.** The foreground window is asked for once per pass, for every module owed a report together, and only when one of them has an `initial` trigger still waiting: enabling a module that has none asks nothing. The callbacks run on the main thread, inside the loop pass whose length the application logs past 250 ms, like every other trigger.
- **The same window an activation hands over.** The callback gets the same window table, and a foreground window an activation never hands over — one without a title — is not reported; the report is used up all the same. Before the first matching callback of the pass, [`host.inputEpoch`](./timer.md#host-inputepoch) turns over once, however many modules the pass reports to, and each module that reads through desktop duplication has it opened before its own first matching callback — both as an activation does.
- **What raises.** `initial` is `true` or `false`; anything else raises from `onTrigger`.

What it does **not** do:

- **Only `"activate"` is dispatched.** `opts.on` defaults to it; any other value — `"open"`, `"close"`, `"focus"`, `"titleChange"` — is accepted and never fires, with or without `initial`. Focus moves inside a window go to [`onFocus`](#host-window-onfocus).
- **There is no handle.** `onTrigger` returns nothing, and a trigger cannot be removed; it stays until the module is reloaded. While the module is disabled it simply is not called. To stop work a trigger started, stop that work — a poll it armed is [cancelled by its token](./timer.md#host-timer-cancel).
- **One failing callback stops the rest.** A module's triggers are called one after another in registration order, with no protection between them: an error in one skips the module's later triggers for that foreground change, or for that initial report. The error is logged every time and shown to the user once.

```luau
local REAPER = { app = { name = "reaper" } }
-- Also called for a REAPER window that is already in front when the module loads or is
-- enabled: once, on the next tick, and not at all while the module is disabled.
host.window.onTrigger(REAPER, { initial = true }, function(w)
  host.speech.output("REAPER focused")
end)
```

In a `code_module` runtime the top level runs once in the runtime's own VM and once in every dependent's, so a trigger registered there is registered — and reported — once per VM; register from `activate`, which runs once, in the module's own VM (see [`host.require`](./require.md#host-require)). A poll that starts from `initial = true` and stops itself when the game leaves the front is the example under [`host.timer.cancel`](./timer.md#host-timer-cancel).

### Windows

A system-wide foreground hook, registered for **every process**, whether or not it cooperates with accessibility, is what fires this. A foreground window **without a title** is never handed to a trigger: it runs a focus round (the `onFocus` callbacks) instead. The focus and name-change hooks beside it only ever run a focus round, so a window whose title becomes matchable after it came forward does not fire `onTrigger` either — re-check such a window from `onFocus`.

The `initial` report asks the same question as [`host.window.active()`](#host-window-active): the foreground window, its title and class, and the name of its process. Unlike `active()`, it treats a window without a title as no window, the rule the foreground hook follows: an untitled dialog or the taskbar in front at load is not reported, and the report is used up.

No focus round is run when watching starts. The window already in front is seen by `onFocus` only when the foreground or the focus next changes, or when a module calls [`host.window.recheck()`](#host-window-recheck).

### macOS

Only *application* activation is system-wide. Focus-within-an-application, window-created and title-changed exist solely as per-process accessibility observers, created lazily the first time that application comes to the front and abandoned after three permanent refusals.

**For an application that is already frontmost, `initial = true` is the only report there is.** A game already in front when its module loads raises no application activation, so without `initial` it stays unreported until the user switches away and back. The report asks the frontmost application for its window through accessibility, as [`host.window.active()`](#host-window-active) does, with the same five-second memory for an application that does not answer; with no answer at all it finds no window, and nothing fires. A window without a title is not reported, as the activation path does not report one.

The consequence lands exactly on the embedded-plug-in case: a plug-in window opening inside a DAW that is **already** frontmost raises no application activation, so the event depends entirely on that per-process observer. In a host that refuses accessibility, the overlay never activates even though the same module works on Windows.

When watching first starts, one focus round is run for the application already in front, so `onFocus` callbacks registered by then see it; `onTrigger` sees it only through `initial = true`.

## host.window.onFocus(cb) {#host-window-onfocus}

`host.window.onFocus(cb: () -> ()) -> ()`

(prelude) Registers `cb` to fire whenever the keyboard focus moves — including within the same top-level window. Takes no arguments; the callback typically re-reads `host.window.active()` / `focusChain()`. Used to catch focus entering an embedded plugin without a foreground change. Like `onTrigger` it returns no handle, stays until the module is reloaded, and a callback that raises skips the module's later `onFocus` callbacks for that focus change.

```luau
host.window.onFocus(function()
  local chain = host.window.focusChain()
  -- re-evaluate which plugin (if any) now has focus
end)
```

### Internal prelude functions

`window_prelude.luau` also defines `W._hasTriggers()`, `W._wantsInitial(reprime)`, `W._dispatchActivate(win, beforeFirst)`, `W._dispatchInitial(win, reprime, beforeFirst)` and `W._dispatchFocus()`, and the host adds `W._requestInitial()`, which `onTrigger { initial = true }` calls to queue its module for the report. The host event loop calls the dispatchers to deliver foreground and focus changes, and the initial report, into the registered `onTrigger`/`onFocus` callbacks; modules do not call any of them directly.

### Windows

Fired by the system-wide focus and name-change hooks, and by a foreground change to a window without a title. No focus round is run when watching starts — see [`onTrigger`](#host-window-ontrigger).

### macOS

Same caveat as `onTrigger` above: focus changes *within* an application are delivered by a per-process accessibility observer rather than by a system-wide hook, so an application that will not answer accessibility produces no focus events at all.

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

### Windows

Answered by the 15 ms tick, in the manager window's timer and in a headless run alike.

### macOS

Answered by the same 15 ms tick as on Windows, in the wxWidgets timer and in a headless run alike.
