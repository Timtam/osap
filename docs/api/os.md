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

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.os.current {#host-os-current}

**Signature:** `host.os.current: string`

Read-only string: the current OS, from Rust `std::env::consts::OS` (`"windows"`, `"macos"`, `"linux"`, …). Not a function — a plain field, set when the module's VM is built and never changed.

```luau
host.log.info("running on " .. host.os.current)
```

### Windows

`"windows"`, on every Windows version.

### macOS

`"macos"`, on Intel and Apple silicon alike.

## host.os.is(name) {#host-os-is}

**Signature:** `host.os.is(name: string) -> boolean`

Returns `true` when `name` equals the current OS string, exactly: `"macOS"` or `"mac"` is simply `false`. It raises only for an argument that is not a string or a number. For the genuine fork, where a whole binding differs — not for a key or a value, which `pick` and the key spec's modifier roles cover.

```luau
-- sforzando: REAPER on macOS publishes no plug-in control, so the Mac gets a binding of its
-- own, against the FX window, with the frame moved to where the plug-in sits inside it.
if host.os.is("macos") then
  local inDaw = O.new("sforzando in a DAW")
  inDaw:frame(function(o)
    local p = daw.reaperPluginOrigin(o)
    return p[1], p[2]
  end)
  inDaw:attach({
    title = { contains = "sforzando" },
    macos = { app = { bundleId = "com.cockos.reaper" }, axSubrole = "AXStandardWindow" },
  })
end
```

### Windows

True for `"windows"` only.

### macOS

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

### Windows

Returns `t.windows`.

### macOS

Returns `t.macos`. A `control` pattern written as a plain string — a Win32 window class such as `"Qt%d+.-QWindowIcon"` — is not picked per platform at all, and on a Mac it is compared with the `AXRole/AXSubrole/AXIdentifier` triple a control's class is there, so it practically never matches. The overlay runtime logs that once per pattern that names no `AX` role and has no `/` — which also catches a real macOS pattern on the identifier alone, hence "probably" in the line; give such a `control` a `macos` entry.
