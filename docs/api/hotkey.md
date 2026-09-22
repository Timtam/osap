---
title: "host.hotkey — system-wide hotkeys"
sidebar_position: 4
toc_max_heading_level: 2
---

A hotkey is claimed from the operating system and fires wherever the user is, whether or not the plug-in is in front, which makes it the way *in* rather than the way around: the DAW module puts the keyboard back into a plug-in window from anywhere with one, and the probe dumps the focused window with another. The overlay also hangs one on individual controls — Kontakt's view toggle answers to Alt+V — and speaks the key when the control is focused, in the platform's own words ([`host.keys.describe`](./keys.md#host-keys-describe): Option+V on a Mac), so a plug-in with an unreadable interface teaches its own shortcuts.

**The scarcity is what to plan for.** A combination can be held by only one claimant on the whole machine and the OS refuses a second claim, so the overlay takes a spec only while a control that uses it is actually visible, and decides which control a press belongs to at press time rather than at registration.

A claim also outranks the application, which is a hazard as much as a feature: with Kontakt's file or snapshot menu open, Alt+P and Alt+M never reached the menu at all until the overlay learned to give its registrations back for as long as a menu is up. And the callback runs while the combination is still physically down, so a key or click synthesised inside it carries those modifiers unless you wait for them to be released.

The spec grammar is shared with `host.keys` and documented there. That includes the modifier roles: `Ctrl` is Command on a Mac and `Win` (`Meta`) is Control, so `"Ctrl+Shift+F9"` is Command+Shift+F9 there. It also includes the `"<modifier> tap"` form, which is a capture only and can never be held here (see below). A module that wants another combination on a Mac picks one with [`host.os.pick`](./os.md#host-os-pick). The key that puts the keyboard back into a plug-in window is Ctrl+Shift+Win+Alt+F6 on Windows and `Cmd+Shift+F6` on a Mac, because the four-modifier chord holds Control and Option on a Mac, which is VoiceOver's layer. [`host.keys.check`](./keys.md#host-keys-check) says before you register what the platform will do with a combination.

A combination that is also captured with [`host.keys.capture`](./keys.md#host-keys-capture) is a platform question: see the Windows section of [`register`](#host-hotkey-register).

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["hotkey"]
```

See [what that list is and is not](./index.md#capabilities).

## host.hotkey.register(spec, callback) {#host-hotkey-register}

**Signature:** `host.hotkey.register(spec: string, callback: () -> ()) ` → `id: number`

Registers a **global** OS hotkey (active regardless of foreground window) for the [key spec](keys#key-spec-string-format) and returns an integer `id`. The callback is invoked with no arguments each time the hotkey fires (only while the owning module is enabled); what a held-down combination does is in the platform sections.

It raises, at once, for every spec the operating system could never hold — each a module author's to fix, and each with a message that says why:

- a spec the shared parser cannot read at all (an unknown modifier or key name; the message names the part);
- a `"<modifier> tap"`, which is something [`host.keys.capture`](./keys.md#host-keys-capture) watches and no system registers;
- a key the platform has no code for: `F21`–`F24` on macOS;
- a combination **the system keeps for itself** — the `reserved` reason of [`host.keys.check`](./keys.md#host-keys-check): F12 and Win+L on Windows; Command+F5, Command+Q, Command+Tab and the rest of the list there on macOS. The message names the combination and what the system uses it for. Carbon would register Command+Q without complaint and take quitting away from every application for as long as the module held it, which is why this is a refusal and not a warning.

These are exactly the `check` reasons marked "raises" in its table. Nothing else `check` can report is refused: a chord on VoiceOver's layer, or one that also types a character, registers as asked. A combination that another **application** holds is not known until the claim is made, so it does not raise; see the platform sections.

The host records, logs and reports a registration by its [normalized](./keys.md#host-keys-normalize) spec, so the log names the chord it is on this platform: `Ctrl+Shift+Win+Alt+F6` appears as `Ctrl+Alt+Shift+Win+F6` on Windows, and `Cmd+Shift+F6` as `Shift+Cmd+F6` on a Mac. The dialogs use the same form on Windows. On a Mac they say the key in [`describe`](./keys.md#host-keys-describe)'s spoken words, `Shift+Command+F6`, because the spec's word for the Control key there, `Meta`, is not the key's name.

```luau
-- Written once: Ctrl+Shift+F9 on Windows, Command+Shift+F9 on a Mac.
local KEY = "Ctrl+Shift+F9"
local id = host.hotkey.register(KEY, function()
  host.speech.output("Hotkey pressed")
end)
host.log.info(host.keys.describe(KEY, { style = "short" }) .. " is registered as " .. id)
```

### A registration is a claim, not a guarantee

Only one module can hold a combination at a time, and a valid spec that somebody else is
already holding does **not** raise. It is recorded as a standing claim, you get a real `id`
back, and your callback simply does not fire while the other holder has it. The user is told
which two modules want the same key.

The claim is honoured the moment the combination becomes free — the other module being
disabled in the module manager, uninstalled, or releasing the key with
[`unregister`](#host-hotkey-unregister). Nothing needs restarting, and there is nothing for
you to retry: the runtime recomputes who holds what whenever the set of enabled modules
changes. Between enabled modules the one that **loaded first** keeps the combination, and within a
single module its own earliest registration does. Load order rather than raw registration
order, so that reloading a module does not cost it its own key: it comes back with a fresh
registration that would otherwise look like the newest claim on the combination.

The practical consequence for an overlay: **do not announce a hotkey as available just
because `register` returned an id.** It returns one either way.

### Windows

`RegisterHotKey`, with auto-repeat suppressed: the callback fires once per press, and holding the combination does not fire it again. Each role is the key of its name: `Ctrl` is Control and `Win` the Windows key.

A hotkey on `Ctrl+Alt+<key>` is `AltGr+<key>` to Windows, so it takes that key's AltGr character away from every application for as long as it is held (the `altgr` reason of [`host.keys.check`](./keys.md#host-keys-check)).

**A granted hotkey is matched in the keyboard hook as well**, the way AutoHotkey's `#UseHook` works. A program can switch registered hotkeys off for everybody while it is in front — WinUAE registers its keyboard with `RIDEV_NOHOTKEYS`, and then no program's `RegisterHotKey` fires, whatever the combination — while the host's low-level keyboard hook still sees the keys. So the hook swallows a press of a combination Windows granted and dispatches it itself. `RegisterHotKey` still claims the combination and still delivers the press whenever the hook does not: while an elevated window is in front (the hook of an ordinary process is not called for input going to it), while a screen reader's modifier (Insert, numpad zero, Caps Lock) is held — the hook lets those presses through, as it does for captured keys — when the key was already down before the rest of the combination was, and after Windows has removed a hook that timed out. A combination Windows **refused** is never taken through the hook: it belongs to another program, whose key would silently stop working.

A press reaches the callback once. Auto-repeat is swallowed without firing, as `MOD_NOREPEAT` does. A key-down counts as a repeat when the same key went down less than 1.2 seconds before with no key-up between — longer when the keyboard's repeat delay or, with FilterKeys on, its delay or repeat interval leaves longer gaps; the settings are read again with every registration, and the log says when they change the figure. A key already held when the modifiers arrive does not fire through the hook. When the hook answered too late — its thread was not scheduled within the system's timeout, so Windows passed the key on and `RegisterHotKey` delivered it as well — the two deliveries are paired by their timestamps and the second is dropped, with a line in the log; a late call is judged by the modifiers held when its key went down, not when the hook got to it. A press that came through `RegisterHotKey` although the hook should have seen it is logged once per window in front, with its possible causes: an elevated window, a hook running late, a hook Windows removed.

The application in front sees what it saw from a registered hotkey — the key-up, not the key-down — and one thing more: when the combination holds Alt or Win, or Ctrl and Shift together, the host presses and releases the unassigned virtual key 0xE8 while the modifiers are still down. Without it, letting go of Alt after a swallowed key opens the window's menu bar, letting go of Win opens Start, and Alt+Shift or Ctrl+Shift switch the input language. AutoHotkey masks its hook hotkeys the same way. The capture scope and the menu rules of `host.keys` do not apply: a hotkey fires whatever window is in front and whatever menu is open, as before.

**A screen reader's gesture on the same chord may lose to it.** Windows calls low-level keyboard hooks newest first. Before hotkeys were matched in the hook, a screen reader's hook always saw a registered chord before the hotkey did; now the one of the two hooks installed later sees it first. With the host started after NVDA, the host swallows the chord: an NVDA gesture assigned to it does nothing, NVDA's input help (NVDA+1) runs the hotkey instead of describing it, and "speak command keys" does not announce it. With NVDA started — or restarted — after the host, NVDA sees the chord first, as before. Captured keys have always depended on the order this way.

The hook runs on a thread of its own that does nothing else. It is installed with the first granted hotkey or the first capture, whichever comes first, and stays for the session; in the module manager the host's own reload key is registered at start, so there the hook is always present. Every keystroke on the machine passes through it: matching is one table lookup, microseconds, and does not wait for module code, so a long callback or a synchronous OCR call on the main thread delays the hotkey's callback — it runs when the main thread is free again — but not the user's typing. Windows waits for the hook up to its `LowLevelHooksTimeout` (Microsoft documents no default; since Windows 10 1709 anything above one second counts as one second), which the hook's thread reaches only when the machine is too loaded to schedule it; Windows then passes the key on without the hook and, as documented, may remove the hook without telling it. Captured keys then stop until a restart; hotkeys go on through `RegisterHotKey`.

A key captured with [`host.keys.capture`](./keys.md#host-keys-capture) by any enabled module is swallowed by the keyboard hook before the hotkey handling sees it, so a hotkey on the same combination does not fire while the hook swallows it, and nothing reports it. The hook lets a captured key through — and the hotkey then fires — while the capture scope pins another window, while a menu is open or declared open, and while a screen reader's modifier (Insert, numpad zero, Caps Lock) is held; see [`host.keys`](./keys.md#host-keys-capture).

A combination held by another **application**
(rather than by another module) cannot be taken, and that refusal is surfaced to the user by
name and logged. The claim still stands, and it is tried again on the next change to the
enabled set — so quitting the application that holds the key can be enough. The Windows shell
is such an application for the four modifiers with a letter: it holds the Office key's W, X, P,
O, T, Y, N, D and L.

### macOS

A Carbon event hotkey — deliberately **not** an event tap, so this needs no Input Monitoring grant even though `host.keys` does. It is registered by role: `Ctrl` is Command, `Alt` Option, `Shift` Shift and `Win` (`Meta`) Control, so Kontakt's `"Ctrl+L"` is Command+L here. `"Ctrl+Q"`, `"Ctrl+H"`, `"Ctrl+Tab"` and the rest of the reserved list raise.

`F21`–`F24` raise here (see above): macOS has no key code for them.

A chord that holds Control and Option together registers and never arrives while VoiceOver runs with its default modifier; the log says so once per chord, and [`host.keys.check`](./keys.md#host-keys-check) says so before you register. In a spec that is `Meta+Alt`, and the four-modifier chord holds it too. Ctrl+Alt without Win is Command+Option here, which is off it. A key that must be free system-wide is best Command+Shift with a function key (`"Cmd+Shift+F6"`): Command+Shift with a letter is the application-menu layer, and a hotkey there takes it from every application.

A letter is the key that types it under the current keyboard layout — with Command held, when the combination holds Command, so a layout such as "Dvorak – QWERTY ⌘", which types QWERTY while Command is down, puts Ctrl+Z (Command+Z) where its Command shortcuts are — and digits and F-keys are positions, as for every spec on macOS: see [the key spec](./keys.md#key-spec-string-format). A Carbon hotkey is registered on a key code, so when the user switches to a layout that puts the letter elsewhere, a hotkey on a letter is registered again on its new key, and the log says so. One that cannot follow — another application holds the combination on the new key, or no key types the letter now — is logged, and tried again at every later layout change and whenever its module registers it again; until then it is not held, while the host still counts it as held. A hotkey on a letter that no key of the current layout types is treated the same way from the start: `register` returns its id and does not raise, the log says the hotkey is not held, and it is registered when a layout that types the letter is selected.

## host.hotkey.unregister(id) {#host-hotkey-unregister}

**Signature:** `host.hotkey.unregister(id: number)` → `nil`

Releases the OS hotkey and forgets the callback for the `id` returned by `register`. Unknown ids are ignored.

```luau
host.hotkey.unregister(id)
```

### Windows

`UnregisterHotKey`, and the combination leaves the keyboard hook's table in the same call, so from the next keystroke on neither path takes it. A press whose key is still down when its hotkey goes keeps its repeats going to the application, as with no registration.

### macOS

`UnregisterEventHotKey`. A refusal is not reported to the module; it is logged, because it leaves the combination claimed.

---

The `host.keys` namespace is a low-level, modifier-aware keyboard hook that **intercepts and suppresses** individual keystrokes (the key does not reach the focused application) and delivers them to your callback. It is distinct from `host.hotkey`: keys are matched on exact modifier state and the captured set is recomputed across all enabled modules whenever it changes.
