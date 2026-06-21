-- host.overlay runtime: a self-voicing virtual control tree layered over the
-- primitives (speech / input / ocr / hotkey). This is the ReaHotkey "overlay"
-- concept in our system. See docs/reahotkey-port-analysis.md.
--
-- Lean first version: StaticText / HotspotButton / CustomButton / OCRButton,
-- keyboard navigation via non-conflicting hotkeys, speech composition
-- "label, type[, value]". Context-scoping + key suppression come later.

local O = {}
host.overlay = O

-- Navigation hotkeys (non-conflicting on purpose; final UX will use Tab/arrows
-- with context-scoping + key suppression).
local NAV = { next = "Ctrl+Alt+Right", prev = "Ctrl+Alt+Left", activate = "Ctrl+Alt+Return" }

local Overlay = {}
Overlay.__index = Overlay

function O.new(label)
  return setmetatable(
    { label = label or "Overlay", controls = {}, focus = 0, registered = {}, active = false },
    Overlay
  )
end

local function add(self, control)
  self.controls[#self.controls + 1] = control
  return control
end

function Overlay:addStaticText(label)
  return add(self, { kind = "static", label = label })
end

function Overlay:addHotspotButton(opts)
  return add(self, { kind = "hotspot", label = opts.label, at = opts.at, hotkey = opts.hotkey })
end

function Overlay:addCustomButton(opts)
  return add(self, { kind = "custom", label = opts.label, onActivate = opts.onActivate, hotkey = opts.hotkey })
end

function Overlay:addOCRButton(opts)
  return add(self, { kind = "ocr", label = opts.label, region = opts.region, hotkey = opts.hotkey })
end

local TYPE_LABEL = { static = "", hotspot = "button", custom = "button", ocr = "button" }

local function speakControl(self, index)
  local c = self.controls[index]
  if not c then return end
  local parts = { c.label or "" }
  local tl = TYPE_LABEL[c.kind]
  if tl ~= "" then parts[#parts + 1] = tl end
  if c.kind == "ocr" and c.region then
    local r = host.ocr.recognize({ region = c.region })
    parts[#parts + 1] = (r.text ~= "" and r.text or "no text")
  end
  host.speech.output(table.concat(parts, ", "), { interrupt = true })
end

local function isFocusable(self, index)
  local c = self.controls[index]
  return c and c.kind ~= "static" -- skip static text when navigating
end

function Overlay:focusNext()
  local n = #self.controls
  if n == 0 then return end
  for _ = 1, n do
    self.focus = (self.focus % n) + 1
    if isFocusable(self, self.focus) then break end
  end
  speakControl(self, self.focus)
end

function Overlay:focusPrev()
  local n = #self.controls
  if n == 0 then return end
  for _ = 1, n do
    self.focus = ((self.focus - 2) % n) + 1
    if isFocusable(self, self.focus) then break end
  end
  speakControl(self, self.focus)
end

function Overlay:activate(index)
  local c = self.controls[index or self.focus]
  if not c then return end
  if c.kind == "hotspot" and c.at then
    host.input.click(c.at[1], c.at[2])
    host.speech.output(c.label .. ", activated", { interrupt = true })
  elseif c.kind == "custom" and c.onActivate then
    c.onActivate()
  elseif c.kind == "ocr" then
    speakControl(self, index or self.focus) -- re-read the region
  end
end

function Overlay:_registerHotkeys()
  self.registered = self.registered or {}
  for i, c in ipairs(self.controls) do
    if c.hotkey then
      local idx = i
      self.registered[#self.registered + 1] = host.hotkey.register(c.hotkey, function() self:activate(idx) end)
    end
  end
  self.registered[#self.registered + 1] = host.hotkey.register(NAV.next, function() self:focusNext() end)
  self.registered[#self.registered + 1] = host.hotkey.register(NAV.prev, function() self:focusPrev() end)
  self.registered[#self.registered + 1] = host.hotkey.register(NAV.activate, function() self:activate() end)
end

function Overlay:_unregisterHotkeys()
  for _, id in ipairs(self.registered or {}) do
    host.hotkey.unregister(id)
  end
  self.registered = {}
end

-- Always-on: register hotkeys immediately and announce.
function Overlay:show()
  self:_registerHotkeys()
  host.speech.output(
    self.label .. " overlay ready. " .. #self.controls ..
    " controls. Control alt left and right to navigate, control alt enter to activate.",
    { interrupt = true }
  )
end

function Overlay:_activate()
  if self.active then return end
  self.active = true
  self.focus = 0
  self:_registerHotkeys()
  host.speech.output(self.label .. " overlay active", { interrupt = true })
end

function Overlay:_deactivate()
  if not self.active then return end
  self.active = false
  self:_unregisterHotkeys()
  host.speech.output(self.label .. " overlay closed", { interrupt = true })
end

-- Context-bound: activate automatically while a window matching `matcher` is
-- focused, deactivate when leaving (ReaHotkey-style).
function Overlay:attach(matcher)
  self.matcher = matcher
  self.active = false
  host.window.onTrigger({}, { on = "activate" }, function(win)
    if host.window.test(matcher, win) then
      self:_activate()
    else
      self:_deactivate()
    end
  end)
  local cur = host.window.active()
  if cur and host.window.test(matcher, cur) then
    self:_activate()
  end
end
