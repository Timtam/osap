-- host.overlay runtime: a self-voicing virtual control tree layered over the
-- primitives (speech / input / ocr / hotkey). This is the ReaHotkey "overlay"
-- concept in our system. See docs/reahotkey-port-analysis.md.
--
-- Lean first version: StaticText / HotspotButton / CustomButton / OCRButton,
-- keyboard navigation via non-conflicting hotkeys, speech composition
-- "label, type[, value]". Context-scoping + key suppression come later.
--
-- NOTE: this file serves a dual role — it is BOTH the overlay prelude the host
-- exec's into every module VM AND the code module com.platform.overlay that a
-- dependent eval's via host.require (the trailing `return O` is what that path
-- captures). It assumes window_prelude has already run in the same VM:
-- host.window.test / onTrigger / onFocus are window_prelude helpers, not native
-- primitives. The host injects window_prelude immediately before this file, so
-- it always holds today; when the overlay injection is removed (migration step
-- 4), window_prelude must stay injected (or become a module dependency) so
-- attach()/attachEmbedded() keep working.

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
      -- Arbiter participation (set when attached with a `slot`): the host elects
      -- which competing overlay is active by specificity, instead of this overlay
      -- activating itself directly. false until attached with a slot.
      _slot = false,
      _claim = false,
      _specificity = 0,
      -- Optional function(control) -> dx, dy shifting the coordinate origin (a
      -- container library uses it for the host version's content offset).
      _originOffset = false,
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

-- A toggle whose on/off state is read by image-matching its region against an
-- "on" and an "off" template (e.g. a Kontakt library's mix switch). Activating
-- clicks the region centre. Coordinates are origin-relative.
function Overlay:addGraphicalToggle(opts)
  return add(self, {
    kind = "gtoggle",
    label = opts.label,
    region = opts.region,
    onImage = opts.onImage,
    offImage = opts.offImage,
    hotkey = opts.hotkey,
  })
end

local TYPE_LABEL =
  { static = "", hotspot = "button", custom = "button", ocr = "button", gtoggle = "toggle button" }

-- Origin (top-left) of the overlay's coordinate space in screen pixels: the
-- client-area top-left of the active context's coordinate window — the plugin
-- window itself when standalone, or the embedded plugin's child control when
-- hosted in a DAW. Re-resolved per call so it tracks window moves. (0, 0) when
-- unattached. All control coordinates (OCR regions, hotspots) are relative to it.
local function originOf(self)
  if self.activeCtx then
    local o = self.activeCtx.origin()
    if o and o.client then
      -- An optional per-overlay offset shifts the whole coordinate frame: a
      -- container library overlay uses it for the host version's content shift
      -- (e.g. Kontakt 8 nudges content +29 px down vs Kontakt 7), resolved per
      -- active control, so authored coordinates adapt without the library knowing
      -- the host version.
      local dx, dy = 0, 0
      if self._originOffset then dx, dy = self._originOffset(o) end
      return o.client.x + dx, o.client.y + dy
    end
  end
  return 0, 0
end

local function ocrCenter(self, c)
  local ox, oy = originOf(self)
  return ox + math.floor((c.region[1] + c.region[3]) / 2),
    oy + math.floor((c.region[2] + c.region[4]) / 2)
end

-- Reads a graphical toggle's state by image-matching its region against the
-- on/off templates. Returns "on" / "off" / nil (unknown).
local function gtoggleState(self, c)
  if not c.region then return nil end
  local ox, oy = originOf(self)
  local region = { ox + c.region[1], oy + c.region[2], ox + c.region[3], oy + c.region[4] }
  if c.onImage and host.screen.imageSearch(c.onImage, { region = region }) then return "on" end
  if c.offImage and host.screen.imageSearch(c.offImage, { region = region }) then return "off" end
  return nil
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
  if c.kind == "gtoggle" then
    local st = gtoggleState(self, c)
    if st then parts[#parts + 1] = st end
  end
  host.speech.output(table.concat(parts, ", "), { interrupt = true })
end

local function isFocusable(self, index)
  -- All controls are focus stops: static text is Tab-reachable and announced, it
  -- just has no activation (Enter does nothing on it).
  return self.controls[index] ~= nil
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
    c.onActivate(self) -- the overlay, so a header control can reach the active context
  elseif c.kind == "ocr" and c.region then
    -- An OCR button is a Button: re-read its label, then click the centre of
    -- the region (it sits over a clickable element), like ReaHotkey.
    speakControl(self, i, false)
    local cx, cy = ocrCenter(self, c)
    host.input.click(cx, cy)
  elseif c.kind == "gtoggle" and c.region then
    local ox, oy = originOf(self)
    host.input.click(ox + math.floor((c.region[1] + c.region[3]) / 2),
      oy + math.floor((c.region[2] + c.region[4]) / 2))
    -- Re-read the new state once the UI redraws.
    if host.timer then host.timer.after(150, function() speakControl(self, i, false) end) end
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
  if self.naturalNav then
    host.keys.scope(false)
    host.keys.menuOpen(false)
  end
  self:_unregisterHotkeys()
end

-- Re-evaluates the overlay's contexts against the current state (active window +
-- focus chain): the first matching context wins and supplies the coordinate
-- origin. In arbiter mode (attached with a `slot`) the match is reported to the
-- host arbiter, which decides — by specificity, across every overlay on that slot
-- — whether this overlay activates; otherwise the overlay activates/deactivates
-- itself directly (the legacy single-overlay path).
function Overlay:_recheck()
  local matched = false
  for _, ctx in ipairs(self.contexts) do
    if ctx.match() then
      self.activeCtx = ctx
      matched = true
      break
    end
  end
  if not matched then self.activeCtx = false end
  if self._slot then
    host.arbiter.setMatching(self._slot, self._claim, matched)
  elseif matched then
    self:_activate()
  else
    self:_deactivate()
  end
end

-- Registers the single foreground/focus trigger (once) that drives _recheck
-- across all of the overlay's contexts. With `opts.slot` the overlay joins the
-- host arbiter for that slot at `opts.specificity` (a base overlay and the
-- overlays inheriting it MUST pass the same slot string, fixed on the first
-- attach); the arbiter then drives _activate/_deactivate. `opts.pollMatch` (ms)
-- additionally re-reports the match on a disable-safe recurring host timer, for
-- matches that change with no window event (a library landmark appearing inside
-- an already-focused plugin).
local function ensureTrigger(self, opts)
  if opts then
    self.naturalNav = opts.naturalNav == true
    self.hoverToRead = opts.hoverToRead == true
    -- Arbiter mode is fixed by the first attach that carries a slot; an overlay
    -- competes on exactly one slot. A later attach with a different slot is a
    -- mistake (its context would silently fold into the first slot's claim), so
    -- log it rather than rebind.
    if opts.slot then
      if not self._slot then
        self._slot = opts.slot
        self._specificity = opts.specificity or 0
        self._claim = host.arbiter.register(
          self._slot,
          self._specificity,
          function() self:_activate() end,
          function() self:_deactivate() end
        )
        if opts.pollMatch then
          host.timer.every(opts.pollMatch, function() self:_recheck() end)
        end
      elseif opts.slot ~= self._slot then
        host.log.info(
          "overlay '"
            .. tostring(self.label)
            .. "' already bound to arbiter slot '"
            .. self._slot
            .. "', ignoring '"
            .. opts.slot
            .. "'"
        )
      end
    end
  end
  if self.triggerSet then return end
  self.triggerSet = true
  -- Foreground changes AND focus changes (the latter catches focus moving into
  -- an embedded plugin without a foreground change).
  host.window.onTrigger({}, { on = "activate" }, function() self:_recheck() end)
  host.window.onFocus(function() self:_recheck() end)
  self:_recheck()
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
  -- Optional identity check: the control class often matches *any* plugin in a
  -- host (e.g. REAPER's "Plugin<ptr>"), so `spec.identify(control)` confirms it
  -- is really this plugin (via UIA / OCR landmark / image search — the module's
  -- choice). Cached per control HWND, since such a check is costly.
  local identify = spec.identify
  local idCache = {}
  local function pluginControl()
    local active = host.window.active()
    if not active then return nil end
    local hostOk = false
    for _, h in ipairs(hosts) do
      if host.window.test(h, active) then hostOk = true break end
    end
    if not hostOk then return nil end
    -- Enumerate the active (plugin host) window's child controls — like
    -- ReaHotkey's WinGetControls — with the keyboard-focus chain as a fallback.
    -- The plugin control is often NOT on the focus chain (e.g. Kontakt's Qt window
    -- keeps the Win32 focus on the host window, with the Qt control a *child* — not
    -- an ancestor), so child enumeration is the reliable path.
    local candidates = host.window.controls()
    for _, c in ipairs(host.window.focusChain()) do
      candidates[#candidates + 1] = c
    end
    for _, c in ipairs(candidates) do
      if string.match(c.class, pattern) then
        if not identify then return c end
        local ok = idCache[c.id]
        if ok == nil then
          ok = identify(c) == true
          idCache[c.id] = ok
          if host.log then
            host.log.info(string.format(
              "attachEmbedded: matched control [%s] id=%d, identify=%s", c.class, c.id, tostring(ok)
            ))
          end
        end
        if ok then return c end
      end
    end
    return nil
  end
  -- The match also requires spec.present(control) when given — a per-call check,
  -- NOT cached like identify, so a condition that changes while the same control
  -- stays focused (a library loading/unloading inside an embedded plugin) is
  -- re-evaluated each time. origin still resolves to the control either way.
  local function matched()
    local c = pluginControl()
    if not c then return false end
    if spec.present and not spec.present(c) then return false end
    return true
  end
  self.contexts[#self.contexts + 1] = {
    match = matched,
    origin = function() return pluginControl() end,
  }
  ensureTrigger(self, opts)
end

-- ===== Container overlays: a plugin that hosts swappable sub-overlays =========
-- (e.g. Kontakt hosting a sample library). The container attaches to the plugin,
-- then watches for libraries registered under a `contract` (via host.providers)
-- and mounts the matching one's controls after its own generic header. Library
-- overlays are plain DATA (no Lua crosses module VMs); the container interprets
-- them. See docs/nested-overlays-design.md.

-- Replaces the control set at runtime (when the detected library changes),
-- re-registering hotkeys and resetting focus to the first focusable control.
function Overlay:setControls(controls)
  if self.active then self:_unregisterHotkeys() end
  self.controls = controls
  self.focus = 0
  if self.active then
    self:_registerHotkeys()
    for i = 1, #self.controls do
      if isFocusable(self, i) then self.focus = i break end
    end
  end
end

-- Resolves which plugin variant (e.g. Kontakt 7 vs 8) is in front and sets the
-- container's coordinate offset accordingly, so a library's base coordinates
-- auto-adapt without the library knowing the version. Cached per control id.
function Overlay:_resolveVariant(ctrl)
  local off = self._variantCache[ctrl.id]
  if off == nil then
    off = self._variants[1].offset or { 0, 0 }
    for _, v in ipairs(self._variants) do
      if (not v.identify) or v.identify(ctrl) then
        off = v.offset or { 0, 0 }
        self._variantName = v.name
        break
      end
    end
    self._variantCache[ctrl.id] = off
  end
  self._offset = off
end

-- All library descriptors registered under this container's contract, flattened.
function Overlay:_libraries()
  local out = {}
  for _, p in ipairs(host.providers(self._contract)) do
    for _, lib in ipairs(p.libraries or {}) do
      out[#out + 1] = lib
    end
  end
  return out
end

-- Shifts a library control's authored (base) coordinates by the container's
-- per-host offset.
function Overlay:_applyOffset(c)
  local dx, dy = self._offset[1], self._offset[2]
  local out =
    { kind = c.kind, label = c.label, hotkey = c.hotkey, onImage = c.onImage, offImage = c.offImage }
  if c.at then out.at = { c.at[1] + dx, c.at[2] + dy } end
  if c.region then
    out.region = { c.region[1] + dx, c.region[2] + dy, c.region[3] + dx, c.region[4] + dy }
  end
  return out
end

-- Mounts a library (header first, then its offset-shifted controls) as the active
-- control set and announces it. nil = the NoLibrary resting state (header only).
function Overlay:_mountLibrary(lib)
  self._current = lib or false
  self._mounted = true
  local controls = {}
  for _, c in ipairs(self._header) do controls[#controls + 1] = c end
  if lib then
    for _, c in ipairs(lib.controls or {}) do controls[#controls + 1] = self:_applyOffset(c) end
  end
  self:setControls(controls)
  host.speech.output(self.label .. ", " .. (lib and lib.name or "no library"), { interrupt = true })
end

-- Detects the loaded library by image-matching each descriptor's landmark in the
-- plugin rect. Sticky: keeps the current one while its landmark still matches.
function Overlay:_detectLibrary()
  if not self.activeCtx then return end
  local ctrl = self.activeCtx.origin()
  if not ctrl or not ctrl.client or not ctrl.bounds then return end
  self:_resolveVariant(ctrl)
  local ox, oy = ctrl.client.x, ctrl.client.y
  local region = { ox, oy, ox + ctrl.bounds.w, oy + ctrl.bounds.h }
  -- A library descriptor comes from another (decentralised) module, so guard the
  -- image search: a bad path / error skips that library, never breaks detection.
  local function found(image)
    if not image then return false end
    local ok, res = pcall(host.screen.imageSearch, image, { region = region })
    return ok and res ~= nil
  end
  if self._current and found(self._current.image) then return end
  for _, lib in ipairs(self:_libraries()) do
    if found(lib.image) then
      if not self._current or self._current.name ~= lib.name then self:_mountLibrary(lib) end
      return
    end
  end
  -- Nothing matched → NoLibrary (header only). Mount it when switching away from
  -- a library, or on the first detection so the header always appears.
  if self._current or not self._mounted then self:_mountLibrary(nil) end
end

-- Recurring landmark poll (only while active), self-rescheduling.
function Overlay:_startLibraryPoll()
  local function tick()
    if self.active and self._contract then self:_detectLibrary() end
    host.timer.after(self._pollMs, tick)
  end
  host.timer.after(self._pollMs, tick)
end

-- While active, watch (via UIA, control type 50009 = Menu) for a menu open inside
-- the plugin and tell the hook to let captured nav keys pass through to it — a
-- plugin's own (Qt) menu is not a Win32 #32768 menu, so the hook can't see it.
function Overlay:_startMenuWatch()
  -- Disable-safe recurring poll (survives a disable/enable cycle, unlike a Lua
  -- self-rescheduling host.timer.after).
  host.timer.every(150, function()
    if self.active and self.naturalNav then
      local ctrl = self.activeCtx and self.activeCtx.origin()
      local open = ctrl ~= nil and ctrl.id ~= nil and host.uia.find(ctrl.id, "", 50009) == true
      host.keys.menuOpen(open)
    end
  end)
end

-- Creates a container overlay bound to an embedded plugin (`hosts`/`control`/
-- `identify`), watching libraries registered under `contract`. `offset` is the
-- per-host coordinate shift applied to a library's base coordinates; `header` is
-- the generic always-present control set.
function O.container(spec)
  local ov = O.new(spec.label or "Plugin")
  -- Variants let the parent (e.g. Kontakt) carry per-version parameters (offset,
  -- and more later): each has its own `identify`; the active one's offset is
  -- applied, so a library's coordinates auto-adapt without knowing the version.
  ov._variants = spec.variants
    or { { name = spec.label, offset = spec.offset or { 0, 0 }, identify = spec.identify } }
  ov._offset = ov._variants[1].offset or { 0, 0 }
  ov._variantCache = {}
  ov._contract = spec.contract
  ov._header = spec.header or {}
  ov._current = false
  ov._mounted = false
  ov._pollMs = spec.pollMs or 500
  local variants = ov._variants
  ov:attachEmbedded(
    {
      hosts = spec.hosts,
      host = spec.host,
      control = spec.control,
      identify = function(c)
        for _, v in ipairs(variants) do
          if (not v.identify) or v.identify(c) then return true end
        end
        return false
      end,
    },
    { naturalNav = spec.naturalNav, hoverToRead = spec.hoverToRead }
  )
  ov:_startLibraryPoll()
  ov:_startMenuWatch()
  return ov
end

-- Exposed as the code module com.platform.overlay: dependents receive O via
-- host.require. Also include_str'd by the host as the injected prelude, where
-- exec() discards this return value.
return O
