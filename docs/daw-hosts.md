---
title: "DAW hosts — adding a DAW"
sidebar_position: 4
---

# DAW hosts — adding a DAW

A plug-in lives inside a DAW, and what a module needs to know about the DAW is the same for every plug-in: which of its windows are plug-in windows, which of its controls are its own chrome rather than the plug-in, and — where the plug-in publishes no control of its own — where in the window the plug-in begins. That knowledge lives in one place, the code module `com.platform.daw-hosts` (`modules/daw-hosts/src/main.luau`), as one **entry** per kind of plug-in window. A plug-in module binds with `hosts = daw.all` and is then recognised in every DAW that has entries there.

**Adding a DAW is adding its entries**, and nothing else. No plug-in module names a DAW, a DAW's executable or bundle, its window titles, its chrome or its geometry, and none requires the window's title to name the plug-in: Logic titles a plug-in window by its channel strip ("Inst 1"), so a module that asked the title would never come up there.

```luau
-- In a plug-in module: every DAW daw-hosts knows, and no DAW named.
local daw = host.require("com.platform.daw-hosts")
ov:attachEmbedded({ hosts = daw.all, control = { windows = "^Plugin%x+$" }, identify = isMine })
```

`daw.all` is every entry, flattened. `daw.reaper`, `daw.ableton` and `daw.logic` are the same entries per DAW, for a tool that has to tell them apart; a plug-in module that bound with one of them would be recognised in that DAW only.

## The entry {#entry}

An entry is a [window matcher](api/window.md#matchers) — `host.window.test` reads its matcher keys and ignores the rest — with these fields beside it:

| Field | Platforms | What it says |
| --- | --- | --- |
| `app`, `title`, `windows`, `macos`, `linux`, `os`, `where` | all | Which windows are this DAW's plug-in windows. A platform block for one system and none for another makes the entry "not here" on the other. |
| `chrome` | every platform it matches on | One plain list of Lua patterns — **not** OS-keyed — over the class of the focused control (`host.window.focusChain()[1].class`). When one matches, the keyboard is on the DAW's own controls and every embedded overlay in that window stays out of the way, on every platform. Written in Win32 vocabulary it matches nothing on a Mac, where the class is `AXRole/AXSubrole/AXIdentifier` and the runtime asks the same question by geometry; a pattern in that vocabulary would apply there, and a table keyed by platform matches nothing anywhere. Optional. |
| `pluginOrigin` | the platforms it names | `(window) -> { dx, dy }`, or an OS-keyed table of such functions: where the plug-in's view begins inside `window`, in points from the window's content origin (`window.client`). The plug-in reaches the window's right and bottom edges. `nil`, or anything but two numbers, means "no origin this time". Needed wherever a plug-in publishes no control of its own — macOS: the overlay runtime builds each plug-in's control from it ([the DAW's plug-in panel](api/overlay.md#o-attachembedded-macos)), and the focus key below knows where to hand the keyboard to. |
| `focusTarget` | all | A matcher the focus key also requires of a window, for an entry whose matcher is wider than a plug-in window. Optional. |
| `daw` | all | The DAW's name, for the log lines. |

### Windows

The matcher, and `chrome`. The plug-in is a child control of the DAW's window with a window class of its own, and every plug-in module finds that control by the class — so no entry gives a `pluginOrigin` for Windows, and none should: with one, every binding whose own class pattern found nothing in that window would take the DAW's panel instead, at coordinates measured from the DAW's origin — Kontakt's relational cells, Kontakt's own "Content Missing" dialog overlay, and sforzando reading its wordmark there once per stay.

`chrome` lists the DAW's own control classes that can hold the keyboard in a plug-in window: its FX list, its buttons, edits and combo boxes, the wrapper it puts the plug-in in, and **the plug-in window's own class** where the plug-in is a child of it (REAPER's list ends in `^#32770$`, "the FX chain dialog itself"), so that the keyboard on the window rather than in the plug-in keeps the overlay out too. An entry with no `chrome` — Ableton's three today — leaves the overlay active whenever its control is found in such a window, wherever the keyboard is in it.

Whether the plug-in's class is the same in every DAW is the plug-in's business, but it is what makes a Windows entry need nothing more: `Plugin<hex>` is a class several vendors' plug-ins share — ReaHotkey matches sforzando, Engine 2 and Zampler by it, in REAPER and in Ableton — and this platform has used it in REAPER only.

### macOS

The matcher and a `pluginOrigin` for macOS. A plug-in inside a DAW publishes no control of its own there, as far as anything has been measured, so without an origin no embedded overlay comes up in that DAW. The bind-time line (`embedded binding is inactive on macos: …`) does not say so: it is decided over all of a binding's hosts, and `daw.all` has entries with origins. The runtime says it instead, once per entry and module, the first time a window of that entry is in front: `[overlay] '<title>' is a plug-in window of <daw>, whose daw-hosts entry gives no pluginOrigin on macos — no plug-in panel there, so no binding without a control of its own comes up in it`.

The matcher's width is a cost, not only a question of correctness. Every plug-in module asks its `identify` once per stay in every window the matcher takes — sforzando a text read off the event loop, Kontakt an accessibility search per module that inherits it — so a matcher wider than the DAW's plug-in windows makes every one of them pay in windows that hold no plug-in at all. `focusTarget` narrows only the focus key, not the overlays.

### The origin function

`pluginOrigin` is called by the overlay runtime once per window and client size in each module's VM that binds with the entry, and by the focus key once per window and client size; both keep at most 64 windows. It runs on the event loop and may walk the window's accessibility tree (`host.window.controls(window)`). Neither asks for it while the focus chain is empty — the application not answering, when that walk answers nothing too.

It runs with **daw-hosts' identity**: daw-hosts is a code module, and its code keeps its own identity and its own manifest's capabilities in whichever VM calls it (see [`host.require`](api/require.md)). So what an origin function calls — `ocr`, `screen` — is declared in `modules/daw-hosts/module.toml`, and adding it there changes the capability list daw-hosts shows on install.

An origin function that raises, or answers `nil` or anything but two numbers, gives that window no panel. The overlay runtime says so once per window and size (`[overlay] the DAW entry's pluginOrigin for '…' (id=…, <w>x<h>) raised: … — no plug-in panel there`, or `answered a nil, not { dx, dy }`) and asks again at the next evaluation; the focus key logs it on each press it needed the origin for (`[daw-hosts] the entry's pluginOrigin for '…' (<w>x<h>) …`), says "could not tell where the plugin is in" and the title, and clicks nothing. Neither keeps a failure. So an origin that could not look this time — REAPER's, when the host could not read the window's surfaces — answers `nil` rather than a fallback, which would be kept for as long as the window keeps its size.

Measure an origin rather than guess it: from the window's own accessibility elements where it publishes them (REAPER's FX list, Logic's header), and against a plug-in whose layout is known. To check one on a real machine, two log lines say where it came out: the runtime's `attachEmbedded: [host-panel] panel of '<title>' id=… at <dx>,<dy> <w>x<h> from its content origin, identify=… — '<overlay>'` once a module's `identify` has answered in the window, and Kontakt 7's `[kontakt] Kontakt 7 in '…' (…): FILE at … puts Kontakt's corner <dx>,<dy> from the DAW's plug-in origin, which is <ox>,<oy> from the content origin`, which measures the origin against a plug-in whose corner is known: a corner far from `0,0` is an origin that is off.

## The entries today {#entries}

| DAW | Windows | macOS |
| --- | --- | --- |
| REAPER | every `#32770` of the process (as ReaHotkey's criteria), and bridged plug-ins' `REAPERb32host` windows; `chrome` its list views, buttons, edits and wrapper | standard windows titled like a plug-in window — `FX: …` (the FX chain) or a format first, `VST3i: …` (a plug-in floated out of it); origin right of the FX list — the leftmost surface that begins left of 240 points and does not span the window — plus 9, and 52 down, in the chain; 22 down in a floating window |
| Ableton Live | `Vst3PlugWindow`, `AbletonVstPlugClass`, `#32770` of the process | — |
| Logic Pro | — | titled `AXDialog` windows of `com.apple.logic10`; origin 2 in and under Logic's header, read off the header when it publishes it, 88 down from the window's top when not |

REAPER's Windows entry takes every dialog of its process because a plug-in's own dialogs are such windows too (Kontakt's "Content Missing" is found there by its own control), so the focus key narrows it with `focusTarget = { title = <the plug-in window titles> }`.

## Example entries {#example}

Logic's, as daw-hosts has it: macOS only, found by bundle, subrole and a non-blank title, with an origin function of its own.

```luau
local logic = {
  { daw = "Logic Pro",
    macos = { app = { bundleId = "com.apple.logic10" }, axSubrole = "AXDialog", title = { pattern = "%S" } },
    pluginOrigin = { macos = logicPluginOrigin } },
}
```

A Windows-only entry is a matcher and its chrome, with no origin (the class and chrome here are placeholders, not a measured DAW):

```luau
local studio = {
  { daw = "Some DAW",
    app = { exe = { contains = "somedaw" } }, windows = { class = { exact = "SomeDawPluginFrame" } },
    chrome = { "^SomeDawList", "^Button$", "^SomeDawPluginFrame$" } },
}
```

A DAW on both systems is one entry with a block for each, its chrome in Win32 vocabulary and its origin for macOS alone — the shape of REAPER's:

```luau
local reaper = {
  { daw = "REAPER",
    app = { exe = { contains = "reaper" } }, windows = { class = { contains = "#32770" } },
    macos = { title = REAPER_PLUGIN_TITLE, axSubrole = "AXStandardWindow" },
    chrome = reaper_chrome,
    pluginOrigin = { macos = reaperPluginOrigin },
    focusTarget = { title = REAPER_PLUGIN_TITLE } },
}
```

A new DAW is one list like these, appended to `all` in the same file:

```luau
local all = {}
for _, h in ipairs(reaper) do all[#all + 1] = h end
for _, h in ipairs(ableton) do all[#all + 1] = h end
for _, h in ipairs(logic) do all[#all + 1] = h end
```

## The focus key {#focus-key}

daw-hosts also registers one global key that puts the keyboard back into a plug-in window: `Ctrl+Shift+Win+Alt+F6` on Windows, `Cmd+Shift+F6` on a Mac. It searches the entries of `all` that can match on the platform it runs on — the window prelude's rule: an entry with platform blocks and none for this system, or an `os` list without it, is left out — in order, and takes the first window an entry's matcher (and its `focusTarget`, where it has one) finds. They are the entries the overlays bind with, so a DAW added for the overlays is a DAW the key finds. It asks for the window to be brought forward ([`host.window.focus`](api/window.md#host-window-focus)); refused, it says "could not switch to" and the title.

Finding no window, it says "could not find a plugin window" and logs the DAWs it searched and the window in front: `[daw-hosts] no plug-in window found (the plug-in windows of REAPER, Ableton Live); in front: '…' (…)` on Windows, `REAPER, Logic Pro` on a Mac.

### Windows

The key says the window's title, and that is all: OSARA's F6 and the screen reader do the rest. Because it searches every entry, it also finds REAPER's bridged plug-in windows and Ableton's VST2 and `#32770` plug-in windows. Ableton's `#32770` entry takes every dialog of Ableton's process, so with no window of Ableton's two plug-in classes open the key can take one of Ableton's own dialogs — a file dialog, a message box — and say its title; the entry is kept because a plug-in's own dialogs are such windows, as ReaHotkey's criteria have it.

### macOS

With a `pluginOrigin` for the entry, the key reads where the keyboard is ([`focusChain`](api/window.md#host-window-focuschain)), and asks for the origin only when that reading needs it — a chain that ends in the window and is deeper than it.

- **Inside** the plug-in — the chain is the window alone, or the focused element's centre lies in the plug-in's panel: it asks every overlay to look again and says the title. That is one more evaluation for every overlay, not a fresh start: it undoes an `identify` that could not tell yet, not a verdict the overlay keeps for the stay.
- **Not readable** — the chain is empty, or ends in another application's window: it says "could not tell whether the keyboard is in" and the title, and clicks nothing.
- **On the DAW's own controls**, and the entry gives no origin for the window: it says "could not tell where the plugin is in" and the title, and clicks nothing.
- **On the DAW's own controls**, with an origin: it asks the first focusable element inside the panel to take the focus ([`host.element.focusWithin`](api/element.md#host-element-focuswithin)), and when none does, clicks three points in from the panel's corner. A quarter of a second later it reads again and says the title if the keyboard is inside, "could not move the keyboard into" and the title if it is on the DAW's controls, and "could not tell whether the keyboard is in" and the title if the chain was empty or another application's — after asking and after the click alike.

An entry with no `pluginOrigin` for the platform gets the Windows behaviour: the title, and nothing else.
