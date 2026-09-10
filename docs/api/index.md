---
title: All functions
sidebar_position: 0
---

# All functions

Every call the platform offers a module, in one place. 117 entries.

A module reaches the host through the global `host` table, which is always there. The overlay is a module like any other and is imported: `local O = host.require("com.platform.overlay")`.

Every callback registered through any of these runs in the calling module's own Luau VM, and only fires while that module is **enabled**.

## What to declare in module.toml {#capabilities}

Each page says which name to put in its manifest:

```toml
[capabilities]
require = ["window", "screen", "speech"]
```

**It is enforced.** A namespace you have not declared is not on your `host` table, and reaching for it raises an error naming your module and the capability it needs rather than evaluating to `nil` three frames from anything that could explain it. Declaring more than you use is legal and harmless; declaring less is a failure at the moment you first need it.

**Permission follows the module that wrote the code, not the one running it.** A `code_module` dependency is evaluated inside its dependent's VM, and what it may reach is decided by its own manifest: the overlay runtime declares `ocr`, so its code may read text on behalf of a module that never declared `ocr` itself. The reverse also holds — a dependency cannot reach something merely because one of its dependents declared it.

Ownership is unaffected, and deliberately so. A hotkey or a timer that a dependency's code registers still belongs to the module whose VM it ran in, so disabling that module takes it away.

**A callback you register is your code.** That is why a module whose own file never writes `host.window` still declares `window` if it attaches an overlay: the gate and the matcher it supplied are called during a focus change, and they are yours.

Nothing is gated on `host.os`, `host.require`, `host.tryRequire`, `host.include`, `host.epoch`, `host.now`, `host.inputEpoch` or `host.calibrating`. A clock, a counter, a platform name and a way to reach a declared dependency are not worth asking permission for, and gating them would mean every manifest names them — which is the same as naming none.

The list is also shown to the user before installing a module from GitHub. It is not a security boundary on its own — a module still runs arbitrary Luau, and the declaration is the module's own word — but it is now the word the platform holds it to.

The overlay is the exception in shape rather than degree: it is a **module**, so it goes under `dependencies` rather than here.


## Overlay

Controls a module defines over a plug-in window, walked with Tab and spoken aloud. A module, not a host namespace: `host.require("com.platform.overlay")`.

| | |
|---|---|
| [`Bindings — O.window / O.embedded / :with / O.hosts / O:bind`](overlay#bindings) | `O.window(matcher, opts?) -> Binding` · `O.embedded(spec, opts?) -> Binding` · `Binding:with(over) -> Binding` · `O.hosts(...)… |
| [`O.doubleClick(x, y)`](overlay#o-doubleclick) | Two clicks at the same point, far enough apart in time to **be** a double-click. |
| [`O.layer`](overlay#o-layer) | The specificity ladder within a slot, named |
| [`O.memoByOrigin(fn, opts?)`](overlay#o-memobyorigin) | Memoizes a per-window property that does not change while that window exists |
| [`O.new(label)`](overlay#o-new) | Creates a new overlay object. |
| [`O.state`](overlay#o-state) | A free-form table on every overlay for the owning module's own state, so it does not have to squat in the runtime's reserved `_`-prefixed fields. |
| [`O:activate(index)`](overlay#o-activate) | Activates the control at `index` (defaults to the focused control). |
| [`O:addCustomButton(opts)`](overlay#o-addcustombutton) | Appends a button that runs a Luau callback on activation. |
| [`O:addGraphicalToggle(opts)`](overlay#o-addgraphicaltoggle) | Appends a toggle whose on/off state is read by image-matching its region against an "on" and "off" template |
| [`O:addHotspotButton(opts)`](overlay#o-addhotspotbutton) | Appends a button that, when activated, clicks a fixed origin-relative point. |
| [`O:addHotspotToggle(opts)`](overlay#o-addhotspottoggle) | Appends a toggle whose on/off state is read from a **single pixel** at its click point (ReaHotkey's `HotspotToggleButton`) |
| [`O:addOCRButton(opts)`](overlay#o-addocrbutton) | Appends a button whose label/value is read live by OCR over a region; activating re-reads it then clicks the region centre. |
| [`O:addStaticText(label)`](overlay#o-addstatictext) | Appends a static text control: Tab-reachable and read aloud on focus, but with no activation (Enter does nothing). |
| [`O:addStepper(opts)`](overlay#o-addstepper) | Appends a **value changed with Left and Right**, where the module knows how to change it. |
| [`O:afterIdle(key, ms, fn)`](overlay#o-afteridle) | Runs `fn` **once**, `ms` after the last call carrying the same `key`. |
| [`O:attach(matcher, opts)`](overlay#o-attach) | Binds the overlay as a **standalone** context |
| [`O:attachEmbedded(spec, opts)`](overlay#o-attachembedded) | Binds the overlay as an **embedded** context: active while keyboard focus is inside a plugin control hosted in a DAW. |
| [`O:focusNext()`](overlay#o-focusnext) | Moves focus to the next control (wrapping) and speaks it, moving the mouse onto OCR controls if `hoverToRead` is set. |
| [`O:focusPrev()`](overlay#o-focusprev) | Moves focus to the previous control (wrapping) and speaks it. |
| [`O:frame(fn)`](overlay#o-frame) | Shifts the overlay's whole coordinate frame |
| [`O:gate(fn) / O:landmark(image)`](overlay#o-gate) | `gate(fn)` sets an extra activation condition ANDed onto the context match |
| [`O:group(pred, build)`](overlay#o-group) | Adds everything `build` adds under a shared condition: `pred` is ANDed onto each control's own `when`, and groups nest. |
| [`O:origin() / O:hwnd()`](overlay#o-origin) | The active context's coordinate window — the plugin control when embedded, the window when standalone — and its handle. |
| [`O:typingWhen(fn)`](overlay#o-typingwhen) | `fn() -> boolean`. |
| [`O:watch(spec)`](overlay#o-watch) | Waits for something to **change**, rather than for a length of time. |

## host.window

Finding windows and the surfaces inside them, and reacting when the focus moves.

| | |
|---|---|
| [`host.window.active()`](window#host-window-active) | Returns the window table for the foreground window, or `nil` if there is none. |
| [`host.window.apps()`](window#host-window-apps) | The running applications, described the way a window table's `app` field describes them |
| [`host.window.controls(win?)`](window#host-window-controls) | Returns the child control tables of `win` (its `id` is used), or of the active window when omitted. |
| [`host.window.find(matcher)`](window#host-window-find) | Returns the first window that satisfies `matcher`, or `nil`. |
| [`host.window.findAll(matcher)`](window#host-window-findall) | Returns all windows that satisfy `matcher`, listed the same narrowed way as `find`. |
| [`host.window.focus(id)`](window#host-window-focus) | Brings the window with that handle to the front and gives it the keyboard. |
| [`host.window.focusChain()`](window#host-window-focuschain) | Returns control tables from the currently focused element up to its top-level window. |
| [`host.window.list(filter?)`](window#host-window-list) | Returns an array of window tables for all enumerable top-level windows |
| [`host.window.onFocus(cb)`](window#host-window-onfocus) | Registers `cb` to fire whenever the keyboard focus moves — including within the same top-level window. |
| [`host.window.onTrigger(matcher, opts, cb)`](window#host-window-ontrigger) | Registers `cb` to fire on every foreground change for which the new active window satisfies `matcher`. |
| [`host.window.ownsPoint(id, x, y)`](window#host-window-ownspoint) | Whether the window `id` belongs to is the one drawn at that screen point. |
| [`host.window.recheck()`](window#host-window-recheck) | Asks the host to run a focus-change round at the end of the current tick |
| [`host.window.test(matcher, win)`](window#host-window-test) | Returns whether the given window table satisfies `matcher` (the same logic `find`/`findAll` apply). |

## host.screen

Reading pixels, profiling a region, and finding an image within one.

| | |
|---|---|
| [`host.screen.imageSearch(template, opts?)`](screen#host-screen-imagesearch) | Searches the screen for the first occurrence of the image at path `template` and returns its match rectangle in **screen pixels** |
| [`host.screen.imageSearchAll(template, opts?)`](screen#host-screen-imagesearchall) | Returns **every** match of `template` in the region, as an array of rectangles in screen pixels, rather than stopping at the first one like `imageSearch`. |
| [`host.screen.imageSearchAsync(template, opts?, cb)`](screen#host-screen-imagesearchasync) | Like `imageSearch`, but the region capture **and** the match both run on a worker thread; `cb` fires on a later tick. |
| [`host.screen.imageSearchMulti(templates, opts?)`](screen#host-screen-imagesearchmulti) | Captures the region **once** and tries each template path against that one frame, returning two values |
| [`host.screen.pixel(x, y)`](screen#host-screen-pixel) | Reads the colour of the screen pixel at `(x, y)`. |
| [`host.screen.profile(opts?)`](screen#host-screen-profile) | where `Axis = { min: number[], max: number[], mean: number[], r: number[], g: number[], b: number[] }` |
| [`host.screen.save(path, opts?)`](screen#host-screen-save) | Captures `opts.region` (see Region form |
| [`host.screen.saveMarked(path, opts)`](screen#host-screen-savemarked) | Everything `save` does, plus a magenta crosshair drawn at every point in `opts.marks`, given in **screen** coordinates. |
| [`host.screen.size()`](screen#host-screen-size) | Returns the primary screen dimensions in pixels as `{ w, h }`. |

## host.ocr

Recognising text in a screen region.

| | |
|---|---|
| [`host.ocr.recognize(opts?)`](ocr#host-ocr-recognize) | Recognizes text inside a screen region and returns the full text plus per-word bounding boxes. |
| [`host.ocr.recognizeMany(opts)`](ocr#host-ocr-recognizemany) | Recognizes several regions from **one** screen capture, so that values which have to agree with each other come from the same instant. |

## host.element

Querying the accessibility tree an application publishes.

| | |
|---|---|
| [`host.element.classNavPoint(hwnd, className, controlType, child, sibling)`](element#host-element-classnavpoint) | Finds the first element whose class contains `className` and whose control type is `controlType`, walks a fixed path from it,… |
| [`host.element.dump(hwnd)`](element#host-element-dump) | The condition-based counterpart to `host.element.rawDump`, and the first thing to run against a window nobody here has seen |
| [`host.element.find(hwnd, name, controlType)`](element#host-element-find) | Returns `true` if the UI Automation subtree of the window `hwnd` contains at least one element whose Name equals `name`… |
| [`host.element.findAny(hwnd, names, types)`](element#host-element-findany) | Answers "is any of these names present as any of these control types?" |
| [`host.element.focusStep(hwnd, direction)`](element#host-element-focusstep) | **This one writes.** It enumerates the visible, keyboard-focusable descendants of `hwnd`'s content area and *moves keyboard… |
| [`host.element.locate(hwnd, name, controlType)`](element#host-element-locate) | Finds the first UIA element in window `hwnd` matching `name` + `controlType` and returns the screen-pixel centre of its… |
| [`host.element.locateVia(hwnd, viaName, viaType, name, controlType)`](element#host-element-locatevia) | Like `locate`, but in two levels |
| [`host.element.pluginLocate(hwnd, containerName, name, controlType)`](element#host-element-pluginlocate) | Like `locate`, but for a plugin hosted inside another application. |
| [`host.element.rawDump(hwnd)`](element#host-element-rawdump) | Diagnostic counterpart to `host.element.dump`, walking the **raw** tree instead of a condition-based search, so it crosses… |
| [`host.element.stateProbe(hwnd, containerName, name, controlType)`](element#host-element-stateprobe) | Asks a named element what it reports about its **own** state, rather than inferring one from its control type. |
| [`host.element.type`](element#host-element-type) | The UIA ControlType ids by name |

## host.input

Synthesising mouse and keyboard input.

| | |
|---|---|
| [`host.input.click(x, y, opts?)`](input#host-input-click) | Moves to `(x, y)` and synthesizes a mouse click there. |
| [`host.input.cursorPos()`](input#host-input-cursorpos) | Returns the current mouse cursor position in screen coordinates. |
| [`host.input.drag(x1, y1, x2, y2, opts?)`](input#host-input-drag) | Presses the mouse button at `(x1, y1)`, drags to `(x2, y2)`, and releases — with real, paced movement in between. |
| [`host.input.mouseDown(x, y, opts?)`](input#host-input-mousedown) | Presses a mouse button at `(x, y)` and **leaves it down**. |
| [`host.input.mouseUp(x, y, opts?)`](input#host-input-mouseup) | Releases a mouse button at `(x, y)`, ending a press begun with `mouseDown`. |
| [`host.input.move(x, y)`](input#host-input-move) | Moves the mouse cursor to the given screen coordinates. |
| [`host.input.post(id, key)`](input#host-input-post) | Delivers a single key to **one window** rather than to whatever has focus. |
| [`host.input.scroll(x, y, amount)`](input#host-input-scroll) | Moves to `(x, y)` and scrolls the mouse wheel by `notches`, which may be fractional. |
| [`host.input.send(combo)`](input#host-input-send) | Sends a keyboard shortcut by pressing the modifiers, tapping the key, and releasing in reverse order. |
| [`host.input.text(text)`](input#host-input-text) | Types a Unicode string as synthetic keystrokes. |

## host.keys

Claiming keys before the focused application sees them.

| | |
|---|---|
| [`host.keys.capture(spec, callback)`](keys#host-keys-capture) | Begins intercepting the key spec |
| [`host.keys.menuOpen(open)`](keys#host-keys-menuopen) | Tells the hook a plugin's own (Qt/UIA) menu is open (`true`) or closed (`false`). |
| [`host.keys.modifiersDown()`](keys#host-keys-modifiersdown) | True while any of Ctrl, Alt, Shift or Win — Control, Option, Shift or Command on macOS — is physically held. |
| [`host.keys.nativeMenuOpen()`](keys#host-keys-nativemenuopen) | True while the application in front has a menu open that the operating system itself drew. |
| [`host.keys.release(token)`](keys#host-keys-release) | Undoes the exact capture identified by the `token` `host.keys.capture` returned, recomputing the global captured set so the… |
| [`host.keys.releaseAll()`](keys#host-keys-releaseall) | Removes **all** key captures owned by this module and refreshes the suppression set. |
| [`host.keys.scope(toForeground)`](keys#host-keys-scope) | Scopes captured-key suppression. |

## host.hotkey

Claiming a combination system-wide.

| | |
|---|---|
| [`host.hotkey.register(spec, callback)`](hotkey#host-hotkey-register) | Registers a **global** OS hotkey (active regardless of foreground window) for the key spec and returns an integer `id`. |
| [`host.hotkey.unregister(id)`](hotkey#host-hotkey-unregister) | Releases the OS hotkey and forgets the callback for the `id` returned by `register`. |

## host.speech

| | |
|---|---|
| [`host.speech.engine()`](speech#host-speech-engine) | The id this module chose, or `nil` when it is on the ordinary path. |
| [`host.speech.engines()`](speech#host-speech-engines) | Everything that could speak on this machine, and whether it can right now. |
| [`host.speech.output(text, opts?)`](speech#host-speech-output) | Speaks `text`; `opts.interrupt` defaults to `true` (omitting `opts` also means interrupt). |
| [`host.speech.use(id)`](speech#host-speech-use) | Chooses what speaks for **this module**. |

## host.sound

| | |
|---|---|
| [`host.sound.play(path)`](sound#host-sound-play) | Plays an audio file from the module's package directory, fire-and-forget. |

## host.timer

Waiting without blocking, and knowing when a cached reading went stale.

| | |
|---|---|
| [`host.timer.after(ms, callback)`](timer#host-timer-after) | Schedules a **one-shot** callback to fire approximately `ms` milliseconds later, driven from the event-loop tick. |
| [`host.timer.every(ms, callback)`](timer#host-timer-every) | Schedules a **recurring** callback to fire approximately every `ms` milliseconds, driven from the event-loop tick. |

## host.settings

Typed, per-module settings, edited by the user in the module manager.

| | |
|---|---|
| [`host.settings.define(key, default, opts?)`](settings#host-settings-define) | Registers a setting `key` for this module, pins its kind from `default`, and |
| [`host.settings.get(key)`](settings#host-settings-get) | Returns the current stored value of a previously-defined setting |
| [`host.settings.onChange(key, callback)`](settings#host-settings-onchange) | Registers `callback` to run whenever this setting changes (via `set` or the |
| [`host.settings.set(key, value)`](settings#host-settings-set) | Validates `value` against the setting's schema and writes it to the store |

## host.config

| | |
|---|---|
| [`host.config.get / host.config.set / host.config.define / host.config.onChange`](settings#host-config-get) | `host.config` is the **same table** as `host.settings` (a catalog-compatibility |

## host.os

| | |
|---|---|
| [`host.os.current`](os#host-os-current) | Read-only string: the current OS, from Rust `std::env::consts::OS` (`"windows"`, `"macos"`, `"linux"`, …). |
| [`host.os.is(name)`](os#host-os-is) | Returns `true` when `name` equals the current OS string. |
| [`host.os.pick(t)`](os#host-os-pick) | The per-platform value, or `nil` when this platform has no entry. |

## host.log

Writing to the log file beside the application.

| | |
|---|---|
| [`host.log.info(msg)`](log#host-log-info) | Writes `msg` to the host log under the `module` channel. |

## host.path

| | |
|---|---|
| [`host.path(rel)`](path#host-path) | Resolves a package-relative path to an **absolute** filesystem path string |

## host.resource

| | |
|---|---|
| [`host.resource.exists(rel)`](resource#host-resource-exists) | Whether a file exists under this module's own root, without reading it. |
| [`host.resource.read(rel)`](resource#host-resource-read) | Reads a package-relative file as a UTF-8 string (`string`) from the calling |

## host.require

| | |
|---|---|
| [`host.require(id)`](require#host-require) | Returns the object that dependency `id` exported, where `id` is a module declared |

## host.tryRequire

| | |
|---|---|
| [`host.tryRequire(id)`](tryrequire#host-tryrequire) | Like `host.require`, but returns **nil** instead of raising when `id` |

## host.include

| | |
|---|---|
| [`host.include(rel)`](include#host-include) | Loads **another file of this module** and returns whatever that file returns |

## host.epoch

| | |
|---|---|
| [`host.epoch()`](timer#host-epoch) | A counter that changes whenever the world may have |

## host.now

| | |
|---|---|
| [`host.now()`](timer#host-now) | Milliseconds since the application started, monotonic, so it cannot go backwards in the middle of a measurement. |

## host.inputEpoch

| | |
|---|---|
| [`host.inputEpoch()`](timer#host-inputepoch) | A counter that turns over only when something **acted** on the screen: input this platform drove, or a window coming forward. |

## host.arbiter

Deciding which of several overlays owns a contested slot.

| | |
|---|---|
| [`host.arbiter.participants(slot)`](arbiter#host-arbiter-participants) | Every claim on `slot`, for diagnosis. |
| [`host.arbiter.register(slot, specificity, onActivate, onDeactivate)`](arbiter#host-arbiter-register) | Enters a claim for `slot` and returns the handle every other call here takes. |
| [`host.arbiter.setMatching(slot, handle, matching)`](arbiter#host-arbiter-setmatching) | Reports whether this claim's own conditions hold right now, and re-elects the slot. |
| [`host.arbiter.unregister(slot, handle)`](arbiter#host-arbiter-unregister) | Withdraws the claim and promotes whoever is next. |
| [`host.arbiter.winner(slot)`](arbiter#host-arbiter-winner) | The module id currently holding `slot`, or `nil` when nothing matches. |
| [`host.arbiter.winnerSpecificity(slot)`](arbiter#host-arbiter-winnerspecificity) | The rank of whoever currently holds `slot`, or `nil` when nothing matches. |

## host.calibrating

| | |
|---|---|
| [`host.calibrating`](calibrating#host-calibrating) | A value rather than a function — read it, do not call it. |

## Concepts

The shapes and grammars the calls above are written in.

| | |
|---|---|
| [`Key spec string format`](keys#key-spec-string-format) | Two namespaces parse `+`-joined spec strings; segments are trimmed and case-insensitive. |
| [`Matchers`](window#matchers) | A *matcher* is a declarative table passed to `host.window.find/findAll/test/onTrigger`. |
| [`Plugin base + library overlays (the cell model)`](overlay#plugin-base-library-overlays) | A plugin is not one overlay. |
| [`Region form`](screen#region-form) | Several functions (`host.screen.imageSearch`, `host.screen.profile`, `host.ocr.recognize`, and each entry of… |
| [`Table shapes`](window#table-shapes) | Returned by `host.window.list()`, `host.window.active()`, `host.window.find()`, `host.window.findAll()`, and passed to trigger/test callbacks. |
| [`ocrLabel — reading a control's name off the screen`](overlay#ocrlabel) | Any hotspot or hotspot-toggle may carry `ocrLabel = {x1, y1, x2, y2}` (origin-relative) |
