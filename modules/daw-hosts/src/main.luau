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

local reaper = {
  { app = { exe = { contains = "reaper" } }, windows = { class = { contains = "#32770" } }, chrome = reaper_chrome },
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
local plugin_windows = {
  { app = { exe = { contains = "reaper" } }, title = { contains = "FX: " } },
  { app = { exe = { contains = "Ableton Live" } }, windows = { class = { contains = "Vst3PlugWindow" } } },
}

-- The key that puts you back in the plugin window.
--
-- On Windows a screen-reader user has OSARA's F6 for this. macOS has nothing: VoiceOver
-- offers no command for it, and a plugin window opened inside a DAW is then genuinely hard to
-- return to — which is what the tester ran into. Same shape as the platform's reload key, one
-- function key along, because a global shortcut has to be one nothing else wants.
local FOCUS_PLUGIN_WINDOW = "Ctrl+Shift+Win+Alt+F6"

-- Puts the focus in the first plugin window we can find, and says which one.
--
-- It says what actually happened rather than what was attempted. `host.window.focus` reports
-- whether the system accepted the change, and both platforms can decline — announcing "back
-- in the plugin" to somebody who cannot look at the screen, when the focus did not move, is
-- the failure this project treats as worse than doing nothing.
local function focusPluginWindow()
  for _, matcher in ipairs(plugin_windows) do
    local w = host.window.find(matcher)
    if w then
      if host.window.focus(w.id) then
        host.speech.output(w.title or "plugin window")
      else
        host.speech.output("could not switch to " .. (w.title or "the plugin window"))
      end
      return
    end
  end
  host.speech.output("no plugin window open")
end

return {
  reaper = reaper,
  ableton = ableton,
  all = all,
  pluginWindows = plugin_windows,

  -- Runs in this module's own VM only — never in the copy a dependent holds — so the
  -- shortcut exists once no matter how many plugin modules require this one.
  activate = function()
    host.hotkey.register(FOCUS_PLUGIN_WINDOW, focusPluginWindow)
    host.log.info("[daw-hosts] " .. FOCUS_PLUGIN_WINDOW .. " puts the focus back in a plugin window")
  end,
}
