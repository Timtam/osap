---
title: "host.hotkey — system-wide hotkeys"
sidebar_position: 4
toc_max_heading_level: 2
---

A hotkey is claimed from the operating system and fires wherever the user is, whether or not the plug-in is in front, which makes it the way *in* rather than the way around: the DAW module puts the keyboard back into a plug-in window from anywhere with one, and the probe dumps the focused window with another. The overlay also hangs one on individual controls — Kontakt's view toggle answers to Alt+V — and speaks the key when the control is focused, in the platform's own words ([`host.keys.describe`](./keys.md#host-keys-describe): Option+V on a Mac), so a plug-in with an unreadable interface teaches its own shortcuts.

**The scarcity is what to plan for.** A combination can be held by only one claimant on the whole machine and the OS refuses a second claim, so the overlay takes a spec only while a control that uses it is actually visible, and decides which control a press belongs to at press time rather than at registration.

A claim also outranks the application, which is a hazard as much as a feature: with Kontakt's file or snapshot menu open, Alt+P and Alt+M never reached the menu at all until the overlay learned to give its registrations back for as long as a menu is up. And the callback runs while the combination is still physically down, so a key or click synthesised inside it carries those modifiers unless you wait for them to be released.

**Only the key that completes the combination is taken; its modifiers still reach the application in front.** They go down before that key, and nothing can know at that moment that a hotkey is coming, so the application sees every Ctrl, Alt, Shift or Win press of a hotkey as it would see any other. A game that uses Ctrl as a button acts on every Ctrl+… hotkey pressed while it is in front. Pick modifiers the application in front ignores, or a combination without the one it reacts to.

The spec grammar is shared with `host.keys` and documented there. That includes the modifier roles: `Ctrl` is Command on a Mac and `Win` (`Meta`) is Control, so `"Ctrl+Shift+F9"` is Command+Shift+F9 there. It also includes the `"<modifier> tap"` form, which is a capture only and can never be held here (see below). A module that wants another combination on a Mac picks one with [`host.os.pick`](./os.md#host-os-pick). The key that puts the keyboard back into a plug-in window is Ctrl+Shift+Win+Alt+F6 on Windows and `Cmd+Shift+F6` on a Mac, because the four-modifier chord holds Control and Option on a Mac, which is VoiceOver's layer. [`host.keys.check`](./keys.md#host-keys-check) says before you register what the platform will do with a combination.

A combination that is also captured with [`host.keys.capture`](./keys.md#host-keys-capture) is a platform question: see the Windows section of [`register`](#host-hotkey-register).

**A key pressed twice is a gesture of its own.** Every press tells its callback how many times in a row the key was pressed — `press.count`, 1, 2, 3 — so once can say a value and twice spell it, as a screen reader's own commands do; and a binding that must not act on the first press of a double one asks to hear only the last ([Quick repeats](#quick-repeats-once-twice-three-times)).

**What each call answers** is under its **Returns**, and what it can fail with under its **Errors** ([Reading an Errors table](./errors.md#reading-an-errors-table)). `register` can answer an [error value](./errors.md), and it is one of the four calls that answer **an error value beside a result** — `page:show`, the two dumps and `register`: an id whose claim is not held now. `unregister` never answers one. A message may change between versions; its code does not. Two raises are not repeated at each entry: the capability gate, for a module that did not declare `hotkey`, and `stopped by the host: …`, once the module has run too long ([the limits](../module-runtime-and-lifecycle.md#limits)).

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["hotkey"]
```

See [what that list is and is not](./index.md#capabilities).

## host.hotkey.register(spec, callback, opts?) {#host-hotkey-register}

**Signature:** `host.hotkey.register(spec: string, callback: (press: { count: number }) -> (), opts: { foreground: boolean?, taps: ("every" | "final")?, within: number? }?)` → `id: number`, `id: number, Error` or `nil, Error`

Registers a **global** OS hotkey (active regardless of foreground window) for the [key spec](keys#key-spec-string-format) and returns an integer `id`. The callback is invoked each time the hotkey fires (only while the owning module is enabled), as a [handler](../module-runtime-and-lifecycle.md#handlers) of the module — one after another with its other handlers — however the press was delivered; what a held-down combination does is in the platform sections. It is handed one argument, `press`: `press.count` is 1 for a first press, 2 for a second quick press of the same combination, and so on ([Quick repeats](#quick-repeats-once-twice-three-times)). A callback written to take no argument runs as it always did.

**It can answer an id and an error value together**: a claim that is made and stands, but is not held at this moment. Read both values; see [Returns](#host-hotkey-register-returns).

`opts.taps = "final"` hears a quick sequence once, when it is over, with its count, instead of at every press; `"every"`, the default, hears every press at once. `opts.within` is the interval of this binding's quick repeats in milliseconds, 100 to 5000, a fraction cut to the whole number below; without it the system's double-click time holds. Both are under [Quick repeats](#quick-repeats-once-twice-three-times).

`opts.foreground = true` is **experimental**, for the HTML probe's test W2 (TODO.md): the press is left to the operating system's own hotkey delivery, which on Windows is what gives this application the right to bring a window of its own to the front — see the Windows section. It may go again once the probe has answered whether it is needed.

**What this platform can never hold is refused at the call**, and nothing is registered: a key it has no code for, `F21`–`F24` on macOS, and a combination **the system keeps for itself** — the `reserved` reason of [`host.keys.check`](./keys.md#host-keys-check): F12 and Win+L on Windows; Command+F5, Command+Q, Command+Tab and the rest of the list there on macOS. The same spec may be fine on the other platform, so the platform decides, not the call, and the answer is `nil, err` rather than a raise. Carbon would register Command+Q without complaint and take quitting away from every application for as long as the module held it, which is why this is a refusal and not a warning.

These are exactly the `check` reasons its table marks as refused. Nothing else `check` can report is refused: a chord on VoiceOver's layer, or one that also types a character, registers as asked. A combination that another **application** holds is not known until the claim is made; it is answered beside the id.

The host records, logs and reports a registration by its [normalized](./keys.md#host-keys-normalize) spec, so the log names the chord it is on this platform: `Ctrl+Shift+Win+Alt+F6` appears as `Ctrl+Alt+Shift+Win+F6` on Windows, and `Cmd+Shift+F6` as `Shift+Cmd+F6` on a Mac. The dialogs use the same form on Windows. On a Mac they say the key in [`describe`](./keys.md#host-keys-describe)'s spoken words, `Shift+Command+F6`, because the spec's word for the Control key there, `Meta`, is not the key's name.

**While its module is busy** — one of its handlers [waits](../module-runtime-and-lifecycle.md#a-handler-waits) for a read — a press waits behind that handler and runs once the module is free, in the order its events came. Up to 256 keys, hotkey presses and controller buttons wait; the next is dropped, with a `[keys]` line. The press runs on the registration it was made for, or, when the module registered the same combination again meanwhile, on the new one — unless the module has [scoped](./keys.md#host-keys-scope) its keys to a window other than the one in front at the press — and the log says so: `[com.example.game] Ctrl+Shift+F9, pressed while the module was busy, went to the module's current registration of it`. When another module holds the combination now, the press is dropped and said (`… pressed while the module was busy, was dropped: the hotkey went to another module meanwhile`); it never runs on another module's hotkey. When the module released the hotkey meanwhile and holds the combination no more, the press goes where a captured key its module let go of goes (see the top of [`host.keys`](./keys.md)): first to the next module that [captures](./keys.md#host-keys-capture) the combination in the window that was in front when the press reached the application, as a key of that module's — a capture is what the hook would have given the key to before the hotkey — said as `[com.example.game] Ctrl+Shift+F9, pressed while the module was busy, went to com.example.synth, the next module that captures it in the window it was pressed in: its registration was released meanwhile`; with none, to the program in front: sent as it was pressed, the same key with the same modifiers, while the window that was in front when the press reached the application still is, and said: `[com.example.game] Ctrl+Shift+F9, pressed while the module was busy, was passed on to the program in front: its registration was released meanwhile`. Otherwise it is dropped, and the line says why: the reasons a captured key's has — another window in front by then, or none known when the press arrived; the key still held down; a modifier, or a screen reader's key, held down that the press was made without; a key that reached the program after the press did, which the press would follow (`… and a key typed after it has reached the program first`); the system not taking the keys sent — or a combination the key spec does not name, which only the system read (`… and its combination is one only the system could read, so it cannot be sent again`). The press is sent on the keycode or scan code its spec names, never another key the system matched it on. Another module's hotkeys are not held up: they run at once.

### Quick repeats: once, twice, three times

**Every press carries its count.** `press.count` is 1, and one more for each press of the same combination that follows the press before it within the interval, with no other key between. The callback runs at once, at every press, with the count it has then — the default, `taps = "every"`: a second quick press runs it again with 2, so a binding that does one thing for one press and another for two does the first thing too, before the second press comes.

```luau
-- Once says the preset, twice spells it: the second press comes too soon to stop the first.
-- `presetName` and `spell` are the module's own.
host.hotkey.register("Ctrl+Alt+P", function(press)
  local name = presetName()
  if press.count == 2 then
    host.speech.output(spell(name))
  else
    host.speech.output(name)
  end
end)
```

**`taps = "final"` hears the sequence once, when it is over**, with its count: for "once does A, twice B, three times C", where A must not happen first. The sequence is over one interval after its last press, or as soon as a press of another key — or of the same key after the interval — starts a count of its own, which then runs after it. That costs one interval of waiting after the last press, and only for the bindings that ask for it. A held key's auto-repeat is never a press of it.

```luau
-- 1: play, 2: stop, 3 or more: back to the start. Nothing happens until the presses stop.
-- `transport` is the module's own.
host.hotkey.register("Ctrl+Alt+Space", function(press)
  if press.count == 1 then transport("play")
  elseif press.count == 2 then transport("stop")
  else transport("start") end
end, { taps = "final" })
```

**When the count starts again** — the press is a 1:

- after the interval, measured from the press before it: the system's **double-click time** — the user's own setting, read at every press, so a change holds from the next one — or the binding's `within`, 100 to 5000 ms. A press that comes the interval or more after the one before it is a 1;
- at a press of **another key**: another combination of any module, a hotkey or a [captured key](./keys.md#host-keys-capture) — the same key with **other modifiers** is another combination;
- after a key-down that **reached the program** in between: typed past every capture and hotkey, let through for a menu or a screen reader, another held key's repeat, or sent — by another program, or by [`host.input.send`](./input.md#host-input-send), this module's own included. So a callback that sends a key ends the count it runs in: with `taps = "final"` it sends after the sequence. The modifiers themselves go down and up between two presses without ending it.

A held key's **auto-repeat** never counts: a hotkey fires once per press however long it is held (the platform sections), and a captured key's repeat carries the count of the press it repeats.

**One count for the whole application**, by combination: a registration of the same combination made again between two presses — the overlay runtime gives its controls' hotkeys back and takes them again at every focus move — counts on, and a press of another module's key ends the count of this one's. A press is counted as it reaches the application, once, however it came — on Windows a press both the keyboard hook and `RegisterHotKey` delivered is paired first — and before its module's mailbox: **a press that waits behind a busy handler keeps the count it had** and runs with it once the module is free. A `"final"` sequence that is over while its module is busy waits there too, in its turn. It is timed by when it was pressed, as near as the platform says (the platform sections), not by when the module got to it.

A `"final"` sequence is handed on by the event loop's tick (every 15 ms), which looks at its deadline as it does at a timer's; nothing waits on the event loop for it. The deadline is measured against the moment the tick took the presses queued until then, not against the tick's own: a press made before the deadline that still waited in the queue — the event loop busy with other modules' callbacks — goes on with the sequence, which is then heard once, with that press in its count. A sequence once heard has ended its count: a press after it begins another, one that the clock, a few milliseconds off, places just inside the interval included — never the same sequence heard as 1 and then as 2. It stands for all of its presses: when the registration it was for is released, or another module holds the combination by the time it is heard, it is dropped and said, `[keys] [com.example.synth] Ctrl+Alt+Space, pressed 3 time(s) in a row, was not run: its registration was released before the sequence was handed over`, at most once every 10 s per module — never passed on to the program as one key, as a single press of a released registration is (above). A module disabled meanwhile hears nothing.

Cost: counted on the event loop as the press is dispatched — a comparison, and the double-click time asked of the system: one call, no screen touch, no question to another program. No timer or thread is added. One sequence waits at a time, the last key's, which the next press of another key ends.

**The numeric keypad is a screen reader's.** On Windows NVDA and JAWS use the numeric keypad for their own commands in their desktop keyboard layout, and a key the screen reader takes never reaches the host, nor its count (see [a screen reader can take a chord](#a-registration-is-a-claim-not-a-guarantee) below). On a Mac VoiceOver uses it only while NumPad Commander is on. The key spec names no keypad key yet ([key names](./keys.md#key-spec-string-format)).

### A registration is a claim, not a guarantee

Only one module can hold a combination at a time, and a valid spec that somebody else is
already holding does **not** raise. It is recorded as a standing claim, you get a real `id`
back with an error value beside it (`taken`), and your callback simply does not fire while the
other holder has it. The user is told which two modules want the same key.

The claim is honoured the moment the combination becomes free — the other module being
disabled in the module manager, uninstalled, or releasing the key with
[`unregister`](#host-hotkey-unregister). Nothing needs restarting, and there is nothing for
you to retry: the runtime recomputes who holds what whenever the set of enabled modules
changes. Between enabled modules the one that **loaded first** keeps the combination, and within a
single module its own earliest registration does. Load order rather than raw registration
order, so that reloading a module does not cost it its own key: it comes back with a fresh
registration that would otherwise look like the newest claim on the combination.

A module the application [stopped](../module-runtime-and-lifecycle.md#limits) — a callback
ran too long or out of memory — gives up its combinations at once, as a disabled one does, so
they go to the program in front or to a waiting claim. Presses of a hotkey made while that
callback held the application are dropped, with a `[keys]` line, rather than delivered all at
once when it ends — on Windows; on a Mac a press during the stall is not dropped (not checked
on a Mac yet). The application's own keys are the exception: pressed during the stall, the
reload key reloads every module after the stop, and the quit key quits the application then. A
stall that ends without a stop drops nothing.

The practical consequence for an overlay: **do not announce a hotkey as available just
because `register` returned an id.** It returns one either way; an error value beside it says
the claim is not held at the moment of the call. A claim held then can still be lost later —
on a Mac to a layout switch (below) — and nothing tells the module.

**A screen reader can take a chord before the host sees it.** JAWS and NVDA have keys of their
own, and both can be extended with scripts and add-ons. When the screen reader acts on a chord
first, the press never reaches the host — on Windows by neither of the two ways described
below: `register` has returned an id, the callback never runs, and no dialog says so, because the host
never learns of the press. Whether a press arrived is in the log — every hotkey press the host
receives writes `[keys] hotkey <spec> arrived` before anything else happens — and the answer
to one that never arrives is another combination. There is no list of screen-reader keys to
check against, on purpose: it could never keep up with what users add
([`host.keys.check`](./keys.md#host-keys-check) reports only what the platform itself does).

**"Binding unavailable"** is the dialog for a combination the operating system refused because
another program already holds it. The claim stays standing and is tried again whenever the host
next works out who holds which key — at any module's next registration or release, and at
every change to the enabled set. The first refusal is logged (`[conflict] [<id>] hotkey
'<spec>' rejected by OS: …`) and the ones after it are counted, not logged: the overlay runtime
registers and releases keys at every focus move, and each try wrote the line again for as long
as the other program held the key. When the key is granted at last, its `hotkey '<spec>' is now
held by [<id>]` line says how often the operating system had refused it. The dialog and the line
both come once, until the module is disabled and enabled again. That program can be this application: a
[headless run](../module-manager.md#headless-mode) started beside the windowed one registers its
modules' keys as any other process would. A second windowed copy of this build or a later one
cannot be the holder — starting the application again only brings the running copy's window
forward (see [the module manager](../module-manager.md#starting-it-again-brings-the-window-forward)).
A build from before that rule takes no lock and looks for none, so it can still run beside a
newer copy and hold keys; quit it from its tray icon.

### Returns {#host-hotkey-register-returns}

:::warning An id and an error value together
**`register` can answer two values that both count: an `id` and an error value.** That is a [partial answer](./errors.md#partial-answers). The claim is made and stands, so keep the id. But the combination is **not held at this moment**, and a press does not reach the callback until it is. A test of `id` alone takes this answer for a hotkey that works, and `if err then return end` throws away a claim that may be held a minute later. Test the two values apart.
:::

- `id` — the claim is made and held now: the next press runs the callback. A disabled module gets the id alone for every spec this platform can hold, because its claims are not made until it is enabled; it is not told then whether they are held. A spec this platform can never hold is answered `nil, err` all the same.
- `id, err` — **a partial answer**: the claim is made and stands, and it is not held now. It is tried again whenever the host next works out who holds which key: at any module's next registration or release, and at every change to the enabled set ([A registration is a claim](#a-registration-is-a-claim-not-a-guarantee)). A parked macOS letter is held as soon as a layout that types it is selected. `err.code` is `taken`, `not_supported` or `failed`, below. [`unregister`](#host-hotkey-unregister) gives the claim up as it gives up a held one.
- `nil, err` — nothing was registered: this platform can never hold the spec. `err.code` is `taken` or `not_supported`, below.

The callback is handed `press`, a table `{ count = n }`: a whole number from 1, the press's place in its quick sequence; with `taps = "final"`, the number of presses the sequence ended with ([Quick repeats](#quick-repeats-once-twice-three-times)). A new table for every press.

```luau
-- Written once: Ctrl+Shift+F9 on Windows, Command+Shift+F9 on a Mac.
local KEY = "Ctrl+Shift+F9"
local id, err = host.hotkey.register(KEY, function()
  host.speech.output("Hotkey pressed")
end)
local said = host.keys.describe(KEY, { style = "short" })
if id == nil then
  host.log.info(said .. " cannot be a hotkey here: " .. err)                   -- nothing registered
elseif err then
  host.log.info(said .. " is claimed as " .. id .. ", not held now: " .. err)  -- partial: keep the id
else
  host.log.info(said .. " is registered as " .. id)                            -- held now
end
```

### Errors {#host-hotkey-register-errors}

None of these is raised. Each comes as the second value, in one of two answers.

**`nil, err` — nothing is registered.** This platform can never hold the spec.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `taken` | return | The system keeps the combination for itself, the `reserved` reason of [`check`](./keys.md#host-keys-check). The message names the combination and what the system uses it for. | `the hotkey '…' (…) is kept by the system and cannot be registered: …. Choose another combination` | both |
| `not_supported` | return | A key macOS has no key code for, `F21`–`F24`. | `the hotkey '…' (…) cannot be registered: macOS has no key code for …. Choose another key` | macOS |
| `not_supported` | return | The build for other systems has no hotkeys. Every spec of an enabled module answers this, and there a spec that does not parse answers it too, from any module, instead of raising. Nothing is kept. A disabled module gets the id alone for a spec that parses, as everywhere, and its claim is refused when it is enabled. | `hotkeys are not implemented on this platform yet (spec '…')` | other |

**`id, err` — the partial answer: the claim is made and stands, and is not held now.** Keep the id.

| code | arrives | when | message | platform |
|---|---|---|---|---|
| `taken` | return | Another enabled module holds the combination, or an earlier registration of this module's own does — the module's earliest registration of a combination keeps it. The claim is held once that one lets go. | `module '…' holds this combination` | both |
| `taken` | return | Windows refused the combination, because another program holds it, or this application's own reload or quit key is on it. The claim refused is this one, or the earlier claim of the combination that would hold it before this one. It is tried again. | `RegisterHotKey failed for '…' (already in use by another app?)` | Windows |
| `taken` | return | macOS refused the combination, most likely because another application holds it. The claim refused is this one, or the earlier claim of the combination that would hold it before this one. It is tried again. | `macOS refused the shortcut '…' (RegisterEventHotKey returned …); another application probably already uses it system-wide` | macOS |
| `not_supported` | return | A letter that no key of the current keyboard layout types. The claim is parked, and held as soon as a layout that types the letter is selected. | `no key of the current keyboard layout types this letter` | macOS |
| `failed` | return | The Carbon handler every hotkey arrives through could not be installed, so no hotkey is held. The claim is tried again, the handler with it. | `could not install the Carbon hotkey handler (InstallEventHandler returned …); no global shortcut will work in this session` | macOS |

The dialogs and log lines of [A registration is a claim](#a-registration-is-a-claim-not-a-guarantee) come as before, beside the error value.

**Raises** for a mistake in the call — each a module author's to fix, and each with a message that says why:

- a spec the shared parser cannot read at all: an unknown modifier or key name, and the message names the part (the build for other systems answers it instead, above);
- a `"<modifier> tap"`, which is something [`host.keys.capture`](./keys.md#host-keys-capture) watches and no system registers: `the hotkey '…' (…) is a modifier pressed on its own, which cannot be a global hotkey: capture it with host.keys.capture instead`;
- a `spec` that is neither a string nor a number, or a `callback` that is not a function;
- an `opts` that is neither a table nor `nil`; a key in it other than `foreground`, `taps` and `within` (`host.hotkey.register: '…' is not an option; the options are foreground, taps and within`); a `foreground` that is not `true`, `false` or `nil` (`host.hotkey.register: foreground is true or false, not a …`); a `taps` other than `"every"`, `"final"` or `nil` (`host.hotkey.register: taps is "every" or "final", not "twice"`); a `within` that is not a number of milliseconds from 100 to 5000 once its fraction is cut — another number, `nan`, or not a number at all (`host.hotkey.register: within is a number of milliseconds from 100 to 5000, not 50`).

### Windows {#host-hotkey-register-windows}

`RegisterHotKey`, with auto-repeat suppressed: the callback fires once per press, and holding the combination does not fire it again. Each role is the key of its name: `Ctrl` is Control and `Win` the Windows key.

**Quick repeats** are timed by the press's own stamp on the system's tick clock — the key event's, for a press the keyboard hook matched, or the `WM_HOTKEY` message's (`GetMessageTime`) — so a press that waited for a busy event loop is timed when it was made. The interval is `GetDoubleClickTime`, the setting under Mouse properties, 500 ms unless the user changed it. The hook swallows the key-down of a press it matches, and is not called for one only `RegisterHotKey` delivers, so neither key-down is counted as one that reached the program. A press the hook lets through on purpose for `RegisterHotKey` to deliver — a screen reader's modifier held, or the key already down before the rest of the combination — goes on past the hook, which counts its key-down with the keys that went on towards the program; that key-down of its own is taken for no other key, so its quick repeats count on as well. Any other key-down the hook let go on between two presses ends the count (the count that decides it is the one a released registration's press is passed on by, [`host.keys`](./keys.md#host-keys-capture)). A key typed where the hook is not called — in an elevated window, or in one of this application's own — is not seen, and does not end a count.

A hotkey on `Ctrl+Alt+<key>` is `AltGr+<key>` to Windows, so it takes that key's AltGr character away from every application for as long as it is held (the `altgr` reason of [`host.keys.check`](./keys.md#host-keys-check)).

**A granted hotkey is matched in the keyboard hook as well**, the way AutoHotkey's `#UseHook` works. A program can switch registered hotkeys off for everybody while it is in front — WinUAE registers its keyboard with `RIDEV_NOHOTKEYS`, and then no program's `RegisterHotKey` fires, whatever the combination — while the host's low-level keyboard hook still sees the keys. So the hook swallows a press of a combination Windows granted and dispatches it itself. `RegisterHotKey` still claims the combination and still delivers the press whenever the hook does not: while an elevated window is in front (the hook of an ordinary process is not called for input going to it) or one of the host's own windows — the module manager, a module's dialog — whose input Windows does not hand the host's hook either, and where the hook is out of the chain (see "Out of the chain in this application's own windows" under [`host.keys.capture`](./keys.md#host-keys-capture)), while a screen reader's modifier (Insert, numpad zero, Caps Lock) is held — the hook lets those presses through, as it does for captured keys — when the key was already down before the rest of the combination was, and after Windows has removed a hook that timed out, until the host has installed it again (see below). A combination Windows **refused** is never taken through the hook: it belongs to another program, whose key would silently stop working.

A press reaches the callback once. Auto-repeat is swallowed without firing, as `MOD_NOREPEAT` does. A key-down counts as a repeat when the same key went down less than 1.2 seconds before with no key-up between — longer when the keyboard's repeat delay or, with FilterKeys on, its delay or repeat interval leaves longer gaps; the settings are read again with every registration, and the log says when they change the figure. A key already held when the modifiers arrive does not fire through the hook, and neither does a key that went down while the hook was not called for it — in front of one of the host's own windows, an elevated window or the secure desktop, or before a re-install — and is still held when the hook is called again: a key-down the hook is called for on time, of a key the system already holds down (`GetAsyncKeyState`), is its repeat, and for a call that comes late the keys the system holds down as the hook goes into the chain are recorded as held. Their repeats go on to `RegisterHotKey`, which knows the key is down and fires nothing (see "Keys held as the hook comes back" under [`host.keys.capture`](./keys.md#host-keys-capture)). Seen live on 2026-10-11: a hotkey pressed in a page reached the host through `RegisterHotKey` with the hook out of the chain, its callback hid the page, and the hook came back while the key was still held — and, before this, took the key's next repeat for a new press and fired the hotkey again. The other way round, a press the hook dispatched kept its key-down from the system, as does one a capture of the same combination took; when the callback brings one of the host's windows to the front, the hook goes out of the chain while the key is still held, and the keyboard's first repeat then reaches `RegisterHotKey` as a new press — the system never saw the key go down. A `RegisterHotKey` press stamped within the auto-repeat threshold after such a press, while its key was still held, is that repeat, and is dropped, with a `[keys]` line (`hotkey id N: RegisterHotKey delivered the keyboard's auto-repeat of a press the keyboard hook had dispatched, its key still held …; not dispatched a second time`); the same holds while an elevated window is in front, or after Windows removed the hook. Whether the key is still held is the hook's record, whose key-ups come from the hook and, where the hook is not called, from the keyboard watch's raw input — so without raw input (its registration refused, which the watch's first `keys` line says) no press is taken for such a repeat; a key-up raw input has not handed on by the time the next press is handled leaves the key held, and that press is dropped with the same line. A press `RegisterHotKey` delivers stamped within the window in which it is paired with a late hook's report of the same press (below) is that press, not its repeat; after a repeat the hook swallowed, the window is gone. When the hook answered too late — its thread was not scheduled within the system's timeout, so Windows passed the key on and `RegisterHotKey` delivered it as well — the two deliveries are paired by their timestamps and the second is dropped, with a line in the log; a late call is judged by the modifiers held when its key went down, not when the hook got to it. A press that came through `RegisterHotKey` although the hook should have seen it is logged once per window in front, with its possible causes: an elevated window or one of the host's own (the module manager, a module's dialog, a [page](./gui.md#host-gui-webview)), a hook running late, a hook Windows removed.

The application in front sees what it saw from a registered hotkey — the key-up, not the key-down — and one thing more: when the combination holds Alt or Win, or Ctrl and Shift together, the host presses and releases the virtual key 0xFF while the modifiers are still down: the code Windows gives a key that has no virtual key of its own, which names no key. Without it, letting go of Alt after a swallowed key opens the window's menu bar, letting go of Win opens Start, and Alt+Shift or Ctrl+Shift switch the input language. AutoHotkey masks its hook hotkeys the same way. The capture scope and the menu rules of `host.keys` do not apply: a hotkey fires whatever window is in front and whatever menu is open, as before.

**A screen reader's gesture on the same chord may lose to it.** Windows calls low-level keyboard hooks newest first. Before hotkeys were matched in the hook, a screen reader's hook always saw a registered chord before the hotkey did; now the one of the two hooks installed later sees it first. With the host started after NVDA, the host swallows the chord: an NVDA gesture assigned to it does nothing, NVDA's input help (NVDA+1) runs the hotkey instead of describing it, and "speak command keys" does not announce it. With NVDA started — or restarted — after the host, NVDA sees the chord first, as before — until the host installs its hook again, which it does whenever the machine resumes from sleep, the session is unlocked or connected to the console or to a remote client again, whenever Windows has removed the hook (see below), and whenever a window of another program comes to the front after one of the host's own windows (the hook is out of the chain while those are in front): each time, the host's hook is first again, ahead of NVDA, as for a host started after NVDA, and the log's `the keyboard hook was installed again` or `the keyboard hook is back, first in the chain` line says so. Captured keys have always depended on the order this way.

The hook runs on a thread of its own that does nothing else. It is installed with the first granted hotkey or the first capture, whichever comes first, and stays for the session — out of the chain only while one of the host's own windows is in front, and back in, first in the chain, at the next window of another program (see "Out of the chain in this application's own windows" under [`host.keys.capture`](./keys.md#host-keys-capture)); in the module manager the host's own reload and quit keys are registered at start, so there the hook is installed from the start. Every keystroke on the machine passes through it: matching is one table lookup, microseconds, and does not wait for module code, so a long callback or a synchronous OCR call on the main thread delays the hotkey's callback — it runs when the main thread is free again — but not the user's typing. Windows waits for the hook up to its `LowLevelHooksTimeout` (Microsoft documents no default; since Windows 10 1709 anything above one second counts as one second), which the hook's thread reaches only when the machine is too loaded to schedule it; Windows then passes the key on without the hook and, as documented, may remove the hook without telling it. Captured keys and the hotkeys the hook matches then stop, and hotkeys go on through `RegisterHotKey`, until the host installs the hook again: at the next resume, unlock, or console or remote connect, or once its keyboard watch has seen the hook miss three physical key-downs in a row, each pressed more than a second after its last call — see "When Windows drops the hook" under [`host.keys.capture`](./keys.md#host-keys-capture) for how that is decided, what is left out of the count and what the log says.

**`{ foreground = true }` leaves the press to Windows** (experimental). The combination is claimed with `RegisterHotKey` as every hotkey is, and **not** filed for the keyboard hook, so the hook lets the press through and Windows delivers it as `WM_HOTKEY`. That is what gives this application the right to bring a window of its own — an [HTML page](./gui.md#page-show) — to the front: a process that receives a registered hotkey has received the last input event. Whether a press the hook swallowed gives the same right, nothing primary says; the HTML probe measures both (test W2). The price is the protection the hook gives: a program that switches `WM_HOTKEY` off while it is in front (`RIDEV_NOHOTKEYS`, as WinUAE does) silences this key while it is in front, and a screen reader's keyboard hook always sees the press before Windows makes a hotkey of it, as it did for every hotkey before they were matched in the hook (below), whichever hook was installed first. The modifiers are Windows' business then, as for any registered hotkey, so no masking key is sent. Its `is now held by` line in the log ends with `foreground: delivered by Windows as WM_HOTKEY, not matched in the keyboard hook`. Once Windows has delivered it, the press goes where every hotkey's goes: the `[keys] hotkey <spec> arrived` line, then the callback, as a handler of the module, in the interactive lane; and like every hotkey it fires whatever window is in front — the module's [key scope](./keys.md#host-keys-scope) is for its captured keys. Like every press Windows delivers, one made during a stall that ended in a module's [stop](../module-runtime-and-lifecycle.md#limits) is dropped by its time stamp rather than delivered late (above).

A key captured with [`host.keys.capture`](./keys.md#host-keys-capture) by any enabled module is swallowed by the keyboard hook before the hotkey handling sees it, so a hotkey on the same combination does not fire while the hook swallows it, and nothing reports it. The hook lets a captured key through — and the hotkey then fires — while every capture of it belongs to a module scoped to another window, while a menu is open or declared open for the window in front, and while a screen reader's modifier (Insert, numpad zero, Caps Lock) is held; see [`host.keys`](./keys.md#host-keys-capture).

A combination held by another **application**
(rather than by another module) cannot be taken, and that refusal is surfaced to the user by
name, logged and answered beside the id (`taken`). The claim still stands, and it is tried again at any module's next
registration or release and at the next change to the enabled set — so quitting the application that holds the key can be
enough. The Windows shell
is such an application for the four modifiers with a letter: it holds the Office key's W, X, P,
O, T, Y, N, D and L.

### macOS {#host-hotkey-register-macos}

A Carbon event hotkey — deliberately **not** an event tap, so this needs no Input Monitoring grant, whether or not `host.keys` does (on macOS 26.6 it did not: see [macOS permissions](../macos-permissions.md#input-monitoring)). That is also what makes a hotkey the application's witness for the tap: one that arrives while the tap, on for more than a second, has seen no key at all says the tap is not handed the keyboard. It is registered by role: `Ctrl` is Command, `Alt` Option, `Shift` Shift and `Win` (`Meta`) Control, so Kontakt's `"Ctrl+L"` is Command+L here. `"Ctrl+Q"`, `"Ctrl+H"`, `"Ctrl+Tab"` and the rest of the reserved list are answered `nil, err` (`taken`).

`F21`–`F24` are answered `nil, err` here (`not_supported`, see above): macOS has no key code for them.

`{ foreground = true }` is accepted and changes nothing: a Carbon hotkey is the system's own delivery already, and there is no hook on its path.

**Quick repeats** are timed when Carbon hands the press to the application's main thread, which also carries the event tap, so a press and a captured key are timed alike, and a press made while that thread was busy is timed late. The interval is `NSEvent.doubleClickInterval`, the double-click speed of the Mac's pointer settings. A key typed between two presses ends the count only while the event tap is in — that is, while some module captures a key with [`host.keys.capture`](./keys.md#host-keys-capture): Carbon alone sees nothing but its own combinations. The tap sees a hotkey's own key-down before Carbon matches it and counts it as let through, so a press may move that count by its own key-down and no more (not checked on a Mac yet).

A chord that holds Control and Option together registers and never arrives while VoiceOver runs with its default modifier; the log says so once per chord, and [`host.keys.check`](./keys.md#host-keys-check) says so before you register. In a spec that is `Meta+Alt`, and the four-modifier chord holds it too. Ctrl+Alt without Win is Command+Option here, which is off it. A key that must be free system-wide is best Command+Shift with a function key (`"Cmd+Shift+F6"`): Command+Shift with a letter is the application-menu layer, and a hotkey there takes it from every application.

A letter is the key that types it under the current keyboard layout — with Command held, when the combination holds Command, so a layout such as "Dvorak – QWERTY ⌘", which types QWERTY while Command is down, puts Ctrl+Z (Command+Z) where its Command shortcuts are — and digits and F-keys are positions, as for every spec on macOS: see [the key spec](./keys.md#key-spec-string-format). A Carbon hotkey is registered on a key code, so when the user switches to a layout that puts the letter elsewhere, a hotkey on a letter is registered again on its new key, and the log says so. One that cannot follow — another application holds the combination on the new key, or no key types the letter now — is logged, and tried again at every later layout change and whenever its module registers it again; until then it is not held, while the host still counts it as held. A hotkey on a letter that no key of the current layout types is treated the same way from the start: `register` returns its id with an error value beside it (`not_supported`, `no key of the current keyboard layout types this letter`), the log says the hotkey is not held, and it is registered when a layout that types the letter is selected.

## host.hotkey.unregister(id) {#host-hotkey-unregister}

**Signature:** `host.hotkey.unregister(id: number)` → `nil`

Releases the OS hotkey and forgets the callback for the `id` returned by `register`. Unknown ids are ignored. The id is not checked against the calling module, so an id another module was given releases that module's hotkey; keep ids to yourself. A number with a fraction is cut to its whole part. The combination is then offered to the next standing claim on it at once, in the same call. A claim that is not held — an id that came with an error value — is given up the same way. The application's own reload and quit keys cannot be released: their ids are ignored, as an unknown id is.

```luau
if hotkeyId then host.hotkey.unregister(hotkeyId) end   -- held or only claimed, it goes
```

### Returns {#host-hotkey-unregister-returns}

Nothing, whether or not the id named a registration.

### Errors {#host-hotkey-unregister-errors}

None. It never answers an error value.

**Raises** for a mistake in the call: an `id` that is not a number — `nil`, a boolean, a table, a function, userdata, or a string that is not a numeral (a numeral string counts).

### Windows {#host-hotkey-unregister-windows}

`UnregisterHotKey`, and the combination leaves the keyboard hook's table in the same call, so from the next keystroke on neither path takes it. A press whose key is still down when its hotkey goes keeps its repeats going to the application, as with no registration.

### macOS {#host-hotkey-unregister-macos}

`UnregisterEventHotKey`. A refusal is not reported to the module; it is logged, because it leaves the combination claimed.
