---
title: All functions
sidebar_position: 0
---

# All functions

Every call the platform offers a module, in one place. 87 entries.

A module reaches the host through the global `host` table, which is always there. The overlay is a module like any other and is imported: `local O = host.require("com.platform.overlay")`.

## Overlay

The self-voicing control tree: what a module builds, and how it is bound to a window. `local O = host.require("com.platform.overlay")`.

| | |
|---|---|
| [`Bindings — O.window / O.embedded / :with / O.hosts / O:bind`](overlay#bindings) | `O.window(matcher, opts?) -> Binding` · `O.embedded(spec, opts?) -> Binding` · `Binding:with(over) -> Binding` · `O.hosts(...)… |
| [`O.doubleClick(x, y)`](overlay#o-doubleclick) | Two clicks at the same point, far enough apart in time to **be** a double-click. |
| [`O.layer`](overlay#o-layer) | The specificity ladder within a slot, named |
| [`O.memoByOrigin(fn, opts?)`](overlay#o-memobyorigin) | Memoizes a per-window property that does not change while that window exists |
| [`O.new(label)`](overlay#o-new) | Creates a new overlay object. |
| [`O.state`](overlay#o-state) | A free-form table on every overlay for the owning module's own state, so it does not have to squat in the runtime's reserved `_`-prefixed fields. |

## host.window

Finding windows and their controls, and reacting when the focus moves.

| | |
|---|---|
| [`host.window.active()`](window#host-window-active) | Returns the window table for the foreground window, or `nil` if there is none. |
| [`host.window.controls(win?)`](window#host-window-controls) | Returns the child control tables of `win` (its `id` is used), or of the active window when omitted. |
| [`host.window.find(matcher)`](window#host-window-find) | Returns the first window from `host.window.list()` that satisfies `matcher`, or `nil`. |
| [`host.window.findAll(matcher)`](window#host-window-findall) | Returns all windows from `host.window.list()` that satisfy `matcher`. |
| [`host.window.focus(id)`](window#host-window-focus) | Brings the window with that handle to the front and gives it the keyboard. |
| [`host.window.focusChain()`](window#host-window-focuschain) | Returns control tables from the currently focused element up to its top-level window. |
| [`host.window.list()`](window#host-window-list) | Returns an array of window tables for all enumerable top-level windows. |
| [`host.window.onFocus(cb)`](window#host-window-onfocus) | Registers `cb` to fire whenever the keyboard focus moves — including within the same top-level window. |
| [`host.window.onTrigger(matcher, opts, cb)`](window#host-window-ontrigger) | Registers `cb` to fire on every foreground change for which the new active window satisfies `matcher`. |
| [`host.window.ownsPoint(id, x, y)`](window#host-window-ownspoint) | Whether the window `id` belongs to is the one drawn at that screen point. |
| [`host.window.test(matcher, win)`](window#host-window-test) | Returns whether the given window table satisfies `matcher` (the same logic `find`/`findAll` apply). |

## host.screen

Reading pixels, and finding a picture within them.

| | |
|---|---|
| [`host.screen.imageSearch(template, opts?)`](uia-screen#host-screen-imagesearch) | Searches the screen for the first occurrence of the image at path `template` and returns its match rectangle in **screen pixels** |
| [`host.screen.imageSearchAsync(template, opts?, cb)`](uia-screen#host-screen-imagesearchasync) | Like `imageSearch`, but the region capture **and** the match both run on a worker thread; `cb` fires on a later tick. |
| [`host.screen.pixel(x, y)`](uia-screen#host-screen-pixel) | Reads the colour of the screen pixel at `(x, y)`. |
| [`host.screen.profile(opts?)`](uia-screen#host-screen-profile) | where `Axis = { min: number[], max: number[], mean: number[], r: number[], g: number[], b: number[] }` |
| [`host.screen.size()`](uia-screen#host-screen-size) | Returns the primary screen dimensions in pixels as `{ w, h }`. |

## host.ocr

Reading text that exists nowhere but on the screen.

| | |
|---|---|
| [`host.ocr.recognize(opts?)`](ocr-input-sound#host-ocr-recognize) | Recognizes text inside a screen region and returns the full text plus per-word bounding boxes. |
| [`host.ocr.recognizeMany(opts)`](ocr-input-sound#host-ocr-recognizemany) | Recognizes several regions from **one** screen capture — on Windows. |

## host.uia

Asking the accessibility layer what a window contains.

| | |
|---|---|
| [`host.uia.find(hwnd, name, controlType)`](uia-screen#host-uia-find) | Returns `true` if the UI Automation subtree of the window `hwnd` contains at least one element whose Name equals `name`… |
| [`host.uia.findAny(hwnd, names, types)`](uia-screen#host-uia-findany) | Answers "is any of these names present as any of these control types?" |
| [`host.uia.locate(hwnd, name, controlType)`](uia-screen#host-uia-locate) | Finds the first UIA element in window `hwnd` matching `name` + `controlType` and returns the screen-pixel centre of its… |
| [`host.uia.pluginLocate(hwnd, containerName, name, controlType)`](uia-screen#host-uia-pluginlocate) | Like `locate`, but for a plugin hosted inside another application. |
| [`host.uia.rawDump(hwnd)`](uia-screen#host-uia-rawdump) | Diagnostic counterpart to `host.uia.dump`, walking the **raw** tree instead of a condition-based search, so it crosses into… |
| [`host.uia.type`](uia-screen#host-uia-type) | The UIA ControlType ids by name |

## host.input

Driving the mouse and keyboard.

| | |
|---|---|
| [`host.input.click(x, y, opts?)`](ocr-input-sound#host-input-click) | Moves to `(x, y)` and synthesizes a mouse click there. |
| [`host.input.cursorPos()`](ocr-input-sound#host-input-cursorpos) | Returns the current mouse cursor position in screen coordinates. |
| [`host.input.drag(x1, y1, x2, y2, opts?)`](ocr-input-sound#host-input-drag) | Presses the mouse button at `(x1, y1)`, drags to `(x2, y2)`, and releases — with real, paced movement in between. |
| [`host.input.move(x, y)`](ocr-input-sound#host-input-move) | Moves the mouse cursor to the given screen coordinates. |
| [`host.input.scroll(x, y, amount)`](ocr-input-sound#host-input-scroll) | Moves to `(x, y)` and scrolls the mouse wheel by `notches`, which may be fractional. |
| [`host.input.send(combo)`](ocr-input-sound#host-input-send) | Sends a keyboard shortcut by pressing the modifiers, tapping the key, and releasing in reverse order. |
| [`host.input.text(text)`](ocr-input-sound#host-input-text) | Types a Unicode string as synthetic keystrokes. |

## host.keys

Claiming keys before the application sees them.

| | |
|---|---|
| [`host.keys.capture(spec, callback)`](speech-hotkey-keys-timer-log#host-keys-capture) | Begins intercepting the key spec |
| [`host.keys.menuOpen(open)`](speech-hotkey-keys-timer-log#host-keys-menuopen) | Tells the hook a plugin's own (Qt/UIA) menu is open (`true`) or closed (`false`). |
| [`host.keys.release(token)`](speech-hotkey-keys-timer-log#host-keys-release) | Undoes the exact capture identified by the `token` `host.keys.capture` returned, recomputing the global captured set so the… |
| [`host.keys.releaseAll()`](speech-hotkey-keys-timer-log#host-keys-releaseall) | Removes **all** key captures owned by this module and refreshes the suppression set. |
| [`host.keys.scope(toForeground)`](speech-hotkey-keys-timer-log#host-keys-scope) | Scopes captured-key suppression. |

## host.hotkey

Claiming a combination system-wide.

| | |
|---|---|
| [`host.hotkey.register(spec, callback)`](speech-hotkey-keys-timer-log#host-hotkey-register) | Registers a **global** OS hotkey (active regardless of foreground window) for the key spec and returns an integer `id`. |
| [`host.hotkey.unregister(id)`](speech-hotkey-keys-timer-log#host-hotkey-unregister) | Releases the OS hotkey and forgets the callback for the `id` returned by `register`. |

## host.speech

| | |
|---|---|
| [`host.speech.output(text, opts?)`](speech-hotkey-keys-timer-log#host-speech-output) | Speaks `text`; `opts.interrupt` defaults to `true` (omitting `opts` also means interrupt). |

## host.sound

| | |
|---|---|
| [`host.sound.play(path)`](ocr-input-sound#host-sound-play) | Plays an audio file from the module's package directory, fire-and-forget. |

## host.timer

| | |
|---|---|
| [`host.timer.after(ms, callback)`](speech-hotkey-keys-timer-log#host-timer-after) | Schedules a **one-shot** callback to fire approximately `ms` milliseconds later, driven from the event-loop tick. |
| [`host.timer.every(ms, callback)`](speech-hotkey-keys-timer-log#host-timer-every) | Schedules a **recurring** callback to fire approximately every `ms` milliseconds, driven from the event-loop tick. |

## host.settings

| | |
|---|---|
| [`host.settings.define(key, default, opts?)`](resource-settings-modules#host-settings-define) | Registers a setting `key` for this module, pins its kind from `default`, and |
| [`host.settings.get(key)`](resource-settings-modules#host-settings-get) | Returns the current stored value of a previously-defined setting |
| [`host.settings.onChange(key, callback)`](resource-settings-modules#host-settings-onchange) | Registers `callback` to run whenever this setting changes (via `set` or the |
| [`host.settings.set(key, value)`](resource-settings-modules#host-settings-set) | Validates `value` against the setting's schema and writes it to the store |

## host.config

| | |
|---|---|
| [`host.config.get / host.config.set / host.config.define / host.config.onChange`](resource-settings-modules#host-config-get) | `host.config` is the **same table** as `host.settings` (a catalog-compatibility |

## host.os

| | |
|---|---|
| [`host.os.current`](window#host-os-current) | Read-only string: the current OS, from Rust `std::env::consts::OS` (`"windows"`, `"macos"`, `"linux"`, …). |
| [`host.os.is(name)`](window#host-os-is) | Returns `true` when `name` equals the current OS string. |

## host.log

| | |
|---|---|
| [`host.log.info(msg)`](speech-hotkey-keys-timer-log#host-log-info) | Writes `msg` to the host log under the `module` channel. |

## host.path

| | |
|---|---|
| [`host.path(rel)`](resource-settings-modules#host-path) | Resolves a package-relative path to an **absolute** filesystem path string |

## host.resource

| | |
|---|---|
| [`host.resource.read(rel)`](resource-settings-modules#host-resource-read) | Reads a package-relative file as a UTF-8 string (`string`) from the calling |

## host.require

| | |
|---|---|
| [`host.require(id)`](resource-settings-modules#host-require) | Returns the object that dependency `id` exported, where `id` is a module declared |

## host.tryRequire

| | |
|---|---|
| [`host.tryRequire(id)`](resource-settings-modules#host-tryrequire) | Like `host.require`, but returns **nil** instead of raising when `id` |

## host.include

| | |
|---|---|
| [`host.include(rel)`](resource-settings-modules#host-include) | Loads **another file of this module** and returns whatever that file returns |

## host.epoch

| | |
|---|---|
| [`host.epoch()`](speech-hotkey-keys-timer-log#host-epoch) | A counter that changes whenever the world may have |

## Concepts

The shapes and grammars the calls above are written in.

| | |
|---|---|
| [`Key spec string format`](speech-hotkey-keys-timer-log#key-spec-string-format) | Two namespaces parse `+`-joined spec strings; segments are trimmed and case-insensitive. |
| [`Matchers`](window#matchers) | A *matcher* is a declarative table passed to `host.window.find/findAll/test/onTrigger`. |
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
| [`Plugin base + library overlays (the cell model)`](overlay#plugin-base-library-overlays) | A plugin is not one overlay. |
| [`Region form`](uia-screen#region-form) | Several functions (`host.screen.imageSearch`, `host.screen.profile`, `host.ocr.recognize`, and each entry of… |
| [`Table shapes`](window#table-shapes) | Returned by `host.window.list()`, `host.window.active()`, `host.window.find()`, `host.window.findAll()`, and passed to trigger/test callbacks. |
| [`ocrLabel — reading a control's name off the screen`](overlay#ocrlabel) | Any hotspot or hotspot-toggle may carry `ocrLabel = {x1, y1, x2, y2}` (origin-relative) |
