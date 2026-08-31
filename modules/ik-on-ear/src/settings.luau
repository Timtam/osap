-- The settings panel behind the gear.

local O = host.require("com.platform.overlay")
local app = host.include("src/app.luau")
local geo = host.include("src/geometry.luau")
local read = host.include("src/tree.luau")

local on_ear, ON_EAR_SLOT, TREE_SENTINEL = app.window, app.SLOT, app.SENTINEL
local scaled, screenPointOf = geo.scaled, geo.screenPointOf
local dumpFor = read.dumpFor

-- == The settings panel ======================================================
--
-- The gear opens another pane inside this same window — no new window, ninety-six elements
-- appearing where there were sixty-six — so it gets its own overlay on the shared slot, like
-- the two choosers. It is the most valuable panel in the application and the one a screen
-- reader had least chance with: a serial number, a firmware version, a battery health reading
-- and four device settings, none of which are reachable any other way.
--
-- Unlike a chooser it does NOT replace the window's tree: the main window's elements are still
-- there behind it. So the marker for "this is open" is the panel's own heading, and the
-- sentinel that tells silence apart from absence is the same one the choosers use.
--
-- The helpers below repeat the shape of the ones above with a different overlay behind them,
-- which is the duplication this module has now produced three times. It is on the list of
-- things to lift into the runtime; doing it here, in the last section written, would mean
-- rewriting the two that are tested and working.
local settings = O.new("ARC ON:EAR Settings")

local sTree = O.memoByEpoch(function() return dumpFor(settings) end)

local function sElement(name)
  for _, n in ipairs(sTree() or {}) do
    if n.name == name then return n end
  end
  return nil
end

-- The same rule as the main window's: measured from where the label STARTS, because these
-- rectangles track the widget rather than the ink and neighbouring boxes overlap.
local function sValueRightOf(labelName)
  local label = sElement(labelName)
  if not label then return nil end
  local lb = label.bounds
  local best, bestDx
  for _, n in ipairs(sTree() or {}) do
    local b = n.bounds
    local sameRow = (b.y + b.h / 2) >= lb.y and (b.y + b.h / 2) <= (lb.y + lb.h)
    local dx = b.x - lb.x
    if sameRow and dx > 0 and n.name ~= "" and n.name ~= labelName then
      if not bestDx or dx < bestDx then
        best, bestDx = n, dx
      end
    end
  end
  return best
end

local function sPoint(dx, dy)
  return screenPointOf(settings, dx, dy)
end

local settingsWasThere = nil
local settingsDark = false

settings:gate(function(origin)
  if not (origin and origin.id) then return false end
  if host.uia.locate(origin.id, "Global Settings", host.uia.type.Text) then
    settingsDark = false
    return true
  end
  if not settingsWasThere then return false end
  if host.uia.locate(origin.id, TREE_SENTINEL, host.uia.type.Text) then
    settingsDark = false
    return false
  end
  -- Nothing answers at all: the device name field has the caret, which is what silences this
  -- application's tree. Absence of an answer is not an answer.
  settingsDark = true
  return true
end)

settings:typingWhen(function() return settingsDark end)

-- Out first, as in the choosers, and for the same reason.
settings:addHotspotButton({ label = "Close", at = scaled(967, 949) })

-- The device's own name, which is the one thing here that can be typed.
settings:addCustomButton({
  label = "Device name",
  typeLabel = "edit",
  editable = true,
  text = function()
    local v = sValueRightOf("Device")
    return v and v.name or nil
  end,
  onActivate = function()
    local v = sValueRightOf("Device")
    if not v then
      host.speech.output("the name field is not where it should be", { interrupt = true })
      return
    end
    host.input.click(v.bounds.x + v.bounds.w // 2, v.bounds.y + v.bounds.h // 2)
  end,
})

-- What the device says about itself. Nothing here can be pressed, and none of it is reachable
-- any other way — a firmware version is the first thing anybody asks for when something is
-- wrong, and until now the answer was "ask somebody who can see the screen".
for _, row in ipairs({
  { label = "Serial", of = "Serial:" },
  { label = "Firmware version", of = "Firmware Version:" },
  { label = "Hardware version", of = "Hardware Version:" },
  { label = "Battery health", of = "Battery Health:" },
}) do
  settings:addStaticText({
    label = row.label,
    text = function()
      local v = sValueRightOf(row.of)
      return v and v.name or "not shown"
    end,
  })
end

-- The four device settings, each a menu. Declared as opening one, so the overlay hands Tab and
-- Return over the moment it is up — a menu it opened and then swallowed the keys for would be
-- worse than not opening it.
for _, combo in ipairs({
  { label = "Dim attenuation", of = "DIM Attenuation" },
  { label = "LED brightness", of = "LED Brightness" },
  { label = "Auto standby", of = "Auto - Standby" },
  { label = "User trim", of = "User Trim" },
}) do
  settings:addCustomButton({
    label = combo.label,
    typeLabel = "combo box",
    opensMenu = true,
    text = function()
      local v = sValueRightOf(combo.of)
      return v and v.name or nil
    end,
    onActivate = function()
      local v = sValueRightOf(combo.of)
      if not v then
        host.speech.output(combo.label .. " is not where it should be", { interrupt = true })
        return
      end
      host.input.click(v.bounds.x + v.bounds.w // 2, v.bounds.y + v.bounds.h // 2)
    end,
  })
end

-- Device management. Painted buttons with no elements behind them, measured from the capture:
-- one column at x 1335-1622, four rows 76 apart with their middles at 181, 257, 334 and 410.
--
-- The last two are not undoable, and they are offered plainly rather than hidden or hedged.
-- Deciding for somebody that they may not reach a button their sighted colleague can is the
-- other way to get this wrong, and the application asks before it does either of them. What
-- the labels must not do is understate what they are, so they say exactly what is written on
-- them.
for _, b in ipairs({
  { label = "Export configuration", y = 181 },
  { label = "Import configuration", y = 257 },
  { label = "Restore factory settings", y = 334 },
  { label = "Firmware recovery", y = 410 },
}) do
  settings:addHotspotButton({ label = b.label, at = scaled(1478, b.y) })
end

settings:attach(on_ear, {
  menus = true,
  slot = ON_EAR_SLOT,
  specificity = O.layer.dialog,
  pollMatch = 500,
})
host.log.info("[on-ear] Settings overlay ready.")

-- Navigation is Tab / Shift+Tab / Enter, captured while this window is in front, as
-- everywhere else in this platform.
--
-- On a SLOT, which it did not need until this window turned out to have chooser panels. They
-- are not separate windows — each browser is a pane drawn inside this one, at (270,105) and
-- 1380x799 of it — so every overlay here matches the same window and they would otherwise all
-- be active at once, with several Tab rings fighting over one key and the 31 controls of this
-- one pointing at things a panel now covers. Pressing "FN Mute" would have clicked whatever
-- tile happened to be at that point.
--
-- The arbiter settles it: same slot, and a chooser is a `dialog` where this is `content`, so
-- while one is up it owns the window and this overlay stands down — which is also exactly
-- what it means to the person using it.
