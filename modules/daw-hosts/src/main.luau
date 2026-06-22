-- Library module (data only): window matchers for DAW host windows that embed
-- VST/AU plugins, so plugin modules don't each hardcode them. Import via
-- `host.require("com.platform.daw-hosts")`. Criteria mirror ReaHotkey's.

local reaper = {
  { app = { exe = { contains = "reaper" } }, windows = { class = { contains = "#32770" } } },
  { app = { exe = { contains = "reaper" } }, windows = { class = { contains = "REAPERb32host" } } },
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
