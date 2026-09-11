-- DAW host definitions: window matchers for the DAWs that embed VST/AU plugins, so plugin
-- modules do not each hardcode them. Imported with `host.require("com.platform.daw-hosts")`.
-- Criteria mirror ReaHotkey's.
--
-- A **code module**, and deliberately: it now carries behaviour as well as data — one global
-- command that puts the focus back into a plugin's window. A data module cannot do that,
-- because its table is round-tripped through JSON and functions do not survive. A code module
-- is evaluated inside every VM that depends on it, which is why the command lives in
-- `activate`: that runs in this module's OWN VM only, never in a dependent's copy, so the
-- hotkey is registered once rather than once per module that requires us.

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

-- The macOS block on the first entry is what lets a plugin be embedded in REAPER there at
-- all: a matcher with a platform block for one system and none for the other says "not
-- here" on the other, so without it every embedded binding's host test failed before the
-- platform's own control lookup was ever reached. REAPER's FX window on a Mac is a standard
-- window titled `FX: Track N "<plugin>"`; the chrome list is Win32 classes and is simply
-- inert there — the runtime asks the same question geometrically.
local reaper = {
  { app = { exe = { contains = "reaper" } }, windows = { class = { contains = "#32770" } },
    macos = { title = { contains = "FX: " }, axSubrole = "AXStandardWindow" },
    chrome = reaper_chrome },
  { app = { exe = { contains = "reaper" } }, windows = { class = { contains = "REAPERb32host" } }, chrome = reaper_chrome },
}
local ableton = {
  { app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "Vst3PlugWindow" } } },
  { app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "AbletonVstPlugClass" } } },
  { app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "#32770" } } },
}

-- `all` = every host, flattened (the common case for an embedded attach).
local all = {}
for _, h in ipairs(reaper) do all[#all + 1] = h end
for _, h in ipairs(ableton) do all[#all + 1] = h end

-- The window a plugin's UI is drawn in, as opposed to the DAW's main window.
--
-- Matched by TITLE rather than by class, because that is the one thing both platforms agree
-- on: REAPER names its plugin window `FX: Track 1 "sforzando"` on Windows and on macOS
-- alike. It is also the only thing that works on macOS at all — a probe of REAPER's FX window
-- there found 22 accessibility elements, all of them REAPER's own chrome, and nothing
-- whatsoever belonging to the plugin.
-- Where the plugin's own panel begins inside REAPER's FX window, as { dx, dy } relative to
-- the window's content origin.
--
-- Measured from a probe capture of a real session rather than guessed: the dark panel begins
-- at x=240 and y=52 of the content area. Only the vertical is authored, because the FX list
-- to the left can be dragged wider or narrower: the horizontal is taken at run time from the
-- one surface REAPER's accessibility tree does expose — that list — plus the nine-point gap
-- between it and the plugin, which the same capture shows. The authored 240 stands in when
-- the list cannot be read.
--
-- Here rather than in a plugin module, because two things need it: an overlay that has to
-- shift its coordinate frame to the plugin (sforzando embedded in REAPER), and the shortcut
-- below, which has to know where to click. One copy, one place to re-measure.
local REAPER_TOP_INSET = 52
local REAPER_LIST_GAP = 9
local REAPER_LEFT_FALLBACK = 240

local function reaperPluginOrigin(win)
  local dx = REAPER_LEFT_FALLBACK
  local surfaces = host.window.controls(win)
  local list = surfaces and surfaces[1]
  if list and list.bounds and win and win.client then
    dx = (list.bounds.x + list.bounds.w) - win.client.x + REAPER_LIST_GAP
  end
  return { dx, REAPER_TOP_INSET }
end

local plugin_windows = {
  { app = { exe = { contains = "reaper" } }, title = { contains = "FX: " }, pluginOrigin = reaperPluginOrigin },
  { app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "Vst3PlugWindow" } } },
}

-- The key that puts you back in the plugin window.
--
-- On Windows a screen-reader user has OSARA's F6 for this. macOS has nothing: VoiceOver
-- offers no command for it, and a plugin window opened inside a DAW is then genuinely hard to
-- return to — which is what the tester ran into. Same shape as the platform's reload key, one
-- function key along, because a global shortcut has to be one nothing else wants.
--
-- Not the same chord on both platforms. The four-modifier one carries Control and Option
-- together on a Mac, and that pair is VoiceOver's own modifier: on a Mac whose VoiceOver
-- modifier is the default Control-Option, the chord is taken by VoiceOver before any
-- application sees it and pressing it plays VoiceOver's error sound. The third Mac session
-- measured that — registered, never delivered — on a machine where the probe's Command-Shift
-- key arrived every time, so the Mac gets that shape instead.
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
-- The panel is the rectangle the plug-in-window rule derives; a rule without one, or a
-- caller that has none, gets the depth reading alone, which is right for sforzando.
local function whereIsTheKeyboard(w, panel)
  local chain = host.window.focusChain()
  if not chain or #chain == 0 then return "unknown" end
  local last = chain[#chain]
  if not last or last.id ~= w.id then return "unknown" end
  if #chain == 1 then return "inside" end
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

-- Memoised per window, for as long as the window exists: the surface walk behind
-- `reaperPluginOrigin` is an accessibility traversal of the whole FX window, and it runs on
-- the thread that carries the event tap. Once per window is affordable; once per press is
-- how a stall gets the tap switched off. The trade is that dragging the FX list divider is
-- not followed until the window is closed and reopened — the same trade sforzando's own
-- copy of this cache makes, for the same reason.
local originByWindow = {}
local function pluginOriginFor(matcher, w)
  local hit = originByWindow[w.id]
  if hit then return hit end
  local p = matcher.pluginOrigin(w)
  originByWindow[w.id] = p
  return p
end

-- Puts the focus in the first plugin window we can find, and says which one.
--
-- It says what actually happened rather than what was attempted. `host.window.focus` reports
-- whether the system accepted the change, and both platforms can decline - announcing "back
-- in the plugin" to somebody who cannot look at the screen, when the focus did not move, is
-- the failure this project treats as worse than doing nothing.
local function focusPluginWindow()
  for _, matcher in ipairs(plugin_windows) do
    local w = host.window.find(matcher)
    if w then
      local title = w.title or "the plugin window"
      if not host.window.focus(w.id) then
        host.speech.output("could not switch to " .. title)
        return
      end
      if not (host.os.is("macos") and matcher.pluginOrigin) then
        host.speech.output(title)
        return
      end
      -- The panel is wanted before the first reading below, not only for the click: a
      -- keyboard already inside Kontakt's search field is "inside" by geometry and would
      -- otherwise be called "chrome" by depth.
      local panelNow = panelRect(w, pluginOriginFor(matcher, w))

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
      local p = pluginOriginFor(matcher, w)
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
            host.speech.output(title .. ", on " .. (given.name ~= "" and given.name or "an unnamed control"))
          else
            host.speech.output("could not move the keyboard into " .. title)
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
  host.speech.output("could not find a plugin window")
end

return {
  reaper = reaper,
  ableton = ableton,
  all = all,
  pluginWindows = plugin_windows,
  reaperPluginOrigin = reaperPluginOrigin,

  -- Runs in this module's own VM only — never in the copy a dependent holds — so the
  -- shortcut exists once no matter how many plugin modules require this one.
  activate = function()
    host.hotkey.register(FOCUS_PLUGIN_WINDOW, focusPluginWindow)
    host.log.info("[daw-hosts] " .. FOCUS_PLUGIN_WINDOW .. " puts the focus back in a plugin window")
  end,
}
