---
title: "host.log — logging"
sidebar_position: 8
toc_max_heading_level: 2
---

One level, `info`, writing to `automation-platform.log` beside the application rather than to the console — a screen reader reads the focused terminal, so console output would be spoken aloud. Where exactly that is, and where the log goes when that folder cannot be written, is in the platform sections of [`info`](#host-log-info).

The same file holds the host's own lines and, marked `[dep:<library>]`, what the libraries the host is built on report (the HTTP client, TLS, ONNX Runtime, wxWidgets' Rust layer and others): their warnings and errors always, their info and debug lines only while **Detailed (trace) logging** is on in the Application settings tab. At most 20 lines per library per minute are written; the first line over that says so, and the number left out is written before that library's next line. A module's own lines are never limited this way.

**How big it gets.** The log is kept to two files of about 8 MB: `automation-platform.log` and the one before it, `automation-platform.log.1`. A file that has grown past 8 MB is moved to `.log.1` — replacing the older one — when the application starts, and also while it runs, right after the line that took it past: the application is left running for days. The new file begins with a `[host] log continued` line naming when the session started and where its earlier lines are, followed by the build, the header's `app folder` line and the settings that are on, so a file sent on its own can still be matched to its build and says where the application is. When the move is refused — another program has the file open and does not allow it, as a viewer or `Get-Content -Wait` can — the lines go on into the same file and the move is tried again after every further half megabyte, with one `could not be moved` line at each try; for as long as that program keeps the file open, the file grows past 8 MB, and no line is lost. When the new file cannot be opened, the lines until the next try — another half megabyte's worth — are lost, and the file that opens then still begins with the `log continued` lines. A [headless run](../module-manager.md#headless-mode) beside the application writes to the same file and counts only its own lines, so it may move the file first. The application's lines then go into `.log.1` until the application's own count reaches 8 MB; it then leaves the file where it is — moving it would put the headless run's new file over `.log.1` — and goes on in the new file, after a line saying the log had been moved away and the `log continued` lines. The move runs on whichever thread wrote the line that crossed 8 MB, inside that `host.log.info` call when it was one: one check that the file is still the one it writes into, one rename, one open and at most six lines written, once per 8 MB. An error a module's callback raises is written in full the first time and then counted, with a line a minute at most (see [the module runtime](../module-runtime-and-lifecycle.md)); `host.log.info` lines are written as they come.

In this project **the log is evidence rather than debugging comfort.** The tester is blind, remote, and often on the platform none of us can run, so what is not in this file did not happen as far as anyone can establish; the probe tool is little more than this one call, a single keypress writing down everything we would otherwise have to ask a person to describe.

That makes the thing worth logging the thing that is unanswerable after the fact. The overlay runtime logs the exact words it hands to speech for the one control kind whose value has been in dispute, because only the words that actually left can settle whether the code found what it claimed to.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["log"]
```

See [what that list is and is not](./index.md#capabilities).

## host.log.info(msg) {#host-log-info}

**Signature:** `host.log.info(msg: string)` → `nil`

Writes `msg` to the host log under the `module` channel. This is the **only** method on `host.log` — there is no `warn`/`error`/`debug`.

The line is written as it is, after a timestamp and `[module]`, and **without the name of the module that wrote it** — unlike the host's own error lines, which carry the module id. Every module's lines share that one channel, so a module that is one of several (the game modules built on one runtime, say) puts its own name at the front of what it logs.

```luau
host.log.info("module initialized")

-- One of several game modules on the same runtime: say which.
local function log(msg) host.log.info("[my-game] " .. msg) end
log("menu watcher started")
```

### Windows

The log is next to the `.exe`. When that folder cannot be written, it goes to
`%LOCALAPPDATA%\AutomationPlatform` instead.

### macOS

The log is in the folder that holds the `.app`. When that folder cannot be written, it goes
to `~/Library/Application Support/AutomationPlatform` instead. When macOS runs a translocated
copy of the `.app` — a download opened while it still carries its quarantine flag — it is the
folder that holds the original `.app` when macOS can say where that is; otherwise it is the
copy's folder, which cannot be written, so the log is in Application Support. See
[building on macOS](../building-on-macos.md#translocation).

A file the log continues in after 8 MB repeats the header's `translocated` line after the
build, before the `app folder` line: it says why the file is in the folder it is in, so a file
sent on its own says so too.
