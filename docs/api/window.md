---
title: "host.window & host.os — windows, controls, matchers, triggers"
sidebar_position: 1
---

All coordinates are screen pixels (i32 → Luau `number`) unless noted. `id` fields are native window handles (Win32 `HWND` as a Luau integer). The `host.window.list`/`active`/`controls`/`focusChain` functions are native (Rust) bindings; `find`/`findAll`/`test`/`onTrigger`/`onFocus` are added by `window_prelude.luau` on top of them.

## Table shapes

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

## Matchers

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

## host.os.current

Read-only string: the current OS, from Rust `std::env::consts::OS` (`"windows"`, `"macos"`, `"linux"`, …). Not a function — a plain field.

```luau
if host.os.current == "windows" then ... end
```

## host.os.is(name)

`host.os.is(name: string) -> boolean`

Returns `true` when `name` equals the current OS string.

```luau
if host.os.is("macos") then ... end
```

---

## host.window.list()

`host.window.list() -> { Window }`

Returns an array of [window tables](#window-table) for all enumerable top-level windows.

```luau
for _, w in ipairs(host.window.list()) do
  host.log.info(w.title .. " [" .. w.class .. "]")
end
```

## host.window.active()

`host.window.active() -> Window?`

Returns the [window table](#window-table) for the foreground window, or `nil` if there is none.

```luau
local w = host.window.active()
if w then host.log.info("front: " .. w.title) end
```

## host.window.focus(id)

**Signature:** `host.window.focus(id: number) -> boolean`

Brings the window with that handle to the front and gives it the keyboard. Returns whether
the system accepted it.

**Believe the answer.** Both platforms can decline — Windows refuses a foreground change
under conditions it does not explain, and on macOS the window's own application has to be
activated as well as the window raised, either of which can fail. A module that announces
"back in the plugin" when the focus did not move has told somebody who cannot check that
they are somewhere they are not, which is the one failure this project treats as worse than
doing nothing.

```lua
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

## host.window.controls(win?)

`host.window.controls(win: Window?) -> { Control }`

Returns the child [control tables](#control-table) of `win` (its `id` is used), or of the active window when omitted. Returns an empty table if no window applies. Used to detect embedded plugins by control class/geometry.

```luau
for _, c in ipairs(host.window.controls()) do
  if string.match(c.class, "^Qt") then
    host.log.info("plugin control id=" .. c.id)
  end
end
```

## host.window.focusChain()

`host.window.focusChain() -> { Control }`

Returns [control tables](#control-table) from the currently focused element up to its top-level window. Used to detect that focus has entered an embedded plugin.

```luau
local chain = host.window.focusChain()
local focused = chain[1]   -- innermost focused control, if any
```

## host.window.find(matcher)

`host.window.find(matcher: Matcher) -> Window?`

(prelude) Returns the first window from `host.window.list()` that satisfies `matcher`, or `nil`.

```luau
local reaper = host.window.find({ app = { name = "reaper" } })
```

## host.window.findAll(matcher)

`host.window.findAll(matcher: Matcher) -> { Window }`

(prelude) Returns all windows from `host.window.list()` that satisfy `matcher`.

```luau
local editors = host.window.findAll({ title = { contains = "Notepad" } })
```

## host.window.test(matcher, win)

`host.window.test(matcher: Matcher, win: Window) -> boolean`

(prelude) Returns whether the given window table satisfies `matcher` (the same logic `find`/`findAll` apply). Used by overlay attach contexts.

```luau
local w = host.window.active()
if w and host.window.test({ windows = { class = "REAPERwnd" } }, w) then ... end
```

## host.window.onTrigger(matcher, opts, cb)

`host.window.onTrigger(matcher: Matcher, opts: { on: string? }?, cb: (win: Window) -> ()) -> ()`

(prelude) Registers `cb` to fire on every foreground change for which the new active window satisfies `matcher`. `opts.on` defaults to `"activate"` (the only event dispatched). The callback receives the matched [window table](#window-table). An empty matcher `{}` matches every window.

```luau
host.window.onTrigger({ app = { name = "reaper" } }, { on = "activate" }, function(w)
  host.speech.output("REAPER focused")
end)
```

## host.window.onFocus(cb)

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

