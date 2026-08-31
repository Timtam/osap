---
title: "com.platform.overlay (O) — self-voicing overlay control layer"
sidebar_position: 6
---

The overlay API is a **code module**, not a host namespace: declare `com.platform.overlay` as a dependency and pull it in with `host.require` (aliased `O` throughout). `O.new` returns an `Overlay` object whose methods are called with `:`. Controls are added to a virtual tree, navigated by keyboard, and spoken as `"label, type[, value]"`. All control coordinates are **origin-relative**: the origin is the client-area top-left (in screen pixels) of the active context's coordinate window — the plugin window when standalone, or the embedded plugin's child control when hosted in a DAW — re-resolved per call so it tracks window moves; `(0, 0)` when unattached.

```lua
local O = host.require("com.platform.overlay")
```

Source: `modules/overlay-runtime/src/main.luau`.

Control **kinds**: `static`, `hotspot`, `custom`, `ocr`, `gtoggle`. The spoken type label is `""` (static), `"button"` (hotspot/custom/ocr), `"toggle button"` (gtoggle). **Every kind is a focus stop** — `static` text is Tab-reachable and read aloud, it just has no activation (Enter does nothing on it).

## O.new(label)

Creates a new overlay object. `label: string?` (defaults to `"Overlay"`).

Returns an `Overlay` (metatable-backed table) with fields `label`, `controls = {}`, `focus = 0`, `active = false`, `hoverToRead = false`, `contexts = {}`, `activeCtx = false`, and internal registration/key/trigger state.

```lua
local ov = O.new("My Plugin")
```

## O:addStaticText(label)

Appends a static text control: Tab-reachable and read aloud on focus, but with no activation (Enter does nothing). `label: string`.

Returns the control table `{ kind = "static", label = label }`.

```lua
ov:addStaticText("Mixer section")
```

## O:addHotspotButton(opts)

Appends a button that, when activated, clicks a fixed origin-relative point. `opts: { label: string, at: {number, number}, hotkey: string?, rawOrigin: boolean?, fromRight: boolean? }` — `at` is `{x, y}` relative to the origin; `hotkey` is an optional global activation hotkey spec (e.g. `"Alt+P"`).

Returns `{ kind = "hotspot", label, at, text, hotkey, rawOrigin, fromRight, opensMenu, when }`. On activate it clicks `(origin.x + at[1], origin.y + at[2])` and speaks `"label, activated"`.

Two checks come before the click. The point must fall inside the origin's own frame, and — where the platform can say — the window it belongs to must be the one actually **drawn** at that point; see [`host.window.ownsPoint`](window#hostwindowownspointid-x-y). A coordinate inside our rectangle can still be covered by a notification or another application, and a click that lands somewhere unknown while the overlay announces "activated" is a press with no way to tell where it went.

`text` is announced between the label and the word "button", and is how a control that acts can also say something about itself — whether it is available, or what it would act on.

`fromRight` measures `at[1]` from the coordinate window's **right edge** instead of its left — the click lands at `origin.x + width − at[1]` (ReaHotkey's `ControlX + ControlWidth − N`). Use it for plugin UI laid out from the right, so the target stays correct whatever the plugin's width is. Also accepted by [`addHotspotToggle`](#addhotspottoggle).

`points` replaces `at` with a **click sequence** — `points = {{x1,y1},{x2,y2},…}`, with `settle` ms between each (default 400) — for UI where reaching a control means getting there first: switch to its tab, then its sub-tab, then click it. Each step re-resolves the origin and goes through the overlay's own coordinate resolution, so a frame offset, `rawOrigin` or landmark anchoring applies exactly as for a single point. Making that one control keeps every action self-contained, so a user who cannot see a tab structure never has to navigate it.

`at` may also be a **function** `(overlay) -> {x, y} | nil`, for a control whose position depends on what is focused right now — one overlay serving several versions of a plugin whose chrome moved between them. Returning `nil` (the version isn't known yet) skips the click rather than guessing. Also accepted by `addHotspotToggle`.

```lua
ov:addHotspotButton({ label = "Play", at = { 120, 40 }, hotkey = "Alt+P" })
-- 352 px in from the right edge, 87 px down — Kontakt's instrument arrows:
ov:addHotspotButton({ label = "Previous instrument", at = { 352, 87 }, fromRight = true, rawOrigin = true })
```

## O:group(pred, build)

**Signature:** `O:group(pred: (overlay) -> boolean, build: (overlay) -> ()) -> Overlay`

Adds everything `build` adds under a shared condition: `pred` is ANDed onto each control's own `when`, and groups nest. Prefer it over repeating the same `when` on every control of a set — forgetting one is silent.

```luau
ov:group(bare, function(ov)
    ov:addCustomButton({ label = "Load instrument", hotkey = "Ctrl+L", onActivate = load })
    ov:group(isVersion("Kontakt 7"), function(ov)
        ov:addHotspotButton({ label = "Library on/off", at = { 231, 19 }, hotkey = "Alt+L" })
    end)
end)
```

A `when` predicate must return exactly `true`; anything else counts as hidden. A hidden control is not Tab-reachable, its hotkey does nothing, and it does not claim a key combination that a visible sibling wants.

## Bindings — O.window / O.embedded / :with / O.hosts / O:bind

**Signatures:**
`O.window(matcher, opts?) -> Binding` · `O.embedded(spec, opts?) -> Binding` · `Binding:with(over) -> Binding` · `O.hosts(...) -> {matcher}` · `O:bind(binding, opts?) -> Overlay`

A **binding** is an inert value saying *where* an overlay lives: which window or embedded control, on which `slot`, at which `specificity`, with `pollMatch` / `menus`. Declare it once and reuse it, instead of writing a factory function that rebuilds an attach spec per attachment — the reason such factories appear is that a caller mutates the table and the next caller needs a clean copy.

`:with(over)` returns a **copy** with `over` merged into the target (`control`, `identify`, `title`, …); `over.opts` merges into the options. `O.hosts(...)` concatenates host lists and skips `nil`, so an optional host (a plugin that may not be installed) can be listed inline. `O:bind(binding, opts)` attaches, with `opts` overriding the binding's own options.

```luau
local HOSTED = O.embedded({
    hosts = O.hosts(daw.all, kk and kk.standaloneWindow),
    control = "Qt%d+.-QWindowIcon",
    identify = identifyHost,
    cacheIdentity = false,
}, { slot = SLOT, menus = true })

base:bind(HOSTED, { specificity = O.layer.base })
dialog:bind(HOSTED:with({ control = "^NIChildWindow%x+$", identify = present,
                          opts = { menus = false } }), { specificity = O.layer.dialog })
```

One overlay object takes **one** binding: an object is pinned to a single slot and its match poll belongs to that join. Two places to live means two overlay objects sharing a build function.

## O.layer

The specificity ladder within a slot, named: `chrome` (a host's own frame), `base` (the plugin's generic header), `content` (what is loaded inside it right now), `dialog` (a modal that must own the keyboard). Pass `specificity = O.layer.content` rather than a bare number.

## O.memoByOrigin(fn, opts?)

**Signature:** `O.memoByOrigin(fn: (origin, ...) -> value, opts: { key: ((origin) -> any)? }?) -> (origin, ...) -> value`

Memoizes a per-window property that does not change while that window exists — which plugin version it is, whether it is a host container, where its content begins.

One rule: a **non-nil** result is cached; **`nil` means "cannot tell yet" and is not cached**. Identity is usually resolved through UIA, which is not ready the instant a plugin window appears, and pinning that first answer is how an overlay ends up permanently convinced it is driving a different plugin version. `false` is a real answer and *is* cached.

`opts.key(origin)` extends the cache key past the window handle — for a property that survives a window *move* but not a *resize*.

```luau
local variantOf = O.memoByOrigin(function(ctrl)
    local i = host.uia.findAny(ctrl.id, VERSIONS, { U.Window, U.Pane })
    return i and VERSIONS[i] or nil   -- nil: UIA not ready, ask again next time
end)
```

## O:origin() / O:hwnd()

**Signature:** `O:origin() -> Control | Window | nil` · `O:hwnd() -> number | nil`

The active context's coordinate window — the plugin control when embedded, the window when standalone — and its handle. `nil` while the overlay is not active. This is the module's handle on the thing it overlays; use it instead of reaching into `activeCtx`.

```luau
onActivate = function(o)
    local p = host.uia.locate(o:hwnd(), "", host.uia.type.Edit)
    if p then host.input.click(p.x, p.y) end
end,
```

## O:frame(fn)

**Signature:** `O:frame(fn: (origin) -> (number, number)) -> Overlay`

Shifts the overlay's whole coordinate frame: `fn` returns `dx, dy`, resolved per active control — a host version's content shift, or a nested plugin's inner origin. Controls marked `rawOrigin` opt out.

## O.state

A free-form table on every overlay for the owning module's own state, so it does not have to squat in the runtime's reserved `_`-prefixed fields.

## ocrLabel — reading a control's name off the screen

Any hotspot or hotspot-toggle may carry `ocrLabel = {x1, y1, x2, y2}` (origin-relative): the control's spoken **name** is then read by OCR from that region instead of announced from the static `label`, which becomes the fallback for when OCR reads nothing.

Use it where the plugin itself changes what a control means. A sample library can put one mixer strip in a fixed place whose five channels are microphone positions for one patch and orchestral sections for another — same buttons, same pixels, different names. A fixed label is then confidently wrong, which is worse than being slow: it tells a user who cannot see the screen that they toggled something they did not.

Costs one OCR read per focus, so put it on controls whose name genuinely varies, not on every control.

```luau
ov:addHotspotToggle({
    label = "Spot 1 Mic",              -- fallback if OCR reads nothing
    at = { -130, 350 },                -- the power icon
    ocrLabel = { -154, 358, -106, 376 }, -- the caption printed under it
    onColor = { 180, 165, 230 },
    offColor = { 77, 79, 85 },
})
```

## O:addCustomButton(opts)

Appends a button that runs a Luau callback on activation. `opts: { label: string, onActivate: (overlay) -> (), text: ((overlay) -> string?)?, typeLabel: string?, editable: boolean?, opensMenu: boolean?, hotkey: string?, when: ((overlay) -> boolean)? }` — `onActivate` receives the overlay itself (so a header control can reach the active context).

`onActivate` is called **guarded**. A control whose action throws says so rather than going silent: the host used to catch the error at the key dispatch and write a line, while the person who pressed it heard nothing at all — which is indistinguishable from a control that quietly did its job.

`text` is announced between the label and the type word, and is how a control that ACTS can also say what it would act on.

`typeLabel` overrides the word for what this control *is*. The kind a control is built from and the thing it stands for are not always the same: a custom button that puts the caret into a text field is an edit box to everyone using it, and announcing "button" describes the implementation to somebody with no interest in it — and says the wrong thing about what typing will do next.

`editable` says that activating this control leaves the caret in a text field, so **Space belongs to the application** from then on. Return still activates, because it is the only way back into the field after tabbing away. This is the same rule an `addOCREdit` gets by virtue of its kind; the flag is for controls that reach their field some other way (a coordinate, an accessibility element) and would otherwise swallow the space in "White 80s".

Returns `{ kind = "custom", label, onActivate, text, typeLabel, editable, opensMenu, hotkey, when }`.

```lua
ov:addCustomButton({
  label = "Next library",
  onActivate = function(self) --[[ ... ]] end,
})
```

## O:addStepper(opts)

Appends a **value changed with Left and Right**, where the module knows how to change it. `opts: { label: string, text: (overlay) -> string?, onStep: (dir: number, overlay) -> (), onActivate: ((overlay) -> ())?, settle: number?, typeLabel: string?, when: ((overlay) -> boolean)? }`.

Announced as a slider, because that is what it is to the person using it; how it is driven underneath is the module's problem rather than theirs.

Use it where `addSlider` cannot serve. That one finds its thumb by matching an image, which needs a template captured for one plug-in at one size — no use for a rotary drawn as an arc, a bar with two handles, or anything in a window the user can resize. What such a control does have is a value printed beside it and something that moves it, and that is all a stepper is.

- `onStep(dir, overlay)` — `dir` is `-1` for Left and `+1` for Right. Called guarded.
- `onActivate` — optional, and what a **press** means. Without it, Space and Return are still captured on a stepper (it is not an inert control) and then do nothing at all, which is a promise without an action. ON:EAR's two use it for "double-click to put this back to its default", which is one keystroke instead of twenty.
- `settle` — how long to wait **at most** for the value to change before announcing it anyway. Not how long to wait: see [`O:watch`](#watch).

What is announced afterwards always comes from reading `text` again, never from what the step intended. A control that reports its own intention rather than the application's state is the failure this project keeps returning to.

```lua
ov:addStepper({
  label = "Tone",
  when = function() return element("Tone") ~= nil end,
  text = function() return valueBelow("Tone") or "not shown" end,
  onStep = function(dir)
    local x, y = screenPoint(290, 780)
    if x then host.input.scroll(x, y, dir * 0.5) end
  end,
})
```

## O:watch(spec) {#watch}

Waits for something to **change**, rather than for a length of time. `spec: { read: (overlay) -> any, was: any?, done: ((now: any, was: any) -> boolean)?, every: number?, within: number?, onDone: ((now, was) -> ())?, onGiveUp: ((now, was) -> ())? }`.

Almost every `host.timer.after` in a module is a guess about how long an application takes, and a guess is wrong in both directions. Too short and the value read back is the one from *before* the action — a real value announced with confidence for the wrong moment, which is how ON:EAR's speaker grid came to name the previously loaded speaker each time a tile was pressed. Too long and everything feels slow.

- `read` — what to look at. **`nil` means "cannot read right now", which is not the same as "unchanged" and never counts as done.** Treating those as the same thing is the specific bug this replaces.
- `was` — the reading from before the action. Omit it and `read` is called once to get it, which is only correct if nothing has happened yet.
- `done(now, was)` — defaults to "it changed".
- `every` — how often to look, default 100 ms. Each look costs whatever `read` costs, and a screen pixel is a compositor frame, so this is not free.
- `within` — give up after this long and call `onGiveUp` with the last reading. Not an error: a radio button that was already chosen is *meant* not to change.

Bound to the overlay. It stops the moment the overlay is no longer active or the window it was watching is no longer in front — a callback arriving after the world has moved is the mistake that pressing the key again cannot undo.

```lua
self:watch({
  read = function() return valueBelow("Width") end,
  was = before,
  within = 600,
  onDone = say,
  onGiveUp = say,   -- it did not move, and that is worth saying too
})
```

## O:addOCRButton(opts)

Appends a button whose label/value is read live by OCR over a region; activating re-reads it then clicks the region centre. `opts: { label: string, region: {number, number, number, number}, hotkey: string? }` — `region` is `{x1, y1, x2, y2}` origin-relative.

Returns `{ kind = "ocr", label, region, hotkey }`. When focused/spoken it appends the OCR text (or `"no text"`) as the value.

```lua
ov:addOCRButton({ label = "Patch", region = { 200, 12, 360, 32 } })
```

## O:addGraphicalToggle(opts)

Appends a toggle whose on/off state is read by image-matching its region against an "on" and "off" template; activating clicks the region centre and re-reads the new state after ~150 ms. `opts: { label: string, region: {number, number, number, number}, onImage: string?, offImage: string?, hotkey: string? }` — `region` is `{x1, y1, x2, y2}` origin-relative; `onImage`/`offImage` are template image paths.

Returns `{ kind = "gtoggle", label, region, onImage, offImage, hotkey }`. Spoken state is `"on"` / `"off"` (omitted when neither template matches).

```lua
ov:addGraphicalToggle({
  label = "Mix",
  region = { 50, 50, 80, 70 },
  onImage = "mix_on.png",
  offImage = "mix_off.png",
})
```

## O:addHotspotToggle(opts) {#addhotspottoggle}

Appends a toggle whose on/off state is read from a **single pixel** at its click point (ReaHotkey's `HotspotToggleButton`) — cheap (one screen touch), where a region scan would cost ~16 ms *per pixel*. The pixel is compared to an on/off reference colour and the **nearest** wins. Activating clicks the point (toggling it) and then [waits for the state to actually change](#watch) before announcing it, giving up after ~900 ms. A control that redraws in thirty milliseconds is announced in thirty; one that takes half a second is announced correctly instead of early; one that does not change at all — the already-chosen member of a radio group — is announced at the deadline, which is the truth about it. `opts: { label: string, at: {number, number}, onColor: {number, number, number}, offColor: {number, number, number}, hotkey: string?, rawOrigin: boolean? }` — `at` is `{x, y}` origin-relative; `onColor`/`offColor` are `{r, g, b}`.

`at` may also be a **function** of the overlay, for a control whose position depends on what is on screen right now; returning `nil` yields no point and the click is skipped rather than guessed. `onColor`/`offColor` may each be a **list** of `{r, g, b}`, because one state can legitimately look several ways. A reading that resembles neither closely enough — measured against how far apart the references are — reports **no state at all** and logs why, rather than announcing whichever is nearer. Announcing the opposite of the truth is the worst thing this system can do to somebody who cannot check it against the screen; saying nothing is merely unhelpful.

`text` is honoured here as everywhere, and is how a toggle can report something the state alone does not say — ON:EAR's panel switches use it for "unavailable", because announcing "off" for a switch that will not respond is the same failure as announcing the wrong state.

Like `addHotspotButton`, the click is refused if another window is drawn over the point (see [`host.window.ownsPoint`](window#hostwindowownspointid-x-y)).

Returns `{ kind = "hotspottoggle", label, at, onColor, offColor, text, hotkey, rawOrigin }`. Spoken state is `"on"` / `"off"` (omitted when the pixel can't be read). Prefer this over `addGraphicalToggle` when the control has a distinct lit/unlit colour (an indicator LED, a lit ⏻ icon) — it needs no template images and is a fraction of the cost.

```lua
ov:addHotspotToggle({
  label = "Spot 1 Mic",
  at = { -138, 366 },
  onColor = { 180, 165, 230 }, -- lit (accent)
  offColor = { 78, 79, 85 },   -- unlit (dim grey)
})
```

## O:focusNext()

Moves focus to the next control (wrapping) and speaks it, moving the mouse onto OCR controls if `hoverToRead` is set. No-op when there are no controls. Returns nothing.

## O:focusPrev()

Moves focus to the previous control (wrapping) and speaks it. No-op when empty. Returns nothing.

## O:activate(index)

Activates the control at `index` (defaults to the focused control). `index: number?`. Behaviour by kind: `hotspot` clicks `at` and speaks `"label, activated"`; `custom` calls `onActivate(self)`; `ocr` re-reads then clicks the region centre; `gtoggle` clicks the region centre and re-reads state after ~150 ms. No-op for `static` or a missing control. Returns nothing.

```lua
ov:activate()      -- activate focused control
ov:activate(3)     -- activate the 3rd control
```

## O:attach(matcher, opts)

Binds the overlay as a **standalone** context: active while a window matching `matcher` is the foreground/active window, with coordinates relative to that window's client area. `matcher` is a window matcher passed to `host.window.test`; `opts: { hoverToRead: boolean? }?`.

While active, the overlay captures and suppresses the navigation keys, scoped to its own window (so `Alt+Tab` and menus pass through natively): `Tab` / `Shift+Tab` move between controls, `Return` and `Space` activate the focused control, and — when the overlay has a tab control — `Left`/`Right`, `Ctrl+Tab`/`Ctrl+Shift+Tab` and `Ctrl+<n>` drive it. `Space` is released while an editable field (an `ocredit` control) is focused, so a literal space can be typed into it. On activation the overlay starts at its first control (and any tab control at its first tab) when a **genuinely new** window opened, but resumes the last-focused control when the *same* still-open window merely regained the foreground (`Alt+Tab` out and back); the two are told apart by the window's identity (its HWND). `hoverToRead` (default `false`) moves the mouse onto an OCR control on focus (some UIs only reveal values on hover). Registers the foreground/focus trigger once. Returns nothing.

```lua
ov:attach({ title = "MySynth" })
```

## O:attachEmbedded(spec, opts)

Binds the overlay as an **embedded** context: active while keyboard focus is inside a plugin control hosted in a DAW. Coordinates are relative to that control's client area, so the same regions work standalone and embedded.

`spec: { hosts: {Matcher}?, host: Matcher?, control: string, identify: ((control) -> boolean)? }` — `hosts` is the list of acceptable DAW host-window matchers (falls back to `{ spec.host }`); `control` is a Luau pattern matched against candidate child/focus-chain control class names; `identify(control)` is an optional confirmation callback (UIA / OCR / image search), cached per control HWND, used because a host's plugin control class often matches any plugin (e.g. REAPER's `Plugin<ptr>`). Candidates come from `host.window.controls()` plus the `host.window.focusChain()`.

`opts: { hoverToRead?, slot: string?, specificity: number?, pollMatch: number? }` — `hoverToRead` and the navigation / focus-reset behaviour are as in `attach`. With `slot` the overlay joins the host **arbiter** for that slot at `specificity` (a base and the overlays inheriting it pass the same slot; the most-specific *matching* one is active — see `host.arbiter`); `pollMatch` (ms) additionally re-checks the match on a recurring timer, for matches that change with no window event (a library landmark appearing inside an already-focused plugin). Returns nothing.

```lua
ov:attachEmbedded({
  hosts = { { title = "REAPER" }, { title = "Cubase" } },
  control = "Plugin",
  identify = function(c) return host.uia.find(c.id, "MySynthGUI", 0) end,
})
```

## O:gate(fn) / O:landmark(image)

`gate(fn)` sets an extra activation condition ANDed onto the context match:
`fn(origin)` (origin = the active context's coordinate window/control) returns
whether the overlay should be active. `landmark(image)` is the common case — a gate
satisfied only while `image` (an **absolute** path, via `host.path`) is found within
the active context's region. A derived / library overlay uses a landmark to take
over from its base only when its product wordmark is on screen. Both return the
overlay (chainable).

**A gate whose condition can change without a window event needs `pollMatch`.** Gates are otherwise re-evaluated only when a window is activated or focused, which is enough for a dialog — opening and closing one *is* a window event — and not enough for anything that appears and disappears *inside* a window that never changes. ON:EAR's chooser panels are exactly that, and without the poll the overlay kept the arbiter slot after its panel had closed: a ring for a panel that was no longer on screen, which somebody who cannot see it has no way to escape. See `pollMatch` under `O:attach` below.

## O:typingWhen(fn)

`fn() -> boolean`. While it returns true, the overlay **holds no keys at all** — not the navigation keys, not Space, not Return.

Letting named keys through one at a time patches a hole whose shape is not known: any key nobody thought of stays swallowed, and a swallowed key in a text box is indistinguishable, from the keyboard, from the application having frozen. The condition is supplied by the module because only the module can tell — ON:EAR's answer is that its accessibility tree goes dark exactly while the caret is in its search box, which its gate is measuring anyway.

The way back is the application's own: Tab moves focus out of its editor, the condition goes false on the next check, and the keys come back. Nothing here can lock, because while it is on there is nothing left to lock with; the worst it can do is go inert, which announces itself the moment Tab does not move the ring.

**Coordinate anchoring (opt-in):** pass `landmark(image, { anchor = true })` and, while
the landmark is on screen, the overlay's control coordinates resolve **relative to the
landmark's top-left** instead of the plugin's client area (a `rawOrigin` control still
uses the client). So an overlay whose landmark sits *in the same UI panel as its
controls* (a library's wordmark above its mixer) stays correctly positioned regardless
of host, window size, or plugin chrome/browser state — the landmark and the controls
move together. Author such controls relative to the wordmark (coordinates may be
negative if a control sits above/left of it). WITHOUT `anchor`, `landmark` is a pure
gate and controls stay client-relative — the right choice when the image is used only
to tell the overlay apart from a sibling on a shared slot (a dialog's header image).

```lua
ov:landmark(host.path("images/MyLib/wordmark.png"))
```

## Plugin base + library overlays (the cell model)

A plugin is not one overlay. It is one overlay per **cell** — per combination of things
that are fixed for as long as the window exists: which version it is, and where it runs
(embedded in a host, wrapped in another plugin, standalone). Each cell gets its own
binding, its own coordinates, and its own control set, and the arbiter decides between
them once. Use a `when` predicate only for what changes *while* an overlay is active
(which tab is front, whether a browser is open, whether a library is loaded) — the
arbiter cannot re-elect on those without tearing the overlay down, which a
screen-reader user hears.

Kontakt is the worked example: `{Kontakt 7, Kontakt 8} × {in a DAW, in Komplete Kontrol,
standalone}` = six overlays, declared by looping over a table of cells
(`modules/kontakt/src/cells.luau`). They share one arbiter slot per environment, so
Komplete Kontrol's own chrome (`O.layer.chrome`), a Kontakt header (`base`), a loaded
library (`content`) and a modal dialog (`dialog`) compose without knowing about each
other.

A sample-library module depends on `com.platform.kontakt` and calls
`kontakt.library(name, landmark, build)`, which builds one inheriting overlay per cell —
each carrying that cell's header plus the library's own controls, gated on the landmark
and anchored to it. `build(ov)` runs once per cell, so it must only add controls:

```lua
local kontakt = host.require("com.platform.kontakt")
kontakt.library("Cinematic Studio Strings", host.path("images/CSS/Product.png"), function(ov)
  ov:addStaticText("Cinematic Studio Strings")
  for i, fallback in ipairs({ "Spot 1 Mic", "Spot 2 Mic", "Main Mic", "Room Mic", "Mix" }) do
    local x = -130 + (i - 1) * 40
    ov:addHotspotToggle({
      label = fallback,
      at = { x, 350 },
      ocrLabel = { x - 24, 358, x + 24, 376 },
      onColor = { 180, 165, 230 },
      offColor = { 77, 79, 85 },
    })
  end
end)
```

Splitting a module across files is what keeps this readable: see
[`host.include`](resource-settings-modules#hostincluderel). Kontakt separates detection,
the cell matrix, the per-version geometry, what a control does, and what a cell contains.

See [Nested overlays design](../nested-overlays-design.md).
