---
title: "host.hotkey — system-wide hotkeys"
sidebar_position: 4
toc_max_heading_level: 2
---

A hotkey is claimed from the operating system and fires wherever the user is, whether or not the plug-in is in front, which makes it the way *in* rather than the way around: the DAW module puts the keyboard back into a plug-in window from anywhere with one, and the probe dumps the focused window with another. The overlay also hangs one on individual controls — Kontakt's view toggle answers to Alt+V — and speaks the key when the control is focused, so a plug-in with an unreadable interface teaches its own shortcuts.

**The scarcity is what to plan for.** A combination can be held by only one claimant on the whole machine and the OS refuses a second claim, so the overlay takes a spec only while a control that uses it is actually visible, and decides which control a press belongs to at press time rather than at registration.

A claim also outranks the application, which is a hazard as much as a feature: with Kontakt's file or snapshot menu open, Alt+P and Alt+M never reached the menu at all until the overlay learned to give its registrations back for as long as a menu is up. And the callback runs while the combination is still physically down, so a key or click synthesised inside it carries those modifiers unless you wait for them to be released.

The spec grammar is shared with `host.keys` and documented there, including the `"<modifier> tap"` form, which is a capture only and can never be held here (see below).

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

It raises only for a spec the shared parser cannot read at all (an unknown modifier or key name). A spec that parses but that the operating system cannot register — a `"<modifier> tap"`, or a key the platform has no code for (see the platform sections) — does **not** raise: `register` returns an `id`, the refusal happens when the claim is made, and it reaches the user as the "Binding unavailable" dialog, whose text blames another application that already uses the combination. The real reason is only in the log. The claim is tried again, and refused again, whenever the set of enabled modules changes.

```luau
local id = host.hotkey.register("Ctrl+Alt+P", function()
  host.speech.output("Hotkey pressed")
end)
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

`RegisterHotKey`, with auto-repeat suppressed: the callback fires once per press, and holding the combination does not fire it again.

A key captured with [`host.keys.capture`](./keys.md#host-keys-capture) by any enabled module is swallowed by the keyboard hook before the hotkey handling sees it, so a hotkey on the same combination does not fire while the hook swallows it, and nothing reports it. The hook lets a captured key through — and the hotkey then fires — while the capture scope pins another window, while a menu is open or declared open, and while a screen reader's modifier (Insert, numpad zero, Caps Lock) is held; see [`host.keys`](./keys.md#host-keys-capture).

A combination held by another **application**
(rather than by another module) cannot be taken, and that refusal is surfaced to the user by
name and logged. The claim still stands, and it is tried again on the next change to the
enabled set — so quitting the application that holds the key can be enough, where it used to
take a restart of this one.

### macOS

A Carbon event hotkey — deliberately **not** an event tap, so this needs no Input Monitoring grant even though `host.keys` does.

A `"<modifier> tap"` spec is refused when the claim is made, with a log line pointing at `host.keys`: Carbon has no notion of a bare modifier press, and registering something plausible instead would fire on the wrong key. Windows refuses it too, but only incidentally — its hotkey parser has no tap branch at all, so the log reads as an unknown key rather than as an explanation. On both, `register` itself has returned an `id` by then (see above). `F21`–`F24` are refused the same way here, because macOS has no key code for them.

Letters, digits and F-keys are US keyboard positions here, as for every spec on macOS: see [the key spec](./keys.md#key-spec-string-format).

## host.hotkey.unregister(id) {#host-hotkey-unregister}

**Signature:** `host.hotkey.unregister(id: number)` → `nil`

Releases the OS hotkey and forgets the callback for the `id` returned by `register`. Unknown ids are ignored.

```luau
host.hotkey.unregister(id)
```

---

The `host.keys` namespace is a low-level, modifier-aware keyboard hook that **intercepts and suppresses** individual keystrokes (the key does not reach the focused application) and delivers them to your callback. It is distinct from `host.hotkey`: keys are matched on exact modifier state and the captured set is recomputed across all enabled modules whenever it changes.
