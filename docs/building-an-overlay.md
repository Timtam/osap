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
  hotkey = "Ctrl+Alt+1",
  onActivate = function() host.speech.output("Hello!") end,
})

ov:attach({ title = { contains = "Notepad" } })
```

Those two files are a complete, working overlay: put the folder under `modules/` beside the application, or run it with `automation-platform <folder>` — which finds the overlay runtime only in the folder that holds yours or in a `modules/` folder beside that one, so keep the runtime's folder there (see [where modules are found](module-package-format.md#where-modules-are-found)). While a window whose title contains "Notepad" is focused, Tab and Shift+Tab move through the two controls, each is spoken as you reach it, and Enter or Space activates the focused one. `Ctrl+Alt+1` fires from anywhere in that window.

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

### Finding the coordinates — the calibrator

Do not derive coordinates from another tool's numbers, and do not trust a value because *something* about it looks right. A cautionary tale from this repo: a set of toggles was calibrated by sampling colours, the colours matched, and the positions were taken to be right — they were 16 px off, on the caption row *under* the buttons. Three of five sampled near-black there and reported "off" forever, and the overlay had shipped like that.

Start the app with `AUTOMATION_PLATFORM_CALIBRATE=1` and three keys arm on whichever overlay is active:

| Key | What it does |
| --- | --- |
| `Ctrl+Alt+Shift+S` | Screenshot of the coordinate window with a **crosshair** where each control will actually click, plus a log line per control: resolved screen point, the pixel read there, and whether it is `rawOrigin`. |
| `Ctrl+Alt+Shift+T` | Crops a template around the **focused** control and writes it into `calibration/`. |
| `Ctrl+Alt+Shift+V` | Counts **every** match of the focused control's template in the region. |

The screenshot is the one that matters: "is my control on its button?" becomes a glance instead of arithmetic. The crosshairs are magenta, a colour these dark plugin interfaces do not use, and are numbered by ticks so they match the log lines.

`Ctrl+Alt+Shift+V` answers the question you must ask before clicking a template match blindly: **exactly one** is what you want. Two means it will eventually click the wrong one; none means the control silently never fires. A close-glyph template in this repo was checked that way before shipping, and sat 20 px from the plugin's own close button.

Output goes to `modules/overlay-runtime/calibration/`, named after the overlay — the runtime resolves its own paths against its own module, which is the same rule that lets an inherited overlay find its own images. The log line prints the absolute path.

---

## Step 3 — a plugin inside another application

A plugin has no window of its own. It is a child control inside a host's window, so instead of matching a window you match a control within one:

```luau
local daw = host.require("com.platform.daw-hosts")

ov:attachEmbedded({
  hosts = daw.all,                 -- the applications that can host it
  control = "^Plugin%x+$",         -- the child control's window class
  identify = function(ctrl)        -- ...but is it OUR plugin?
    return host.element.find(ctrl.id, "PlogueXMLGUI", host.element.type.Pane)
  end,
})
```

The class alone is rarely enough — one vendor's class covers all of their plugins. `identify` is the second question, and it should be a *positive* test: something that is true of your plugin and nothing else. Its result is cached per window handle unless you pass `cacheIdentity = false`, which you need when the answer depends on the plugin's *surroundings* rather than the control itself.

Coordinates are now relative to that control, so the same numbers work in every host.

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

The embedded binding has one more of these, for the same reason — a host names its plugin
surface with a window class on Windows and an accessibility role on macOS:

```luau
O.embedded {
  hosts = daw.reaper,
  control = { windows = "^Plugin%x+$", macos = "^AXGroup/" },
}
```

Where the Mac publishes nothing that *is* the plugin — Kontakt inside REAPER puts its elements straight into the FX window — the macOS entry can be a function that finds the panel from one of the plugin's own named buttons and returns it as a control; see [`O:attachEmbedded`](api/overlay#o-attachembedded).

And for the genuine one-off, `host.os.is("macos")` and `host.os.current` are always there:

```luau
if host.os.is("macos") then ... end
local step = host.os.pick { windows = 3, macos = 1 }
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
- Anything that can open a menu → `menus = true`, or the menu will be unusable.
- Shared controls → a part, and if another module owns them, ask that module for them.
