# Nested plugin overlays — design

How we support overlays for **plugins inside plugins**: a DAW hosts Komplete
Kontrol (KK), KK hosts Kontakt, and Kontakt hosts a sample library (Cinematic
Studio Series, Audio Imperia, …). Each level may contribute overlay controls.
The model is ported from ReaHotkey (AHK), whose working implementation we
studied; this doc records the design and the concrete facts to port.

## The model: overlay = control tree + portal slots + flattener

- An **overlay is a tree of controls**. A node is a control with a stable id and
  a parent link; an *overlay* is a control that is also a focus container
  (`ChildControls`). Leaves: Button, HotspotButton (click x,y), ToggleButton
  (state read by pixel colour **or** by template image), Slider (value from a
  template's matched position), ListBox, StaticText, **Passthrough** (an empty
  container that hands Tab/arrows to the underlying native/UIA elements), and
  **Tab** (itself a sub-overlay; only the active tab's children are focusable).
- **Portal / slot (the React-portal idea, confirmed):** a parent overlay
  reserves a **slot** (an empty placeholder overlay). A child overlay is mounted
  into it (`mount(parent, slot, child)`) and unmounted back to empty. An empty
  slot contributes **zero** focusable entries.
- **Flattener:** the host computes the Tab order on demand by walking the tree
  and **inlining** a child overlay's focusables at the parent level (the wrapper
  itself is *not* a focus stop); for a Tab control it emits the tab control then
  inlines only the active tab. So a parent's own controls (e.g. Kontakt's header)
  and a mounted child's controls (the library) form **one flat list**, and
  swapping a slot instantly changes navigation — no re-registration.
- **Current/previous control live on the ROOT only**; speech de-dups against
  previous (re-landing on a toggle announces only its new state).

## Detection — three layers, most-specific wins

Identity is **not** OCR-of-a-name. Each level uses a different primitive:

1. **DAW → KK, KK → Kontakt: window control-class regex + UIA name/type.** The
   embedded plugin is a child control of the host window (found by scanning its
   controls). KK and Kontakt 7/8 share the same Qt class — disambiguate **only**
   by UIA `Name` ("Komplete Kontrol" / "Kontakt 7" / "Kontakt 8"). Kontakt-in-KK
   is **not a separate HWND**: it's a sibling control deeper in KK's control list
   (KK walks its controls *past* its own anchor to find it).
2. **Kontakt → library: image template match.** Each library ships a
   `Product.png` landmark; the dispatcher iterates candidates and the **first**
   image found inside the plugin rect wins. **No OCR, no name read.**
3. **OCR: only for transient pixel values** (snapshot/preset name), never for
   identity.

## Provider stack + origin stacking + triggers

- Resolve the nesting **top-down** into an explicit **provider stack**
  (DAW → KK → Kontakt → library), cache it, re-resolve on focus change.
- **Origin stacks:** controls are authored **relative**; the host resolves
  absolute coords by **summing the stack's rects/offsets** at focus/activate time
  (idempotent: cache the authored value, always recompute). Each level adds its
  offset; per-host offset constants absorb Kontakt7 vs Kontakt8 vs KK geometry.
- **Triggers:** we are event-driven where ReaHotkey polls. Plugin-level changes
  ride `EVENT_OBJECT_FOCUS` / `watch_foreground`. The **library level** has no
  focus event when the user loads a different `.nki`, so it needs a **targeted,
  debounced poll** (image match in the plugin rect) while Kontakt is active.
- **Sticky:** confirm the current overlay first; only switch on failure (no
  flicker).

## Composition

- The container **prepends** its generic header into *every* library overlay
  (slot 1) and **appends** a Passthrough (Tab continues into native elements).
- **Hotkeys merge** from the resolved stack (host ∪ plugin ∪ library), rebound
  atomically per focus-container change (fits our transactional/idempotent arm).
- **Fallback:** a `NoLibrary` overlay (no landmark → never auto-selected, always
  registered, the resting default = header + passthrough) + a manual
  "Choose library" menu (Vendor → Product → Patch from metadata) for override.

## Host APIs

Have: `host.window.controls`, `host.window.focusChain`, `host.element.find`,
`watch_foreground` + `EVENT_OBJECT_FOCUS`, OCR (WinRT + PaddleOCR),
**`host.screen.imageSearch(template, {region, tolerance})`** (template match — the
library-identity primitive, already implemented; honours the template's alpha as a
mask), `host.path`/`host.resource` (absolute package paths), the overlay prelude
(contexts/activeCtx/attachEmbedded/navigation/origin), and **`host.require(id)`**
— for a [`code_module`](module-package-format.md) dependency the dep's code is
evaluated *inside the dependent's VM*, so its returned table crosses with
**functions intact** (the mechanism the decentralised-library decision below rests
on). *(An earlier soft-discovery primitive `host.providers(contract)` was designed
here but never needed — the inheritance model subsumed it and it has since been
removed.)*

**Decentralised libraries (key decision):** a **library is its own
`code_module`** that **depends on the container** (`kontakt`) and, evaluated into
the container's VM, calls the container's exported builder directly —
`kontakt.library(name, image, function(ov) ov:addHotspotButton{…} end)` — with
**closures intact**. (The original plan here was data-only descriptors queried via
`host.providers`, because plain VM isolation can't pass functions; the
inheritance model — [`code_module`](module-package-format.md) deps evaluated into
the dependent VM — removed that limit, so libraries now ship real control-builder
code, not just declarative tables.) New library support = a small installable
`code_module`.

New (to build):
- Overlay prelude → **control tree with portal slots + recursive flattener**, the
  control types (HotspotButton, GraphicalToggle by image, ToggleButton by pixel,
  Slider, Passthrough), and a reusable **container** (attach + library registry +
  image-detection poll + sticky swap + per-container offset).
- **Origin stacking** in the host (needed at Phase B, two embedding levels).

## Module shapes

- **Container** (e.g. `kontakt`, a `code_module`): attaches to the plugin
  (control-class + UIA identity), holds generic header controls + a per-container
  coordinate offset, and **exports a `library(name, image, build)` builder**. Each
  library that depends on it calls that builder (evaluated into the container's VM)
  to register itself; the container runs the image-detection poll and mounts the
  matched library's controls. A built-in NoLibrary fallback (header only) is the
  resting default. May itself be hosted by KK (Phase B).
- **Library** (e.g. `cinematic-studio-strings`, a `code_module`):
  `dependencies = ["…kontakt"]`, entry **calls**
  `kontakt.library(name, image, function(ov) …control builders… end)` — real
  closures, image paths absolute via `host.path`. One module may carry several
  products/patches. Installable on its own via the manager (topic `osap-module`) —
  decentralised, no central list.

## Concrete facts (from ReaHotkey, to port)

- Kontakt control-class regex (Qt): `^Qt6[0-9][0-9]QWindowIcon\{GUID\}1$` (+ an
  `NI_6_x_y_Rz` variant); same regex as KK → disambiguate by UIA name.
- UIA anchor: ClassName `ni::qt::QuickWindow` then substring `QWindowIcon`; accept
  the element whose `Name` is "Kontakt 8" / "Komplete Kontrol" and ControlType
  50032 (Window) / 50033 (Pane/Group). Type constants: Button 50000, Edit 50004,
  Menu 50009, MenuItem 50011, Window 50032, Group 50033.
- CSS landmark: `Images/SampleLibraries/CinematicStudioStrings/Product.png`
  (Strings) + Brass; controls authored relative + per-host offsets
  (e.g. `Kontakt8YOffset := 29`, `KompleteKontrolXOffset/YOffset := 190/111`).
- Library poll: ~500 ms, gated by a config flag, sticky.
- Detection of nested Kontakt-in-KK: scan KK's controls *backward* past KK's own
  anchor; confirm by UIA name; resolve ambiguous sub-windows (browser) by UIA
  numeric-path depth.

## Phases

- **A (one embedding level):** Kontakt directly in a DAW + **one** library (CSS).
  Validates portal/slot + flattener, image-template detection (sticky,
  first-match), header prepend, single origin. ~90 % of the machinery.
- **B (two levels):** Kontakt inside KK. Adds the second origin layer + the
  KK-as-host control scan + hotkey merge.
- **C:** more libraries (just a `Product.png` + controls + offsets) + the manual
  chooser.

## Gotchas (load-bearing)

- Origin compensation must run **every focus/activate**, idempotent by caching the
  authored value — never offset once at build (double-offsets otherwise).
- The fallback (`NoLibrary`) must always be **registered and the default**, or
  Tab navigation of the raw plugin breaks when nothing matches.
- Don't key plugin identity on window/control class alone — KK and Kontakt share
  it; read UIA **name + type**.
- Nested plugin is a **sibling control**, not a child HWND; detect by control
  order/depth, not a window handle.
- Per-host offsets are mandatory — the same library description re-anchors under
  Kontakt7 / Kontakt8 / KK by adding the host's offset to the plugin-rect origin.
- Library identity is **image template match**, never OCR. Reserve OCR for
  transient value readouts.
- Image matching every frame is expensive — the debounced poll + an active-context
  gate is load-bearing, not incidental.
