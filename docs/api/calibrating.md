---
title: "host.calibrating — measuring, not behaving"
sidebar_position: 18
toc_max_heading_level: 2
---

`host.calibrating` is a boolean, true while **Calibration keys in overlays** is switched on in the Application settings tab (or the launch variable is set, for a session with no window to click in).

It exists so that a measuring instrument is not also a feature. The overlay runtime arms its calibration keys only when this is true, so a normal user never has those combinations taken away from their plug-in; and a module gates measurements that are too expensive to run for somebody who is not measuring — a capture per selection change is an instrument, not a behaviour.

It is read **once**, when the module's VM is built, which is why that setting's label says "reload modules to apply" rather than promising to take effect immediately.

## What to declare {#declare}

Nothing — this is available to every module. See [what the capability list is and is not](./index.md#capabilities).

## host.calibrating {#host-calibrating}

**Signature:** `host.calibrating: boolean`

A value rather than a function — read it, do not call it.

```luau
-- Melodyne only pays for a capture per selection change while somebody is measuring.
if host.calibrating then
  host.keys.capture("Ctrl+Alt+M", dumpTheStripToTheLog)
end
```
