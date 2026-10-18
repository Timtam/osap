---
title: "host.gui — an HTML page of the module's own (experimental)"
sidebar_position: 3.7
toc_max_heading_level: 2
---

:::caution Experimental
`host.gui.webview` is stage 1 of the plan of 2026-09-28: enough for the probe, `tools/html-probe`, with which the maintainer answers the questions W1–W10 in TODO.md with NVDA on Windows. On macOS it is newer still: built on 2026-10-11, checked on CI's Macs, and not yet tried with VoiceOver; there the same probe is the instrument of the tester's part of the Mac's questions, MW1–MW21 (TODO.md, stage 4). **The API may change after the probes.**
:::

A module can show an **HTML page from its own folder in a window of the application's own**, and exchange JSON messages with it. A screen reader reads the page as it reads a web page: headings, lists, buttons, fields and live regions, in NVDA's browse and focus modes — which is the reason to want it. A page is for things a Tab ring does badly: a lot to read, a list to browse, a menu a plug-in draws in a way no screen reader reaches.

A walkthrough that builds a page step by step — beside another window, with buttons that click in it and read it — is [Building a page](../building-a-page.md), written around the example module `examples/page`.

**The page is small, visible, and beside the plug-in — over it only where there is no room beside it.** A screen reader works only in the window that has the keyboard, so a page that is read has to be in front, and it is shown where it does not cover the plug-in: a page over the plug-in is what this application's own screen reads and clicks would then see. Where no monitor has room beside the plug-in window (it is maximised, or full screen), a page brought to the front goes over part of it rather than not at all — a page that cannot be shown cannot be reached — and then every click into the plug-in and every read of it must bring the plug-in window in front of the page first ([`show`](#page-show)); a page that would be over it without the keyboard is not shown. A page that is kept invisible is not offered: Chromium stops drawing a window it considers covered or hidden and slows its scripts, and an invisible window that takes the keyboard from a program that also hooks the keyboard and synthesises input looks like malware.

**While the page is in front, the plug-in is not.** The DAW's keys and OSARA's do not reach it, and an overlay on the plug-in window is not active. An **overlay module** gives its overlay the page with [`O:addPage`](./overlay.md#o-addpage): the overlay runtime then holds the overlay's place while the page is in front — no keys, nothing said, its hooks waiting — and makes the page key, the way back and every round trip into the plug-in ([`O:inPlugin`](./overlay.md#o-inplugin)); any other module, and an overlay that lets go for a page it showed itself, deactivates for the page as for any other window, and writes the round trip by hand ([Building a page](../building-a-page.md#step-5)). An action that clicks into the plug-in is a round trip — the plug-in window forward, the click, the page forward again — and the system decides whether the page may come back ([`show`](#page-show)). On Windows, where the plug-in publishes its controls, [`host.element.press`](./element.md#host-element-press) acts on one without the round trip, the page keeping the keyboard; and on both platforms a plain click can be made without it too, where the plug-in takes it: [`host.input.click`](./input.md#host-input-click) with `via = "post"`, and on Windows `via = "noactivate"` (experimental). Which plug-ins take which is what the HTML probe (`tools/html-probe`) measures.

**Or a window of its own.** All of the above is the **tool page**, the default: a companion of the plug-in window that no list of windows shows, so that Alt+Tab stays the DAW's. A page described with [`window = "app"`](#host-gui-webview) is instead a **window like any program's** — in Alt+Tab with its title and on the taskbar (on a Mac, the application in Command-Tab and the Dock while one is shown), minimised, maximised and closed as any window is — for a page that is an interface of its own: a preset browser, a settings page, a whole user interface in HTML. Everything on this page holds for both kinds, except where it says otherwise.

**Nothing here waits.** The page is built the first time it is shown, or before with [`prepare`](#page-prepare), on the event loop after the modules' callbacks have run, and `show` answers `false, "not ready yet"` until the page has loaded; [`onReady`](#host-gui-webview) says when it has. Part of the first build — WebView2's environment, its browser process starting, and on a Mac the pages' rule list — is not waited for; how a cold start's time splits between that part and the part that is waited for is test W8 ([cost](#page-show)). No timer is involved anywhere.

## What to declare {#declare}

A module that uses this names it in its manifest:

```toml
[capabilities]
require = ["gui"]
```

See [what that list is and is not](./index.md#capabilities). The install review says of it that the module may show windows of its own and bring them to the front, taking the keyboard from the application in front — and that this experimental version does not yet keep such a page off the internet entirely ([below](#what-a-page-is-served-and-what-it-may-not-do)).

## host.gui.webview(opts) {#host-gui-webview}

**Signature:** `host.gui.webview(opts: { file: string, title: string, window: ("tool" | "app")?, activates: boolean?, onMessage: (value) -> (), onReady: (() -> ())?, onClose: ((why: string) -> ())? })` → `Page`

Describes a page and returns the object every call below is a method of. **It builds nothing and costs nothing**: no window, no process, until the first [`show`](#page-show) or [`prepare`](#page-prepare).

- `file` — the page's HTML file, relative to the module's folder, with `/` between its parts: `"ui/index.html"`. Resolved in full at once, links included, and it has to be a file inside the folder: a path that leaves it, an absolute path, a `\`, a `:`, a `.` or `..` part, or a file that is not there raises here, at the caller's line. For a `code_module`'s code running in a dependent, the folder is the code module's own, as for [every relative path](./index.md#paths).
- `title` — the window's title: what the screen reader says when the page comes to the front, and for an app page its name in Alt+Tab. Not empty.
- `window` — optional: which window the page is in, `"tool"` (the default) or `"app"`, in lower case; anything else raises. Fixed for the life of the page object — the window cannot change kind while it exists — so a module that wants both describes two pages.
  - `"tool"` — the page beside the plug-in of the paragraphs above. Not in Alt+Tab and not on the taskbar; the only way to it is a key the module offers. No minimise or maximise box.
  - `"app"` — a window like any program's: in Alt+Tab with the page's title, and on the taskbar (under the application's own button, [below](#host-gui-webview-windows)); a minimise and a maximise box besides the close box. The user reaches it as any window, the module's key included. Its close box hides it, as a tool page's does, and a `show` that does not bring it to the front never hides or moves one the user has open. Where the methods below treat it differently, they say so: [`show`](#page-show), [`focus`](#page-focus), [`hide`](#page-hide), [`isShown`](#page-isshown).

  The host hides neither kind when the user goes to another window: a module hides a tool page when it is no longer wanted — the way back to the plug-in, say — and leaves an app page open until the user closes it, as any program's window stays.
- `activates` — optional, **experimental**: whether the page's window makes this application the active one when it takes the keyboard — `true`, the default, or `false`, for a tool page only. Anything but a boolean raises, and so does `false` with `window = "app"`: an app page is a window like any program's, which makes its application the active one. Fixed for the life of the page object, as `window` is. It matters on a Mac alone, where a tool page described `false` takes the keyboard while the DAW stays the active application ([macOS](#host-gui-webview-macos)) — a way that has not met VoiceOver yet and may change or go, offered so that a module's developer can try it beside the default with their DAW; on Windows it changes nothing ([Windows](#host-gui-webview-windows)). The log's line for the description says it (`[gui] … described, …, activates = false …`).
- `onMessage(value)` — called with each value the page sends with `platform.post(value)`, decoded as [`host.json.decode`](./json.md#host-json-decode) decodes: an object is a table, `null` is `nil`. A message that is not JSON, or larger than 1 MiB, is dropped with a line in the log; so is one the module's VM has no memory left to decode, and the line says so — the decoding happens in that VM before the callback starts, so it is neither the module's error nor a [stop](../module-runtime-and-lifecycle.md#limits).
- `onReady()` — optional; called each time a document of the page's own has loaded successfully: after the first build, and again whenever the page reloads itself. A navigation the host refused (a link to anything else) does not call it. Messages posted before that are delivered to the page when it has loaded, in the order they were posted, before `onReady` runs. A first document that does not load at all (its file deleted since, say) is a build that failed: `onReady` is not called, and the next `show` answers `"could not be built"`.
- `onClose(why)` — optional; called when the window goes without the module asking: `"user"` when the user closed it with its close box (the page is **hidden**, not destroyed, and keeps its state for the next `show`, and the window it came from gets the front back, as at [`hide`](#page-hide) — on Windows the keyboard with it, on the control it was on; Alt+F4 pressed inside the page does the same — seen on 2026-10-11 — and on a Mac Command-W), `"failed"` when the page is gone — WebView2's browser or renderer process ended, or on a Mac WebKit's content process, the page's script got round the `window.close()` that does nothing ([below](#the-page-s-side)), or it went to something that is not its own and could not be refused (`about:blank`); the next `show` builds it again.

Any other option, or an option of the wrong type, raises. **The three callbacks are events of the module's**, as a hotkey's or a timer's callback is: each runs as a [handler](../module-runtime-and-lifecycle.md#handlers) of the module whose VM created the page, one after another with that module's other handlers, on the event loop — never from inside the web view's own callbacks. It runs at the first tick after the window reported the event (a tick comes every 15 ms), or, when events of the module's are still waiting to run then, after them — and while a handler of the module [waits](../module-runtime-and-lifecycle.md#a-handler-waits), for a reading say, the page's events wait with the module's others and run in order after it. A tick hands the pages' events on in the order the windows reported them, after that tick's keys, hotkeys, controller events, window and focus changes, timers and answers, and before its report of the window already in front (`onTrigger { initial = true }`) and its [`host.window.recheck()`](./window.md#host-window-recheck) round: a message the page sent just before a key was pressed can run after the key's callback. They run only while that module is enabled: what arrives while it is disabled is dropped, and so is what waited for it when it was disabled. At most 256 of a module's pages' messages wait, apart from its keys and buttons; the next are dropped, and the log says so under `[gui]` at the first, then at most every 10 seconds with how many since, and the rest once the module has caught up. `onReady` and `onClose` are never dropped, and an `onReady` that would wait right behind one of the same page and build is one with it. Each is delivered to the page and the build it came from: once the module has called `page:close()`, nothing the closed page sent reaches it, while a build that failed is heard to the end — what it sent before the failure, then `onClose("failed")` when its window failed (a first document that did not load calls none, above). `onMessage` and `onClose("user")` run in the interactive lane, as a key's callback does — a message whoever sent it, as the host cannot tell a person's click in the page from the page's own script — so a [text read](./ocr.md#host-ocr-recognize) one asks for goes ahead of the polls', and a poll's read a handler of the module waits for meanwhile is made an interactive one, as a key makes it; `onReady` in the lane of the `show` or `prepare` that started the build, or in the interactive one if a `show` or `prepare` made in it while the build was under way asked too (a poll's when the page loaded again by itself); `onClose("failed")` in a poll's. An error in one is logged and shown once in a dialog as an error in a `page message`, `page ready` or `page close` callback. Each is held to the [limits](../module-runtime-and-lifecycle.md#limits) every callback is held to: one that runs past 2 seconds of the event loop's processor time or 10 seconds in all, or runs out of memory without catching it, stops its module, and the sentence and the dialog name it as its `page message`, `page ready` or `page close` callback — and a method of the page the time ran out in, such as `in page:show`. The page belongs to the module whose VM created it: disabling that module hides it, and reloading it closes it — the reloaded code describes its page again. A module the application stopped is turned off at once, so what its pages send from then on is dropped, and at the end of that turn of the event loop its pages are hidden, as a disabled module's are; ticking it again builds it afresh, as a reload does, which closes them. A `page:close()` made inside `onReady` or `onClose` takes the window down like any other.

**Describe a page once**, at load, and keep the object: each `host.gui.webview` is a page of its own until it is closed or its module is reloaded, and one described on every key press would build a new web view each time it is shown. Nothing is freed when a page object is no longer referenced.

```luau
-- module.toml: require = ["gui", "hotkey", "window"]
local page
page = host.gui.webview({
  file = "ui/index.html",
  title = "Presets",                      -- what NVDA says when the page comes forward
  onMessage = function(msg)
    if msg.action == "hello" then
      page:post({ kind = "status", text = "Hello from the module" })
    end
  end,
  onReady = function()
    host.log.info("the page has loaded")
  end,
})
```

An app page, shown by a key of the module's — in the middle of the screen the first time, where the user left it after that:

```luau
local browser
local wanted = false
browser = host.gui.webview({
  file = "ui/browser.html",
  title = "Preset browser",               -- its name in Alt+Tab
  window = "app",
  onMessage = onBrowserMessage,
  onReady = function()
    if wanted then
      wanted = false
      browser:show()
    end
  end,
})
host.hotkey.register("Ctrl+Shift+Win+Alt+B", function()
  local ok, why = browser:show()
  wanted = not ok and why == "not ready yet"
end)
```

### Windows {#host-gui-webview-windows}

WebView2, the web engine of Microsoft Edge, through the Tauri team's wry. The WebView2 runtime is part of Windows 11 and installed on most Windows 10 machines; without it `show` answers `"WebView2 runtime not installed"`, asked again at every `show`, so installing it needs no restart. The runtime updates itself with Edge, so the engine a page runs in can change under the application without anything here changing: in 2023 an Edge update made web views silent to JAWS for a while.

A tool page's window is a tool window (`WS_EX_TOOLWINDOW`), described under [`show`](#page-show). An app page's is an ordinary top-level window with no owner, with a minimise, a maximise and a close box: an Alt+Tab entry with the page's title, and a place on the taskbar. Windows combines a program's windows under one taskbar button — Windows 11 does by default — so app pages are under the application's own button, with the module manager's window when it is open; what a screen reader reads on that button, the application's name or a page's title, is unmeasured (TODO.md). With virtual desktops, an ordinary window is on one of them and Alt+Tab and the taskbar list the current one's by default; which desktop an app page is on, and what a `show` from another desktop does, is unmeasured (TODO.md). Both are windows of this application, so the keyboard hook leaves the chain while either is in front (see [`show`](#page-show)).

`activates = false` is accepted and changes nothing: a window that takes the keyboard on Windows takes the front with it, and a tool page is the same window either way. The log's line for the description says so.

### macOS {#host-gui-webview-macos}

The system's web engine, WebKit (`WKWebView`), through the Tauri team's wry — taken on a Mac from wry's repository, at the change that leaves the application alone when a web view is built without the keyboard, until wry 0.57.1, which is to carry it, is released. WebKit is part of macOS and is updated with it, so `show` never answers `"WebView2 runtime not installed"`, and the engine a page runs in changes with the system; the first build of a session says its version in the log (`[gui] pages run in the system's web engine, WebKit …`). None of this has been tried with VoiceOver yet: what only a Mac with VoiceOver can tell is in TODO.md (stage 4), not here.

**A tool page** is a utility panel (`NSPanel`): a small title bar with a close button, resizable, not minimisable. It is in no list of windows — not in the window cycle, and Command-Tab and the Dock list applications, where this application has no icon while only tool pages are shown — so the only way to it is a key the module offers, as on Windows. It is on every Space and beside an application in full screen, and it is not hidden when the application stops being the active one. **Taking the keyboard, it makes this application the active one**, the way the avenger_control project's panel does, which VoiceOver users run with Logic: while a page has the keyboard, the DAW is not the active application. **It floats only while it has the keyboard**: above the ordinary windows while it is the key window, and at the normal level from the moment another window takes the keyboard, so a plug-in window brought forward is drawn over it — the round trip above. (That panel floats all the time instead, and is hidden before every click it sends.)

**A tool page described `activates = false`** (experimental) is the same panel made as one that takes the keyboard without activating its application (`NSWindowStyleMaskNonactivatingPanel`, given when the panel is made, since it cannot be changed afterwards): **taking the keyboard, it leaves the DAW the active application.** Nothing asks for the front — no activation, which would undo it — so what [`show`](#page-show-macos) answers is decided at the call, and leaving while the DAW is still the frontmost application hands the front back to it at once, with nothing asked and nothing waited for ([`hide`](#page-hide-macos)). Everything else is the tool page's: its level follows the keyboard, the keys are the same, and no capture applies while it has the keyboard. Whether VoiceOver follows the keyboard into such a panel, and where it lands, is unmeasured, and so is whether the keyboard is back on the plug-in window after it leaves, whether a DAW that hides its plug-in windows while another application is active keeps them shown, and whether one in full screen stays there (TODO.md, MW1b, MW6, MW18).

**An app page** is an ordinary window (`NSWindow`) — a title bar with a close, a minimise and a zoom button — at the normal level and in the window cycle, and it makes the application the active one too. It never goes full screen: its zoom button zooms it, as Windows' maximise box maximises, so a `show` with `avoid` can always put it beside the plug-in. Command-Tab and the Dock list applications, not windows, and this application runs without a Dock icon: while an app page is shown, the application has a Dock icon and an entry in Command-Tab under its own name, as it has while the module manager is open — the same claim, under the same setting of the Application settings tab — and loses both when the last app page is hidden — by its module, by the user, or because its show did not get the keyboard (see [`show`](#page-show-macos)) — closed or taken down, after the page has given the front back. A minimised app page, or one in the application hidden with Command-H, still counts as shown and keeps the claim.

**Points.** `avoid`, the page's place and its size — 520 by 640, at least 320 by 360 — are in points from the top left of the primary display, as [`host.window`](./window.md#table-shapes) gives a window's `bounds`; AppKit scales them for a Retina display.

**DAWs.** While a page has the keyboard the DAW is not the active application — unless it is a tool page described `activates = false` (above) — and some DAWs hide their plug-in windows then: Logic's floating plug-in windows and Ableton Live's are reported to, which is unmeasured here (TODO.md, MW6). With such a DAW nothing of the plug-in can be read or clicked while a page is in front — every read and every click is the round trip — and a module cannot refresh its page from the plug-in in the background. Whether a page over a DAW in full screen appears in that Space, or takes the DAW out of full screen, is unmeasured too (TODO.md, MW18).

## page\:show(opts) {#page-show}

**Signature:** `page:show(opts: { focus: boolean?, avoid: { x, y, w, h }? }?)` → `ok: boolean, reason: string?`

Shows the page, raised above the ordinary windows without taking the front, and with `focus` (the default) brings it to the front and puts the keyboard into the document — on the element that had the focus last time, or the document itself. `avoid` is a rectangle the page must not cover, in screen coordinates as [`host.window`](./window.md#table-shapes) gives them — physical pixels on Windows, points on a Mac — so a window table's `bounds` is one. Anything else in `opts`, a `focus` that is not a boolean, or an `avoid` without four numbers or with no area raises.

**Where the page goes.** The page is 520 by 640 at 100 % display scaling (scaled with the display scaling of the monitor `avoid` is on; on a Mac 520 by 640 points). With `avoid`: on the monitor that rectangle is mostly on — to its right, then to its left, then below it, then above it, each lined up with it and kept inside the monitor's work area — and failing all four, in the middle of another monitor that has room. Where none of that fits, it is made smaller, down to 320 by 360, on the side of `avoid` that leaves the most room on its monitor — and failing that, as large as another monitor's work area allows (not smaller than 320 by 360), in its middle. **Where not even that fits** — a maximised or full-screen plug-in window, and no other monitor with room — a `show` with `focus` puts the page **over part of `avoid`**: at 320 by 360, in the corner of that monitor's work area where it covers the least of `avoid` (with equal cover: bottom right, top right, bottom left, top left — a plug-in draws from the top left of its window, and a maximised window's extra room is to the right and below), and answers `true, "over the rectangle to avoid"`. A `show` with `focus` is never refused for want of room: a page that is not shown is a page the user cannot reach. But the page stays over `avoid` only in front: a `show` without `focus` answers `false, "no room beside the rectangle to avoid"` and leaves the page hidden (hiding it if it was shown), and one the system declines is hidden again (below) — except an app page the user has open, which neither hides nor moves ([Windows](#page-show-windows), [macOS](#page-show-macos)). A `show` with `avoid` gives the page its size again, so a size the user dragged it to lasts until then. Without `avoid`, the first `show` puts it in the middle of the monitor the window in front is on, and after that it stays where it is. With no monitor's work area readable at all (never seen), the page stays where it is, with a line in the log.

**A page over the plug-in window** is where the application's own clicks and screen reads would land. So a module **must** bring the plug-in window to the front **first** — [`host.window.focus`](./window.md#host-window-focus), then the click or the read on its foreground event — for every click into the plug-in **and every screen read of it** (a capture, OCR, a pixel of the plug-in's rectangle would otherwise contain the page), and show the page again after: the page is not topmost, so the activated plug-in window is drawn over it. The host does not do this for the module. [`host.window.ownsPoint`](./window.md#host-window-ownspoint) answers `false` for a point the page still covers, so a runtime that asks before it clicks does not click into the page; nothing guards a screen read. The log says the first time a run of shows goes over `avoid`, with why.

**The answers.** `true` when the page is shown and — with `focus` — in front (on a Mac, the front asked for: [macOS](#page-show-macos)); a second value only when it had to go over `avoid` (above), which only a `show` with `focus` that ends in front does: `true, "over the rectangle to avoid"`. Otherwise `false` and one of these, in the order they are decided:

| reason | when | the page afterwards |
|---|---|---|
| `"headless"` | a [headless run](../module-manager.md#headless-mode): there is no window system to show anything in | not built |
| `"not supported on this platform yet"` | a platform with no page window: any but Windows and macOS, which the application does not run on | not built |
| `"WebView2 runtime not installed"` | Windows without the runtime | not built; the next `show` asks again |
| `"not ready yet"` | the first `show` — it starts the build — and every `show` until the page has loaded | building or loading; `onReady` comes when it has loaded |
| `"could not be built"` | the build started by an earlier `show` or [`prepare`](#page-prepare) failed — WebView2's environment or the page's controller could not be made (on a Mac, the pages' rule list or the web view), or the page's first document did not load; the log says why | said once: the next `show` starts a new build |
| `"no room beside the rectangle to avoid"` | a `show` without `focus`, and no room beside `avoid` (above) | hidden — never over `avoid` without the keyboard; an app page the user has open stays open, where it is |
| `"declined by the system"` | Windows refused to bring the page to the front; on a Mac, only a refusal that comes at once ([macOS](#page-show-macos)) | **shown and raised, without the keyboard**: the window in front keeps it, as far as Windows answered — Windows has been seen to bring a declined page to the front all the same a moment later ([Windows](#page-show-windows)), so the answer does not say where the keyboard ends up — or, where it would be over `avoid`, **hidden**: a page over the plug-in that the plug-in window, still in front, could not be raised above again; an app page the user has open is then neither moved, raised nor hidden |

A page that was never shown, or was closed, is built by the first `show`, which answers `"not ready yet"`: show it again from `onReady` if it is still wanted — or build it before it is wanted, with [`prepare`](#page-prepare). The build runs at the end of a tick, never inside a Luau call.

**Cost.** On Windows — a Mac's is [below](#page-show-macos) — the first build of a session starts WebView2's processes — a browser process, a renderer, a GPU process and utility processes, shared by every page of the application — and Microsoft calls the delay of a cold start "noticeable": the maintainer's first measurement (2026-09-28, a debug build) was 4.4 seconds for the build and 8.4 seconds until the page had loaded. A build has two parts:

- **WebView2's environment** — its browser process starting, its data folder opened. **Not waited for**, apart from the call that asks for it (it loads WebView2's library into the application): the event loop goes on — module callbacks, hotkeys, speech — until WebView2 answers through it, and the tick after the answer builds the page. `[gui] the WebView2 environment is being created …`, then `… is ready, N ms after it was asked for (M ms of that in the call that asked …)`. Should WebView2 never answer, every `show` and `prepare` of a page waiting for it answers `"not ready yet"`, with one `… waits for WebView2's environment` line for each build — never seen.
- **The page itself** — its window, and its controller in that environment, which wry makes in a message loop of its own. **While this part runs, no module callback runs**: the event loop waits in it (the application's windows still answer), and hotkeys and captured keys pressed meanwhile reach their callbacks when it is over, as behind any long callback. It runs after the tick's callbacks, inside none of them, so however long it takes it counts against no module's [time limit](../module-runtime-and-lifecycle.md#limits): the `show` that asked for it only queued it. `[gui] … building in WebView2's environment …`, then `… built in N ms`. Which of the two parts holds most of a cold start is unmeasured (TODO.md, W8): the controller brings up the renderer and GPU processes. A dialog of the application's own opened meanwhile (the module manager's) holds it until it is closed. Should WebView2 never answer here, the wait would not end and no key of the application would work until it is quit — never seen; the log's last line would be `building …`.

Loading the page afterwards does not hold the loop up: `… ready, N ms after the show or prepare that asked for it`. Once ready, and after the first successful `show` of each build, a line has the WebView2 processes' working set and private memory, taken at the end of that tick, outside the `show`. A `show` of a built page is a few window calls. **The environment is shared and kept while its browser process runs**: a second page, or one built again after `close`, is built in it while any page of the application is open. After the last page closes, WebView2 ends its processes (`[gui] the WebView2 browser process ended …`), and the next build makes a new environment — again without waiting, and in the runtime version installed then. A build that fails in an environment a page was built in before is tried once more in a new one (`… tried once more, in a new environment`) before `show` answers `"could not be built"`. Whether the application runs as a debug or a release build changes only the host's own part of this — making a window and registering the page's handlers, milliseconds — not WebView2's.

```luau
local plugin = nil
host.hotkey.register("Ctrl+Shift+Win+Alt+F7", function()
  plugin = host.window.active()
  if not plugin then return end
  local ok, why = page:show({ focus = true, avoid = plugin.bounds })
  if not ok and why ~= "not ready yet" then
    host.speech.output("Page not shown: " .. why)
  end
end)
```

### Windows {#page-show-windows}

The window is a tool window with a title bar and a close box: **no taskbar button and no Alt+Tab entry** (`WS_EX_TOOLWINDOW`), so the only way back to a page is a key the module offers. It is **not topmost**: NVDA speaks for a topmost window that is not in front, and a page's live region would then talk over the plug-in. It is shown without being activated and raised to the top of the ordinary windows (`SetWindowPos`), so it is not left under the window that covered it last, and `focus` then asks Windows with `SetForegroundWindow`, and hands the keyboard to the document (`MoveFocus`). The log's line for each `show` says whether the point in the middle of the page is really the page's (`on top at its centre: yes`) — whether it is covered is measured, not assumed. Moved onto a monitor of another display scaling, the window is resized by Windows for that scaling; the host puts it back at the rectangle it chose, so it still does not cover `avoid`.

**An app page** is an ordinary window (see [`host.gui.webview`](#host-gui-webview)), shown in the same way: without being activated — wxWidgets shows an ordinary window by activating it, so the host shows it as wxWidgets shows a disabled one and enables it again at once — then raised, not topmost, and brought to the front by `focus` alone. It goes where a tool page goes, and keeps its state as the user left it otherwise: a minimised app page is restored by `show`, without being activated; a maximised one stays maximised, unless `avoid` is given — then it is restored and put beside `avoid` as a tool page is. One hidden while minimised comes back restored; one hidden while maximised comes back maximised — or, with `avoid`, restored and beside `avoid`. Without `avoid`, the first `show` puts it in the middle of the monitor the window in front is on, and after that it stays where the user moved it. **An app page the user has open is the user's window:** where there is no room beside `avoid`, a `show` without `focus` answers `"no room beside the rectangle to avoid"` and leaves it as and where it is, and a `show` with `focus` asks for the front first, where the page is, and puts it over `avoid` only once it has it — declined, the page is left where it was (restored, if it was minimised), neither raised nor hidden. When Windows declines to bring an app page to the front, it flashes the taskbar button the page is under instead (Microsoft's documentation of `SetForegroundWindow`).

Windows lets a program bring its own window to the front only under conditions ("the calling process received the last input event", "the calling process is the foreground process", and a few others), and may refuse even then. A key delivered as `WM_HOTKEY` meets them; whether a key matched in this application's keyboard hook does — which is how every hotkey arrives unless it is registered with [`{ foreground = true }`](./hotkey.md#host-hotkey-register) — is test W2: on 2026-10-11 both ways were accepted every time. Showing the page by itself when a plug-in window comes to the front, with no key pressed, was declined in 13 of 26 shows that day — most of them right after a switch of virtual desktops — and in 12 of those 13 the page came to the front all the same, 60 to 150 milliseconds later: `"declined by the system"` is what `SetForegroundWindow` answered, not where the keyboard ends up. A module that needs to know follows the page's window coming to the front ([`host.window.onTrigger`](./window.md#host-window-ontrigger)), or the page's script its window's `focus` event. Handing the front on while the page is in front is allowed, so to land in a particular window — the plug-in's — take it forward with [`host.window.focus`](./window.md#host-window-focus) **first** and [`hide`](#page-hide) the page after. Hidden while it is in front, the page gives the front back to the window it came from ([`hide`](#page-hide)), which is the plug-in's only when the page was brought forward from there. Either way **the keyboard goes back to the control it was on** when the page took the front — the plug-in's own child window, after a key pressed there — not only the front to its window: the host lands it there ([`host.window.focus`](./window.md#host-window-focus), Windows). W10 of the probe saw why: brought forward by a click, REAPER's FX window had the front and its keyboard was nowhere until NVDA's F7 had been pressed three times.

NVDA before 2026.1 did not enter a web view that had just been created until the focus was set into it again; the host sets it on every `show` with `focus` (a line in the log when that fails). wry moves the keyboard into the page as well when its window becomes active, so it is moved twice. Whether that is enough with the maintainer's NVDA, and whether NVDA then says the page twice, is test W1. The page's script sees its window's `focus` event more than once as the page comes to the front — in the probe's log of 2026-10-11, mostly `focus`, `blur` and `focus` again within a few milliseconds — so a script that acts on that event acts each time, or only on what is still waiting for it.

**Low-level keyboard hooks and a page.** For keys going to a window of this application's own — a page's frame and the web view inside it included — Windows calls neither the application's low-level keyboard hook nor the hooks behind it in the chain while the application's is ahead of them, and Microsoft documents neither. On 2026-09-28 NVDA's hook was not called for the keys going into a page until NVDA was restarted, which put its hook first; on 2026-10-10 the same was seen in the module manager. So the keyboard hook **leaves the chain while a window of the application's own is in front**, a page included, and comes back first in the chain when a window of another program comes to the front — see "Out of the chain in this application's own windows" under [`host.keys.capture`](./keys.md#host-keys-capture). A screen reader's hook is then called for the keys going into the page, whichever program was started first. No capture applies in a page, and hotkeys still arrive, through `RegisterHotKey`. The log says it as the page comes forward (`[keys] the keyboard hook is out while one of our windows is in front ('<title>') …`) and as the plug-in comes back (`… is back, first in the chain …`). A key pressed in the moment between the page coming forward and the hook going out reaches the page unseen by the application's hook and every hook behind it, a screen reader's there included — a hotkey's own key-ups, say, when they come that soon. Whether NVDA follows a page from its first show on is test W1 and W4 (TODO.md).

### macOS {#page-show-macos}

**Shown without `focus`**, the window is ordered in front of the other windows at the normal level, without the keyboard (`orderFrontRegardless`). **With `focus`**, this application asks for the front (`NSApplication`'s `activateIgnoringOtherApps:`), the window is made the key window and ordered in front, and the web view is given the keyboard (`makeKeyAndOrderFront`, `makeFirstResponder`) — at the normal level until it is the key window. An activation is asynchronous, so here **`true` means that the front was asked for**, not that the page has it. What came of it is an event, said in the log, and only events decide it, never a length of time:

- the page's window becomes the key window: `[gui] page '…': got the keyboard, N ms after the show asked for the front`;
- another application becomes the active one first: `… the front went to <application> (pid N) instead`;
- the event tap sees a key pressed while no window of this application has the keyboard — that key went elsewhere: `… a key went to <application>`; this needs the permission the tap needs;
- the page is hidden first.

A page this `show` put over `avoid` that did not get the keyboard is hidden again when the event says so, as a declined one is on Windows — and an app page hidden so gives up the Dock icon as any hidden app page does; an app page the user has open goes over `avoid` only once it has the keyboard this `show` asked for, and stays where it was if it does not get it. The window is ordered in front with the request, before any answer: a page that does not get the keyboard has been raised all the same, at the normal level. So an app page the user has open that does not get it — or is answered `"declined by the system"` — is neither moved nor hidden, but raised, where on Windows it is not raised either (the table above). So, too, the second value comes with the request: `true, "over the rectangle to avoid"` says where the page goes, not that it is there — an app page the user has open goes there only once it has the keyboard, and not at all if it does not get it. `"declined by the system"` is answered only when the refusal comes at once: this application was the active one already, or the page is one described `activates = false`, and the window is still not the key window after the calls. A module that needs to know follows the page's window coming to the front ([`host.window.onTrigger`](./window.md#host-window-ontrigger)), or the page's script its window's `focus` event. **A tool page described `activates = false`** asks for no activation: the window is made the key window and ordered in front and the web view given the keyboard, and with nothing on its way the answer is decided at the call — `true` with the window the key window (the log's `got the keyboard` line comes with it), `"declined by the system"` otherwise, and over `avoid` it is then hidden again at once. A page hidden before its answer came leaves the request on its way, and that can still make this application the active one with nothing of its own on screen — so when AppKit says it did, and no window of the application that could take the keyboard is shown, the front goes back where the page would have given it.

**Where it goes.** As above, in points: a display's work area is its frame without the menu bar and the Dock. Without `avoid`, the first `show` puts the page in the middle of the display the window with the keyboard is on. The log's `on top at its centre` is the window server's answer for the point in the middle of the page.

**The keyboard inside the page** goes back where it was each time the page's window takes the keyboard — the host asks at AppKit's word that the window became the key window, and the page's own window `focus` event asks too: the element the keyboard was last on gets it back if it is still in the page and shown; otherwise WebKit's choice stands. Until the user's first key, pointer press or click in the page, the keyboard going to nothing — the document's body — is put back once each time it goes, with a line in the log; a move to another element is never undone, since with VoiceOver's "keyboard focus follows the VoiceOver cursor" the user's own navigation moves it. No timer decides anything. The log says where the keyboard is (`the keyboard in the page is …`) and which kinds of view AppKit made the first responder while the page was shown (`AppKit's first responder is now a …`).

**Keys.** The page and WebKit see every key first. A Command key they leave unhandled is the host's, whatever menu the application has installed — also under a keyboard layout whose letters are not Latin, where the letters typed with Command held decide, as they do for a menu:

- Escape does nothing.
- Command-W does what the close box does: the page is hidden and gives the front back, and `onClose("user")` is called.
- Command-X, C, V, A, Z and Shift-Command-Z cut, copy, paste, select all, undo and redo in the field the keyboard is in.
- In a tool page, Command-Q, Command-H and Option-Command-H do nothing: the application goes on running and shown, and the DAW's own shortcuts stay the DAW's. The log says so once a page.
- In an app page they do what they do in any application: Command-Q quits the application as its Quit item does, Command-H hides it and Option-Command-H hides the others; Command-M minimises the page.

No capture applies while a window of this application has the keyboard — the event tap lets captured keys through to the page — and hotkeys still arrive: see "Keys in this application's own windows" under [`host.keys.capture`](./keys.md#host-keys-capture). While the event loop is busy — a page being built, a file of a page being read — every key waits for it, as behind any long callback. A password field turns on secure input for the whole system while it has the keyboard, and the event tap then sees no key at all: a page has no password fields.

**Cost.** There is no controller to wait for. The first build of a session waits for the pages' rule list, which WebKit compiles off the event loop: the event loop goes on meanwhile, and the tick after WebKit's answer builds (`[gui] the pages' rule list is being compiled …`, then `… is compiled, N ms after it was asked for …`). A page is then a window and a web view, made on the event loop — while they are made, no module callback runs (`building with WebKit …`, then `… built in N ms`) — and it loads afterwards without holding the loop up. `page-check` measures both on CI's Macs ([building on macOS](../building-on-macos.md#page-check)); on a real Mac they are unmeasured (TODO.md). The memory of WebKit's processes is not measured: they are the system's, started for this application, and the log says so once.

## page\:prepare() {#page-prepare}

**Signature:** `page:prepare()` → `ok: boolean, reason: string?`

Builds the page without showing it, so that the first `show` does not wait for the engine's cold start: `true` when the page is ready already, and otherwise the answers `show` gives before a page is shown — `"headless"`, `"not supported on this platform yet"`, `"WebView2 runtime not installed"`, `"could not be built"` (said once, as by `show`), and `"not ready yet"` when it started the build or one is under way. `onReady` comes when the page has loaded; a `show` after that shows it at once. It never shows, moves or focuses the window, and calling it again while the page's build is under way starts nothing.

The build is the one `show` starts (see its cost): WebView2's environment, or a Mac's rule list, is not waited for, the page's own part is. So prepare at a moment when a short wait of the event loop costs nothing much — when the module is loaded, or when the window the page is for first comes to the front — rather than inside the action the user is waiting for. A prepared page costs its engine's memory (on Windows test W8) for as long as it is not closed.

```luau
-- Build the page the first time a plug-in window comes forward, not on the first key — and again
-- after the page failed.
local ready = false
page = host.gui.webview({
  file = "ui/index.html", title = "Presets",
  onMessage = onMessage,
  onReady = function() ready = true end,
  onClose = function(why) if why == "failed" then ready = false end end,
})
host.window.onTrigger(pluginMatcher, function(w)
  if w and not ready then
    page:prepare()
  end
end)
```

### Windows {#page-prepare-windows}

As `show`'s build, hidden.

### macOS {#page-prepare-macos}

As `show`'s build, hidden: the window and its web view are made, and the window stays out of sight until a `show`.

## page\:focus() {#page-focus}

**Signature:** `page:focus()` → `boolean`

Brings the shown page to the front and the keyboard into its document, as `show` does with `focus`, and says whether the system accepted — on a Mac, that the front was asked for. `false` for a page that is hidden or not built — it does not show one — and wherever `show` would not work. Logged with the window in front afterwards.

```luau
if not page:focus() then
  host.speech.output("Could not bring the page back.")
end
```

### Windows {#page-focus-windows}

`SetForegroundWindow`, under the conditions in [`show`](#page-show). A minimised app page is restored first, without being activated, and then asked to the front; declined, it stays restored.

### macOS {#page-focus-macos}

As `show` with `focus` ([macOS](#page-show-macos)): `true` means that the front was asked for, and the log says what came of it. A minimised app page is brought back first, and the application with it when it was hidden. `false` for a page that is hidden or not built, and when this application was the active one already — or the page is one described `activates = false` — and the window still did not become the key window. It places nothing, and so hides nothing: a page an earlier `show` left over `avoid` stays shown when it does not get the keyboard, as on Windows — but ordered in front with the request, at the normal level, where on Windows a declined `focus` leaves it as it was; a plug-in window brought forward is drawn over it, as in the round trip.

## page\:hide() {#page-hide}

**Signature:** `page:hide()` → `nil`

Hides the window. The page stays loaded, with its state, its focus and its scroll position, and the next `show` shows it at once. Nothing for a page that is not built.

**Hidden while it is in front, the page gives the front back first** to the window that had it when the page last took it — with `show`'s `focus`, or [`focus`](#page-focus) — if that window is still there and shown. A page that took the front from another page of the application's, hidden since — one shown in place of the other: the new one first, then the old one hidden, so the front goes from page to page and the screen reader never hears the plug-in window between them — gives it to the window that other page came from. Otherwise, or when the system declines, the system gives the front to the window next in its order — on a Mac, to the application next in line — which need not be the plug-in's: the page's window has no owner. The same happens when the user closes the page, when [`close`](#page-close) takes it down in front, and when its module is disabled or reloaded. A page that is not in front gives nothing back: a module that brought the plug-in window forward first has decided where the front goes. **An app page gives the front back only when it has kept it since it last took it**: one the user came to with Alt+Tab, the taskbar or a click — or left for another window and came back to — is left as any program's window is, and the system decides. The log says each case — on Windows `[gui] page '<title>': leaves the front; given back to window <id>, which had it before the page`, or why Windows decides.

```luau
host.window.focus(plugin.id) -- the plug-in first, while the page still has the front
page:hide()
```

One page in place of another, without the plug-in window in between:

```luau
-- The new page first, while this application has the front; then the old one, no longer in
-- front, which hands nothing on. The new one gives the front back to where the old one came from.
local ok = settings:show({ avoid = plugin.bounds })
if ok then
  browser:hide()
end
```

### Windows {#page-hide-windows}

The window is hidden and WebView2 stops drawing it (`IsVisible` false); its scripts go on running, and `post` still hands it messages. The front is given back with `SetForegroundWindow`, which Windows allows the process that has the front. **The keyboard goes back with it**: when the page took the front ([`show`](#page-show)'s `focus`, [`focus`](#page-focus)), the host noted the control the keyboard was on in that window — a window inside it, its thread's focus as `GetGUIThreadInfo` says — and kept the last one it found for the same window when the keyboard was on nothing there; given the front back, that window's keyboard is landed on that control, as [`host.window.focus`](./window.md#host-window-focus) lands it on a control. A page that gives the front to where another page came from gives it that page's control. And because a module that brings the plug-in forward **first** leaves the page nothing to hand back, `host.window.focus` of the window the page in front came from lands on that control too. Whether the keyboard took is in the log, under `[window]`. Whether Chromium runs a hidden or covered page's handlers at once or later is unmeasured (TODO.md). An app page has kept the front from the moment it takes it with `show`'s `focus` or [`focus`](#page-focus) until its window is deactivated, for whatever reason. A hidden app page leaves the taskbar and Alt+Tab until it is shown again.

### macOS {#page-hide-macos}

The window is ordered out and its web view hidden; its scripts go on running, and `post` still hands it messages ([below 14](#what-a-page-is-served-and-what-it-may-not-do-macos)). The front is given back only while the page is the key window — and an app page's only when it has kept the front since it took it — to the window and the application it took it from: this application is deactivated, that application's activation asked for at once, and the window raised and given the focus when the workspace announces that application as the active one, as [`host.window.focus`](./window.md#host-window-focus) does, ended by events only as there. With Logic, the first foreground event after it may name Logic's main window, and the plug-in's only later (unmeasured, TODO.md): act when [`host.window`](./window.md#host-window-foreground) reports the plug-in's window in front. A page that took the front from an application whose window could not be named gives it back to that application, whose own choice of window then stands. A tool page described `activates = false` never made the DAW stop being the active application: hidden while it has the keyboard, with the workspace still naming the application it came from the frontmost one, it is ordered out, and the window it came from is raised and given the focus at once, with nothing asked and nothing waited for — through accessibility, as [`host.window.focus`](./window.md#host-window-focus) raises and focuses a window, which needs the Accessibility grant. Should AppKit give the keyboard to another window of the application as the page leaves — a page shown without the keyboard, the module manager — the application is deactivated, which asks nothing of the DAW. With another application named the frontmost one by then, the front is given back as from any page, that application's activation asked for and waited for. Whether the keyboard is back on the plug-in window afterwards is unmeasured (TODO.md, MW1b). An app page has kept the front from the moment its window gets the keyboard that `show`'s `focus` or [`focus`](#page-focus) asked for, taking it from another window, until it lets the keyboard go — a request that is not got leaves it not kept, so one the user later comes to with Command-Tab or a click is left as any application's window is; hidden as the last app page shown, it takes the Dock icon and the Command-Tab entry with it, after the front has gone back.

## page\:post(value) {#page-post}

**Signature:** `page:post(value: any)` → `nil`

Sends the page a value. It is encoded here, by the host, as [`host.json.encode`](./json.md#host-json-encode) encodes it — a value JSON cannot hold raises, naming `page:post` — and it reaches the page as **data**: the page's `platform.onmessage(value)` receives the parsed value. No script is ever made out of it, so a string with quotes, `</script>` or line separators in it is just a string.

A page that has loaded is handed it at once, whether it is shown or hidden (whether a hidden or covered page's script handles it at once is unmeasured, TODO.md). Before that — never shown, building, loading, or closed and not yet built again — it waits, in order, and is delivered when the page has loaded, before `onReady` runs; at most 256 wait, and the oldest gives way to a newer one, with a line in the log. `close` drops what is waiting. Where no page can ever be shown — a headless run — it is dropped at once.

```luau
page:post({ kind = "preset", name = presetName })
```

### Windows {#page-post-windows}

`PostWebMessageAsJson`, asynchronous: `post` does not wait for the page.

### macOS {#page-post-macos}

`callAsyncJavaScript`, with a constant script and the JSON text as its one argument, asynchronous: `post` does not wait for the page. One WebKit could not hand over is a line in the log.

## page\:isShown() {#page-isshown}

**Signature:** `page:isShown()` → `boolean`

Whether the window is on screen, asked of the window system rather than remembered: `false` after the user closed it too. A page that is shown but not in front is shown.

```luau
if page:isShown() then page:hide() end
```

### Windows {#page-isshown-windows}

`IsWindowVisible`. A minimised app page is shown.

### macOS {#page-isshown-macos}

`isVisible`. An app page that is minimised, or in this application while it is hidden (Command-H), is shown: it comes back with it.

## page\:close() {#page-close}

**Signature:** `page:close()` → `nil`

Takes the window and its web view down, and drops the messages waiting for it. The description stays: the next `show` builds the page again, answering `"not ready yet"` first. For a page that is not built it drops the messages waiting for it as well, and a build a `show` or `prepare` started and that has not got as far as a window — queued, or waiting for WebView2's environment — is called off: its `onReady` does not come, and the next `show` starts a new one. `onClose` is not called for a close the module made, and nothing the closed page sent that has not reached the module yet reaches it ([above](#host-gui-webview)). Taken down while it is in front, the page gives the front back first, as at [`hide`](#page-hide).

```luau
page:close()
```

### Windows {#page-close-windows}

The page's controller is closed, and its window hidden and destroyed. WebView2's processes are shared by every page of the application, and ending them is WebView2's decision.

### macOS {#page-close-macos}

The web view and its window are taken down. WebKit's processes are the system's, and ending them is WebKit's decision.

## page.window {#page-window}

**Signature:** `page.window` → `number?`

The window's id from the first time it was shown until it is closed: the id [`host.window.active()`](./window.md#host-window-active) and [`host.window.foreground()`](./window.md#host-window-foreground) report while the page is in front, so a module can tell its own page from the plug-in. `nil` before that. A field, not a method.

```luau
local front = host.window.foreground()
if front and front.id == page.window then
  host.log.info("the page is in front")
end
```

### Windows {#page-window-windows}

The page's top-level window, an `HWND`, for both kinds. `host.window.active()` reports it with the page's title and this application's executable.

### macOS {#page-window-macos}

The page's window as [`host.window`](./window.md#table-shapes-macos) gives this application's own: 2⁴⁰ plus the window server's number for it, for both kinds, answered by AppKit and never through accessibility. `host.window.active()` reports it with the page's title and this application. [`host.window.ownsPoint`](./window.md#host-window-ownspoint) answers `false` for a point a page covers.

## The page's side — platform.post and platform.onmessage {#the-page-s-side}

The host puts a small script into every page before the page's own, and it gives the page one object, `platform`:

- `platform.post(value)` sends the module a value; it arrives at `onMessage`. The value goes through `JSON.stringify`, so a function or a cycle throws in the page.
- `platform.errors` holds the codes of the host's [error values](./errors.md#error-values), as `host.errors` does for a module: `platform.errors.NOT_READY` is `"not_ready"`. It is frozen, and a name it does not have throws a `TypeError` listing the codes — except the four JavaScript itself asks an object for, `then`, `toJSON`, `toString` and `valueOf`, which read as `undefined` as on any object without them, so `JSON.stringify(platform.errors)` gives the codes and `await` or an async function's `return` hands it back as it is. Turning it into a string, `String(platform.errors)` or a template literal, throws, as it does for any object without a prototype. A module that posts an error value — `page:post({ ok = false, error = err })` — hands the page `{ ok: false, error: { code, message, detail } }`, `detail` only when the error has one, and the page compares `error.code` with these, never the message.
- `platform.onmessage = function (value) { … }` receives what the module posts. Messages that arrive before the page sets it wait for it (up to 256). A handler that throws does not stop the next message.

It hides which engine carries the messages, so a page is written once. The same script makes `window.close()` do nothing: a page is closed by its module, not by itself. The page's own script comes from its own files — inline scripts do not run, see below — and puts data into the document as **text**, with `textContent`: names read off the screen, preset names from a file and file names are data somebody else wrote, and put in with `innerHTML` they could become part of the page.

```html
<!-- ui/index.html -->
<h1 id="preset">Preset</h1>
<button type="button" data-action="next">Next preset</button>
<p id="status" aria-live="polite"></p>
<script src="page.js"></script>
```

```js
// ui/page.js — actions only on "click", which NVDA's browse mode also sends
document.addEventListener("click", (e) => {
  const b = e.target.closest("[data-action]");
  if (b) platform.post({ action: b.dataset.action });
});
platform.onmessage = (m) => {
  if (m.kind === "preset") {
    document.getElementById("preset").textContent = m.name;
    document.getElementById("status").textContent = "Preset: " + m.name;
  }
};
```

A failure goes to the page as it came, and the page compares its code:

```luau
-- The module: a preset it could not find, posted with the error value.
local err = host.errors.new(host.errors.NOT_FOUND, "no preset named '" .. name .. "'")
page:post({ kind = "preset", ok = false, error = err })
```

```js
// ui/page.js
platform.onmessage = (m) => {
  if (m.kind === "preset" && m.ok === false) {
    document.getElementById("status").textContent =
      m.error.code === platform.errors.NOT_FOUND ? "No such preset." : m.error.message;
  }
};
```

**Keys.** In NVDA's browse mode the page does not get most keys: letters, arrows, Home and End, Page Up and Down belong to NVDA's own navigation, and Enter or Space on a button reach the page as a click, with no key event. So every action is a real button, link or form field handled on `click`; a page has no shortcuts of its own ("N for next"); and the key that opens and leaves the page is the module's hotkey, not a key in the page. `role="application"` would give the page every key and take browse mode away, which is the reason for a page at all. In focus mode — NVDA switches to it in a field — the first Escape only goes back to browse mode, so "Escape closes the page" takes two presses from a search field. Which keys reach a page in each mode is test W4 (TODO.md). On a Mac, what the host does with a key the page leaves unhandled is under [`show`](#page-show-macos).

**What works for a screen reader** (Chromium with NVDA; the same list under WebKit with VoiceOver is unverified, TODO.md): headings (`h1`–`h6`), a `main` landmark, native `button`, `a href`, `input` with a `label`, `select`, `fieldset` with `legend`, lists and tables, and `aria-live="polite"` on an element whose text changes. A live region speaks only while the page is in front: NVDA takes no events from a window that is not. And a page coming to the front is announced — its title, then the element that has the focus — which cuts off a sentence said just before: with NVDA, a status the module posted right after the `show` that brought the page back was not heard (2026-10-11). What must be heard as the page comes back goes where the keyboard lands: the [walkthrough's example](../building-a-page.md#step-5) posts a round trip's result before the `show`, and the page puts it into its status line and the keyboard focus on it (`tabindex="-1"`), so that the page comes back on it. Whether NVDA then says it is not measured yet (TODO.md).

## What a page is served, and what it may not do {#what-a-page-is-served-and-what-it-may-not-do}

**Its own folder, and nothing else.** A page is served from a scheme of its module's own (`mod-<module id>-<hash>`, the hash 64 bits of SHA-256 of the whole id, so no module can pick an id that lands on another's; on Windows `http://mod-….localhost/`, on a Mac `mod-…://localhost/`, and storage belongs to an origin, so two modules' pages never share it), by the host rather than by any server — no port is opened. Every request is resolved in full, links included, and answered only with a file inside the module's folder; anything else is 404, and `GET` and `HEAD` are the only methods. A file is read whole when the page asks for it, on the event loop, so a page's files should be small; one larger than 16 MiB is answered 413 rather than read. Each is served with the type its extension names (UTF-8 for text) and `Cache-Control: no-store`, so a page built again shows the files as they are on disk.

**A policy of the host's.** Every response carries this `Content-Security-Policy` header, which no page can loosen:

```text
default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; media-src 'self'; connect-src 'self'; manifest-src 'none'; worker-src 'none'; frame-src 'none'; child-src 'none'; object-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'none'
```

Scripts come only from the module's files — no inline script, no `eval`, no `onclick="…"` attributes — so text that got into the document cannot run. `fetch` and `XMLHttpRequest` reach the module's own files (a JSON data file, say) and nothing else — no WebSocket, no event stream, no beacon — and on Windows every request for anything outside the page's own origin — one a refused navigation makes meanwhile included — is answered 403 by the host before WebView2 sends it anywhere, and on a Mac blocked before WebKit sends it ([macOS](#what-a-page-is-served-and-what-it-may-not-do-macos)). No frames, no form submissions, no plugins, no workers. Inline `style` is allowed; it runs nothing.

**Not kept off the internet yet.** Two ways out are neither a request nor covered by any policy: **WebRTC** (a page can open a connection to a relay server on the internet and send it data) and the browser's **look-ahead** (`<link rel="dns-prefetch">` and `preconnect` resolve and connect to a name before anything is requested, which sends the name out). The host sends `X-DNS-Prefetch-Control: off` with every response; what Chromium in WebView2, or WebKit on a Mac, still looks up is unmeasured. Closing both is stage 3 (TODO.md), and until then the install review says a page is not kept off the internet entirely. Whether the policy is enforced at all is measured by the probe (TODO.md).

**It stays where it is.** A navigation to anything but the page's own origin is refused, and so are new windows (`window.open`, a link with a target), downloads, links that would start another program (`mailto:` …) and every permission a page can ask for (the microphone, notifications, reading the clipboard …). Each refusal is a line in the log — the first twenty of a page; after that they are counted, and the count is said when the page goes. `alert`, `confirm` and `prompt` show nothing, and the developer tools are off. On Windows the context menu and the browser's own keys (F5, Ctrl+R, Ctrl+F, Ctrl+P, Alt+Left …) are off too; a Mac's are [below](#what-a-page-is-served-and-what-it-may-not-do-macos).

**What a page can still do, as any web page can.** On Windows, write the clipboard: that needs no permission in Chromium, so a page in front can put text on it (`navigator.clipboard.writeText`). And read a file **the user picks**: in a file field's dialog, by dropping one on the page, or by printing to a file — never one the page names. A Mac's: [below](#what-a-page-is-served-and-what-it-may-not-do-macos).

**Messages only from itself.** A message is taken only from the page's main document on its own origin, and none larger than 1 MiB; the host never builds script from a module's strings — what the module posts is JSON the host encoded.

**No page data is kept.** WebView2's data folder is `WebView2` beside the application, like its settings and log, and pages run privately: their storage, cookies and cache are held in memory by the one private session every page of the application shares — each origin's apart from the others' — and are gone when WebView2's browser process ends, which is when the application's last page is closed, or with the application. WebView2 still writes files of its own into that folder: its settings, and a crash report when one of its processes crashes, which can hold what the page showed. A runtime too old to run pages privately (before WebView2 101) keeps a page's storage in that folder; the log says so. On a Mac, [below](#what-a-page-is-served-and-what-it-may-not-do-macos). Anything a page has to remember goes to the module in a message and into [`host.settings`](./settings.md#host-settings-define).

### Windows {#what-a-page-is-served-and-what-it-may-not-do-windows}

**WebView2 reads settings from outside the application.** Microsoft's loader takes `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` (and the policy of the same meaning) **in addition to** the application's own arguments, and `WEBVIEW2_USER_DATA_FOLDER` and `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` **instead of** its own — any process of the same user can set them, to open a remote debugging port, say. The host writes every `WEBVIEW2_*` variable that is set, with its whole value (a line break written out as `\n`) and what it does, into the log when the first page of the session is built (`no WEBVIEW2_* environment variable is set` otherwise), and a line of its own for each switch there that opens the pages to another program or turns a protection off (`--remote-debugging-port`, `--no-sandbox`, `--renderer-cmd-prefix` …). It logs them and nothing more. The registry policies of the same meaning need an administrator to set; the host neither reads nor logs them.

### macOS {#what-a-page-is-served-and-what-it-may-not-do-macos}

**Served** as `mod-…://localhost/` — WebKit serves an application's own scheme as it is — by the host, on the event loop, with the same files, answers and headers as on Windows. `page-check` holds WebKit to the policy on CI's Macs: a page's own script must run, and an inline one must not ([building on macOS](../building-on-macos.md#page-check)).

**Every other load is blocked** by a content rule list, compiled once a session and kept in the folder `WebKitRules` beside the application rather than in WebKit's own under `~/Library`: before WebKit sends anything, it blocks every request but the pages' own schemes and `data:` URLs, whatever the page's policy lets through. The first build of a session waits for it, and the event loop does not; a rule list that does not compile is a page that is not built (`"could not be built"`, and the next `show` asks again). Like Windows' 403, it does not cover WebRTC or WebKit's own look-ahead.

**It stays where it is**, as above: navigations away, new windows, downloads and permissions are refused, each a line in the log. WebKit's own context menu is not turned off; a reload from it is the page reloading itself, and calls `onReady` again. WebKit in an application has no keys of a browser's: a Command key the page leaves unhandled is the host's ([`show`](#page-show-macos)). Whether such a page can write the clipboard, and whether it counts as a secure context, is measured by `page-check` on CI's Macs and not written down here yet (TODO.md). A file field opens the system's open panel, which wry runs as a modal dialog: whether module callbacks run while it is open is unmeasured (TODO.md).

**No page data is kept**: each build has a data store of its own that keeps nothing on disk, so a page built again starts empty, and two pages do not share storage. Which folders WebKit still makes under `~/Library` is measured by `page-check`, as data. A page whose window is hidden is not slowed down on macOS 14 and later (WebKit's `inactiveSchedulingPolicy`); macOS 12 and 13 have no such switch, and how long a page hidden there for minutes takes to answer its first message is unmeasured (TODO.md).
