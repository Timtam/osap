---
title: "host.speech + host.hotkey + host.keys + host.timer + host.log"
sidebar_position: 4
toc_max_heading_level: 2
---

Input/output reference for the speech, hotkey, low-level key-capture, timer, and logging host namespaces. These are the closures registered in `crates/host/src/lib.rs` (`install_host_api`). Coordinates do not apply to this group. All callbacks run in the calling module's own Luau VM and only fire while that module is **enabled**.

## Key spec string format {#key-spec-string-format}

Two namespaces parse `+`-joined spec strings; segments are trimmed and case-insensitive.

- **`host.hotkey.register`** uses the OS hotkey parser (`parse_spec`, Windows `RegisterHotKey`). The last segment is the key, all earlier segments are modifiers. Registered with `MOD_NOREPEAT` (no auto-repeat).
- **`host.keys.capture`** uses `backend::key_spec`, which yields `(vk, modifier-mask)`. A captured key fires **only when the pressed modifier state equals the mask exactly** — so `"Tab"` (mask 0) does not swallow `Alt+Tab`, and you must register `"Shift+Tab"` separately from `"Tab"`. `host.keys.capture` returns a **token** that `host.keys.release` takes to undo that exact capture.

**Modifiers** (any combination): `Ctrl`/`Control`, `Alt`/`Option`, `Shift`, `Win`/`Super`/`Cmd`/`Command`/`Meta`.

**Key names** (`key_to_vk`): single ASCII letter `A`–`Z` (case-insensitive) or digit `0`–`9`; function keys `F1`–`F24`; and the named keys `Space`, `Enter`/`Return`, `Esc`/`Escape`, `Tab`, `Backspace`, `Delete`/`Del`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`. An unknown modifier or key makes the call raise an error.

Examples: `"Ctrl+Alt+P"`, `"Tab"`, `"Shift+Tab"`, `"Ctrl+Right"`, `"F5"`.

---

There is a third form the prose above does not mention: **`"<modifier> tap"`** — a modifier pressed and released on its own, with no other key in between. It is a `host.keys` capture only, never a global hotkey; macOS refuses it outright with a message pointing at `host.keys`, because Carbon has no notion of it.

```luau
-- A global hotkey: modifiers first, the key last. Ctrl+Option belongs to VoiceOver on
-- macOS, so the Mac gets a different combination.
local KEY = host.os.pick { windows = "Ctrl+Shift+F9", macos = "Cmd+Shift+F9" }
host.hotkey.register(KEY, writeProbeLog)

-- host.keys matches the modifier state EXACTLY, so the two directions are two captures:
-- "Tab" (mask 0) never fires for Shift+Tab, and never for Alt+Tab either.
host.keys.capture("Tab", function() ov:focusNext() end)
host.keys.capture("Shift+Tab", function() ov:focusPrev() end)

-- The tap form. Never suppressed: the modifier still reaches the application.
host.keys.capture("Alt tap", function() ov:activate() end)
```

### Windows

Modifier names are the Windows ones and mean what they say.

### macOS

Modifiers map **by position, not by name**: Ctrl is Control, Alt is Option, and Win/Cmd is Command. A spec written for one platform therefore names a different physical key here, which matters most for `Alt` — on macOS that key also composes characters, so claiming `Alt+E` or `Alt+N` takes the acute and tilde dead keys away from any text field while the overlay is up.

### Both

The tap form behaves the same on either platform: armed when a modifier goes down from rest, dropped by anything at all in between — an ordinary key, a second modifier, Caps Lock — and fired on the release. It is never suppressed, so the modifier keeps working as a modifier.

Until recently only `"Alt tap"` was armed on Windows, and `"Ctrl tap"`, `"Shift tap"` and `"Cmd tap"` were accepted, returned a token and could never fire. All four work now.

## host.speech.output(text, opts?) {#host-speech-output}

**Signature:** `host.speech.output(text: string, opts: { interrupt: boolean? }?)` → `nil`

Speaks `text`; `opts.interrupt` defaults to `true` (omitting `opts` also means interrupt). Output is **not** echoed to the console — a screen reader reading the terminal would double the speech.

```luau
host.speech.output("Reverb enabled")
host.speech.output("loading...", { interrupt = false })  -- queue, don't cut off
```

**Where it comes out**, which the caller does not choose and does not need to know:

- **Windows** — the running screen reader if there is one (NVDA, JAWS and Narrator all go through Tolk), otherwise SAPI.
- **macOS** — the platform's own voice, unless **Speak through VoiceOver** is ticked in the Application settings tab. Ticked, the line goes to VoiceOver and arrives in the user's voice, at their rate, and **on their braille display**, which nothing else can do.

  It is off until somebody asks for it, and the reason is the permission rather than the feature: the first line through this path is an Apple Event, and the first Apple Event makes macOS put an Automation consent dialog on screen. On by default, that dialog appears at startup — before the user has asked for anything, about a thing they may not want, in front of a person who cannot see it to dismiss it. Ticking the box is the request, and that is the moment to ask.

  Two things then send a line to the platform's own voice anyway:
  - **VoiceOver is not running.** Checked before every line, and deliberately not remembered: VoiceOver started later in the session simply starts being used. The check is also why the overlay cannot *start* VoiceOver — `tell application "VoiceOver"` would launch it, and a tool that switches on a screen reader nobody asked for is not acceptable behaviour.
  - **VoiceOver refuses**, most often because AppleScript control is not allowed. The line comes back and is said by the fallback, the reason is logged **once**, and the path is parked for the session so no further line pays for a process launch that will fail. Ticking the setting again re-arms it.

  Either way the line is said, and unticking the switch turns the path off again for anyone who prefers a second, distinct voice.

`interrupt` governs the platform's own queue. On the VoiceOver path, whether an announcement also cuts off what VoiceOver is saying for its own reasons is VoiceOver's decision, not one this API can make.

This call never raises: a failing speech engine must not take a module's key handler down with it.

---

## host.hotkey.register(spec, callback) {#host-hotkey-register}

**Signature:** `host.hotkey.register(spec: string, callback: () -> ()) ` → `id: number`

Registers a **global** OS hotkey (active regardless of foreground window) for the [key spec](#key-spec-string-format) and returns an integer `id`. The callback is invoked with no arguments each time the hotkey fires (only while the owning module is enabled). Raises an error if the spec is invalid or the OS refuses the registration (e.g. already taken).

```luau
local id = host.hotkey.register("Ctrl+Alt+P", function()
  host.speech.output("Hotkey pressed")
end)
```

### Windows

`RegisterHotKey`, with auto-repeat suppressed. A combination already held by another application is refused, and the refusal is logged by name.

### macOS

A Carbon event hotkey — deliberately **not** an event tap, so this needs no Input Monitoring grant even though `host.keys` does.

A `"<modifier> tap"` spec is **refused outright here**, with a message pointing at `host.keys`: Carbon has no notion of a bare modifier press, and registering something plausible instead would fire on the wrong key. Windows rejects it too, but only incidentally — its hotkey parser has no tap branch at all, so the refusal reads as an unknown key rather than as an explanation.

## host.hotkey.unregister(id) {#host-hotkey-unregister}

**Signature:** `host.hotkey.unregister(id: number)` → `nil`

Releases the OS hotkey and forgets the callback for the `id` returned by `register`. Unknown ids are ignored.

```luau
host.hotkey.unregister(id)
```

---

The `host.keys` namespace is a low-level, modifier-aware keyboard hook that **intercepts and suppresses** individual keystrokes (the key does not reach the focused application) and delivers them to your callback. It is distinct from `host.hotkey`: keys are matched on exact modifier state and the captured set is recomputed across all enabled modules whenever it changes.

## host.keys.capture(spec, callback) {#host-keys-capture}

**Signature:** `host.keys.capture(spec: string, callback: (mods: { shift: boolean, ctrl: boolean, alt: boolean, win: boolean }) -> ())` → `number` (a release token)

Begins intercepting the [key spec](#key-spec-string-format): the keypress is swallowed (not passed to the underlying app) and `callback` is invoked with a `mods` table describing the modifier state at press time. Re-capturing the same `(vk, mask)` for this module replaces the previous callback (and mints a new token). Installs the low-level keyboard hook on first use (idempotent). Raises an error for an unknown spec. **Returns a token** to pass to [`host.keys.release`](#host-keys-release); keep it if you'll release this specific capture (two overlays in one module can each capture the same key, so releasing by spec would be ambiguous).

```luau
host.keys.capture("Tab", function(mods)
  -- Tab is now swallowed app-wide (or in the scoped window); move overlay focus
  moveFocus(mods.shift and -1 or 1)
end)
host.keys.capture("Shift+Tab", function() moveFocus(-1) end)
```

### Windows

A low-level keyboard hook. It needs no permission and installs essentially always.

While a **screen reader's own modifier** is physically down — Insert, numpad zero or Caps Lock — a matched key is passed through instead of swallowed. None of those is a Windows modifier, so without that rule NVDA+Space would arrive as a bare Space and be eaten by any overlay claiming `"Space"`.

### macOS

This call **can raise**. Without the Accessibility grant the event tap is refused and the error reaches Lua. Worse is the case where only Input Monitoring is missing: the tap is created, reports itself as enabled, and never fires — `capture` returns a token, no key ever arrives, and nothing errors. A module that announces "overlay ready" on a successful return can be announcing it into a session where no key will ever reach it. (Only the first capture in a process can raise; later ones reuse the installed tap.)

There is **no screen-reader pass-through rule**. Caps Lock contributes no modifier bit at all, so with VoiceOver's modifier set to Caps Lock, VO+Space arrives as a bare Space, mask 0, and an overlay claiming Space will capture and suppress it. With the default Ctrl+Option the mask is non-zero and a bare-key capture does not match, so this bites only on the Caps Lock setting.

## host.keys.release(token) {#host-keys-release}

**Signature:** `host.keys.release(token: number)` → `nil`

Undoes the exact capture identified by the `token` [`host.keys.capture`](#host-keys-capture) returned, recomputing the global captured set so the key reaches apps normally again (unless another capture still holds it). Keying on the token — not `(vk, mask, module)` — means one overlay releasing a key cannot drop another overlay in the same module that has since re-captured it. An unknown/stale token (already superseded by a later capture of the same key) is a harmless no-op.

```luau
local tok = host.keys.capture("Tab", onTab)
-- later:
host.keys.release(tok)
```

## host.keys.releaseAll() {#host-keys-releaseall}

**Signature:** `host.keys.releaseAll()` → `nil`

Removes **all** key captures owned by this module and refreshes the suppression set. Useful when tearing down an overlay.

```luau
host.keys.releaseAll()
```

## host.keys.scope(toForeground) {#host-keys-scope}

**Signature:** `host.keys.scope(toForeground: boolean)` → `nil`

Scopes captured-key suppression. `true` pins it to the **current foreground window** (captured via `GetForegroundWindow` at call time) — the hook then only intercepts keys while that window is foreground, so a menu/popup that brings another window forward receives keys natively (ReaHotkey's `HotIf WinActive` model). `false` makes capture global again.

```luau
host.keys.scope(true)   -- only suppress while the plugin window is focused
```

### Windows

The scope is a window handle, and at press time the hook compares it against a **live** query for what is in front. It is always current.

### macOS

The comparison is against a value **cached from notifications** — application activated, focused window changed. A window that merely *opens* inside an already-frontmost application raises neither, so the pin can disagree with what the tap believes is in front. Setting the scope re-asks once and stores the fresh answer, which repairs the common case; it goes unresponsive only when that second resolve disagrees too, and then Tab does nothing until the user switches away and back.

## host.keys.menuOpen(open) {#host-keys-menuopen}

**Signature:** `host.keys.menuOpen(open: boolean)` → `nil`

Tells the hook a plugin's own (Qt/UIA) menu is open (`true`) or closed (`false`). While open, captured navigation keys (e.g. `Tab`/`Enter`) **pass through** to that menu instead of being consumed by the overlay — covering plugin menus the Win32 menu-state check can't see.

```luau
host.keys.menuOpen(true)
-- ... user navigates the plugin's native menu ...
host.keys.menuOpen(false)
```

---

## host.timer.after(ms, callback) {#host-timer-after}

**Signature:** `host.timer.after(ms: number, callback: () -> ())` → `nil`

Schedules a **one-shot** callback to fire approximately `ms` milliseconds later, driven from the event-loop tick. The callback runs once with no arguments (only if the module is still enabled at fire time) and is then discarded. There is no returned handle and no way to cancel an individual timer.

```luau
host.timer.after(500, function()
  host.speech.output("Half a second later")
end)
```

---

## host.timer.every(ms, callback) {#host-timer-every}

**Signature:** `host.timer.every(ms: number, callback: () -> ())` → `nil`

Schedules a **recurring** callback to fire approximately every `ms` milliseconds, driven from the event-loop tick. Unlike a self-rescheduling `host.timer.after` chain, a recurring timer is **re-armed even while the owning module is disabled** (the callback is only *invoked* while enabled), so a poll resumes on re-enable instead of dying. There is no returned handle or per-timer cancel; it is released when the module is reloaded/unloaded. Use it for polling that must survive a disable/enable cycle — e.g. an overlay watching for a landmark to appear.

```luau
host.timer.every(150, function()
  -- e.g. re-check whether a library landmark is on screen
end)
```

---

## host.epoch() {#host-epoch}

**Signature:** `host.epoch() -> number`

A counter that changes whenever the world may have: an OS event dispatched into a module (hotkey, key, window activation, focus change), a timer firing, an async image result arriving, or the module itself driving input (click, move, drag, scroll, key send, typing).

Memoize an expensive observation against it, so repeats within one dispatch are free while a genuinely new situation is always re-observed:

```luau
local cachedEpoch, cached = -1, nil
local function expensiveThing()
    local e = host.epoch()
    if e ~= cachedEpoch then
        cached, cachedEpoch = reallyWorkItOut(), e
    end
    return cached
end
```

Deliberately **not** time-based, and it does not advance on an idle tick. A stale answer here means acting on the wrong screen position, and "it was fresh 50 ms ago" is not a safety property.

## host.log.info(msg) {#host-log-info}

**Signature:** `host.log.info(msg: string)` → `nil`

Writes `msg` to the host log under the `module` channel. This is the **only** method on `host.log` — there is no `warn`/`error`/`debug`.

```luau
host.log.info("module initialized")
```

