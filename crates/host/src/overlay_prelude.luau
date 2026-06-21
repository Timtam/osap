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
    {
      label = label or "Overlay",
      controls = {},
      focus = 0,
      registered = {},
      capturedKeys = {},
      active = false,
      naturalNav = false,
      hoverToRead = false,
      contexts = {},
      activeCtx = false,
      triggerSet = false,
    },
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

-- Origin (top-left) of the overlay's coordinate space in screen pixels: the
-- client-area top-left of the active context's coordinate window — the plugin
-- window itself when standalone, or the embedded plugin's child control when
-- hosted in a DAW. Re-resolved per call so it tracks window moves. (0, 0) when
-- unattached. All control coordinates (OCR regions, hotspots) are relative to it.
local function originOf(self)
  if self.activeCtx then
    local o = self.activeCtx.origin()
    if o and o.client then return o.client.x, o.client.y end
  end
  return 0, 0
end

local function ocrCenter(self, c)
  local ox, oy = originOf(self)
  return ox + math.floor((c.region[1] + c.region[3]) / 2),
    oy + math.floor((c.region[2] + c.region[4]) / 2)
end

local function speakControl(self, index, moveMouse)
  local c = self.controls[index]
  if not c then return end
  local parts = { c.label or "" }
  local tl = TYPE_LABEL[c.kind]
  if tl ~= "" then parts[#parts + 1] = tl end
  if c.kind == "ocr" and c.region then
    local ox, oy = originOf(self)
    local region = { ox + c.region[1], oy + c.region[2], ox + c.region[3], oy + c.region[4] }
    if moveMouse and self.hoverToRead then
      -- Move the mouse onto the control on focus — only for overlays that opt in.
      -- Some plugin UIs reveal the value on hover; others (like sforzando) show a
      -- *different* value while hovered, so this is off by default.
      host.input.move(ox + math.floor((c.region[1] + c.region[3]) / 2),
        oy + math.floor((c.region[2] + c.region[4]) / 2))
    end
    local r = host.ocr.recognize({ region = region })
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
  speakControl(self, self.focus, true)
end

function Overlay:focusPrev()
  local n = #self.controls
  if n == 0 then return end
  for _ = 1, n do
    self.focus = ((self.focus - 2) % n) + 1
    if isFocusable(self, self.focus) then break end
  end
  speakControl(self, self.focus, true)
end

function Overlay:activate(index)
  local i = index or self.focus
  local c = self.controls[i]
  if not c then return end
  if c.kind == "hotspot" and c.at then
    local ox, oy = originOf(self)
    host.input.click(ox + c.at[1], oy + c.at[2])
    host.speech.output(c.label .. ", activated", { interrupt = true })
  elseif c.kind == "custom" and c.onActivate then
    c.onActivate()
  elseif c.kind == "ocr" and c.region then
    -- An OCR button is a Button: re-read its label, then click the centre of
    -- the region (it sits over a clickable element), like ReaHotkey.
    speakControl(self, i, false)
    local cx, cy = ocrCenter(self, c)
    host.input.click(cx, cy)
  end
end

function Overlay:_registerHotkeys()
  self.registered = self.registered or {}
  self.capturedKeys = self.capturedKeys or {}
  -- Per-control hotkeys (e.g. Alt+P) are always global hotkeys.
  for i, c in ipairs(self.controls) do
    if c.hotkey then
      local idx = i
      self.registered[#self.registered + 1] = host.hotkey.register(c.hotkey, function() self:activate(idx) end)
    end
  end
  if self.naturalNav then
    -- Capture (+ suppress) the normal navigation keys, ReaHotkey-style — only
    -- while the overlay is active (its window is focused). Modifier-exact, so
    -- Alt+Tab etc. pass through.
    local function cap(spec, fn)
      host.keys.capture(spec, fn)
      self.capturedKeys[#self.capturedKeys + 1] = spec
    end
    -- Tab moves *between* elements; arrow keys are left for value changes within
    -- a control (standard interaction scheme), so we don't capture them here.
    cap("Tab", function() self:focusNext() end)
    cap("Shift+Tab", function() self:focusPrev() end)
    cap("Return", function() self:activate() end)
  else
    self.registered[#self.registered + 1] = host.hotkey.register(NAV.next, function() self:focusNext() end)
    self.registered[#self.registered + 1] = host.hotkey.register(NAV.prev, function() self:focusPrev() end)
    self.registered[#self.registered + 1] = host.hotkey.register(NAV.activate, function() self:activate() end)
  end
end

function Overlay:_unregisterHotkeys()
  for _, id in ipairs(self.registered or {}) do
    host.hotkey.unregister(id)
  end
  self.registered = {}
  for _, spec in ipairs(self.capturedKeys or {}) do
    host.keys.release(spec)
  end
  self.capturedKeys = {}
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
  -- Remember the last-focused control across deactivate/reactivate, so re-entering
  -- the window (e.g. Alt+Tab out and back) resumes where you were — only pick the
  -- first focusable control on the very first activation.
  if self.focus == 0 then
    for i = 1, #self.controls do
      if isFocusable(self, i) then
        self.focus = i
        break
      end
    end
  end
  self:_registerHotkeys()
  if self.naturalNav then
    -- Scope key suppression to this (now-foreground) window: when a control opens
    -- a menu (another window comes forward), Tab/Enter pass to it natively, and
    -- resume here once the plugin window is focused again.
    host.keys.scope(true)
  end
  -- Announce the focused control shortly after the window gains focus, once the
  -- screen reader's window-title announcement has passed (so it isn't clobbered).
  if host.timer and self.focus > 0 then
    host.timer.after(350, function()
      if self.active then speakControl(self, self.focus, false) end
    end)
  end
end

function Overlay:_deactivate()
  if not self.active then return end
  self.active = false
  if self.naturalNav then host.keys.scope(false) end
  self:_unregisterHotkeys()
end

-- Registers the single foreground trigger (once) that drives activation across
-- all of the overlay's contexts: the first context that matches the focused
-- window wins and supplies the coordinate origin.
local function ensureTrigger(self, opts)
  if opts then
    self.naturalNav = opts.naturalNav == true
    self.hoverToRead = opts.hoverToRead == true
  end
  if self.triggerSet then return end
  self.triggerSet = true
  -- Re-evaluate the contexts from the *current* state (active window + focus
  -- chain): the first context that matches wins and supplies the origin.
  local function check()
    for _, ctx in ipairs(self.contexts) do
      if ctx.match() then
        self.activeCtx = ctx
        self:_activate()
        return
      end
    end
    self.activeCtx = false
    self:_deactivate()
  end
  -- Foreground changes AND focus changes (the latter catches focus moving into
  -- an embedded plugin without a foreground change).
  host.window.onTrigger({}, { on = "activate" }, function() check() end)
  host.window.onFocus(function() check() end)
  check()
end

-- Context-bound (standalone): active while a window matching `matcher` is
-- focused; coordinates are relative to that window's client area.
function Overlay:attach(matcher, opts)
  self.contexts[#self.contexts + 1] = {
    match = function()
      local a = host.window.active()
      return a ~= nil and host.window.test(matcher, a)
    end,
    origin = function() return host.window.active() end,
  }
  ensureTrigger(self, opts)
end

-- Context-bound (embedded): active while the keyboard focus is *inside* a plugin
-- control — a window whose class matches `spec.control` (a Luau pattern) on the
-- focus-ancestor chain — within a DAW host window (any of `spec.hosts`). Keying
-- on focus-in-plugin (not just host-foreground) means we don't capture the
-- host's own navigation keys. Coordinates are relative to that control's client
-- area, so the same regions work standalone and embedded.
function Overlay:attachEmbedded(spec, opts)
  local hosts = spec.hosts or { spec.host }
  local pattern = spec.control
  local function pluginControl()
    local active = host.window.active()
    if not active then return nil end
    local hostOk = false
    for _, h in ipairs(hosts) do
      if host.window.test(h, active) then hostOk = true break end
    end
    if not hostOk then return nil end
    for _, c in ipairs(host.window.focusChain()) do
      if string.match(c.class, pattern) then return c end
    end
    return nil
  end
  self.contexts[#self.contexts + 1] = {
    match = function() return pluginControl() ~= nil end,
    origin = function() return pluginControl() end,
  }
  ensureTrigger(self, opts)
end
