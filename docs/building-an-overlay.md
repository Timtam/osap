---
title: "Building an overlay — a walkthrough"
sidebar_position: 2
---

# Building an overlay

An **overlay** gives an application that a screen reader cannot read a keyboard interface that it can. You declare a list of controls, say where they live on screen and what they do, and the platform makes them navigable with Tab and Enter and speaks each one as you land on it. The application itself is untouched — you are laying an accessible surface over its pixels.

This page builds one up from the simplest possible case to the shape the Kontakt module uses, introducing each concept only when you need it. If you are looking for exact signatures instead, see the [overlay API reference](api/overlay.md).

---

## The vocabulary, in one place

You will meet these words throughout. Read them once now; each is explained properly where it first appears.

| Term | What it means |
| --- | --- |
| **Overlay** | One list of controls, plus the conditions under which it is live. Only one overlay is active at a time per slot. |
| **Control** | One thing the user can reach with Tab: a button, a toggle, a read-out, a static label. |
| **Binding** | *Where* an overlay lives — a window, or a control embedded inside someone else's window. |
| **Slot** | A competition group. Overlays on the same slot are alternatives; the arbiter picks one. |
| **Layer** | Rank within a slot. The most specific matching overlay wins. |
| **Cell** | One combination that is fixed for a window's lifetime (which version, where it runs). Each cell gets its own overlay. |
| **Part** | A plain function that adds controls, so several overlays can share a header. |
| **Frame** | A shift of the coordinate origin, for when your coordinates are relative to something other than the window. |
| **Landmark** | An image that must be on screen for the overlay to apply — and, optionally, what its coordinates are measured from. |
| **`when`** | A per-control condition that can change *while* the overlay is active. |

---

## Step 1 — the smallest overlay

A module is a folder with a `module.toml` and a Luau file. The manifest names the module — `id`, `name` and `version` are required — declares the overlay runtime as a dependency, and lists what the module's own code calls: here `host.speech`, from the button's callback. What the runtime does on the module's behalf — watching for the window, matching it, capturing Tab, reading OCR — is covered by the runtime's own manifest, so `window` is not on this list: the module's own code never calls `host.window` (see [What to declare](api/index.md#capabilities)).

```toml
# module.toml
id           = "com.example.notepad-helper"
name         = "Notepad helper"
version      = "0.1.0"
dependencies = ["com.platform.overlay"]

[capabilities]
require = ["speech"]
```

Then `src/main.luau`, the default entry file, pulls the runtime in:

```luau
local O = host.require("com.platform.overlay")

local ov = O.new("Notepad")
ov:addStaticText("Notepad helper")
ov:addCustomButton({
  label = "Say hello",
  hotkey = "Alt+1",
  onActivate = function() host.speech.output("Hello!") end,
})

ov:attach({ title = { contains = "Notepad" } })
```

Those two files are a complete, working overlay: put the folder under `modules/` beside the application, or run it with `automation-platform <folder>` — which finds the overlay runtime only in the folder that holds yours or in a `modules/` folder beside that one, so keep the runtime's folder there (see [where modules are found](module-package-format.md#where-modules-are-found)). While a window whose title contains "Notepad" is focused, Tab and Shift+Tab move through the two controls, each is spoken as you reach it, and Enter or Space activates the focused one. `Alt+1` fires from anywhere in that window.

Three things happened implicitly, and they are worth knowing:

- **Tab is captured** while the overlay is active, and released when it is not. Your overlay owns navigation; the application does not see those keys.
- **Every control is a focus stop**, including static text. Nothing is decorative.
- **The overlay announces the control you land on** as `"label, type"` — `"Say hello, button"`.
- **You arrive on the first control**, which is focused for real: announced, and holding whatever keys it needs (Space, Enter, a slider's or a tab control's arrows). So the first control decides what the application loses the moment its window comes forward — declare something inert first, such as a static text naming the place, if that matters here. Alt-Tabbing out and back to the *same* window resumes where the user was instead.

### Order matters

Everything that describes the overlay must come *before* the call that binds it. Binding is what makes it live, and the runtime evaluates the overlay immediately. Getting this wrong is now an error rather than a subtle misbehaviour, but the rule is simple: **declare first, bind last**.

---

## Step 2 — controls that act on pixels

Most applications worth covering expose nothing to read. You work in coordinates.

```luau
ov:addHotspotButton({ label = "Play", at = { 120, 40 }, hotkey = "Alt+P" })
```

`at` is `{x, y}` **relative to the origin** — the client-area top-left of whatever the overlay is bound to, re-resolved on every use so it follows the window as it moves. You never write screen coordinates.

Other control kinds, all origin-relative:

- `addOCRButton{ region = {x1,y1,x2,y2} }` — reads its value by OCR and speaks it. For a value the application only draws.
- `addHotspotToggle{ at, onColor, offColor }` — reads on/off from a **single pixel** at its click point, whichever reference colour it is nearer. Cheap; prefer it when a control has a distinct lit/unlit colour.
- `addGraphicalToggle{ region, onImage, offImage }` — reads on/off by matching two template images. For a control with no single distinguishing pixel.
- `addCustomButton{ onActivate = fn }` — you do the work yourself.

### A plug-in that zooms

A plug-in that draws its whole interface at a size the user picks — VPS Avenger zooms from 50 to 200 % — moves every control with it. Write each coordinate at one size and tell the overlay the factor, and every `at`, `region`, `points` step and calibration crosshair follows ([`O:scale`](api/overlay.md#o-scale)):

```luau
ov:scale(function(origin)
  local zoom = zoomOf(origin)            -- read off the plug-in, never guessed
  -- nil: nothing is clicked until it is known. On Windows a display scaled to 150 % draws the
  -- plug-in 1.5 times as large at the same zoom, which the factor has to carry too (O:scale,
  -- Windows): learn it from the screen, as VPS Avenger's module does from its header.
  return zoom and zoom / 50 * displayFactor(origin) or nil
end)
```

What does not zoom — an offset of the plug-in's own corner from the origin, as VPS Avenger's (-2, +2) on a DAW's panel, or a centring as ARC ON:EAR's half width — goes in [`O:frame`](api/overlay.md#o-frame), which is added unscaled. The DAW's own chrome never does: daw-hosts puts the origin below it ([daw-hosts](daw-hosts.md)), so a plug-in's module knows no DAW's geometry. Your own code places points with [`o:toScreen(x, y)`](api/overlay.md#o-toscreen).

### An item in the plug-in's own menu

A popup the plug-in draws is a window of its own, so a click sequence cannot reach its items: the second click is refused, because another window is drawn where the plug-in was. Say what to choose instead, and the overlay clicks it once a [menu test](api/overlay.md#o-menutests) sees the menu, and again if the menu did not take the click ([`O:chooseMenuItem`](api/overlay.md#o-choosemenuitem)):

```luau
local binding = O.embedded({ hosts = daw.all, control = { windows = "^JUCE_%x+$" }, identify = isMine })
ov:addHotspotButton({ label = "Load preset", at = { 262, 14 }, menuItem = { 5, 14 } })
ov:bind(binding, { menus = { O.menuTests.newWindow } })
```

`menuItem` is the item's offset from the menu's corner; in a calibrating run the menu is photographed with a crosshair on it, which is where that offset is measured from.

### Acting on what was read

Text read off the screen with [`host.ocr.recognize`](api/ocr.md#host-ocr-recognize) comes back later — in its callback, or to the handler that waited for it — and by then the user may have moved on: another control, another key, another window. Before acting on the answer, ask whether the overlay is still where it was: [`o:here()`](api/overlay.md#o-here) when the key is pressed, [`o:stillHere(mark, opts)`](api/overlay.md#o-here) when the answer comes. It is the check the overlay makes itself before it says a value or clicks an OCR button, and it answers why not in the same words.

```luau
ov:addCustomButton({ label = "Bank", onActivate = function(o)
  local mark = o:here()
  local r = o:toScreenRect({ 300, 20, 400, 36 }, { whole = true })   -- the corners `recognize` takes
  if not r then return end
  host.ocr.recognize(r, function(reading)
    if reading.status == "text" and o:stillHere(mark, { keys = true }) then
      host.speech.output(reading.text, { interrupt = true })
    end
  end)
end })
```

`here`, `stillHere` and `whole` came with version 0.2 of the overlay runtime, so a module that uses them says so in its manifest: `dependencies = ["com.platform.overlay >= 0.2"]`.

### Finding the coordinates — the calibrator {#calibrator}

Do not derive coordinates from another tool's numbers, and do not trust a value because *something* about it looks right. A cautionary tale from this repo: a set of toggles was calibrated by sampling colours, the colours matched, and the positions were taken to be right — they were 16 px off, on the caption row *under* the buttons. Three of five sampled near-black there and reported "off" forever, and the overlay had shipped like that.

Tick **Calibration keys and pictures in overlays** in the overlay runtime's own settings — the module manager's Installed list, **Overlay runtime**, **Settings…** — and three keys arm on the next overlay to come to the front, with no reload (for a launch without a window, see [`O.calibrating`](api/overlay.md#o-calibrating)). Every module built on the runtime asks it the same switch. They are written `Ctrl+Alt+Shift+…`: Ctrl+Alt+Shift on Windows, and Command+Option+Shift on a Mac (a spec's Ctrl is Command there), which is off Control+Option, VoiceOver's layer.

| Key | What it does |
| --- | --- |
| `Ctrl+Alt+Shift+S` | Screenshot of the coordinate window with a **crosshair** where each control will actually click, plus a log line per control: resolved screen point, the pixel read there, and whether it is `rawOrigin`. Every crosshair's pixel comes from one read of the screen ([`host.screen.pixels`](api/screen.md#host-screen-pixels)); when that read fails, the lines start with `the pixels could not be read: <why>` and every pixel is `?`. |
| `Ctrl+Alt+Shift+T` | Crops a template around the **focused** control and writes it into `calibration/`. |
| `Ctrl+Alt+Shift+V` | Counts **every** match of the focused control's template in the region. |
| (no key) | Activating a control declared with `opensMenu` saves three pictures of the overlay's origin whole — a snapshot just before it acts, written once the other two are in, and ~600 ms and ~1500 ms after — as `<overlay>-<control>-menu-before.png`, `-menu-after-600.png` and `-menu-after-1500.png`. What a menu test for a menu drawn inside the plug-in is written from; see [`O.calibrating`](api/overlay.md#o-calibrating). |
| (no key) | A control that chooses an item in its menu also saves `<overlay>-<control>-menu-item.png` on those presses: the menu, with a crosshair on the item, just before it is clicked. What an item's offset is measured from. |

The screenshot is the one that matters: "is my control on its button?" becomes a glance instead of arithmetic. The crosshairs are magenta, a colour these dark plugin interfaces do not use, and are numbered by ticks so they match the log lines.

`Ctrl+Alt+Shift+V` answers the question you must ask before clicking a template match blindly: **exactly one** is what you want. Two means it will eventually click the wrong one; none means the control silently never fires. A close-glyph template in this repo was checked that way before shipping, and sat 20 px from the plugin's own close button.

Output goes to `modules/overlay-runtime/calibration/`, named after the overlay — the runtime resolves its own paths against its own module, which is the same rule that lets an inherited overlay find its own images. The log line prints the absolute path.

---

## Step 3 — a plugin inside another application

A plugin has no window of its own. It is drawn inside a DAW's window, so instead of matching a window you say where the plugin is within one:

```luau
local daw = host.require("com.platform.daw-hosts")

-- Where the plugin prints its name, from its own top-left. Its Mac build lays the header out a
-- few points differently — the plugin's difference, not a DAW's — so the region is picked.
local WORDMARK = host.os.pick { windows = { 605, 33, 762, 70 }, macos = { 597, 29, 770, 74 } }

ov:attachEmbedded({
  hosts = daw.all,                          -- every DAW daw-hosts has an entry for
  control = { windows = "^Plugin%x+$" },    -- the plugin's own child window, on Windows
  identify = function(ctrl)                 -- ...but is it OUR plugin?
    -- An accessibility element of its own: found on Windows, and on a Mac no plugin measured
    -- so far publishes one inside a DAW — so this alone would never say yes there.
    if host.element.find(ctrl.id, "PlogueXMLGUI", host.element.type.Pane) then return true end
    -- Its name where it prints it, relative to the control: what answers on the panel too.
    local x, y = math.floor(ctrl.client.x), math.floor(ctrl.client.y)
    local r = host.ocr.recognize({ x + WORDMARK[1], y + WORDMARK[2], x + WORDMARK[3], y + WORDMARK[4] }, { lang = "en" })
    return string.find(string.lower(r.text), "sforzando", 1, true) ~= nil
  end,
})
```

That read waits in the handler that asked, once per control on Windows: the module's next key runs after it, and the rest of the application goes on ([Where it waits](api/ocr.md#where-it-waits)). The corners are cut to whole numbers, which a read takes, and the wordmark is read in English, as it is written. On the DAW's panel it is asked once per stay in *every* plugin window of every DAW, and on a Mac a read that finds nothing can take a quarter of a second — a wait the module's first key after coming into any plugin would sit behind — so sforzando reads there with a callback, answering `nil` until the answer lands; [`O:attachEmbedded`](api/overlay#o-attachembedded) shows how.

**Never name a DAW.** `hosts = daw.all` is every DAW [daw-hosts](daw-hosts.md) knows, and a DAW added there is one your plugin is recognised in, with no change to your module. So nothing in a plugin module may depend on which DAW it is in: not the DAW's executable or bundle, not its window titles — Logic titles a plug-in window "Inst 1", by its channel strip — not its chrome, not where in its window it draws a plugin. If you find yourself needing one of those, it belongs in the DAW's entry, where every plugin gets it.

On Windows the plugin is a child control of the DAW's window, with a window class of its own, and `control` matches that class. On a Mac there is no such control: the plugins measured so far publish nothing inside a DAW that *is* the plugin. So a binding with no `macos` entry is given **the DAW's plug-in panel** there — where the DAW's entry says a plugin begins, to the window's edges — and your coordinates, written against your plugin's own top-left, land in it as they do against the control on Windows. You do not write a macOS entry to work on a Mac.

The class alone is rarely enough — `Plugin<hex>` is shared by several vendors' plugins (ReaHotkey matches sforzando, Engine 2 and Zampler by it) — and the panel says nothing at all about *which* plugin it shows. `identify` is the second question, and it should be a *positive* test: something that is true of your plugin and nothing else, asked inside the control — an accessibility element of your plugin's, a word it prints at a known place relative to its top-left, a pixel. It has to answer on the panel too, because a binding with no `identify` never takes one: a module whose class was its whole identity is simply inactive on a Mac, and says so in the log. And an `identify` that finds nothing there — an accessibility element no Mac plugin publishes — answers "no" in every DAW on a Mac, which is why the example above reads the plugin's name as well. Its result is kept per window handle on Windows, and on a Mac for as long as the keyboard stays in the plugin — a REAPER FX chain window shows whichever FX is selected in its list, so the question is asked again once the keyboard has been on the list. Pass `cacheIdentity = false` when the answer depends on the plugin's *surroundings* rather than the control itself; then it is asked every time, in the runtime's coroutine for the hooks asked on every scan, and has to be cheap and answer at once — no read that waits ([where a hook runs](api/overlay.md#where-a-hook-runs)).

Coordinates are now relative to the plugin, so the same numbers work in every host. Where your plugin's own layout differs between its Windows and Mac builds, that difference is the plugin's, not a DAW's: pick it with `host.os.pick`, as sforzando does for its read-outs.

---

## Step 4 — several overlays, and how to choose

Here is where most of the confusion lives, so here is the whole rule:

> **A separate overlay** when the discriminator is fixed for the lifetime of the thing you attach to — which application, which version, where it runs.
>
> **A `when` predicate on a control** when the discriminator changes while the overlay is active — which tab is at the front, whether a panel is open, whether something is loaded.

The reason is not aesthetic. The arbiter decides *between overlays* once, when a window is focused; it cannot re-decide mid-use without tearing the overlay down and rebuilding it, which the user hears as focus jumping and an announcement repeating. So anything that changes under the user's hands must be a `when`, and anything settled when the window opened should be its own overlay — where its coordinates are plain numbers and its control set is fixed.

A combination that is fixed for a window's lifetime is a **cell**. Kontakt has six: two versions × three places it can run (embedded in a DAW, wrapped inside another plugin, standalone). Rather than one overlay asking at runtime where it is, the module loops over a table:

```luau
for _, cell in ipairs(cells.all) do
  local ov = O.new(cell.version)
  header.add(ov, cell)               -- a part; see below
  ov:frame(cell.frame)
  ov:bind(cell.binding, { specificity = O.layer.base })
end
```

Each cell carries its own binding (whose `identify` matches only that version, in that place) and its own geometry. Nothing is worked out per keystroke that was already settled when the window opened.

### Slots and layers

Overlays that are **alternatives for the same situation** share a `slot` — a plain string, agreed between modules:

```luau
ov:bind(binding, { specificity = O.layer.content })
```

Within a slot the arbiter activates the matching overlay with the highest layer:

- `O.layer.chrome` — a host's own frame around someone else's plugin
- `O.layer.base` — the plugin's generic controls
- `O.layer.content` — what is loaded inside it right now
- `O.layer.dialog` — a modal that must own the keyboard while it is up

That is how three modules compose without knowing about each other: Komplete Kontrol contributes its chrome, Kontakt its header, a sample library its controls, and whichever is most specific and currently matching is the one the user gets.

Two overlays that can be live **at the same time** belong on *different* slots — an embedded plugin and a standalone copy of the same application, for instance.

### One module per plug-in

A module runs one callback at a time: every key, hotkey, timer and window event of all its overlays, in turn ([each callback is a handler](module-runtime-and-lifecycle.md#handlers)). So a callback that takes long holds the module's other overlays with it. The long thing a callback does is a read: [`host.ocr.recognize`](api/ocr.md#host-ocr-recognize) without a callback waits, and only its own module waits ([where a hook runs](api/overlay.md#where-a-hook-runs)) — so a port that keeps every plug-in in one module, as ReaHotkey keeps every overlay in one script, has one plug-in's read hold the keys of another in the next window. Put each plug-in in a module of its own, as a sample library on top of Kontakt already is ([the cell model](api/overlay.md#plugin-base-library-overlays)).

---

## Step 5 — sharing controls between overlays

A **part** is just a function that adds controls:

```luau
local function addHeader(ov, cell)
  ov:addCustomButton({ label = "Load", hotkey = "Ctrl+L", onActivate = … })
end
```

Call it from every overlay that should carry that header. There is no inheritance mechanism to learn — an overlay is built by calling functions, so sharing is done by calling the same function.

Parts cross module boundaries too. A module marked `code_module = true` in its manifest has its exported functions available to modules that depend on it, so the module that *owns* a set of controls can hand them out instead of everyone reimplementing them. Its source is evaluated inside each dependent's VM and also runs once in a VM of its own, so anything it does at its top level happens once per copy; see [`host.require`](api/require.md) for the `activate` convention that runs setup only once:

```luau
local kk = host.tryRequire("com.platform.komplete-kontrol")
kk.chrome(ov)   -- Komplete Kontrol's own controls, defined once, by its own module
```

---

## Step 6 — when the overlay depends on what is loaded

A sample library inside a plugin has no window and no class of its own. What it does have is a recognisable picture — a wordmark:

```luau
ov:landmark(host.path("images/MyLibrary/wordmark.png"), { anchor = true })
```

This does two jobs:

1. **A gate.** The overlay only applies while that image is on screen. Combined with a higher layer, it takes over from the plugin's plain header exactly when the library is loaded.
2. **An anchor** (with `anchor = true`). Coordinates are then measured from the *wordmark's* position rather than the window's — so they survive the plugin being resized, the host being different, or a browser panel shifting everything sideways. The wordmark and the controls move together.

Image paths must be **absolute** — always `host.path("…")`. A relative path is resolved against the module whose code makes the call, and the search is made by the overlay runtime's code (or, for an inherited overlay, by the base module's), so it would be looked for in that module's folder, not yours — see [Paths](api/index.md#paths). This is checked when you bind, and reported.

Because a library can load or unload with no window event, gate it on a poll:

```luau
ov:bind(binding, { specificity = O.layer.content, pollMatch = 500 })
```

---

## Step 7 — splitting a module across files

Once a module has several overlays it stops fitting in one file. `host.include` loads another file *of the same module* and returns whatever it returns:

```luau
-- src/geometry.luau
return { BUTTONS = { play = { 120, 40 }, stop = { 160, 40 } } }

-- src/main.luau
local geo = host.include("src/geometry.luau")
```

Each file runs once per VM, and sees the same `host` as the file that included it — so a shared framework's files resolve against *its* module, not against whoever is using it. The Kontakt module is split into detection, the cell table, the geometry, what a control does, what a cell contains, and the wiring.

Luau's own `require` does not work here — it raises `require is not supported in this context` in module code — so `host.include` is the way to split a module.

---

## Step 8 — the same module on Windows and macOS

Almost all of an overlay is already portable: coordinates have their origin at the top left
of the primary display on both systems, every call takes them in one unit per system, and
clicking, dragging, typing, image search and OCR all take the same numbers the screen calls
return. **Nothing about the controls, the hotkeys or the actions needs to know which system
it is on.**

The unit is where the two differ. On **Windows** a coordinate is a physical device pixel — the
application is per-monitor DPI aware — and on **macOS** it is a point, and a capture there is
one pixel per point. The numbers are the same only at 100 % scaling: on a Retina Mac they are
exactly half what the same panel gives on Windows at 200 %, and a template cut on one is a
different size on the other. Geometry measured on one system at another scaling has to be
scaled, or measured again (see [Coordinates](api/index.md#coordinates)). The same applies to
numbers taken from a tool that is not DPI-aware: at 150 % on Windows, multiply them by 1.5.

What genuinely differs is *identity* — how you say "this window is Kontakt". There is no
common vocabulary for it: Windows has a window class, macOS has an accessibility role and a
bundle identifier, and no abstraction can make `NINormalWindow` and `AXWindow` the same
string. So identity is the one thing a matcher spells twice, and the grammar keeps that
confined to a single table per platform:

```luau
local KONTAKT = {
  title = { contains = "Kontakt" },        -- shared, and often enough on its own
  windows = { class = { prefix = "NINormalWindow" },
              app = { exe = { contains = "Kontakt" } } },
  macos   = { axSubrole = "AXStandardWindow",
              app = { bundleId = { contains = "native-instruments" } } },
}
```

A platform block takes `title` and `app` as the top level does, plus `class` — which only a
platform block takes — and, on macOS, `axRole`, `axSubrole` and `axIdentifier`. Those three are parts of the one `class`
string the macOS backend publishes (`AXRole/AXSubrole/AXIdentifier`, both separators always
present); matching them individually just saves writing a pattern around the separators.

Three more rules worth knowing:

- **A matcher with no platform block at all matches everywhere.** If title and application
  name are enough to identify the window, you have written a cross-platform matcher without
  meaning to.
- **A matcher that names platforms matches only on those.** That is the OS gate, and it is
  why a Windows-only module is a clean no-op on a Mac rather than a source of wrong matches.
  `os = { "windows" }` says the same thing for a matcher that needs no block.
- **Any single field can be given per platform**, since the OS keys and the match modes are
  different words: `title = { windows = "Kontakt 8", macos = { prefix = "Kontakt" } }`. Use it
  where one field differs and the rest of the matcher does not.
- **`class` belongs inside a platform block.** Only `title`, `app`, `os`, the platform blocks
  and `where` are read at the top level; a top-level `class` — where AutoHotkey's `ahk_class`
  habit puts it — is ignored without an error, and the matcher then matches every window of
  the application. The full list of keys is under [Matchers](api/window.md#matchers).

The embedded binding has one more of these, for the same reason — a plugin's own control has a
window class on Windows and, where it publishes one at all, an accessibility role on macOS:

```luau
O.embedded {
  hosts = daw.all,
  control = { windows = "^Plugin%x+$", macos = "^AXGroup/" },
}
```

Most plugins publish no control of their own on a Mac, and then you leave the `macos` entry out: the binding takes the DAW's plug-in panel there (see Step 3). A `macos` entry is for a plugin that does publish its own container — or a function, `(activeWindow, hostPanel) -> control`, for one that knows its own corner better than the DAW does: Kontakt finds its FILE button near where its geometry puts it and hands back the panel from its own corner; see [`O:attachEmbedded`](api/overlay#o-attachembedded). `macos = false` says the binding does not apply on a Mac at all, panel included. A `control` written as a plain string is almost always a Win32 class pattern, which on a Mac is compared with the accessibility role, subrole and identifier and so practically never matches; the binding then takes the panel as well when it has an `identify` (and stays inert when it has none), and the runtime says which in the log, once per pattern that names no `AX` role and has no `/`.

**Keys are written once, and land on each platform's counterpart.** The key spec's modifiers
are roles, the way Qt names them. On a Mac `Ctrl` is Command, `Alt` is Option and `Win` (written
`Meta` by a Mac author) is Control. So `hotkey = "Ctrl+L"` is Command+L there and
`hotkey = "Alt+P"` is Option+P, and the runtime announces each that way when the control is
focused ([`host.keys.describe`](api/keys.md#host-keys-describe)). When the counterpart is wrong on
a Mac, pick another combination there with [`host.os.pick`](api/os.md#host-os-pick). That is the
case when the Mac keeps the counterpart for itself: Command+Tab, Command+H, +M, +Q. The runtime's
next-tab key is `{ windows = "Ctrl+Tab", macos = "Meta+Tab" }` for that reason. It is also the case
when the counterpart holds Control and Option together: `Meta+Alt+…`, or all four modifiers.
That is VoiceOver's modifier, and a key there never arrives on a Mac whose VoiceOver uses its
default setting. `Ctrl+Alt+…` is Command+Option on a Mac and is off it, but on Windows it is AltGr,
a character on many layouts (Ctrl+Alt+2 is ² on a German one).
[`host.keys.check`](api/keys.md#host-keys-check) says all of these before anybody presses the
key. A key that has to be free system-wide is best a function key with modifiers. With a letter,
the four modifiers are held by the Windows shell for the Office key, and Command+Shift with a
letter is a Mac application's own menu layer.

Screen readers take keys of their own as well — VoiceOver the whole Control+Option layer, JAWS
and NVDA whatever their scripts and add-ons define — and no list could keep up with the last
two. If a user reports that a key does nothing while the screen reader runs, the answer is
another key.

`host.os.pick` and `host.os.is` are for what differs in **the other program** — the plug-in or
the application, which is a different build on each system:

```luau
-- Kontakt names its Info Pane button after its own shortcut, which NI changed on the Mac.
local INFO_PANE = host.os.pick {
  windows = "Info Pane (F9): Toggles the visibility of the information hint.",
  macos = "Info Pane (Cmd+I): Toggles the visibility of the information hint.",
}
if host.os.is("macos") then ... end -- a whole binding that only exists there
```

Reach for that last. A module that branches on the operating system in ten places is a
module that will drift apart into two, and the reason the rest of this page never mentions
platforms is that it does not have to.

---

## Step 9 — when it does not appear

Every failure here is silent by nature: nothing errors, the overlay simply never activates. Three places tell you why.

**The slot roster**, logged as each overlay binds — the newcomer and the count it makes:

```
arbiter slot 'com.platform.kontakt': +com.platform.cinematic-studio-strings(30) -> 11 participant(s)
```

If your overlay arrives as `-> 1 participant(s)` in a slot that should have several, your slot string does not match the one the other module uses. A typo cannot error — it silently creates a private slot where you always win, or never compete.

**The landmark log**, on every transition:

```
[landmark] Kontakt — Cinematic Studio Strings: found at 667,225 294x24  (region 138,181–1164,919)
```

No line at all means the gate is never even evaluated — the *context* does not match, so look at your binding. A `lost` line means the image search failed: the picture on screen is not the picture you captured (a different version, a different scale, or something covering it).

**The calibrator** (above): the screenshot shows where every control actually lands, which answers "is it even pointing at the thing?" before you look anywhere else.

**The declaration report**, logged when you bind, naming the control:

```
[overlay] My Plugin: control 3 (hotspot 'Play') has neither `at` nor `points` —
  activating it does nothing
```

---

## A checklist

- Declare everything, then bind. Never the other way round.
- Coordinates relative to the origin, measured from a capture, colours confirmed at the measured point.
- A positive `identify`, not just a window class.
- Fixed for the window's life → its own overlay. Changes while active → `when`.
- Same situation → same slot, different layers. Can be live together → different slots.
- Image paths through `host.path`.
- Anything that can open a menu → `opensMenu = true` on the control, and `menus = { … }` naming the tests that see the plug-in's menus ([`O.menuTests`](api/overlay.md#o-menutests)); a menu no test sees keeps the overlay's keys and is unusable. A menu that is a window of its own coming to the front needs `O.menuTests.newWindow`, which is also what lets the overlay hold its place over it.
- Shared controls → a part, and if another module owns them, ask that module for them.
- One plug-in → one module. A `when`, a menu test, a `present` or an `identify` with `cacheIdentity = false` answers at once — no read that waits, and no read of the screen: a pixel a `when` needs is a [probe](api/overlay.md#o-probe)'s, which the runtime reads before the scan; a `text`, an `onActivate` or an `onStep` may read ([where a hook runs](api/overlay.md#where-a-hook-runs)).
