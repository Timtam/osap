---
title: "host.speech + host.hotkey + host.keys + host.timer + host.log"
sidebar_position: 4
---

Input/output reference for the speech, hotkey, low-level key-capture, timer, and logging host namespaces. These are the closures registered in `crates/host/src/lib.rs` (`install_host_api`). Coordinates do not apply to this group. All callbacks run in the calling module's own Luau VM and only fire while that module is **enabled**.

## Key spec string format

Two namespaces parse `+`-joined spec strings; segments are trimmed and case-insensitive.

- **`host.hotkey.register`** uses the OS hotkey parser (`parse_spec`, Windows `RegisterHotKey`). The last segment is the key, all earlier segments are modifiers. Registered with `MOD_NOREPEAT` (no auto-repeat).
- **`host.keys.capture` / `host.keys.release`** use `backend::key_spec`, which yields `(vk, modifier-mask)`. A captured key fires **only when the pressed modifier state equals the mask exactly** — so `"Tab"` (mask 0) does not swallow `Alt+Tab`, and you must register `"Shift+Tab"` separately from `"Tab"`.

**Modifiers** (any combination): `Ctrl`/`Control`, `Alt`/`Option`, `Shift`, `Win`/`Super`/`Cmd`/`Command`/`Meta`.

**Key names** (`key_to_vk`): single ASCII letter `A`–`Z` (case-insensitive) or digit `0`–`9`; function keys `F1`–`F24`; and the named keys `Space`, `Enter`/`Return`, `Esc`/`Escape`, `Tab`, `Backspace`, `Delete`/`Del`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`. An unknown modifier or key makes the call raise an error.

Examples: `"Ctrl+Alt+P"`, `"Tab"`, `"Shift+Tab"`, `"Ctrl+Right"`, `"F5"`.

---

## host.speech.output(text, opts?)

**Signature:** `host.speech.output(text: string, opts: { interrupt: boolean? }?)` → `nil`

Speaks `text` through the host's screen-reader/TTS bridge; `opts.interrupt` defaults to `true` (omitting `opts` also means interrupt). Output is **not** echoed to the console (a screen reader reading the terminal would double the speech). Raises an error if the TTS backend fails.

```lua
host.speech.output("Reverb enabled")
host.speech.output("loading...", { interrupt = false })  -- queue, don't cut off
```

---

## host.hotkey.register(spec, callback)

**Signature:** `host.hotkey.register(spec: string, callback: () -> ()) ` → `id: number`

Registers a **global** OS hotkey (active regardless of foreground window) for the [key spec](#key-spec-string-format) and returns an integer `id`. The callback is invoked with no arguments each time the hotkey fires (only while the owning module is enabled). Raises an error if the spec is invalid or the OS refuses the registration (e.g. already taken).

```lua
local id = host.hotkey.register("Ctrl+Alt+P", function()
  host.speech.output("Hotkey pressed")
end)
```

## host.hotkey.unregister(id)

**Signature:** `host.hotkey.unregister(id: number)` → `nil`

Releases the OS hotkey and forgets the callback for the `id` returned by `register`. Unknown ids are ignored.

```lua
host.hotkey.unregister(id)
```

---

The `host.keys` namespace is a low-level, modifier-aware keyboard hook that **intercepts and suppresses** individual keystrokes (the key does not reach the focused application) and delivers them to your callback. It is distinct from `host.hotkey`: keys are matched on exact modifier state and the captured set is recomputed across all enabled modules whenever it changes.

## host.keys.capture(spec, callback)

**Signature:** `host.keys.capture(spec: string, callback: (mods: { shift: boolean, ctrl: boolean, alt: boolean, win: boolean }) -> ())` → `nil`

Begins intercepting the [key spec](#key-spec-string-format): the keypress is swallowed (not passed to the underlying app) and `callback` is invoked with a `mods` table describing the modifier state at press time. Re-capturing the same `(vk, mask)` for this module replaces the previous callback. Installs the low-level keyboard hook on first use (idempotent). Raises an error for an unknown spec.

```lua
host.keys.capture("Tab", function(mods)
  -- Tab is now swallowed app-wide (or in the scoped window); move overlay focus
  moveFocus(mods.shift and -1 or 1)
end)
host.keys.capture("Shift+Tab", function() moveFocus(-1) end)
```

## host.keys.release(spec)

**Signature:** `host.keys.release(spec: string)` → `nil`

Stops capturing exactly that `(vk, mask)` for this module and recomputes the global captured set so the key reaches apps normally again. A spec that doesn't parse is silently ignored.

```lua
host.keys.release("Tab")
```

## host.keys.releaseAll()

**Signature:** `host.keys.releaseAll()` → `nil`

Removes **all** key captures owned by this module and refreshes the suppression set. Useful when tearing down an overlay.

```lua
host.keys.releaseAll()
```

## host.keys.scope(toForeground)

**Signature:** `host.keys.scope(toForeground: boolean)` → `nil`

Scopes captured-key suppression. `true` pins it to the **current foreground window** (captured via `GetForegroundWindow` at call time) — the hook then only intercepts keys while that window is foreground, so a menu/popup that brings another window forward receives keys natively (ReaHotkey's `HotIf WinActive` model). `false` makes capture global again.

```lua
host.keys.scope(true)   -- only suppress while the plugin window is focused
```

## host.keys.menuOpen(open)

**Signature:** `host.keys.menuOpen(open: boolean)` → `nil`

Tells the hook a plugin's own (Qt/UIA) menu is open (`true`) or closed (`false`). While open, captured navigation keys (e.g. `Tab`/`Enter`) **pass through** to that menu instead of being consumed by the overlay — covering plugin menus the Win32 menu-state check can't see.

```lua
host.keys.menuOpen(true)
-- ... user navigates the plugin's native menu ...
host.keys.menuOpen(false)
```

---

## host.timer.after(ms, callback)

**Signature:** `host.timer.after(ms: number, callback: () -> ())` → `nil`

Schedules a **one-shot** callback to fire approximately `ms` milliseconds later, driven from the event-loop tick. The callback runs once with no arguments (only if the module is still enabled at fire time) and is then discarded. There is no returned handle and no way to cancel an individual timer.

```lua
host.timer.after(500, function()
  host.speech.output("Half a second later")
end)
```

---

## host.log.info(msg)

**Signature:** `host.log.info(msg: string)` → `nil`

Writes `msg` to the host log under the `module` channel. This is the **only** method on `host.log` — there is no `warn`/`error`/`debug`.

```lua
host.log.info("module initialized")
```

