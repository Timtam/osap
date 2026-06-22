-- Library module (data only): window matchers for DAW host windows that embed
-- VST/AU plugins, so plugin modules don't each hardcode them. Import via
-- `host.require("com.platform.daw-hosts")`. Criteria mirror ReaHotkey's.

-- Control classes that mean "keyboard focus is in REAPER's own chrome (the FX
-- list, track list, transport, …), NOT the embedded plugin". A plugin overlay
-- uses these to gate activation, so it doesn't fire while the plugin is merely
-- selected/visible but the user is browsing the FX list. Lua patterns over the
-- raw window class; mirrors ReaHotkey's InPluginControl REAPER exclusion list.
-- (Carried on the matcher as an extra field; host.window.test ignores it.)
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

return { reaper = reaper, ableton = ableton, all = all }
