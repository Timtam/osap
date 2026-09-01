---
title: "host.log — logging"
sidebar_position: 7
toc_max_heading_level: 2
---

One level, `info`, writing to `automation-platform.log` beside the application rather than to the console — a screen reader reads the focused terminal, so console output would be spoken aloud.

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

```luau
host.log.info("module initialized")
```
