---
title: "host.keys — capturing keys from the application"
sidebar_position: 7
toc_max_heading_level: 2
---

Claims a key so that the focused application does not receive it. Where a hotkey fires wherever the user happens to be, a capture **takes the key away**: while a capture holds, the keystroke arrives at your callback and the focused plug-in never sees it.

**There is no listen-only mode.** A captured key never reaches the application, except the `"<modifier> tap"` form below, which only watches, and a key whose busy module let go of the capture before the key's turn came, which then goes where it would have gone without that capture: to the next module that captures it in that window, or else to the application (below). Sending the key on yourself does not help: [`host.input.send`](./input.md#host-input-send) goes through the same hook as a real key and is caught by your own capture again, and [`host.input.post`](./input.md#host-input-post) reaches the window's message queue only, which a game reading DirectInput or Raw Input never looks at. For a game played with a controller, [`host.gamepad`](./gamepad.md) is the observer: it sees every press and takes none.

**A stopped module's keys go back at once.** When the application [stops a module](../module-runtime-and-lifecycle.md#limits) — a callback ran past 2 seconds of processor time or 10 seconds in all, or out of memory — its captures leave the suppression set as soon as that callback has returned, and the keys reach the application in front again. Keys it captured that were pressed while that callback held the application are dropped, with a `[keys]` line naming them, rather than delivered late to whichever module captures them next (Windows; on macOS the system lets keys through during such a stall — not checked on a Mac yet). A stall that ends without a stop drops nothing: the keys typed ahead arrive late, as below.

**A key goes to one callback.** While an enabled module captures a combination, the hook suppresses it, and the press is handed to exactly one callback: the earliest capture of that combination still standing among the enabled modules whose [scope](#host-keys-scope) is the window in front, or every window. (A module that captures a combination again replaces its own earlier capture and moves to the back of that order.) Which module gets the key is decided at the press, by the window in front then, and the key goes to that module's capture of it and to nobody else's: if the module no longer captures that combination when the key is delivered, or its scope has moved to another window, the key is dropped, and the log says so (`[<module>] Tab was dropped: its capture was released between the press and its delivery…`, at most once every 10 seconds per module); a capture of the combination the module released and made again in between takes the key. (A key that waited behind its busy module's handler is the exception, below.) A key whose module was disabled in between is dropped without a line. A second module's capture of the same combination in the same window is neither dispatched to nor reported as a conflict: it simply never fires while the first one holds. With trace logging on, the log's `captured set:` line names every module holding each key, in that order, each with its scope (`@0x…`) and `+menu` while it says a menu is open, and each key the hook or the tap took has a `dispatch` line when the main thread hands it to its module, with the window it was pressed in and how long it waited for the main thread since the hook or the tap queued it: `[keys] dispatch vk 0x09/m0 to com.example.synth, pressed in window 0x2a0c4e, after 0.3 ms in the queue`.

**While its module is busy** — one of its handlers [waits](../module-runtime-and-lifecycle.md#a-handler-waits) for a read — a key the hook took for the module waits behind that handler and runs once the module is free, in the order its events came. Up to 256 keys, hotkey presses and controller buttons wait; the next is dropped, with a `[keys]` line. A held key's repeats fold into one while the last of that key waiting is a repeat, so a hold keeps one step; two presses never fold. The key still goes to its module, and to no other while the module captures it: to the capture it was pressed for, or, when the module captured the same combination again meanwhile — an overlay that went and came back — to the new capture, if its scope is every window or the window that was in front at the press; the log says so, `[com.example.game] Tab, pressed while the module was busy, went to the module's current registration of it`. When its scope has moved to another window meanwhile, the key is dropped and said: `[com.example.game] Tab, pressed while the module was busy, was dropped: the module's scope moved to another window meanwhile`. Another module's captures and hotkeys are not held up: their keys run at once.

**A key its busy module let go of goes where it would have gone.** When the module released the capture while the key waited, and did not capture the combination again, the hook kept the key for nothing. When the key's turn comes, it goes where the hook would have sent it had the module never captured it.

**First to the next module that captures it in that window**: the earliest capture of the combination still standing among the enabled modules, other than the one that let it go, whose scope is the window the key was pressed in or every window — the rule above, for the window in front at the press, not the one in front now. It arrives in that module's mailbox as a key the hook took for that module: it runs at once while that module is free and waits behind its handler while it is busy, under that module's rules, whatever window is in front by then and whatever was typed meanwhile, and after that module's keys that ran or waited before it came. A menu is not asked about again, as for any key that waited. The log says so: `[com.example.game] Tab, pressed while the module was busy, went to com.example.synth, the next module that captures it in the window it was pressed in: its registration was released meanwhile`. When that module lets go of it as well before its turn, it goes on the same way, but never to a module that has let go of it already — the first one included, should it have captured the key again since — so a key is at one place at a time and moves on at most once per module.

**Then to the program in front.** When no other module captures it there, it is sent to the program in front as if no module had captured it, and so only where it would have arrived then: while the window it was pressed in is still in front, and before any key typed after it. The log says so: `[com.example.game] Tab, pressed while the module was busy, was passed on to the program in front: its registration was released meanwhile`. It is sent as it was pressed, the same key with the same modifiers: on the physical key it was pressed on, where the hook or the tap named one, so the numeric keypad's Enter is sent as the keypad's Enter; a modifier the press was made with that nobody holds now goes down before the key and up after it, one the user holds is left alone, and none is left down. A held key's press and its repeats, folded into one while they waited, are sent as two presses. It is not sent, and the line says why (`… was dropped: its registration was released meanwhile, and the window it was pressed in is no longer in front`), when:

- another window is in front by then, no window was known to be in front at the press, or — on a Mac — the frontmost application does not answer which window is in front now (`… and nobody could say which window is in front now`);
- the key itself is still held down;
- a modifier is held down that the press was made without, since sent then it would arrive as another combination; on Windows also a screen reader's own key — Insert, the numeric keypad's 0 or Caps Lock — since sent then it would reach the screen reader as one of its commands (`… and the screen reader's key is held down now, which the press was made without`);
- a key has reached the program since the press (`… and a key typed after it has reached the program first`): a key-down that is not a modifier's and that no capture or hotkey took — typed, let through for a menu or a screen reader, a held key's repeat, or sent by another program or by [`host.input.send`](./input.md#host-input-send), this module's own included. Sent after it, the key would act on what that key changed: a Delete on the line a Down moved to. Keys captured meanwhile, by any module, do not count, so several keys a busy module let go of still go, one after another, in the order they were pressed;
- a hotkey of this application holds the combination now — a module's registration that the system granted, its module enabled or not, or the application's own reload key (`… and a hotkey holds its combination now, which would take it instead of the program`): the system would hand the key sent to that hotkey, not to the program. A registration whose spec only the system reads cannot be compared, and does not stop a key;
- or the system does not take the keys sent, which the line says as well (see [`capture`](#host-keys-capture)'s platform sections).

The reasons are checked in this order, and the line gives the first that holds. The keys sent are marked, and the hook or the tap lets them through untouched: no capture takes them (a module that captures the combination in that window was given the key first, above), they are no press, hold or modifier tap of the user's, and they do not count as a key that reached the program; only their key-down ends a modifier tap the user has started, as the key would have. A modifier tap was never kept from the program, so nothing is sent for it (`… was not run: …`). These lines are said at most once every 10 seconds per module, the one for a key gone to another module and the one for a key sent each on a clock of its own. A key whose capture went before the key even reached its free module is dropped, as above, not sent.

That is what makes an overlay navigable — Tab and Shift+Tab walk the control ring, Space and Return activate what is focused, Left and Right belong to a focused slider or tab control — and it is why the discipline is to hold only what the control in focus actually needs and hand everything else straight back. A key held is a key the plug-in does not get, and from the outside that is indistinguishable from the plug-in ignoring it: a tab control that claimed the arrow keys statically cost Melodyne's editor its arrows entirely, and Space in Melodyne is transport play and stop.

Matching is exact on the modifier state, so a bare key never fires for its modified form and the two Tab directions are two separate captures. Whether a held key fires the callback again is platform-specific: see [`capture`](#host-keys-capture). Suppression can be pinned to the window that was in front when the scope was set, and a plug-in's own menu can be declared open so navigation keys fall through to it — both exist so that a menu coming forward is driven by the operating system instead of being eaten by the overlay. Both are **your module's own**, and apply to the keys it captures: see [`scope`](#host-keys-scope).

On macOS the hook is an event tap. Without the Accessibility grant the call **raises**. Whether the tap also needs Input Monitoring is not known yet (see [macOS permissions](../macos-permissions.md#input-monitoring)); if it does, the failure is the other way round: with Input Monitoring missing the call returns a token, the tap reports itself enabled, and no key is delivered.

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

Returns `nil` for a spec that does not parse. It raises for a `spec` that is neither a string nor a number, for an `opts` that is neither a table nor `nil`, and for a `style` other than the two: another string or a number (`host.keys.describe: style must be "spoken" or "short", …`), or a value that is not a string at all, such as `true` or a table. A `style` of `nil` is `"spoken"`. **A description is not a spec**: on a Mac `"Delete"` is the key the parser calls `Backspace`, so never feed one back into a call that takes a key. String work only, main thread, no capability needed (see [`normalize`](#host-keys-normalize)).

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

Cost: string work, plus — only for the shapes that can be `altgr` or `composes`, and only with `layout` left on — one question to the keyboard layout. Main thread, like every Lua call. It raises for a `spec` that is neither a string nor a number, for an `opts` that is neither a table nor `nil`, and for a `layout` that is not `true`, `false` or `nil` (`host.keys.check: layout is true or false, not a string` for `{ layout = "no" }`); `nil` is the default, `true`. No capability needed (see [`normalize`](#host-keys-normalize)); unlike `normalize` and `describe` it reads something besides the string, the character the current layout types with the chord.

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

Begins intercepting the [key spec](#key-spec-string-format): the keypress is swallowed (not passed to the underlying app) and `callback` is invoked with a `mods` table describing the modifier state at press time. Its fields are the roles, so on a Mac `mods.ctrl` is Command held and `mods.win` is Control held. The callback runs on the next pump tick, for a key-down; the key-up is never reported. What auto-repeat does, and whether the key-up reaches the application, are in the platform sections. Re-capturing the same `(vk, mask)` for this module replaces the previous callback (and mints a new token) — a module holds one capture per combination. Across modules the earliest standing capture whose module's [scope](#host-keys-scope) is the window in front gets the key (see the top of this page). Installs the low-level keyboard hook on first use (idempotent). Raises an error for an unknown spec. **Returns a token** to pass to [`host.keys.release`](#host-keys-release); keep it if you'll release this specific capture: when two overlays in one module capture the same key one after the other, the later capture has replaced the earlier, and the earlier overlay's token no longer releases anything.

```luau
host.keys.capture("Tab", function(mods)
  -- Tab is now swallowed app-wide (or in the scoped window); move overlay focus
  moveFocus(mods.shift and -1 or 1)
end)
host.keys.capture("Shift+Tab", function() moveFocus(-1) end)
```

### Windows

A low-level keyboard hook. It needs no permission and installs essentially always. It runs on a thread of its own that does nothing else, and stays installed for the rest of the session — out of the chain while one of this application's own windows is in front, and installed again whenever Windows may have dropped it (see below). Every keystroke on the machine passes through it, and matching costs microseconds whatever module code is running: a captured key is swallowed at once and queued, and its callback runs when the main thread is free, late by as long as that thread was busy — a long callback delays the overlay, not the user's typing, and does not let a captured key through to the application. Windows does not call it for keys going to this application's own windows — the module manager, a module's dialog — so there a captured key reaches the window and no callback runs; Microsoft does not document this, it was seen in the log (see "When Windows drops the hook" below), and the hook is out of the chain there anyway (see "Out of the chain in this application's own windows" below).

A capture fires on **every** key-down, auto-repeat included: holding an arrow runs the callback at the keyboard's repeat rate, and with it any screen read inside.

The key-up is not tied to its key-down: the hook matches it again on its own, against the modifiers held, the modules' scopes and the menu state at the moment of release, and swallows it only if that matches too. So after a captured `"Ctrl+Right"`, letting go of Ctrl before Right hands the application a bare Right key-up; the same happens when the scoped window lost the foreground, or a menu opened or was declared open, while the key was down. The other way round, a matched key-up is never swallowed when its key-down reached the application — pressed before the capture was made, while a menu was open, or while the hook was being installed again (see below): the hook asks the system whether the key is down (`GetAsyncKeyState`, which a key-down the hook swallowed never sets) and lets the key-up follow, so the application is never left holding a key.

While a **screen reader's own modifier** is physically down — Insert, numpad zero or Caps Lock — a matched key is passed through instead of swallowed. None of those is a Windows modifier, so without that rule NVDA+Space would arrive as a bare Space and be eaten by any overlay claiming `"Space"`. The hook knows the modifier is down from having seen it go down (a screen reader whose hook comes after the host's swallows it, so the system cannot say), and it is not called for keys going to a window of a higher integrity level — a screen reader's own menus and dialogs among them — nor for keys going to this application's own windows. So NVDA+N, with the modifier released once NVDA's menu is in front, would leave it recorded as held, and every captured key would then go to the application. The host forgets that record whenever the keyboard goes where the hook is not called: a window of a higher integrity level, or one of this application's own — the module manager, a module's dialog — coming to the front, the session being locked, unlocked, disconnected or connected, the machine suspending or resuming. The first time it forgets one, the log says so, naming the window when one came to the front (`a window the keyboard hook is not called for came to the front (one of this application's own: Windows does not call its hook for keys going to them) while the keyboard hook had a screen reader's modifier recorded as held (seen going down N ms before): forgotten …`, or `(integrity level 0x2010, above this process's)`; once per session). The cost is a keystroke made with the modifier held across such a moment, which a capture of it then takes.

**A key passed on** to the program after its busy module let go of the capture (see the top of this page) is one `SendInput` of every event: the left key of each modifier it presses (left Ctrl, Alt, Shift or the Windows key), the key itself, and the same keys going up. The key goes on the scan code the hook saw at the press, with the extended-key flag if it had it — so the numeric keypad's Enter is sent as that Enter, and a keypad arrow with Num Lock off as the keypad's; a key whose event carried no scan code, a hotkey's and each modifier go on the scan code `MapVirtualKeyW` gives, with the extended-key flag for a key that has one (the arrows, Insert, Delete, Home, End, Page Up and Page Down, the Windows key). `SendInput` puts them in one after another, with no other input between them. Each carries a mark in its `dwExtraInfo`, and the hook lets an event with that mark through before it records, matches or counts anything about the key. The key counts as held down while the system says it is down (`GetAsyncKeyState`) or while the hook swallowed its key-down and has not seen its key-up since, which the system cannot report; the modifiers held are the system's, and a screen reader's key counts as held as the hook's rule above has it. The keys that reach the program are the key-downs the hook lets go on, but for those of 0xFF: the masking key a [hotkey](./hotkey.md#host-hotkey-register) the hook matches sends, and the code Windows gives a key that has no virtual key of its own, so such a key typed after the captured one does not keep it from being passed on; a key the hook does not see — one typed into a window of a higher integrity level or of this application — is not counted, and the window check covers that case. A hotkey press takes the count as the hook matched it, or as its `WM_HOTKEY` reached the application when only `RegisterHotKey` delivered it. When Windows takes only a part of the events, or none, the key-ups for what that part pressed are sent at once, and the line says so: `… was dropped: its registration was released meanwhile, and passing it on to the program in front failed: SendInput took 0 of its 2 key events`.

**Why a captured key was let through** is written to the log, once per reason and window in front for as long as keys stay captured: `captured Tab (vk 0x09/m0) was let through to the application:` followed by a screen reader's modifier recorded as held (with how many milliseconds ago the hook saw it go down) or down as far as the system knows, every capture of it scoped to another window than the one in front (the earliest capture's window and the one in front), the window in front in menu mode, or a menu declared open with [`host.keys.menuOpen(true)`](#host-keys-menuopen) by a module scoped to the window in front or to every window. A key the hook took is not logged here; with tracing on, `dispatch` lines show those.

With tracing on, the first five **arrow key-downs the hook lets go on** towards the program after each change of the captured set, while a module's [scope](#host-keys-scope) is pinned to the window in front, are written too — a press each: a held arrow's auto-repeat is not written again until the arrow has been released — with what the hook read at the key: the window in front (`GetForegroundWindow`) and, from `GetGUIThreadInfo` for the thread in front, its focus window — with that window's class, read when the line is written — its active and capture windows, and its flags: `[keys] arrow vk 0x27 let through: window 0x72195a thread focus 0x3305ae class 'wxWindowNR'; active 0x72195a; capture 0x0; flags 0x0 (2 ms ago; 1 of the first 5 since the captured set changed)`, with `thread focus none` when that thread has no focus window and `its thread could not be read (GetGUIThreadInfo error N)` when the call failed. They are for an arrow that reached no overlay and did nothing in the program: whether the program's thread had a focus window at all. Both calls are ones the hook makes for captured keys as well, and neither sends a message; the scope is looked up in the hook's key table, as for a captured key, and the line is written on the main thread.

**When Windows drops the hook.** Windows removes a low-level hook without telling its owner when a call to it times out, and says so in its documentation: there is no way for the owner to find out. Captured keys then reach the application as if nothing had captured them, and hotkeys go on through `RegisterHotKey`. The host installs its hook again, on the hook's own thread, in two cases:

- when the machine **resumes** from sleep or hibernation, and when the session is **unlocked** or connected to the console or to a remote client again — whether or not the hook was lost. Not while the hook is out of the chain for one of this application's own windows (below): then the window in front is looked at, and the hook goes back in, freshly and first in the chain, if it is another program's by now, and otherwise at the next window of another program;
- when its **keyboard watch** sees the hook gone quiet. The watch is a thread of its own that receives the keyboard's raw input (`RegisterRawInputDevices` with `RIDEV_INPUTSINK` — posted to it, never waited for, so no timeout can remove it) and compares the physical key-downs it sees with the hook's calls. Three counted key-downs in a row that the hook was not called for install it again. Each is judged by whether the hook was called since the key-down before it, with a second of slack, so a key-down counts as missed only when the one before it came more than a second after the hook's last call: the hook is installed again at the fourth key-down pressed more than a second after its last call — typing five keys a second, around the ninth key after the removal — and from the next one on the captures work. The key-downs until then reached the application, captured or not; their key-ups follow them there (see above). No clock decides it: an idle machine installs nothing.

A key-down counts only when the hook could have seen it: not while a window of a **higher integrity level** is in front (an elevated program, or a screen reader's own dialogs, which run a step above an ordinary program), not while the **lock screen or a UAC prompt's secure desktop** has the keyboard, never an **injected** key (from [`host.input.send`](./input.md#host-input-send), a screen reader or remote-control software), which raw input reports without a device, and never a key typed into **this application's own windows** — the module manager, a module's dialog. Windows does not call the application's hook for keys going to its own windows, and Microsoft does not document it: on 2026-10-05 Tab pressed in the module manager installed the hook again, the lines saying 3, 6, 12 and 24 key-downs had reached raw input and not the hook. Raw input reports with each key whether this application had the foreground when it was pressed (`RIM_INPUT`), and that report decides, not the window in front when the watch gets to the key. While the hook is out of the chain for one of those windows (below), the watch counts no key at all — not even one typed into another program's window before the hook is back there — and it starts its count afresh when the hook is back. A hook that answers late still counts as called. Each re-install the watch asks for raises the number the next one takes — 6, 12, and so on up to 320 — until the hook is called again, so a reply that goes missing costs one longer round, never the watch; a failed re-install for a resume or an unlock raises it the same way, a successful one does not.

Every capture and every granted hotkey carries over; nothing is registered again. Each re-install writes one `keys` line to the log: the reason (`resumed`, `unlocked`, `the hook stopped seeing keys the system delivered: 3 key-downs …`), whether the old hook was still installed or Windows had already removed it — only in the second case is what the old hook recorded as held forgotten, since only then did it miss key-ups — and that the new one is **first in the chain** of low-level keyboard hooks again — ahead of every hook installed since the host started, a screen reader started or restarted since then included, which is the order of a host started after the screen reader (see [`host.hotkey`](./hotkey.md#windows)). A failed re-install writes the `SetWindowsHookExW` error and keeps the old hook; so does one whose old hook cannot be taken out (`UnhookWindowsHookEx` with another error than `ERROR_INVALID_HOOK_HANDLE`), which takes the new one out again rather than leave two hooks of the host handling every key twice. A re-install for missed key-downs adds what the watch saw of them: for how many this process had the foreground when the key was pressed (raw input's `RIM_INPUT`), the window in front when the watch asked — its handle, class, program and process, and whether it is this application's own — and how often the hook procedure was entered since the first missed key-down and since the host started, with how long before that key-down it was last called: `… 3 key-downs in a row reached raw input and not the hook; this process in the foreground for 0 of them; window in front 0x72195a class 'REAPERwnd' reaper.exe pid 4321 (this application: no); hook entered 0 times since the first missed key (51234 in all), last called 4012 ms before it)`. Since a key typed into this application's own windows is not counted, only the first of the key-downs can be one: the key the first miss was judged from. The window is read on the watch's thread, when it asks; the hook only counts its entries, one add as its first statement. The watch writes one line when it starts, saying which of its registrations Windows accepted. It costs one message per key event, to a thread that does nothing else, and changes nothing about the keyboard for any program, this one included.

**Out of the chain in this application's own windows.** For keys going to this application's own windows Windows calls neither the host's hook nor the hooks behind it in the chain of low-level keyboard hooks, and Microsoft documents neither. It was seen live on 2026-10-10 with a screen reader, whose hook reads out the keys typed into the module manager: started after the host, its hook ahead of the host's, it read them; with only the host started again, the host's hook ahead, it read none there, and every key in other programs' windows. So while a window of this application is in front — the module manager, a module's dialog, the tray menu — the host's hook is out of the chain, and while a window of another program is, the hook is in it, installed again **first in the chain** as when the host starts (see [`host.hotkey`](./hotkey.md#windows) for what the order means). Windows of this application one after another, or of other programs one after another, move nothing. A window counts as this application's when its process is the host's, or the process of its root owner is (`GetAncestor` with `GA_ROOTOWNER`): a web page in a window of the host is drawn by another process (WebView2's `msedgewebview2.exe`) inside that window, and a popup of it, a select list's, is a window of that process owned by the host's.

Where the hook belongs is decided by the window in front at the moment the keyboard watch looks (`GetForegroundWindow`), never by the window a system event names, and compared with where the hook's thread says the hook is: the watch asks that thread to take the hook out or install it, one move at a time, and looks again when the answer comes, so a switch made while a move is on its way is caught then and no stale move waits behind it. A re-install for missed key-downs, a resume or an unlock can be on its way beside a move; the look waits for the answers to all of them, and a re-install counts as a move back. The watch looks, on its own thread, at every foreground event (`EVENT_SYSTEM_FOREGROUND`), at every focus event (`EVENT_OBJECT_FOCUS`), at every answer of the hook's thread, at every physical key raw input reports from the other side — a key-down or a key-up gone to this application (`RIM_INPUT`) while the hook belongs in the chain, one gone elsewhere while it belongs out — and at every key while a move is owed or on its way, and at a resume, an unlock or a connect while the hook is out. A look costs a few queries (`GetForegroundWindow`, `GetWindowThreadProcessId`, `GetAncestor`) and asks the hook's thread for nothing when the hook is where it belongs; no clock decides it, and none of it waits for the main thread. Seen live on 2026-10-11 with a test build that shows a web page, and why the events alone are not trusted: a page whose show Windows had declined was named by a foreground event while another program's window kept the front, and when the page did come to the front a few seconds later no foreground event came at all, so the hook stayed in the chain there for two seconds — and the screen reader, its hook behind the host's, missed the key-ups of the combination that had brought the page forward and was disturbed from then on, most likely by modifiers it took as held. The key-ups are why a key-up looks as well: they go up in the page.

Nothing the hook does is lost meanwhile: no capture applies in the application's own windows, and hotkeys arrive through `RegisterHotKey`. The captured set, the granted hotkeys and the modules' scopes and menu flags stay as they are, for the hook that comes back. What the hook recorded as held — a screen reader's modifier, a pending modifier tap, captured keys held down, and the modifiers it saw go by — is forgotten as it goes out, since it sees no key-up until it is back; keys owed their key-up for a screen reader stay owed. The watch keeps its raw input registration: it was registered in both of the runs above, so it is not what cuts the other hooks off. A key pressed in the moment between a switch and the hook's thread having made the move goes on without the hook: into the application's window, where the hook is not called anyway — and, the hook still in the chain, neither are the hooks behind it, so a screen reader whose hook is behind the host's misses those first keys there — or into the other program's, even when a module captures it there.

Each move writes one `keys` line once the hook's thread has made it: `the keyboard hook is out while one of our windows is in front ('Modules'), after 'REAPER' (reaper.exe)`, naming the other program's window that came to the front last before it (left out when there was none, as when the hook goes out at the start), and `the keyboard hook is back, first in the chain ('REAPER' (reaper.exe) in front), after 12.3 s with our windows in front`, the time from the move out to this one. A window is named by its title, or `untitled, class '…'` when it has none, or `a window that is gone` when it has neither by the time it is named, and another program's window by its program as well; the title is read without sending the window a message (`InternalGetWindowText`), so naming one of the application's own never waits for the main thread. When Windows had removed the hook already, the out line adds `; Windows had already removed it — it had stopped being called`. A hook that cannot be taken out (`the keyboard hook could not be taken out while one of our windows is in front (…): UnhookWindowsHookEx error N, so it stays in the chain while our windows are in front …`) stays in the chain, and is asked out again at the next window that comes to the front (a foreground event), or once one of another program has been in front and one of the application's is again; one that cannot be put back (`the keyboard hook could not be put back (… in front): SetWindowsHookExW error N …`) is tried again when the watch sees it miss key-downs, and at the next window that comes to the front. Neither is asked for again at a focus change or a key, which come many to a window, nor at the answer of a re-install that was refused as well. A move the hook's thread could not be asked for at all says so once (`could not ask the keyboard hook's thread to … (PostThreadMessageW error N)`) and is asked for again at the next key and the next foreground or focus event. When the watch starts, its line says whether Windows accepted the focus events as well (`focus events yes`).

**A key that reached the hook late.** A physical key-down the hook is called for 250 ms or more after it was pressed — measured against the key event's own timestamp, so an injected key, whose timestamp is its sender's, never counts — writes one `keys` line: `a key reached our keyboard hook 812 ms after it was pressed (vk 0x09, 'REAPER' (reaper.exe) in front): it was held that long before our hook was called — by a hook ahead of ours in the chain, or by a machine too busy to run our hook's thread`. The hook cannot tell which: Windows calls the hooks in the chain one after another, so a hook ahead of the host's that takes long keeps the host's waiting, and so does a machine too loaded to schedule the hook's thread. At most one such line is written a minute. The first late key-down after a minute without one is said at once; the ones after it are counted, and a minute after that line one more names the one most late of them, with the window that was in front at the first, and adds `; 37 more key-down(s) came 250 ms or more late since the line before (this one names the most late; a line at most once a minute)` — and so on, a line a minute, for as long as key-downs come late; with none in a minute, none is written. So a key held down with auto-repeat while the keys come late writes one line a minute, not one per repeat. Key-downs that come late faster than the keyboard watch gets to them are counted the same way. A key-up is never said. In the hook this costs a comparison per key event and, for a late key-down, a few atomic operations, and for the first since the keyboard watch last took them one `GetForegroundWindow` and one `PostMessageW` to it; neither waits. The watch writes the lines on its own thread, with a timer on its own window for the line a minute after the last. While the hook is out of the chain for one of the application's own windows (above) it is not called, so nothing is said there.

### macOS

This call **can raise**. Without the Accessibility grant the event tap is refused and the error reaches Lua. Worse would be the case where only Input Monitoring is missing, if the tap needs it — which is not known yet (see [macOS permissions](../macos-permissions.md#input-monitoring)): the tap is created, reports itself as enabled, and never fires — `capture` returns a token, no key ever arrives, and nothing errors. A module that announces "overlay ready" on a successful return can be announcing it into a session where no key will ever reach it.

A refused tap is not remembered as installed, so **every** later capture tries again, and raises again, until the tap can be created. A capture that raised is nevertheless registered — the key is entered in the captured set before the tap is asked for — and its token is lost with the error: once a later capture installs the tap, that key is suppressed and handed to the callback passed to the `capture` call that raised. [`releaseAll`](#host-keys-releaseall), the same module capturing the same combination again, or a reload of the module replaces or removes it; disabling the module stops the suppression for as long as it is disabled.

A capture is **not refused for a combination the system keeps** (`reserved` in [`check`](#host-keys-check)); only a spec that does not parse raises. A spec written for Windows lands on the Mac's counterpart, so a capture of `"Ctrl+Q"`, `"Ctrl+H"`, `"Ctrl+Space"` or `"Ctrl+Tab"` asks the event tap for Command-Q, Command-H, Command-Space or Command-Tab, for as long as it holds (and, with a [scope](#host-keys-scope), while that window is in front). The log says so once per chord, with the reason the system keeps it. Give the Mac another key with [`host.os.pick`](./os.md#host-os-pick), as the overlay runtime does for its tab keys (Control-Tab, `"Meta+Tab"`).

**App Nap.** While any key is captured — by any module — the host holds an `NSProcessInfo` activity with the options `UserInitiatedAllowingIdleSystemSleep` and `LatencyCritical`, the request macOS offers against App Nap, and lets it go once the captured set is empty again and no game controller is listened to ([`host.gamepad.on`](./gamepad.md#host-gamepad-on) holds the same activity). At a first setup, while the application's own Screen Recording request waits for Accessibility, an activity without `LatencyCritical` is held (see [macOS permissions](../macos-permissions.md#how-the-application-gets-into-the-screen-recording-list)); keys captured meanwhile make it latency-critical, and releasing them makes it plain again. An application with no Dock icon and no window in front is otherwise a candidate for App Nap, which delays its main thread — the one that carries the event tap. The activity does not keep the Mac from sleeping when it is idle. The first time it is held for captured keys in a session the log says so (`App Nap: holding a latency-critical activity while keys are captured …`), also when a game controller or the setup wait already held it; later holds and ends are traced. Of the key paths, only captured keys hold it: a registered hotkey alone, or a module waiting for a window, does not.

**A key passed on** to the program after its busy module let go of the capture (see the top of this page) is posted with `CGEventPost` at the HID level, as [`host.input.send`](./input.md#host-input-send) posts, but from an event source of its own with a private state table, so that what it posts never reads back as keys the user holds: each modifier it presses as a `FlagsChanged` event of the left key of its role (Command, Option, Shift or Control), the key's own key-down and key-up, and the modifiers going up, every event carrying as flags the modifiers held from it on — so the key's own two carry exactly the press's. The key goes on the keycode the tap saw at the press, so the keypad's Enter is not sent as Return; a hotkey's on the keycode its spec names. Every event is built before the first is posted, so one that cannot be built posts nothing. Each carries a mark in its event-source user data (`kCGEventSourceUserData`), and the tap lets an event with that mark through before it records, matches or counts anything about the key. The window in front is asked afresh of the frontmost application when the key's turn comes — one accessibility round trip — because a window that opens in the application already in front, a dialog, need not tell the tap. The key, by that keycode, counts as held down while the HID system reports it down or while the tap swallowed its key-down and has not seen its key-up since; the modifiers held are the HID system's. No screen reader's key is asked: VoiceOver's are Control and Option, held modifiers like any other, and Caps Lock as VoiceOver's key is the gap below. The keys that reach the program are the key-downs the tap lets go on; a hotkey press takes the count as Carbon hands it to the application.

There is **no screen-reader pass-through rule**. Caps Lock contributes no modifier bit at all, so with VoiceOver's modifier set to Caps Lock, VO+Space arrives as a bare Space, mask 0, and an overlay claiming Space will capture and suppress it. With the default Control+Option the mask is non-zero and a bare-key capture does not match, so this bites only on the Caps Lock setting.

**When the system switches the tap off.** A tap whose callback takes too long is switched off by the system (`kCGEventTapDisabledByTimeout`), and user input can switch it off too (`kCGEventTapDisabledByUserInput`). The system says so through the tap, and the host re-enables it at once; the log line (`the system disabled the event tap …`) is written just after, not inside the callback. A tap switched off without a word is found by a watchdog on the main run loop within two seconds and re-enabled, with a line. A tap whose mach port has become invalid is **created again** — at the head of the taps, with every capture carried over — and says so; if it cannot be created (the Accessibility grant withdrawn, say), that is said once and tried again at every check. A tap that stays off when re-enabled is created again once; if the new one is off as well — a permission, or secure input — the line says so and it is left alone until it reports itself on, or one of the moments below. When the Mac **wakes**, the screen is **unlocked** or this user's session becomes **active again** after fast user switching, the tap is checked at once rather than within the two seconds, and the log says what was found (`the Mac woke from sleep: the event tap was checked and is valid and enabled`). Keys pressed while the tap was off reached the application. Nothing on a Mac counts keys the tap missed — there is no witness like the Windows keyboard watch, only the question whether the tap is enabled and its port valid — so keys typed into this application's own windows cannot make the host create the tap again. While the screen is locked, a password field holds secure input or another user's session is in front, the tap sees no keys at all; that is the system's doing, not a fault, and nothing is done about it. The tap stays in place while one of this application's own windows is in front: the Windows hook's moves out of the chain and back have no counterpart here.

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

Removes **all** key captures owned by this module and refreshes the suppression set. Useful when tearing down an overlay. The module's [scope](#host-keys-scope) and [menu flag](#host-keys-menuopen) stay as they are, for its next captures.

```luau
host.keys.releaseAll()
```

## host.keys.scope(toForeground) {#host-keys-scope}

**Signature:** `host.keys.scope(toForeground: boolean)` → `nil`

Scopes your module's captured keys. `true` pins them to the **current foreground window**, taken at call time — the hook then only intercepts them while that window is foreground, so a menu/popup that brings another window forward receives keys natively (ReaHotkey's `HotIf WinActive` model). `false` makes them global again. Main thread; what `true` asks of the system is in the platform sections.

**Your module's own.** The scope and the [`menuOpen`](#host-keys-menuopen) flag belong to the module that set them and apply to the keys it captures: `scope(true)` pins *your* captures to the window in front, `scope(false)` makes them global again; nobody else's change. "Your module" is the module whose VM runs the call, so a code dependency's `host.keys` scopes the captures of the module that depends on it. A module that never calls `scope` captures everywhere. A captured key goes to the earliest registration among the captures whose scope is in front; a capture scoped to another window does not take it, and the key reaches the application if no capture in scope wants it. The scope is a whole top-level window: two plug-ins inside one DAW window share it, so there the earlier registration wins. A menu flag counts for the window it was set for: while a module that holds captures and is scoped to the window in front, or to every window, says a menu is open, every capture in that window lets its key through. Disabling a module takes its captures out of the hook and forgets its scope and flag: enabled again, its captures are back — in every window, with no menu — until it sets them again. Reloading or removing a module drops its captures, its scope and its flag. The [`passedThrough`](#host-keys-passedthrough) record is per module too. The overlay runtime sets and clears its module's scope and flag as its overlays activate and deactivate, and a module's overlays share the one scope and the one flag: the overlay that comes to the front pins the scope to its window, and only the last of them to leave sets it back to every window — one leaving while another is in front leaves that one's pin, and the flag is then what the ones still in front say. A module that also drives `host.keys` directly shares them with its own overlays.

```luau
-- This module's captures only: Tab is taken while the plug-in's window is in front, and goes
-- to another module's overlay in another window, or to the application.
host.keys.capture("Tab", function() ov:focusNext() end)
host.keys.scope(true)

-- On teardown: this module's captures, and its scope and flag, set back. Another module's
-- overlay that came up meanwhile keeps its own.
host.keys.releaseAll()
host.keys.scope(false)
host.keys.menuOpen(false)
```

### Windows

The scope is a window handle, `GetForegroundWindow` at call time, and at press time the hook compares each module's against a **live** query for what is in front. It is always current. While the foreground changes hands Windows names no window for a moment; a `scope(true)` then is every window, and the log says so: `key scope: no window was in front when asked, so the scope is every window for now`.

### macOS

The comparison is against a value **cached from notifications** — application activated, focused window changed. A window that merely *opens* inside an already-frontmost application raises neither, so the pin can disagree with what the tap believes is in front. Setting the scope asks the frontmost application for its window in front (an accessibility round trip into it), and re-asks once when the answer disagrees with the tap and stores the fresh answer, which repairs the common case; it goes unresponsive only when that second resolve disagrees too, and then Tab does nothing until the user switches away and back. When the application does not name a window in front — it does not answer, or answers with none — the scope is the window the tap last saw in front, and the log says so (`key scope: the frontmost application did not answer when asked which window is in front, so the scope is the window the event tap last saw in front, …`); only when the tap has seen none either is it every window. It used to be every window at once, which let such a module's captures take their keys in every other window too.

## host.keys.menuOpen(open) {#host-keys-menuopen}

**Signature:** `host.keys.menuOpen(open: boolean)` → `nil`

Tells the hook a plugin's own (Qt/UIA) menu is open (`true`) or closed (`false`). While open, captured navigation keys (e.g. `Tab`/`Enter`) **pass through** to that menu instead of being consumed by the overlay — covering plugin menus the Win32 menu-state check can't see. Like [`scope`](#host-keys-scope), the flag is your module's own: your `menuOpen(false)` never closes another module's menu. It counts for the window your captures are scoped to — every window, without a scope — and only while your module holds a capture; there it lets **every** module's captured keys through, since a menu open in a window is open for every key pressed in it. The overlay runtime writes it on every tick of its menu timer while one of a module's overlays with [menu tests](overlay.md#o-menutests) is in front — `true` while a menu counts as open for one of them — and, when one of its overlays leaves the front, `false` if it was the last of them in front — otherwise what the ones still in front say — and once `false` when one starts [holding its place](overlay.md#o-menutests) over its own menu's window — right after the `true` of that menu's opening, when the tests saw the menu only at that moment — after which it writes nothing for that overlay until the hold ends; so while such an overlay is in front a value a module writes itself lasts until the runtime's next tick (150 ms) at most.

**While your module waits.** The hook reads the flag at every press, so it holds while a handler of your module [waits](../module-runtime-and-lifecycle.md#a-handler-waits) for a read: the menu in its window keeps its keys meanwhile. What sets it again waits too — the overlay runtime's menu timer is an `every`, skipped while the module is busy — so a menu that closes during the wait lets the captured keys of that window through until the wait has ended and the flag is set again, or until the module is disabled or [stopped](../module-runtime-and-lifecycle.md#limits), which forgets it.

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

A false answer means "no menu the OS drew", not "no menu". A plug-in that paints its own menu inside its window — a Qt menu, typically — is invisible here; that case is what [`host.keys.menuOpen`](#host-keys-menuopen) is for, and the overlay runtime drives it from the menu tests an overlay names (see [`O.menuTests`](overlay.md#o-menutests)), whose building block `O.menuTests.nativePopup` is this call. Note also that you do not need this call to get key pass-through while a native menu is up: the key hook consults the same answer itself on both platforms and stops suppressing captured keys for the duration. What the call adds is everything else an open menu should change — the overlay runtime gives up its registered hotkeys while it is true — and modules call it to quiet their *own* polling and reading.

```luau
-- The overlay runtime's building block, whole: a menu test that is cheap enough for every tick.
local nativePopup = {
  name = "nativePopup",
  cheap = true,
  test = function(_, answer) answer(host.keys.nativeMenuOpen() == true) end,
}

-- A module's own guard: a read-out watcher that stays quiet while a menu is drawn over it,
-- rather than announcing the menu's text as a changed value.
host.timer.every(500, function()
  if host.keys.nativeMenuOpen() then return end
  readTheStrip()
end)
```

### Windows

A live question, asked fresh on every call: `GetGUIThreadInfo` for the foreground thread, true when that thread is in menu mode, popup-menu mode or system-menu mode. So it covers real Win32 menus — a `#32768` popup, the menu bar, a window's system menu — and it is never stale, because nothing is cached. A menu open in some other program does not count: only the thread that owns the foreground window is inspected.

### macOS

Nothing is asked at call time. The answer is a counter kept by the accessibility observer from the frontmost application's `AXMenuOpened` / `AXMenuClosed` notifications, so the common case — nothing open — is one relaxed load. It **counts rather than latches**, so a submenu opening and closing again does not clear its parent, and notifications from any application that is not frontmost are discarded — an unrelated program with a menu up must never disarm the overlay. The count is also cleared outright when a different application comes to the front, because whatever menu was believed open belonged to the application just left; switching away from an app with a menu up therefore re-arms at once.

**No clock decides the answer.** A menu counts as open from its `AXMenuOpened` until its `AXMenuClosed`, however long it stays up. So the answer depends on the close notification arriving: if an application ever loses one, the count stays up — and the event tap goes on letting captured keys through — until a different application comes to the front. The log names each transition (`a menu opened in pid …`, `the menu in pid … closed`), so an opened line with no closed line after it names the application that lost one. An application that draws a menu without posting either notification reads here as no menu, which is the same shape of gap as a self-drawn Qt menu on Windows and has the same answer: a test of the module's own ([`O.menuTests`](overlay.md#o-menutests)).

---

## host.keys.passedThrough() {#host-keys-passedthrough}

**Signature:** `host.keys.passedThrough()` → `{ { vk: number, mask: number, key: string } }`

The keys the hook let through to the application because a menu was open, since the last call — your module's record, drained on read, so each key is reported to it once. Each module has a record of its own, which holds at most 32 keys until it is read: a key let through by a [menu flag](#host-keys-menuopen) is recorded for every module whose flag counted for the window in front, and one let through by a menu the system drew for the module whose capture would have taken it. Two kinds: any **captured** key let past because a menu was open, and **Return or Escape, captured or not**, whenever a module's `host.keys.menuOpen(true)` counts for the window in front — those two end a menu, no overlay captures Escape, and Return is captured only while the focused control wants it, so a record kept for captured keys alone would never hold the Escape that cancelled a menu. `key` is the spelling `host.keys.capture` would accept (`"Return"`, `"Escape"`, `"Tab"`, `"A"`, `"F5"`), or `"vk 0x.."` for a key the spec grammar cannot name.

What it is for: **knowing what reached a menu.** Whether a key that went through arrived where the user meant it is invisible from outside — the menu might have closed on it, or the keyboard might have been somewhere else all along. The overlay runtime asks on every tick of its menu timer while one of its overlays is in front, logs each key against the overlay whose menu is open, and follows an `Escape`: when a menu test still sees the menu two ticks later, the log says the key did not reach it or the menu ignores Escape. It decides nothing from these keys — whether a menu is open is only ever its [menu tests'](overlay.md#o-menutests) answer. Drained on read, so while such an overlay is in front its module's runtime is the one that gets that module's.

```luau
for _, k in ipairs(host.keys.passedThrough()) do
  if k.key == "Escape" then
    host.log.info("Escape went through to the menu")
  end
end
```

### Windows

Recorded by the low-level keyboard hook, on its own thread: a captured key it let past because the foreground thread was in menu mode or a module's `host.keys.menuOpen(true)` counted for the window in front, and an unmodified Return or Escape whenever such a flag counts for the window in front. Only key-down events; the matching key-up is not reported. Noted only — nothing here is suppressed that was not already.

### macOS

Recorded by the event tap: a captured key it let past because a native menu was open or a module's `host.keys.menuOpen(true)` counted for the window in front, and an unmodified Return or Escape whenever such a flag counts for the window in front. Key-down only. A key that never reached the tap at all — VoiceOver's own chords, for instance — is not in here, because the tap never saw it.
