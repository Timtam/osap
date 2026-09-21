---
title: "host.screen — pixels, profiles and image search"
sidebar_position: 14
toc_max_heading_level: 2
---

Reads what is on screen: the colour of a point, a per-column and per-row profile of a region, and a search for a template image within one — a PNG, or a template built in memory from bytes or from the screen itself.

Use it where there is no element to ask. State that exists only as colour — Kontakt reads whether its snapshot bar is open from one pixel on the camera icon, and ON:EAR classifies each of its switches from a single pixel — and anything whose position moves, which is what image search is for, since Kontakt's five instrument-editor sections are found by their captions because opening one pushes the rest down the window.

**Every touch of the screen costs a compositor frame** through the standard path on Windows, measured at about 16.7 ms whether it reads one pixel or a whole window. So `pixel` is not the cheap call it looks like: read a point once per pump and let everything that asks about it share the answer. When the question is *where* something is rather than what colour a known point has, `profile` reduces one capture to a value per column and row — the way Melodyne finds a selected note by diffing two column profiles.

The matching, unlike the capture, does grow with the area. A template search across a whole plug-in window is the slow call here — measured at twelve seconds for one full-region match while twelve library overlays polled the same window every 500 ms — which is why anything on a detection poll uses `imageSearchAsync` on its worker thread and re-searches a small box around the last hit before widening.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["screen"]
```

See [what that list is and is not](./index.md#capabilities).

## Which picture a read sees {#which-picture-a-read-sees}

Every call on this page, and `host.ocr.recognize` / `recognizeMany`, reads the screen one way for the whole module, chosen in its manifest rather than per call. Nothing needs declaring for the standard way, which is what every module gets by default. A module written for an application the standard way may not read correctly — a game whose picture it sees frozen or black — can ask for the other one:

```toml
# module.toml of a game module
[capabilities]
require = ["screen", "timer", "speech"]

[screen]
capture = "duplication"   # "standard" (the default) or "duplication"
fallback = "none"         # with "duplication": "standard" (the default) or "none"
```

The code does not change. With the manifest above, this poll reads through desktop duplication, and says the menu state only when it changes:

```luau
local lastState = nil
host.timer.every(100, function()
    local c = host.screen.pixel(412, 318)
    if c == nil then return end   -- only under fallback = "none": no picture this time
    local state = (c.r > 200 and c.g < 60) and "Start" or "Options"
    if state ~= lastState then
        lastState = state
        host.speech.output(state)
    end
end)
```

A module that depends on a `code_module` which declares `[screen]` inherits the declaration unless it declares its own: the module's own table wins, then the nearest declaring dependency, then the earliest in manifest order. A dependency's table counts only if that dependency requires `screen` or `ocr`. The log's `[capture]` lines say which source each module ended up with, and why.

With `fallback = "none"`, a read that duplication cannot answer fails the way a failed capture always fails on this page — `pixel` returns `nil`, `profile` and `imageSearch` return `nil`, `imageSearchMulti` returns `(nil, nil)`, `imageSearchAsync` and `imageSearchEach` call back with `nil`, `imageSearchAll` returns an empty list, `template{ capture = … }` returns `nil`, `save` and `saveMarked` return `false` — and `host.ocr.recognize` returns `{ text = "", words = {}, skipped = false, error = "…" }` instead of raising. That is for an application where the standard picture is wrong rather than slow: saying a frozen menu item is worse than saying nothing. With the default `fallback = "standard"`, such a read is quietly read the standard way instead.

Poll only while your window is in front. A poll that runs all the time keeps desktop duplication open all the time.

### Windows

`duplication` is DXGI Desktop Duplication: it reads the picture the compositor hands the display, where the standard path reads the screen device context. It is meant for an application whose picture the standard path reads frozen or black, and whether it sees that application's frames any better has to be checked with the application itself. The log helps: after such a module loads, its first read is also made both ways — over the region it read, or over the foreground window's client area when that region is smaller than 64×64 pixels — and a `[capture]` line naming the module says whether the two pictures are the same.

It returns the most recently composed frame without waiting for the next one. Measured in a test build on the reference machine (GTX 1060, 1920×1080): about 0.3–1 ms for a region up to 633×418 against the standard path's 16 ms, 5 ms for the whole screen against 28 ms, and the same bytes from both paths on an ordinary desktop. That the latest frame comes back at once also means a read made right after `host.input` may still see the frame from before the input, so re-read after a short wait, as you would anyway.

- **The mouse pointer:** duplication reports it separately and this path does not draw it in, so, as with the standard path, it is normally not in the picture. Windows documents that a pointer can also be drawn into the desktop image itself, and then it is.
- **When it cannot answer:** Windows documents that duplication stops during a UAC prompt and on the lock screen and must be reopened after a display mode change or a switch into or out of fullscreen, that no more than four programs of a session may duplicate at once, and that it is refused on some laptops with two graphics chips and over Remote Desktop. A read then gets the standard picture, or fails under `fallback = "none"`, and the log names the reason once.
- **Opening it takes time.** The first read of a session creates a Direct3D device, which took about 0.2 s on the reference machine and occasionally several seconds. Until it is open, reads get the standard picture or fail under `fallback = "none"`. When a window trigger of such a module fires, the opening is started before the trigger's callback runs, so the callback's first read does not wait for it — that read, and the others made while it is still opening, get the standard picture or fail under `fallback = "none"`.
- **Regions touching a rotated monitor** are read the standard way (or fail under `fallback = "none"`).
- **HDR:** Windows converts the picture to 8 bits per channel for this path, so its colours may differ from the standard path's. Cut templates with `host.screen.save` from a module that declares the same `capture`.
- **"Let modules that ask for it read the screen through the graphics card"** in the Application settings tab turns it off for every module. Turning it off and on again also gives it another try after it stopped answering.

### macOS

`[screen]` is accepted and ignored: a module that declares it reads the screen exactly as one that does not, and `pixel` never returns `nil`.

## host.screen.pixel(x, y) {#host-screen-pixel}

**Signature:** `host.screen.pixel(x: number, y: number) -> { r: number, g: number, b: number, hex: string } | nil`

Reads the colour of the screen pixel at `(x, y)`. Returns the 8-bit channels `r`, `g`, `b` (0–255) plus `hex`, an uppercase `#RRGGBB` string. Returns `nil` only in a module that declared `fallback = "none"` (see [Which picture a read sees](#which-picture-a-read-sees)), when there was no picture to read; a module that did not always gets a colour.

```luau
local c = host.screen.pixel(100, 200)
if c == nil then return end   -- only under fallback = "none"
if c.hex == "#FF0000" then
    host.log.info("red pixel")
end
host.log.info(string.format("rgb(%d,%d,%d)", c.r, c.g, c.b))
```

### Windows

Reads the pixel straight from the screen. What you get is what is there — unless the module declares [`[screen] capture = "duplication"`](#which-picture-a-read-sees), in which case it is read from the duplicated picture. A point that lies on no monitor reads as black (0, 0, 0), the same as a region read gives there, whichever way the module reads.

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

**Signature:** `host.screen.profile(opts: { region: Region?, axes: ("both" | "columns" | "rows")?, dark: number? }?) -> { x, y, w, h, columns: Axis?, rows: Axis? } | nil` where `Axis = { min: number[], max: number[], dark: number[], mean: number[], r: number[], g: number[], b: number[] }`

Takes **one** capture of `region` (see [Region form](#region-form); **omitted, it profiles the whole primary screen**) and reduces it, per column and per row, to: the darkest pixel, the lightest, how many pixels are darker than `opts.dark`, the mean, and the mean of each colour channel. Luminance is ITU-R BT.601. `min` and `max` are whole numbers 0–255 because they are particular pixels; the means are **fractional** — a mean over hundreds of rows moves by less than one unit when something note-sized changes inside it, and rounding would floor exactly the signal you are looking for.

`dark` is a luminance threshold, 0–255, default `128`; each axis's `dark` sequence counts the pixels of that column (or row) whose luminance is below it. For finding a **shape** it is the statistic that works where the other two do not: a mean over hundreds of rows dilutes a note-sized object to a couple of units, and a minimum saturates because a busy column already contains something black. A count is proportional to how much of the column the object covers — Melodyne finds its notes with `dark = 190`. A value outside 0–255 silently falls back to `128`, and a fraction is cut to the whole number below it.

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

## host.screen.template(spec) {#host-screen-template}

**Signature:** `host.screen.template(spec: TemplateSpec) -> Template | nil` where `TemplateSpec = { name: string?, rgba: (string | buffer)?, rgb: (string | buffer)?, w: number?, h: number?, capture: Region?, file: string? }` — `name` plus **exactly one** of `rgba`, `rgb`, `capture` or `file` — and `Template = { w: number, h: number, name: string?, count: number }`, read-only.

Builds a template in memory, for every search below to take wherever it takes a path. A template no longer has to be a PNG file in the module's folder: its pixels can come from bytes the module computed or read from a data file, or from the screen itself, now.

- **`rgba`** with `w` and `h`: `w * h * 4` bytes, row-major from the top-left, in R, G, B, A order. As in a PNG, **alpha 0 is a wildcard** and any other alpha is compared in full.
- **`rgb`** with `w` and `h`: `w * h * 3` bytes; every pixel is compared.
- **`capture`**: the given [region](#region-form) of the screen, captured now. Every pixel is compared (alpha is forced opaque). Returns **`nil`** when the capture fails — the one failure that is a runtime condition rather than a mistake, so the one that does not raise.
- **`file`**: a PNG, resolved exactly as a path given to a search is, and sharing its cached decode. The handle is a **snapshot**: a re-captured PNG is picked up live only by a search that is given the path.

Bytes are a Luau `string` or a `buffer`, copied once. A handle built from bytes may be at most 4096 pixels on a side and 1,048,576 pixels in all (1024x1024); a `capture` is refused before anything is captured when its region is larger than that, or empty. `name` (at most 64 characters) comes back in every hit the template makes and in `tostring(t)`; a `file` template given none is named by its path as written. `count` is the number of pixels a search compares. Everything else raises, naming the field: a wrong byte count (`rgba is 1436 bytes; 10x36 RGBA needs 1440`), two sources or none, `w`/`h` given to a `capture` or a `file`, and any field this version does not know, so a misspelt `rbga` or a `tolerance` it does not take is never silently ignored. Tolerance and scales are the search's options, as for a path.

A template belongs to the module VM that built it, like any Luau value. It cannot be passed through a legacy data export or stored in `host.settings`. One VM's handles built from bytes or a capture may hold **32 MiB** between them; over that, the collector is run and, if they still do not fit, the constructor raises. `file` handles cost nothing against it. Build templates once, not on every poll: one from bytes or a file when the module loads, a `capture` once what it learns is on screen (see Windows below for a module that reads through desktop duplication).

```luau
-- A pack a converter wrote as JSON: each signature a name, a size and its RGB bytes as
-- numbers. Decoded once, when the module loads; each template keeps its name for its hits.
local pack = host.json.decode(host.resource.read("data/gamepack.json"))
local T = {}
for _, sig in ipairs(pack.signatures) do
  local bytes = buffer.create(#sig.rgb)
  for i, v in ipairs(sig.rgb) do buffer.writeu8(bytes, i - 1, v) end
  T[sig.name] = host.screen.template({ name = sig.name, rgb = bytes, w = sig.w, h = sig.h })
end
```

```luau
-- Learn the selection marker once, in this machine's own pixels, then follow it. A capture
-- can come back nil, so a nil is not kept: the next call asks the screen again.
local marker = nil
local function learnedMarker()
  if marker == nil then
    marker = host.screen.template({ name = "marker", capture = { 120, 80, 132, 92 } })
    if marker then
      host.log.info("learned " .. tostring(marker))   -- Template(marker 12x12, 144 compared)
    end
  end
  return marker
end

local m = learnedMarker()
local hit = m and host.screen.imageSearch(m, { tolerance = 8 })
```

### Windows

Sizes and coordinates are device pixels (see [`host.screen.size`](#host-screen-size)). A `capture` is one screen capture on the event loop — through the standard path measured at a fixed ~16.7 ms whatever the region's size — so learn a captured template once, not on a poll; it is counted in the observation log with `pixel` and `profile`. It is always a live read of the screen, never an earlier frame. It reads through the module's [source](#which-picture-a-read-sees) like every other read, and in a module that declares `capture = "duplication"` that alone does not make it the picture the module's searches will see: a `capture` made before duplication has opened — one made at load, or just after the module's window came forward — is read the standard way, or returns `nil` under `fallback = "none"`, and the template keeps whichever picture it got for as long as the module keeps it. Under `fallback = "none"` a `pixel` that returned a colour was answered through duplication, so such a module learns its captured templates after one and asks again later when it gets `nil`; under the default fallback nothing tells a module which picture a read got. A game that is not DPI-aware is bitmap-stretched by the desktop compositor at any scaling other than 100% — Microsoft's own documentation says such applications "appear blurry" — so a template made at one scaling setting will not match pixel for pixel at another: give the search a `tolerance`, and one `scales` factor computed from the window's actual size rather than a ladder of guesses.

### macOS

Captures are in **points** (see [`host.screen.size`](#host-screen-size)), so a template cut on a Windows machine at 200% scaling is twice the size of the same element on a Retina Mac; a `capture` template taken on the Mac is in the Mac's own resolution. Without the Screen Recording permission a capture does not fail — it is a picture of the desktop wallpaper — so a `capture` template made without the grant is a template of the wallpaper, and every search without the grant will find it there. The image worker's capture measured about 19 ms per batch in the fifth macOS session; a `capture` template pays for its capture on the event loop instead, so build it once.

## host.screen.imageSearch(template, opts?) {#host-screen-imagesearch}

**Signature:** `host.screen.imageSearch(template: string | Template, opts: { region: Region?, tolerance: number?, scales: {number}? }?) -> Hit | nil` where `Hit = { x: number, y: number, w: number, h: number, n: number, name: string? }`

Searches the screen for the first occurrence of `template` — an image path or a [`Template`](#host-screen-template) — and returns its match rectangle in **screen pixels**: `x, y` is the top-left of the match and `w, h` the matched size (the template's own, or a scaled one — see `scales`). `n` is `1` here; it is the index into the list for the calls that take several. `name` is the template's name, or the path exactly as you wrote it. Returns `nil` if not found (or if the search region could not be captured). The search runs top to bottom, then left to right, and the first position that matches wins; there is no score.

- A path is resolved relative to the module's root directory; **absolute paths are also accepted**.
- `opts.region` restricts the search area (see [Region form](#region-form)); default is the full primary screen.
- `opts.tolerance` is the per-channel RGB match tolerance (0–255, default `0` = exact match). A value outside 0–255 is read as `0`, an exact match, without a word — keep it in range.
- `opts.scales` is a list of factors to try, **in the order given**, the first that matches winning; each resizes the template (bilinear) before searching. Omitted, the template is searched at its own size only. A factor that is not a positive finite number, that rounds the template to nothing, or that makes it larger than the region is skipped. Resizing blurs, so pair scales with a tolerance.
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

**Signature:** `host.screen.imageSearchAsync(template: string | Template | {string | Template}, opts: { region: Region?, tolerance: number?, scales: {number}? }?, cb: (hit: Hit | nil) -> ()) -> nil`

Like `imageSearch`, but the region capture **and** the match both run on a worker thread; `cb` fires on a later tick. Use this for anything on a detection poll — a screen capture costs about a compositor frame, and doing it inline stalls the event loop that also carries speech and key handling.

Pass a **list of templates** — paths and [`Template`](#host-screen-template) handles mixed as you like — to try several renderings of the same thing (a dialog's close glyph as two plugin versions draw it): they are matched in order against the **same captured frame**, first hit wins, and `hit.n` reports which one matched (1-based), `hit.name` its name. Chaining separate searches instead pays a fresh capture per template. Every template is resolved before the call returns, so a bad path raises here and not on the worker. `nil` means no match **or** that the region could not be captured; [`imageSearchEach`](#host-screen-imagesearcheach) tells the two apart.

The callback belongs to the module whose VM made the call — for code a `code_module` dependency runs inside your module, that is **your** module. If that module is **disabled** before the answer arrives, the answer is not delivered; the search is held and run again when the module is enabled, and the callback gets that fresh answer — so a poll that waits for its callback before it searches again, like the overlay runtime's landmark gate, picks up where it left off, just as a [`host.timer.every`](./timer.md#host-timer-every) poll does. If the module is **reloaded** or removed first, the callback is dropped, never called. A search that panics inside the host is answered with `nil` rather than never.

```luau
host.screen.imageSearchAsync({ "images/close-v8.png", "images/close-v7.png" },
    { region = { b.x, b.y, b.x + b.w, b.y + b.h }, tolerance = 8 },
    function(hit)
        if hit then host.input.click(hit.x + hit.w // 2, hit.y + hit.h // 2) end
    end)
```

### Windows

The worker captures each search's region through the asking module's [source](#which-picture-a-read-sees). Searches queued in the same moment share a capture only when both the region and the source are the same, and the regions that go through desktop duplication are captured together, one request per `fallback` setting.

## host.screen.imageSearchEach(entries, opts?, cb) {#host-screen-imagesearcheach}

**Signature:** `host.screen.imageSearchEach(entries: { string | Template | { template: string | Template, within: Region?, name: string? } }, opts: { region: Region?, tolerance: number?, scales: {number}? }?, cb: (list: { Hit | false } | nil) -> ()) -> nil`

**One** capture of `opts.region` on the worker thread, and an answer for **every** entry: `cb(list)`, where `list[i]` is entry *i*'s [hit](#host-screen-imagesearch) (with `n = i`) or `false`. `cb(nil)` means the region could not be captured — "could not look", which the other async search cannot tell apart from "not there".

It is for the question "which of these states is showing?" — a menu whose cursor can be on one of several rows, a panel that shows one of several pages. `imageSearchAsync` answers only the first match of a list; separate calls share a frame only by the accident of landing in the same few milliseconds. Here one frame is the contract.

An entry is a path, a [`Template`](#host-screen-template), or a table naming one with two options: `within`, a [region](#region-form) in screen coordinates that this entry may be found in — it is cut to `opts.region`, and an entry whose `within` lies outside it, or is smaller than its template, answers `false` — and `name`, which replaces the template's name in its hit. `within` is what keeps a state's template from inventing a hit elsewhere in the window, and what keeps the search cheap: matching, unlike the capture, costs in proportion to the area searched. Entries are matched in parallel. `tolerance` and `scales` apply to every entry. The callback follows the same ownership rule as `imageSearchAsync`'s.

```luau
-- Which of three menu pages is open? One capture of the game window per poll, each page's
-- template looked for only in its own strip, and never two searches in flight at once.
local GAME = { title = { contains = "My Game" } }
local PAGES = { "Items", "Magic", "Status" }
local since = nil
host.timer.every(250, function()
  local win = host.window.active()
  if not win or not host.window.test(GAME, win) then return end
  if since and host.now() - since < 1000 then return end   -- one in flight, unless it got lost
  since = host.now()
  local b = win.client
  local entries = {}
  for i, page in ipairs(PAGES) do
    entries[i] = {
      template = host.path("images/menu-" .. page:lower() .. ".png"),
      within = { b.x, b.y + 40 * i, b.x + 200, b.y + 40 * i + 40 },
      name = page,
    }
  end
  host.screen.imageSearchEach(entries, { region = { b.x, b.y, b.x + b.w, b.y + b.h }, tolerance = 12 },
    function(list)
      since = nil
      if list == nil then return end        -- could not look: keep what we knew
      for _, hit in ipairs(list) do
        if hit then host.speech.output(hit.name .. " menu") return end
      end
    end)
end)
```

### Windows

The capture runs on the worker thread, not on the event loop, through the module's [source](#which-picture-a-read-sees) exactly as `imageSearchAsync`'s does; through the standard path it costs the same fixed compositor frame as any other (~16.7 ms whatever its size). The matching is spread across cores, one entry per task.

### macOS

The same worker and the same capture as `imageSearchAsync`. Without the Screen Recording permission the capture succeeds with a picture of the wallpaper, so the answer is **not** `nil` — `nil` only ever means the capture itself failed — but usually a list of `false`. Usually: a [`capture` template](#host-screen-template) that was itself made without the grant is a picture of the wallpaper, and its entry will match.

## Region form {#region-form}

Several functions (`host.screen.imageSearch` and the other searches, `host.screen.profile`, `host.ocr.recognize`, and each entry of `host.ocr.recognizeMany`'s `regions` list) accept a `region` table, and the same form is what `host.screen.template`'s `capture` and an `imageSearchEach` entry's `within` take. A region describes an axis-aligned rectangle by its top-left and bottom-right corners and may be written in **named** or **positional** form (named keys take precedence):

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

### Windows

The shot is taken through the module's [source](#which-picture-a-read-sees) like every other read. In a module that declares `capture = "duplication"`, a shot taken before duplication has opened — at load, or just after the module's window came forward — is taken the standard way, or `save` returns `false` under `fallback = "none"`, so a template cut from such a shot is not necessarily the kind of picture the module's searches read.

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

**Signature:** `host.screen.imageSearchAll(template: string | Template, opts: { region: Region?, tolerance: number? }?) -> { Hit }`

Returns **every** match of `template` in the region, as an array of [hits](#host-screen-imagesearch) in screen pixels, rather than stopping at the first one like `imageSearch`. `template` is a path or a [`Template`](#host-screen-template); each hit carries `n = 1` and the template's `name`. The list is empty when there is no match — and also when the region could not be captured; it is never `nil`. Its use is deciding whether a template is safe to click blindly: a close-glyph template that matches twice will eventually click the wrong one, and a template that matches nowhere is a control that silently never fires. "Matched exactly once, here" is the answer you want before shipping either, which is why the overlay runtime keeps it on a calibration key rather than in any live path.

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

**Signature:** `host.screen.imageSearchMulti(templates: {string | Template}, opts: { region: Region?, tolerance: number?, scales: {number}? }?) -> (number, Hit) | (nil, nil)`

Captures the region **once** and tries each template against that one frame, returning two values: the 1-based index of the first template that matched, and its [hit](#host-screen-imagesearch) in screen pixels (whose `n` is that same index). The list may mix paths and [`Template`](#host-screen-template) handles as you like. Reach for it whenever one question needs more than one template — a toggle's on/off pair, or a glyph as two plug-in versions draw it. The capture is the fixed cost (about a compositor frame on Windows, whatever its size); over a small region the extra comparisons are the cheap part, which is what makes one call beat two. The index is what turns a hit back into meaning: build the list in a known order and keep a parallel list of labels.

Sharing the frame also buys correctness, not only time: both templates are matched against the same pixels, so a toggle that changes mid-repaint cannot fall between two separate captures and come back as neither. Templates are tried in list order and the whole region is scanned for the first before the second is tried, so when both would match, order decides and not position. Decoded templates are cached by path and modification time, shared with `imageSearch` and `imageSearchAsync`, so a re-captured template is picked up live and a repeated poll does not re-decode PNGs. A template file that cannot be opened raises, as with `imageSearch`, and every template in the list is opened **before** the capture — so a bad path raises on the first call wherever it sits in the list. (Until 2026-09 the files were opened inside the loop, after the capture, and a bad path late in the list stayed dormant until every earlier template happened to miss.) A failed capture returns `(nil, nil)` as well, which is indistinguishable from "no template matched": for a state read like `gtoggleState` that is the difference between "off" and "I could not look". This call is synchronous — for anything on a detection poll use [`imageSearchAsync`](#host-screen-imagesearchasync), which takes a template list for the same reason and moves the capture off the event loop.

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
