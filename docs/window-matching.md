# Window Matching & Triggers

*Status: implemented, 2026-08-13 — the platform blocks, the macOS `ax*` fields, `os = {…}`
gating and per-field OS keys all work as described. Originally drafted 2026-06-21. Belongs to [host-api-capability-catalog.md](host-api-capability-catalog.md) (`host.window`). Formalizes "capability detection instead of parity" for window detection.*

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

## WindowMatcher (declarative)

```luau
local matcher = host.match {
  -- Shared fields (host normalizes per OS where possible)
  title = { regex = "^Serum 2" },
  app   = { name = "REAPER", bundleId = "com.cockos.reaper", exe = "reaper.exe" },

  -- Platform-specific blocks (OS gating)
  windows = { class = { regex = "^VSTGUI%x+$" }, controlClass = "SysListView321" },
  macos   = { axRole = "AXWindow", axSubrole = "AXFloatingWindow",
              axIdentifier = { contains = "Serum" } },
  linux   = { wmClass = "...", appId = "..." },          -- later

  -- Escape hatch (ReaHotkey CheckerFunction): arbitrary check
  where = function(win)
    return win:listViewContent("SysListView321"):match("Serum 2")
        or host.screen.imageSearch("assets/serum2.png", { region = win.bounds })
  end,
}
```

**Evaluation:** shared fields AND the block matching the current OS. **If the OS block is missing → no match on that OS** (OS gating for free). The `where` predicate runs **last** — cascade: cheap structured fields first (title/app/class), then callback/image/OCR only on candidates (like ReaHotkey).

**Match modes per field:** `{ exact = "…" }` (default), `{ contains = "…" }`, `{ regex = "…" }`, `{ prefix/suffix = "…" }`, `{ not = … }` (exclude).

## Any single field, per platform

The match modes (`exact`, `contains`, `prefix`, `suffix`, `pattern`, `regex`, `not`) and the
platform names (`windows`, `macos`, `linux`) are different words, so one table can carry
either and a matcher only reaches for the platform keys at the field that actually differs:

```luau
{ title = { contains = "Kontakt" },
  class = { windows = { prefix = "NINormalWindow" }, macos = "AXWindow/AXStandardWindow/" } }
```

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
- **Coarse (manifest):** `supported_os` in `module.toml` at the package level.

## API

```luau
host.window.find(matcher)    -> Window?      -- first match
host.window.findAll(matcher) -> Window[]
host.window.active()         -> Window?
host.window.list()           -> Window[]
```
`Window` (normalized): `id`, `title`, `app` `{ name, bundleId, exe, pid }`, `bounds` `{ x, y, w, h }`; raw OS access via `win:attr("AXSubrole")` / `win:class()` / `win:focusedControl()` / `win:listViewContent(id)`.

## Determining parameters (Inspector)

The values needed per OS (Win32 class, AX role/subrole/identifier, bundle ID, control coordinates) are not determined by guessing — for this the tool ships a **window/control inspector** (à la AHK *Window Spy* / ReaHotkey *Overlay Designer*, but inspection only, without code generation; a project-backlog "dev tools" item). It shows, live for the window/element under the cursor, all matchable parameters + geometry + pixel color, and is itself built on the first-class primitives (`host.window`/`host.a11y`/`host.screen`).

## Triggers ("trigger-like system")

```luau
host.window.onTrigger(matcher, { on = "activate" }, function(win)
  -- e.g. auto-activate overlay / start automation
end)
-- on = "activate" | "open" | "close" | "focus" | "titleChange"
```
Fires when a matching window becomes active/appears — the auto-activation for overlays (ReaHotkey's 500-ms state machine, but **event-driven** where possible).

**Backend:** Win = `SetWinEventHook` (`EVENT_SYSTEM_FOREGROUND` etc.); macOS = `NSWorkspace.didActivateApplication` + `AXObserver` (`kAXFocusedWindowChanged`); polling fallback (for non-event-capable cases). Wayland: heavily restricted → check `host.window.triggers.available()`.

## Connections

- `host.overlay` registration uses a WindowMatcher + trigger for auto-activation — replaces ReaHotkey's `Plugin.Register(class-regex, CheckerFunction)` with a declarative, OS-gated equivalent.
- **Per-OS matchers + per-OS calibration** (coordinates/images, see [reahotkey-port-analysis.md](reahotkey-port-analysis.md) §2) together form the "content" layer of a module: a module *can* support multiple OSes, but each OS with its own matchers and its own calibrated assets.
- The matcher system is not overlay-specific — every automation uses `host.window.find/findAll/onTrigger` directly (first-class, design principle catalog §1).
