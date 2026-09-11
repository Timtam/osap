---
title: "host.keys — capturing keys from the application"
sidebar_position: 7
toc_max_heading_level: 2
---

Claims a key so that the focused application does not receive it. Where a hotkey fires wherever the user happens to be, a capture **takes the key away**: while a capture holds, the keystroke arrives at your callback and the focused plug-in never sees it.

That is what makes an overlay navigable — Tab and Shift+Tab walk the control ring, Space and Return activate what is focused, Left and Right belong to a focused slider or tab control — and it is why the discipline is to hold only what the control in focus actually needs and hand everything else straight back. A key held is a key the plug-in does not get, and from the outside that is indistinguishable from the plug-in ignoring it: a tab control that claimed the arrow keys statically cost Melodyne's editor its arrows entirely, and Space in Melodyne is transport play and stop.

Matching is exact on the modifier state, so a bare key never fires for its modified form and the two Tab directions are two separate captures. Suppression can be pinned to the window that was in front when the scope was set, and a plug-in's own menu can be declared open so navigation keys fall through to it — both exist so that a menu coming forward is driven by the operating system instead of being eaten by the overlay.

On macOS the hook is an event tap, and the failure is asymmetric: without the Accessibility grant the call **raises**, while with Input Monitoring missing it returns a token, reports itself enabled, and never delivers a key.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["keys"]
```

See [what that list is and is not](./index.md#capabilities).

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

## host.keys.modifiersDown() {#host-keys-modifiersdown}

**Signature:** `host.keys.modifiersDown()` → `boolean`

True while any of Ctrl, Alt, Shift or Win — Control, Option, Shift or Command on macOS — is physically held. The reason it exists is that a hotkey callback runs *while its own combination is still down*: you are still holding Alt when the handler for Alt+V starts, so anything the handler synthesises inherits those modifiers. `host.input.send("F10")` arrives at the plug-in as Alt+F10, and a synthesised click arrives as Alt+click, which is a different gesture entirely in most UIs. Ask this before synthesising, and defer until it answers false. It costs nothing worth counting — the hardware keyboard state, no screen touch and no accessibility query — which is why the runtime is willing to poll it every 15 ms.

Do not spin waiting for it: the thread that would block is the one carrying speech and the keyboard hook. Poll with `host.timer.after` and **bound the wait**, acting anyway when the bound is reached, or a key the user happens to hold unusually long means the control silently never fires. Caps Lock is not one of these modifiers on either platform — it is a latch rather than something held — so a screen reader whose modifier is Caps Lock does not keep this true. Overlay controls activated through the runtime's own hotkeys already wait; this call is for modules that register their own hotkeys or drive `host.input` directly.

```luau
-- modules/overlay-runtime/src/main.luau: a hotkey callback runs while its own combination
-- is still held, so the F10 it sends arrives as Alt+F10 and toggles nothing.
local function afterModifiersReleased(fn, tries)
  if not host.keys.modifiersDown() then return fn() end
  tries = (tries or 0) + 1
  -- 40 x 15 ms ≈ 600 ms, then act ANYWAY: a long hold must not mean the control never fires.
  if tries > 40 then return fn() end
  host.timer.after(15, function() afterModifiersReleased(fn, tries) end)
end
host.hotkey.register("Alt+V", function()
  afterModifiersReleased(function() host.input.send("F10") end)
end)
```

## host.keys.nativeMenuOpen() {#host-keys-nativemenuopen}

**Signature:** `host.keys.nativeMenuOpen()` → `boolean`

True while the application in front has a menu open that the operating system itself drew. This is the **cheap half** of "is a menu open": no accessibility traversal, no screen touch, cheap enough to ask from inside the key path and from a 150 ms timer. The expensive half — `host.element.find(hwnd, "", host.element.type.Menu)`, which walks a plug-in's entire accessibility tree across a process boundary — was measured at 50–194 ms per call, more than its own 150 ms interval, and was being spent almost entirely on answering "no". Ask this first and the common case is settled outright, because the menus these overlays open (u-he's preset menu, Komplete Kontrol's menu bar) turn out to be native ones. It is also the guard a module wants around anything that reads the screen on a timer: a menu is drawn *over* the region, so a read-out watcher that keeps going reports the menu's own text as a changed value, over and over, as the user moves through it.

A false answer means "no menu the OS drew", not "no menu". A plug-in that paints its own menu inside its window — a Qt menu, typically — is invisible here; that case is what [`host.keys.menuOpen`](#host-keys-menuopen) is for, and the runtime's `Overlay:watchMenus` already drives it for you if you attach with `menus = true`. Note also that you do not need this call to get key pass-through while a native menu is up: the key hook consults the same answer itself on both platforms and stops suppressing captured keys for the duration. Modules call it to quiet their *own* polling and reading.

```luau
-- modules/overlay-runtime/src/main.luau, Overlay:watchMenus — the cheap question first.
local tick = 0
host.timer.every(150, function()
  if not ov.active then return end
  -- Every menu these overlays open turns out to be native (u-he's preset menu, KK's menu bar).
  local open = host.keys.nativeMenuOpen() == true
  -- The tree walk costs 50-194 ms, so it is paid on a slow backstop, not every tick.
  tick += 1
  if not open and tick % 8 == 0 then
    local hwnd = ov:hwnd()
    open = hwnd ~= nil and host.element.find(hwnd, "", host.element.type.Menu) == true
  end
  host.keys.menuOpen(open)
end)
```

### Windows

A live question, asked fresh on every call: `GetGUIThreadInfo` for the foreground thread, true when that thread is in menu mode, popup-menu mode or system-menu mode. So it covers real Win32 menus — a `#32768` popup, the menu bar, a window's system menu — and it is never stale, because nothing is cached. A menu open in some other program does not count: only the thread that owns the foreground window is inspected.

### macOS

Nothing is asked at call time. The answer is a counter kept by the accessibility observer from the frontmost application's `AXMenuOpened` / `AXMenuClosed` notifications, so the common case — nothing open — is one relaxed load. It **counts rather than latches**, so a submenu opening and closing again does not clear its parent, and notifications from any application that is not frontmost are discarded — an unrelated program with a menu up must never disarm the overlay. The count is also cleared outright when a different application comes to the front, because whatever menu was believed open belonged to the application just left; switching away from an app with a menu up therefore re-arms at once rather than waiting on the valve below.

Because the answer depends on a close notification arriving, there is a safety valve: a depth that has not moved for 60 seconds is treated as closed and the overlay re-arms, writing a line to the log that says so. The failure it guards against is invisible from the outside — a stuck flag hands every captured key to the application underneath and the overlay simply stops answering. An application that draws a menu without posting either notification reads here as no menu, which is the same shape of gap as a self-drawn Qt menu on Windows and has the same answer: `host.keys.menuOpen`.

---

## host.keys.passedThrough() {#host-keys-passedthrough}

**Signature:** `host.keys.passedThrough()` → `{ { vk: number, mask: number, key: string } }`

The keys the hook let through to the application because a menu was open, since the last call — drained on read, so each key is reported once. Two kinds: any **captured** key let past because a menu was open, and **Return or Escape, captured or not**, whenever `host.keys.menuOpen(true)` is in force — those two end a menu, no overlay captures Escape, and Return is captured only while the focused control wants it, so a record kept for captured keys alone would never hold the Escape that cancelled a menu. `key` is the spelling `host.keys.capture` would accept (`"Return"`, `"Escape"`, `"Tab"`, `"A"`, `"F5"`), or `"vk 0x.."` for a key the spec grammar cannot name.

What it is for: **Return and Escape end a menu.** Where nothing can see a plugin's menu — no notification, no menu element, no window of its own — the overlay runtime's hold is a stopwatch, and the only word it can get that the menu has closed is one of those two keys going through to it. The menu watch asks this on its tick and cuts the hold to a short grace when it finds one, instead of leaving Tab and Return with the plugin for the rest of the stopwatch. Only for a hold no detector has confirmed: where a detector can see the menu, its word is better than a guess about what a key did.

```luau
for _, k in ipairs(host.keys.passedThrough()) do
  if k.key == "Return" or k.key == "Escape" then
    host.log.info(k.key .. " reached the menu, which is therefore closing")
  end
end
```

### Windows

Recorded by the low-level keyboard hook, on its own thread: a captured key it let past because the foreground thread was in menu mode or `host.keys.menuOpen(true)` was in force, and an unmodified Return or Escape whenever `menuOpen(true)` is in force and the scoped window is foreground. Only key-down events; the matching key-up is not reported. Noted only — nothing here is suppressed that was not already.

### macOS

Recorded by the event tap: a captured key it let past because a native menu was open or `host.keys.menuOpen(true)` was in force, and an unmodified Return or Escape whenever `menuOpen(true)` is in force. Key-down only. A key that never reached the tap at all — VoiceOver's own chords, for instance — is not in here, because the tap never saw it.
