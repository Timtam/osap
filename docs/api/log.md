---
title: "host.log — logging"
sidebar_position: 8
toc_max_heading_level: 2
---

One level, `info`, writing to `automation-platform.log` beside the application rather than to the console — a screen reader reads the focused terminal, so console output would be spoken aloud. Where exactly that is, and where the log goes when that folder cannot be written, is in the platform sections of [`info`](#host-log-info).

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
to `~/Library/Application Support/AutomationPlatform` instead.
