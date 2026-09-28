---
title: "host.calibrating — calibration mode"
sidebar_position: 2
toc_max_heading_level: 2
---

`host.calibrating` is a boolean, true while **Calibration keys in overlays** is switched on in the Application settings tab (or the launch variable is set, for a session with no window to click in).

It exists so that a measuring instrument is not also a feature. The overlay runtime arms its calibration keys and takes its menu shots only when this is true, so a normal user never has those combinations taken away from their plug-in or pictures written behind their back; and a module gates measurements that are too expensive to run for somebody who is not measuring — a capture per selection change is an instrument, not a behaviour.

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

**The menu shots.** In a calibrating run, activating an overlay control declared with `opensMenu` saves three pictures — no crosshairs — of the overlay's origin whole: the plug-in's control when it is embedded, the window with its frame when it is standalone (the origin's `bounds`, see [`O:origin()`](overlay.md#o-origin)). The rectangle is read once, just before the control acts, and all three pictures use it, so they line up pixel for pixel; the part of a popup that reaches past it is not in them. **One screen capture is all that stands between the key and the click**: the first picture is a [snapshot](screen.md#host-screen-snapshot) taken on the press and kept. Nothing else is looked up on the press — no control's visibility or point. The later two are taken off the event loop at their time ([`snapshotAsync`](screen.md#host-screen-snapshotasync) with `at`) and each is written when it arrives; the first is written after the second of them, not straight after the click, which is when the menu's first answer, its window coming to the front and the user's first key arrive. So a key pressed while the menu opens waits for no capture and no PNG, and one pressed at about 600 or 1500 ms waits for one or two PNGs being written. The log lines come in the order the pictures are written — 600 ms, 1500 ms, then the one of the press:

| File, under `modules/overlay-runtime/calibration/` | Taken |
| --- | --- |
| `<overlay>-<control>-menu-before.png` | just before the control acts |
| `<overlay>-<control>-menu-after-600.png` | about 600 ms after |
| `<overlay>-<control>-menu-after-1500.png` | about 1500 ms after |

`<overlay>` and `<control>` are the two labels with every run of characters other than letters and digits turned into `-`. A later press of the same control is numbered — `<overlay>-<control>-2-menu-before.png`, then `-3-` — counting from the files already on disk, so nothing is overwritten, across restarts too. **Only the first two openings of each control are photographed** — per overlay, until the module is reloaded: what a menu looks like open does not change on the tenth opening, and a session of days calibrating wrote three PNGs at every one. The third opening writes one `[calibrate]` line saying the later ones are not photographed (`… menu shots were taken of its first 2 openings; later ones are not photographed until the module is reloaded`), and nothing after that; the control acts as always. Each picture writes a `[calibrate]` log line with its rectangle, where the plug-in's content begins on screen (`content at (x,y)`, the point the overlay's coordinates count from, so a pixel in the picture is at content point *picture x + left − content x*), its absolute path and `(true)`, or `(false: …)` with the reason it was not saved — a capture refused for the [snapshot budget](screen.md#host-screen-snapshot) included; a picture that fails stops neither the control nor the other two. The later two are taken whatever happened in between, including when the overlay has left the front; a press on an overlay whose window is not known writes one line saying so and no pictures. The timing is part of the measurement only: nothing in the runtime reads these pictures.

They are what a module's own [menu test](overlay.md#o-menutests) is written from. A menu a plug-in paints inside its own window has no window, no accessibility element and no notification, and the difference between the "before" picture and the "after" ones is the pixels that only an open menu has.

**The menu item shot.** A control that chooses an item in its menu ([`O:chooseMenuItem`](overlay.md#o-choosemenuitem), or a hotspot with `menuItem`) clicks the item on the pass in which a test sees the menu, so the pictures above — the later two taken 600 and 1500 ms after the press — show the menu already closed. On a press that was photographed (the first two of each control), the item is photographed as well: one [snapshot](screen.md#host-screen-snapshot) of the menu's rectangle as the test gave it, 12 pixels round, taken just before the item's first click, and written after it, on the next pass, with a crosshair on the point clicked. A click the menu did not take and that is made again is not photographed again:

| File, under `modules/overlay-runtime/calibration/` | Taken |
| --- | --- |
| `<overlay>-<control>-menu-item.png` | just before the item is first clicked |

numbered like the other pictures (`<overlay>-<control>-2-menu-item.png`, …). Its log line gives the rectangle captured, the item's point and the path: `[calibrate] '<overlay>' '<control>': menu item shot, (x1,y1)-(x2,y2), the item marked at (x,y) -> <path> (true)` — or `(false: …)` with the reason, and the click is made either way. It is the picture an item's offset is measured from: the offset is from the menu's top-left corner, which is the picture's corner plus 12.

**A scaled overlay's calibration shot** ([`O:scale`](overlay.md#o-scale)). Its first line adds the factor its crosshairs were placed with and the frame, `… scale 1.6000 about (0,0), frame (0,0) -> …`, or `no scale now (<why>)`; each control's line adds every region it reads as the screen rectangle, `region (x1,y1)-(x2,y2)` (and `ocrLabel (…)`), since the authored numbers are not where it reads. A control with no factor now has no crosshair and is listed with `no position`. In any overlay, the line of a control that chooses a menu item ends `then item (dx,dy) of its menu` (or `then an item of its menu`, for a `menuItem` function): the opener is what the crosshair marks, and the item is in the menu item shot. An opener a module's own code has asked [`O:chooseMenuItem`](overlay.md#o-choosemenuitem) to click — a stepper's, which no control's `at` names — is marked too once it has been asked for, at the `at` it was last given, after the controls, in label order: `  n <label> screen (x,y)  pixel r,g,b  [menu opener]` (or `-- <label> no position  [menu opener]` when it cannot be placed now).

```luau
-- Nothing to call: the control only has to say that it opens a menu.
ov:addHotspotButton({ label = "Preset menu", at = { 412, 118 }, hotkey = "Alt+M", opensMenu = true })
-- With calibration on, Alt+M on the overlay "Diva" writes
--   Diva-Preset-menu-menu-before.png
--   Diva-Preset-menu-menu-after-600.png
--   Diva-Preset-menu-menu-after-1500.png
-- and the next press Diva-Preset-menu-2-menu-before.png and its two companions. The log says
--   [calibrate] 'Diva' 'Preset menu': menu shot just before the control acts,
--     (120,90)-(1120,742), content at (120,90) -> C:\…\Diva-Preset-menu-menu-before.png (true)
```

### Windows

The capture before the click goes through the module's [source](screen.md#which-picture-a-read-sees): through the standard path about one compositor frame (~16.7 ms) for a plug-in-sized rectangle. Writing a PNG is the larger cost of a shot — several times larger in a debug build than in a release build — and it runs on the event loop when a picture is written: never between the key and the click, and not before the 600 ms picture has arrived. The menu item shot puts one capture of the same kind, of the menu's rectangle, between the menu being seen and the item's click, and its PNG is written on the pass after the click.

### macOS

The capture before the click is taken at the display's own resolution and kept at up to five times the bytes of the rectangle (see [`host.screen.snapshot`](screen.md#host-screen-snapshot)), for the 1.5 seconds until the last picture has arrived; the pictures of a maximised window held at once come close to a module's snapshot budget, and a capture the budget refuses is written as `(false: …)`. The menu item shot is one more capture, of the menu's rectangle only, held until the next pass writes it. Without the Screen Recording permission the pictures show the wallpaper.
