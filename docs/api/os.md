---
title: "host.os — which platform this is"
sidebar_position: 13
toc_max_heading_level: 2
---

Which operating system this is running on, for the few decisions that cannot be made declaratively.

Most platform differences should not come through here. A window matcher carries its own `windows` / `macos` blocks, and per-platform values — a key spec, an OCR region, a step size — are written with `host.os.pick`, which keeps both answers side by side in the source instead of splitting the module in two.

What is left is a genuine fork, and sforzando is the case: on macOS REAPER exposes no plug-in control to attach to, so the binding is a different one entirely — attach to the FX window, then shift the coordinate frame to where the plug-in sits inside it — and that whole branch hangs off one `host.os.is("macos")`.

A module that has simply never been tried on the other platform says so with `supported_os` in `module.toml`, where the manager can report it, rather than testing for it at run time.

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.os.current {#host-os-current}

Read-only string: the current OS, from Rust `std::env::consts::OS` (`"windows"`, `"macos"`, `"linux"`, …). Not a function — a plain field.

```luau
if host.os.current == "windows" then ... end
```

## host.os.is(name) {#host-os-is}

`host.os.is(name: string) -> boolean`

Returns `true` when `name` equals the current OS string.

```luau
if host.os.is("macos") then ... end
```

---

## host.os.pick(t) {#host-os-pick}

**Signature:** `host.os.pick(t: { windows: any?, macos: any?, linux: any? }) -> any?`

The per-platform value, or `nil` when this platform has no entry. (prelude)

This is how a module carries both answers instead of forking. A matcher already does it field by field, and this is the same idea for everything else — a key spec, a control-class pattern, a step size, an OCR region — with both platforms' values side by side in the source where the difference can be read at a glance rather than hunted for in two branches.

Returning `nil` for an absent platform is deliberate and load-bearing: an embedded binding whose `control` pattern has no entry for the running platform goes **inert** rather than matching wrongly.

```luau
-- Control+Option belongs to VoiceOver, so the Mac gets a different combination.
local KEY = host.os.pick { windows = "Ctrl+Shift+F9", macos = "Cmd+Shift+F9" }
host.hotkey.register(KEY, writeProbe)
```
