---
title: "host.os — platform detection"
sidebar_position: 10
toc_max_heading_level: 2
---

Which operating system this is running on, for the differences that are not ours to remove.

**Most platform differences should not come through here.** The host resolves the ones that come from the platform itself, so a module writes them once:

- **Your own keys** are written once when each platform's counterpart suits you. The [key spec](./keys.md#key-spec-string-format)'s modifiers are roles, so `"Ctrl+Shift+F9"` is Ctrl+Shift+F9 on Windows and Command+Shift+F9 on a Mac, and [`host.keys.describe`](./keys.md#host-keys-describe) says either one in the platform's words. Pick when you want a different combination on a Mac: when the counterpart is taken there (Command+Tab, Command+Q) or sits on VoiceOver's Control+Option layer.
- **A window** is matched by one matcher with a `windows` and a `macos` block where the identity genuinely differs (see [Matchers](./window.md#matchers)).

What is left for `pick` and `is` is a difference in the **other program** — the application or plug-in the module works on, which is a different program on each platform:

- Kontakt names its Info Pane button `Info Pane (F9)` on Windows and `Info Pane (Cmd+I)` on a Mac, so a module that finds it by name carries both names.
- sforzando lays out a few points differently in its two builds, so its OCR regions are given per platform.
- On macOS REAPER exposes no plug-in control to attach to, so sforzando's binding there is a different one entirely — attach to the FX window, then shift the coordinate frame to where the plug-in sits inside it — and that whole branch hangs off one `host.os.is("macos")`.

A module that has simply never been tried on the other platform says so with `supported_os` in `module.toml`, where the manager can report it, rather than testing for it at run time.

**No call here answers an [error value](./errors.md#error-values).** `host.os.is` raises only for an argument it cannot read, and `host.os.pick` never raises; each entry's **Errors** says so. Nothing here is gated, so the one other raise is the guard's ([Return or raise](./errors.md#return-or-raise)): a module that has run too long is stopped with `stopped by the host: …` ([the limits](../module-runtime-and-lifecycle.md#limits)).

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.os.current {#host-os-current}

**Signature:** `host.os.current: string`

Read-only string: the current OS, from Rust `std::env::consts::OS` (`"windows"`, `"macos"`, `"linux"`, …). Not a function — a plain field, set when the module's VM is built and never changed.

```luau
host.log.info("running on " .. host.os.current)
```

### Returns {#host-os-current-returns}

- The platform's name, a lower-case string: `"windows"` or `"macos"`, and on the build for other systems the name Rust gives that system, such as `"linux"`. The same in every module, and the same for the whole run.

### Windows {#host-os-current-windows}

`"windows"`, on every Windows version.

### macOS {#host-os-current-macos}

`"macos"`, on Intel and Apple silicon alike.

## host.os.is(name) {#host-os-is}

**Signature:** `host.os.is(name: string) -> boolean`

Returns `true` when `name` equals the current OS string, exactly: `"macOS"` or `"mac"` is simply `false`. It raises only for an argument that is not a string or a number. For the genuine fork, where what a module offers differs by platform — a binding, or a set of controls that one platform cannot support — not for a key or a value, which `pick` and the key spec's modifier roles cover.

```luau
-- Kontakt standalone (modules/kontakt/src/cells.luau): on a Mac the rack question has no honest
-- answer — Kontakt 7 publishes its VIEW button there while the preset browser fills the window —
-- so the controls that depend on it are not offered on that platform.
if host.os.is("macos") then
  standalone.inRack = function() return false end
  standalone.inClassic = function() return false end
end
```

### Returns {#host-os-is-returns}

- `true` — `name` is exactly [`host.os.current`](#host-os-current), in the same case.
- `false` — anything else, a number among them.

### Errors {#host-os-is-errors}

None. It never answers an error value.

**Raises** only for a `name` that is neither a string nor a number (a number is read as its digits), or a string that is not valid UTF-8.

### Windows {#host-os-is-windows}

True for `"windows"` only.

### macOS {#host-os-is-macos}

True for `"macos"` only.

---

## host.os.pick(t) {#host-os-pick}

**Signature:** `host.os.pick(t: { windows: any?, macos: any?, linux: any? }) -> any?`

The per-platform value, or `nil` when this platform has no entry. (prelude)

For a value the *other program* makes different on each platform — a control's name, a layout measured in two builds, a control-class pattern — with both answers side by side in the source where the difference can be read at a glance rather than hunted for in two branches. A table without any of the three keys is returned unchanged, so a value that does not differ need not be wrapped. It is plain table work in Luau, with no system call.

For your own keys, a `pick` is for a Mac combination that is not the counterpart of the Windows one. A key written once already becomes Command where it says `Ctrl` (see [the key spec](./keys.md#key-spec-string-format)). The overlay runtime picks its next-tab key, `{ windows = "Ctrl+Tab", macos = "Meta+Tab" }`, because Command+Tab is the Mac's application switcher.

Returning `nil` for an absent platform is deliberate and load-bearing: an embedded binding whose `control` pattern has no entry for the running platform goes **inert** rather than matching wrongly.

```luau
-- Kontakt names this button after its own shortcut, which differs between its two builds.
local INFO_PANE = host.os.pick {
  windows = "Info Pane (F9): Toggles the visibility of the information hint.",
  macos = "Info Pane (Cmd+I): Toggles the visibility of the information hint.",
}
```

### Returns {#host-os-pick-returns}

- For a table with a `windows`, `macos` or `linux` key that is not `nil`: the value under this platform's key, [`host.os.current`](#host-os-current), or `nil` when it has none.
- Anything else as it is: a table without those keys, a string, a number, `nil`.

### Errors {#host-os-pick-errors}

None. It never answers an error value.

**Raises** only for a table whose own `__index` raises for the keys it looks at, `windows`, `macos` and `linux`: an error value (`an error value has code, message and detail, not 'windows'`), `host.errors` or `O.errors`. Any other value is taken.

### Windows {#host-os-pick-windows}

Returns `t.windows`.

### macOS {#host-os-pick-macos}

Returns `t.macos`. A `control` pattern written as a plain string — a Win32 window class such as `"Qt%d+.-QWindowIcon"` — is not picked per platform at all, and on a Mac it is compared with the `AXRole/AXSubrole/AXIdentifier` triple a control's class is there, so it practically never matches. The overlay runtime logs that once per pattern that names no `AX` role and has no `/` — which also catches a real macOS pattern on the identifier alone, hence "probably" in the line; give such a `control` a `macos` entry.
