# Window Matching & Triggers (Draft)

*Status: Draft, 2026-06-21. Belongs to [host-api-capability-catalog.md](host-api-capability-catalog.md) (`host.window`). Formalizes "capability detection instead of parity" for window detection.*

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

```lua
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

## OS gating

- **Primarily** via the presence of the OS blocks (a matcher without a `macos` block never matches there → a Windows-only module is cleanly declared).
- **Explicitly:** `os = {"windows","macos"}` in the matcher; `host.os.current` (`"windows"|"macos"|"linux"`) and `host.os.is("macos")` for imperative branching.
- **Coarse (manifest):** `supported_os` in `module.toml` at the package level.

## API

```lua
host.window.find(matcher)    -> Window?      -- first match
host.window.findAll(matcher) -> Window[]
host.window.active()         -> Window?
host.window.list()           -> Window[]
```
`Window` (normalized): `id`, `title`, `app` `{ name, bundleId, exe, pid }`, `bounds` `{ x, y, w, h }`; raw OS access via `win:attr("AXSubrole")` / `win:class()` / `win:focusedControl()` / `win:listViewContent(id)`.

## Determining parameters (Inspector)

The values needed per OS (Win32 class, AX role/subrole/identifier, bundle ID, control coordinates) are not determined by guessing — for this the tool ships a **window/control inspector** (à la AHK *Window Spy* / ReaHotkey *Overlay Designer*, but inspection only, without code generation; see [TODO](../TODO.md) → dev tools). It shows, live for the window/element under the cursor, all matchable parameters + geometry + pixel color, and is itself built on the first-class primitives (`host.window`/`host.a11y`/`host.screen`).

## Triggers ("trigger-like system")

```lua
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
