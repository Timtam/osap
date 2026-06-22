---
title: "host.uia + host.screen — UI Automation & screen pixels/images"
sidebar_position: 2
---

Functions for querying the Windows UI Automation tree of a window and for reading screen pixels / searching for an image template on screen. All coordinates are screen pixels.

## host.uia.find(hwnd, name, controlType)

**Signature:** `host.uia.find(hwnd: number, name: string, controlType: number) -> boolean`

Returns `true` if the UI Automation subtree of the window `hwnd` contains at least one element whose Name equals `name` **and** whose ControlType equals `controlType`; otherwise `false`. Primarily used for plugin/window identity checks.

Pass an **empty `name`** (`""`) to match any element of that control type (i.e. test only the control type, ignoring the Name).

Common UIA control-type ids:

| Control | id |
| --- | --- |
| Button | `50000` |
| Edit | `50004` |
| Menu | `50009` |
| MenuItem | `50011` |
| Window | `50032` |
| Group / Pane | `50033` |

```luau
-- Is this window a Kontakt instance? (a Pane named "PlogueXMLGUI" exists)
local hwnd = host.window.focused().hwnd
if host.uia.find(hwnd, "PlogueXMLGUI", 50033) then
    host.log("Kontakt detected")
end

-- Does the window contain *any* Edit control?
if host.uia.find(hwnd, "", 50004) then
    host.log("has at least one edit box")
end
```

## host.uia.locate(hwnd, name, controlType)

**Signature:** `host.uia.locate(hwnd: number, name: string, controlType: number) -> { x: number, y: number } | nil`

Finds the first UIA element in window `hwnd` matching `name` + `controlType` and returns the screen-pixel centre of its bounding rectangle as `{ x, y }`, suitable for clicking. Returns `nil` if no match. As with `find`, an empty `name` matches any element of the given control type. Same control-type ids as above.

```luau
local hwnd = host.window.focused().hwnd
local pt = host.uia.locate(hwnd, "Play", 50000) -- Button named "Play"
if pt then
    host.input.click(pt.x, pt.y)
end
```

## host.screen.pixel(x, y)

**Signature:** `host.screen.pixel(x: number, y: number) -> { r: number, g: number, b: number, hex: string }`

Reads the colour of the screen pixel at `(x, y)`. Returns the 8-bit channels `r`, `g`, `b` (0–255) plus `hex`, an uppercase `#RRGGBB` string.

```luau
local c = host.screen.pixel(100, 200)
if c.hex == "#FF0000" then
    host.log("red pixel")
end
host.log(string.format("rgb(%d,%d,%d)", c.r, c.g, c.b))
```

## host.screen.size()

**Signature:** `host.screen.size() -> { w: number, h: number }`

Returns the primary screen dimensions in pixels as `{ w, h }`.

```luau
local s = host.screen.size()
host.log("screen is " .. s.w .. "x" .. s.h)
```

## host.screen.imageSearch(template, opts?)

**Signature:** `host.screen.imageSearch(template: string, opts: { region: Region?, tolerance: number? }?) -> { x: number, y: number, w: number, h: number } | nil`

Searches the screen for the first occurrence of the image at path `template` and returns its match rectangle in **screen pixels**: `{ x, y, w, h }`, where `x, y` is the top-left of the match and `w, h` are the template's dimensions. Returns `nil` if not found (or if the search region could not be captured).

- `template` is resolved relative to the module's root directory; **absolute paths are also accepted**.
- `opts.region` restricts the search area (see [Region form](#region-form)); default is the full primary screen.
- `opts.tolerance` is the per-channel RGB match tolerance (0–255, default `0` = exact match).
- **Template pixels with alpha = 0 are wildcards** — they are skipped during matching, so you can mask out irrelevant parts of the template.
- Raises an error if the template file cannot be opened.

```luau
-- exact, whole-screen
local m = host.screen.imageSearch("assets/play_button.png")
if m then host.input.click(m.x + m.w // 2, m.y + m.h // 2) end

-- with a search region and fuzzy match
local hit = host.screen.imageSearch("assets/icon.png", {
    region = { x1 = 0, y1 = 0, x2 = 400, y2 = 300 },
    tolerance = 16,
})
```

## Region form

Several functions (`host.screen.imageSearch`, `host.ocr.recognize`) accept an optional `region` table inside their options. A region describes an axis-aligned rectangle by its top-left and bottom-right corners and may be written in **named** or **positional** form (named keys take precedence):

- Named: `{ x1 = .., y1 = .., x2 = .., y2 = .. }`
- Positional: `{ x1, y1, x2, y2 }` (array indices `[1]`=x1, `[2]`=y1, `[3]`=x2, `[4]`=y2)

Missing coordinates default to the screen edges: `x1, y1` default to `0`; `x2, y2` default to screen width/height. The rectangle is `[x1, y1] .. [x2, y2]`; width/height are clamped to be non-negative. Omitting `region` entirely searches the full primary screen.

```luau
-- these two regions are equivalent
{ region = { x1 = 10, y1 = 20, x2 = 210, y2 = 120 } }
{ region = { 10, 20, 210, 120 } }
```

