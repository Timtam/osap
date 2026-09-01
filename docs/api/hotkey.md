---
title: "host.hotkey — a key claimed system-wide"
sidebar_position: 8
toc_max_heading_level: 2
---

A hotkey is claimed from the operating system and fires wherever the user is, whether or not the plug-in is in front, which makes it the way *in* rather than the way around: the DAW module puts the keyboard back into a plug-in window from anywhere with one, and the probe dumps the focused window with another. The overlay also hangs one on individual controls — Kontakt's view toggle answers to Alt+V — and speaks the key when the control is focused, so a plug-in with an unreadable interface teaches its own shortcuts.

**The scarcity is what to plan for.** A combination can be held by only one claimant on the whole machine and the OS refuses a second claim, so the overlay takes a spec only while a control that uses it is actually visible, and decides which control a press belongs to at press time rather than at registration.

A claim also outranks the application, which is a hazard as much as a feature: with Kontakt's file or snapshot menu open, Alt+P and Alt+M never reached the menu at all until the overlay learned to give its registrations back for as long as a menu is up. And the callback runs while the combination is still physically down, so a key or click synthesised inside it carries those modifiers unless you wait for them to be released.

The spec grammar is shared with `host.keys` and documented there, including the `"<modifier> tap"` form, which is a capture only and is refused here.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["hotkey"]
```

See [what that list is and is not](./index.md#capabilities).

## host.hotkey.register(spec, callback) {#host-hotkey-register}

**Signature:** `host.hotkey.register(spec: string, callback: () -> ()) ` → `id: number`

Registers a **global** OS hotkey (active regardless of foreground window) for the [key spec](keys#key-spec-string-format) and returns an integer `id`. The callback is invoked with no arguments each time the hotkey fires (only while the owning module is enabled). Raises an error if the spec is invalid or the OS refuses the registration (e.g. already taken).

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
