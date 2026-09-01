---
title: "host.screen — what the plug-in looks like"
sidebar_position: 3
toc_max_heading_level: 2
---

What the plug-in looks like, for everything it will not say. This is where a module goes when there is no element to ask: state that exists only as colour — Kontakt reads whether its snapshot bar is open from one pixel on the camera icon, and ON:EAR classifies each of its switches from a single pixel — and anything whose position moves, which is what image search is for, since Kontakt's five instrument-editor sections are found by their captions because opening one pushes the rest down the window.

**Every touch of the screen costs a compositor frame**, measured at about 16.7 ms whether it reads one pixel or a whole window. So `pixel` is not the cheap call it looks like: read a point once per pump and let everything that asks about it share the answer. When the question is *where* something is rather than what colour a known point has, `profile` reduces one capture to a value per column and row — the way Melodyne finds a selected note by diffing two column profiles.

The matching, unlike the capture, does grow with the area. A template search across a whole plug-in window is the slow call here — measured at twelve seconds for one full-region match while twelve library overlays polled the same window every 500 ms — which is why anything on a detection poll uses `imageSearchAsync` on its worker thread and re-searches a small box around the last hit before widening.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["screen"]
```

See [what that list is and is not](./index.md#capabilities).

## host.screen.pixel(x, y) {#host-screen-pixel}

**Signature:** `host.screen.pixel(x: number, y: number) -> { r: number, g: number, b: number, hex: string }`

Reads the colour of the screen pixel at `(x, y)`. Returns the 8-bit channels `r`, `g`, `b` (0–255) plus `hex`, an uppercase `#RRGGBB` string.

```luau
local c = host.screen.pixel(100, 200)
if c.hex == "#FF0000" then
    host.log.info("red pixel")
end
host.log(string.format("rgb(%d,%d,%d)", c.r, c.g, c.b))
```

### Windows

Reads the pixel straight from the screen. What you get is what is there.

### macOS

Captures a small area around the point and downsamples it, so the value is a **box average of the backing pixels** rather than one of them. On a Retina display an exact comparison — `c.hex == "#FF0000"` — can therefore fail on a colour that is genuinely there, at a boundary or on a thin line. Compare with a tolerance, or read a point well inside a flat area.

## host.screen.size() {#host-screen-size}

**Signature:** `host.screen.size() -> { w: number, h: number }`

Returns the primary screen dimensions in pixels as `{ w, h }`.

```luau
local s = host.screen.size()
host.log.info("screen is " .. s.w .. "x" .. s.h)
```

### Windows

**Device pixels.** The application declares per-monitor DPI awareness, so a 4K panel at 200% scaling reports 3840x2160 and every coordinate the platform takes or returns is one physical pixel.

### macOS

**Points.** The same panel reports 1920x1080 on a Retina Mac; a capture is downsampled so that one pixel of a captured image is one point, and no coordinate can address a single backing pixel.

This is the difference most likely to waste a day. Coordinates and template images calibrated on a HiDPI Windows machine are exactly **twice** the numbers a Retina Mac needs, and nothing reports it — the click simply lands half a screen away.

## host.screen.profile(opts?) {#host-screen-profile}

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

## host.screen.imageSearch(template, opts?) {#host-screen-imagesearch}

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

### macOS

Without the Screen Recording permission, macOS does **not** fail a capture — it hands back a picture of the desktop wallpaper. So a missing grant does not show up as an error or an empty result; it shows up as a search that never matches, or worse, matches something that was never on the plug-in. If every image search suddenly stops matching on a Mac, check the grant before the template.

## host.screen.imageSearchAsync(template, opts?, cb) {#host-screen-imagesearchasync}

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

## Region form {#region-form}

Several functions (`host.screen.imageSearch`, `host.screen.profile`, `host.ocr.recognize`, and each entry of `host.ocr.recognizeMany`'s `regions` list) accept a `region` table. A region describes an axis-aligned rectangle by its top-left and bottom-right corners and may be written in **named** or **positional** form (named keys take precedence):

- Named: `{ x1 = .., y1 = .., x2 = .., y2 = .. }`
- Positional: `{ x1, y1, x2, y2 }` (array indices `[1]`=x1, `[2]`=y1, `[3]`=x2, `[4]`=y2)

Missing coordinates default to the screen edges: `x1, y1` default to `0`; `x2, y2` default to screen width/height. The rectangle is `[x1, y1] .. [x2, y2]`; width/height are clamped to be non-negative. Omitting `region` entirely searches the full primary screen.

```luau
-- these two regions are equivalent
{ region = { x1 = 10, y1 = 20, x2 = 210, y2 = 120 } }
{ region = { 10, 20, 210, 120 } }
```

## host.screen.save(path, opts?) {#host-screen-save}

**Signature:** `host.screen.save(path: string, opts: { region: Region? }?) -> boolean`

Captures `opts.region` (see [Region form](#region-form); omitted, the whole primary screen) and writes it to `path`; the format follows the file extension, so give it `.png`. This is the evidence call: an author who cannot see the screen has no way to check that a coordinate lands on its button, so the picture goes to somebody who can look at it — `tools/probe` writes one shot of the window beside its text dump for exactly that, and the overlay runtime's calibration crop key (`Ctrl+Alt+Shift+T`) uses it to cut a fresh template out of the live plug-in when an old one stopped matching. It costs one capture, so about one compositor frame (~16.7 ms on Windows; the macOS round trip is unmeasured) regardless of the region's size, plus the PNG encode.

`path` is resolved under the calling module's root; an absolute path (what `host.path` returns) is used as it stands, and the parent directory is created if it does not exist. Returns `true` on success and `false` when the region could not be captured; a path that cannot be *written* raises instead of returning `false`, so a typo in the folder name is not mistaken for a failed capture.

```luau
-- tools/probe: the one artefact a sighted helper can check in seconds, saved next to
-- the numbers it is supposed to confirm.
local win = host.window.active()
if not win then
  host.log.info("probe: nothing is focused, so there is nothing to capture")
else
  local shot = "probe.png"
  local saved = host.screen.save(shot, {
    region = { win.bounds.x, win.bounds.y, win.bounds.x + win.bounds.w, win.bounds.y + win.bounds.h },
  })
  host.log.info(string.format(
    "capture: %s (%dx%d requested)", saved and shot or "FAILED", win.bounds.w, win.bounds.h
  ))
end
```

### macOS

Without the Screen Recording permission the capture does **not** fail — macOS hands back a picture of the desktop wallpaper. A saved shot is therefore the cheapest way to tell a missing grant from a wrong rectangle: if the PNG shows the wallpaper, no coordinate in the module is at fault. A template cut around the same on-screen element on a Retina Mac also comes back at point resolution, i.e. half the pixel dimensions of the same crop taken on a HiDPI Windows machine (see [`host.screen.size`](#host-screen-size)).

## host.screen.saveMarked(path, opts) {#host-screen-savemarked}

**Signature:** `host.screen.saveMarked(path: string, opts: { region: Region?, marks: { { x: number, y: number } }? }) -> boolean`

Everything `save` does, plus a magenta crosshair drawn at every point in `opts.marks`, given in **screen** coordinates. It puts the calibration question into one picture — is my control on its button? — instead of leaving somebody to read coordinates out of a log and find that spot in a plain screenshot by hand, which is the step that hides errors: a set of toggles in this repo sat 16 px off, on the caption row under the buttons, for as long as it existed, because the colours sampled there happened to look plausible. Magenta because these dark plug-in UIs do not use it, so ink cannot be mistaken for interface. Each crosshair is 19 px across, and the *n*th mark gets *n* dots on a row 12 px below its centre, so marks stay tellable apart without any text.

Four things bite. `opts` is **required** here (`save`'s is optional) — pass at least `{}`. Mark entries are read by **name**: `{ x = 100, y = 200 }`, never positional `{ 100, 200 }`, which reads as screen `(0, 0)`: dropped when the captured region does not reach the screen origin, and otherwise drawn in the corner, which is worse — a crosshair that means nothing. A mark outside the captured region is skipped without a word, so grow the region to cover the marks the way `calibrationShot` does — Komplete Kontrol's standalone menu bar sits 10 px above the client top, and a shot clipped to the client rectangle cut every crosshair off, producing a picture with no marks at all, which reads as "nothing was marked" rather than "the marks are off-frame". And the ink lands on the very thing the mark points at, so when you also need to *look* at those pixels, take the same frame twice.

```luau
-- overlay-runtime's calibration shot (Ctrl+Alt+Shift+S): one crosshair per VISIBLE control
-- — a hidden one gets none, because marking it would claim the user can reach it.
local o = self:origin()
local region = { o.client.x, o.client.y, o.client.x + o.bounds.w, o.client.y + o.bounds.h }
local stem = "kontakt"
local marks = {}
for _, c in ipairs(self.controls) do
  if isVisible(self, c) then
    local x, y = calibrationPoint(self, c)
    if x then marks[#marks + 1] = { x = x, y = y } end
  end
end
host.screen.saveMarked(host.path("calibration/" .. stem .. ".png"), { region = region, marks = marks })
-- The SAME frame without the ink: measuring why a slider's thumb stopped matching meant
-- reading its knob through 32 pixels of magenta. One more capture of a known region.
host.screen.saveMarked(host.path("calibration/" .. stem .. "-clean.png"), { region = region, marks = {} })
```

### macOS

The capture is `save`'s, so the wallpaper-instead-of-error case applies here too — and it is nastier with marks on it, because crosshairs drawn over a picture of the desktop look like a considered answer rather than a missing permission. Check that the shot shows the plug-in before reading anything into where the marks landed.

## host.screen.imageSearchAll(template, opts?) {#host-screen-imagesearchall}

**Signature:** `host.screen.imageSearchAll(template: string, opts: { region: Region?, tolerance: number? }?) -> { { x: number, y: number, w: number, h: number } }`

Returns **every** match of `template` in the region, as an array of rectangles in screen pixels, rather than stopping at the first one like `imageSearch`. The list is empty when there is no match — and also when the region could not be captured; it is never `nil`. Its use is deciding whether a template is safe to click blindly: a close-glyph template that matches twice will eventually click the wrong one, and a template that matches nowhere is a control that silently never fires. "Matched exactly once, here" is the answer you want before shipping either, which is why the overlay runtime keeps it on a calibration key rather than in any live path.

It cannot stop early, so it always pays the **full** scan of the region, synchronously on the event loop — and matching, unlike the capture, grows with the area; there is no async form of this call. Two further limits are worth knowing before you read a count. It honours `tolerance` but **not** `scales`: the search runs at the template's native size only, so a control your overlay matches through a scale ladder can verify as zero hits here without anything being wrong with it. And de-duplication is per row — after a hit the scan resumes past that hit's right edge on the same row, but the next row starts again from the left, so a slack tolerance can report one button once per row it survives on.

```luau
-- overlay-runtime, Ctrl+Alt+Shift+V: is the focused control's template unique in the window?
local hits = host.screen.imageSearchAll(image, {
  region = { o.client.x, o.client.y, o.client.x + o.bounds.w, o.client.y + o.bounds.h },
  tolerance = 8,
})
host.log.info(string.format("%s: %d match(es) for %s", tostring(c.label), #hits, tostring(image)))
for i, h in ipairs(hits) do
  host.log.info(string.format("  %d at (%d,%d) %dx%d", i, h.x, h.y, h.w, h.h))
end
```

## host.screen.imageSearchMulti(templates, opts?) {#host-screen-imagesearchmulti}

**Signature:** `host.screen.imageSearchMulti(templates: {string}, opts: { region: Region?, tolerance: number?, scales: {number}? }?) -> (number, { x: number, y: number, w: number, h: number }) | (nil, nil)`

Captures the region **once** and tries each template path against that one frame, returning two values: the 1-based index of the first template that matched, and its rectangle in screen pixels. Reach for it whenever one question needs more than one template — a toggle's on/off pair, or a glyph as two plug-in versions draw it. The capture is the fixed cost (about a compositor frame on Windows, whatever its size); over a small region the extra comparisons are the cheap part, which is what makes one call beat two. The index is what turns a hit back into meaning: build the list in a known order and keep a parallel list of labels.

Sharing the frame also buys correctness, not only time: both templates are matched against the same pixels, so a toggle that changes mid-repaint cannot fall between two separate captures and come back as neither. Templates are tried in list order and the whole region is scanned for the first before the second is tried, so when both would match, order decides and not position. Decoded templates are cached by path and modification time, shared with `imageSearch` and `imageSearchAsync`, so a re-captured template is picked up live and a repeated poll does not re-decode PNGs. A template file that cannot be opened raises, as with `imageSearch` — but the load happens inside the loop and after the capture, so a bad path late in the list stays dormant until an earlier template fails to match. A failed capture returns `(nil, nil)` as well, which is indistinguishable from "no template matched": for a state read like `gtoggleState` that is the difference between "off" and "I could not look". This call is synchronous — for anything on a detection poll use [`imageSearchAsync`](#host-screen-imagesearchasync), which takes a template list for the same reason and moves the capture off the event loop.

```luau
-- overlay-runtime's gtoggleState: read a graphical toggle. The region is touched ONCE,
-- not once per template, and the hit's index maps straight back to the on/off label.
local templates, labels = {}, {}
addTemplates(templates, labels, c.onImage, "on")   -- appends each path with its label
addTemplates(templates, labels, c.offImage, "off")
if #templates == 0 then return nil end
local idx = host.screen.imageSearchMulti(templates, {
  region = region,
  tolerance = c.tolerance or MATCH_TOLERANCE,
  scales = c.scales or MATCH_SCALES,
})
return idx and labels[idx] or nil

-- Ignore the index and take the second return when you only need WHERE: the runtime's
-- sliderThumb passes a one-entry list and centres on the hit.
local _, hit = host.screen.imageSearchMulti(asList(c.thumbImage), {
  region = region,
  tolerance = c.tolerance or MATCH_TOLERANCE,
  scales = c.scales or MATCH_SCALES,
})
```
