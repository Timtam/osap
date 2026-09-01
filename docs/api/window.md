---
title: "host.window — windows, controls and focus"
sidebar_position: 16
toc_max_heading_level: 2
---

Finds windows and the surfaces inside them, reports where the keyboard is, and raises an event when either changes. In short: which window is in front, what surfaces sit inside it, where the keyboard actually is, and whether a point on screen still belongs to the window you think it does.

Most modules never call any of it — an overlay's binding does the finding, and the overlay runtime registers `onTrigger` and `onFocus` once on everyone's behalf — so you come here for the question a binding cannot settle by itself: Kontakt walks `controls()` for the Qt container that says which Kontakt this is and whether a Komplete Kontrol wraps it, sforzando reads the depth of `focusChain()` to tell REAPER's own chrome from the plug-in inside it, and daw-hosts pairs `find` with `focus` to build the "put me back in the plug-in window" key that macOS otherwise has no command for.

Nothing here touches the screen, so one call is cheap — but every overlay makes several on every foreground and focus change. Kontakt measured a single cell's context match at 15 to 96 ms with every underlying OS answer already served from cache, which is thousands of cheap calls rather than one expensive one, and why its six cells derive the whole scene once per epoch instead of each walking the control list.

The same call answers with different things per platform, so read the platform sections: `controls()` yields every visible child window on Windows and container elements only on macOS, and a focus chain's links are windows there and accessibility elements here — test what a chain contains rather than how deep it is. Believe what you are told rather than what you assume: `focus` can be declined by either platform, and `ownsPoint` answers `nil` on macOS, which a caller must read as permission rather than refusal.

All coordinates are screen pixels on Windows and **points** on macOS (i32 → Luau `number`) unless noted — see [`host.screen.size()`](screen#host-screen-size) for why that distinction costs a day when it is missed. `id` fields are native window handles on Windows (a Win32 `HWND` as a Luau integer) and interned counters on macOS; see the platform sections under [Table shapes](#table-shapes).

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
  client = { x = 8, y = 31 },                      -- client-area top-left (screen px)
}
```

`client` is the screen-pixel origin of the window's client area; overlay regions are expressed relative to it.

### Control table

Returned by `host.window.controls()` and `host.window.focusChain()`. Built by `control_to_table`:

```luau
{
  id     = 0x001B44,                            -- control handle (HWND), integer
  class  = "Qt5152QWindowIcon",                 -- control window class
  bounds = { x = 100, y = 200, w = 640, h = 480 }, -- control rect (screen px)
  client = { x = 108, y = 231 },                -- control client-area top-left (screen px)
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

A *matcher* is a declarative table passed to `host.window.find/findAll/test/onTrigger`. Fields (all optional; all must match):

- `title` — a **field matcher** tested against the window title.
- `app` — table with any of `name` / `exe` (field matchers) and `pid` (exact number).
- `windows` / `macos` / `linux` — per-OS blocks. If any platform block is present but the current OS is not among them, the matcher fails. The current-OS block may contain `class` (a field matcher against the window class).
- `where` — a `function(win) -> boolean` escape-hatch predicate, evaluated last.

A **field matcher** is either a string (case-insensitive equality) or a table with exactly one of:

- `exact` — case-insensitive equality
- `contains` / `prefix` / `suffix` — case-insensitive substring/edge match
- `pattern` / `regex` — Luau string pattern (both evaluated as Luau patterns; **case-sensitive**)
- `not` — negates a nested field matcher

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

## host.window.list() {#host-window-list}

`host.window.list() -> { Window }`

Returns an array of [window tables](#window-table) for all enumerable top-level windows.

```luau
for _, w in ipairs(host.window.list()) do
  host.log.info(w.title .. " [" .. w.class .. "]")
end
```

## host.window.active() {#host-window-active}

`host.window.active() -> Window?`

Returns the [window table](#window-table) for the foreground window, or `nil` if there is none.

```luau
local w = host.window.active()
if w then host.log.info("front: " .. w.title) end
```

## host.window.focus(id) {#host-window-focus}

**Signature:** `host.window.focus(id: number) -> boolean`

Brings the window with that handle to the front and gives it the keyboard. Returns whether
the system accepted it.

**Believe the answer.** Both platforms can decline — Windows refuses a foreground change
under conditions it does not explain, and on macOS the window's own application has to be
activated as well as the window raised, either of which can fail. A module that announces
"back in the plugin" when the focus did not move has told somebody who cannot check that
they are somewhere they are not, which is the one failure this project treats as worse than
doing nothing.

```luau
local w = host.window.find({ title = { contains = "sforzando" } })
if w and host.window.focus(w.id) then
    host.speech.output("sforzando")
else
    host.speech.output("could not get back to sforzando")
end
```

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

Three separate things are attempted — raising the window, activating its application, and setting the accessibility focus flag — and the return is true if **either of the first two** succeeded. That is weaker than it reads: a tester session had it return `true` while the keyboard stayed on the host application's own control, and one Shift+Tab was needed to get in. So `true` means "raised", not "the keyboard is in it", and an overlay gating on focus-being-inside-the-plug-in can still decline to act after this reported success.

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

## host.window.ownsPoint(id, x, y) {#host-window-ownspoint}

`host.window.ownsPoint(id: number, x: number, y: number) -> boolean?`

Whether the window `id` belongs to is the one drawn at that screen point.

Every hotspot in this project clicks a coordinate worked out from a window's own frame, and knowing the point falls *inside* that frame says nothing about what is drawn there. A notification, a tooltip, another application raised over it: the click goes to whichever window owns the pixel. This is how an overlay finds that out before clicking rather than after.

Compared at the **top level**. An overlay's origin is often a child — an embedded plug-in is a control inside its host's window — and the point resolves to whichever child is drawn there, usually a different one. So both sides are taken up to their root, and the question answered is "does this point belong to the same application window".

**`nil` means the platform cannot say, and must be read as permission rather than refusal.** A check with no answer must not block what it cannot judge. Only a definite `false` should stop anything.

### Windows

Implemented with `WindowFromPoint` + `GetAncestor(GA_ROOT)` on both sides. `WindowFromPoint` is the window manager's own hit test, so a click-through window (`WS_EX_TRANSPARENT`) is correctly seen through rather than treated as a cover.

### macOS

Returns `nil` — not implemented. An overlay there behaves exactly as it did before this existed, and the probe records which answer it got, so a log says whether the check is live on that machine.

It is unimplemented on purpose rather than by omission. The cheap route, `CGWindowListCopyWindowInfo`, answers *what is drawn* at a point, and a click is delivered by *what would be hit* — which is a different question wherever a window lets clicks through. A screen reader's cursor ring is exactly such a window, and it is drawn over the control being operated, so that implementation would refuse essentially every press with a spoken excuse, on the machines of the people this exists for. `NSWindow.windowNumberAtPoint:belowWindowWithWindowNumber:` asks the right question; it needs a Cargo feature, the main thread, and a bottom-left coordinate flip.

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

(prelude) Returns the first window from `host.window.list()` that satisfies `matcher`, or `nil`.

```luau
local reaper = host.window.find({ app = { name = "reaper" } })
```

## host.window.findAll(matcher) {#host-window-findall}

`host.window.findAll(matcher: Matcher) -> { Window }`

(prelude) Returns all windows from `host.window.list()` that satisfy `matcher`.

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

`host.window.onTrigger(matcher: Matcher, opts: { on: string? }?, cb: (win: Window) -> ()) -> ()`

(prelude) Registers `cb` to fire on every foreground change for which the new active window satisfies `matcher`. `opts.on` defaults to `"activate"` (the only event dispatched). The callback receives the matched [window table](#window-table). An empty matcher `{}` matches every window.

```luau
host.window.onTrigger({ app = { name = "reaper" } }, { on = "activate" }, function(w)
  host.speech.output("REAPER focused")
end)
```

### Windows

Three system-wide hooks — foreground change, focus change, and name change — registered for **every process**, whether or not it cooperates with accessibility.

### macOS

Only *application* activation is system-wide. Focus-within-an-application, window-created and title-changed exist solely as per-process accessibility observers, created lazily the first time that application comes to the front and abandoned after three permanent refusals.

The consequence lands exactly on the embedded-plug-in case: a plug-in window opening inside a DAW that is **already** frontmost raises no application activation, so the event depends entirely on that per-process observer. In a host that refuses accessibility, the overlay never activates even though the same module works on Windows.

## host.window.onFocus(cb) {#host-window-onfocus}

`host.window.onFocus(cb: () -> ()) -> ()`

(prelude) Registers `cb` to fire whenever the keyboard focus moves — including within the same top-level window. Takes no arguments; the callback typically re-reads `host.window.active()` / `focusChain()`. Used to catch focus entering an embedded plugin without a foreground change.

```luau
host.window.onFocus(function()
  local chain = host.window.focusChain()
  -- re-evaluate which plugin (if any) now has focus
end)
```

### Internal prelude functions

`window_prelude.luau` also defines `W._hasTriggers()`, `W._dispatchActivate(win)`, and `W._dispatchFocus()`. These are called by the host event loop to deliver foreground/focus changes into the registered `onTrigger`/`onFocus` callbacks; modules do not call them directly.

### macOS

Same caveat as `onTrigger` above: focus changes *within* an application are delivered by a per-process accessibility observer rather than by a system-wide hook, so an application that will not answer accessibility produces no focus events at all.

## host.window.recheck() {#host-window-recheck}

**Signature:** `host.window.recheck() -> ()`

Asks the host to run a focus-change round at the end of the current tick — after OS events, timers and image results have run — exactly as if the OS had reported one: the observation epoch turns over (so cached window answers are re-read) and every enabled module's `host.window.onFocus` callbacks fire — including the one the overlay runtime registers, which re-evaluates each overlay's context match and gate. It exists for the case where a module *itself* changed what is detectable on screen and no OS event will follow: Komplete Kontrol auto-closing its library browser reveals the nested Kontakt underneath, and nothing about that is a focus change, so without this nobody notices until the next `pollMatch` tick (~500 ms) — half a second in which the ring sits on the wrong overlay for somebody who cannot see that it moved. The dispatch is **cross-VM**, which is the whole point here: the module that acted is usually not the module that has to notice.

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
