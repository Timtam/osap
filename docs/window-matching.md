# Window Matching & Triggers

*Status: partly built, 2026-09-21. Built and working as the [API reference](api/window.md#matchers) describes: the declarative matcher as a plain table (`title`, `app`, `os`, the `windows` / `macos` / `linux` blocks with `class` and the macOS `ax*` fields, per-field OS keys, `where`), `host.window.find` / `findAll` / `test`, and `host.window.onTrigger` for **activation only**. Not built: the `host.match` constructor, `controlClass` and `wmClass`/`appId`, methods on window tables, the trigger events other than `activate`, `host.window.triggers.available()` and a polling fallback — each is marked where it appears below. Originally drafted 2026-06-21. Belongs to [host-api-capability-catalog.md](host-api-capability-catalog.md) (`host.window`). Formalizes "capability detection instead of parity" for window detection.*

## Principle

Window detection **cannot** be solved as "one matcher equivalent across all OSes": the parameters are named differently per OS or only exist there. There is no universal "class". Therefore: **shared, normalized fields + platform-specific blocks + OS gating + an escape-hatch predicate.** Inspirations: AHK `WinTitle` criteria (`ahk_class`/title/`ahk_exe`/`ahk_pid`), ReaHotkey's detection cascade, Keyboard Maestro triggers — generalized and platform-gated.

## Why per-OS blocks are necessary

| Concept | Windows | macOS | Linux (X11 / Wayland) |
|---|---|---|---|
| App identity | exe (`reaper.exe`), PID | **Bundle ID** (`com.cockos.reaper`), PID | comm / app_id |
| "Window class" | Win32 class (`ahk_class`) | **AXRole / AXSubrole** (`AXWindow`/`AXFloatingWindow`) | WM_CLASS / app_id |
| Window ID | HWND | AXUIElement / CGWindowID | XID / — |
| Title | window text | AXTitle / CGWindowName | _NET_WM_NAME |
| Control class | ClassNN | AXIdentifier / AXRole | AT-SPI role |
| Stable identifier | (Class) | AXIdentifier (if set) | — |

## The matcher as built

A matcher is a **plain Luau table** — there is no constructor — passed to `host.window.find`, `findAll`, `test` and `onTrigger`, and to the overlay's bindings:

```luau
local matcher = {
  -- Shared fields
  title = { prefix = "Serum 2" },
  app   = { name = "reaper" },               -- name / exe / bundleId / pid

  -- Platform-specific blocks (OS gating)
  windows = { class = { pattern = "^VSTGUI%x+$" } },
  macos   = { axRole = "AXWindow", axSubrole = "AXFloatingWindow",
              axIdentifier = { contains = "Serum" },
              app = { bundleId = "com.cockos.reaper" } },

  -- Escape hatch (ReaHotkey CheckerFunction): arbitrary check, run last
  where = function(win)
    return host.screen.imageSearch(host.path("images/serum2.png"), {
      region = { win.bounds.x, win.bounds.y, win.bounds.x + win.bounds.w, win.bounds.y + win.bounds.h },
    }) ~= nil
  end,
}
```

**Evaluation:** shared fields AND the block for the current OS. **If blocks are present but none for the current OS → no match on that OS** (OS gating for free). The `where` predicate runs **last** — cascade: cheap structured fields first (title/app/class), then callback/image/OCR only on candidates (like ReaHotkey).

**Match modes per field:** a plain string (case-insensitive equality), or a table with one of `exact`, `contains`, `prefix`, `suffix` (all case-insensitive), `pattern` / `regex` (both Luau string patterns, case-sensitive), `not`. A table with none of them matches nothing; the exact precedence is in the [reference](api/window.md#matchers).

**Keys that are read**, and no others: `title`, `app` (`name`, `exe`, `bundleId`, `pid`), `os`, the `windows` / `macos` / `linux` blocks (each taking `title`, `app`, `class`, and on macOS `axRole`, `axSubrole`, `axIdentifier`), and `where`. **Any other key is ignored without an error** — a top-level `class`, or `controlClass`, included — so a misplaced field widens the match rather than failing.

**Not built from the original design:** the `host.match { … }` constructor (a matcher is the table itself); `controlClass` for a child control's class (walk [`host.window.controls`](api/window.md#host-window-controls) in `where`, or use an embedded binding); the `linux` block's `wmClass` / `appId` (the `linux` key gates, but no Linux backend fills anything in).

## Any single field, per platform

The match modes (`exact`, `contains`, `prefix`, `suffix`, `pattern`, `regex`, `not`) and the
platform names (`windows`, `macos`, `linux`) are different words, so one table can carry
either and a matcher only reaches for the platform keys at the field that actually differs.
That works for every field the matcher reads — `title`, the `app` fields, and the fields
inside a platform block:

```luau
{ title = { windows = { contains = "Kontakt" }, macos = { prefix = "Kontakt" } },
  windows = { class = { prefix = "NINormalWindow" } },
  macos   = { class = "AXWindow/AXStandardWindow/" } }
```

A `class` keyed by platform at the **top level** — `class = { windows = …, macos = … }` — is
not one of those fields: it is ignored, and the matcher then matches any window whose title
contains "Kontakt". The class goes in the platform blocks, as above.

A field that names platforms and not the current one does **not** match — the same rule the
blocks follow, and for the same reason: a field that quietly matched everything on a fourth
system would be a Windows-only module claiming a Mac window.

`host.os.pick { windows = …, macos = … }` applies the same resolution outside a matcher, for
the places that are not fields — the control-class pattern of an embedded binding, mostly.

## What macOS actually publishes in `class`

One string, composed as `AXRole/AXSubrole/AXIdentifier`, both separators always present and
a missing part empty: `"AXWindow/AXStandardWindow/"`, `"AXGroup//NI.Kontakt.Main"`. It is
one field because `class` is what every module's detection already matches on. A matcher can
name a part instead — `macos = { axRole = …, axSubrole = …, axIdentifier = … }` — and the
accessibility dump prints the identical string, so what you read in a dump is what you write
in a matcher.

## OS gating

- **Primarily** via the presence of the OS blocks (a matcher without a `macos` block never matches there → a Windows-only module is cleanly declared).
- **Explicitly:** `os = {"windows","macos"}` in the matcher; `host.os.current` (`"windows"|"macos"|"linux"`) and `host.os.is("macos")` for imperative branching.
- **Coarse (manifest):** `supported_os` in `module.toml` at the package level. It decides whether the module is loaded at all; it cannot name a window. The manifest has no window or process matching — see [module-package-format.md](module-package-format.md#the-manifest-moduletoml).

## API

```luau
host.window.find(matcher)    -> Window?      -- first match
host.window.findAll(matcher) -> Window[]
host.window.test(matcher, win) -> boolean
host.window.active()         -> Window?
host.window.list()           -> Window[]
```

`Window` is a plain table: `id`, `title`, `class`, `app` `{ name, exe, pid, bundleId }`, `bounds` `{ x, y, w, h }`, `client` `{ x, y, w, h }` — see [Table shapes](api/window.md#table-shapes). **It has no methods.** The design's `win:attr("AXSubrole")`, `win:class()`, `win:focusedControl()` and `win:listViewContent(id)` were not built; the class is the `class` field, the focused element is the first entry of [`host.window.focusChain()`](api/window.md#host-window-focuschain), and a list view's content is read through [`host.element`](api/element.md) or OCR.

## Determining parameters (Inspector)

The values needed per OS (Win32 class, AX role/subrole/identifier, bundle ID, control coordinates) are not determined by guessing — for this the tool is meant to ship a **window/control inspector** (à la AHK *Window Spy* / ReaHotkey *Overlay Designer*, but inspection only, without code generation; a project-backlog "dev tools" item). Today `tools/probe` and the overlay calibrator answer most of it.

## Triggers ("trigger-like system")

```luau
host.window.onTrigger(matcher, { on = "activate" }, function(win)
  -- e.g. auto-activate overlay / start automation
end)
```

Fires when a matching window **becomes the foreground window** — the auto-activation for overlays (ReaHotkey's 500-ms state machine, but **event-driven**). What is built, and what is not:

- **Only `"activate"` is dispatched.** The design's `"open"`, `"close"`, `"focus"` and `"titleChange"` are accepted and never fire. Focus moves within a window reach [`host.window.onFocus`](api/window.md#host-window-onfocus) instead.
- **The window already in front is reported only when asked for:** `{ initial = true }` reports it once, on the next tick after the load, after each enable and after a later registration; see [`onTrigger`](api/window.md#host-window-ontrigger). On macOS that is the only report a game already frontmost at load gets. Overlay bindings do evaluate at once.
- **No handle, no removal**; a trigger stays until the module is reloaded.

**Backend:** Win = `SetWinEventHook` (`EVENT_SYSTEM_FOREGROUND` for triggers; focus and name changes only run a focus round); macOS = `NSWorkspace.didActivateApplication` + per-application `AXObserver`. **Not built:** a polling fallback for applications that raise no events, and `host.window.triggers.available()` to ask whether triggers work at all; there is no Linux or Wayland backend.

## Connections

- The overlay's bindings use a matcher for auto-activation — replacing ReaHotkey's `Plugin.Register(class-regex, CheckerFunction)` with a declarative, OS-gated equivalent.
- **Per-OS matchers + per-OS calibration** (coordinates/images, see [reahotkey-port-analysis.md](reahotkey-port-analysis.md) §2) together form the "content" layer of a module: a module *can* support multiple OSes, but each OS with its own matchers and its own calibrated assets.
- The matcher system is not overlay-specific — every automation uses `host.window.find/findAll/onTrigger` directly (first-class, design principle catalog §1).
