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

-- Image-match robustness. ReaHotkey matches templates exactly (variation 0); we add a
-- per-channel colour tolerance plus a spread of scale factors, so a template still
-- matches under minor render / colour-profile / DPI-scale differences instead of
-- failing outright (which forced per-version re-captures). 1.0 is tried first, so an
-- exact hit costs nothing extra and the other scales are only reached when it misses.
-- An image control may override these with its own `tolerance` / `scales`.
local MATCH_TOLERANCE = 20
local MATCH_SCALES = { 1.0, 0.9, 1.1, 0.8, 1.2, 1.25, 0.75, 1.5, 0.67, 2.0 }

local Overlay = {}
Overlay.__index = Overlay

function O.new(label)
  return setmetatable(
    {
      label = label or "Overlay",
      controls = {},
      focus = 0,
      registered = {},
      controlHotkeys = {},
      capturedKeys = {},
      active = false,
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
      -- Optional module hook fn(overlay) fired each time it stops being active.
      _onDeactivate = false,
      -- HWND (`.id`) of the window this overlay was last active on. On activation the
      -- overlay restarts at its first control (and any tab control at tab 1) when this
      -- differs — i.e. a genuinely NEW window opened — but resumes where the user was
      -- when it's the SAME window regaining focus (Alt+Tab out and back). See _activate.
      _lastActiveId = false,
      -- Token of the dynamically (re)captured Space key, or false. Space activates the
      -- focused control like Return, but is released while an editable field is focused
      -- so the user can type spaces into it — see _syncSpaceCapture.
      _spaceToken = false,
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

-- A tab control (ReaHotkey's TabControl): a single focus stop where Left / Right switch
-- the current tab, and only the CURRENT tab's controls are Tab-reachable. `opts.tabs` is
-- an ordered list of { label, at = {x, y} } — `at` is the client-relative point clicked
-- to switch the underlying (UIA-less) dialog to that tab. Returns a handle whose
-- `h:onTab(n)` is a `when` predicate; assign it to the child controls of tab n (added
-- after this call) so they only show — and are only Tab-reachable — while tab n is
-- current. Tab controls can nest: gate a nested control on `h1:onTab(a)` AND its own.
function Overlay:addTabControl(opts)
  local c = add(self, { kind = "tabcontrol", label = opts.label or "", tabs = opts.tabs or {}, _current = 1 })
  self._hasTabControl = true
  return {
    control = c,
    onTab = function(_, n)
      return function() return c._current == n end
    end,
  }
end

function Overlay:addHotspotButton(opts)
  -- `rawOrigin` pins the control to the matched control's OWN client frame,
  -- skipping the per-overlay content offset — for chrome controls (e.g. Komplete
  -- Kontrol's menu) that live in the host frame, not the nested plugin's content.
  -- `fromRight` anchors `at`'s x to the control's RIGHT edge (see hotspotPoint) —
  -- for UI that is laid out from the right, so it stays put as the plugin is resized.
  return add(self, {
    kind = "hotspot",
    label = opts.label,
    at = opts.at,
    hotkey = opts.hotkey,
    rawOrigin = opts.rawOrigin,
    fromRight = opts.fromRight,
  })
end

function Overlay:addCustomButton(opts)
  return add(self, { kind = "custom", label = opts.label, onActivate = opts.onActivate, hotkey = opts.hotkey })
end

function Overlay:addOCRButton(opts)
  -- `readOnly` makes the control a pure announcer: focusing/activating re-reads the
  -- region via OCR but does NOT click it. For value/label readouts sitting over a
  -- drag knob or near a destructive control, where a click would change state.
  return add(self, { kind = "ocr", label = opts.label, region = opts.region, hotkey = opts.hotkey, readOnly = opts.readOnly })
end

-- An OCR-read EDIT field (ReaHotkey's OCREdit — a distinct control type from OCRButton,
-- not a "button that types"): on focus it reads + announces the current value via OCR
-- and clicks the field so the caret lands in the real edit control, letting the user type
-- to change it (e.g. rename a preset) then Tab on to a Save button. Announced as an
-- "edit", so a screen-reader user hears it is an editable field, not a button.
function Overlay:addOCREdit(opts)
  return add(self, { kind = "ocredit", label = opts.label, region = opts.region, hotkey = opts.hotkey })
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
    -- Optional per-control overrides of the module-wide match robustness defaults.
    tolerance = opts.tolerance,
    scales = opts.scales,
  })
end

-- A toggle whose on/off state is read from a SINGLE pixel at its click point (one screen
-- touch — cheap, unlike a per-pixel region scan), compared to an on/off reference colour
-- (nearest wins) — ReaHotkey's HotspotToggleButton. Activating clicks the point (toggling
-- it) and re-reads the state. `at` is origin-relative; `onColor`/`offColor` are {r, g, b}.
function Overlay:addHotspotToggle(opts)
  return add(self, {
    kind = "hotspottoggle",
    label = opts.label,
    at = opts.at,
    onColor = opts.onColor,
    offColor = opts.offColor,
    hotkey = opts.hotkey,
    rawOrigin = opts.rawOrigin,
    fromRight = opts.fromRight,
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
--
-- `opts.anchor` (default false): also ANCHOR the overlay's control coordinates to the
-- landmark's on-screen position (see originOf) — for a library whose landmark sits in
-- the same UI panel as its controls, so they stay put across hosts/window states.
-- WITHOUT it, :landmark is a pure gate and controls stay client-relative (e.g. a dialog
-- using its header image only to tell itself apart from a sibling on a shared slot).
function Overlay:landmark(image, opts)
  self._anchorLandmark = opts ~= nil and opts.anchor == true
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
      -- Tolerance but NO multi-scale here: the landmark searches the FULL plugin
      -- region, and trying many scales over that (each a fresh full-region scan +
      -- resize) is far too slow and starves the worker when 1.0 misses. Scaling is for
      -- small regions (gtoggle); the wordmark is captured at this setup, so 1.0 fits.
      host.screen.imageSearchAsync(image, { region = region, tolerance = MATCH_TOLERANCE }, function(hit)
        pending = false
        local was = matched
        lastHit = hit or nil
        -- Expose the hit's screen rect so a landmark-anchored overlay can resolve its
        -- control coordinates RELATIVE to the on-screen library UI (host / browser-state /
        -- version independent — the wordmark and the controls move together). See originOf.
        self._landmarkHit = hit or nil
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
  { static = "", hotspot = "button", custom = "button", ocr = "button", ocredit = "edit",
    gtoggle = "toggle button", hotspottoggle = "toggle button", tabcontrol = "tab control" }

-- Origin (top-left) of the overlay's coordinate space in screen pixels: the
-- client-area top-left of the active context's coordinate window — the plugin
-- window itself when standalone, or the embedded plugin's child control when
-- hosted in a DAW. Re-resolved per call so it tracks window moves. (0, 0) when
-- unattached. All control coordinates (OCR regions, hotspots) are relative to it.
local function originOf(self, control)
  if self.activeCtx then
    local o = self.activeCtx.origin()
    if o and o.client then
      -- An optional per-overlay offset shifts the whole coordinate frame: a
      -- container library overlay uses it for the host version's content shift
      -- (e.g. Kontakt 8 nudges content +29 px down vs Kontakt 7; a Kontakt nested
      -- in Komplete Kontrol re-anchors to KK's inner content origin), resolved per
      -- active control, so authored coordinates adapt without the library knowing
      -- the host version.
      --
      -- A control flagged `rawOrigin` opts OUT of that offset: it is authored in
      -- the matched control's OWN client frame, not the nested plugin's content
      -- frame. This lets one overlay mix two coordinate frames — content controls
      -- (offset applied) and host-chrome controls like KK's menu hotspot (raw) —
      -- which is exactly the KK→Kontakt nesting case (two frames).
      if control and control.rawOrigin then
        return o.client.x, o.client.y
      end
      -- A landmark-ANCHORED overlay (landmark(image, {anchor=true}) and its image is on
      -- screen) resolves NON-raw control coordinates relative to the landmark's top-left,
      -- so positions track the on-screen library UI regardless of host, Kontakt browser
      -- state, window size or version — the wordmark and the controls move together.
      -- This replaces the absolute content frame (which shifts per host) entirely.
      -- Opt-in: an overlay that uses :landmark only as a GATE (e.g. a dialog telling
      -- itself apart from a sibling on a shared slot) keeps client-relative coordinates.
      if self._anchorLandmark and self._landmarkHit then
        return self._landmarkHit.x, self._landmarkHit.y
      end
      local dx, dy = 0, 0
      if self._originOffset then
        dx, dy = self._originOffset(o)
      end
      return o.client.x + dx, o.client.y + dy
    end
  end
  return 0, 0
end

-- The overlay's (first) tab control and its index in `controls`, or nil if none.
-- Defined here, after `originOf`, since the selectors below use it (a Luau local is
-- only visible after its declaration — so tab helpers all live below originOf).
local function findTabControl(self)
  for i, c in ipairs(self.controls) do
    if c.kind == "tabcontrol" then return i, c end
  end
  return nil
end

-- Spoken form of a tab, ReaHotkey-style: "<label> tab selected" — a tab's state is
-- "selected" when it is the current one, which is the only tab we ever announce. An
-- optional `extra` (e.g. "tab control") is inserted, and a bound hotkey's label is spoken
-- last (ReaHotkey appends the HotkeyLabel), so the user hears how to jump to it.
local function tabMessage(tab, extra)
  local m = ((tab and tab.label) or "") .. " tab selected"
  if extra and extra ~= "" then m = m .. ", " .. extra end
  if tab and tab.hotkeyLabel and tab.hotkeyLabel ~= "" then m = m .. ", " .. tab.hotkeyLabel end
  return m
end

-- Clicks tab control `c`'s CURRENT tab coordinate (so the underlying dialog follows) and
-- announces it. Shared by every tab selector.
local function selectTab(self, c)
  local tab = c.tabs[c._current]
  if tab and tab.at then
    local ox, oy = originOf(self, c)
    host.input.click(ox + tab.at[1], oy + tab.at[2])
  end
  -- Focus is now on the tab control (a non-editable stop): make sure Space is captured
  -- again if we arrived here from an editable field via Ctrl+Tab / Ctrl+<n>.
  self:_syncSpaceCapture()
  -- The new tab exposes a different set of controls, so the hotkeys they claim change.
  if self.active then self:_syncControlHotkeys() end
  host.speech.output(tabMessage(tab), { interrupt = true })
end

-- Left / Right on a focused tab control: switch its tab by `dir` (±1), wrapping. No-op
-- unless the focused control IS a tab control (so arrows stay inert on other controls).
function Overlay:switchTab(dir)
  local c = self.controls[self.focus]
  if not c or c.kind ~= "tabcontrol" or #c.tabs == 0 then return end
  c._current = ((c._current - 1 + dir) % #c.tabs) + 1
  selectTab(self, c)
end

-- Ctrl+Tab / Ctrl+Shift+Tab: switch the overlay's tab control from ANYWHERE (focus need
-- not be on it) and move focus onto it — ReaHotkey's FocusNextPreviousTab.
function Overlay:cycleTab(dir)
  local i, c = findTabControl(self)
  if not c or #c.tabs == 0 then return end
  self.focus = i
  c._current = ((c._current - 1 + dir) % #c.tabs) + 1
  selectTab(self, c)
end

-- Ctrl+<n>: jump straight to tab n and move focus onto the tab control.
function Overlay:gotoTab(n)
  local i, c = findTabControl(self)
  if not c or n < 1 or n > #c.tabs then return end
  self.focus = i
  c._current = n
  selectTab(self, c)
end

local function ocrCenter(self, c)
  local ox, oy = originOf(self, c)
  return ox + math.floor((c.region[1] + c.region[3]) / 2),
    oy + math.floor((c.region[2] + c.region[4]) / 2)
end

-- Reads a graphical toggle's state by image-matching its region against the
-- on/off templates. Returns "on" / "off" / nil (unknown).
local function gtoggleState(self, c)
  if not c.region then return nil end
  local ox, oy = originOf(self, c)
  local region = { ox + c.region[1], oy + c.region[2], ox + c.region[3], oy + c.region[4] }
  local opts = {
    region = region,
    tolerance = c.tolerance or MATCH_TOLERANCE,
    scales = c.scales or MATCH_SCALES,
  }
  -- One capture, both templates: the region is touched ONCE (not once per template,
  -- each a ~1-frame screen read), and on/off are matched against the SAME frame — so a
  -- toggle changing mid-repaint can't slip between two separate captures (which read
  -- nil). The hit's 1-based index maps back to the on/off label.
  if host.screen.imageSearchMulti then
    local templates, labels = {}, {}
    if c.onImage then templates[#templates + 1] = c.onImage; labels[#labels + 1] = "on" end
    if c.offImage then templates[#templates + 1] = c.offImage; labels[#labels + 1] = "off" end
    if #templates == 0 then return nil end
    local idx = host.screen.imageSearchMulti(templates, opts)
    return idx and labels[idx] or nil
  end
  -- Fallback for a host without imageSearchMulti: two separate captures.
  if c.onImage and host.screen.imageSearch(c.onImage, opts) then return "on" end
  if c.offImage and host.screen.imageSearch(c.offImage, opts) then return "off" end
  return nil
end

-- Absolute screen point of a control's `at` hotspot. Normally origin + at, but a
-- `fromRight` control measures x from the coordinate window's RIGHT edge instead
-- (ReaHotkey's `ControlX + ControlWidth − N`): plugin UI that is laid out from the
-- right stays correctly targeted whatever the plugin's width is.
local function hotspotPoint(self, c)
  -- `at` may be a FUNCTION of the active overlay, for a control whose position depends
  -- on what is focused right now — e.g. one Kontakt header serving both Kontakt 7 and
  -- Kontakt 8, whose chrome sits at different coordinates. Returning nil (version not
  -- yet known) yields no point, and the caller skips the click rather than guessing.
  local at = c.at
  if type(at) == "function" then at = at(self) end
  if type(at) ~= "table" then return nil end
  local ox, oy = originOf(self, c)
  local x = at[1]
  if c.fromRight then
    local o = self.activeCtx and self.activeCtx.origin()
    local w = (o and o.bounds and o.bounds.w) or 0
    x = w - at[1]
  end
  return ox + x, oy + at[2]
end

-- On/off state of a HotspotToggle: read the single pixel at its click point and return
-- whichever reference colour it is closer to (Manhattan distance) — ReaHotkey's pixel
-- toggle. Cheap (one screen touch). nil if unconfigured or the pixel can't be read.
local function hotspotToggleState(self, c)
  if not (c.at and c.onColor and c.offColor) then return nil end
  local px, py = hotspotPoint(self, c)
  if not px then return nil end
  local p = host.screen.pixel(px, py)
  if not p then return nil end
  local don = math.abs(p.r - c.onColor[1]) + math.abs(p.g - c.onColor[2]) + math.abs(p.b - c.onColor[3])
  local doff = math.abs(p.r - c.offColor[1]) + math.abs(p.g - c.offColor[2]) + math.abs(p.b - c.offColor[3])
  if don <= doff then return "on" else return "off" end
end

local function speakControl(self, index, moveMouse)
  local c = self.controls[index]
  if not c then return end
  -- A tab control announces WHICH tab is current, then that it is a tab control (so the
  -- user knows Left/Right — or Ctrl+Tab, Ctrl+number — switch it). Its current tab's
  -- controls are the ones Tab then steps into (see isFocusable's `when` gating).
  if c.kind == "tabcontrol" then
    host.speech.output(tabMessage(c.tabs[c._current], "tab control"), { interrupt = true })
    return
  end
  local parts = { c.label or "" }
  local tl = TYPE_LABEL[c.kind]
  if tl ~= "" then parts[#parts + 1] = tl end
  if (c.kind == "ocr" or c.kind == "ocredit") and c.region then
    local ox, oy = originOf(self, c)
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
    -- An OCREdit drops the caret into the real edit control on focus, so the user can type
    -- to change the value right away. Read first (clean), then click. `moveMouse` is true
    -- only on a focus read, so navigating away doesn't re-click.
    if c.kind == "ocredit" and moveMouse then
      host.input.click(ox + math.floor((c.region[1] + c.region[3]) / 2),
        oy + math.floor((c.region[2] + c.region[4]) / 2))
    end
  end
  if c.kind == "gtoggle" then
    local st = gtoggleState(self, c)
    if st then parts[#parts + 1] = st end
  end
  if c.kind == "hotspottoggle" then
    local st = hotspotToggleState(self, c)
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
  if found then
    self:_syncSpaceCapture()
    speakControl(self, self.focus, true)
  end
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
  if found then
    self:_syncSpaceCapture()
    speakControl(self, self.focus, true)
  end
end

-- Enable a standalone UIA Tab pass-through: Tab / Shift+Tab move keyboard focus through
-- the attached window's OWN controls (via host.uia.focusStep) and announce each — for a
-- plugin whose window doesn't move focus on Tab itself (Kontakt standalone). Return /
-- Space / arrows are deliberately NOT captured, so the focused control handles them
-- natively (toggle a check box, open a combo). The overlay's own actions stay on their
-- global hotkeys. Ported from ReaHotkey's AddCustomPassThrough("Standalone", …).
function Overlay:enableUiaPassThrough()
  self._passThrough = function(o, dir)
    local ctrl = o.activeCtx and o.activeCtx.origin()
    local hwnd = ctrl and ctrl.id
    if not hwnd then return end
    -- Move the plugin's OWN keyboard focus to the next/previous control. We deliberately
    -- do NOT announce it: SetFocus raises a real UIA focus event, so the screen reader
    -- (NVDA) already reads the control — speaking it here too would double-announce.
    -- (Our other controls are VIRTUAL, not real focus, so those we still announce.)
    host.uia.focusStep(hwnd, dir)
  end
  return self
end

function Overlay:activate(index)
  local i = index or self.focus
  local c = self.controls[i]
  if not c then return end
  if c.when and c.when(self) ~= true then return end -- a hidden control's hotkey no-ops
  if c.kind == "hotspot" and c.at then
    local px, py = hotspotPoint(self, c)
    if not px then return end
    host.input.click(px, py)
    host.speech.output((c.label or "") .. ", activated", { interrupt = true })
  elseif c.kind == "custom" and c.onActivate then
    c.onActivate(self) -- the overlay, so a header control can reach the active context
  elseif (c.kind == "ocr" or c.kind == "ocredit") and c.region then
    -- An OCR control re-reads its label on activate. An OCRButton then clicks the region
    -- centre (it sits over a clickable element), like ReaHotkey — unless it is `readOnly`
    -- (a pure value/label announcer, safe next to a destructive control). An OCREdit
    -- always clicks: activating it re-focuses the edit field for typing.
    speakControl(self, i, false)
    if c.kind == "ocredit" or not c.readOnly then
      local cx, cy = ocrCenter(self, c)
      host.input.click(cx, cy)
    end
  elseif c.kind == "gtoggle" and c.region then
    local ox, oy = originOf(self, c)
    host.input.click(ox + math.floor((c.region[1] + c.region[3]) / 2),
      oy + math.floor((c.region[2] + c.region[4]) / 2))
    -- Re-read the new state once the UI redraws.
    if host.timer then host.timer.after(150, function() speakControl(self, i, false) end) end
  elseif c.kind == "hotspottoggle" and c.at then
    local px, py = hotspotPoint(self, c)
    if not px then return end
    host.input.click(px, py)
    -- Re-read the new state (announced with the label) once the UI redraws.
    if host.timer then host.timer.after(150, function() speakControl(self, i, false) end) end
  end
end

-- The first of `candidates` (control indices) that is currently visible, or nil when
-- none is — i.e. which control a shared hotkey belongs to right now.
function Overlay:_visibleClaimant(candidates)
  for _, i in ipairs(candidates) do
    local c = self.controls[i]
    if c and (c.when == nil or c.when(self) == true) then return i end
  end
  return nil
end

-- (Re-)registers the per-control global hotkeys. Kept separate from the key CAPTURES,
-- which must NOT be dropped and retaken on a re-sync: their release is token-scoped and
-- shared with sibling overlays in the same module.
--
-- One key combination can only be registered ONCE — the OS rejects a second claim on it
-- — and an overlay legitimately carries mutually exclusive control sets that reuse keys:
-- a Kontakt nested in Komplete Kontrol offers KK's menu on Alt+M, a bare one offers
-- Kontakt's snapshot menu on the same key, each gated by `when`. So all claimants of a
-- spec are collected, and:
--   • the OS combination is taken only while at least one of them is visible, so a
--     control gated to a plugin variant doesn't swallow a key on a plugin where it can
--     never appear (nor kill a visible sibling's identical hotkey by declaration order);
--   • WHICH claimant a press activates is decided at PRESS time, not here — the overlay
--     stays active when focus moves to another matching plugin window, so a winner
--     frozen at registration would go stale (Alt+M would keep firing Komplete Kontrol's
--     menu after focus moved to a bare Kontakt).
-- Re-run whenever the visible set can have changed: a tab switch (see selectTab) and a
-- change of coordinate origin (see _recheck).
function Overlay:_syncControlHotkeys()
  for _, id in ipairs(self.controlHotkeys or {}) do
    host.hotkey.unregister(id)
  end
  self.controlHotkeys = {}
  local o = self.activeCtx and self.activeCtx.origin()
  self._hotkeyOriginId = (o and o.id) or false
  local claim, specs = {}, {}
  for i, c in ipairs(self.controls) do
    if c.hotkey then
      local held = claim[c.hotkey]
      if held then
        held[#held + 1] = i
      else
        claim[c.hotkey] = { i }
        specs[#specs + 1] = c.hotkey
      end
    end
  end
  for _, spec in ipairs(specs) do
    local candidates = claim[spec]
    if self:_visibleClaimant(candidates) then
      self.controlHotkeys[#self.controlHotkeys + 1] = host.hotkey.register(spec, function()
        self:activate(self:_visibleClaimant(candidates) or candidates[1])
      end)
    end
  end
end

function Overlay:_registerHotkeys()
  self.registered = self.registered or {}
  self.capturedKeys = self.capturedKeys or {}
  self:_syncControlHotkeys()
  for _, c in ipairs(self.controls) do
    -- A tab control's individual TABS may each carry a hotkey (ReaHotkey's Tab.SetHotkey):
    -- pressing it brings that tab to the front from anywhere in the overlay (like
    -- Ctrl+<n>), and its label is spoken in the tab's announcement (see tabMessage). KK
    -- doesn't bind any, but other plugins (e.g. Dubler) do — so the mechanism is here.
    if c.kind == "tabcontrol" then
      for tn, tab in ipairs(c.tabs) do
        if tab.hotkey then
          local n = tn
          self.registered[#self.registered + 1] = host.hotkey.register(tab.hotkey, function() self:gotoTab(n) end)
        end
      end
    end
  end
  -- Capture (+ suppress) the normal navigation keys, ReaHotkey-style — only while the
  -- overlay is active (its window is focused). Modifier-exact, so Alt+Tab etc. pass
  -- through. Track the token capture returns (not the spec): release must target THIS
  -- exact registration, so another overlay in the same module re-capturing the same key
  -- (e.g. KK chrome and its Preferences dialog both capturing Tab) isn't dropped when
  -- this one deactivates. See host.keys.capture/release.
  local function cap(spec, fn)
    self.capturedKeys[#self.capturedKeys + 1] = host.keys.capture(spec, fn)
  end
  -- Tab moves *between* elements; arrow keys are left for value changes within a
  -- control (standard interaction scheme), so we don't capture them here.
  if self._passThrough then
    -- Standalone pass-through: Tab / Shift+Tab drive the plugin's native UIA focus.
    -- Return / Space / arrows are LEFT uncaptured so the focused control handles them
    -- natively; the overlay's own actions are reached via their global hotkeys.
    cap("Tab", function() self._passThrough(self, 1) end)
    cap("Shift+Tab", function() self._passThrough(self, -1) end)
  else
    cap("Tab", function() self:focusNext() end)
    cap("Shift+Tab", function() self:focusPrev() end)
    cap("Return", function() self:activate() end)
    -- Tab controls (ReaHotkey): Left/Right switch the tab when the tab control itself
    -- is focused (LeftRightHK); Ctrl+Tab / Ctrl+Shift+Tab switch it from anywhere in
    -- the overlay (ControlTab/ControlShiftTabHK); Ctrl+<n> jumps straight to tab n (our
    -- addition). Captured only when this overlay has a tab control, so these keys stay
    -- free everywhere else.
    if self._hasTabControl then
      cap("Left", function() self:switchTab(-1) end)
      cap("Right", function() self:switchTab(1) end)
      cap("Ctrl+Tab", function() self:cycleTab(1) end)
      cap("Ctrl+Shift+Tab", function() self:cycleTab(-1) end)
      local _, tcCtrl = findTabControl(self)
      local ntabs = tcCtrl and #tcCtrl.tabs or 0
      for n = 1, math.min(ntabs, 9) do
        cap("Ctrl+" .. n, function() self:gotoTab(n) end)
      end
    end
    -- Space activates the focused control like Return (standard Windows button
    -- behaviour). It is captured/released as focus moves — NOT via the static `cap`
    -- set above — so that while an editable field (ocredit) is focused the Space
    -- reaches the field and the user can type a literal space (e.g. a preset name
    -- with spaces). Set the initial state for the control focused right now.
    self:_syncSpaceCapture()
  end
end

function Overlay:_unregisterHotkeys()
  for _, id in ipairs(self.registered or {}) do
    host.hotkey.unregister(id)
  end
  self.registered = {}
  for _, id in ipairs(self.controlHotkeys or {}) do
    host.hotkey.unregister(id)
  end
  self.controlHotkeys = {}
  self._hotkeyOriginId = nil
  for _, token in ipairs(self.capturedKeys or {}) do
    host.keys.release(token)
  end
  self.capturedKeys = {}
  if self._spaceToken then
    host.keys.release(self._spaceToken)
    self._spaceToken = false
  end
end

-- Keeps the Space-activates-the-focused-control capture in sync with the focused
-- control's kind: captured for a normal (button-like) control, released while an
-- editable field (ocredit) is focused so Space reaches it as typed text. Idempotent;
-- called on every focus move (and at registration). No-op unless the overlay is active
-- and not a pass-through overlay (pass-through overlays never capture Space — the real
-- plugin control handles it).
function Overlay:_syncSpaceCapture()
  if not self.active or self._passThrough then
    return
  end
  local c = self.controls[self.focus]
  local editable = c ~= nil and c.kind == "ocredit"
  if editable then
    if self._spaceToken then
      host.keys.release(self._spaceToken)
      self._spaceToken = false
    end
  elseif not self._spaceToken then
    self._spaceToken = host.keys.capture("Space", function() self:activate() end)
  end
end

-- A module hook fired (with the overlay) each time it becomes the ACTIVE overlay —
-- runs after the activation context is set, so origin()/pluginHwnd are available.
function Overlay:onActivate(fn)
  self._onActivate = fn
  return self
end

-- A module hook fired (with the overlay) each time it stops being the active overlay —
-- the counterpart to onActivate. Used e.g. by the Komplete Kontrol scanning dialog to
-- announce "scanning stopped" (and let its progress loop wind down) when it closes.
function Overlay:onDeactivate(fn)
  self._onDeactivate = fn
  return self
end

function Overlay:_activate()
  if self.active then return end
  self.active = true
  -- The overlay starts FRESH — focus back on the first control, any tab control back on
  -- its first tab — when it becomes active on a genuinely NEW window, but resumes where
  -- the user was when the SAME still-open window merely regained the foreground (Alt+Tab
  -- out and back). We tell them apart by the origin window's HWND (`.id`): closing then
  -- reopening a window yields a new id → reset; Alt+Tab keeps the same id → resume. (A
  -- window reusing the very same HWND across close/reopen would resume too, but these
  -- plugin/dialog windows are destroyed and recreated, so their id changes.) Clearing
  -- focus here lets the `if self.focus == 0` block below re-pick the first focusable.
  local origin = self.activeCtx and self.activeCtx.origin()
  local id = (origin and origin.id) or false
  if id ~= self._lastActiveId then
    self.focus = 0
    for _, c in ipairs(self.controls) do
      if c.kind == "tabcontrol" then c._current = 1 end
    end
  end
  self._lastActiveId = id
  -- Pick the first focusable control after a reset (or on the very first activation);
  -- otherwise self.focus still holds the resumed position.
  if self.focus == 0 then
    for i = 1, #self.controls do
      if isFocusable(self, i) then
        self.focus = i
        break
      end
    end
  end
  self:_registerHotkeys()
  -- Scope key suppression to this (now-foreground) window: when a control opens a menu
  -- (another window comes forward), Tab/Enter pass to it natively, and resume here once
  -- the plugin window is focused again.
  host.keys.scope(true)
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
  host.keys.scope(false)
  host.keys.menuOpen(false)
  self:_unregisterHotkeys()
  if self._onDeactivate then self._onDeactivate(self) end
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
  -- An already-active overlay STAYS active when focus moves to another window it also
  -- matches (the arbiter re-reports the match without re-activating), and the visible
  -- control set can differ there — e.g. a Kontakt 7's header controls versus a Kontakt
  -- 8's. Re-sync the per-control hotkeys when that coordinate origin changes.
  if self.active and matched then
    local o = self.activeCtx and self.activeCtx.origin()
    if ((o and o.id) or false) ~= self._hotkeyOriginId then
      self:_syncControlHotkeys()
    end
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
    -- Pick the first focusable BEFORE re-registering, so _registerHotkeys' Space sync
    -- sees the real focused control — an editable field (ocredit) that ends up first
    -- must not have Space captured over it. Same order as _activate.
    for i = 1, #self.controls do
      if isFocusable(self, i) then self.focus = i break end
    end
    self:_registerHotkeys()
  end
end

-- While active, watch (via UIA, control type 50009 = Menu) for a menu open inside
-- the plugin and tell the hook to let captured nav keys pass through to it — a
-- plugin's own (Qt) menu is not a Win32 #32768 menu, so the hook can't see it.
function Overlay:_startMenuWatch()
  -- Disable-safe recurring poll (survives a disable/enable cycle, unlike a Lua
  -- self-rescheduling host.timer.after).
  host.timer.every(150, function()
    if self.active then
      local ctrl = self.activeCtx and self.activeCtx.origin()
      local open = ctrl ~= nil and ctrl.id ~= nil and host.uia.find(ctrl.id, "", 50009) == true
      host.keys.menuOpen(open)
    end
  end)
end

-- Calibration helper: capture the live on-screen pixels of the control labelled
-- `label` — its exact region with the active origin/offset applied — to `path` (an
-- absolute file path) as a PNG, for (re-)capturing an image control's on/off
-- templates against the real plugin. Returns whether it saved. The overlay must be
-- active (so the origin resolves to the focused plugin).
function Overlay:captureControl(label, path)
  if not self.active then
    host.log.info("captureControl: overlay not active")
    return false
  end
  for _, c in ipairs(self.controls) do
    if c.region and c.label == label then
      local ox, oy = originOf(self, c)
      local region = { ox + c.region[1], oy + c.region[2], ox + c.region[3], oy + c.region[4] }
      local ok = host.screen.save ~= nil and host.screen.save(path, { region = region })
      host.log.info(string.format(
        "captureControl '%s' region=(%d,%d,%d,%d) -> %s ok=%s",
        label, region[1], region[2], region[3], region[4], path, tostring(ok)
      ))
      return ok == true
    end
  end
  host.log.info("captureControl: no control labelled '" .. tostring(label) .. "' with a region")
  return false
end

-- Like captureControl but for an arbitrary origin-relative region {x1,y1,x2,y2} — a
-- wider context shot for calibration/inspection. Overlay must be active.
function Overlay:captureRegion(region, path)
  if not self.active then
    host.log.info("captureRegion: overlay not active")
    return false
  end
  local ox, oy = originOf(self)
  local r = { ox + region[1], oy + region[2], ox + region[3], oy + region[4] }
  local ok = host.screen.save ~= nil and host.screen.save(path, { region = r })
  host.log.info(string.format(
    "captureRegion (%d,%d,%d,%d) -> %s ok=%s", r[1], r[2], r[3], r[4], path, tostring(ok)
  ))
  return ok == true
end

-- Captures the WHOLE active plugin control (its client rect) to `path` — the full
-- plugin UI, for locating controls and recomputing coordinates/offsets. Active only.
function Overlay:captureFull(path)
  if not self.active then
    host.log.info("captureFull: overlay not active")
    return false
  end
  local o = self.activeCtx and self.activeCtx.origin()
  if not o or not o.client or not o.bounds then return false end
  local r = { o.client.x, o.client.y, o.client.x + o.bounds.w, o.client.y + o.bounds.h }
  local ok = host.screen.save ~= nil and host.screen.save(path, { region = r })
  host.log.info(string.format(
    "captureFull client=(%d,%d) bounds=%dx%d -> %s ok=%s",
    o.client.x, o.client.y, o.bounds.w, o.bounds.h, path, tostring(ok)
  ))
  return ok == true
end

-- Dev calibration: dump a content-frame-exact shot of the whole plugin content
-- (`content-full.png`, where pixel (x,y) == authored content coordinate (x,y)) plus
-- a small box around every positioned control, into `dir` (an absolute path), so
-- re-derived coordinates can be verified against the live plugin in one shot.
-- Skips rawOrigin (chrome-frame) controls. Active only.
function Overlay:captureAll(dir)
  if not self.active then
    host.log.info("captureAll: overlay not active")
    return false
  end
  local ox, oy = originOf(self)
  host.log.info(string.format("captureAll: content origin=(%d,%d) -> %s", ox, oy, dir))
  self:captureRegion({ 0, 0, 640, 430 }, dir .. "/content-full.png")
  for _, c in ipairs(self.controls) do
    if c.label and c.label ~= "" and not c.rawOrigin then
      local cx, cy
      if type(c.at) == "table" then
        cx, cy = c.at[1], c.at[2]
      elseif c.region then
        cx = math.floor((c.region[1] + c.region[3]) / 2)
        cy = math.floor((c.region[2] + c.region[4]) / 2)
      end
      if cx then
        local safe = string.gsub(c.label, "[^%w]", "_")
        self:captureRegion({ cx - 34, cy - 26, cx + 34, cy + 26 }, string.format("%s/ctl-%s.png", dir, safe))
      end
    end
  end
  return true
end

-- Exposed as the code module com.platform.overlay: a module that depends on it
-- receives O via host.require (the trailing return is what that captures).
return O
