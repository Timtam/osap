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

-- The specificity ladder overlays compete on within a slot, named:
--
--   chrome  — a host's own frame (Komplete Kontrol's menu bar around a nested plugin)
--   base    — the plugin's generic header, when nothing more specific applies
--   content — what is loaded INSIDE the plugin right now (a sample library)
--   dialog  — a modal that must own the keyboard while it is up
--
-- Spaced by ten so a case nobody has met yet can slot BETWEEN two of these without
-- renumbering the ladder — which would be a silent break: an overlay on a new number
-- competing against one that still passes an old literal simply loses, with no error.
-- That is also why every module had to be on O.layer before this spacing could be
-- introduced (verified: no bare specificity numbers remain outside the runtime).
O.layer = { chrome = 10, base = 20, content = 30, dialog = 40 }

-- The width and height of an origin's CONTENT, which is what coordinates are measured
-- against — `client` for a window that has a frame, falling back to the window rect for
-- anything that does not report one. An embedded plugin's control is borderless, so the
-- two agree there and this changes nothing; a standalone plugin's window is not, and a
-- Kontakt 8 one is 1026 wide around 1010 px of plugin. Anything anchored to the right edge
-- (Kontakt's whole header cluster) or probing near it (its view probe) has to use this,
-- or it sits a border-width off in exactly the case that is hardest to notice.
function O.contentSize(o)
  if not o then return 0, 0 end
  local c, b = o.client, o.bounds
  local w = (c and c.w and c.w > 0 and c.w) or (b and b.w) or 0
  local h = (c and c.h and c.h > 0 and c.h) or (b and b.h) or 0
  return w, h
end

-- One line per VM when the calibration keys arm (see the Calibration section).
local calibrationAnnounced = false

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
      -- Free-form per-overlay scratch space for the module that owns it, so module
      -- state does not have to squat in the runtime's reserved `_`-prefixed fields.
      state = {},
    },
    Overlay
  )
end

-- Declaring something AFTER the overlay is bound is always a mistake, and a subtle one:
-- `_join` ends with a synchronous `_recheck`, so a gate or landmark set afterwards misses
-- that first evaluation and the overlay can activate ungated for one pass. Nothing in the
-- code enforced the order, and every call site happened to get it right — which is exactly
-- the kind of rule that holds until someone new writes the next module.
local function assertUnbound(self, what)
  if self._bound then
    error(string.format(
      "overlay '%s': %s must be called BEFORE binding it — the binding is what starts it",
      tostring(self.label), what
    ), 3)
  end
end

-- ANDs an enclosing group's predicate (see Overlay:group) onto a control's own `when`.
-- Both must hold, and both must return exactly `true` — see isVisible.
local function andWhen(outer, own)
  if not outer then return own end
  if not own then return outer end
  return function(o) return outer(o) == true and own(o) == true end
end

local function add(self, control)
  assertUnbound(self, "adding a control")
  control.when = andWhen(self._groupWhen, control.when)
  self.controls[#self.controls + 1] = control
  return control
end

-- Is this control shown right now? A `when` predicate must return exactly `true`;
-- anything else — including a truthy non-boolean such as a `string.find` result — counts
-- as hidden. Every reader of `when` goes through here, because they must agree: one
-- place decides Tab-reachability (isFocusable), whether a hotkey press does anything
-- (activate), and which control owns a shared hotkey (_visibleClaimant). Disagreeing
-- would make a control silently unreachable while its key still fired something else.
local function isVisible(self, c)
  return c ~= nil and (c.when == nil or c.when(self) == true)
end

-- Adds everything `build` adds under a shared condition: `pred` is ANDed onto each
-- control's own `when`, and groups nest. Replaces poking `.when` onto every returned
-- control by hand — which is easy to forget on one control of a set, and reads as
-- unrelated statements rather than "these belong together".
function Overlay:group(pred, build)
  local outer = self._groupWhen
  self._groupWhen = andWhen(outer, pred)
  build(self)
  self._groupWhen = outer
  return self
end

-- The active context's coordinate window — the plugin control when embedded, the window
-- when standalone — or nil when the overlay isn't active. `:hwnd()` is its handle. This
-- is the module's handle on the thing it overlays, and every module was re-deriving it.
function Overlay:origin()
  return self.activeCtx and self.activeCtx.origin() or nil
end

function Overlay:hwnd()
  local o = self:origin()
  return o and o.id or nil
end

-- Shifts this overlay's whole coordinate frame: `fn(origin) -> dx, dy`, resolved per
-- active control (a host version's content shift, a nested plugin's inner origin).
function Overlay:frame(fn)
  assertUnbound(self, "frame")
  self._originOffset = fn
  return self
end

function Overlay:addStaticText(labelOrOpts)
  if type(labelOrOpts) == "table" then
    return add(self, { kind = "static", label = labelOrOpts.label, when = labelOrOpts.when })
  end
  return add(self, { kind = "static", label = labelOrOpts })
end

-- A tab control (ReaHotkey's TabControl): a single focus stop where Left / Right switch
-- the current tab, and only the CURRENT tab's controls are Tab-reachable. `opts.tabs` is
-- an ordered list of { label, at = {x, y} } — `at` is the client-relative point clicked
-- to switch the underlying (UIA-less) dialog to that tab. Returns a handle whose
-- `h:onTab(n)` is a `when` predicate; assign it to the child controls of tab n (added
-- after this call) so they only show — and are only Tab-reachable — while tab n is
-- current. Tab controls can nest: gate a nested control on `h1:onTab(a)` AND its own.
function Overlay:addTabControl(opts)
  local c = add(self, {
    kind = "tabcontrol", label = opts.label or "", tabs = opts.tabs or {},
    _current = 1, when = opts.when,
  })
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
  -- `points` (instead of `at`) clicks a SEQUENCE, with `settle` ms between each — for UI
  -- where reaching a control means getting there first: switch to its tab, then its
  -- sub-tab, then click it. Making that one control keeps every action self-contained, so
  -- the user never has to navigate a tab structure they cannot see.
  return add(self, {
    kind = "hotspot",
    label = opts.label,
    ocrLabel = opts.ocrLabel,
    at = opts.at,
    points = opts.points,
    settle = opts.settle,
    hotkey = opts.hotkey,
    rawOrigin = opts.rawOrigin,
    fromRight = opts.fromRight,
    when = opts.when,
  })
end

function Overlay:addCustomButton(opts)
  return add(self, {
    kind = "custom", label = opts.label, onActivate = opts.onActivate,
    hotkey = opts.hotkey, when = opts.when,
  })
end

function Overlay:addOCRButton(opts)
  -- `readOnly` makes the control a pure announcer: focusing/activating re-reads the
  -- region via OCR but does NOT click it. For value/label readouts sitting over a
  -- drag knob or near a destructive control, where a click would change state.
  return add(self, {
    kind = "ocr", label = opts.label, region = opts.region, hotkey = opts.hotkey,
    readOnly = opts.readOnly, when = opts.when,
  })
end

-- An OCR-read EDIT field (ReaHotkey's OCREdit — a distinct control type from OCRButton,
-- not a "button that types"): on focus it reads + announces the current value via OCR
-- and clicks the field so the caret lands in the real edit control, letting the user type
-- to change it (e.g. rename a preset) then Tab on to a Save button. Announced as an
-- "edit", so a screen-reader user hears it is an editable field, not a button.
function Overlay:addOCREdit(opts)
  return add(self, {
    kind = "ocredit", label = opts.label, region = opts.region,
    hotkey = opts.hotkey, when = opts.when,
  })
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
    when = opts.when,
  })
end

-- A GRAPHICAL SLIDER — ReaHotkey's GraphicalHorizontalSlider, the control Audio Imperia's
-- libraries use for a Close/Mid/Far mic blend. There is nothing to read its value from: the
-- position IS where the thumb is drawn, so the thumb is found by image search inside
-- `region`, and its centre against `from` (0 %) and `to` (100 %) gives a percentage.
--
-- `thumbImage` may be a list, like a toggle's images — the thumb is commonly drawn one way
-- at rest and another while it is being dragged.
--
-- Adjusting it is a CLOSED LOOP rather than a calculation, because a plugin's slider does
-- not necessarily move a pixel when you drag it a pixel: drag by one percent, look again,
-- and if the reading has not changed, extend by one more pixel and repeat — ReaHotkey does
-- exactly this (AccessibilityOverlay.ahk:2560-2600). Bounded here, which ReaHotkey's is
-- not: a loop that cannot converge would otherwise drag forever inside the user's plugin.
function Overlay:addSlider(opts)
  return add(self, {
    kind = "slider",
    label = opts.label,
    region = opts.region,
    thumbImage = opts.thumbImage,
    from = opts.from,
    to = opts.to,
    hotkey = opts.hotkey,
    tolerance = opts.tolerance,
    scales = opts.scales,
    when = opts.when,
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
    ocrLabel = opts.ocrLabel,
    at = opts.at,
    onColor = opts.onColor,
    offColor = opts.offColor,
    hotkey = opts.hotkey,
    rawOrigin = opts.rawOrigin,
    fromRight = opts.fromRight,
    when = opts.when,
  })
end

-- An extra activation condition ANDed on top of the context match: `fn(origin)`
-- (origin = the active context's coordinate window/control) returns whether this
-- overlay should be active. A derived/library overlay uses it to gate on its own,
-- more-specific landmark while sharing the base's window context.
function Overlay:gate(fn)
  assertUnbound(self, "gate")
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
  assertUnbound(self, "landmark")
  self._landmarkImage = image
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
    -- Don't hunt for a landmark that cannot be relevant. Only one sample library is loaded
    -- in a plugin at a time, so once a claimant of at least this overlay's rank owns the
    -- slot, this search cannot change anything until that one lets go — and it WILL let go,
    -- because the winner keeps checking its own landmark. Measured before adding this: a
    -- single full-region match ran to 12 s, and twelve libraries polled every 500 ms, so
    -- eleven of every twelve searches were work nobody could use.
    --
    -- Rank matters: a LESS specific winner (Kontakt's own header while no library is
    -- recognised) must not silence a library overlay, or it could never take over.
    if self._slot and self._claim and host.arbiter.winnerSpecificity then
      local owner = host.arbiter.winner(self._slot)
      local rank = host.arbiter.winnerSpecificity(self._slot)
      if owner and owner ~= self._claim and rank and rank >= (self._specificity or 0) then
        return matched
      end
    end
    if not pending then
      pending = true
      local sticky = lastHit ~= nil
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
        -- A miss in the STICKY box is not the same as "gone". The box is the last hit ±6 px,
        -- so anything that repaints the landmark — a hover highlight, a value redraw, the
        -- panel scrolling a pixel — misses there while the library is plainly still loaded.
        -- Treating that as lost tore the overlay down and rebuilt it on the next pass, which
        -- is what "sometimes there is no overlay, sometimes there is" turned out to be:
        -- Cerberus flapping found → lost → found. So a sticky miss only DROPS THE ANCHOR and
        -- lets the next pass search the whole plugin; only a full-region miss means lost.
        -- Costs one extra poll before a genuinely closed library deactivates.
        if hit == nil and sticky then
          lastHit = nil
          host.log.info(("[landmark] %s: not where it was, widening the search"):format(
            tostring(self.label)
          ))
          return
        end
        local was = matched
        lastHit = hit or nil
        -- Expose the hit's screen rect so a landmark-anchored overlay can resolve its
        -- control coordinates RELATIVE to the on-screen library UI (host / browser-state /
        -- version independent — the wordmark and the controls move together). See originOf.
        self._landmarkHit = hit or nil
        matched = hit ~= nil
        if matched ~= was then
          -- Logged on the TRANSITION only, never per poll. "My library overlay does not
          -- appear" is the failure a module author hits first, and the landmark search is
          -- the step that decides it — but it runs asynchronously on a worker, so nothing
          -- about it is otherwise observable. Quiet in the steady state, and it records
          -- both the region searched and the hit that anchors every coordinate.
          host.log.info(("[landmark] %s: %s  (region %d,%d–%d,%d)"):format(
            tostring(self.label),
            hit and ("found at " .. hit.x .. "," .. hit.y .. " " .. hit.w .. "x" .. hit.h) or "lost",
            region[1], region[2], region[3], region[4]
          ))
          self:_recheck()
        end
      end)
    end
    return matched
  end)
end

local TYPE_LABEL =
  { static = "", hotspot = "button", custom = "button", ocr = "button", ocredit = "edit",
    gtoggle = "toggle button", hotspottoggle = "toggle button", tabcontrol = "tab control",
    slider = "slider", passthrough = "" }

-- Origin (top-left) of the overlay's coordinate space in screen pixels: the
-- client-area top-left of the active context's coordinate window — the plugin
-- window itself when standalone, or the embedded plugin's child control when
-- hosted in a DAW. Re-resolved per call so it tracks window moves. (0, 0) when
-- unattached. All control coordinates (OCR regions, hotspots) are relative to it.
local function originOf(self, control)
  if self.activeCtx then
    local o = self:origin()
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
  self:_syncNativeKeys()
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

-- A hotspot's point has to land inside the origin it was measured against. Outside it,
-- there are only other windows' pixels — the DAW's own chrome, another plugin, the
-- desktop — and a click there does something arbitrary in an application the user was not
-- aiming at and cannot see. Live: Kontakt 7's multi arrows carried Kontakt 8's
-- coordinates, 704 px from the left of a window 649 px wide, so both fired past the
-- plugin's right edge into REAPER.
--
-- This is the one coordinate error that pressing the key again cannot undo, so it refuses
-- and says so rather than clicking. The refusal names the control: one that is present and
-- silently inert is the harder thing to make sense of without seeing the screen.
local function pointInOrigin(self, x, y)
  local o = self:origin()
  if not (o and o.client and o.bounds) then return false end
  return x >= o.client.x
    and x < o.client.x + o.bounds.w
    and y >= o.client.y
    and y < o.client.y + o.bounds.h
end

local function refuseOutside(self, c, x, y)
  if pointInOrigin(self, x, y) then return false end
  local o = self:origin()
  host.log.info(string.format(
    "%s: '%s' resolves to (%d,%d), outside its origin (%d,%d) %dx%d — not clicking",
    tostring(self.label), tostring(c.label), x, y,
    o and o.client and o.client.x or 0, o and o.client and o.client.y or 0,
    o and o.bounds and o.bounds.w or 0, o and o.bounds and o.bounds.h or 0
  ))
  host.speech.output((c.label or "Control") .. " is not where it should be", { interrupt = true })
  return true
end

-- A state may be drawn MORE THAN ONE WAY, so `onImage`/`offImage` take either one path or
-- a list of them. ReaHotkey carries two "off" variants for each button of Audio Imperia's
-- segmented mixers, and a toggle that only knows one of them reports no state at all in the
-- other — which for a screen-reader user is a control that has simply gone quiet.
local function asList(v)
  if v == nil then return {} end
  if type(v) == "table" then return v end
  return { v }
end
local function addTemplates(templates, labels, images, label)
  for _, path in ipairs(asList(images)) do
    templates[#templates + 1] = path
    labels[#labels + 1] = label
  end
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
    addTemplates(templates, labels, c.onImage, "on")
    addTemplates(templates, labels, c.offImage, "off")
    if #templates == 0 then return nil end
    local idx = host.screen.imageSearchMulti(templates, opts)
    return idx and labels[idx] or nil
  end
  -- Fallback for a host without imageSearchMulti: one capture per template.
  for _, t in ipairs(asList(c.onImage)) do
    if host.screen.imageSearch(t, opts) then return "on" end
  end
  for _, t in ipairs(asList(c.offImage)) do
    if host.screen.imageSearch(t, opts) then return "off" end
  end
  return nil
end

-- Where the slider's thumb is right now, in screen coordinates, or nil when it cannot be
-- found (the panel is not showing, or the plugin draws it differently than the template).
local function sliderThumb(self, c)
  if not (c.region and c.thumbImage) then return nil end
  local ox, oy = originOf(self, c)
  local _, hit = host.screen.imageSearchMulti(asList(c.thumbImage), {
    region = { ox + c.region[1], oy + c.region[2], ox + c.region[3], oy + c.region[4] },
    tolerance = c.tolerance or MATCH_TOLERANCE,
    scales = c.scales or MATCH_SCALES,
  })
  if not hit then return nil end
  return {
    cx = hit.x + math.floor(hit.w / 2),
    cy = hit.y + math.floor(hit.h / 2),
  }
end

-- The thumb's position as a percentage of the track, clamped. nil when the track is
-- degenerate (from == to) — better to say nothing than to announce a number from a
-- division that means nothing.
local function sliderPercent(self, c, thumb)
  local ox = originOf(self, c)
  local from, to = ox + c.from, ox + c.to
  if to <= from then return nil end
  local p = math.floor(((thumb.cx - from) / (to - from)) * 100 + 0.5)
  return math.max(0, math.min(100, p))
end

-- One arrow-key step. Drags by a percent of the track, then RE-READS: if the plugin did not
-- actually move, the target is extended a pixel at a time until it does, the track end is
-- reached, or the attempt budget runs out. Everything after the first drag runs on a timer,
-- so this never blocks the loop, and every continuation re-checks that the overlay is still
-- on the same window — a drag into whatever window arrived meanwhile is not recoverable.
local SLIDER_SETTLE, SLIDER_MAX_NUDGES = 25, 24
local function sliderStep(self, c, dir)
  local thumb = sliderThumb(self, c)
  if not thumb then
    host.speech.output((c.label or "") .. ", not found", { interrupt = true })
    return
  end
  local ox = originOf(self, c)
  local step = math.max(1, math.ceil((c.to - c.from) / 100))
  local startPct = sliderPercent(self, c, thumb)
  local pinned = self:hwnd()
  local target = thumb.cx + dir * step
  local nudges = 0
  local function attempt()
    if not self.active or self:hwnd() ~= pinned then return end
    local t = sliderThumb(self, c)
    if not t then return end
    if refuseOutside(self, c, target, t.cy) then return end
    host.input.drag(t.cx, t.cy, target, t.cy)
    host.timer.after(SLIDER_SETTLE, function()
      if not self.active or self:hwnd() ~= pinned then return end
      local after = sliderThumb(self, c)
      local pct = after and sliderPercent(self, c, after)
      local ended = pct == nil
        or (pct == 0 and dir < 0)
        or (pct == 100 and dir > 0)
        or pct ~= startPct
      if not ended and nudges < SLIDER_MAX_NUDGES then
        nudges += 1
        target += dir
        attempt()
        return
      end
      host.speech.output(
        pct and (tostring(pct) .. " percent") or ((c.label or "") .. ", not found"),
        { interrupt = true }
      )
    end)
  end
  attempt()
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
    -- Measured from the right edge of the CONTENT, not of the window frame. They are the
    -- same thing for an embedded plugin (its control is borderless) and not for a
    -- standalone one: a Kontakt 8 window is 1026 wide around 1010 of plugin, so every
    -- right-anchored control would sit 16 px too far right there.
    x = O.contentSize(self:origin()) - at[1]
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
  -- A pass-through announces that Tab goes INTO the plugin from here, because nothing else
  -- would tell the user that: what follows is the plugin's own focus ring, announced by
  -- the screen reader rather than by us, and it is not obvious that Tab will come back out
  -- again at the end. Landing on it also restarts the ring, so entering it, leaving by
  -- hotkey and coming back walks the whole thing again rather than resuming mid-way.
  if c.kind == "passthrough" then
    c._entry = nil
    host.speech.output((c.label or "") .. ", Tab to step through them", { interrupt = true })
    return
  end
  -- `ocrLabel`: read the control's NAME off the screen instead of announcing a fixed one,
  -- for a control whose meaning the plugin itself changes. A sample library can put a mixer
  -- strip in one place whose five channels are mic positions for one patch and orchestral
  -- sections for another — same buttons, same pixels, different names. A fixed label is
  -- then confidently wrong, which is worse than slow: it tells a user who cannot see the
  -- screen that they toggled something they did not. Falls back to the static label when
  -- OCR reads nothing, so a failed read never leaves a control nameless.
  local label = c.label or ""
  if c.ocrLabel then
    local ox, oy = originOf(self, c)
    local r = host.ocr.recognize({
      region = {
        ox + c.ocrLabel[1], oy + c.ocrLabel[2],
        ox + c.ocrLabel[3], oy + c.ocrLabel[4],
      },
    })
    if r and r.text and r.text ~= "" then label = r.text end
  end
  local parts = { label }
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
  -- A slider's value IS its thumb position, so it is read on focus like a toggle's state.
  -- "not found" rather than a number when the thumb cannot be located: a percentage
  -- invented from a failed search is worse than an admission, because it sounds right.
  if c.kind == "slider" then
    local thumb = sliderThumb(self, c)
    local pct = thumb and sliderPercent(self, c, thumb)
    parts[#parts + 1] = pct and (tostring(pct) .. " percent") or "not found"
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
  if c == nil then return false end
  if not isVisible(self, c) then return false end
  return true
end

function Overlay:focusNext()
  if self:_stepPassThrough(1) then return end
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
    self:_syncNativeKeys()
    speakControl(self, self.focus, true)
  end
end

function Overlay:focusPrev()
  if self:_stepPassThrough(-1) then return end
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
    self:_syncNativeKeys()
    speakControl(self, self.focus, true)
  end
end

-- ReaHotkey's AddCustomPassThrough, as what it actually is there: a CONTROL in the
-- overlay's own Tab ring, not a mode that replaces it.
--
-- We had it as a mode, and it was fine while the overlay it sat on carried exactly one
-- other control. The moment the standalone header gained the rack controls, they were
-- present and unreachable — Tab went straight into the plugin and never came back to
-- them — which for someone who cannot see the screen is the same as their not existing.
-- A control you cannot find is a control you do not have.
--
-- As a control: Tab reaches it like any other and announces it; Tab again enters the
-- plugin's OWN focus ring and walks it (host.uia.focusStep, which raises a real UIA focus
-- event, so the screen reader announces each element and we deliberately do not); and
-- when that ring returns to where it was entered, Tab carries on through the rest of the
-- overlay's controls. For a plugin window that does not move focus on Tab by itself
-- (Kontakt standalone), this is the only way into its native controls.
function Overlay:addPassThrough(opts)
  return add(self, {
    kind = "passthrough",
    label = (opts and opts.label) or "Plugin controls",
    when = opts and opts.when,
  })
end

-- Kept for the old spelling; it now adds the control rather than switching on a mode.
function Overlay:enableUiaPassThrough(opts)
  assertUnbound(self, "enableUiaPassThrough")
  self:addPassThrough(opts)
  return self
end

-- One Tab inside the focused pass-through control. True when it consumed the key (focus
-- moved within the plugin), false to let Tab carry on through the overlay's controls.
--
-- focusStep WRAPS at the ends, so "we are done" is not something it reports; what it does
-- report is a 1-based index, and coming back to the index we entered at means the ring has
-- been walked once. Entering at whatever the plugin happens to have focused (rather than
-- at element 1) is what makes that the full ring rather than the tail of it.
function Overlay:_stepPassThrough(dir)
  local c = self.controls[self.focus]
  if not (c and c.kind == "passthrough" and isVisible(self, c)) then return false end
  local ctrl = self:origin()
  if not (ctrl and ctrl.id) then return false end
  local r = host.uia.focusStep(ctrl.id, dir)
  if not r then return false end -- nothing focusable in there: just move on
  if c._entry == nil then
    c._entry = r.index
    return true
  end
  if r.index == c._entry then
    c._entry = nil -- walked the ring; hand Tab back to the overlay
    return false
  end
  return true
end

function Overlay:activate(index)
  local i = index or self.focus
  local c = self.controls[i]
  if not c then return end
  if not isVisible(self, c) then return end -- a hidden control's hotkey no-ops
  if c.kind == "hotspot" and c.points then
    -- Acknowledge at PRESS time, like every other control. Speaking after the last click
    -- instead would land hundreds of milliseconds late — indistinguishable from a key that
    -- did not register — and, with interrupt, would cut off the announcement of whatever
    -- that final click opened. The last click is the one that usually opens a dialog, so
    -- its announcement is the one the user actually needs.
    host.speech.output((c.label or "") .. ", activated", { interrupt = true })
    -- Each step re-resolves the origin (an earlier click may have moved what we aim at)
    -- and goes through the overlay's own coordinate resolution, so a frame offset,
    -- `rawOrigin` or landmark anchoring applies exactly as for a single-point hotspot.
    --
    -- But a continuation runs LATER, and by then the overlay may have lost its window:
    -- originOf answers (0, 0) with no active context, which would fire the remaining
    -- clicks at raw screen coordinates on whatever window is now in front. So each step
    -- re-checks that the overlay is still active on the SAME window it started on, and
    -- abandons the sequence otherwise — a half-finished sequence is recoverable, a click
    -- into an unknown window is not.
    local settle = c.settle or 400
    local pinned = self:hwnd()
    local function step(k)
      local p = c.points[k]
      if not p then return end
      if not self.active or self:hwnd() ~= pinned then return end
      local ox, oy = originOf(self, c)
      if refuseOutside(self, c, ox + p[1], oy + p[2]) then return end
      host.input.click(ox + p[1], oy + p[2])
      -- No trailing delay after the last click: there is nothing left to settle for.
      if c.points[k + 1] then host.timer.after(settle, function() step(k + 1) end) end
    end
    step(1)
  elseif c.kind == "hotspot" and c.at then
    local px, py = hotspotPoint(self, c)
    if not px then return end
    if refuseOutside(self, c, px, py) then return end
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
    if isVisible(self, c) then return i end
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
  local o = self:origin()
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
  -- Calibration keys, only in a calibrating run (see the section above). Captured, so
  -- they act on the overlay that is active — the one that knows its own coordinates.
  if host.calibrating then
    -- Say so once per VM: whether the flag took is otherwise invisible, and "I pressed
    -- the key and nothing happened" has two very different causes.
    if not calibrationAnnounced then
      calibrationAnnounced = true
      host.log.info(
        "[calibrate] armed — Ctrl+Alt+Shift+S shot, +T template, +V verify (active overlay)"
      )
    end
    -- NOT the Ctrl+Alt+F-keys: graphics drivers commonly claim those system-wide, and a
    -- key another process holds never reaches our hook at all.
    cap("Ctrl+Alt+Shift+S", function() self:calibrationShot() end)
    cap("Ctrl+Alt+Shift+T", function() self:calibrationCrop() end)
    cap("Ctrl+Alt+Shift+V", function() self:calibrationVerify() end)
  end
  -- Tab moves *between* elements; arrow keys are left for value changes within a
  -- control (standard interaction scheme), so we don't capture them here.
  do
    cap("Tab", function() self:focusNext() end)
    cap("Shift+Tab", function() self:focusPrev() end)
    -- Return is captured per FOCUS, not statically: while the pass-through control has
    -- the focus, the plugin's own control is what should receive it (open a combo, toggle
    -- a check box). Same reasoning as Space below. See _syncNativeKeys.
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
    -- Space and Return activate the focused control (standard Windows button
    -- behaviour). They are captured/released as focus moves — NOT via the static `cap`
    -- set above — so that while an editable field (ocredit) is focused Space reaches the
    -- field and the user can type a literal space (e.g. a preset name with spaces), and
    -- while a pass-through control is focused BOTH reach the plugin's real control. Set
    -- the initial state for the control focused right now.
    self:_syncNativeKeys()
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
  if self._returnToken then
    host.keys.release(self._returnToken)
    self._returnToken = false
  end
  if self._arrowTokens then
    for _, t in ipairs(self._arrowTokens) do host.keys.release(t) end
    self._arrowTokens = false
  end
end

-- Keeps the two keys that ACTIVATE a control — Space and Return — in sync with what is
-- focused, because for two kinds of control they belong to something else:
--
--   ocredit       an editable field: Space must reach it, or the user cannot type a space
--                 in a preset name. Return still activates.
--   passthrough   the plugin's OWN control has the real keyboard focus, so both belong to
--                 it — Space toggles its check box, Return opens its combo. Capturing
--                 either here would swallow the interaction the pass-through exists for.
--
-- Idempotent; called on every focus move and at registration. No-op unless active.
function Overlay:_syncNativeKeys()
  if not self.active then return end
  local c = self.controls[self.focus]
  local passthrough = c ~= nil and c.kind == "passthrough"
  local wantSpace = not passthrough and not (c ~= nil and c.kind == "ocredit")
  local wantReturn = not passthrough
  if wantSpace and not self._spaceToken then
    self._spaceToken = host.keys.capture("Space", function() self:activate() end)
  elseif not wantSpace and self._spaceToken then
    host.keys.release(self._spaceToken)
    self._spaceToken = false
  end
  if wantReturn and not self._returnToken then
    self._returnToken = host.keys.capture("Return", function() self:activate() end)
  elseif not wantReturn and self._returnToken then
    host.keys.release(self._returnToken)
    self._returnToken = false
  end
  -- Left/Right belong to a focused SLIDER, and only then. Arrows are otherwise left alone
  -- on purpose (the standard scheme keeps them for value changes inside whatever control
  -- has focus), which is exactly what this is — the overlay's slider IS that control.
  -- Skipped when the overlay also has a tab control, which claims Left/Right statically;
  -- the declaration report says so out loud rather than leaving a slider silently inert.
  local wantArrows = c ~= nil and c.kind == "slider" and not self._hasTabControl
  if wantArrows and not self._arrowTokens then
    self._arrowTokens = {
      host.keys.capture("Left", function() self:adjust(-1) end),
      host.keys.capture("Right", function() self:adjust(1) end),
    }
  elseif not wantArrows and self._arrowTokens then
    for _, t in ipairs(self._arrowTokens) do host.keys.release(t) end
    self._arrowTokens = false
  end
end

-- One step of the focused control's value, from Left/Right. Only a slider has one today.
function Overlay:adjust(dir)
  local c = self.controls[self.focus]
  if c and c.kind == "slider" and isVisible(self, c) then sliderStep(self, c, dir) end
end

-- Deprecated name; the capture covers Return as well now.
function Overlay:_syncSpaceCapture()
  return self:_syncNativeKeys()
end

-- A module hook fired (with the overlay) each time it becomes the ACTIVE overlay —
-- runs after the activation context is set, so origin()/pluginHwnd are available.
function Overlay:onActivate(fn)
  assertUnbound(self, "onActivate")
  self._onActivate = fn
  return self
end

-- A module hook fired (with the overlay) each time it stops being the active overlay —
-- the counterpart to onActivate. Used e.g. by the Komplete Kontrol scanning dialog to
-- announce "scanning stopped" (and let its progress loop wind down) when it closes.
function Overlay:onDeactivate(fn)
  assertUnbound(self, "onDeactivate")
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
  local origin = self:origin()
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
    matched = self._gate(self:origin()) == true
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
    local o = self:origin()
    if ((o and o.id) or false) ~= self._hotkeyOriginId then
      self:_syncControlHotkeys()
    end
  end
end

-- ── Declaration checks ───────────────────────────────────────────────────────
-- Everything about this system fails SILENTLY. A control with no coordinates simply
-- never clicks anything; a relative image path resolves under the wrong module and the
-- landmark never matches, so the overlay never appears; a specificity passed without a
-- slot is quietly ignored. None of it errors, and the person who would notice cannot see
-- the screen. So whatever can be decided when an overlay is DECLARED is decided then, and
-- said out loud with the control's name in it.
--
-- Reported, not raised: a finding here may be deliberate (a label-less spacer), and a
-- module that mostly works should keep working. Order violations below are different.
-- Spelled out with sub() rather than a pattern: a character class containing a backslash
-- is exactly the kind of escaping that silently degrades into something that matches
-- nothing, and a path check that always says "no" would report every correct module.
local function isAbsolutePath(p)
  if type(p) ~= "string" then return false end
  local drive, sep = string.match(p, "^%a:"), string.sub(p, 3, 3)
  if drive and (sep == "/" or sep == "\\") then return true end -- C:\… or C:/…
  if string.sub(p, 1, 2) == "\\\\" then return true end -- \\server\share
  return string.sub(p, 1, 1) == "/" -- POSIX
end

local NEEDS_REGION = { ocr = true, ocredit = true, gtoggle = true }

local function report(self, opts)
  local found = {}
  local function bad(fmt, ...)
    found[#found + 1] = string.format(fmt, ...)
  end

  for i, c in ipairs(self.controls) do
    local who = string.format("control %d (%s '%s')", i, tostring(c.kind), tostring(c.label or ""))
    if c.label == nil or c.label == "" then
      if c.kind ~= "tabcontrol" then bad("%s has no label — it will be announced as nothing", who) end
    end
    if c.kind == "hotspot" and c.at == nil and c.points == nil then
      bad("%s has neither `at` nor `points` — activating it does nothing", who)
    end
    if NEEDS_REGION[c.kind] and c.region == nil then
      bad("%s has no `region` — there is nothing for it to read", who)
    end
    if c.kind == "hotspottoggle" then
      if c.at == nil then bad("%s has no `at`", who) end
      if c.onColor == nil or c.offColor == nil then
        bad("%s needs both `onColor` and `offColor` to tell its state apart", who)
      end
    end
    if c.kind == "gtoggle" and c.onImage == nil and c.offImage == nil then
      bad("%s has neither `onImage` nor `offImage`", who)
    end
    if c.kind == "slider" then
      if c.region == nil then bad("%s has no `region` to look for its thumb in", who) end
      if c.thumbImage == nil then bad("%s has no `thumbImage`", who) end
      if c.from == nil or c.to == nil then
        bad("%s needs `from` and `to` — the x that reads 0 %% and the x that reads 100 %%", who)
      elseif c.to <= c.from then
        bad("%s: `to` (%s) is not right of `from` (%s), so its value cannot be computed",
          who, tostring(c.to), tostring(c.from))
      end
      for _, path in ipairs(asList(c.thumbImage)) do
        if not isAbsolutePath(path) then
          bad("%s: `thumbImage` is not an absolute path — use host.path()", who)
        end
      end
      if self._hasTabControl then
        bad("%s is a slider on an overlay that also has a tab control, which claims "
          .. "Left/Right — the slider's arrows will not work here", who)
      end
    end
    for _, field in ipairs({ "onImage", "offImage" }) do
      -- Either may be a LIST of variants (see asList), so every entry is checked.
      for _, path in ipairs(asList(c[field])) do
        if not isAbsolutePath(path) then
          bad("%s: `%s` is not an absolute path — use host.path(); a relative one resolves "
            .. "under whichever module is RUNNING, which for an inherited overlay is not yours",
            who, field)
        end
      end
    end
    if c.when ~= nil and type(c.when) ~= "function" then
      bad("%s: `when` must be a function, got %s", who, type(c.when))
    end
    if c.hotkey ~= nil and type(c.hotkey) ~= "string" then
      bad("%s: `hotkey` must be a string like \"Alt+P\", got %s", who, type(c.hotkey))
    end
  end

  if self._landmarkImage ~= nil and not isAbsolutePath(self._landmarkImage) then
    bad("landmark image is not an absolute path — use host.path()")
  end
  if #self.controls == 0 then
    bad("no controls and no pass-through — this overlay has nothing to offer")
  end
  if opts and opts.specificity ~= nil and not opts.slot then
    bad("`specificity` was given without a `slot`, so it is ignored — overlays only rank "
      .. "against each other within a slot")
  end

  for _, msg in ipairs(found) do
    host.log.info(string.format("[overlay] %s: %s", tostring(self.label), msg))
  end
  return #found
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
  report(self, { slot = slot, specificity = specificity })
  -- From here on the overlay is LIVE: _join ends with a synchronous _recheck, so anything
  -- that changes what the overlay is must already be in place. See the guard below.
  self._bound = true
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
      -- Log the slot's roster as it grows. A slot is a plain string typed independently
      -- in modules that never see each other, and a typo does not error — it quietly
      -- creates a private slot where this overlay wins every time, or never competes.
      -- A slot that stays at one participant while its sibling shows several is the only
      -- visible symptom, so make it visible.
      if host.arbiter.participants then
        local names = {}
        for _, p in ipairs(host.arbiter.participants(self._slot)) do
          names[#names + 1] = string.format("%s(%d)", p.module, p.specificity)
        end
        host.log.info(string.format(
          "arbiter slot '%s': %d participant(s) — %s",
          self._slot, #names, table.concat(names, ", ")
        ))
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
    -- `menus = true`: hand the captured navigation keys to a plugin menu while one is
    -- open (see watchMenus). Any overlay whose controls can open a plugin menu needs it.
    if opts.menus then self:watchMenus() end
    self:_join(opts.slot, opts.specificity, opts.pollMatch)
  else
    self:_join(nil, nil, nil)
  end
end

-- Memoizes `fn(origin, …) -> value` per origin window, for a property of a window that
-- does not change while that window exists: which plugin version it is, whether it is a
-- host container, where its content begins.
--
-- One rule, because the modules doing this by hand each invented their own and each got a
-- different corner wrong: a NON-NIL result is cached, and `nil` means "could not tell
-- yet" and is NOT cached. Identity here is resolved through UIA, which is not ready the
-- instant a plugin window appears — pinning that first answer is how an overlay ends up
-- permanently convinced a Kontakt 8 is a Kontakt 7. `false` is a real answer and is
-- cached; only `nil` is transient.
--
-- `opts.key(origin)` extends the cache key beyond the window handle, for a property that
-- survives a window MOVE but not a RESIZE (a host that repositions its embedded plugin).
function O.memoByOrigin(fn, opts)
  local cache = {}
  local keyOf = opts and opts.key
  return function(origin, ...)
    if not (origin and origin.id) then return fn(origin, ...) end
    local k = origin.id
    if keyOf then k = tostring(k) .. "|" .. tostring(keyOf(origin)) end
    local hit = cache[k]
    if hit ~= nil then return hit end
    local v = fn(origin, ...)
    if v ~= nil then cache[k] = v end
    return v
  end
end

-- ── Bindings ─────────────────────────────────────────────────────────────────
-- A BINDING is an inert value saying WHERE an overlay lives: which window or embedded
-- control it attaches to, on which slot, at which layer. Being a value rather than a
-- call, it can be named once and reused — `:with{…}` derives a variant without touching
-- the original. Modules were writing a `spec()` FUNCTION for exactly this, because a
-- caller mutated the table it returned and the next caller needed a clean copy.
--
-- One overlay object takes ONE binding: `_join` pins an object to a single slot, and its
-- match poll is registered with that join. Two places to live means two overlay objects
-- (share a build function), not two bindings.
local Binding = {}
Binding.__index = Binding

local function binding(kind, target, opts)
  return setmetatable({ _bind = kind, target = target, opts = opts or {} }, Binding)
end

-- A top-level window (see host.window matchers).
function O.window(matcher, opts)
  return binding("window", matcher, opts)
end

-- A plugin control embedded in a host window (see attachEmbedded's spec).
function O.embedded(spec, opts)
  return binding("embedded", spec, opts)
end

-- A copy with `over` merged into the target (control, identify, title, …); `over.opts`
-- merges into the options (slot, specificity, pollMatch, menus).
function Binding:with(over)
  local target = {}
  for k, v in pairs(self.target) do target[k] = v end
  local opts = {}
  for k, v in pairs(self.opts) do opts[k] = v end
  for k, v in pairs(over or {}) do
    if k == "opts" then
      for ok, ov in pairs(v) do opts[ok] = ov end
    else
      target[k] = v
    end
  end
  return binding(self._bind, target, opts)
end

-- Concatenates host lists, skipping nils — so an OPTIONAL host (a plugin that may not be
-- installed) can be listed inline instead of appended by a loop.
function O.hosts(...)
  local out = {}
  -- Counted explicitly: `ipairs` would stop at the FIRST nil, so an absent host in any
  -- position but the last would silently swallow every host after it.
  for i = 1, select("#", ...) do
    local list = select(i, ...)
    if type(list) == "table" then
      if list.title or list.app or list.windows then
        out[#out + 1] = list -- a single matcher, not a list of them
      else
        for _, h in ipairs(list) do out[#out + 1] = h end
      end
    end
  end
  return out
end

-- Attaches the overlay according to `b`, with `opts` overriding the binding's own.
function Overlay:bind(b, opts)
  local merged = b.opts
  if opts then
    merged = {}
    for k, v in pairs(b.opts) do merged[k] = v end
    for k, v in pairs(opts) do merged[k] = v end
  end
  if b._bind == "embedded" then
    self:attachEmbedded(b.target, merged)
  else
    self:attach(b.target, merged)
  end
  return self
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
  -- Resolving the origin is the most expensive thing this runtime does repeatedly: a
  -- window enumeration, a focus-chain walk, and — for a relational spec like Kontakt's,
  -- which cannot cache its identity per HWND — a UIA query per candidate control, which
  -- itself enumerates again. And it is asked for constantly: once per control coordinate,
  -- again for a right-anchored hotspot, on every hotkey re-sync, on activation, and every
  -- 150 ms from the menu watch. All of those want the SAME answer.
  --
  -- So memoize it against host.epoch(), which changes whenever an OS event reaches a
  -- module, a timer fires, or the module drives input — every point at which the answer
  -- could differ. Within one such pass the origin is resolved once; a new pass always
  -- re-resolves. Not a time-based cache: a stale origin means clicking the wrong place.
  local cachedEpoch, cachedOrigin = -1, nil
  local function origin(opts)
    local e = host.epoch and host.epoch() or -1
    if not (opts and opts.fresh) and e == cachedEpoch then return cachedOrigin end
    cachedOrigin = pluginControl()
    cachedEpoch = e
    return cachedOrigin
  end
  -- The match asks the same question ("is the plugin focused?") and so shares the memo:
  -- a recheck plus the activation and coordinate work that follows it resolve once
  -- between them, where each used to enumerate the window afresh.
  local function matched()
    local c = origin()
    if not c then return false end
    if spec.present and not spec.present(c) then return false end
    return true
  end
  self.contexts[#self.contexts + 1] = { match = matched, origin = origin }
  ensureTrigger(self, opts)
end

-- Watch for a MENU opening inside the plugin and tell the key hook to let the captured
-- navigation keys pass through to it — a plugin's own (Qt) menu is not a Win32 #32768
-- menu, so the hook cannot see it by itself. Without this, opening a plugin menu from an
-- overlay control leaves the user unable to move in it: the overlay keeps swallowing Tab,
-- Return and the arrows.
--
-- ONE watch per VM, not one per overlay. Every overlay needed this and it was a private
-- method each module had to remember to call — sforzando and overlay-attach do not, and
-- silently swallow keys as a result. Opt in with `menus = true` on attach.
--
-- `host.keys.menuOpen` is a process-wide flag, so it is only written while an opted-in
-- overlay is actually active; otherwise this would clobber the state of whichever overlay
-- is live elsewhere.
local menuWatched = {}
local menuTimer = false
function Overlay:watchMenus()
  for _, o in ipairs(menuWatched) do
    if o == self then return self end
  end
  menuWatched[#menuWatched + 1] = self
  if menuTimer then return self end
  menuTimer = true
  -- Disable-safe recurring poll (survives a disable/enable cycle, unlike a Lua
  -- self-rescheduling host.timer.after).
  host.timer.every(150, function()
    local anyActive, open = false, false
    for _, o in ipairs(menuWatched) do
      if o.active then
        anyActive = true
        local ctrl = o:origin()
        if ctrl ~= nil and ctrl.id ~= nil
          and host.uia.find(ctrl.id, "", host.uia.type.Menu) == true
        then
          open = true
          break
        end
      end
    end
    if anyActive then host.keys.menuOpen(open) end
  end)
  return self
end

-- Deprecated name kept so existing modules keep loading; `menus = true` on attach is the
-- supported way in.
function Overlay:_startMenuWatch()
  return self:watchMenus()
end

-- ── Calibration ──────────────────────────────────────────────────────────────
-- Armed only when the app runs with AUTOMATION_PLATFORM_CALIBRATE=1, and captured (not
-- registered as global hotkeys) so the keys belong to whichever overlay is ACTIVE — which
-- is also the only overlay that can answer where its controls land, since the coordinate
-- frame (origin, frame offset, rawOrigin, landmark anchoring) is its own. A separate tool
-- would have to reimplement that resolution, and would then be measuring something other
-- than what the overlay clicks.
--
--   Ctrl+Alt+Shift+S  a screenshot of the coordinate window with a CROSSHAIR on every
--                     control where it will actually click, plus the pixel read there
--   Ctrl+Alt+Shift+T  crop a template around the focused control, straight into the module
--   Ctrl+Alt+Shift+V  count every match of the focused control's template in the region
--
-- Written into `calibration/` under the module's own directory.

-- The point a control resolves to right now, or nil for one that has no point (a
-- region-based control reports its centre instead).
local function calibrationPoint(self, c)
  if c.at then return hotspotPoint(self, c) end
  -- A SLIDER's region is the whole track it searches, so its centre is a point the control
  -- has nothing to do with — it marked bare track and read the background colour, which
  -- reads in the log exactly like a mis-measured control. Mark the thumb where it actually
  -- is; fall through to the region centre only when it cannot be found, which is itself
  -- the useful thing to see.
  if c.kind == "slider" then
    local thumb = sliderThumb(self, c)
    if thumb then return thumb.cx, thumb.cy end
  end
  if c.region then
    local ox, oy = originOf(self, c)
    return ox + math.floor((c.region[1] + c.region[3]) / 2),
      oy + math.floor((c.region[2] + c.region[4]) / 2)
  end
  return nil
end

-- A label as a file name: overlays are named after plugins and libraries, which contain
-- spaces and dashes.
local function safeName(label)
  return (string.gsub(tostring(label or "overlay"), "[^%w]+", "-"))
end

-- Shots of the SAME overlay no longer overwrite each other: the second gets "-2", the
-- third "-3". Measuring a library that has no module yet is done from the plain Kontakt
-- header, so every product in a row lands under the same label — and the last one silently
-- replaced the evidence for all the others. It cost an hour once: two products' shots
-- collapsed into one file and the wrong one got measured. The log prints the exact path it
-- wrote, so nothing has to be guessed from a directory listing.
local shotsTaken = {}
local function shotPath(label)
  local base = safeName(label)
  local n = (shotsTaken[base] or 0) + 1
  shotsTaken[base] = n
  return base .. (n > 1 and ("-" .. n) or "")
end

-- WHICH control the overlay took as its origin, and what else was on offer.
--
-- An overlay whose readings are all wrong is nearly always a correct overlay on a WRONG
-- ORIGIN, and that is invisible in the list below: every coordinate is then computed
-- faithfully against the wrong rectangle, so the numbers look reasonable and the picture
-- shows a plausible window. Live, a Kontakt 7 header attached itself to Kontakt's own
-- "Content Missing" dialog — same Qt window class, same "Kontakt 7" in its UIA subtree,
-- 600x440 instead of the plugin's 649x657 — and nothing in the report said so.
--
-- attachEmbedded takes the FIRST enumerated control that matches the class pattern and
-- passes identify, with no tie-break, so the roster is exactly the decision it made: the
-- window it enumerated, every control that was a candidate, and which one won.
local function calibrationContext(origin)
  local lines = {}
  local a = host.window.active()
  if a and a.bounds then
    lines[#lines + 1] = string.format(
      "  window  \"%s\"  class %s  exe %s  bounds (%d,%d) %dx%d",
      tostring(a.title), tostring(a.class), tostring(a.app and a.app.exe or "?"),
      a.bounds.x, a.bounds.y, a.bounds.w, a.bounds.h
    )
  end
  local controls = host.window.controls()
  lines[#lines + 1] = string.format("  controls (%d, in enumeration order):", #controls)
  for _, c in ipairs(controls) do
    local b = c.bounds
    lines[#lines + 1] = string.format(
      "   %s %-52s id=%d  (%d,%d) %dx%d",
      (origin and c.id == origin.id) and "->" or "  ",
      tostring(c.class), c.id,
      b and b.x or 0, b and b.y or 0, b and b.w or 0, b and b.h or 0
    )
  end
  return lines
end

-- The named elements in the origin's own accessibility tree, over the RAW walker (the one
-- that crosses into a hosted plugin's Qt fragment). This is how you find out what a window
-- IS when its class and size cannot tell it apart from another — and what to key a
-- discriminator on. Bounded: a plugin's tree runs to hundreds of nodes, and this is a
-- manual action, not something worth flooding the log for.
local UIA_DUMP_MAX = 60
local function calibrationUia(origin)
  local lines, n = {}, 0
  for _, e in ipairs(host.uia.rawDump(origin.id) or {}) do
    if e.name ~= nil and e.name ~= "" then
      n += 1
      if n > UIA_DUMP_MAX then
        lines[#lines + 1] = "    … (more, truncated)"
        break
      end
      lines[#lines + 1] = string.format(
        "    %s%s  [%s]", string.rep("  ", e.depth or 0), tostring(e.name), tostring(e.ctype)
      )
    end
  end
  if n == 0 then lines[#lines + 1] = "    (no named elements — this window exposes no UIA)" end
  table.insert(lines, 1, "  origin accessibility tree (named elements, raw walker):")
  return lines
end

function Overlay:calibrationShot()
  local o = self:origin()
  if not (o and o.client and o.bounds) then
    host.log.info("[calibrate] no active origin")
    return false
  end
  local marks, lines = {}, {}
  for _, c in ipairs(self.controls) do
    -- A control that is currently HIDDEN gets no crosshair: drawing one would say the user
    -- can reach something they cannot, which is the opposite of what this picture is for.
    -- It is still listed, marked, so its absence is visible rather than mysterious.
    if not isVisible(self, c) then
      lines[#lines + 1] = string.format("  -- %-24s hidden (its `when` is false)", tostring(c.label))
      continue
    end
    local x, y = calibrationPoint(self, c)
    if x then
      marks[#marks + 1] = { x = x, y = y }
      local px = host.screen.pixel(x, y)
      lines[#lines + 1] = string.format(
        "  %2d %-24s screen (%d,%d)  pixel %s%s",
        #marks, tostring(c.label), x, y,
        px and string.format("%d,%d,%d", px.r, px.g, px.b) or "?",
        c.rawOrigin and "  [rawOrigin]" or ""
      )
    else
      lines[#lines + 1] = string.format("  -- %-24s no position", tostring(c.label))
    end
  end
  -- Written under the OVERLAY RUNTIME's own directory, not the module whose overlay this
  -- is: host.path resolves against the module that DEFINES the running code, which here is
  -- the runtime. That rule is what makes an inherited overlay find its own images, so it
  -- stays -- calibration output just gets one known home instead of a per-module one. The
  -- log line prints the absolute path, and the file is named after the overlay.
  -- The CONTENT rectangle, which is the one the controls are measured against — using the
  -- window rect instead made a standalone shot overshoot by its border and include a strip
  -- of whatever was behind it, so a picture meant to settle where a coordinate lands
  -- disagreed with the coordinate by 16 px.
  local ow, oh = O.contentSize(o)
  local region = { o.client.x, o.client.y, o.client.x + ow, o.client.y + oh }
  local stem = shotPath(self.label)
  local path = host.path("calibration/" .. stem .. ".png")
  local ok = host.screen.saveMarked(path, { region = region, marks = marks })
  -- The SAME frame without the crosshairs. They are drawn over the very thing a control
  -- points at, which is fine for a hotspot and destroys the evidence for an image-matched
  -- one: measuring why a slider's thumb had stopped matching meant reading its knob through
  -- 32 pixels of magenta. Cheap — one more capture of a region already known — and it is
  -- also the frame a template gets cropped from.
  host.screen.saveMarked(host.path("calibration/" .. stem .. "-clean.png"), {
    region = region,
    marks = {},
  })
  host.log.info(string.format(
    "[calibrate] %s — origin (%d,%d) %dx%d%s -> %s (%s)",
    tostring(self.label), o.client.x, o.client.y, o.bounds.w, o.bounds.h,
    self._landmarkHit
      and string.format("  landmark at (%d,%d)", self._landmarkHit.x, self._landmarkHit.y)
      or "",
    path, tostring(ok)
  ))
  for _, l in ipairs(calibrationContext(o)) do host.log.info("[calibrate]" .. l) end
  for _, l in ipairs(calibrationUia(o)) do host.log.info("[calibrate]" .. l) end
  for _, l in ipairs(lines) do host.log.info("[calibrate]" .. l) end
  return ok == true
end

-- Crops `size` pixels around the FOCUSED control and writes it as a template. The size a
-- template wants is the thing that distinguishes it plus a little of its surroundings —
-- too tight matches everywhere, too loose breaks when anything near it changes.
function Overlay:calibrationCrop(size)
  local c = self.controls[self.focus]
  if not c then return false end
  local x, y = calibrationPoint(self, c)
  if not x then
    host.log.info("[calibrate] focused control has no position to crop around")
    return false
  end
  local half = math.floor((size or 28) / 2)
  local name = string.gsub(tostring(c.label or "control"), "[^%w]", "-")
  local path = host.path("calibration/" .. name .. ".png")
  local ok = host.screen.save(path, { region = { x - half, y - half, x + half, y + half } })
  host.log.info(string.format(
    "[calibrate] cropped %s around (%d,%d) -> %s (%s)", tostring(c.label), x, y, path, tostring(ok)
  ))
  return ok == true
end

-- Counts every match of the focused control's template across the coordinate window.
-- Exactly one is what you want before clicking it blindly; two means it will eventually
-- click the wrong one, and none means the control silently never fires.
function Overlay:calibrationVerify()
  local c = self.controls[self.focus]
  local o = self:origin()
  if not (c and o and o.client and o.bounds) then return false end
  local image = c.onImage or c.offImage
  if not image then
    local name = string.gsub(tostring(c.label or "control"), "[^%w]", "-")
    image = host.path("calibration/" .. name .. ".png")
  end
  local hits = host.screen.imageSearchAll(image, {
    region = { o.client.x, o.client.y, o.client.x + o.bounds.w, o.client.y + o.bounds.h },
    tolerance = 8,
  })
  host.log.info(string.format(
    "[calibrate] %s: %d match(es) for %s", tostring(c.label), #hits, tostring(image)
  ))
  for i, h in ipairs(hits) do
    host.log.info(string.format("[calibrate]   %d at (%d,%d) %dx%d", i, h.x, h.y, h.w, h.h))
  end
  return #hits
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
  local o = self:origin()
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
