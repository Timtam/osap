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

## host.uia.type

**Signature:** `host.uia.type: { [string]: number }`

The UIA ControlType ids by name — `host.uia.type.Button` (50000), `.Edit`, `.Menu`, `.MenuItem`, `.Tab`, `.Window` (50032), `.Pane` (50033), and the rest. Pass these instead of writing the integers at call sites.

```luau
local U = host.uia.type
if host.uia.find(hwnd, "Kontakt 8", U.Pane) then … end
```

## host.uia.findAny(hwnd, names, types)

**Signature:** `host.uia.findAny(hwnd: number, names: {string}, types: {number}) -> number | nil`

Answers "is any of these names present as any of these control types?" — the shape a plugin-identity check takes, since a plugin may expose its name as a Window on one host and a Pane on another. Returns the **1-based index of the matching name**, so a caller learns *which* one matched (e.g. the plugin's version) rather than asking once per candidate. `nil` if none match; an empty name matches any element of the given types.

One tree traversal per name, with the types folded into the condition — where a loop of `find` calls costs one full subtree walk per name×type pair. This runs on every detection pass, per candidate control, so the difference is not academic.

```luau
local U = host.uia.type
local VERSIONS = { "Kontakt 8", "Kontakt 7" }
local i = host.uia.findAny(ctrl.id, VERSIONS, { U.Window, U.Pane })
if i then host.log("this is " .. VERSIONS[i]) end
```

## host.uia.pluginLocate(hwnd, containerName, name, controlType)

**Signature:** `host.uia.pluginLocate(hwnd: number, containerName: string, name: string, controlType: number) -> { x: number, y: number } | nil`

Like `locate`, but for a plugin hosted inside another application. It first finds the element that **is** the plugin — Name `containerName` (e.g. `"Kontakt 8"`) with control type Window (50032) or Pane (50033), preferring the `ni::qt::QuickWindow` class (a Qt plugin's scene root, which carries the content) over the `…QWindowIcon` window host — and then searches for the target **within** it, using the **raw** tree walker. A port of ReaHotkey's `GetPluginUIAElement` + `MainElement.FindElement(...)`.

The raw walker matters: the condition-based search (`find` / `locate`) stops at a hosted fragment's boundary, so a plugin's UI can look entirely absent to it. Note that some plugins expose no accessible content when embedded regardless — a DAW-embedded Kontakt 8 is a UIA **leaf** — in which case this returns `nil` and only coordinates remain.

```luau
local pt = host.uia.pluginLocate(hwnd, "Kontakt 8", "Kontakt File Menu", 50000)
if pt then host.input.click(pt.x, pt.y) end
```

## host.uia.rawDump(hwnd)

**Signature:** `host.uia.rawDump(hwnd: number) -> { { depth: number, name: string, class: string, ctype: number }, … }`

Diagnostic counterpart to `host.uia.dump`, walking the **raw** tree instead of a condition-based search, so it crosses into hosted fragments the latter cannot see. Bounded by node budget and depth. Use it to find out whether a plugin exposes any accessible content at all before building on UIA.

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

## host.screen.profile(opts?)

**Signature:** `host.screen.profile(opts: { region: Region?, axes: ("both" | "columns" | "rows")? }?) -> { x, y, w, h, columns: Axis?, rows: Axis? } | nil`
where `Axis = { min: number[], max: number[], mean: number[], r: number[], g: number[], b: number[] }`

Takes **one** capture of `region` (see [Region form](#region-form); **omitted, it profiles the whole primary screen**) and reduces it, per column and per row, to: the darkest pixel, the lightest, the mean, and the mean of each colour channel. Luminance is ITU-R BT.601. `min` and `max` are whole numbers 0–255 because they are particular pixels; the means are **fractional** — a mean over hundreds of rows moves by less than one unit when something note-sized changes inside it, and rounding would floor exactly the signal you are looking for.

Index `1` is the left (or top) edge of the region, so a column index maps back as `x + i - 1`. `x`, `y`, `w`, `h` are the **requested** rectangle: nothing is clipped to the desktop, and a region that runs off the screen comes back with black in the part that is not there. Returns `nil` when the region is empty, when the capture fails, or when the capture came back shorter than its own dimensions. `axes` must be exactly `"both"`, `"columns"` or `"rows"` — anything else is an error rather than a silent `"both"`.

Use it to find something whose **position** you do not know: a vertical line is a dark column in a light band, an edge is where the mean steps, a shape's extent is where its run of changed columns begins and ends, and a differently coloured patch shows up in `r`/`g`/`b` while the luminance stays flat. For a feature much smaller than the region, prefer `min`/`max` over `mean`: a two-pixel dark line moves a column's minimum by everything and its mean by a little.

**Why this rather than many `pixel` calls:** a screen capture costs a fixed compositor frame — measured on the reference machine at 16.6 ms for 55×27 and 16.7 ms for 1028×666 — and `host.screen.pixel` costs the same 16.7 ms *for one pixel*. Four point probes are four frames; a profile of the whole window is one. The **capture** is size-independent, but this call's reduction is not: it walks every pixel and builds up to six sequences on the event loop, so profile the band you need rather than the screen. `axes` skips the half you do not want.

```luau
-- Where are the dark vertical lines in this strip?
local x, y = 100, 200
local p = host.screen.profile({ region = { x, y, x + 400, y + 20 }, axes = "columns" })
if p then
    for i, m in ipairs(p.columns.min) do
        if m < 160 then
            host.log.info("line at screen x " .. (p.x + i - 1))
        end
    end
end
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

## host.screen.imageSearchAsync(template, opts?, cb)

**Signature:** `host.screen.imageSearchAsync(template: string | {string}, opts: { region: Region?, tolerance: number?, scales: {number}? }?, cb: (hit: { x: number, y: number, w: number, h: number, n: number } | nil) -> ()) -> nil`

Like `imageSearch`, but the region capture **and** the match both run on a worker thread; `cb` fires on a later tick. Use this for anything on a detection poll — a screen capture costs about a compositor frame, and doing it inline stalls the event loop that also carries speech and key handling.

Pass a **list of templates** to try several renderings of the same thing (a dialog's close glyph as two plugin versions draw it): they are matched in order against the **same captured frame**, first hit wins, and `hit.n` reports which one matched (1-based). Chaining separate searches instead pays a fresh capture per template.

```luau
host.screen.imageSearchAsync({ "images/close-v8.png", "images/close-v7.png" },
    { region = { b.x, b.y, b.x + b.w, b.y + b.h }, tolerance = 8 },
    function(hit)
        if hit then host.input.click(hit.x + hit.w // 2, hit.y + hit.h // 2) end
    end)
```

## Region form

Several functions (`host.screen.imageSearch`, `host.screen.profile`, `host.ocr.recognize`, and each entry of `host.ocr.recognizeMany`'s `regions` list) accept a `region` table. A region describes an axis-aligned rectangle by its top-left and bottom-right corners and may be written in **named** or **positional** form (named keys take precedence):

- Named: `{ x1 = .., y1 = .., x2 = .., y2 = .. }`
- Positional: `{ x1, y1, x2, y2 }` (array indices `[1]`=x1, `[2]`=y1, `[3]`=x2, `[4]`=y2)

Missing coordinates default to the screen edges: `x1, y1` default to `0`; `x2, y2` default to screen width/height. The rectangle is `[x1, y1] .. [x2, y2]`; width/height are clamped to be non-negative. Omitting `region` entirely searches the full primary screen.

```luau
-- these two regions are equivalent
{ region = { x1 = 10, y1 = 20, x2 = 210, y2 = 120 } }
{ region = { 10, 20, 210, 120 } }
```

