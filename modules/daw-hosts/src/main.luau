-- DAW host definitions: one ENTRY per kind of plug-in window a DAW has, so that no plug-in module
-- names a DAW. Imported with `host.require("com.platform.daw-hosts")`; a plug-in module binds
-- with `hosts = daw.all` and is then recognised in every DAW listed here. Criteria mirror
-- ReaHotkey's for REAPER and Ableton; Logic Pro (macOS only) is our own, from a Mac probe, since
-- ReaHotkey is Windows-only.
--
-- ADDING A DAW IS ADDING ITS ENTRIES HERE, and nothing else (the maintainer's decision of
-- 2026-09-27). An entry is a window matcher (host.window.test ignores the other fields) plus what
-- the DAW, and only the DAW, knows about its plug-in windows:
--
--   app / title / windows / macos / linux   which windows are this DAW's plug-in windows
--   chrome        control classes (Lua patterns, one plain list, not OS-keyed) meaning the
--                 keyboard is on the DAW's own controls — its FX list, its buttons, and the
--                 plug-in window itself where the plug-in is a child of it — and not in the
--                 plug-in; the embedded binding's focus gate, tested against the focused
--                 control's class on every platform. Written in Win32 vocabulary it matches
--                 nothing on a Mac, where the runtime asks the same question geometrically.
--   pluginOrigin  `(window) -> { dx, dy }`, or an OS-keyed table of such functions: where the
--                 plug-in's view begins inside the window, from its content origin, the view
--                 reaching the window's right and bottom edges. Needed on a platform where the
--                 plug-in publishes no control of its own — macOS: the overlay runtime builds the
--                 plug-in's control from it (the "host panel"), and the focus key below knows
--                 where to ask for or click the keyboard into. On Windows every plug-in module
--                 finds its own child control, so no entry gives one there — and none should:
--                 a binding whose own class pattern finds nothing would take the panel instead.
--                 It runs with THIS module's identity and capabilities, in whichever VM calls it
--                 (a code module's code does), so what it calls is declared in daw-hosts'
--                 manifest. nil, or anything but two numbers, is "no origin this time": not kept.
--   focusTarget   optional, a matcher the focus key also requires of a window, where the entry's
--                 matcher is wider than a plug-in window (REAPER's takes every dialog of its
--                 process on Windows, as ReaHotkey's does, because a plug-in's own dialogs are
--                 such windows too).
--   daw           the DAW's name, for the focus key's log line.
--
-- Every entry of `all` is a plug-in window, and the focus key searches exactly those that can
-- match on the platform it runs on: there is no second list to keep in step.
--
-- A **code module**, and deliberately: it carries behaviour as well as data — the origin
-- functions and one global command that puts the focus back into a plugin's window. A data
-- module cannot do that, because its table is round-tripped through JSON and functions do not
-- survive. A code module is evaluated inside every VM that depends on it, which is why the
-- command lives in `activate`: that runs in this module's OWN VM only, never in a dependent's
-- copy, so the hotkey is registered once rather than once per module that requires us.

-- Control classes that mean "keyboard focus is in REAPER's own chrome (the FX list, track
-- list, transport, …), NOT the embedded plugin". A plugin overlay uses these to gate
-- activation, so it does not fire while the plugin is merely selected/visible but the user is
-- browsing the FX list. Lua patterns over the raw window class; mirrors ReaHotkey's
-- InPluginControl REAPER exclusion list. (Carried on the matcher as an extra field;
-- host.window.test ignores it.)
local reaper_chrome = {
  "^SysListView32", -- the FX list (and other list views)
  "^SysTreeView32",
  "^SysHeader32",
  "^Button$",
  "^ComboBox$",
  "^Edit$",
  "^Static$",
  "^REAPERknob",
  "^reaperPluginHostWrapProc",
  "^#32770$", -- the host (FX chain) dialog window itself
}

-- REAPER's plug-in windows, by title. Two kinds, the same on both platforms: the FX chain,
-- `FX: Track 1 "Kontakt 7"`, with REAPER's FX list down its left side; and a plug-in floated
-- out of it — which is where REAPER puts an instrument inserted on a new track — titled by
-- its format first: `VST3i: VPS Avenger (keilwerth Audio / Vengeance Sound) (34 out) - Track
-- 1 "VPS Avenger"`. The formats are VST, VSTi, VST3, VST3i, AU, AUi, CLAP, CLAPi, JS, LV2 and
-- LV2i, so one pattern takes both kinds: capitals and digits, an optional i, then ": ". It was
-- `contains "FX: "` alone until a Mac log of 2026-09-27 had the shortcut below pressed five
-- times in a REAPER whose only plug-in window was a floating VPS Avenger, answering "could not
-- find a plugin window" each time.
local REAPER_PLUGIN_TITLE = { pattern = "^%u[%u%d]*i?: " }

-- Where the plugin's own panel begins inside REAPER's FX window, as { dx, dy } relative to
-- the window's content origin.
--
-- Measured from a probe capture of a real session rather than guessed: the dark panel begins
-- at x=240 and y=52 of the content area. Only the vertical is authored, because the FX list
-- to the left can be dragged wider or narrower: the horizontal is taken at run time from the
-- one surface REAPER's accessibility tree does expose — that list — plus the nine-point gap
-- between it and the plugin, which the same capture shows. The authored 240 stands in when
-- none of the window's surfaces is the list; when the host could not read them at all, there is
-- no origin that time (below).
--
-- Here rather than in a plugin module, because it is REAPER's geometry and no plug-in's: the
-- overlay runtime builds every plug-in's control from it on a Mac, and the shortcut below has
-- to know where to click. One copy, one place to re-measure. Measured on a Mac, and given for
-- macOS only: on Windows each plug-in module finds its own child control.
local REAPER_TOP_INSET = 52
local REAPER_LIST_GAP = 9
local REAPER_LEFT_FALLBACK = 240

-- A FLOATING plug-in window has no FX list: the plug-in spans the whole width, under one row
-- of REAPER's own controls (the preset menu, Param, the outputs, UI, FX Active). Taken from
-- the probe of a floating VPS Avenger on the 2026-09-27 Mac log, not from a picture: that row's
-- elements span content y 1 to 22 (screen 61–82, content origin 60), and the same plug-in's
-- text read in Logic's window and in REAPER's puts its top within a few points of 22 — so 22,
-- with no horizontal inset. Unverified on a plug-in with a known layout; the panel the overlay
-- runtime derives from it is logged when a plug-in is recognised in it ("attachEmbedded:
-- [host-panel] panel of '…' id=… at 0,22 …").
local REAPER_FLOAT_TOP_INSET = 22

-- The FX list is picked by WHERE it is, not by where it comes in the walk: the leftmost surface
-- that begins left of where a plug-in begins by default and ends before the window's right edge
-- (a container spanning the whole window is not the list), the widest of those when several
-- begin at the same place. It was the first surface the walk returned until a review of
-- 2026-09-28 pointed out that a plug-in which publishes a group or scroll area of its own puts
-- it in this same window — the third session's Kontakt 7 published none, but nothing says every
-- plug-in is as quiet — and one that came first would have moved every coordinate of every
-- plug-in module by the difference. The list lies against the window's left edge, a plug-in's
-- surfaces right of it.
--
-- NO surface at all is not a list of the default width. REAPER publishes its list in every FX
-- chain window, so an empty answer is the host not having asked: the application in the busy
-- quarantine, which `host.window.controls` answers with nothing. Then nil — no origin this time,
-- which neither the overlay runtime nor the focus key keeps, so it is asked again at the next
-- evaluation — rather than the authored 240, which both would have kept for as long as the window
-- keeps its size, wrong for anybody whose list is not the default width. Surfaces of which none
-- is the list get the authored 240.
local function reaperPluginOrigin(win)
  if win and win.title and string.sub(win.title, 1, 4) ~= "FX: " then
    return { 0, REAPER_FLOAT_TOP_INSET }
  end
  if not (win and win.client) then return { REAPER_LEFT_FALLBACK, REAPER_TOP_INSET } end
  local surfaces = host.window.controls(win)
  if not surfaces or #surfaces == 0 then return nil end
  local right = win.client.x + win.client.w - REAPER_LIST_GAP
  local list = nil
  for _, s in ipairs(surfaces) do
    local b = s.bounds
    if b and b.w > 0 and b.x - win.client.x < REAPER_LEFT_FALLBACK and b.x + b.w <= right
      and (list == nil or b.x < list.x or (b.x == list.x and b.w > list.w)) then
      list = b
    end
  end
  local dx = REAPER_LEFT_FALLBACK
  if list then dx = (list.x + list.w) - win.client.x + REAPER_LIST_GAP end
  return { dx, REAPER_TOP_INSET }
end

-- Where the plug-in begins inside a Logic plug-in window: under Logic's header — the preset
-- menu row, then the row of Back/Forward, Compare, Copy/Paste and Undo/Redo — 2 points in
-- from the left, 88 down from the window's TOP EDGE. From the tester's probe of a VPS Avenger
-- in Logic and in a floating REAPER window (2026-09-27), not from a picture:
--   * Logic's header — its three segment groups — ends 84 below the window's top (screen y
--     114 in a window at y 30), so the plug-in starts just under it;
--   * the same Avenger text read in both windows (its "80%" zoom caption, content 24,41 in
--     REAPER, whose plug-in starts at 0,22, and 26,106 from the frame in Logic) puts Avenger's
--     corner 2 in and 87 down in Logic, give or take the point a recognised word's box moves;
--     the windows' widths agree (1398 against REAPER's 1394: a 2-point border each side).
--   * Roughly also a third-party VPS Avenger tool calibrated in Logic Pro 11: its Avenger
--     origin is 90 below the window's top and 0 in. That origin is where its own 50 %-zoom
--     click table starts, fitted to button centres, not a measured edge; its REAPER figures
--     are marked uncalibrated in the tool, so nothing here is derived from them.
-- Measured from the window's FRAME, not its content origin, on purpose: the host could not
-- derive the title bar's height for this window ("content == frame … the title-bar inset
-- could not be derived" in the probe — Logic draws its own close button, without the subrole
-- the derivation looks for), so today its content origin IS the frame, and a host that learns
-- the inset later would otherwise move every coordinate here by a title bar.
--
-- The height is read off the header when the window publishes it — the lowest bottom edge of
-- the surfaces that BEGIN above where the plug-in would and end within its top 150 points,
-- plus the 4-point gap — and 88 stands in when it does not (the header can be hidden with the
-- window's toolbar button; where the plug-in sits then is unmeasured). "Begin above", and not
-- merely "lie in the band", because a plug-in that publishes a surface of its own — a group or
-- a scroll area near its top; `host.window.controls` returns surfaces only, so a plug-in's
-- buttons (Kontakt 7's FILE) are never among them — puts it in the same window, and counted as
-- header it would move the origin down onto the plug-in's own top row.
--
-- The line below comes once per window and size in every VM that evaluates the window — each
-- plug-in module binding with `daw.all`, Kontakt's library modules included — identical each
-- time. Kept in all of them on purpose: this module's own VM evaluates no overlay, so a line
-- limited to it would come only when the focus key is pressed.
-- Unverified on a live Logic: TODO.md, "Logic Pro as a host".
local LOGIC_LEFT = 2
local LOGIC_TOP_FALLBACK = 88
local LOGIC_HEADER_GAP = 4
local LOGIC_HEADER_BAND = 150

local function logicPluginOrigin(win)
  local frame, client = win and win.bounds, win and win.client
  if not (frame and client) then return { LOGIC_LEFT, LOGIC_TOP_FALLBACK } end
  local top, from = frame.y + LOGIC_TOP_FALLBACK, "the authored 88"
  local bottom = nil
  for _, s in ipairs(host.window.controls(win) or {}) do
    local b = s.bounds
    if b and b.y >= frame.y and b.y + LOGIC_HEADER_GAP < frame.y + LOGIC_TOP_FALLBACK
      and b.y + b.h < frame.y + LOGIC_HEADER_BAND then
      local e = b.y + b.h
      if not bottom or e > bottom then bottom = e end
    end
  end
  if bottom then top, from = bottom + LOGIC_HEADER_GAP, "its header's bottom edge" end
  local p = { frame.x + LOGIC_LEFT - client.x, top - client.y }
  host.log.info(("[daw-hosts] Logic plug-in window '%s': the plug-in starts at %d,%d from the content origin (from %s)"):format(
    tostring(win.title), p[1], p[2], from))
  return p
end

-- ── The entries ──────────────────────────────────────────────────────────────
-- REAPER. The macOS block on the first entry is what lets a plugin be embedded in REAPER there
-- at all: a matcher with a platform block for one system and none for the other says "not
-- here" on the other, so without it every embedded binding's host test failed before the
-- platform's own control lookup was ever reached. REAPER's plug-in windows on a Mac are
-- standard windows with the titles above (its main window is `AXUnknown`, and none of them);
-- the chrome list is Win32 classes and is simply inert there — the runtime asks the same
-- question geometrically.
--
-- On Windows the entry takes every `#32770` of REAPER's process, as ReaHotkey's criteria do,
-- and not only the titled plug-in windows: Kontakt's own "Content Missing" dialog is one of
-- them (a `#32770` owned by reaper.exe), and its overlay finds it there by the dialog's own
-- control. So the focus key, which has to FIND a plug-in window rather than test one, narrows
-- the entry by the title (`focusTarget`): the key must not go to REAPER's Preferences.
local reaper = {
  { daw = "REAPER",
    app = { exe = { contains = "reaper" } }, windows = { class = { contains = "#32770" } },
    macos = { title = REAPER_PLUGIN_TITLE, axSubrole = "AXStandardWindow" },
    chrome = reaper_chrome,
    pluginOrigin = { macos = reaperPluginOrigin },
    focusTarget = { title = REAPER_PLUGIN_TITLE } },
  { daw = "REAPER",
    app = { exe = { contains = "reaper" } }, windows = { class = { contains = "REAPERb32host" } },
    chrome = reaper_chrome },
}
-- Ableton Live, Windows only: ReaHotkey's three plug-in window classes. `#32770` last, so the
-- focus key finds a window of a plug-in's own class before any dialog.
local ableton = {
  { daw = "Ableton Live", app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "Vst3PlugWindow" } } },
  { daw = "Ableton Live", app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "AbletonVstPlugClass" } } },
  { daw = "Ableton Live", app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "#32770" } } },
}

-- Logic Pro, macOS only. From the probe of a VPS Avenger window in a tester's Mac log of
-- 2026-09-27: a plug-in window is its own window, `AXWindow/AXDialog/`, of the application
-- `com.apple.logic10`, TITLED BY THE CHANNEL STRIP ("Inst 1"), not by the plug-in — so a
-- plug-in module cannot tell its own window from the title here and must look inside it, which
-- is what every module's `identify` does on the panel the runtime builds from
-- `logicPluginOrigin`. The non-blank title keeps Logic's untitled dialogs out (an alert is also
-- AXDialog); a titled dialog of Logic's own that is not a plug-in window is not ruled out by
-- anything measured yet, which matters to the focus key only when both are open (it takes the
-- first it finds), and to an embedded overlay only as a cost: it asks its own identity question
-- there, once for each time the keyboard comes in.
local logic = {
  { daw = "Logic Pro",
    macos = { app = { bundleId = "com.apple.logic10" }, axSubrole = "AXDialog", title = { pattern = "%S" } },
    pluginOrigin = { macos = logicPluginOrigin } },
}

-- `all` = every entry, flattened: what a plug-in module binds with, and what the focus key
-- searches. A new DAW's list is appended here as well.
local all = {}
for _, h in ipairs(reaper) do all[#all + 1] = h end
for _, h in ipairs(ableton) do all[#all + 1] = h end
for _, h in ipairs(logic) do all[#all + 1] = h end

-- The key that puts you back in the plugin window.
--
-- On Windows a screen-reader user has OSARA's F6 for this. macOS has nothing: VoiceOver
-- offers no command for it, and a plugin window opened inside a DAW is then genuinely hard to
-- return to — which is what the tester ran into. Same shape as the platform's reload key, one
-- function key along, because a global shortcut has to be one nothing else wants.
--
-- Not the same chord on both platforms. The four-modifier one carries Control and Option
-- together on a Mac (its Win and Alt are Control and Option there), and that pair is
-- VoiceOver's own modifier: on a Mac whose VoiceOver modifier is the default Control-Option,
-- the chord is taken by VoiceOver before any application sees it and pressing it plays
-- VoiceOver's error sound. The third Mac session measured that — registered, never
-- delivered — on a machine where the probe's Command-Shift key arrived every time, so the Mac
-- gets that shape instead. `Cmd` is the Ctrl role: Command-Shift-F6.
local FOCUS_PLUGIN_WINDOW = host.os.pick {
  windows = "Ctrl+Shift+Win+Alt+F6",
  macos = "Cmd+Shift+F6",
  linux = "Ctrl+Shift+Win+Alt+F6",
}

-- Where the keyboard is, relative to the window we just asked for, as one of three words.
--
-- Read from the focus chain, and read CAREFULLY, because two easy readings of it are wrong:
--   * An EMPTY chain is not "the keyboard is in the window itself". It is the backend's
--     word for "could not read where the keyboard is" - the application did not answer
--     within its timeout, or is in the busy quarantine - and calling that success would
--     announce a window the keyboard never reached. (It once came back as [window] in that
--     case too, indistinguishable from a real answer; the backend was changed so that it
--     does not.)
--   * A chain that does not END in our window belongs to some other application. Activation
--     is asynchronous, so the read can describe whatever was in front when the key was
--     pressed. Nothing about that chain says anything about our window, and clicking on the
--     strength of it would put a click into that other application.
-- So: "unknown" for both of those; "inside" when the chain is exactly the window, OR when
-- the focused element's centre lies inside the plugin's panel; "chrome" when it is our
-- window with something OUTSIDE the panel under it. The second "inside" is the Kontakt case,
-- met on the third Mac session: a plug-in that exposes its controls puts the keyboard
-- several elements deep inside itself, and by depth alone that read as "chrome" — the
-- shortcut would have clicked at its corner and then reported a failure that was not one.
-- The panel is the rectangle the entry's origin derives; a caller that has none gets the depth
-- reading alone, which is right for sforzando. `panel` may also be a function that makes it,
-- called only when the chain is readable and deeper than the window — so the origin, which
-- walks the window's accessibility tree, is not asked for on a chain that says nothing.
local function whereIsTheKeyboard(w, panel)
  local chain = host.window.focusChain()
  if not chain or #chain == 0 then return "unknown" end
  local last = chain[#chain]
  if not last or last.id ~= w.id then return "unknown" end
  if #chain == 1 then return "inside" end
  if type(panel) == "function" then panel = panel() end
  local focused = chain[1]
  if panel and focused and focused.bounds then
    local cx = focused.bounds.x + focused.bounds.w / 2
    local cy = focused.bounds.y + focused.bounds.h / 2
    if cx >= panel.x and cx <= panel.x + panel.w and cy >= panel.y and cy <= panel.y + panel.h then
      return "inside"
    end
  end
  return "chrome"
end

-- The plug-in's panel as a screen rectangle: from the rule's origin to the window's edges,
-- because REAPER sizes the FX window to the plug-in (measured: 1023 wide for Kontakt 7 in
-- both its standalone window and REAPER's).
local function panelRect(w, p)
  local x, y = w.client.x + p[1], w.client.y + p[2]
  return { x = x, y = y, w = w.client.x + w.client.w - x, h = w.client.y + w.client.h - y }
end

-- Memoised per window and client size, as the overlay runtime keys its copy of this cache and
-- for the same reason: the surface walk behind `reaperPluginOrigin` is an accessibility
-- traversal of the whole FX window, and it runs on the thread that carries the event tap. Once
-- per window is affordable; once per press is how a stall gets the tap switched off. The trade
-- is that dragging the FX list divider is not followed until the window is resized, or closed
-- and opened again as a new window. The size is in the key because a DAW lays a window out
-- again when it is resized; it was the id alone until 2026-09-28, so a resized window kept the
-- origin of its old layout here while the overlays had moved on.
--
-- An origin that raises or answers anything but two numbers is not kept, and is said: the key
-- then has no panel to hand the keyboard to, and says so (see the key). Nor is REAPER's nil for
-- a window the host could not ask, so the next press asks again. At most ORIGIN_MEMO_CAP
-- windows are kept, then the memo starts again: the application runs for days, and one entry
-- per plug-in window ever opened would only grow.
local ORIGIN_MEMO_CAP = 64
local originByWindow, originsKept = {}, 0
local function pluginOriginFor(originOf, w)
  local key = tostring(w.id) .. "|" .. tostring(w.client.w) .. "x" .. tostring(w.client.h)
  local hit = originByWindow[key]
  if hit then return hit end
  local ok, p = pcall(originOf, w)
  if not (ok and type(p) == "table" and type(p[1]) == "number" and type(p[2]) == "number") then
    host.log.info(("[daw-hosts] the entry's pluginOrigin for '%s' (%dx%d) %s — no plug-in panel to hand the keyboard to"):format(
      tostring(w.title), w.client.w, w.client.h,
      ok and ("answered a " .. type(p) .. ", not { dx, dy }") or ("raised: " .. tostring(p))))
    return nil
  end
  if originsKept >= ORIGIN_MEMO_CAP then originByWindow, originsKept = {}, 0 end
  originByWindow[key] = { p[1], p[2] }
  originsKept += 1
  return originByWindow[key]
end

-- The first plug-in window of entry `entry`, or nil: a window its matcher takes, and its
-- `focusTarget` too where it has one (REAPER's, whose matcher takes every dialog of the process
-- on Windows).
local function pluginWindowOf(entry)
  if not entry.focusTarget then return host.window.find(entry) end
  for _, w in ipairs(host.window.findAll(entry)) do
    if host.window.test(entry.focusTarget, w) then return w end
  end
  return nil
end

-- The entries the key searches: those of `all` that can match on this platform, by the window
-- prelude's own rule — an entry with platform blocks and none for this system, or an `os` list
-- without it, matches nothing here. Worked out once, at load. Searching the rest found nothing
-- and cost something: on Windows, Logic's entry (a `macos` block, no application clause this
-- platform can use) had the prelude list EVERY window on each press that found no plug-in
-- window; on a Mac, the Windows-only bridged-REAPER entry listed REAPER's windows a second
-- time, at the full timeout, on the thread that carries the event tap. And the DAWs the log line
-- names are the ones that could have been found.
local PLATFORMS = { "windows", "macos", "linux" }
local function appliesHere(m)
  local named, here = false, false
  for _, p in ipairs(PLATFORMS) do
    if m[p] ~= nil then
      named = true
      if p == host.os.current then here = true end
    end
  end
  if named and not here then return false end
  if m.os ~= nil then
    for _, p in ipairs(m.os) do
      if p == host.os.current then return true end
    end
    return false
  end
  return true
end
local searched = {}
for _, h in ipairs(all) do
  if appliesHere(h) then searched[#searched + 1] = h end
end

-- The DAWs the key searches, for its log line: taken from the entries, so a DAW added there is
-- named here without anybody remembering to.
local function dawNames()
  local names, seen = {}, {}
  for _, entry in ipairs(searched) do
    local n = entry.daw or "?"
    if not seen[n] then
      seen[n] = true
      names[#names + 1] = n
    end
  end
  return table.concat(names, ", ")
end

-- Puts the focus in the first plugin window we can find, and says which one.
--
-- It searches `all`, the same entries every plug-in module binds with — one list, so a DAW
-- added for the overlays is a DAW this key finds, and the other way round — less the entries
-- that cannot match on this platform (`searched`, above). It kept a list of its own until
-- 2026-09-27, which knew fewer windows than the overlays did (no bridged REAPER window, one of
-- Ableton's three).
--
-- It says what actually happened rather than what was attempted. `host.window.focus` reports
-- whether the system accepted the change, and both platforms can decline - announcing "back
-- in the plugin" to somebody who cannot look at the screen, when the focus did not move, is
-- the failure this project treats as worse than doing nothing.
local function focusPluginWindow()
  for _, entry in ipairs(searched) do
    local w = pluginWindowOf(entry)
    if w then
      local title = w.title or "the plugin window"
      if not host.window.focus(w.id) then
        host.speech.output("could not switch to " .. title)
        return
      end
      -- The entry's origin on this platform: none on Windows, where the key has nothing to
      -- ask or click and OSARA's F6 does the rest.
      local originOf = host.os.pick(entry.pluginOrigin)
      if not (host.os.is("macos") and type(originOf) == "function") then
        host.speech.output(title)
        return
      end
      -- The panel is wanted for the first reading below, not only for the click: a keyboard
      -- already inside Kontakt's search field is "inside" by geometry and would otherwise be
      -- called "chrome" by depth. Asked for only when that reading needs it — a chain that is
      -- readable and deeper than the window — because on a chain that is empty the application
      -- is in the host's busy quarantine, when the origin's own walk answers nothing too.
      -- `false` once asked and not given.
      local p = nil
      local function panelNow()
        if p == nil then p = pluginOriginFor(originOf, w) or false end
        return p and panelRect(w, p) or nil
      end

      -- On macOS, a window that accepted the focus has not necessarily taken the keyboard.
      --
      -- The tester's session settled that, five presses out of five: raising the window,
      -- activating REAPER and setting the window's focused attribute were all ACCEPTED, and
      -- the focus chain read afterwards was three deep every time - REAPER's FX list, not
      -- the plugin. He reported that Tab then walked REAPER's own controls, and that one
      -- Shift+Tab got him in. The plugin's view exposes no accessibility element, so there is
      -- nothing to hand the keyboard to by asking; asking the window was as polite as it gets.
      --
      -- What a view does understand is a click. One click just inside the plugin's panel -
      -- three points in from its top-left corner, which in sforzando is margin next to a
      -- label; other plugins have not been looked at, and a control there would be pressed -
      -- gives REAPER a reason to move the keyboard. The click is only POSTED, though:
      -- `host.input.click` hands three events to the window server and returns, and REAPER
      -- processes them on its own time. Reading the chain in the same breath would race the
      -- click and report the state from before it, every time the read won. So the second
      -- reading waits a quarter of a second, on a timer, and what is announced is what THAT
      -- reading says. Never the title before it, and never a second click.
      local before = whereIsTheKeyboard(w, panelNow)
      if before == "inside" then
        -- Already inside, so there is nothing to move — but the overlay may never have been
        -- TOLD. A keyboard that arrived here by a click (the tester's VOCR route) or by
        -- Shift-Tab fires no notification this runtime hears, and an in-DAW overlay has no
        -- poll under it, so this press is the only free retry there is. The other two
        -- branches both recheck for exactly that reason; this one returned silently, which
        -- made it the one outcome that left no trace at all — and the outcome a tester
        -- reaches by doing the obvious thing after a click that did not seem to work.
        -- What it is NOT is a fresh start: the keyboard stays inside, so the overlay runtime's
        -- stay goes on, and a plug-in its `identify` could not tell yet is asked again while a
        -- verdict already given for this stay is kept (see the runtime's "the stay").
        host.log.info(
          ("[daw-hosts] the keyboard was already inside '%s'; nothing asked and nothing "
            .. "clicked, asking every overlay to look again"):format(title))
        host.window.recheck()
        host.speech.output(title)
        return
      end
      if before == "unknown" then
        -- No click: the chain either could not be read or belongs to another application,
        -- and a click sent on that basis lands wherever the keyboard actually is.
        host.log.info(("[daw-hosts] the focus chain after focusing '%s' was empty or belonged elsewhere; not clicking"):format(title))
        host.speech.output("could not tell whether the keyboard is in " .. title)
        return
      end
      -- On the DAW's own controls, and the entry gave no origin for this window (logged by
      -- pluginOriginFor): nothing to ask inside and nowhere to click. Said, not guessed — a
      -- click at an authored corner could press a control of the DAW's.
      if not p then
        host.log.info(("[daw-hosts] the keyboard is on the DAW's own controls in '%s' and there is no plug-in origin for it; not asking and not clicking"):format(title))
        host.speech.output("could not tell where the plugin is in " .. title)
        return
      end
      local panel = panelRect(w, p)
      -- ASK FIRST. A plug-in that publishes its elements — Kontakt 7 publishes its header
      -- buttons and a search field — can simply be handed the keyboard: `focusWithin` sets
      -- the focused attribute on the first element inside the panel that takes it, which is
      -- what a screen reader's own navigation does, and presses nothing. The click below is
      -- for the plug-in that publishes nothing (sforzando), where there is no element to
      -- ask; it used to be the only path, and at Kontakt's corner it would have pressed
      -- Kontakt's logo button.
      local given = host.element and host.element.focusWithin and host.element.focusWithin(w.id, panel)
      if given then
        host.timer.after(250, function()
          local after = whereIsTheKeyboard(w, panel)
          host.log.info(("[daw-hosts] keyboard was on the host's chrome after focusing '%s'; handed to '%s' by asking, it is now: %s"):format(
            title, tostring(given.name), after))
          if after == "inside" then
            host.window.recheck()
            -- The title alone, as the other two ways in say it. Naming the element as well
            -- ("…, on Play View") read like a view state to the tester, when it is Kontakt 8's
            -- button of that name; VoiceOver names the element itself, and an overlay that
            -- comes up names its own control — which is the one the keyboard then works.
            host.speech.output(title)
          elseif after == "chrome" then
            host.speech.output("could not move the keyboard into " .. title)
          else
            -- An empty chain, or another application's: nobody can say, and "could not move"
            -- would claim a reading nobody took. The same words as the click's path below.
            host.speech.output("could not tell whether the keyboard is in " .. title)
          end
        end)
        return
      end
      host.input.click(w.client.x + p[1] + 3, w.client.y + p[2] + 3)
      host.timer.after(250, function()
        local after = whereIsTheKeyboard(w, panel)
        host.log.info(("[daw-hosts] keyboard was on the host's chrome after focusing '%s'; nothing in its panel took focus by asking; after a click at content (%d,%d) it is: %s"):format(
          title, p[1] + 3, p[2] + 3, after))
        if after == "inside" then
          -- Ask every overlay to look again. A keyboard that moved into a view exposing no
          -- accessibility element fires no focus notification, so nothing else would tell
          -- the runtime that the world changed, and an overlay gating on being inside the
          -- plug-in would stay switched off until something unrelated triggered a recheck.
          host.window.recheck()
          host.speech.output(title)
        elseif after == "chrome" then
          host.speech.output("could not move the keyboard into " .. title)
        else
          host.speech.output("could not tell whether the keyboard is in " .. title)
        end
      end)
      return
    end
  end
  -- "could not find", not "none open": on macOS an application that did not answer a
  -- moment ago is left out of the listing for a few seconds, and its windows with it. The
  -- log names the application it did not ask.
  --
  -- And a line of our own, because the speech leaves none: the 2026-09-27 Mac log has twelve
  -- presses of this key that say "arrived" and nothing after — over a floating REAPER window
  -- this did not know yet, over Logic, over Komplete Kontrol on its own — and could not tell
  -- which of them found nothing and which did something unlogged.
  local front = host.window.active()
  host.log.info(("[daw-hosts] no plug-in window found (the plug-in windows of %s); in front: '%s' (%s)"):format(
    dawNames(), tostring(front and front.title), tostring(front and front.app and front.app.name)))
  host.speech.output("could not find a plugin window")
end

-- The entries, per DAW and all together. A plug-in module binds with `all`; the per-DAW lists
-- are for a tool that has to tell the DAWs apart, never for a plug-in module, which would then
-- be recognised in that DAW only. The origin functions are not exported: they belong to their
-- entries, and the overlay runtime reads them there (`pluginOrigin`).
return {
  reaper = reaper,
  ableton = ableton,
  logic = logic,
  all = all,

  -- Runs in this module's own VM only — never in the copy a dependent holds — so the
  -- shortcut exists once no matter how many plugin modules require this one.
  activate = function()
    host.hotkey.register(FOCUS_PLUGIN_WINDOW, focusPluginWindow)
    host.log.info("[daw-hosts] " .. host.keys.describe(FOCUS_PLUGIN_WINDOW, { style = "short" })
      .. " puts the focus back in a plugin window")
  end,
}
