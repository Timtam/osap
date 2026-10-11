---
title: "Building a page — a walkthrough"
sidebar_position: 2.5
---

# Building a page

A **page** is an HTML page from your module's own folder, shown in a window of the application's own. A screen reader reads it as it reads any web page — headings, lists, buttons, fields, a status line that speaks when it changes — and its buttons do nothing by themselves: each one sends your module a message, and your module does the work. It brings another program's window to the front, clicks in it, reads what is there, and tells the page what happened.

This page builds one up from the smallest case to a page whose buttons click in another window, introducing each idea where it is needed. The finished module is `examples/page` in the repository: a page beside Notepad with two buttons that click into Notepad's text — which only moves the caret — and read the line there, and one that goes back to Notepad. For exact signatures, see the [host.gui reference](api/gui.md).

:::caution Windows only, for now
Pages run on **Windows** today. On a Mac a module that describes a page loads, the description is checked exactly as on Windows, and every `show` answers `false, "not supported on this platform yet"`; the Mac's pages are being built ([Step 8](#step-8)). `host.gui.webview` is experimental, and the API may change after the Windows probe.
:::

**A page or an overlay?** An [overlay](building-an-overlay.md) lays a Tab ring over the plug-in itself, and the user never leaves the plug-in's window. A page is a window of its own: while it is in front, it has the keyboard and the plug-in does not — the DAW's keys do not reach the plug-in, and an overlay on the plug-in window is not active then. A page is for what a Tab ring does badly: a lot to read, a list to browse, a menu the plug-in draws where no screen reader reaches it.

---

## The vocabulary, in one place

| Term | What it means |
| --- | --- |
| **Page** | An HTML file of the module's, described with [`host.gui.webview`](api/gui.md#host-gui-webview) and shown in a window of the application's own. |
| **Tool page** | The default kind: a companion of the window it works for, shown beside it, in no list of windows. |
| **App page** | A page described with `window = "app"`: a window like any program's, in Alt+Tab and on the taskbar. |
| **Target window** | The window the page acts in — Notepad here, a plug-in window in a real module. |
| **`avoid`** | The rectangle a `show` must not cover: the target window's bounds. |
| **Message** | A JSON value. The page sends one with `platform.post`, and it arrives at `onMessage`; the module sends one with `page:post`, and it arrives at `platform.onmessage`. |
| **Round trip** | An action in the target window while the page is in front: the target window forward, the action, the page forward again. |
| **Live region** | An element whose text the screen reader says by itself when it changes, while its page is in front: the page's status line. |

---

## Step 1 — the smallest page {#step-1}

A module with a page is a module like any other — a folder with a `module.toml` and a Luau entry file — with the page's files beside them. Make a folder of your own and build it up step by step; `examples/page` is where it ends up:

```text
my-page/
  module.toml
  src/main.luau
  ui/index.html
  ui/page.css
```

The manifest names the module and lists what its own code calls: `gui` for the page, `hotkey` for the key that shows it, `speech` for saying why it did not come. A page is Windows-only today, and so is the window this example clicks in, so it says so (see [`supported_os`](module-package-format.md#supported_os-and-why-omitting-it-is-the-safe-default)):

```toml
# module.toml
id           = "com.example.my-page"
name         = "My Page"
version      = "0.1.0"
supported_os = ["windows"]

[capabilities]
require = ["gui", "hotkey", "speech"]
```

The page is plain HTML. Its stylesheet is a file of its own beside it (`ui/page.css`, any CSS you like):

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Click helper</title>
<link rel="stylesheet" href="page.css">
</head>
<body>
<main>
<h1>Click helper</h1>
<p>Window: <span id="target">none yet</span></p>
<h2>Actions</h2>
<ul class="actions">
<li><button type="button" data-action="middle">Click the middle of the text</button></li>
<li><button type="button" data-action="lower">Click lower left in the text</button></li>
<li><button type="button" data-action="back">Back to the window</button></li>
</ul>
<h2>Status</h2>
<p id="status" aria-live="polite">Nothing clicked yet.</p>
</main>
</body>
</html>
```

And `src/main.luau` describes the page once and shows it on a key:

```luau
local KEY = "Ctrl+Shift+Win+Alt+F12"

local page -- described below; the functions before it reach it only when they run
local wanted = false -- a show answered "not ready yet": shown once the page has loaded

-- The page has loaded: shown now if a show was waiting for it.
local function onPageReady()
  if wanted then
    wanted = false
    local ok, why = page:show()
    if not ok then
      host.speech.output("The page did not come to the front: " .. why, { interrupt = true })
    end
  end
end

page = host.gui.webview({
  file = "ui/index.html",
  title = "Click helper", -- what the screen reader says as the page comes to the front
  onMessage = function(m) end, -- Step 2
  onReady = onPageReady,
})

host.hotkey.register(KEY, function()
  local ok, why = page:show()
  if ok then
    return
  end
  if why == "not ready yet" then
    wanted = true
    host.speech.output("Building the page.", { interrupt = true })
  else
    host.speech.output("The page did not come to the front: " .. why, { interrupt = true })
  end
end)
```

Put your folder under `modules/` beside the application, or run it with `automation-platform <folder>` (from the repository: `cargo run -p app -- path/to/my-page`), and press Ctrl+Shift+Win+Alt+F12 in any window. The page comes up in the middle of the monitor the window in front is on, takes the keyboard, and the screen reader says its title. (The finished `examples/page` does what Steps 4 to 6 build: it answers the key only in a Notepad window, and puts the page beside it.)

Five things happened that are worth knowing:

- **Describing builds nothing.** `host.gui.webview` checks its options — `file`, `title` and `onMessage` are required, `onReady`, `onClose` and `window` optional, and anything else raises at your line, as does a `file` that is not a file inside your module's folder — and returns the page object. No window and no process exists until the first `show` (or [`prepare`](#step-4)). **Describe a page once**, at load, and keep the object: each call describes a page of its own, and nothing is freed when an object is no longer referenced.
- **The first `show` builds the page** and answers `false, "not ready yet"`; `onReady` says when the page has loaded, and that is where you show it. The first build of a session starts WebView2, Edge's web engine, and takes seconds — the maintainer's first measurement was 4.4 seconds for the build and 8.4 until the page had loaded, in a debug build — and for part of it **no module callback runs**: your keys and every other module's wait until it is over. [Step 4](#step-4) moves that wait to a better moment.
- **Say what every `show` answers, the one in `onReady` too.** It comes seconds after the key, and Windows may decline to bring a page to the front when no key has just asked for it ([Windows](api/gui.md#windows-1)). [Step 4](#step-4) replaces this `onReady` with one that also checks that the user is still where they pressed the key.
- **`show()` with no options brings the page to the front** with the keyboard in the document. Without [`avoid`](#step-4), it goes in the middle of the monitor the first time and stays where it is after that.
- **Nothing waits.** `show` never holds your callback until the page is there; it answers at once, and `onReady` comes later, as an event of your module.

---

## Step 2 — messages, both ways {#step-2}

The page talks to your module through one object the host puts into every page before the page's own script: `platform`. Add the script to the page's head —

```html
<script src="page.js" defer></script>
```

— and write `ui/page.js`:

```js
"use strict";

const status = document.getElementById("status");
const target = document.getElementById("target");

if (window.platform) {
  document.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-action]");
    if (button) window.platform.post({ action: button.dataset.action });
  });
  window.platform.onmessage = (message) => {
    if (!message || typeof message !== "object") return;
    if (message.kind === "status") status.textContent = String(message.text);
    else if (message.kind === "target") target.textContent = String(message.title);
  };
} else {
  // Opened in a browser, to work on how it looks and reads: there is no module to talk to.
  status.textContent = "This page is not running in the automation platform.";
}
```

On the module's side, `onMessage` gets each value the page posts, and `page:post` sends one back. The callbacks grow from here on, so the example gives them names; `onPageMessage` goes beside `onPageReady`, before the description that names them both:

```luau
local function onPageMessage(m)
  if type(m) == "table" and m.action ~= nil then
    page:post({ kind = "status", text = "The module heard: " .. tostring(m.action) })
  end
end

page = host.gui.webview({
  file = "ui/index.html",
  title = "Click helper",
  onMessage = onPageMessage,
  onReady = onPageReady,
})
```

What to know about the two directions:

- **Everything is JSON.** `platform.post(value)` runs the value through `JSON.stringify`, so a function or a cycle throws in the page; it arrives decoded as [`host.json.decode`](api/json.md#host-json-decode) decodes, an object as a table. `page:post(value)` is encoded by the host and reaches `platform.onmessage` as the parsed value — data, never script, so a string with quotes or `</script>` in it is just a string. A message that is not JSON, or larger than 1 MiB, is dropped with a line in the log.
- **Text goes into the page as text.** The page puts what it is sent into the document with `textContent`, never `innerHTML`: a window title, a line read off the screen, a preset name from a file are text somebody else wrote.
- **Nothing is lost while the page loads.** What the module posts before the page has loaded waits, in order, and is handed over before `onReady` runs (up to 256 messages; the oldest gives way). What arrives before the page sets `platform.onmessage` waits for it too.
- **`onMessage`, `onReady` and `onClose` are events of your module**, as a hotkey's callback is: each runs as a [handler](module-runtime-and-lifecycle.md#handlers), one after another with your module's other handlers, under the same [time limits](module-runtime-and-lifecycle.md#limits), and an error in one is logged and shown once in a dialog as an error in a `page message`, `page ready` or `page close` callback. A message counts as something a person did — the host cannot tell a click in the page from the page's own script — so a text read it asks for is recognised before reads asked from a [`host.timer.every`](api/timer.md#host-timer-every) poll, as a key's is.
- **Validate what arrives.** The example checks that a message is a table and acts only on the actions it knows.

---

## Step 3 — a page a screen reader can use {#step-3}

A page exists because a screen reader reads a web page well. Keep it that way:

- **Real elements.** A `<button>` for every action, a `<label>` for every field, headings (`h1`–`h6`) and a `<main>` landmark for the structure, lists and tables for what is a list or a table. With NVDA in Chromium these are known to work; so is `aria-live="polite"` on an element whose text changes ([what works](api/gui.md#the-page-s-side)).
- **Actions on `click`, nothing else.** In NVDA's browse mode most keys never reach the page — letters, arrows, Home and End belong to NVDA's own navigation — and Enter or Space on a button arrive as a click, with no key event. So every action is a button, a link or a form field handled on `click`, as `page.js` does with one listener for the whole document.
- **No keys of the page's own.** No "N for next", no `role="application"` (it would give the page every key and take browse mode away, which is the reason for a page at all), and nothing that catches Tab or Escape: the user must be able to leave every element the way they came. The key that opens the page and leaves it is your module's hotkey ([Step 6](#step-6)).
- **Leave the focus alone, but for a round trip's result.** The host puts the keyboard into the document as the page comes forward — on the element that had the focus last time, or the document itself the first time. Do not move the focus from a script while the user is in the page — what the page is told then goes into its live region — and keep a button enabled even when its action cannot run now: a button disabled under the keyboard takes the focus away with it. The example answers such a press in its status instead. The one move it makes is onto a round trip's result, told while the page is away (next point).
- **Results in a live region, there from the start; a round trip's result where the focus lands.** Put the live region in the HTML, as the example's status line is, and change only its text: a screen reader announces changes to a live region it already knows, so one a script inserts with its text already in it is, as a rule, not announced. A live region speaks only while its page is in front, and **a page that comes to the front is announced** — its title, then the element that has the focus — and that announcement cuts off a sentence said just before it: in the example's first run with NVDA (2026-10-11), a click's result posted to the status line right after the `show` that brought the page back was not heard. So a round trip's result is not a change of the live region as the page comes back: the module posts it just before the `show`, while the target window still has the keyboard, and the page puts it into the status line and the keyboard focus on it ([Step 5](#step-5)). The page comes back on the result, and the result is in the announcement of the page rather than before it. Not heard yet (TODO.md): whether NVDA says the result as the page comes back, and once; and whether it says a status that the page is told while it is in front with the same text as the one before — two presses of a button whose action cannot run now give the same status.
- **Do not talk over the screen reader when the user moves.** When the user has just gone to another window, the screen reader is saying where they landed; what the module has to say about it goes with `interrupt = false`, queued behind what is being said rather than cutting it off ([`host.speech.output`](api/speech.md#host-speech-output)). The example does so when a round trip is given up ([Step 5](#step-5)).

You can open `ui/index.html` in any browser to look at it and walk through it with a screen reader. The host's part is missing there — `window.platform` is `undefined`, which is why `page.js` checks for it — and so is its content policy: an inline script runs in a browser and **not** in the host ([Step 9](#step-9)). And the engine is the browser's, not WebView2.

---

## Step 4 — beside the window it works for {#step-4}

A page that clicks in a window has to stay off it: a page over the window is what the module's own clicks and screen reads would hit. Give `show` the window's bounds as `avoid`, and the page goes beside it. The module now finds the window with `host.window`, and writes a line to the log, so its manifest adds `"window"` and `"log"`.

Two calls tell the module about the window in front, and they answer different questions. [`host.window.active()`](api/window.md#host-window-active) gives the whole [window table](api/window.md#table-shapes) — its title, its `bounds`, its `client` area, its application — and is kept for the [epoch](api/timer.md#host-epoch): asked again before anything turns the epoch over, it gives the same answer. [`host.window.foreground()`](api/window.md#host-window-foreground) gives only the id (with the process and whether the window is shown), read afresh at every call. So the module takes Notepad's table from `active()` when the key is pressed, and asks `foreground()` whether a window is the one in front **now**. These go near the top of the file, after the state of Step 1:

```luau
local NOTEPAD = { app = { name = "notepad" } }
local target = nil -- the Notepad window the page was last shown beside, as a window table
local ready = false -- the page is built and has loaded
local prepareFailed = false -- a prepare failed: from then on only the key builds the page

-- Whether window `id` is the one in front now.
local function inFront(id)
  local front = host.window.foreground()
  return front ~= nil and front.id == id
end

-- Window `w` as it is now — it may have moved — or nil when it is gone.
local function current(w)
  if not w then
    return nil
  end
  for _, x in ipairs(host.window.list({ pids = { w.app.pid } })) do
    if x.id == w.id then
      return x
    end
  end
  return nil
end
```

A window table is a picture of the window when it was asked for; `current` asks [`host.window.list`](api/window.md#host-window-list) for that window again, so the page goes beside the window where it is now.

Then the page beside Notepad, on the key — `notShown` and `showBeside` after `current` and before `onPageReady`, which calls `showBeside`; the hotkey at the end of the file, in place of Step 1's:

```luau
local function notShown(why)
  if why == "declined by the system" then
    return "Windows did not bring the page to the front; if it does not come by itself, the key brings it."
  end
  return "The page did not come to the front: " .. why .. "."
end

local function showBeside(w)
  target = w
  page:post({ kind = "target", title = w.title })
  local ok, why = page:show({ focus = true, avoid = w.bounds })
  if ok then
    wanted = false
  elseif why == "not ready yet" then
    wanted = true
    host.speech.output("Building the page.", { interrupt = true })
  else
    wanted = false
    host.speech.output(notShown(why), { interrupt = true })
  end
end

host.hotkey.register(KEY, function()
  local w = host.window.active()
  if not (w and host.window.test(NOTEPAD, w)) then
    host.speech.output("Press this key in a Notepad window.", { interrupt = true })
    return
  end
  showBeside(w)
end)
```

**Where it goes.** The page is 520 by 640 at 100 % display scaling. With `avoid`, it goes to the right of the rectangle, then the left, then below, then above, on the monitor the rectangle is mostly on — then to another monitor with room, then smaller, down to 320 by 360. Where there is no room anywhere — a maximised Notepad, a full-screen plug-in — a `show` with `focus` puts the page **over part of** the rectangle, in the corner where it covers the least, and answers `true, "over the rectangle to avoid"`: a page that cannot be shown cannot be reached. From then on, every click into the window and every read of it has to bring the window in front of the page first, which is what [Step 5](#step-5) does anyway. The full rules are under [`page:show`](api/gui.md#page-show).

**The answers.** `show` answers `true`, or `false` and a reason, and the module says the reason: `"headless"` (no window system), `"not supported on this platform yet"` (macOS), `"WebView2 runtime not installed"`, `"not ready yet"`, `"could not be built"` (the log says why), `"no room beside the rectangle to avoid"` (only without `focus`), and `"declined by the system"` — Windows refused to bring the page to the front, and it is shown and raised without the keyboard, or hidden again where it would be over the rectangle. That last one is not "the page is not there": Windows has been seen to bring a declined page to the front all the same a moment later ([Windows](api/gui.md#windows-1)). So `notShown` says it in words of its own — Windows kept the keyboard where it was, the page may still come, and the key brings it — and every other reason as the page not coming.

**A page that loads late is not pushed at the user.** The first build takes seconds, and the user may have gone elsewhere meanwhile. So `onReady` shows the page only if it was asked for and its window is still the one in front. This replaces Step 1's `onPageReady`:

```luau
local function onPageReady()
  ready = true
  if wanted and target and inFront(target.id) then
    showBeside(current(target) or target)
  else
    wanted = false
  end
end
```

**Build it before it is wanted.** [`page:prepare()`](api/gui.md#page-prepare) builds the page hidden, so the first key shows it at once. The build holds the event loop for part of it either way, so do it when that costs least: when the window the page is for comes to the front, rather than on the key the user is waiting on. `onReady` comes when it has loaded. An `onClose("failed")` means it is gone; once the `onClose` of [Step 6](#step-6) has set `ready` back to `false`, the next Notepad window prepares it again. A prepare that answers a failure — `"could not be built"`, `"WebView2 runtime not installed"` — is not tried again from here, where nobody asked for it: the key builds the page from then on, and says why it cannot. The function goes after `showBeside`, and the trigger after the description, beside the hotkey:

```luau
local function prepare()
  local ok, why = page:prepare()
  if not ok and why ~= "not ready yet" then
    prepareFailed = true
    host.log.info("Page demo: the page was not prepared (" .. why .. "); the key builds it from now on.")
  end
end

host.window.onTrigger(NOTEPAD, { initial = true }, function(w)
  if not ready and not prepareFailed then
    prepare()
  end
end)
```

`{ initial = true }` also reports the Notepad window that is already in front when the module loads ([`onTrigger`](api/window.md#host-window-ontrigger)).

---

## Step 5 — a button that acts in the window: the round trip {#step-5}

Now the buttons. A click into Notepad is not made while the page is in front: the page has the keyboard and the screen reader with it, a page over Notepad would take the click itself, and the rule is the target window forward first ([`page:show`](api/gui.md#page-show)). So every action in the target window is a **round trip**, each step on the event that says the one before is done:

1. **Ask the target window to the front** — [`host.window.focus(id)`](api/window.md#host-window-focus). `true` means Windows accepted the request, not that it has happened; `false` means it refused, and nothing is clicked.
2. **Act on its foreground event.** [`host.window.onTrigger`](api/window.md#host-window-ontrigger) reports the window coming to the front — never wait a guessed time instead. Check that it is still in front when the callback runs: an event can wait behind others, and the user may have gone on.
3. **Work the point out from the window's origin.** The top left of the window's client area, `w.client.x` and `w.client.y`, is the window's origin — the one an overlay's coordinates are relative to, too; add your offset to it. Coordinates are [screen coordinates](api/index.md#coordinates) — physical pixels on Windows, points on a Mac — and a click cuts a fraction toward zero, so work in whole numbers. Notepad's text grows with its window, so the example uses fractions of the client area; a plug-in, whose layout does not grow, gets offsets in pixels.
4. **Ask what is drawn there** — [`host.window.ownsPoint`](api/window.md#host-window-ownspoint). `false` means another window covers the point (a notification, a tooltip, the page itself): click nothing. `nil` means the platform cannot say, and is read as permission.
5. **Click** — [`host.input.click(x, y)`](api/input.md#host-input-click).
6. **Read, with a callback** — [`host.ocr.recognize(region, cb)`](api/ocr.md#host-ocr-recognize). The picture is taken at the call, after the click; the corners are whole numbers. The callback form never holds your module.
7. **Bring the page back in the read's answer** — if the target window is still in front — beside the window and with the keyboard, the result posted to the page just **before** the `show`, while the target window still has the keyboard: the page puts the keyboard focus on it, so that the page comes back on the result ([Step 3](#step-3)).

At any step, **another window coming to the front means the user went elsewhere**: give the trip up, click nothing more, and do not bring the page back over them.

The manifest adds `"input"` and `"ocr"`:

```toml
[capabilities]
require = ["gui", "hotkey", "window", "input", "ocr", "speech", "log"]
```

**Where each piece goes.** Luau resolves a `local` at the line where it is written, so a local function has to come before every function that calls it, or the call finds `nil` (`attempt to call a nil value`). The pieces below are in the order they go into `main.luau`: the constants, the trip's state and `status` at the top, with the others; the other functions after Step 4's `prepare`; `onPageMessage` in place of Step 2's, before the description, which names it; the trigger at the end, in place of Step 4's. `examples/page/src/main.luau` is the whole file, in that order.

The constants, the trip's state, and the status line:

```luau
local POINTS = {
  middle = { x = 0.5, y = 0.5, words = "the middle" },
  lower = { x = 0.25, y = 0.75, words = "lower left" },
}
local LINE = 24 -- pixels above and below the click that are read
local trip = nil -- the click under way: { point, id, stage = "forward" | "read" }

local function status(text)
  page:post({ kind = "status", text = text })
end
```

The point to click, the band to read around it, and the sentence a reading makes — the row of `r.lines` nearest the click, "Nothing is written there." for a reading with no rows, and `r.error` for one whose `status` is `"failed"` ([the reading](api/ocr.md#host-ocr-recognize)):

```luau
local function pointIn(w, p)
  local c = w.client
  return c.x + math.floor(c.w * p.x), c.y + math.floor(c.h * p.y)
end

local function lineAround(w, y)
  local c = w.client
  return { c.x, math.max(c.y, y - LINE), c.x + c.w, math.min(c.y + c.h, y + LINE) }
end

local function lineWords(r, y)
  if r.status == "failed" then
    return "It could not be read: " .. r.error .. "."
  end
  local best, distance = nil, math.huge
  for _, row in ipairs(r.lines) do
    local d = math.abs(row.y + row.h / 2 - y)
    if d < distance then
      best, distance = row, d
    end
  end
  if not best then
    return "Nothing is written there."
  end
  return "The line there: " .. best.text
end
```

The trip's two ends. `giveUp` is for another window, said with `interrupt = false` ([Step 3](#step-3)); `comeBack` posts the result and then brings the page back, and when the page does not come, says so with the result, and logs it; `arrived` is Notepad in front — checked once more, since its event may have waited — the click, and the read whose answer brings the page back:

```luau
local function giveUp()
  local clicked = trip ~= nil and trip.stage == "read"
  trip = nil
  if clicked then
    host.speech.output("Clicked, but another window came to the front before the line was read.", { interrupt = false })
  else
    host.speech.output("Another window came to the front; nothing was clicked.", { interrupt = false })
  end
end

local function comeBack(w, text)
  trip = nil
  page:post({ kind = "result", text = text })
  local ok, why = page:show({ focus = true, avoid = w.bounds })
  if not ok then
    host.log.info("Page demo: the page did not come back (" .. why .. "); the result waits on it: " .. text)
    host.speech.output(notShown(why) .. " " .. text, { interrupt = true })
  end
end

local function arrived(w)
  if not inFront(w.id) then
    giveUp() -- its foreground event waited, and the user has gone on since
    return
  end
  local mine = trip
  mine.stage = "read"
  local x, y = pointIn(w, mine.point)
  if host.window.ownsPoint(w.id, x, y) == false then
    comeBack(w, "Something covers Notepad there; nothing was clicked.")
    return
  end
  host.input.click(x, y)
  host.ocr.recognize(lineAround(w, y), function(r)
    if trip ~= mine then
      return -- given up meanwhile: another window came, the key was pressed, or the page closed
    end
    if not inFront(mine.id) then
      giveUp() -- the user went elsewhere, and that window's foreground event has not come yet
      return
    end
    comeBack(w, "Clicked " .. mine.point.words .. ". " .. lineWords(r, y))
  end)
end
```

The trip's start. A button press starts it — only one made while the page is in front, since a press can reach the module after the user has gone elsewhere — and never a second trip on top of one. [`page.window`](api/gui.md#page-window) is the page's own window id from its first show, which `foreground()` reports while the page is in front:

```luau
local function clickAndRead(p)
  if trip then
    status("The last click is still under way.")
    return
  end
  local w = current(target)
  if not w then
    status("The Notepad window is gone.")
    return
  end
  trip = { point = p, id = w.id, stage = "forward" }
  if not host.window.focus(w.id) then
    trip = nil
    status("Notepad would not come to the front; nothing was clicked.")
  end
end

local function onPageMessage(m)
  if type(m) ~= "table" then
    return
  end
  if not inFront(page.window) then
    return -- pressed before the user went elsewhere: not acted on
  end
  if POINTS[m.action] then
    clickAndRead(POINTS[m.action])
  end
end
```

And the window trigger from Step 4 becomes one for every window, because a trip has to hear any other window coming to the front — at its first step or while the read is out:

```luau
host.window.onTrigger({}, { initial = true }, function(w)
  if trip then
    if w.id ~= trip.id then
      giveUp()
    elseif trip.stage == "forward" then
      arrived(w)
    end
    return
  end
  if not ready and not prepareFailed and host.window.test(NOTEPAD, w) then
    prepare()
  end
end)
```

The page's side of the result. The status line in `index.html` gets `tabindex="-1"`, which lets a script focus it and keeps it out of Tab's way:

```html
<p id="status" aria-live="polite" tabindex="-1">Nothing clicked yet.</p>
```

and `platform.onmessage` in `page.js` a branch for the result — the status line's text, and the keyboard focus on it — in place of Step 2's:

```js
window.platform.onmessage = (message) => {
  if (!message || typeof message !== "object") return;
  if (message.kind === "status") {
    status.textContent = String(message.text);
  } else if (message.kind === "result") {
    status.textContent = String(message.text);
    status.focus();
  } else if (message.kind === "target") {
    target.textContent = String(message.title);
  }
};
```

- **The page comes back on the result.** `show` puts the keyboard on the element that had the focus last ([`page:show`](api/gui.md#page-show)) — the status line, which the script focused while the page was away: a screen reader announces a page coming to the front with the element that has the focus, so the result is in that announcement, not before it. The page's window can get its `focus` event more than once as it comes back ([Windows](api/gui.md#windows-1)); the focus in the document stays on the result through them.
- **A status goes to the live region.** While the page is in front, a status — "The last click is still under way." — changes the live region and leaves the focus where it is.
- **The focus is on the result afterwards.** Shift+Tab goes back to the buttons, and in browse mode the arrows go on from it as from any line.
- **Nothing is timed.** The result waits, focused, however long the page takes to come: by itself after a declined `show`, or with the key. A newer result replaces one that waits.

What to know about the trip:

- **Windows decides whether the page may come back.** It lets a program bring its own window to the front only under conditions, and may refuse even then. Refused, `show` answers `false, "declined by the system"`: the page is shown and raised, without the keyboard — or hidden again, where it would have been over the window — so the module says the result with [`host.speech`](api/speech.md), after `notShown`'s sentence, and writes it to the log. The page has it as well, focused: Windows has been seen to bring a declined page to the front all the same a moment later ([Windows](api/gui.md#windows-1)), and the page then comes back on it, as it does when the user brings it with the key.
- **The page comes back after the read, not with the click.** The read's answer is the event that ends the trip, and the click has been made before its picture was taken. A value the click itself changes — a preset name after "next" — is drawn when the window gets round to it: wait for the change with [`host.screen.snapshotAsync`'s change wait](api/screen.md#waiting-for-the-picture-to-change) and read that snapshot.
- **A click from a hotkey carries the hotkey's modifiers.** On Windows a synthesised click inherits whatever the user is still holding. The example clicks from a page message, where nothing is held; a module that starts a trip from its own key waits for [`host.keys.modifiersDown()`](api/keys.md#host-keys-modifiersdown) to answer `false` first (and declares `keys` for it).
- **The trip is heard.** The module says nothing on a trip that goes well, but the screen reader follows the keyboard: as Notepad comes forward it says what it says for any window coming to the front — Notepad's title, its text field, the line at the caret — and as the page comes back, the page's title and the element with the focus: the result ([Step 3](#step-3)). In the example's first run with NVDA, the click came through, Notepad came forward and the page came back, and a result posted to the status line right after the `show` was not heard: the announcement of the page cut it off. Whether the result is heard now that the page comes back on it is what the next run listens for (TODO.md). With the HTML probe in REAPER, NVDA was heard announcing the focus leaving the page and coming back, and the page's focused element again. Acting in a plug-in without bringing its window to the front is being researched (TODO.md, "W3: acting in the plug-in without the round trip"); the round trip is the way that works everywhere.
- **The mouse pointer stays where it clicked.** [`host.input.click`](api/input.md#host-input-click) moves the pointer to the point and leaves it there, in Notepad's text. Whether a screen reader that follows the mouse — NVDA does by default — reads what is under it during the trip is not measured.
- **An answer can come too late.** Text read off the screen comes back later, and the user may have moved on by then. The example gives the trip up when another window's foreground event comes, compares `trip` with the one the read was asked for, and asks `inFront` once more before it brings the page back; the key and a closed page give the trip up too.

---

## Step 6 — back to the window, and away {#step-6}

**Back.** The "Back to the window" button, and the key pressed while the page is in front, take the user back to Notepad — the window to the front **first**, while the page still has the front, and only then the page hidden. `backToTarget` goes after `prepare` (it calls `status` and `current`, and `onPageMessage` calls it); `onPageMessage` gets its `back` branch, and the hotkey of Step 4 its first lines:

```luau
local function backToTarget()
  trip = nil
  local w = current(target)
  if not w then
    status("The Notepad window is gone.")
    return
  end
  if not host.window.focus(w.id) then
    status("Notepad would not come to the front.")
    return
  end
  page:hide()
end

local function onPageMessage(m)
  if type(m) ~= "table" then
    return
  end
  if not inFront(page.window) then
    return -- pressed before the user went elsewhere: not acted on
  end
  if m.action == "back" then
    backToTarget()
  elseif POINTS[m.action] then
    clickAndRead(POINTS[m.action])
  end
end

host.hotkey.register(KEY, function()
  if inFront(page.window) then
    backToTarget()
    return
  end
  trip = nil
  local w = host.window.active()
  if not (w and host.window.test(NOTEPAD, w)) then
    host.speech.output("Press this key in a Notepad window.", { interrupt = true })
    return
  end
  showBeside(w)
end)
```

The order matters. [`page:hide()`](api/gui.md#page-hide) on a page that is in front gives the front back to the window that had it when the page took it — which is Notepad's only when the page was brought forward from there — and otherwise Windows picks the next window. A page that is no longer in front gives nothing back: the module that brought its window forward first has decided where the front goes. Declined, the example keeps the page in front rather than leave the keyboard to whichever window Windows picks.

**Your key works in the page.** While a window of the application's own is in front — a page included — the application's keyboard hook leaves the chain, so that a screen reader's hook gets the page's keys; hotkeys still arrive, and no [capture](api/keys.md#host-keys-capture) applies there.

**Away.** The page goes in other ways too, and each is a step of its own:

- **The user closes it** — its close box, or Alt+F4 in the page. `onClose("user")` runs; the page is only **hidden**, keeps its state (a word typed into a field is still there), and the front goes back to the window it came from.
- **It fails** — WebView2's process ended. `onClose("failed")`; the next `show` builds it again, answering `"not ready yet"` first. The example sets `ready` back to `false`, so that Notepad coming forward prepares it again.
- **Your module is disabled** — the page is hidden; **reloaded** — it is closed, and the reloaded code describes it again.
- **[`page:close()`](api/gui.md#page-close)** takes the window and its web view down; the description stays, and the next `show` builds it again. `onClose` is not called for a close the module made. `window.close()` in the page's own script does nothing: a page is closed by its module.

The example's `onClose`, and the description as it stands at the end:

```luau
local function onPageClose(why)
  trip = nil
  if why == "failed" then
    ready = false
  end
end

page = host.gui.webview({
  file = "ui/index.html",
  title = "Click helper", -- what the screen reader says as the page comes to the front
  onMessage = onPageMessage,
  onReady = onPageReady,
  onClose = onPageClose,
})
```

**Editing the page.** The host serves the page's files with `Cache-Control: no-store`, so a page built again shows them as they are on disk. After an edit, [reload the module](module-manager.md#reload) — or press Ctrl+Alt+Shift+Win+F5, which reloads every module — and the next `show` builds the page from the files you saved.

---

## Step 7 — a tool page, or a window of its own {#step-7}

Everything so far is the **tool page**, the default: a companion of the window it works for. A page described with `window = "app"` is instead **a window like any program's**:

```luau
local browser = host.gui.webview({
  file = "ui/browser.html",
  title = "Preset browser", -- its name in Alt+Tab
  window = "app",
  onMessage = onBrowserMessage,
})
```

| | Tool page (`"tool"`, the default) | App page (`"app"`) |
| --- | --- | --- |
| Alt+Tab, taskbar | in neither: the only way to it is your module's key | in both (on the taskbar under the application's own button, where Windows combines a program's windows, as Windows 11 does by default) |
| Window boxes | close | minimise, maximise, close |
| When the user goes elsewhere | stays where it is; the module hides it when it is no longer wanted | stays open, as any program's window does |
| `hide` while in front | gives the front back to the window it came from | the same, but only if it has kept the front since it last took it |
| A `show` that cannot be beside `avoid` without the keyboard | hidden | not shown; one the user has open stays as and where it is |

Choose the tool page for a companion of the plug-in — a list of its presets, its menu read aloud — and the app page for an interface of its own that the user comes back to by Alt+Tab: a preset browser, a settings page, a whole user interface in HTML. The kind is fixed for the page object's life; a module that wants both describes two pages, and can show one in place of the other — the new one first, then the old one hidden — so that the front goes from page to page without passing through the plug-in window ([`page:hide`](api/gui.md#page-hide)). On a Mac each kind already has its decided shape for when pages are built there ([macOS](api/gui.md#macos)).

---

## Step 8 — on macOS {#step-8}

**Pages are not built for macOS yet.** On a Mac, today:

- `host.gui.webview` describes a page and checks every option exactly as on Windows, so a mistake in the description raises on a Mac too;
- every `show` and `prepare` answers `false, "not supported on this platform yet"`, `page:focus()` and `page:isShown()` answer `false`, `page.window` is `nil`, and `page:post` drops what it is given;
- no `onReady`, `onMessage` or `onClose` ever comes.

The web view library the host uses activates the application every time it creates a web view on a Mac, which would take the front from the DAW at the wrong moment; its fix is not released yet. What each kind of page is to be on a Mac is decided — a tool page a panel that takes the keyboard without making the application the active one, an app page an ordinary window that does — but nothing about how VoiceOver will follow either is measured: those are the questions M1–M9 in TODO.md. **Nothing on this page promises how a page will behave on a Mac.**

What you can do on a Mac now:

- **Write the module and the page.** Everything above is written once. The description is the same on both systems, the page's HTML and script are the same, and a module that answers `show`'s reasons — as the example does — says "not supported on this platform yet" on a Mac instead of failing.
- **Mind the module's own `supported_os`.** The example says `["windows"]`, so a Mac does not load it at all (the log says why); leave it out, or name both systems, for a module whose page should simply wait for the Mac's pages.
- **Pick a key a Mac can press.** A key spec's `Ctrl` is Command on a Mac and `Win` is Control, so the four modifiers hold Control and Option there — VoiceOver's layer. Choose another chord on a Mac with [`host.os.pick`](api/os.md#host-os-pick), as the application does for its own keys: `host.os.pick({ windows = "Ctrl+Shift+Win+Alt+F12", macos = "Cmd+Shift+F12" })` — and check it with [`host.keys.check`](api/keys.md#host-keys-check). An F-key on a Mac needs "Use F1, F2, etc. as standard function keys" turned on, or `fn` held down.
- **Look at the page in a browser.** Open `ui/index.html` in Safari to work on its structure with VoiceOver ([Step 3](#step-3)). It is a check of your HTML, not of the platform: WebKit with VoiceOver is not Chromium with NVDA, and which elements work there in a page is unverified.
- **Coordinates are points on a Mac**, and the round trip's calls — `host.window.focus`, `ownsPoint`, `host.input.click`, `host.ocr.recognize` — exist there as they do on Windows, each with its own platform section in the reference.

---

## Step 9 — when it does not work {#step-9}

**Say every answer.** A `show` that answers `false` says why in its second value; the example speaks it. Most failures are one of those words.

**The log**, under `[gui]`, names the page as `[<module id>] page '<title>'` and says each step: `described, from ui/index.html …`, `prepared — …`, `building in WebView2's environment …`, `built in N ms`, `ready, N ms after the show or prepare that asked for it`, and for each show where it went and what is in front — `shown and brought to the front (accepted) …, beside x,y wxh; in front now: …; on top at its centre: yes` — or `shown, and Windows DECLINED to bring it to the front`, or `show answered false, "…"`. A build that failed says why there (`could not be built …`).

**Nothing arrives from the page.** The page's script did not run, or failed. The host's [content policy](api/gui.md#what-a-page-is-served-and-what-it-may-not-do) allows scripts only from the module's own files: an inline `<script>`, an `onclick="…"` attribute, `eval` and `new Function` do not run — they do in a browser, which is why a page that works there can do nothing in the host. A script in a file that is not served (a typo in `src`, a file outside the module's folder) is a 404. The page's console and the developer tools are off, so a page that needs to know reports its own trouble to the module. The example's `page.js` begins its `if (window.platform)` block with two listeners — from there on: a script that fails to load or to parse reports nothing, that one included:

```js
window.addEventListener("error", (event) => {
  window.platform.post({ kind: "problem", text: `${event.message} (${event.filename}:${event.lineno})` });
});
document.addEventListener("securitypolicyviolation", (event) => {
  window.platform.post({ kind: "problem", text: `the policy refused ${event.blockedURI || "inline code"} (${event.effectiveDirective})` });
});
```

— and its `onPageMessage`, as it stands at the end, logs what they send, before it asks whether the page is in front, since a problem is worth a line in the log wherever the user is:

```luau
local function onPageMessage(m)
  if type(m) ~= "table" then
    return
  end
  if m.kind == "problem" then
    host.log.info("Page demo: the page reports: " .. tostring(m.text))
    return
  end
  if not inFront(page.window) then
    return -- pressed before the user went elsewhere: not acted on
  end
  if m.action == "back" then
    backToTarget()
  elseif POINTS[m.action] then
    clickAndRead(POINTS[m.action])
  end
end
```

**The page reaches nothing outside its folder.** Its requests get the module's own files and nothing else — `fetch` of a JSON file of the module's works, anything on the internet is refused; links to other places, new windows, downloads and the browser's permission prompts are refused too; `alert`, `confirm` and `prompt` show nothing. Two ways out are not closed yet (WebRTC, and the browser's look-ahead to a link's host), which is why the install review says a page is not kept off the internet entirely. A page keeps nothing between sessions: what it must remember goes to the module in a message, and from there into [`host.settings`](api/settings.md#host-settings-define).

**An error in your callback** is logged and shown once in a dialog, naming the `page message`, `page ready` or `page close` callback; one that runs past the [time limits](module-runtime-and-lifecycle.md#limits) stops the module, as any callback does.

**The round trip stops halfway.** The example says each place it can stop: Notepad would not come to the front (`host.window.focus` answered `false`), another window came to the front before the click or while the line was read, something covers the point, or the page was not brought back — with Windows' reason and the result, which is in the log as well (`Page demo: the page did not come back …`).

---

## A checklist

- Describe the page once, at load; keep the object. Show it from `onReady` when a `show` answered `"not ready yet"` — and only if the user is still where they asked for it.
- Prepare it when its window first comes to the front, not on the key the user waits on — and not again from there after a prepare failed.
- Every action a real button, handled on `click`; text into the page with `textContent`; scripts in files of the module's own, never inline.
- Results in a live region that is in the HTML from the start, for what the page is told while it is in front; a round trip's result posted just before the `show` that brings the page back, with the keyboard focus put on it, and said with `host.speech` only when the page does not come.
- No keys of the page's own, no `role="application"`, nothing that catches Tab or Escape, no focus moved by a script but onto a round trip's result.
- `avoid = w.bounds` on every `show` beside a window.
- Every action in the window is a round trip: the window forward first, the act on its foreground event once it is still in front, the point from its origin, `ownsPoint` before the click, a read with a callback, the page back in its answer.
- One round trip at a time; a message that reaches the module while the page is not in front is not acted on, whichever button sent it; another window coming to the front at any step gives the trip up, and the page is not brought back over it.
- Back to the window: the window forward first, the page hidden after.
- Say every `false` that `show` and `host.window.focus` answer; the user cannot see that the page did not come. A declined `show` is Windows keeping the keyboard, not a page that is not there.
- When the user has just moved, speak with `interrupt = false`, behind the screen reader.
- Windows only today. Write the page once, and pick a Mac key with `host.os.pick` for the day the Mac's pages arrive.
