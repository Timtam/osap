---
title: "host.keys — capturing keys from the application"
sidebar_position: 7
toc_max_heading_level: 2
---

Claims a key so that the focused application does not receive it. Where a hotkey fires wherever the user happens to be, a capture **takes the key away**: while a capture holds, the keystroke arrives at your callback and the focused plug-in never sees it.

**There is no listen-only mode.** A captured key never reaches the application, except the `"<modifier> tap"` form below, which only watches. Sending the key on yourself does not help: [`host.input.send`](./input.md#host-input-send) goes through the same hook as a real key and is caught by your own capture again, and [`host.input.post`](./input.md#host-input-post) reaches the window's message queue only, which a game reading DirectInput or Raw Input never looks at. For a game played with a controller, [`host.gamepad`](./gamepad.md) is the observer: it sees every press and takes none.

**A key goes to one callback.** The captured set is shared by the whole application: while any enabled module captures a combination, the hook suppresses it, and the press is handed to exactly one callback — the earliest capture of that combination still standing among enabled modules. (A module that captures a combination again replaces its own earlier capture and moves to the back of that order.) Nothing routes a key by which window is in front, and a second module's capture of the same combination is neither dispatched to nor reported as a conflict: it simply never fires while the first one holds. With trace logging on, the log's `captured set:` line names every module holding each key.

That is what makes an overlay navigable — Tab and Shift+Tab walk the control ring, Space and Return activate what is focused, Left and Right belong to a focused slider or tab control — and it is why the discipline is to hold only what the control in focus actually needs and hand everything else straight back. A key held is a key the plug-in does not get, and from the outside that is indistinguishable from the plug-in ignoring it: a tab control that claimed the arrow keys statically cost Melodyne's editor its arrows entirely, and Space in Melodyne is transport play and stop.

Matching is exact on the modifier state, so a bare key never fires for its modified form and the two Tab directions are two separate captures. Whether a held key fires the callback again is platform-specific: see [`capture`](#host-keys-capture). Suppression can be pinned to the window that was in front when the scope was set, and a plug-in's own menu can be declared open so navigation keys fall through to it — both exist so that a menu coming forward is driven by the operating system instead of being eaten by the overlay. Both are **switches for the whole application**, not for your module: see [`scope`](#host-keys-scope).

On macOS the hook is an event tap, and the failure is asymmetric: without the Accessibility grant the call **raises**, while with Input Monitoring missing it returns a token, reports itself enabled, and never delivers a key.

This page also holds the [key spec](#key-spec-string-format) every call that takes a key reads, and three calls that work on a spec rather than on the keyboard: [`normalize`](#host-keys-normalize) says which key it is, [`describe`](#host-keys-describe) says it in the platform's words, and [`check`](#host-keys-check) says what the platform does with it.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["keys"]
```

`host.keys.normalize`, `describe` and `check` are the exception: they need no declaration, so a module that only registers a hotkey can still say its key. Everything else on this page needs `keys`.

See [what that list is and is not](./index.md#capabilities).

## Key spec string format {#key-spec-string-format}

One grammar, read by one parser, for every call that takes a key: [`host.keys.capture`](#host-keys-capture), [`host.hotkey.register`](./hotkey.md#host-hotkey-register), [`host.input.send`](./input.md#host-input-send), and [`normalize`](#host-keys-normalize), [`describe`](#host-keys-describe) and [`check`](#host-keys-check) below. A spec is `+`-joined segments, trimmed and case-insensitive; the last segment is the key and every earlier one a modifier. [`host.input.post`](./input.md#host-input-post) takes one key name and no modifiers.

**Modifiers are roles**, the way Qt names them: `Ctrl`, `Alt`, `Shift` and `Win`. On Windows and Linux each is the key of that name. On a Mac `Ctrl` is **Command**, `Alt` is Option, `Shift` is Shift and `Win` is **Control**, as Qt's `ControlModifier` is Command there and its `MetaModifier` Control. So one spec lands on each platform's counterpart: `"Ctrl+C"` copies on both, and `host.input.send("Ctrl+S")` is the application's Save on both.

| Role | Also written | Windows | Linux | macOS |
|---|---|---|---|---|
| `Ctrl` | `Control`, `Cmd`, `Command` | Control | Control | **Command** |
| `Alt` | `Option` | Alt | Alt | Option |
| `Shift` | | Shift | Shift | Shift |
| `Win` | `Super`, `Meta` | Windows | Super | **Control** |

**On a Mac the Control key is written `Meta`** (or `Win`). `Control` is a spelling of the Ctrl role, so on a Mac it means Command, not the key labelled Control. A spelling means the same role on every platform: `"Cmd+S"` is Ctrl+S on Windows, and `"Meta+E"` is Win+E there. VoiceOver's Control+Option layer is `Meta+Alt` in a spec. A spec with Ctrl+Alt and no Win in it is Command+Option on a Mac, which is off that layer. Everything follows the roles: hotkeys, captures and the `mods` a capture callback gets, [`host.input.send`](./input.md#host-input-send), taps, and `normalize`, `describe` and `check` below.

Naming a role twice in one spelling family is harmless: `"Ctrl+Control+S"` is Ctrl+S. Spelling the Ctrl role both as `Ctrl`/`Control` and as `Cmd`/`Command` in one spec is a parse error, because to a Mac author `"Control+Command+F"` is two keys and the roles would make it one, Command+F. The error names the two words and says the Mac's Control key is `Meta`, so the spec for Control+Command+F is `"Meta+Cmd+F"`. No word stands for a combination. Which chord a key uses is the module's choice.

**Write a key once, or pick one per platform.** A key written once is each platform's counterpart: Ctrl+Shift+F9 on Windows is Command+Shift+F9 on a Mac. When a module wants a different combination on a Mac, it picks one per platform with [`host.os.pick`](./os.md#host-os-pick). That happens when the counterpart is taken there: Command+Tab is the application switcher, and Command+H, +M and +Q hide, minimise and quit (all of them refused as hotkeys, see [`check`](#host-keys-check)). It also happens when a spec lands on VoiceOver's layer, as the four-modifier chord does. The host's own reload key is picked per platform for that reason: Ctrl+Shift+Win+Alt+F5 on Windows, `Cmd+Shift+F5` on a Mac.

```luau
-- The overlay runtime's next-tab key is Control+Tab on both platforms. Written once, a Mac's
-- would be Command+Tab, the application switcher.
local NEXT_TAB = host.os.pick { windows = "Ctrl+Tab", macos = "Meta+Tab", linux = "Ctrl+Tab" }
```

`pick` is also how a module follows a difference in the other program. Kontakt's Info Pane is F9 on Windows and Command+I on a Mac because Kontakt made it so, and a module that presses it has to follow.

A key that has to be free system-wide is best a function key with modifiers. With a letter, the four modifiers are not free on Windows. They are the Office key, and the shell holds the Office key's letters (W, X, P, O, T, Y, N, D and L) system-wide, so registering one ends in the "Binding unavailable" dialog. On a Mac, Command+Shift with a letter is the application-menu layer (Shift+Command+Z is Redo in most applications), and a hotkey there takes the key from every application for as long as it is held.

**Key names** (`key_to_vk`): single ASCII letter `A`–`Z` (case-insensitive) or digit `0`–`9`; function keys `F1`–`F24`; and the named keys `Space`, `Enter`/`Return`, `Esc`/`Escape`, `Tab`, `Backspace`, `Delete`/`Del`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`. That is the whole list, here and for [`host.input.send`](./input.md#host-input-send) and [`post`](./input.md#host-input-post): there are no numpad keys, no punctuation, no `Insert` and no media keys. An unknown modifier or key makes `host.keys.capture`, `host.hotkey.register` and `host.input.send` raise an error naming the part it could not read; `normalize`, `describe` and `check` answer `nil` or `"parse"` instead.

Examples: `"Ctrl+Alt+P"`, `"Tab"`, `"Shift+Tab"`, `"Ctrl+S"`, `"Ctrl+Shift+F9"`, `"Meta+Tab"`, `"F5"`.

---

There is a third form: **`"<modifier> tap"`** — a modifier pressed and released on its own, with no other key in between. The modifier is any spelling of a role (`Alt`/`Option`, `Ctrl`/`Control`/`Cmd`/`Command`, `Shift`, `Win`/`Super`/`Meta`), so on a Mac `"Ctrl tap"` is a tap of Command and `"Win tap"` a tap of Control. The suffix is written ` tap` or ` Tap` — `"Alt TAP"` is not a tap and raises as an unknown key. It is a `host.keys` capture only, never a global hotkey and never a key to send (see [`host.hotkey.register`](./hotkey.md#host-hotkey-register) for what happens if you try). The parser accepts any key name before ` tap`, but only the modifiers are ever seen as taps: `"Space tap"` returns a token and never fires.

```luau
-- A global hotkey, written once: Ctrl+Shift+F9 on Windows, Command+Shift+F9 on a Mac, which
-- is off Control+Option, VoiceOver's layer.
host.hotkey.register("Ctrl+Shift+F9", writeProbeLog)

-- host.keys matches the modifier state EXACTLY, so the two directions are two captures:
-- "Tab" (mask 0) never fires for Shift+Tab, and never for Alt+Tab either.
host.keys.capture("Tab", function() ov:focusNext() end)
host.keys.capture("Shift+Tab", function() ov:focusPrev() end)

-- The tap form. Never suppressed: the modifier still reaches the application.
host.keys.capture("Alt tap", function() ov:activate() end)
```

**Screen readers own some combinations.** VoiceOver's is structural — every chord that holds Control and Option together, `Meta+Alt` in a spec — and [`check`](#host-keys-check) says so. JAWS and NVDA take keys of their own as well, and both can be extended with scripts and add-ons, so no list here could say which: `check` does not try. When a key does not arrive while the screen reader runs, the answer is another key.

### Windows

Each role is the key of its name: `Ctrl` (and `Control`, `Cmd`, `Command`) is Control, `Win` (and `Super`, `Meta`) is the Windows key. Letters and digits are virtual keys, which Windows assigns by the current keyboard layout, so `"Z"` is the key labelled Z on a German keyboard as on a US one.

The modifier state is read from the generic Ctrl and Alt keys, and **AltGr is Ctrl+Alt** to Windows. So a capture of `"Ctrl+Alt+Q"` also matches, and swallows, AltGr+Q — the `@` of a German layout — and `"Ctrl+Alt+E"` takes the `€`. A hotkey registered on the same combination matches AltGr the same way, everywhere, for as long as it is held. [`check`](#host-keys-check) reports `"altgr"` with the character, for the layout in use when it is asked. A chord with Win in it is not AltGr, so the four-modifier chord is not affected.

Only the matched key is suppressed; the modifiers themselves still reach the application. After a captured `"Alt+X"` the application has seen Alt go down and up with nothing in between, which is what a bare Alt press looks like; a standard Win32 window answers that by activating its menu bar, and a bare Win key opens Start. Nothing hides it for a capture: the masking key AutoHotkey sends around its hook hotkeys is sent here only for a [hotkey](./hotkey.md#host-hotkey-register) the hook matches. It has not been observed for a capture in this project, but a capture of Alt or Win plus a key is where to look if the focus jumps to a menu bar.

### macOS

`Ctrl` is Command, `Alt` is Option, `Shift` is Shift and `Win` (`Meta`) is Control. A spec written for Windows therefore lands on the Mac's counterpart. Kontakt's `"Ctrl+L"` is Command+L, and its `"Alt+E"` is Option+E. Option matters most here, because on macOS it also composes characters: claiming `Alt+E` or `Alt+N` takes the acute and tilde dead keys away from any text field while the overlay is up ([`check`](#host-keys-check) reports `"composes"`). The Carbon registration, the event tap's match and the `mods` table of a capture callback follow the roles as well: `mods.ctrl` is Command held and `mods.win` is Control held.

**Control and Option together are VoiceOver's modifier.** With VoiceOver's default setting every chord that holds both is VoiceOver's before any application sees it: registered, never delivered, and VoiceOver's error sound on each press (measured on the third Mac session). `check` reports `"voiceover"` for all of them. In a spec the pair is `Meta+Alt` (or `Win+Alt`), so the four-modifier chord is on it. A spec that holds Ctrl+Alt without Win is Command+Option here and is not.

**Letters follow the keyboard layout**, as on Windows: a letter is the key that types it under the layout the user has selected, for `host.keys`, `host.hotkey`, `host.input.send` and `host.input.post` alike, and a key the event tap reports is named the same way. On a German (QWERTZ) Mac `"Ctrl+Z"` (Command+Z) is the key labelled Z, so `host.input.send("Ctrl+Z")` undoes, and on a French (AZERTY) one `"Ctrl+A"` is the key labelled A. A combination that holds Command takes its letter from what the layout types with Command held, which differs only for layouts built that way: "Dvorak – QWERTY ⌘" types QWERTY while Command is down, so there `"Ctrl+Z"` is the QWERTY Z key, where that layout's own Command shortcuts are, and `"Meta+Z"` (Control+Z) is Dvorak's. The host asks the layout which key types each letter (`UCKeyTranslate`, 48 keys, without and with Command, on the main thread) once at start and again whenever the selected input source changes, and writes a `keyboard letters …` line to the log at start and whenever a letter moved, naming the layout and every letter that is not on its US position. A layout that types no Latin letters — Russian, Greek — gives way to the ASCII-capable layout the system offers for shortcuts. A letter no layout types falls back to its US position, unless that key types another letter, and then it has no key: `send` and `post` raise, a capture of it fires only once a layout that types it is selected, and a [hotkey](./hotkey.md#macos) on it is accepted but not held until then — the log says so, and it is registered on that layout's key when the user selects it. [`check`](#host-keys-check) does not look at the letters of the current layout, so `no-keycode` is never said for a letter.

Everything that is not a letter is a **position on a US keyboard**: digits, function keys and the named keys. On a French Mac `"Cmd+1"` is the key that types `&`, which is the key labelled 1 — the key Windows' French layout calls 1 as well. `F21`–`F24` have no macOS key code at all (`"no-keycode"`).

### Both

The tap form behaves the same on either platform: armed when a modifier goes down from rest, dropped by anything at all in between — an ordinary key, a second modifier, Caps Lock — and fired on the release. It is never suppressed, so the modifier keeps working as a modifier. All four modifiers are armed; a tap of any other key is accepted and can never fire.

## host.keys.normalize(spec) {#host-keys-normalize}

**Signature:** `host.keys.normalize(spec: string) -> string?`

The key `spec` stands for, in this platform's spelling. The modifiers come in the platform's order and words: Ctrl, Alt, Shift, Win on Windows and Linux, and Meta, Option, Shift, Cmd on macOS, which is Apple's order (Control, Option, Shift, Command). The key is written as the parser names it (`"Return"`, `"PageUp"`, `"A"`). Two specs are the same key exactly when their normalized forms are equal. The normalized form parses back to the same key on every platform, because a spelling names the same role everywhere. The overlay runtime compares its controls' hotkeys this way, so `"Cmd+S"` beside an inherited `"Ctrl+S"` is one claim, not two registrations of which only one could live.

Returns `nil` for a spec that does not parse; it raises only for an argument that is neither a string nor a number (a number is read as its digits, so `5` is the key 5). String work only — no system call, microseconds. Like every Lua call it runs on the main thread. **Needs no capability**: `normalize`, `describe` and `check` are reachable without declaring `keys`, which is about capturing keystrokes.

```luau
-- Controls that name one key two ways are one claim; keep the first.
local claimed = {}
local function claim(spec, label)
  local key = host.keys.normalize(spec) or spec
  if claimed[key] then
    host.log.info(label .. ": " .. spec .. " is the key '" .. claimed[key] .. "' already has")
    return false
  end
  claimed[key] = label
  return true
end
claim("Ctrl+S", "Save")
claim("Cmd+S", "Store") -- the same key everywhere: Ctrl+S on Windows, Command+S on a Mac
```

### Windows

`"Cmd+S"` and `"ctrl + s"` are both `"Ctrl+S"`; `"Ctrl+Shift+Win+Alt+F6"` is `"Ctrl+Alt+Shift+Win+F6"`; `"Option+P"` is `"Alt+P"`, and `"Meta+E"` is `"Win+E"`. The host names hotkeys in its log and in the "Binding unavailable" and "Binding conflict" dialogs in this form, so the reload key reads `Ctrl+Alt+Shift+Win+F5` there.

### macOS

`"Ctrl+S"` and `"Cmd+S"` are both `"Cmd+S"`; `"Win+X"` is `"Meta+X"`; `"Cmd+Shift+F6"` is `"Shift+Cmd+F6"`; `"Alt+P"` is `"Option+P"`. The log uses this form too, so the reload key reads `Shift+Cmd+F5` there. The "Binding unavailable" and "Binding conflict" dialogs say the key in [`describe`](#host-keys-describe)'s spoken words instead, `Shift+Command+F5`, because `Meta` is not what the Control key is called on a Mac. The result is spelled the Mac's way, and it names the same key on any platform: `"Cmd+S"` is Ctrl+S on Windows.

## host.keys.describe(spec, opts) {#host-keys-describe}

**Signature:** `host.keys.describe(spec: string, opts: { style: ("spoken" | "short")? }?) -> string?`

`spec` in this platform's own words, for telling a user which key to press. `"spoken"`, the default, spells every modifier out — `"Control+Alt+P"`, `"Shift+Command+F9"`, `"Up Arrow"` — and is what the overlay runtime says when a control with a hotkey is focused. `"short"` uses the written abbreviations — `"Ctrl+Alt+P"`, `"Shift+Cmd+F9"`, `"Up"` — for a log line or a label. A tap is `"Alt pressed on its own"` spoken and `"Alt tap"` short.

Returns `nil` for a spec that does not parse, and raises for a style other than the two, or for a `spec` that is neither a string nor a number. **A description is not a spec**: on a Mac `"Delete"` is the key the parser calls `Backspace`, so never feed one back into a call that takes a key. String work only, main thread, no capability needed (see [`normalize`](#host-keys-normalize)).

```luau
-- Written once, and said the way each platform's user knows it:
-- "Control+Shift+F6" on Windows, "Shift+Command+F6" on a Mac.
local KEY = "Ctrl+Shift+F6"
host.hotkey.register(KEY, backIntoPlugin)
host.speech.output("Press " .. host.keys.describe(KEY) .. " to return to the plug-in")
host.log.info("[mine] " .. host.keys.describe(KEY, { style = "short" }) .. " is registered")
```

### Windows

Spoken: Control, Alt, Shift, Windows; short: Ctrl, Alt, Shift, Win. Return is `"Enter"`, as the key is labelled; Backspace and Delete keep their names. `"Ctrl+Shift+Win+Alt+F6"` is `"Control+Alt+Shift+Windows+F6"` spoken, and `"Cmd+S"` is `"Control+S"`.

### macOS

Spoken: Control, Option, Shift, Command, in the order Apple prints them; short: Control, Option, Shift, Cmd. The Control key is `Control` in both styles, because `Ctrl` in a Mac's log would read as the spec spelling, which is Command. The roles are said as the keys they are here: `"Ctrl+Shift+F6"` is `"Shift+Command+F6"`, `"Meta+Tab"` is `"Control+Tab"`, `"Alt+V"` is `"Option+V"`, and `"Ctrl tap"` is `"Command pressed on its own"`. The two keys whose names swap on a Mac are said as the Mac labels them: Backspace is `"Delete"` and Delete is `"Forward Delete"`; Return is `"Return"`.

## host.keys.check(spec, opts) {#host-keys-check}

**Signature:** `host.keys.check(spec: string, opts: { layout: boolean? }?) -> { ok: boolean, resolved: string?, reasons: { string }, produces: string?, deadKey: boolean? }`

What this platform does with a combination, structurally. `resolved` is [`normalize`](#host-keys-normalize)'s answer (`nil` when it does not parse). `reasons` lists what stands in the way, and is empty for a key with nothing to say about it. `ok` is false when a reason means the key cannot work here — `parse`, `reserved`, `no-keycode` — or cannot on a Mac whose VoiceOver keeps its default modifier (`voiceover`, said whether or not VoiceOver runs). It stays true for a tap, which works as the capture it is, and for the two reasons that only cost the user a character. `produces` is that character, with `deadKey` true when it is a dead key waiting for the next keystroke.

| Reason | Where | Means | `ok` | [`host.hotkey.register`](./hotkey.md#host-hotkey-register) |
|---|---|---|---|---|
| `parse` | both | Not a key spec. | false | raises |
| `reserved` | both | The system keeps the combination for itself. | false | raises |
| `no-keycode` | macOS | `F21`–`F24`, which have no key code there. A capture of one never fires. | false | raises |
| `voiceover` | macOS | It holds Control and Option together, VoiceOver's modifier: `Meta+Alt` (or `Win+Alt`) in a spec. | false | registers, and never arrives while VoiceOver runs with its default modifier |
| `tap` | both | A modifier pressed on its own: a capture that watches, never a hotkey. | true | raises |
| `altgr` | Windows | Ctrl+Alt without Win, and the current layout types `produces` with it: AltGr. Holding it takes that character from the user. | true | registers |
| `composes` | macOS | Option (and Shift) without Control or Command, which is `Alt` without `Win` or `Ctrl`, and the current layout types `produces` with it. | true | registers |

The reasons are structural on purpose: they are what the platform does, never what another program happens to hold. JAWS and NVDA take keys of their own and both are extensible, so a key `check` passes can still be a screen reader's; see the note under [the key spec](#key-spec-string-format). [`host.keys.capture`](#host-keys-capture) raises only for `parse`.

`opts.layout = false` leaves the keyboard layout unasked, so `altgr` and `composes` are never reported and nothing but the string is read. The overlay runtime asks that way, at every bind and every hotkey sync, because it needs only the reasons `register` raises for. Default `true`.

Cost: string work, plus — only for the shapes that can be `altgr` or `composes`, and only with `layout` left on — one question to the keyboard layout. Main thread, like every Lua call. It raises for an argument that is neither a string nor a number, and for an `opts` that is not a table. No capability needed (see [`normalize`](#host-keys-normalize)); unlike `normalize` and `describe` it reads something besides the string, the character the current layout types with the chord.

```luau
-- Before announcing a key a user will press.
local KEY = "Ctrl+Alt+Q"
local c = host.keys.check(KEY)
if not c.ok then
  host.log.info(KEY .. " will not work here: " .. table.concat(c.reasons, ", "))
elseif c.produces then
  host.log.info(KEY .. " also types " .. c.produces .. " in this layout; holding it takes that away")
end
```

### Windows

`reserved` is exactly two combinations, from the `RegisterHotKey` documentation: `F12` with no modifiers (kept for the debugger, even when none is running) and `Win+L` (locks the computer). The Office key's letters (the four modifiers with W, X, P, O, T, Y, N, D or L) are not among them: the shell holds those, where they have not been removed, so they are refused at the claim, as a key another application holds is. `altgr` is asked with `ToUnicodeEx` against the keyboard layout of the thread that owns the foreground window — the one the user types into — at the moment of the call, with a handful of system calls and no screen touch; a layout switched afterwards is not reflected, and on a layout without AltGr (US) nothing is reported. Asking leaves the keyboard's dead-key state alone (Windows 10 1607 and later).

### macOS

`reserved`: Command+F5 (VoiceOver on and off) and Option+Command+F5 (Accessibility Shortcuts), Command+Tab and Shift+Command+Tab, Command+Space, Command+H, Command+M, Command+Q, Shift+Command+Q, Control+Command+Q, Option+Command+Escape, and Shift+Command+3, 4 and 5. Command is `Ctrl` in a spec, so these are `"Ctrl+F5"`, `"Ctrl+Tab"`, `"Ctrl+Q"` and so on, and Control+Command+Q is `"Ctrl+Win+Q"`. `"Win+Q"` and `"Meta+Tab"` (Control+Q, Control+Tab) are free. `composes` is asked of `UCKeyTranslate` with the current keyboard layout, or the current ASCII-capable one for an input method that has no layout of its own — the same read of the layout the [letters](#macos) come from, taken afresh for every such question (the 48 keys, without and with Command), so the answer is for the layout selected at the moment of the call even when the system's notice of a switch has not arrived yet, and the letters are brought up to date with it. The key asked about is the one a hotkey on the spec is registered on: on a German Mac `"Alt+Z"` is Option with the key labelled Z.

## host.keys.capture(spec, callback) {#host-keys-capture}

**Signature:** `host.keys.capture(spec: string, callback: (mods: { shift: boolean, ctrl: boolean, alt: boolean, win: boolean }) -> ())` → `number` (a release token)

Begins intercepting the [key spec](#key-spec-string-format): the keypress is swallowed (not passed to the underlying app) and `callback` is invoked with a `mods` table describing the modifier state at press time. Its fields are the roles, so on a Mac `mods.ctrl` is Command held and `mods.win` is Control held. The callback runs on the next pump tick, for a key-down; the key-up is never reported. What auto-repeat does, and whether the key-up reaches the application, are in the platform sections. Re-capturing the same `(vk, mask)` for this module replaces the previous callback (and mints a new token) — a module holds one capture per combination. Across modules the earliest standing capture gets the key (see the top of this page). Installs the low-level keyboard hook on first use (idempotent). Raises an error for an unknown spec. **Returns a token** to pass to [`host.keys.release`](#host-keys-release); keep it if you'll release this specific capture: when two overlays in one module capture the same key one after the other, the later capture has replaced the earlier, and the earlier overlay's token no longer releases anything.

```luau
host.keys.capture("Tab", function(mods)
  -- Tab is now swallowed app-wide (or in the scoped window); move overlay focus
  moveFocus(mods.shift and -1 or 1)
end)
host.keys.capture("Shift+Tab", function() moveFocus(-1) end)
```

### Windows

A low-level keyboard hook. It needs no permission and installs essentially always. It runs on a thread of its own that does nothing else, and stays installed for the rest of the session. Every keystroke on the machine passes through it, and matching costs microseconds whatever module code is running: a captured key is swallowed at once and queued, and its callback runs when the main thread is free, late by as long as that thread was busy — a long callback delays the overlay, not the user's typing, and does not let a captured key through to the application.

A capture fires on **every** key-down, auto-repeat included: holding an arrow runs the callback at the keyboard's repeat rate, and with it any screen read inside.

The key-up is not tied to its key-down: the hook matches it again on its own, against the modifiers held, the scope and the menu state at the moment of release, and swallows it only if that matches too. So after a captured `"Ctrl+Right"`, letting go of Ctrl before Right hands the application a bare Right key-up; the same happens when the scoped window lost the foreground, or a menu opened or was declared open, while the key was down.

While a **screen reader's own modifier** is physically down — Insert, numpad zero or Caps Lock — a matched key is passed through instead of swallowed. None of those is a Windows modifier, so without that rule NVDA+Space would arrive as a bare Space and be eaten by any overlay claiming `"Space"`.

### macOS

This call **can raise**. Without the Accessibility grant the event tap is refused and the error reaches Lua. Worse is the case where only Input Monitoring is missing: the tap is created, reports itself as enabled, and never fires — `capture` returns a token, no key ever arrives, and nothing errors. A module that announces "overlay ready" on a successful return can be announcing it into a session where no key will ever reach it.

A refused tap is not remembered as installed, so **every** later capture tries again, and raises again, until the tap can be created. A capture that raised is nevertheless registered — the key is entered in the captured set before the tap is asked for — and its token is lost with the error: once a later capture installs the tap, that key is suppressed and handed to the callback passed to the `capture` call that raised. [`releaseAll`](#host-keys-releaseall), the same module capturing the same combination again, or a reload of the module replaces or removes it; disabling the module stops the suppression for as long as it is disabled.

A capture is **not refused for a combination the system keeps** (`reserved` in [`check`](#host-keys-check)); only a spec that does not parse raises. A spec written for Windows lands on the Mac's counterpart, so a capture of `"Ctrl+Q"`, `"Ctrl+H"`, `"Ctrl+Space"` or `"Ctrl+Tab"` asks the event tap for Command-Q, Command-H, Command-Space or Command-Tab, for as long as it holds (and, with a [scope](#host-keys-scope), while that window is in front). The log says so once per chord, with the reason the system keeps it. Give the Mac another key with [`host.os.pick`](./os.md#host-os-pick), as the overlay runtime does for its tab keys (Control-Tab, `"Meta+Tab"`).

There is **no screen-reader pass-through rule**. Caps Lock contributes no modifier bit at all, so with VoiceOver's modifier set to Caps Lock, VO+Space arrives as a bare Space, mask 0, and an overlay claiming Space will capture and suppress it. With the default Control+Option the mask is non-zero and a bare-key capture does not match, so this bites only on the Caps Lock setting.

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

**One switch for the whole application.** The scope, the [`menuOpen`](#host-keys-menuopen) flag and the [`passedThrough`](#host-keys-passedthrough) record are each a single value shared by every module, not a setting of yours: the last caller wins, so one module's `scope(false)` unpins every other module's captures, and a `scope(true)` pins them all to its window. Disabling a module does not undo what it set. The overlay runtime sets and clears them as its overlays activate and deactivate; a module that also drives `host.keys` directly shares them with it.

```luau
host.keys.scope(true)   -- only suppress while the plugin window is focused
```

### Windows

The scope is a window handle, and at press time the hook compares it against a **live** query for what is in front. It is always current.

### macOS

The comparison is against a value **cached from notifications** — application activated, focused window changed. A window that merely *opens* inside an already-frontmost application raises neither, so the pin can disagree with what the tap believes is in front. Setting the scope re-asks once and stores the fresh answer, which repairs the common case; it goes unresponsive only when that second resolve disagrees too, and then Tab does nothing until the user switches away and back.

## host.keys.menuOpen(open) {#host-keys-menuopen}

**Signature:** `host.keys.menuOpen(open: boolean)` → `nil`

Tells the hook a plugin's own (Qt/UIA) menu is open (`true`) or closed (`false`). While open, captured navigation keys (e.g. `Tab`/`Enter`) **pass through** to that menu instead of being consumed by the overlay — covering plugin menus the Win32 menu-state check can't see. Like [`scope`](#host-keys-scope), this is one flag for the whole application: `menuOpen(true)` lets **every** module's captured keys through until somebody sets it back.

```luau
host.keys.menuOpen(true)
-- ... user navigates the plugin's native menu ...
host.keys.menuOpen(false)
```

---

## host.keys.modifiersDown() {#host-keys-modifiersdown}

**Signature:** `host.keys.modifiersDown()` → `boolean`

True while any of the four modifiers is physically held: Ctrl, Alt, Shift or Win on Windows, Command, Option, Shift or Control on macOS. The reason it exists is that a hotkey callback runs *while its own combination is still down*: you are still holding Alt when the handler for Alt+V starts, so anything the handler synthesises inherits those modifiers. `host.input.send("F10")` arrives at the plug-in as Alt+F10, and a synthesised click arrives as Alt+click, which is a different gesture entirely in most UIs. Ask this before synthesising, and defer until it answers false. It costs nothing worth counting — the hardware keyboard state, no screen touch and no accessibility query — which is why the runtime is willing to poll it every 15 ms.

Do not spin waiting for it: the thread that would block is the one carrying speech and every callback — and on macOS the event tap. Poll with `host.timer.after` and **bound the wait**, acting anyway when the bound is reached, or a key the user happens to hold unusually long means the control silently never fires. Caps Lock is not one of these modifiers on either platform — it is a latch rather than something held — so a screen reader whose modifier is Caps Lock does not keep this true. Overlay controls activated through the runtime's own hotkeys already wait; this call is for modules that register their own hotkeys or drive `host.input` directly.

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

The keys the hook let through to the application because a menu was open, since the last call — drained on read, so each key is reported once, to whichever module asks first: the record is one for the whole application, and it holds at most 32 keys until it is read. Two kinds: any **captured** key let past because a menu was open, and **Return or Escape, captured or not**, whenever `host.keys.menuOpen(true)` is in force — those two end a menu, no overlay captures Escape, and Return is captured only while the focused control wants it, so a record kept for captured keys alone would never hold the Escape that cancelled a menu. `key` is the spelling `host.keys.capture` would accept (`"Return"`, `"Escape"`, `"Tab"`, `"A"`, `"F5"`), or `"vk 0x.."` for a key the spec grammar cannot name.

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
