-- Overlay runtime — the code module `com.platform.overlay`. A self-voicing
-- virtual control tree layered over the primitives (speech / input / ocr /
-- hotkey); the ReaHotkey "overlay" concept in our system. A module that needs it
-- declares a dependency and gets O via host.require — the trailing `return O` is
-- what that captures. See docs/reahotkey-port-analysis.md.
--
-- Controls: StaticText / HotspotButton / CustomButton / OCRButton / gtoggle;
-- keyboard navigation; speech composition "label, type[, value]".
--
-- NOTE: this assumes window_prelude has run in the same VM — host.window.test /
-- onTrigger / onFocus are window_prelude helpers, not native primitives. The host
-- still injects window_prelude into every VM, so it always holds; if that ever
-- changes, the overlay must depend on a window module instead.

local O = {}

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
      -- Optional gate fn(origin) -> bool: an extra activation condition ANDed on
      -- top of the context match (e.g. a library's landmark image on screen).
      _gate = false,
      -- Optional module hook fn(overlay) fired each time the overlay becomes the
      -- active one (e.g. Komplete Kontrol auto-closes its library browser).
      _onActivate = false,
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

-- An extra activation condition ANDed on top of the context match: `fn(origin)`
-- (origin = the active context's coordinate window/control) returns whether this
-- overlay should be active. A derived/library overlay uses it to gate on its own,
-- more-specific landmark while sharing the base's window context.
function Overlay:gate(fn)
  self._gate = fn
  return self
end

-- Convenience gate: active only while `image` is found within the active context's
-- region (a library's product wordmark inside the plugin). `image` must be an
-- ABSOLUTE path (host.path) — the imageSearch runs in the active context's identity
-- scope, so a relative path would resolve under the wrong module root.
function Overlay:landmark(image)
  -- The template match runs on a worker thread (host.screen.imageSearchAsync) so the
  -- detection poll never blocks the main thread on it. The gate returns the LAST
  -- known result and kicks off a refresh (at most one in flight); when the result
  -- lands, a state TRANSITION re-checks immediately (snappy activate/deactivate)
  -- while the steady state stays cheap. Sticky: once found, only a tiny region around
  -- the last hit is re-searched; a miss there falls back to the full region.
  local lastHit, matched, pending = nil, false, false
  return self:gate(function(origin)
    if not image or not origin or not origin.client or not origin.bounds then
      lastHit, matched = nil, false
      return false
    end
    if not pending then
      pending = true
      local region
      if lastHit then
        region = { lastHit.x - 6, lastHit.y - 6, lastHit.x + lastHit.w + 6, lastHit.y + lastHit.h + 6 }
      else
        region =
          { origin.client.x, origin.client.y, origin.client.x + origin.bounds.w, origin.client.y + origin.bounds.h }
      end
      host.screen.imageSearchAsync(image, { region = region }, function(hit)
        pending = false
        local was = matched
        lastHit = hit or nil
        matched = hit ~= nil
        if matched ~= was then self:_recheck() end
      end)
    end
    return matched
  end)
end

-- A named placeholder in the control tree: a base overlay reserves a slot, and a
-- derived/library overlay fills it (via :fill) so its controls land at THIS
-- position instead of appended at the end. An unfilled slot is skipped in nav.
function Overlay:addSlot(name)
  return add(self, { kind = "slot", name = name })
end

-- Fills the named slot with the controls `build` adds: the placeholder is replaced
-- in place by them, so a base's `header + slot + footer` becomes `header + the
-- child's controls + footer`. No-op if no such slot (e.g. already filled).
function Overlay:fill(name, build)
  local idx
  for i, c in ipairs(self.controls) do
    if c.kind == "slot" and c.name == name then
      idx = i
      break
    end
  end
  if not idx then return self end
  local tmp = O.new()
  if build then build(tmp) end
  table.remove(self.controls, idx)
  for j, c in ipairs(tmp.controls) do
    table.insert(self.controls, idx + j - 1, c)
  end
  return self
end

-- Overrides an inherited control: replaces the first control labelled `label` with
-- the (first) control `build` adds, leaving the rest of the tree intact.
function Overlay:replace(label, build)
  if label == nil then return self end -- never match a label-less control on nil == nil
  for i, c in ipairs(self.controls) do
    if c.kind ~= "slot" and c.label == label then
      local tmp = O.new()
      if build then build(tmp) end
      if tmp.controls[1] then self.controls[i] = tmp.controls[1] end
      return self
    end
  end
  return self
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
  -- All real controls are focus stops: static text is Tab-reachable and announced,
  -- it just has no activation (Enter does nothing on it). An UNFILLED slot is a
  -- structural placeholder, skipped in navigation (an empty slot = 0 nav entries). A
  -- control with a `when` predicate is skipped while it returns non-true (e.g. KK
  -- chrome controls, shown only while Kontakt is nested inside Komplete Kontrol).
  local c = self.controls[index]
  if c == nil or c.kind == "slot" then return false end
  if c.when and c.when(self) ~= true then return false end
  return true
end

function Overlay:focusNext()
  local n = #self.controls
  if n == 0 then return end
  local found = false
  for _ = 1, n do
    self.focus = (self.focus % n) + 1
    if isFocusable(self, self.focus) then
      found = true
      break
    end
  end
  -- Nothing focusable (e.g. a control set that is only unfilled slots): don't
  -- announce a phantom empty stop with interrupt.
  if found then speakControl(self, self.focus, true) end
end

function Overlay:focusPrev()
  local n = #self.controls
  if n == 0 then return end
  local found = false
  for _ = 1, n do
    self.focus = ((self.focus - 2) % n) + 1
    if isFocusable(self, self.focus) then
      found = true
      break
    end
  end
  if found then speakControl(self, self.focus, true) end
end

function Overlay:activate(index)
  local i = index or self.focus
  local c = self.controls[i]
  if not c then return end
  if c.when and c.when(self) ~= true then return end -- a hidden control's hotkey no-ops
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

-- A module hook fired (with the overlay) each time it becomes the ACTIVE overlay —
-- runs after the activation context is set, so origin()/pluginHwnd are available.
function Overlay:onActivate(fn)
  self._onActivate = fn
  return self
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
  if self._onActivate then self._onActivate(self) end
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
  -- An extra gate (e.g. a library landmark) further narrows the match, evaluated
  -- against the matched context's coordinate origin.
  if matched and self._gate then
    matched = self._gate(self.activeCtx.origin()) == true
    if not matched then self.activeCtx = false end
  end
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
-- Arbiter mode is fixed by the first join that carries a slot; an overlay competes
-- on exactly one slot. A later join with a different slot is a mistake (its context
-- would silently fold into the first slot's claim), so log it rather than rebind.
-- Shared by attach* (via ensureTrigger) and derive.
function Overlay:_join(slot, specificity, pollMs)
  if slot then
    if not self._slot then
      self._slot = slot
      self._specificity = specificity or 0
      self._claim = host.arbiter.register(
        self._slot,
        self._specificity,
        function() self:_activate() end,
        function() self:_deactivate() end
      )
      if pollMs then
        host.timer.every(pollMs, function() self:_recheck() end)
      end
    elseif slot ~= self._slot then
      host.log.info(
        "overlay '"
          .. tostring(self.label)
          .. "' already bound to arbiter slot '"
          .. self._slot
          .. "', ignoring '"
          .. slot
          .. "'"
      )
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

-- Sets the interaction flags from `opts`, then joins the arbiter + arms the trigger.
local function ensureTrigger(self, opts)
  if opts then
    self.naturalNav = opts.naturalNav == true
    self.hoverToRead = opts.hoverToRead == true
    self:_join(opts.slot, opts.specificity, opts.pollMatch)
  else
    self:_join(nil, nil, nil)
  end
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
  -- identify may be RELATIONAL — its verdict depends on sibling controls, not just
  -- this one (e.g. "this Kontakt is wrapped by a Komplete Kontrol, so defer to KK").
  -- Such a spec sets cacheIdentity = false so the per-HWND cache below (which assumes
  -- identify is a pure function of the single control) can't pin a stale verdict when
  -- the neighbourhood changes — e.g. a Kontakt loaded into an already-open KK, whose
  -- container HWND is stable, would otherwise keep its first (host-less) verdict and
  -- the overlay would never attach.
  local cacheId = spec.cacheIdentity ~= false
  local idCache = {}
  local function pluginControl()
    local active = host.window.active()
    if not active then return nil end
    local chrome = nil
    local hostOk = false
    for _, h in ipairs(hosts) do
      if host.window.test(h, active) then
        hostOk = true
        chrome = h.chrome
        break
      end
    end
    if not hostOk then return nil end
    local focusChain = host.window.focusChain()
    -- Focus gate (ReaHotkey-style): only treat the plugin as focused when the
    -- keyboard focus is NOT on one of the DAW's own chrome controls (its FX list,
    -- track list, transport, …), supplied per host as `chrome` patterns. Without
    -- it the overlay activates whenever the plugin control is merely a *child* of
    -- the focused host window — e.g. while browsing REAPER's FX list with Kontakt
    -- selected, before the plugin GUI itself is focused. focusChain()[1] is the
    -- focused control (GetGUIThreadInfo's hwndFocus).
    if chrome then
      local focused = focusChain[1]
      if focused then
        for _, pat in ipairs(chrome) do
          if string.match(focused.class, pat) then return nil end
        end
      end
    end
    -- Enumerate the active (plugin host) window's child controls — like
    -- ReaHotkey's WinGetControls — with the keyboard-focus chain as a fallback.
    -- The plugin control is often NOT on the focus chain (e.g. Kontakt's Qt window
    -- keeps the Win32 focus on the host window, with the Qt control a *child* — not
    -- an ancestor), so child enumeration is the reliable path.
    local candidates = host.window.controls()
    for _, c in ipairs(focusChain) do
      candidates[#candidates + 1] = c
    end
    for _, c in ipairs(candidates) do
      if string.match(c.class, pattern) then
        if not identify then return c end
        local ok
        if cacheId then
          ok = idCache[c.id]
          if ok == nil then
            ok = identify(c) == true
            idCache[c.id] = ok
            if host.log then
              host.log.info(string.format(
                "attachEmbedded: matched control [%s] id=%d, identify=%s", c.class, c.id, tostring(ok)
              ))
            end
          end
        else
          ok = identify(c) == true -- relational spec: re-evaluate every recheck
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

-- Derives a child overlay from this one: it inherits this overlay's controls (a
-- fresh list — `build(child)` then appends the child's own), its coordinate frame +
-- interaction flags, and its activation context, and competes on the SAME arbiter
-- slot one specificity higher (so the child wins over its parent when active). The
-- child sets its own, more-specific gate in `build` (e.g. child:landmark(image)).
-- This is how a sub-library composes onto a library — each level is itself a full
-- overlay, assembled by the arbiter (React-portal style). The parent must already
-- be attached (so its context + slot exist to inherit).
function Overlay:derive(name, build)
  local child = O.new(name or self.label)
  for _, c in ipairs(self.controls) do
    child.controls[#child.controls + 1] = c
  end
  for _, ctx in ipairs(self.contexts) do
    child.contexts[#child.contexts + 1] = ctx
  end
  child._originOffset = self._originOffset
  child.naturalNav = self.naturalNav
  child.hoverToRead = self.hoverToRead
  if build then build(child) end
  child:_join(self._slot or nil, (self._specificity or 0) + 1, 500)
  return child
end

-- Replaces the control set at runtime (e.g. a Kontakt base overlay vs a library's
-- header + controls), re-registering hotkeys and resetting focus to the first
-- focusable control.
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

-- Exposed as the code module com.platform.overlay: a module that depends on it
-- receives O via host.require (the trailing return is what that captures).
return O
