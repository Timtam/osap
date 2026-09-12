---
title: "host.element — the accessibility tree"
sidebar_position: 3
toc_max_heading_level: 2
---

Queries the accessibility tree an application publishes — UI Automation on Windows, the Accessibility API on macOS — for elements by name and control type, for a point to click, and for a dump of what a window contains.

It is the first thing to try on a new plug-in, because a tree gives names and rectangles and those survive the window being moved, resized or redrawn. Every coordinate a module writes down instead is a claim about one window size on one machine.

Two shapes account for nearly all the use of it. **Identity**, where `findAny` asks "which of these names is present" and reports *which*, so Kontakt learns the version it is attached to from the same call that recognises it at all. And **geometry**, where `locate` and `pluginLocate` hand back a point to click — ON:EAR is driven entirely off the rectangles in a `rawDump`, its five identically named preset slots picked out as the only elements sharing an exact left edge.

It is a cross-process query and it is not free: a 53-element window takes about 30 ms to walk, and took 211 ms before the backend learned to fetch a node's properties in one request, while a raw walk into a hosted plug-in has measured 60–300 ms against a pixel's 17. Detection and activation can afford that; a focus step cannot.

The trap is that **not finding something and not being able to see anything are the same `false`.** A DAW-embedded Kontakt 8 is a single leaf; Kontakt's instrument editor is absent from a tree that lists its header and status bar; a probe of a REAPER FX window on macOS found 22 elements of which every one belonged to REAPER. So dump the window before building on any of this, and where a miss could mean either, decide in advance which way to fail — Kontakt's rack-view test counts "the plug-in is not answering" as yes, because a wrong no hides seven controls from somebody who cannot see that they are gone.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["uia"]
```

See [what that list is and is not](./index.md#capabilities).

## host.element.find(hwnd, name, controlType) {#host-element-find}

**Signature:** `host.element.find(hwnd: number, name: string, controlType: number) -> boolean`

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
local hwnd = host.window.active().id
if host.element.find(hwnd, "PlogueXMLGUI", 50033) then
    host.log.info("Kontakt detected")
end

-- Does the window contain *any* Edit control?
if host.element.find(hwnd, "", 50004) then
    host.log.info("has at least one edit box")
end
```

### Windows

A subtree search performed by UIA itself, with no traversal budget of ours.

The documented "is a menu open" idiom — an empty name with `host.element.type.Menu` — is exactly that: an ordinary search for any element of that control type.

### macOS

A hand-rolled walk capped at **1500 nodes and depth 24**. Exhausting that budget returns the same "not found" a completed search returns, and only a once-per-session log line tells them apart — so a large or deep plug-in tree can make an identity check answer `false` permanently here while succeeding on Windows, with nothing to distinguish that from "this is not the plug-in".

The menu idiom is special-cased rather than searched: it asks the *application* whether a menu is open, walking its children and one level into its other windows, and never descends the menu bar itself. `locate` has no such special case.

## host.element.locate(hwnd, name, controlType) {#host-element-locate}

**Signature:** `host.element.locate(hwnd: number, name: string, controlType: number) -> { x: number, y: number } | nil`

Finds the first UIA element in window `hwnd` matching `name` + `controlType` and returns the screen-pixel centre of its bounding rectangle as `{ x, y }`, suitable for clicking. Returns `nil` if no match. As with `find`, an empty `name` matches any element of the given control type. Same control-type ids as above.

```luau
local hwnd = host.window.active().id
local pt = host.element.locate(hwnd, "Play", 50000) -- Button named "Play"
if pt then
    host.input.click(pt.x, pt.y)
end
```

### macOS

Subject to the same **1500 node / depth 24** traversal budget as `host.element.find` — a tree larger than that answers "not here" to every question, indistinguishably from an honest miss.

## host.element.type {#host-element-type}

**Signature:** `host.element.type: { [string]: number }`

The UIA ControlType ids by name — `host.element.type.Button` (50000), `.Edit`, `.Menu`, `.MenuItem`, `.Tab`, `.Window` (50032), `.Pane` (50033), and the rest. Pass these instead of writing the integers at call sites.

```luau
local U = host.element.type
if host.element.find(hwnd, "Kontakt 8", U.Pane) then … end
```

### Windows

Every constant in the table is a real UIA property condition and can match.

### macOS

The ids are translated to accessibility roles, and **ten of them have no mapping at all**: `Calendar`, `StatusBar`, `Thumb`, `DataGrid`, `DataItem`, `Document`, `SplitButton`, `Header`, `HeaderItem` and `TitleBar`. A query whose control type is one of those is well-formed, can never succeed, and says so only in the log — which the module never sees.

`findAny` behaves differently again: given several types it **drops the unmapped ones and searches the rest**, returning nothing only when every type it was given was unmapped. So the same unmapped id makes `find` impossible and makes `findAny` quietly narrower.

## host.element.findAny(hwnd, names, types) {#host-element-findany}

**Signature:** `host.element.findAny(hwnd: number, names: {string}, types: {number}) -> number | nil`

Answers "is any of these names present as any of these control types?" — the shape a plugin-identity check takes, since a plugin may expose its name as a Window on one host and a Pane on another. Returns the **1-based index of the matching name**, so a caller learns *which* one matched (e.g. the plugin's version) rather than asking once per candidate. `nil` if none match; an empty name matches any element of the given types.

One tree traversal per name, with the types folded into the condition — where a loop of `find` calls costs one full subtree walk per name×type pair. This runs on every detection pass, per candidate control, so the difference is not academic.

```luau
local U = host.element.type
local VERSIONS = { "Kontakt 8", "Kontakt 7" }
local i = host.element.findAny(ctrl.id, VERSIONS, { U.Window, U.Pane })
if i then host.log.info("this is " .. VERSIONS[i]) end
```

## host.element.pluginLocate(hwnd, containerName, name, controlType) {#host-element-pluginlocate}

**Signature:** `host.element.pluginLocate(hwnd: number, containerName: string, name: string, controlType: number) -> { x: number, y: number } | nil`

Like `locate`, but for a plugin hosted inside another application. It first finds the element that **is** the plugin — Name `containerName` (e.g. `"Kontakt 8"`) with control type Window (50032) or Pane (50033), preferring the `ni::qt::QuickWindow` class (a Qt plugin's scene root, which carries the content) over the `…QWindowIcon` window host — and then searches for the target **within** it, using the **raw** tree walker. A port of ReaHotkey's `GetPluginUIAElement` + `MainElement.FindElement(...)`.

The raw walker matters: the condition-based search (`find` / `locate`) stops at a hosted fragment's boundary, so a plugin's UI can look entirely absent to it. Note that some plugins expose no accessible content when embedded regardless — a DAW-embedded Kontakt 8 is a UIA **leaf** — in which case this returns `nil` and only coordinates remain.

```luau
local pt = host.element.pluginLocate(hwnd, "Kontakt 8", "Kontakt File Menu", 50000)
if pt then host.input.click(pt.x, pt.y) end
```

### Windows

A hand-rolled walk of its own, bounded at 4000 nodes and depth 40 — deliberately larger than the ordinary search, because a hosted plug-in fragment is where the deep trees are.

### macOS

The general **1500 node / depth 24** budget applies here too; there is no larger allowance for this call. A plug-in tree that this reaches on Windows can be out of reach here, and the miss looks identical to a genuine one.

## host.element.rawDump(hwnd) {#host-element-rawdump}

**Signature:** `host.element.rawDump(hwnd: number) -> { { depth: number, name: string, class: string, ctype: number }, … }`

Diagnostic counterpart to `host.element.dump`, walking the **raw** tree instead of a condition-based search, so it crosses into hosted fragments the latter cannot see. Bounded by node budget and depth. Use it to find out whether a plugin exposes any accessible content at all before building on UIA.

Each node also carries `bounds = { x, y, w, h }` in screen coordinates, which the signature above omits and which is usually the reason to reach for this: a rectangle measures things nothing else can — a window's own close button says how tall its title bar is, and that is the difference between an authored coordinate landing on a control and landing a title bar above it.

```luau
-- Does this plug-in publish anything at all? The probe dumps the whole tree, because on a
-- machine nobody here can see, the log is the only way to find out.
local win = host.window.active()
local elements = win and host.element.rawDump(win.id) or {}
host.log.info(string.format("%d element(s) in '%s'", #elements, win and win.title or ""))
for _, e in ipairs(elements) do
  local b = e.bounds
  host.log.info(string.format("  %sdepth=%d type=%d class='%s' name='%s' at %d,%d %dx%d",
    string.rep(" ", math.min(e.depth, 12)), e.depth, e.ctype, e.class, e.name,
    b.x, b.y, b.w, b.h))
end
```

## host.element.dump(hwnd) {#host-element-dump}

**Signature:** `host.element.dump(hwnd: number) -> { { depth: number, name: string, class: string, ctype: number, bounds: { x: number, y: number, w: number, h: number } }, … }`

The condition-based counterpart to [`host.element.rawDump`](#host-element-rawdump), and the first thing to run against a window nobody here has seen: every element of `hwnd`'s subtree that carries a Name or a class key, as `{ depth, name, class, ctype, bounds }`. It is what tells you the Name/ControlType pair to key an identity check on, and the rectangles are what a coordinate can be authored against. Where this comes back empty or suspiciously thin against a plug-in hosted inside another application, that is the fragment boundary the condition-based search stops at — reach for `rawDump`, which crosses it.

`bounds` is in screen coordinates. An element the toolkit never places reports all zeros, which is a legitimate answer and not a miss. Every property is fetched across the process boundary per element, so this belongs on a diagnostic hotkey, not on a focus step.

```luau
-- Ctrl+Alt+U in tools/inspect: dump the focused window, then every Qt plug-in control in
-- it — on a machine nobody here can see, the log is the only way to learn what a tree holds.
local win = host.window.active()
local function dumpOne(label, id)
  host.log.info(string.format("UIA DUMP %s id=%d", label, id))
  for _, e in ipairs(host.element.dump(id)) do
    host.log.info(string.format("  type=%-5d class='%s' name='%s'", e.ctype, e.class, e.name))
  end
end
dumpOne("window '" .. win.title .. "'", win.id)
for _, c in ipairs(host.window.controls()) do
  if string.match(c.class, "Qt%d") then dumpOne("control [" .. c.class .. "]", c.id) end
end
```

### Windows

One `FindAll` over the subtree, returning a **flat** list capped at the first 2000 elements: `depth` is `0` on every node here, so nesting has to be read from the rectangles rather than from the field. `rawDump` is the one that reports real depth.

### macOS

Accessibility has only one view of the tree, so `dump` and `rawDump` are the same call and return the same list — a `dump` is never the thinner answer it can be on Windows. Bounded at **2000 nodes and depth 24**, and `depth` is the real nesting depth.

`class` is not a class name, because this platform has none: it is the joined `role/subrole/AXIdentifier` key, printed exactly as a module's pattern would match it, so what appears in the log can be pasted straight into the module.

## host.element.locateVia(hwnd, viaName, viaType, name, controlType) {#host-element-locatevia}

**Signature:** `host.element.locateVia(hwnd: number, viaName: string, viaType: number, name: string, controlType: number) -> { x: number, y: number } | nil`

Like [`locate`](#host-element-locate), but in two levels: it first finds a container element (`viaName` + `viaType`) anywhere under `hwnd`, then searches for the target **within that container** and returns its click centre. That matters for a plug-in that publishes an identity pane and hangs its real UI off it as a nested fragment — a search that starts at the window misses what a search that starts at the container finds. An empty `name` (or `viaName`) matches any element of the given type, as elsewhere in this module. Two subtree searches, so roughly twice the cost of one `locate`.

The difference from [`pluginLocate`](#host-element-pluginlocate) is that you choose the container's control type here; `pluginLocate` accepts only a Window or Pane container, prefers the Qt scene root over the content-empty window host, and searches with the raw walker. That is the harder-edged version and the one the shipped Kontakt module uses — nothing in `modules/` calls `locateVia`. Reach for it when the container you need is neither a Window nor a Pane.

```luau
local U = host.element.type
-- Take both names from a dump first: the container is the element that publishes the
-- plug-in's identity, the target is what you want to click inside it. Shown with a Pane
-- container for familiarity; a Pane is also what `pluginLocate` would handle for you.
local fg = host.window.active()
local pt = fg and host.element.locateVia(fg.id, "Kontakt 8", U.Pane, "Kontakt File Menu", U.Button)
if pt then
  host.input.click(pt.x, pt.y)
else
  host.log.info("no 'Kontakt File Menu' inside the 'Kontakt 8' pane")
end
```

### Windows

Both levels are condition-based `FindFirst` calls: the container must itself be reachable by a condition-based search from `hwnd`, and there is no raw-walker pass here — where `pluginLocate` uses the raw walker for both of its levels.

### macOS

Control types are translated to accessibility roles, so an unmapped type at **either** level makes the call impossible — it can never match, and says so only in the log (see [`host.element.type`](#host-element-type)). Each level gets its own **1500 node / depth 24** budget, and an exhausted budget returns the same `nil` an honest miss returns.

## host.element.stateProbe(hwnd, containerName, name, controlType) {#host-element-stateprobe}

**Signature:** `host.element.stateProbe(hwnd: number, containerName: string, name: string, controlType: number) -> { toggle: number?, legacyState: number, checked: boolean } | nil`

Asks a named element what it reports about its **own** state, rather than inferring one from its control type. It exists because "the accessibility tree has nothing to offer here" turned out to be a conclusion drawn from the type — Kontakt's status-bar toggles publish as plain Buttons — by code that had only ever fetched Name, ClassName and ControlType and so had never observed the absence of a state, only assumed it. `nil` means the element was not found at all, which stays distinguishable from "found, and it says nothing".

It is **expensive**: the same raw tree walk as `pluginLocate`, measured at 60–300 ms on the message pump against roughly 17 ms for a single pixel read. It is the fallback for what a cheap pixel probe cannot answer, never the primary route. And a missing `toggle` is not "off" — Kontakt's Side and Info panes carry no state at all, so a module that turned that into `false` would be inventing an answer.

```luau
local U = host.element.type
-- Kontakt's status-bar toggles publish as plain Buttons, so the type says nothing and the
-- element has to be asked. Adapted from modules/kontakt/src/actions.luau (uiaChecked).
local KEYBOARD = "Keyboard (F3): Toggles the visibility of the Keyboard panel, which shows the virtual keyboard."
local fg = host.window.active()
local st = fg and host.element.stateProbe(fg.id, "Kontakt 8", KEYBOARD, U.Button)
if not st then return nil end                       -- not found: no answer at all
if st.toggle ~= nil then return st.toggle == 1 end  -- 0 off, 1 on
-- Found, and it carries no toggle state: `st.checked` is false for an element that never
-- sets the bit, and reporting that as "collapsed" would be inventing an answer.
return nil
```

### Windows

`legacyState` is a real `LegacyIAccessible` state word, so bits beyond CHECKED (`0x10`) carry information: `0x08` PRESSED, `0x04` FOCUSED, `0x100000` FOCUSABLE. Read them with care — Kontakt's Side and Info panes were seen going `0x100000` → `0x100004` across a press, and that `0x4` is the consequence of our own click rather than a panel state.

The search is confined to elements named `containerName` with control type Window or Pane (the `ni::qt::QuickWindow` class preferred over the window host), and a window with no container of that name answers `nil` without looking further.

### macOS

There is no state word here. `toggle` comes from `AXValue` — `0` off, `1` on, `2` for anything else, absent when the element carries no value — and `legacyState` is synthesised from it: `0x10` when `toggle` is `1`, otherwise `0`. Only `checked` is meaningful; the other bits are never set, so a module that reads PRESSED or FOCUSED gets a permanent zero.

If no container of that name exists, the search falls back to the whole window rather than giving up — the container is a fragment boundary on Windows, and is not one here.

## host.element.classNavPoint(hwnd, className, controlType, child, sibling) {#host-element-classnavpoint}

**Signature:** `host.element.classNavPoint(hwnd: number, className: string, controlType: number, child: number, sibling: number) -> { x: number, y: number } | nil`

Finds the first element whose class contains `className` and whose control type is `controlType`, walks a fixed path from it, and returns that element's click centre. For the case where the control you want carries no name of its own but sits at a known position next to one that is identifiable: Komplete Kontrol's library-browser toggle is the previous sibling of the browser pane, Kontakt's What's-New close button is the second child of its `WhatsNewScreen`. A port of ReaHotkey's `FindElement(ClassName)` + `WalkTree(path)`, and it walks the raw tree, so it reaches into a Qt plug-in's content that a condition-based search stops short of.

`child` is 1-based with `0` meaning "do not descend"; `sibling` is signed, negative walking backwards. `nil` covers both "not found" and "found but off-screen", which makes it usable as a presence probe — KK reads the `nil` as "no browser is open" and asks nothing else.

```luau
local U = host.element.type
-- KK's browser-close toggle has no name of its own: it is the PREVIOUS SIBLING of the
-- browser pane. Doubles as the "is a browser open?" probe — nil means closed.
-- From modules/komplete-kontrol/src/main.luau (browserToggle).
local function browserToggle(hwnd)
  if not hwnd then return nil end
  return host.element.classNavPoint(hwnd, "FileTypeSelector", U.Tab, 0, -1)
    or host.element.classNavPoint(hwnd, "TagCloudAccordionWithBrands", U.Pane, 0, -1)
end
```

### Windows

Matches on ClassName containing the substring, searched depth-first over the raw view to depth 25, and the sibling steps are raw-view siblings.

### macOS

There are no class names. The first pass matches `AXIdentifier` containing the substring — Qt fills that from `objectName`, which is where the ReaHotkey class names came from in the first place — and only if that finds nothing does a second pass try `AXDescription`, for toolkits that leave the identifier empty. An identifier match therefore always beats a description match.

Siblings are counted within the parent's own children, and a step off either end returns `nil` rather than clamping.

## host.element.focusStep(hwnd, direction) {#host-element-focusstep}

**Signature:** `host.element.focusStep(hwnd: number, direction: number) -> { name: string, ctype: number, index: number, count: number } | nil`

**This one writes.** It enumerates the visible, keyboard-focusable descendants of `hwnd`'s content area and *moves keyboard focus* to the next (`direction >= 0`) or previous one, wrapping at the ends, returning the element it landed on. It is the Tab pass-through for a standalone plug-in window that does not move focus on Tab by itself (Kontakt standalone), and it is the only way into such a window's native controls. Because it raises a real focus event, the screen reader announces the element — so the overlay must deliberately stay quiet rather than speaking `name` itself.

Candidates are re-enumerated every step, since the tree changes shape as the user moves; and a candidate that accepts the focus request without actually taking it is skipped, verified by reading focus straight back. What the ring is scoped to differs by platform — see below. It **wraps**, so "the ring is finished" is not something it reports — the caller remembers the 1-based `index` it entered at and, from each result's `index` and `count`, works out whether the *next* step would land there again; if so it hands Tab back on that press **without** calling this, because calling it would move the plug-in's focus onto the entry element a second time and the screen reader would name it twice. A `count` that changes mid-lap means the plug-in's tree changed (a panel was shown or hidden), and the lap starts afresh. `nil` means nothing in the scope accepted focus at all. A module normally gets this through `Overlay:addPassThrough`.

```luau
-- One Tab inside the pass-through control, shortened from modules/overlay-runtime/src/main.luau
-- (Overlay:_stepPassThrough). The lap ends BEFORE the step that would close it.
if c._leaveOn == dir then               -- last press saw the entry come up next
  c._entry, c._count, c._leaveOn = nil, nil, nil
  return false                          -- hand Tab back without touching the plug-in's focus
end
local r = host.element.focusStep(ctrl.id, dir)
if not r then return false end          -- nothing focusable in there: let Tab carry on
if c._entry == nil or r.count ~= c._count then
  c._entry, c._count = r.index, r.count -- a new lap, or the tree changed under the old one
end
local nxt = ((r.index - 1 + (dir >= 0 and 1 or -1)) % r.count) + 1
c._leaveOn = (nxt == c._entry) and dir or nil
return true
```

### Windows

The ring is scoped to the window's first child — ReaHotkey's content area — which drops the frame and the menu bar without a control-type blocklist. Candidates are the descendants the provider reports as keyboard-focusable and not off-screen, and focus is moved with UIA's own `SetFocus`. No traversal budget of ours applies — the whole content subtree is enumerated however large it is.

### macOS

The ring is the **whole window**, minus the window's own close, minimise, zoom and full-screen buttons, which are skipped by subrole along with anything under them. Not the first child, as on Windows: on macOS that is whatever the application published first, and in a standalone Kontakt 7 it is Kontakt's logo — a button with no children, which made the ring empty. The menu bar is not inside a window on this platform, so nothing else needs excluding. Focus is moved by setting `AXFocused`, and a candidate qualifies when that attribute is *settable* and the element has a non-zero rectangle. The walk is bounded at **800 nodes and depth 20** — tighter than the other calls here, because it runs per keystroke. Elements below depth 20 are simply not in the ring, with nothing in the log to say so; running out of the 800-node budget does produce one line, once per session.

## host.element.focusWithin(hwnd, rect) {#host-element-focuswithin}

**Signature:** `host.element.focusWithin(hwnd: number, rect: { x: number, y: number, w: number, h: number }) -> { name: string, ctype: number } | nil`

**This one writes.** It gives keyboard focus to the **first** keyboard-focusable element of `hwnd` whose centre lies inside `rect` (screen coordinates), and returns the element it landed on. It is the polite half of "put the keyboard back into the plug-in": focusing the *window* hands the keyboard to whatever that window last had focused — in REAPER, its FX list — and the only thing that moved it on from there used to be a click into the plug-in's panel. A plug-in that publishes its elements can simply be asked, which is what a screen reader's own navigation does, raises a real focus event, and presses nothing. `nil` means nothing inside the rectangle took focus — a plug-in that publishes no element has nothing to ask, and the caller falls back to what the platform does understand.

Because it raises a real focus event, the screen reader announces the element; a caller that speaks afterwards should name the element rather than repeat it.

```luau
-- modules/daw-hosts/src/main.luau, the F6 shortcut on macOS: ask before clicking.
local given = host.element.focusWithin(w.id, panel)
if given then
  host.speech.output(title .. ", on " .. given.name)
else
  host.input.click(panel.x + 3, panel.y + 3)   -- sforzando publishes nothing to ask
end
```

### Windows

Not implemented — returns `nil`. Screen-reader users have OSARA's F6 there, and the plug-in host's own controls are reachable by Tab.

### macOS

Walked from the window itself, not from its first child as `focusStep` is: inside REAPER's FX window Kontakt's elements *are* the window's children, with no content group to scope to. The rectangle is the scope instead. A candidate qualifies when `AXFocused` is settable and its centre is inside the rectangle; the first in tree order that actually takes the focus (read back from the system-wide focused element, not assumed) wins, so a text field near the top of a panel is what usually gets it. Same bounds as `focusStep`. One log line either way, naming the element and its role, or the number of candidates that accepted the question and then did not take the focus. Unverified on hardware as of 2026-09-11.
