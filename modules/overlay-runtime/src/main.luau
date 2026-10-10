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

-- How far apart the two halves of a DELIBERATE double-click are sent.
--
-- Not zero, which is the mistake worth naming. Two clicks sent back to back go out in the same
-- instant, and an application deciding whether it has seen a double-click is looking at the
-- interval between two presses — an interval of zero reads as easily as one press with a
-- stutter as it does as two. Ninety milliseconds is plainly two, and plainly inside any
-- double-click time a system would use: the Windows default is 500 ms and nothing sets it
-- below about 200.
local DOUBLE_CLICK_GAP = 90

-- A double-click, on purpose.
--
-- Worth having in one place because the same fact is needed from both sides. A plug-in that
-- resets a control to its default on a double-click is offering a genuinely useful gesture —
-- one keystroke instead of twenty steps — and reaching it means sending two clicks close
-- enough together. The same fact is a hazard everywhere else, which `Overlay:afterIdle` exists
-- to avoid; keeping both here means neither can drift away from the other's timing.
--
-- Falls back to two immediate clicks with no timer, which is not a double-click and says so in
-- the log rather than silently doing something else.
function O.doubleClick(x, y)
  host.input.click(x, y)
  if host.timer then
    host.timer.after(DOUBLE_CLICK_GAP, function() host.input.click(x, y) end)
  else
    host.log.info("overlay: no timer, so a double-click went out as two immediate clicks")
    host.input.click(x, y)
  end
end

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

-- Calibration, switched on under Installed → Overlay runtime → Settings…: this runtime's own
-- setting, one value for every module built on it (a code module's settings are stored under its
-- own id, whichever VM defines them). Held here, set by define at load and by the onChange below,
-- and never asked of host.settings.get, which raises in a dependent whose runtime failed to
-- reload. See O.calibrating and the Calibration section.
local calibrating = host.settings.define("calibrate", false, {
  label = "Calibration keys and pictures in overlays, for writing an overlay — takes effect immediately",
}) == true
-- One line per VM when the calibration keys arm (see the Calibration section).
local calibrationAnnounced = false

local Overlay = {}
Overlay.__index = Overlay

-- Every overlay this VM made, held weakly: which of them are active decides who sets back the
-- module's key scope and menu flag (see _deactivate).
local overlaysMade = setmetatable({}, { __mode = "k" })

-- Whether calibration is switched on: the calibration keys of the overlay in front, the menu shots
-- and the picture of a search that missed, and whatever a module built on the runtime measures
-- only for somebody writing an overlay. A read of the local above.
function O.calibrating()
  return calibrating
end

-- The dialog's change reaches every VM that carries the runtime, as a handler of its module. The
-- keys are taken when an overlay comes to the front, so one already in front keeps what it holds
-- until it leaves; the line says how many were, in a VM that made overlays.
host.settings.onChange("calibrate", function(new)
  new = new == true
  if new == calibrating then return end
  calibrating = new
  calibrationAnnounced = false
  local made, active = 0, 0
  for o in pairs(overlaysMade) do
    made += 1
    if o.active then active += 1 end
  end
  if made == 0 then return end
  local after
  if new then
    after = "the keys arm the next time an overlay comes to the front"
  elseif active > 0 then
    after = "the one in front keeps its keys until it leaves"
  else
    after = "no overlay holds the keys"
  end
  host.log.info(("[calibrate] %s — %d overlay(s) here, %d in front; %s"):format(
    new and "on" or "off", made, active, after))
end)

function O.new(label)
  local o = setmetatable(
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
      -- false (O:resume(false)): every activation starts at the first control, the same window
      -- or not — for a box that comes and goes inside one window, whose origin id never changes.
      _resume = true,
      -- How many times the overlay has come to the front: one more on every _activate, so a
      -- reply asked for in one stay in front can tell it is not in that stay any more.
      _stay = 0,
      -- How many of its own keys have reached it (ownKey), how many announcements it has begun
      -- (speakControl) and how many times a menu has opened over it (_menuChange): an answer to a
      -- read asked before any of them moved on is not spoken. And how many announcements it has
      -- actually said: one that waits for its read is begun long before it is heard.
      _keys = 0,
      _said = 0,
      _spoken = 0,
      _menuOpens = 0,
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
  overlaysMade[o] = true
  return o
end

-- Declaring something AFTER the overlay is bound is always a mistake, and a subtle one: the
-- binding makes it live. `_join` asks for its first `_recheck` then, on the module's next turn,
-- and from then on any window or focus event, poll or tick may evaluate it — so a gate or
-- landmark declared later, from a timer or another callback, is missing from every evaluation
-- before it, and the overlay can activate ungated meanwhile. Nothing in the code enforced the
-- order, and every call site happened to get it right — which is exactly the kind of rule that
-- holds until someone new writes the next module.
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

-- ── Where a hook runs ────────────────────────────────────────────────────────
-- Every callback of a module runs as a handler of the host, one at a time per module
-- (docs/module-runtime-and-lifecycle.md, "Each callback is a handler"), and a handler may stop at
-- a host wait — a read — while the rest of the application goes on; the module's next events wait
-- behind it. That is right for a hook that says or does something the user asked for: a control's
-- `text`, a tab control's `current` and `verticalName`, a stored `identify` asked for a new
-- control, a control's `onActivate` and `onStep`. It would be wrong for the hooks asked on EVERY
-- scan or tick — a `when`, a relational `identify`, a binding's `present`, a menu test: a read
-- there would keep the module busy all the time, and every key of the module would wait behind
-- its own Tab's scan. So those run in a coroutine of the runtime's own, where a host wait cannot
-- stop the handler: one coroutine for a whole pass over them — a scan for the next control, a
-- menu tick's questions — and each hook inline inside it. host.ocr.recognize without a callback
-- finds itself "in a coroutine the module made" there; with a callback, like a pixel, a snapshot
-- or an image search, it is fine. A hook that yields there anyway is ended, with an error the
-- caller's guard logs — a menu test's alone (see _menuAsk); a `when`'s ends the scan it was in.
--
-- The overlay's origin is resolved BEFORE a pass, outside it: a stored `identify` asked for a new
-- control runs in the handler, where it may wait, and inside the pass the origin comes from the
-- epoch's memo. One reached inside a pass all the same — another overlay's origin, asked by a
-- `when` — is not asked there at all (see attachEmbedded).
--
-- Not a host API: plain Luau. A hook asked outside any pass — a single `when` on a key press —
-- costs one coroutine of its own, as a whole pass does; `passesMade` counts them with the passes,
-- so a test can hold a scan to one.
local passDepth = 0
local passesMade = 0

-- Runs fn(...) in a pass of its own, even inside another: a coroutine of the runtime's, where a
-- host wait cannot stop the module's handler. For a hook known to yield (a menu test, see
-- _menuAsk), so that it fails alone rather than end the pass it was asked in.
local function ownPass(fn, ...)
  passDepth += 1
  passesMade += 1
  local co = coroutine.create(fn)
  local out = table.pack(coroutine.resume(co, ...))
  passDepth -= 1
  if coroutine.status(co) == "suspended" then
    coroutine.close(co)
    error("a hook asked on every scan yielded; it must return at once", 0)
  end
  if not out[1] then error(out[2], 0) end
  return table.unpack(out, 2, out.n)
end

-- Runs fn(...) where a host wait cannot stop the module's handler. Inline inside a pass.
local function noWait(fn, ...)
  if passDepth > 0 then return fn(...) end
  return ownPass(fn, ...)
end

-- Overlay `o`'s origin, resolved now, in the handler — unless a pass is open already, or the
-- overlay is not in front (nothing of it is asked then). Guarded: a raise here is raised again
-- where the origin is asked next, inside the hook that asks it, and logged there as it always was.
local function originFirst(o)
  if passDepth == 0 and o.active then pcall(o.origin, o) end
end

-- One pass over the hooks asked on every scan: the overlay's origin first, outside the pass,
-- then `fn(...)` in the pass.
local function scanPass(self, fn, ...)
  originFirst(self)
  return noWait(fn, ...)
end

-- For a test: how many coroutines the passes have made in this VM, and whether one is open now.
function O._passes()
  return passesMade, passDepth > 0
end

-- Is this control shown right now? A `when` predicate must return exactly `true`;
-- anything else — including a truthy non-boolean such as a `string.find` result — counts
-- as hidden. Every reader of `when` goes through here, because they must agree: one
-- place decides Tab-reachability (isFocusable), whether a hotkey press does anything
-- (activate), and which control owns a shared hotkey (_visibleClaimant). Disagreeing
-- would make a control silently unreachable while its key still fired something else.
--
-- Asked in a pass (above): inline inside one, and in a pass of its own outside one.
local function isVisible(self, c)
  if c == nil then return false end
  if c.when == nil then return true end
  -- A broken predicate hides ONE control, not the overlay.
  --
  -- This ran unprotected, and a module with a value used above its own declaration — a nil
  -- where a function should be — took the entire ring down with it: Tab stopped doing
  -- anything at all, in every control, with no announcement. For somebody who cannot see the
  -- screen that is indistinguishable from the application having died, and the actual mistake
  -- was one line in one control's `when`.
  --
  -- Hidden rather than shown, because a control whose visibility cannot be determined cannot
  -- promise anything either. Said once per control, so a predicate that fails on every scan
  -- does not bury the log it is trying to explain itself in. A `coroutine.yield` of its own in a
  -- `when` is ended by the pass, which raises (noWait): asked alone, that hides the control as a
  -- raise does; inside a scan the yield reaches past this guard, and the scan raises.
  local ok, v = pcall(scanPass, self, c.when, self)
  if not ok then
    if not c._whenBroke then
      c._whenBroke = true
      host.log.info(string.format("%s: '%s' — its `when` failed (%s); hiding just this control",
        tostring(self.label), tostring(c.label), tostring(v)))
    end
    return false
  end
  return v == true
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
  -- Its origin raised as it came to the front (see _activate): unknown for the rest of that epoch,
  -- as on a window whose handle the overlay does not know, so that the activation is not stopped
  -- half way by the same raise one call later.
  if self._originRaised ~= nil and host.epoch and self._originRaised == host.epoch() then return nil end
  return self.activeCtx and self.activeCtx.origin() or nil
end

function Overlay:hwnd()
  local o = self:origin()
  return o and o.id or nil
end

-- Whether an activation on the window the overlay was last active on resumes on the control the
-- user was on (the default: Alt+Tab out and back should not throw them to the start), or starts
-- at the first control as a new window does (`false`). A dialog drawn INSIDE a plug-in's window
-- — VPS Avenger's warning box — is the case for `false`: its origin is the plug-in's window every
-- time it appears, so the second box would come up on the button the first was answered with,
-- and a user who counts stops from the box's text would land on the other one.
function Overlay:resume(on)
  assertUnbound(self, "resume")
  self._resume = on ~= false
  return self
end

-- Shifts this overlay's whole coordinate frame: `fn(origin) -> dx, dy`, resolved per
-- active control (a host version's content shift, a nested plugin's inner origin). A second call
-- replaces the first.
function Overlay:frame(fn)
  assertUnbound(self, "frame")
  self._originOffset = fn
  return self
end

-- SCALES this overlay's authored coordinates: `fn(origin) -> factor | nil`, asked at every
-- resolution, and `opts.about = {x, y}` the authored point that stays where the frame puts it.
-- See the Scaling section below `originOf` for what it applies to, in which order, and why it is
-- a function rather than a design size.
function Overlay:scale(fn, opts)
  assertUnbound(self, "scale")
  if type(fn) ~= "function" then
    error(("overlay '%s': scale takes a function(origin) -> factor, got %s")
      :format(tostring(self.label), type(fn)), 2)
  end
  local about = opts and opts.about or { 0, 0 }
  if type(about) ~= "table" or type(about[1]) ~= "number" or type(about[2]) ~= "number" then
    error(("overlay '%s': scale's `about` must be {x, y}, got %s")
      :format(tostring(self.label), type(about)), 2)
  end
  self._scale = { fn = fn, about = { about[1], about[2] } }
  return self
end

function Overlay:addStaticText(labelOrOpts)
  if type(labelOrOpts) == "table" then
    return add(self, {
      kind = "static",
      label = labelOrOpts.label,
      -- `text = function(overlay) -> string`: a read-out rather than a caption. Appended to
      -- the label each time the control is announced, so a value the user wants to CHECK — the
      -- plug-in's current size, say — can be read by arriving at it, with nothing pressed and
      -- nothing changed. The alternative is a button, and a button that only reports is a
      -- promise of an action it does not perform.
      text = labelOrOpts.text,
      when = labelOrOpts.when,
      -- `hotkey`: a read-out the user checks often is worth a key, and pressing it says the
      -- control, since there is nothing to press. Space and Return stay the application's, as on
      -- any static text: Melodyne's "Analysis" sits beside its play/stop.
      hotkey = labelOrOpts.hotkey,
      hotkeyKeepsFocus = labelOrOpts.hotkeyKeepsFocus == true,
    })
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
    -- `typeLabel`: what to call it instead of "tab control". A row of tool icons navigated
    -- with Left/Right IS a tab control mechanically, and calling it one tells the user the
    -- wrong thing about what they are looking at.
    typeLabel = opts.typeLabel,
    -- `current = function(overlay) -> index|nil`: where the application actually is, consulted
    -- on arrival. Without it this control only knows what IT last did, which is wrong the
    -- moment the user changes the same setting by the app's own shortcut — Melodyne's tool bar
    -- has F1 to F6 for exactly that. Returning nil leaves the remembered position alone.
    current = opts.current,
    -- A SECOND AXIS. Left and Right choose among the tabs; Up and Down step something WITHIN
    -- the current one. Melodyne's tool bar is the case: each icon can stack extra tools behind
    -- a press-and-hold, so the variant is a property OF the tool rather than a control beside
    -- it, and putting it on a stop of its own said otherwise.
    --
    --   verticalStep = function(overlay, tabIndex, dir) -> string|nil
    --       performs the change and returns what to announce; nil declines — no second axis
    --       here, or already at the end of it.
    --   verticalName = function(overlay, tabIndex) -> string|nil
    --       what this tab is called RIGHT NOW, when the sub-value has a name of its own.
    --       Without it the tab announces its base label, which is a name that is WRONG rather
    --       than a name that is missing — the worse of the two for someone who cannot check.
    verticalStep = opts.verticalStep,
    verticalName = opts.verticalName,
    -- `hotkeyKeepsFocus`: a tab's own hotkey SELECTS it without jumping to it.
    --
    -- Moving focus is the right default and stays the default: pressing a tab's key usually
    -- means "take me there", and the controls that follow belong to that tab. It is wrong when
    -- the tab control is a SETTING rather than a place — Melodyne's tool bar is one, and its
    -- F-keys are Melodyne's own, so a user changing tool in the middle of editing gets thrown
    -- out of the editor and has to find their way back for every switch.
    hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true,
    -- `onSelect = function(overlay, tabIndex)`: this tab was just selected and announced.
    --
    -- For the module that has something ELSE watching the same window and must not talk over
    -- the announcement. Melodyne is the case: its tool bar and its two read-outs are the same
    -- strip of pixels, so changing tool changes the read-outs, and the watcher that speaks a
    -- changed read-out was cutting off the name of the tool the user had just chosen. The
    -- module cannot see the switch any other way — a tab is selected by a hotkey, by Left and
    -- Right, by Ctrl+Tab and by Ctrl+<n>, and all four end up here.
    onSelect = opts.onSelect,
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
  -- `opensMenu`: activating this control puts a MENU on screen. Declared, because the
  -- overlay cannot tell. It makes the overlay's menu tests run on every tick for a while
  -- after the press, and gives a test that compares against the moment before the click
  -- (newWindow) its starting point — see the Menus section. It never counts a menu as open
  -- by itself: only a test does that.
  -- `button`: which mouse button the click at `at` is ("right" for a menu a right click opens).
  -- `menuItem`: the click at `at` opens a menu, and this is the item to CHOOSE in it, as an
  -- offset from the menu's corner — clicked when a menu test sees the menu, checked against the
  -- menu's own window, and clicked again while the menu is seen not to have taken it, up to
  -- `retries` more times (see "Choosing an item" in the Menus section). It implies `opensMenu`.
  return add(self, {
    kind = "hotspot",
    label = opts.label,
    ocrLabel = opts.ocrLabel,
    text = opts.text,
    at = opts.at,
    points = opts.points,
    settle = opts.settle,
    hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true,
    opensMenu = (opts.menuItem ~= nil) or opts.opensMenu,
    rawOrigin = opts.rawOrigin,
    fromRight = opts.fromRight,
    button = opts.button,
    menuItem = opts.menuItem,
    retries = opts.retries,
    when = opts.when,
  })
end

-- A VALUE, changed with Left and Right, where the module knows how to change it.
--
-- The runtime already had a slider, and it cannot serve here: it finds its thumb by matching
-- an image, which needs a template captured for one plug-in at one size. ON:EAR's Tone knob is
-- a rotary drawn as an arc, its Width a two-handled bar, and both live in a window the user
-- can resize freely — there is no thumb template to hold still.
--
-- What they do have is a value printed beside them and something that moves them, and that is
-- all a stepper is: `text` says where it stands, `onStep(dir)` moves it, and the announcement
-- afterwards comes from reading it again rather than from counting what we asked for. A
-- control that reports its own intention rather than the plug-in's state is the failure this
-- project keeps returning to, so the value is always re-read.
--
-- Announced as a slider because that is what it is to the person using it; how it is driven
-- underneath is our problem, not theirs.
function Overlay:addStepper(opts)
  return add(self, {
    kind = "stepper",
    label = opts.label,
    text = opts.text,
    onStep = opts.onStep,
    -- What a PRESS means, where the control has something. Optional: a stepper without one is
    -- a value you change with the arrows and nothing else.
    onActivate = opts.onActivate,
    settle = opts.settle,
    typeLabel = opts.typeLabel,
    hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true,
    when = opts.when,
  })
end

function Overlay:addCustomButton(opts)
  return add(self, {
    kind = "custom", label = opts.label, onActivate = opts.onActivate,
    typeLabel = opts.typeLabel,
    -- `editable`: activating this control leaves the caret in a text field, so Space belongs
    -- to the application from then on — the same rule an OCREdit already gets, for controls
    -- that reach their field some other way than by an OCR region.
    editable = opts.editable,
    -- `text = function(overlay) -> string`: read-out, announced with the label. On a button
    -- it is how a control that ACTS can also say what it would act on — a panel toggle that
    -- announces only its name leaves the user to press it and find out, which for a collapse
    -- means losing the panel to learn that it was open.
    text = opts.text,
    hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true, opensMenu = opts.opensMenu, when = opts.when,
  })
end

function Overlay:addOCRButton(opts)
  -- `readOnly` makes the control a pure announcer: focusing/activating re-reads the
  -- region via OCR but does NOT click it. For value/label readouts sitting over a
  -- drag knob or near a destructive control, where a click would change state.
  -- It is also announced without the word "button" (see speakControl) — a control that
  -- cannot be pressed must not describe itself as one.
  return add(self, {
    kind = "ocr", label = opts.label, region = opts.region, hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true,
    readOnly = opts.readOnly, opensMenu = opts.opensMenu, when = opts.when,
    -- `text = function(overlay) -> string`: announced between the label and the value, the
    -- same as on a custom button. A read-out whose MEANING changes needs it — Melodyne's
    -- left inspector box holds whichever parameter the live tool owns, so "Inspector,
    -- Amplitude, -3.0 dB" is the only reading of it that is not a guess.
    --
    -- It was accepted by the module and silently dropped here for a day: the option was
    -- passed, nothing complained, and the tool name simply never reached the user. That is
    -- the failure mode of building a control table field by field.
    text = opts.text,
    -- `fallback`: what to say when the region reads as nothing. The generic answer is "no
    -- text", which describes OUR RECOGNISER rather than the application — true, but the
    -- wrong subject. A field that is legitimately empty under some states of the plug-in
    -- should say so in the plug-in's terms ("no value"), and the prior art this project
    -- ports from did exactly that.
    fallback = opts.fallback,
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
    hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true, when = opts.when,
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
    hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true,
    hint = opts.hint,
    -- `reveal`: this control lives inside a panel that can be COLLAPSED, and while it is,
    -- the control is not on screen at all. { at = {x,y}, closedColor = {r,g,b}, settle = ms }
    -- — the point whose colour says "still closed", which is also the point that opens it.
    -- See ensureRevealed for why this has to run before the state is READ, not only before
    -- the control is activated.
    reveal = opts.reveal,
    -- Optional per-control overrides of the module-wide match robustness defaults.
    tolerance = opts.tolerance,
    scales = opts.scales,
    when = opts.when,
  })
end

-- A button that is FOUND rather than placed — ReaHotkey's GraphicalButton. The template is
-- searched for inside `region`, and activating clicks the centre of WHERE IT WAS FOUND, not
-- the centre of the region.
--
-- That distinction is the entire control. Kontakt's "Performance | FX Rack" tab row is the
-- case that needed it: it belongs to an INSTRUMENT and sits at the foot of that instrument's
-- own GUI, so with two instruments loaded there are two of them, at heights that depend on
-- how tall each GUI is and where the rack is scrolled. There is no coordinate to write down —
-- which is also why ReaHotkey's fixed probe for it corresponded to nothing in our window. It
-- was never drift; it was a number that could not exist.
--
-- Reports "not found" when the template is absent, as ReaHotkey does (States 0 = "not found").
-- A button that cannot be located must say so rather than click the middle of its region: that
-- region is chosen wide enough to contain the thing WHEREVER it has moved to, so its centre is
-- the one point almost guaranteed to be something else.
function Overlay:addGraphicalButton(opts)
  return add(self, {
    kind = "gbutton",
    label = opts.label,
    region = opts.region,
    image = opts.image,
    hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true,
    opensMenu = opts.opensMenu,
    -- `dragBy = {dx, dy}`: DRAG from where the template was found instead of clicking it.
    -- For a handle rather than a button — a window's resize grip, a splitter, a fader — where
    -- the gesture is the whole point. Being found rather than placed matters doubly here: a
    -- resize grip MOVES as a result of being dragged, so a fixed coordinate would work exactly
    -- once and then aim at empty space.
    --
    -- May also be `function(overlay, hit) -> dx, dy, why`, for a handle with a limit only the
    -- caller knows. Returning no offset declines the drag and speaks `why`.
    dragBy = opts.dragBy,
    -- `clickOffset = {dx, dy}`: press somewhere OTHER than the middle of what was found, for a
    -- control whose only findable part is not its clickable part. Kontakt's editor sections are
    -- the case: the caption "Insert Effects" is what can be recognised — Kontakt's own chrome,
    -- the same on every library — while the thing to press is the small fold button beside it,
    -- which carries no text and looks identical on all five.
    clickOffset = opts.clickOffset,
    -- `state = function(overlay, hit) -> string|nil`: what to announce after the label, worked
    -- out from WHERE the control was just found. A second template per state would do the same
    -- job and cost a second full search; a caller that knows the offset can read one pixel.
    state = opts.state,
    -- `unavailable = function(overlay) -> string|nil`: what to announce INSTEAD of searching,
    -- when the caller knows the control cannot act right now. See valueOfNow.
    unavailable = opts.unavailable,
    -- `rawOrigin`, as on a hotspot: pins the region to the matched control's OWN frame,
    -- skipping the per-overlay content offset. For anything living in the plug-in's chrome
    -- rather than inside the library's content — Kontakt's instrument rack, for instance,
    -- where the cell's +29 px offset silently clipped the top of the searched-for image and
    -- turned a correct template into "not found".
    rawOrigin = opts.rawOrigin,
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
    hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true,
    tolerance = opts.tolerance,
    scales = opts.scales,
    when = opts.when,
  })
end

-- A toggle whose on/off state is read from a SINGLE pixel at its click point (one screen
-- touch — cheap, unlike a per-pixel region scan), compared to an on/off reference colour
-- (nearest wins, but only if it is near ENOUGH — see hotspotToggleState) — ReaHotkey's
-- HotspotToggleButton. Activating clicks the point (toggling it) and re-reads the state.
-- `at` is origin-relative; `onColor`/`offColor` are {r, g, b}, or a LIST of such tables when
-- one state can legitimately look several ways.
function Overlay:addHotspotToggle(opts)
  return add(self, {
    kind = "hotspottoggle",
    label = opts.label,
    ocrLabel = opts.ocrLabel,
    -- `text = function(overlay) -> string`: announced between the label and "toggle button".
    -- A state and an AVAILABILITY are different questions, and the pixel that answers one
    -- often answers the other: a greyed switch still has an on/off state, and saying only
    -- that state invites the user to press something the application will ignore.
    text = opts.text,
    at = opts.at,
    onColor = opts.onColor,
    offColor = opts.offColor,
    hotkey = opts.hotkey, hotkeyKeepsFocus = opts.hotkeyKeepsFocus == true,
    hint = opts.hint,
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
-- Can this overlay's answer still matter, or does a claimant of at least its own rank
-- already own the slot?
--
-- Only one thing is loaded in a plugin at a time, so once something of equal or greater
-- specificity has won, nothing this overlay could discover changes the outcome — and it will
-- get another chance, because the winner keeps re-checking ITSELF and gives the slot up when
-- it stops matching. Rank matters in the other direction: a LESS specific winner (Kontakt's
-- own header, while no library is recognised) must never silence a library overlay, or it
-- could never take over. And the current winner must never skip its own check, or it would
-- hold the slot forever.
--
-- This test lived inside :landmark, guarding only the image search. Measured, that was the
-- cheap half: with every OS answer served from a per-epoch cache, a context match still cost
-- 15-96 ms, because it is ~19 crossings of the Lua/Rust boundary and there is no single
-- expensive call in it to fix. So the test belongs here, around the whole recheck — not a
-- faster answer, but no question at all.
function Overlay:_outranked()
  if not (self._slot and self._claim and host.arbiter.winnerSpecificity) then return false end
  local owner = host.arbiter.winner(self._slot)
  if not owner or owner == self._claim then return false end
  local rank = host.arbiter.winnerSpecificity(self._slot)
  return rank ~= nil and rank >= (self._specificity or 0)
end

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
  -- Near-miss diagnosis, armed once per plug-in window and re-armed whenever a landmark that
  -- WAS matching stops.
  --
  -- A landmark that no longer matches fails silently and always has: no overlay, no
  -- announcement, no line saying which gate was tried or how close it came. For a user who
  -- cannot see the screen that is indistinguishable from "this library is not supported", and
  -- it is what happened — a second patch of one library moved 29 pixels of 17480 outside
  -- tolerance and the only symptom available to anyone was nothing at all.
  --
  -- What makes this loggable without drowning the log is that a MISS IS NORMAL. Twelve library
  -- overlays search every poll and eleven of them are supposed to fail. The signal is not the
  -- miss, it is HOW CLOSE it came: a different library is nowhere near, while a template that
  -- has gone stale is a few units of tolerance away. So a full-region miss re-runs the same
  -- search once at three times the tolerance, and says something only if THAT hits.
  local probedOrigin = nil
  -- Set while the searches answer "could not look" (see the callback), so that is logged once
  -- per spell rather than at every poll.
  local unanswered = false
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
    if self:_outranked() then return matched end
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
      host.screen.imageSearchAsync(image, { region = region, tolerance = MATCH_TOLERANCE }, function(hit, why)
        pending = false
        -- "Could not look" is not "not there". A reason means no search was made — the capture
        -- failed, or the search was ended because the module had too many waiting — so it says
        -- nothing about the landmark, and the last verdict stands until a search is made at the
        -- next pass. Read as a miss, it would tear down an overlay whose library is plainly
        -- loaded, and a gate whose search was ended at every poll could never be detected.
        if hit == nil and why ~= nil then
          if not unanswered then
            unanswered = true
            host.log.info(("[landmark] %s: could not look (%s); the last verdict (%s) stands until "
              .. "a search can be made"):format(tostring(self.label), tostring(why),
              matched and "found" or "not found"))
          end
          return
        end
        unanswered = false
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
        -- Ask "how close?" on a full-region miss, at most once per window, and once more
        -- whenever a landmark that had been matching stops — which is the shape a patch change
        -- takes. Re-arming on that transition matters as much as the first probe: the failure
        -- this exists for happened by loading a second patch into a window already open.
        if hit == nil and (probedOrigin ~= origin.id or was) then
          probedOrigin = origin.id
          host.screen.imageSearchAsync(image, {
            region = region,
            tolerance = MATCH_TOLERANCE * 3,
          }, function(near)
            if not near then return end   -- genuinely a different library: say nothing
            host.log.info(("[landmark] %s: NEAR MISS — no match at tolerance %d, but found at "
              .. "%d,%d with tolerance %d. The template is probably stale rather than the "
              .. "library absent; compare it against a fresh capture."):format(
              tostring(self.label), MATCH_TOLERANCE, near.x, near.y, MATCH_TOLERANCE * 3
            ))
          end)
        end
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
    gbutton = "button", stepper = "slider",
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

-- ── Scaling ──────────────────────────────────────────────────────────────────
-- A plug-in that ZOOMS its whole interface moves every control with the zoom, and no fixed
-- coordinate survives that. VPS Avenger zooms from 50 to 200 %, and its layout is one picture
-- drawn at the zoom's size: every point measured at 50 % from Avenger's corner lands at
-- (zoom / 50) times that distance (the avenger_control project's coordinate model, and the
-- reason its tables are written at 50 %). ON:EAR is the other case already in this repository:
-- it scales its whole window uniformly by the height and centres it across, which the module
-- carried out by hand in every `at`, and again in every point its own code clicked.
--
-- So the runtime scales, for the overlay that asks: `O:scale(fn, { about = {x, y} })`, and every
-- authored coordinate goes through one formula,
--
--     screen = origin + frame + k * (authored - about)
--
-- rounded once, to the nearest whole unit (half up: a pixel on Windows, a point on a Mac) — `k`
-- what `fn(origin)` answers now, the frame what `O:frame` answers (unscaled: it is the part of
-- the geometry that does NOT zoom — an offset of the plug-in's own corner from the origin, as
-- Avenger's (-2, +2) on a DAW's panel, or the half-width ON:EAR centres on; never the DAW's
-- chrome, which daw-hosts has already put the origin below), and `about` the authored point
-- that stays where the frame puts it ({0, 0} unless given). Rounded once and as one sum, in that
-- order, because that is the order ON:EAR's own arithmetic had — which is what makes its
-- migration give the same points, not merely nearly the same. Two places do not fit the sum as
-- written: `fromRight` is origin + frame's dx + the content's width − k × at[1] (a distance
-- from an edge, which `about` has nothing to say about), and a landmark-ANCHORED overlay places
-- from the landmark's corner with no frame at all (originOf).
--
-- A FUNCTION for the factor rather than a design size compared with the measured one, because
-- neither user has a size to compare: Avenger's factor is its zoom READ off its own caption
-- (the plug-in's width has an intercept nobody has measured, and a zoom guessed from it reads
-- 80 % as 75 % — the tool's own formula does), and ON:EAR's is the height alone, the width
-- being free. A design size is one line inside the function; a read zoom cannot be expressed as
-- a design size at all.
--
-- `nil` from the function is "cannot tell now" (the caption could not be read yet): nothing is
-- placed — no click, no read — and a control that would have acted says so. Not 1: a factor
-- guessed at 1 is a click at the wrong place with the confidence of the right one.
--
-- What does NOT scale: a `rawOrigin` control, which is in the origin's own pixels (neither the
-- frame nor the scale — it names DAW chrome, or a point the module worked out from the screen);
-- a `dragBy` FUNCTION's answer, a `menuItem` FUNCTION's answer, and anything else a module
-- computes from a hit or a menu's rectangle, which are screen pixels already; and template
-- images, which are matched at their own `scales`.
--
-- An overlay that never calls O:scale takes none of this: every site below keeps the arithmetic
-- it had, character for character, on its unscaled branch.

-- The factor in effect now, or nil and why not. Asked of the module's function at every
-- resolution and not kept here: the function knows what its answer depends on (a zoom read once
-- per epoch, a window height) and memoises accordingly. Guarded, because a raising factor must
-- cost the controls it would have placed and not the key that asked.
local function factorNow(self)
  local s = self._scale
  local o = self:origin()
  if not o then return nil, "the overlay has no origin now" end
  local ok, k = pcall(s.fn, o)
  local why = nil
  if not ok then
    why = "its scale function raised: " .. tostring(k)
    k = nil
  elseif k == nil then
    why = "its scale function answered nil"
  elseif type(k) ~= "number" or k ~= k or k <= 0 or k == math.huge then
    why = "its scale function answered " .. tostring(k) .. ", not a factor above 0"
    k = nil
  end
  -- Said when it CHANGES, never per resolution: a calibration shot resolves every control, and
  -- a Tab resolves one. The change is the event a log reader wants — Avenger zoomed, ON:EAR's
  -- window was resized, the caption stopped being readable.
  local now = k and string.format("%.4f", k) or why
  if now ~= self._scaleSaid then
    self._scaleSaid = now
    if k then
      host.log.info(string.format("[scale] '%s': factor %s about (%s,%s)", tostring(self.label),
        now, tostring(s.about[1]), tostring(s.about[2])))
    else
      host.log.info(string.format("[scale] '%s': no factor now — %s; nothing it places is "
        .. "clicked or read until it answers one", tostring(self.label), tostring(why)))
    end
  end
  return k, why
end

-- Whether control `c` goes through the scale: every control of a scaled overlay but a
-- `rawOrigin` one. `c` nil is a point asked of the overlay itself (O:toScreen, captureRegion).
local function scaledFor(self, c)
  return self._scale ~= nil and not (c and c.rawOrigin)
end

-- Authored point (ax, ay) of control `c` on screen: x, y — or nil, nil and why. Every function
-- here that answers a POINT answers three values on failure, so that `local x, y, why =` reads
-- the reason where it expects it; a rectangle answers nil and why.
local function placePoint(self, c, ax, ay)
  if not scaledFor(self, c) then
    local ox, oy = originOf(self, c)
    return ox + ax, oy + ay
  end
  local k, why = factorNow(self)
  if not k then return nil, nil, why end
  local ox, oy = originOf(self, c)
  local a = self._scale.about
  return math.floor(ox + k * (ax - a[1]) + 0.5), math.floor(oy + k * (ay - a[2]) + 0.5)
end

-- Authored rectangle {x1, y1, x2, y2} of control `c` on screen, each corner as a point — or
-- nil and why.
local function placeRect(self, c, r)
  if not scaledFor(self, c) then
    local ox, oy = originOf(self, c)
    return { ox + r[1], oy + r[2], ox + r[3], oy + r[4] }
  end
  local k, why = factorNow(self)
  if not k then return nil, why end
  local ox, oy = originOf(self, c)
  local a = self._scale.about
  return {
    math.floor(ox + k * (r[1] - a[1]) + 0.5), math.floor(oy + k * (r[2] - a[2]) + 0.5),
    math.floor(ox + k * (r[3] - a[1]) + 0.5), math.floor(oy + k * (r[4] - a[2]) + 0.5),
  }
end

-- A control's authored rectangle in `field` ("region", "ocrLabel"): the table, or what its
-- function answers now — nil and why when it answers none. A table is handed on as it is, exactly
-- as before functions were allowed; only a function's answer is checked, because it is the one
-- that can change between two reads.
local function authoredRect(self, c, field)
  local r = c[field]
  if type(r) ~= "function" then return r end
  local ok, v = pcall(r, self)
  if not ok then return nil, "its `" .. field .. "` function raised: " .. tostring(v) end
  if v == nil then return nil, "its `" .. field .. "` function answered nil" end
  if type(v) ~= "table" or type(v[1]) ~= "number" or type(v[2]) ~= "number"
    or type(v[3]) ~= "number" or type(v[4]) ~= "number" then
    return nil, "its `" .. field .. "` function answered " .. tostring(v) .. ", not {x1, y1, x2, y2}"
  end
  return v
end

-- Control `c`'s rectangle in `field` on screen: the rectangle, and the authored one it came
-- from — or nil and why.
local function regionOf(self, c, field)
  local ar, why = authoredRect(self, c, field or "region")
  if not ar then return nil, why end
  local r, rwhy = placeRect(self, c, ar)
  if not r then return nil, rwhy end
  return r, ar
end

-- The middle of authored rectangle `ar` of control `c`, on screen: the middle of the rectangle
-- it resolves to, rounded down — or nil, nil and why. Unscaled, the arithmetic it always had.
local function rectCentre(self, c, ar)
  if not scaledFor(self, c) then
    local ox, oy = originOf(self, c)
    return ox + math.floor((ar[1] + ar[3]) / 2), oy + math.floor((ar[2] + ar[4]) / 2)
  end
  local r, why = placeRect(self, c, ar)
  if not r then return nil, nil, why end
  return math.floor((r[1] + r[3]) / 2), math.floor((r[2] + r[4]) / 2)
end

-- An authored DISTANCE from screen point (x0, y0) — a click offset from where a template was
-- found, an item's offset from a menu's corner — on screen. Scaled, never moved by the frame or
-- `about`: those place the plug-in, and this is measured from something already placed.
local function placeFrom(self, c, x0, y0, dx, dy)
  if not scaledFor(self, c) then return x0 + dx, y0 + dy end
  local k, why = factorNow(self)
  if not k then return nil, nil, why end
  return math.floor(x0 + k * dx + 0.5), math.floor(y0 + k * dy + 0.5)
end

-- A control that cannot be placed right now is SAID, not clicked and not silently dropped: from
-- the keyboard a key that does nothing is a broken tool. `quiet` when the control has already
-- said something about it (an OCR control's "cannot be read now").
local function unplaced(self, c, why, quiet)
  host.log.info(string.format("[place] '%s': '%s' cannot be placed now — %s; nothing clicked",
    tostring(self.label), tostring(c and c.label), tostring(why)))
  if not quiet then
    host.speech.output(((c and c.label) or "That control") .. " is not available now",
      { interrupt = true })
  end
end

-- Authored point (x, y) of this overlay on screen, for a module's own code: what an `at` would
-- click, through the same frame and scale. `opts.rawOrigin` places it as a `rawOrigin` control.
-- nil and why when the overlay has no origin or no factor now — the ordinary Lua pair, since a
-- module reads `local x, y = o:toScreen(…)` and tests `x`.
function Overlay:toScreen(x, y, opts)
  if not self:origin() then return nil, "the overlay has no origin now" end
  local sx, sy, why = placePoint(self, (opts and opts.rawOrigin) and { rawOrigin = true } or nil, x, y)
  if sx == nil then return nil, why end
  return sx, sy
end

-- Corners as host.ocr.recognize takes them — whole numbers, not empty — from corners as the
-- overlay places them: each cut toward zero, as the older reading of a region cut them, so a focus
-- read covers exactly the pixels it always covered — an unscaled overlay's fractional frame
-- included. nil when nothing is left: corners turned around, or under one across or down.
local function readCorners(r)
  local function cut(v) return v < 0 and math.ceil(v) or math.floor(v) end
  local x1, y1, x2, y2 = cut(r[1]), cut(r[2]), cut(r[3]), cut(r[4])
  if x2 <= x1 or y2 <= y1 then return nil end
  return { x1, y1, x2, y2 }
end

-- Authored rectangle {x1, y1, x2, y2} on screen, likewise: what a `region` would read.
--
-- `opts.whole = true` gives the corners host.ocr.recognize and host.screen.snapshotAsync take, which
-- are whole numbers and not empty: cut toward zero, the pixels the overlay's own focus read takes
-- (readCorners), and nil with "empty on screen" when nothing is left. A scaled overlay places whole
-- numbers already; an unscaled one with a fractional frame does not, and a read would refuse its
-- corners at the call.
function Overlay:toScreenRect(r, opts)
  if not self:origin() then return nil, "the overlay has no origin now" end
  local placed, why = placeRect(self, (opts and opts.rawOrigin) and { rawOrigin = true } or nil, r)
  if not placed or not (opts and opts.whole == true) then return placed, why end
  local whole = readCorners(placed)
  if not whole then return nil, "empty on screen" end
  return whole
end

-- ── Where the overlay is: a mark, and whether it still holds ─────────────────────────────────
-- An answer that comes later — a read's callback, a timer — is about the moment it was asked in,
-- and the overlay may have moved on since: left the front, gone to another window, the focus
-- elsewhere, a key of its own pressed, another announcement begun, a menu opened.
-- The announcement of a read value has always checked all of that before it spoke, and an OCR
-- button's click before it clicked; a module's own code that reads and then acts needs the same
-- checks, and wrote them by hand from the runtime's private counters, or did without. So the
-- runtime keeps them in one place: `O:here()` notes where the overlay is, and `O:stillHere(mark,
-- opts)` says whether that still holds — true, or false and why, in the words the `[read]` lines
-- have always used, so a module's log line and the runtime's say the same thing the same way.
--
-- A mark is the overlay's own counters, read at the call, nothing else: no timer, no epoch. Each
-- thing it remembers changes only when that thing happens.

-- The metatable that tells a mark from any other table.
local Mark = {}

-- A mark of overlay `self` for control `index`. The runtime's own announcements announce a control
-- that need not be the focused one — a control's hotkey that keeps the focus where it is — so the
-- control is the one at `index`, and the focus is noted beside it; O:here() takes the focused one.
local function markAt(self, index)
  return setmetatable({
    overlay = self,
    stay = self._stay,
    hwnd = self:hwnd(),
    at = self.focus,
    index = index,
    control = self.controls[index],
    keys = self._keys,
    said = self._said,
    spoken = self._spoken,
    menuOpens = self._menuOpens,
  }, Mark)
end

-- Whether mark `m` still holds — true, or false and the first reason in this order. `place = false`
-- leaves the first three out. `control` is the runtime's own: the control the mark was taken on
-- still at its index, whatever has the focus, which an OCR button's click asks (a hotkey can press
-- one the focus is not on). So is `menusSince`: only a menu opened since, not one open now — what a
-- sentence asks after each of its hooks (speakControl), since a menu open before the hook is no news
-- after it.
local function stillHereAt(self, m, opts)
  if not (opts and opts.place == false) then
    if not self.active then return false, "the overlay is no longer active" end
    if self._stay ~= m.stay then return false, "the overlay left the front and came back since" end
    if self:hwnd() ~= m.hwnd then return false, "the overlay is on another window now" end
  end
  if not opts then return true end
  if opts.control and self.controls[m.index] ~= m.control then
    return false, "the control is not in the overlay any more"
  end
  if opts.focus and (self.focus ~= m.at or self.controls[m.index] ~= m.control) then
    return false, "the focus has moved on"
  end
  if opts.keys and self._keys ~= m.keys then return false, "a later key reached the overlay" end
  if opts.said and self._said ~= m.said then return false, "a later announcement was made" end
  if opts.spoken and self._spoken ~= m.spoken then return false, "a later announcement was spoken" end
  if opts.menus and self:menuOpen() then return false, "a menu is open over the overlay" end
  if (opts.menus or opts.menusSince) and self._menuOpens ~= m.menuOpens then
    return false, "a menu opened over the overlay since"
  end
  return true
end

-- Where the overlay is now, as a mark for O:stillHere: in front or not, which stay in front, the
-- window, the focus and the control on it, and how many keys of its own, announcements and menu
-- openings it has seen. A table lookup or two and one origin resolution (O:hwnd).
function Overlay:here()
  return markAt(self, self.focus)
end

local STILL_HERE_OPTS = { focus = true, keys = true, said = true, spoken = true, menus = true, place = true }

-- Whether what `mark` noted still holds: true, or false and why. Always asked, unless `place =
-- false`: the overlay still active, in the same stay in front, on the same window. And with
-- `focus = true` the same focus on the same control, `keys = true` no key of the overlay's own
-- since, `said = true` no announcement begun since, `spoken = true` none said since, `menus =
-- true` no menu open over it and none opened since. A mark from another overlay, anything but a mark, or an option it does not know raises:
-- a typo in an option would otherwise check nothing, silently.
function Overlay:stillHere(mark, opts)
  if getmetatable(mark) ~= Mark then
    error(("overlay '%s': stillHere takes a mark from O:here(), got %s")
      :format(tostring(self.label), type(mark)), 2)
  end
  if mark.overlay ~= self then
    error(("overlay '%s': stillHere was handed a mark of overlay '%s'")
      :format(tostring(self.label), tostring(mark.overlay.label)), 2)
  end
  if opts ~= nil then
    if type(opts) ~= "table" then
      error(("overlay '%s': stillHere's options are a table, got %s")
        :format(tostring(self.label), type(opts)), 2)
    end
    for k in pairs(opts) do
      if not STILL_HERE_OPTS[k] then
        error(("overlay '%s': stillHere has no option '%s' (it has focus, keys, said, spoken, menus and place)")
          :format(tostring(self.label), tostring(k)), 2)
      end
    end
  end
  return stillHereAt(self, mark, opts)
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

-- The key a hotkey spec stands for on this platform, in the host's one spelling: what two
-- controls' specs are compared by and what the claim maps are keyed by. "Cmd+S" beside an
-- inherited "Ctrl+S" — or "Option+P" beside "Alt+P" — is ONE key everywhere, and keyed
-- by the raw strings it became two registrations of which the host could keep only one live:
-- one control's key silently dead, the holds line claiming both. A spec that does not parse
-- keys as itself; the declaration report names it, and _syncControlHotkeys leaves it out.
local function keyOf(spec)
  if type(spec) ~= "string" then return spec end
  return host.keys.normalize(spec) or spec
end

-- A hotkey spec as this platform says it: "Option+V" on a Mac for the "Alt+V" a module wrote,
-- "Control+L" on Windows and "Command+L" on a Mac for "Ctrl+L". The authored
-- string was spoken before, which told a Mac user to press Alt, a key their keyboard does not
-- have by that name. Anything that is not a spec — a free-form label — is said as written.
local function spokenKey(spec)
  if type(spec) ~= "string" then return tostring(spec) end
  return host.keys.describe(spec) or spec
end

-- The host.keys.check reasons host.hotkey.register raises for: a spec that does not parse, a
-- combination the system keeps, a key this platform has no code for, and a modifier tap.
-- VoiceOver's layer is not one — it registers, and arrives on a Mac whose VoiceOver modifier
-- is Caps Lock — and neither is a character the layout types with the key.
local REFUSES_HOTKEY = { parse = true, reserved = true, ["no-keycode"] = true, tap = true }
local hotkeyRefusals = {}

-- Why the host will not hold `spec` as a hotkey, or nil when it will. Asked with the layout
-- left out: only the structure matters here, and asking the layout costs a system call per
-- spec at every bind and every hotkey sync. Remembered per spec for the life of the VM, because
-- nothing that decides it can change while the application runs.
local function hotkeyRefusal(spec)
  if type(spec) ~= "string" and type(spec) ~= "number" then return "parse" end
  local key = tostring(spec)
  local known = hotkeyRefusals[key]
  if known == nil then
    known = false
    for _, why in ipairs(host.keys.check(key, { layout = false }).reasons) do
      if REFUSES_HOTKEY[why] then
        known = why
        break
      end
    end
    hotkeyRefusals[key] = known
  end
  return known or nil
end

-- Spoken form of a tab, ReaHotkey-style: "<label> tab selected" — a tab's state is
-- "selected" when it is the current one, which is the only tab we ever announce. An
-- optional `extra` (e.g. "tab control") is inserted, and a bound hotkey's label is spoken
-- last (ReaHotkey appends the HotkeyLabel), so the user hears how to jump to it — in this
-- platform's words when the label is a key spec, as Melodyne's are. Not for a hotkey the host
-- refuses: that one is never registered, and telling the user to press it sends them to a key
-- that does nothing.
local function tabMessage(tab, extra)
  local m = ((tab and tab.label) or "") .. " tab selected"
  if extra and extra ~= "" then m = m .. ", " .. extra end
  if tab and tab.hotkeyLabel and tab.hotkeyLabel ~= "" and not (tab.hotkey and hotkeyRefusal(tab.hotkey)) then
    m = m .. ", " .. spokenKey(tab.hotkeyLabel)
  end
  return m
end

-- Clicks tab control `c`'s CURRENT tab coordinate (so the underlying dialog follows) and
-- announces it. Shared by every tab selector. `from` is the tab that was current before the
-- caller moved it: a tab whose point cannot be placed now (a scaled overlay with no factor)
-- goes back to it and says so, because announcing "selected" for a tab nobody clicked is a
-- statement about the application that is not true.
local function selectTab(self, c, from)
  local tab = c.tabs[c._current]
  if tab and tab.at then
    local x, y, why = placePoint(self, c, tab.at[1], tab.at[2])
    if not x then
      host.log.info(string.format("[place] '%s': tab '%s' cannot be placed now — %s; nothing clicked",
        tostring(self.label), tostring(tab.label), tostring(why)))
      if from then c._current = from end
      host.speech.output(tostring(tab.label or "") .. " tab is not available now", { interrupt = true })
      return
    end
    host.input.click(x, y)
  end
  -- Focus is now on the tab control (a non-editable stop): make sure Space is captured
  -- again if we arrived here from an editable field via Ctrl+Tab / Ctrl+<n>.
  self:_syncNativeKeys()
  -- The new tab exposes a different set of controls, so the hotkeys they claim change.
  if self.active then self:_syncControlHotkeys() end
  host.speech.output(tabMessage(tab), { interrupt = true })
  -- After the announcement, so a module that uses this to hold its own speech back is armed
  -- for what follows rather than for what was just said. Guarded like every other module
  -- callback: a bad hook must not take the tab switch with it.
  if c.onSelect then
    local ok, err = pcall(c.onSelect, self, c._current)
    if not ok then
      host.log.info(("[overlay] %s: tab onSelect failed: %s"):format(tostring(self.label), tostring(err)))
    end
  end
end

-- Left / Right on a focused tab control: switch its tab by `dir` (±1), wrapping. No-op
-- unless the focused control IS a tab control (so arrows stay inert on other controls).
function Overlay:switchTab(dir)
  local c = self.controls[self.focus]
  if not c or c.kind ~= "tabcontrol" or #c.tabs == 0 then return end
  local from = c._current
  c._current = ((c._current - 1 + dir) % #c.tabs) + 1
  selectTab(self, c, from)
end

-- Ctrl+Tab / Ctrl+Shift+Tab: switch the overlay's tab control from ANYWHERE (focus need
-- not be on it) and move focus onto it — ReaHotkey's FocusNextPreviousTab.
function Overlay:cycleTab(dir)
  local i, c = findTabControl(self)
  if not c or #c.tabs == 0 then return end
  self.focus = i
  local from = c._current
  c._current = ((c._current - 1 + dir) % #c.tabs) + 1
  selectTab(self, c, from)
end

-- Ctrl+<n>: jump straight to tab n and move focus onto the tab control.
-- One step along a tab control's second axis, announced by whatever performed it — the caller
-- knows what it picked, so nothing has to be re-detected to say it.
function Overlay:stepTabVertical(dir)
  local _, c = findTabControl(self)
  if not (c and c.verticalStep) then return end
  local ok, said = pcall(c.verticalStep, self, c._current, dir)
  if ok and type(said) == "string" and said ~= "" then
    host.speech.output(said, { interrupt = true })
  end
end

function Overlay:gotoTab(n, keepFocus)
  local i, c = findTabControl(self)
  if not c or n < 1 or n > #c.tabs then return end
  -- The selection still happens and is still announced either way; what `keepFocus` decides is
  -- only whether the user is taken along with it.
  if not (keepFocus or c.hotkeyKeepsFocus) then self.focus = i end
  local from = c._current
  c._current = n
  selectTab(self, c, from)
end

-- The middle of control `c`'s region on screen — or nil, nil and why (a region function with
-- no answer, a scaled overlay with no factor).
local function ocrCenter(self, c)
  local ar, why = authoredRect(self, c, "region")
  if not ar then return nil, nil, why end
  return rectCentre(self, c, ar)
end

-- A hotspot's point has to land inside the origin it was measured against. Outside it,
-- there are only other windows' pixels — the DAW's own chrome, another plugin, the
-- desktop — and a click there does something arbitrary in an application the user was not
-- aiming at and cannot see. Live: Kontakt 7's multi arrows carried Kontakt 8's
-- coordinates, 704 px from the left of a window 649 px wide, so both fired past the
-- plugin's right edge into REAPER.
--
-- This is the one coordinate error that pressing the key again cannot undo, so it refuses
-- and says so rather than clicking.
--
-- The legal area is the window's OWN PIXELS — its frame, not its client area.
--
-- This started as the client rectangle, and got two things wrong at once. It paired the
-- client ORIGIN with the frame SIZE, which for an embedded plug-in control is the same
-- rectangle (borderless, so the two agree) and for a standalone window overshoots the right
-- and bottom edges by a border width. And, worse, it made a NEGATIVE client-relative
-- coordinate impossible — when that is a legitimate, deliberate way to author a control.
--
-- A window's client origin sits BELOW its caption and native menu bar. So a hotspot that
-- addresses that menu bar is authored at a negative y, which is exactly what ReaHotkey does
-- for Komplete Kontrol's standalone menu bar (Includes/Overlays/KompleteKontrol.ahk:32-36,
-- File/Edit/View/Controller/Help at y = -10) and what our port copied. Every one of those
-- five was refused before it could click: the controls existed, Tab announced them, and
-- pressing one said "File menu is not where it should be" — a message that reads like
-- version drift and pointed nowhere near the real cause. Since the overlay's hotkeys are
-- RegisterHotKey claims, which outrank the application, it also swallowed Alt+F/E/V/C/H:
-- the very access keys that were the last route to that menu bar without us.
--
-- Testing the frame keeps the case this guard was written for. Kontakt 7's multi arrow
-- resolved 704 px from the left of a 649 px window; the frame is still 649 px wide, so it
-- is still refused. That is the error worth refusing — the one where pressing the key again
-- cannot undo what the first press did to whatever window lay beyond the edge.
local function pointInOrigin(self, x, y)
  local o = self:origin()
  if not (o and o.bounds) then return false end
  return x >= o.bounds.x
    and x < o.bounds.x + o.bounds.w
    and y >= o.bounds.y
    and y < o.bounds.y + o.bounds.h
end

-- The refusal names the control: one that is present and silently inert is the harder thing
-- to make sense of without seeing the screen.
local function refuseOutside(self, c, x, y)
  local o = self:origin()
  if not pointInOrigin(self, x, y) then
    local b = o and o.bounds
    host.log.info(string.format(
      "%s: '%s' resolves to (%d,%d), outside its origin's window (%d,%d) %dx%d — not clicking",
      tostring(self.label), tostring(c.label), x, y,
      b and b.x or 0, b and b.y or 0, b and b.w or 0, b and b.h or 0
    ))
    host.speech.output((c.label or "Control") .. " is not where it should be",
      { interrupt = true })
    return true
  end
  -- INSIDE THE RECTANGLE IS NOT THE SAME AS ON TOP OF IT.
  --
  -- Everything this project clicks is a screen coordinate worked out from a window's own
  -- frame, and the check above only asks whether the point falls within that frame. It says
  -- nothing about what is DRAWN there. A notification, a tooltip, another application raised
  -- over ours: the click goes to whichever window owns the pixel, and the overlay announces
  -- "activated" either way. For somebody who cannot see the screen that is a press that went
  -- somewhere unknown and reported success.
  --
  -- Asked of the platform rather than guessed, and a platform with no answer is taken as
  -- permission. Only a definite "that pixel belongs to another window" refuses — a check that
  -- cannot judge must not block what it cannot judge. On macOS the window server's own hit test
  -- answers (host.window.ownsPoint, unverified on hardware), and there its `nil` — no pairing
  -- yet, a window of VoiceOver's own — is permission too.
  --
  -- A popup the plug-in opens is a window of its own, so a point in it is "another window" to
  -- this check: an item in such a menu is chosen through "Choosing an item" (Menus), which asks
  -- about the menu's window instead.
  if o and o.id and host.window and host.window.ownsPoint then
    local ok, mine = pcall(host.window.ownsPoint, o.id, x, y)
    if ok and mine == false then
      host.log.info(string.format(
        "%s: '%s' at (%d,%d) is inside our window's frame but another window is drawn there "
          .. "— not clicking",
        tostring(self.label), tostring(c.label), x, y
      ))
      host.speech.output("something else is covering " .. (c.label or "that control"),
        { interrupt = true })
      return true
    end
  end
  return false
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
  local region = regionOf(self, c)
  if not region then return nil end
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
  local region = regionOf(self, c)
  if not region then return nil end
  local _, hit = host.screen.imageSearchMulti(asList(c.thumbImage), {
    region = region,
    tolerance = c.tolerance or MATCH_TOLERANCE,
    scales = c.scales or MATCH_SCALES,
  })
  if not hit then return nil end
  return {
    cx = hit.x + math.floor(hit.w / 2),
    cy = hit.y + math.floor(hit.h / 2),
  }
end

-- The rectangle a graphical button hunts in, in SCREEN coordinates.
--
-- `region` is optional, and omitting it means THE WHOLE ORIGIN — which is the right default
-- far more often than a written-out rectangle. A found control is used precisely when its
-- position is not fixed, and a static region then has to guess how far the thing can wander:
-- Kontakt's resize grip escaped its region on the second press, because dragging it made the
-- window taller and moved the grip past the lower bound that had looked generous. The origin
-- grows with the window, so it cannot go stale that way.
--
-- nil and why when there is a `region` and it cannot be placed now (a function with no answer, a
-- scaled overlay with no factor): not the whole origin, which would search where the module
-- said not to.
local function searchRect(self, c)
  if c.region then return regionOf(self, c) end
  local ox, oy = originOf(self, c)
  local o = self:origin()
  if not o then return { ox, oy, ox, oy } end
  local w, h = O.contentSize(o)
  return { ox, oy, ox + w, oy + h }
end

-- Where a graphical button's template currently is, as a click point, or nil.
local function graphicalButtonPoint(self, c)
  if not c.image then return nil end
  local rect = searchRect(self, c)
  if not rect then return nil end
  local _, hit = host.screen.imageSearchMulti(asList(c.image), {
    region = rect,
    tolerance = c.tolerance or MATCH_TOLERANCE,
    scales = c.scales or MATCH_SCALES,
  })
  if not hit then return nil end
  return hit.x + math.floor(hit.w / 2), hit.y + math.floor(hit.h / 2)
end

-- The thumb's position as a percentage of the track, clamped. nil when the track is
-- degenerate (from == to) — better to say nothing than to announce a number from a
-- division that means nothing.
local function sliderPercent(self, c, thumb)
  local from, to
  if scaledFor(self, c) then
    -- `from` and `to` are x coordinates like any other, so they scale as points do.
    from = placePoint(self, c, c.from, 0)
    to = placePoint(self, c, c.to, 0)
    if not (from and to) then return nil end
  else
    local ox = originOf(self, c)
    from, to = ox + c.from, ox + c.to
  end
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
  -- A track that cannot be placed now (a scaled overlay with no factor, a region function with
  -- no answer) is said as every control that cannot be placed is: "not found" would describe a
  -- thumb search that was never made.
  if c.region then
    local r, why = regionOf(self, c)
    if not r then return unplaced(self, c, why) end
  end
  if scaledFor(self, c) then
    local _, _, why = placePoint(self, c, c.from, 0)
    if why then return unplaced(self, c, why) end
  end
  local thumb = sliderThumb(self, c)
  if not thumb then
    host.speech.output((c.label or "") .. ", not found", { interrupt = true })
    return
  end
  -- One percent of the track, on screen: of the track as it is drawn now in a scaled overlay.
  local step = math.max(1, math.ceil((c.to - c.from) / 100))
  if scaledFor(self, c) then
    local from = placePoint(self, c, c.from, 0)
    local to = placePoint(self, c, c.to, 0)
    if from and to then step = math.max(1, math.ceil((to - from) / 100)) end
  end
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
  if scaledFor(self, c) then
    -- The same point, through the scale. A third value (why) says a point exists but cannot be
    -- placed now, which the caller says out loud; a plain nil stays the silent "no point" an
    -- `at` function has always been allowed to answer.
    local k, why = factorNow(self)
    if not k then return nil, nil, why end
    local ox, oy = originOf(self, c)
    local a = self._scale.about
    local x
    if c.fromRight then
      -- A distance leftwards from the content's right edge, scaled like any distance; `about`
      -- is a point and has nothing to say about an edge.
      x = math.floor(ox + O.contentSize(self:origin()) - k * at[1] + 0.5)
    else
      x = math.floor(ox + k * (at[1] - a[1]) + 0.5)
    end
    return x, math.floor(oy + k * (at[2] - a[2]) + 0.5)
  end
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
local function colorDist(p, ref)
  return math.abs(p.r - ref[1]) + math.abs(p.g - ref[2]) + math.abs(p.b - ref[3])
end

-- A reference may be ONE colour or a LIST of them — ReaHotkey's OnColors/OffColors are
-- arrays, because a control can legitimately look several ways in one state (a lit button
-- under two different skins, a glyph the renderer antialiases differently at two sizes).
local function asColors(v)
  if type(v[1]) == "number" then return { v } end
  return v
end

local function nearestRef(p, refs)
  local best
  for _, ref in ipairs(asColors(refs)) do
    local d = colorDist(p, ref)
    if best == nil or d < best then best = d end
  end
  return best
end

-- How far apart the two states actually LOOK, which is the only scale on which "near
-- enough" means anything: Diva's lit and unlit tab labels are 109 apart, a Cinematic Studio
-- mic icon's are 334. A fixed tolerance would be far too loose for one and too tight for
-- the other. Constant per control, so it is computed once.
local function refSeparation(c)
  if c._sep then return c._sep end
  local best
  for _, on in ipairs(asColors(c.onColor)) do
    for _, off in ipairs(asColors(c.offColor)) do
      local d = colorDist({ r = on[1], g = on[2], b = on[3] }, off)
      if best == nil or d < best then best = d end
    end
  end
  c._sep = best or 0
  return c._sep
end

-- A floor, for a control whose two states genuinely look almost alike: half of "almost
-- nothing" is a threshold nothing could ever pass, which would silence a working control.
local TOGGLE_MIN_MARGIN = 30

local function hotspotToggleState(self, c)
  if not (c.at and c.onColor and c.offColor) then return nil end
  local px, py = hotspotPoint(self, c)
  if not px then return nil end
  local p = host.screen.pixel(px, py)
  if not p then return nil end
  local don, doff = nearestRef(p, c.onColor), nearestRef(p, c.offColor)
  -- REFUSE rather than guess. "Whichever is nearer" always has an answer, including for a
  -- pixel that resembles neither state — and these probes are single pixels, sometimes
  -- landing on the antialiased edge of a glyph stroke (Diva's preset-browser tab is exactly
  -- that: one pixel wide, its neighbours nothing like it). Let the window drift, the plug-in
  -- rescale or a version change the font, and the probe reads background — from which
  -- "nearer" cheerfully derives a state, and announces it with the same confidence as a
  -- correct one. Announcing the OPPOSITE of the truth is the worst thing this system can do
  -- to someone who cannot check it against the screen; saying nothing is merely unhelpful.
  --
  -- So the reading must be close to one of the references on the scale of how far apart
  -- they are. Logged when refused, because a toggle that has quietly stopped reporting is
  -- otherwise indistinguishable from one that has no state.
  local limit = math.max(TOGGLE_MIN_MARGIN, math.floor(refSeparation(c) / 2))
  local near = math.min(don, doff)
  if near > limit then
    host.log.info(string.format(
      "[overlay] %s: '%s' at (%d,%d) reads (%d,%d,%d) — %d from the nearer reference, "
      .. "limit %d; refusing to report a state",
      tostring(self.label), tostring(c.label or ""), px, py, p.r, p.g, p.b, near, limit))
    return nil
  end
  if don <= doff then return "on" else return "off" end
end

-- How far the probe may drift and still count as "closed".
local REVEAL_TOLERANCE = 30


-- Opens the panel this control lives in, if it is shut, and then continues.
--
-- ReaHotkey binds its ClickFXRack to a Soundiron toggle TWICE — before focus and before
-- activation — and the doubling is the whole point. A state read taken while the rack is
-- closed is not "off": the button is not on screen, so the templates match nothing and the
-- control would report whatever the closed panel happens to look like. Revealing only on
-- activation would fix the click and leave the announcement lying.
--
-- Asynchronous, because we cannot sleep — the 250 ms ReaHotkey spends blocked is a timer
-- here, and everything after it is a continuation. Which means it can land in a different
-- world: the continuation re-checks that the overlay is still where the probe was read —
-- active, in the same stay in front, on the SAME window — so a reveal started in one plug-in
-- cannot deliver its answer into another.
--
-- Fails OPEN. If the probe cannot be read at all we carry on rather than refuse: a control
-- that silently stops responding is worse than one that acts on a panel that was already
-- open, and clicking the probe when it is open is what ReaHotkey's own colour test avoids.
--
-- The probe is a read of the screen, and a read can wait: the overlay must still be where the
-- probe was read before anything follows it — the probe's click when it reads closed, the
-- toggle's own click or its state read when it reads open or not at all — or that lands in
-- whatever came to the front meanwhile. Where it is not, nothing follows, and the log says why.
local function ensureRevealed(self, c, done)
  local r = c.reveal
  if not r or not r.at or not r.closedColor then return done() end
  local px, py = placePoint(self, c, r.at[1], r.at[2])
  -- A probe that cannot be placed now cannot be read either: fail open, as for a failed read.
  if not px then return done() end
  local mark = markAt(self, self.focus)
  local function gone(after)
    local fine, why = stillHereAt(self, mark)
    if fine then return false end
    host.log.info(string.format("[overlay] %s: '%s' not going on %s — %s",
      tostring(self.label), tostring(c.label), after, why))
    return true
  end
  local p = host.screen.pixel(px, py)
  if gone("after reading its panel's probe") then return end
  if not p or colorDist(p, r.closedColor) > REVEAL_TOLERANCE then return done() end
  if refuseOutside(self, c, px, py) then return end
  host.input.click(px, py)
  host.timer.after(r.settle or 250, function()
    if not gone("after opening its panel") then done() end
  end)
end

-- The value of an image-read control, delivered to `done` when the worker answers.
--
-- Same measurement as the synchronous versions above and the same verdict, only off the
-- main thread: one capture of the control's own region, every template matched against
-- that single frame. `done(nil)` when there is nothing to say — a toggle whose templates
-- both miss has no state to report, and inventing one is worse than silence. And when the
-- search could not be made at all (`why`: the capture failed, or the module had too many
-- searches waiting): that is no answer, and "not found" would be a claim nobody checked.
local function couldNotLook(c, why)
  host.log.info(("[%s] '%s': could not look (%s), so no value is said"):format(
    tostring(c.kind), tostring(c.label), tostring(why)))
end

local function valueOfNow(self, c, done)
  -- The region, placed; or the whole origin for a graphical button without one (searchRect).
  -- One that cannot be placed now is no answer, like a search that could not be made.
  local region, why = searchRect(self, c)
  if not region then
    host.log.info(("[%s] '%s': no region now (%s), so no value is said"):format(
      tostring(c.kind), tostring(c.label), tostring(why)))
    return done(nil)
  end
  local opts = {
    region = region,
    tolerance = c.tolerance or MATCH_TOLERANCE,
    scales = c.scales or MATCH_SCALES,
  }
  if c.kind == "gtoggle" then
    local templates, labels = {}, {}
    addTemplates(templates, labels, c.onImage, "on")
    addTemplates(templates, labels, c.offImage, "off")
    if #templates == 0 then return done(nil) end
    host.screen.imageSearchAsync(templates, opts, function(hit, why)
      if hit == nil and why ~= nil then couldNotLook(c, why) end
      done(hit and labels[hit.n] or nil)
    end)
    return
  end
  -- A graphical button says only whether it is THERE. Present is the unremarkable case and
  -- gets no extra word (ReaHotkey's States maps found to ""); absent has to be said, because
  -- the alternative is a button that announces itself normally and then does nothing.
  if c.kind == "gbutton" then
    -- A caller may answer for the control before the search runs. For a button that is
    -- present and findable but cannot presently DO anything — Kontakt's "make it taller" once
    -- the plug-in is as big as it can safely get — where "not found" would be false and
    -- silence would be worse: a blind user tabbing past a button hears a button, and a button
    -- that announces itself normally and then declines is the failure this control type was
    -- introduced to stop. Returning nil falls through to the search as usual.
    if c.unavailable then
      local why = c.unavailable(self)
      if why then return done(why) end
    end
    -- `opts.region` is searchRect's already (above), so there is nothing to work out again.
    host.screen.imageSearchAsync(asList(c.image), opts, function(hit, why)
      if not hit and why ~= nil then
        couldNotLook(c, why)
        return done(nil)
      end
      if not hit then
        -- "not found" is the whole value of this control type, so when it says that, it has to
        -- say enough to check it. A region in the wrong coordinate frame, a scale the thing is
        -- never drawn at, a template cut from another state — all three produce the identical
        -- two words, and all three have now happened here.
        host.log.info(string.format(
          "[gbutton] '%s' not found: searched (%d,%d)-(%d,%d) for %d template(s) at %d scale(s)",
          tostring(c.label), opts.region[1], opts.region[2], opts.region[3], opts.region[4],
          #asList(c.image), #(c.scales or MATCH_SCALES)
        ))
        -- And SAVE WHAT WAS ACTUALLY LOOKED AT, once, while calibrating.
        --
        -- Everything else about a failed search can be checked from the log line above, and on
        -- this control all of it checked out while the answer stayed "not found" through four
        -- corrections. What could never be checked is the one thing that matters most: whether
        -- the pixels the search saw are the pixels the template was cut from. A template is
        -- always cropped from a CALIBRATION shot, and until now nothing compared that shot with
        -- what the live capture returns — so a difference between the two would look exactly
        -- like a wrong template, a wrong region and a wrong scale all at once.
        --
        -- Written only in a calibrating run, and only once per control, so a polling overlay
        -- cannot fill the disk with the same disappointment.
        if calibrating and not c._savedMiss then
          c._savedMiss = true
          -- Sanitised inline: safeName is declared with the calibration helpers far below this
          -- point, and a local is not visible above its own declaration.
          local stem = (string.gsub(tostring(c.label or "control"), "[^%w]+", "-"))
          local path = "calibration/miss-" .. stem .. ".png"
          host.screen.save(path, { region = opts.region })
          host.log.info("[gbutton] wrote what the search actually saw to " .. path)
        end
      end
      if hit and c.state then
        local ok, v = pcall(c.state, self, hit)
        if ok and v ~= nil and v ~= "" then return done(tostring(v)) end
      end
      -- NOT `done(hit and nil or "not found")`. That idiom is broken whenever the middle
      -- value is nil or false: `hit and nil` is nil even for a real hit, so the `or` branch
      -- always wins and every successful search reported "not found". Eight rounds went into
      -- the templates, the region, the scales, the coordinate frame and the identity scoping
      -- because the search really was succeeding and saying otherwise — a diagnostic that
      -- lies is worse than none, and this one was in the diagnostic itself.
      if hit then done(nil) else done("not found") end
    end)
    return
  end
  -- A slider: its value IS where the thumb is drawn. "not found" rather than a number
  -- when it cannot be located — a percentage invented from a failed search is worse than
  -- an admission, because it sounds right.
  host.screen.imageSearchAsync(asList(c.thumbImage), opts, function(hit, why)
    if not hit and why ~= nil then
      couldNotLook(c, why)
      return done(nil)
    end
    if not hit then return done("not found") end
    local pct = sliderPercent(self, c, { cx = hit.x + math.floor(hit.w / 2) })
    done(pct and (tostring(pct) .. " percent") or "not found")
  end)
end

-- Reading the value REVEALS first, so the region searched is one this control is actually
-- drawn in. Nothing else changes: the read is still asynchronous and still delivers through
-- `done`, so the reveal just adds itself to the front of a wait the caller already handles.
local function valueOf(self, c, done)
  ensureRevealed(self, c, function() valueOfNow(self, c, done) end)
end

-- What a toggle should ADD to its announcement once it is in a given state — ReaHotkey's
-- ExecuteOnActivationPostSpeech, which every u-he synth uses for the same sentence: turning
-- the preset browser on is only half the information, because nothing on screen and nothing
-- in the control's name says the arrow keys now pick presets. A sighted user sees a list
-- appear and infers it; this is the sentence that replaces seeing it.
--
-- Spoken on ACTIVATION only, not on focus (`withHint`), and that restraint is the point: a
-- hint repeated on every Tab pass through a header is noise a screen-reader user has to sit
-- through, and it says nothing new — they were told when they turned it on. Keyed BY STATE,
-- so it is tied to the thing that just became true rather than to the control.
local function hintFor(c, state)
  if not (c.hint and state) then return nil end
  return c.hint[state]
end

-- Text a recogniser read, made fit for one log line: rows joined with a space, and at most 80
-- bytes of it, cut on a character, not a byte — a split UTF-8 sequence is not a string the log
-- binding accepts, and the log line must never be what breaks the announcement. One helper for
-- every line that quotes a reading (a focused OCR control's `[read]`, an unnamed pass-through
-- stop's `[passthrough]`), so the two quote alike.
local function forLog(text)
  local shown = ((text or ""):gsub("[\r\n]+", " "))
  if #shown > 80 then
    shown = shown:sub(1, 80):gsub("[\192-\255][\128-\191]*$", "") .. "…"
  end
  return shown
end

-- One `host.ocr.recognize` key per overlay for what its announcements read — a control's value
-- region and its `ocrLabel` — so that a later announcement's read replaces one that has not
-- started recognising yet. Numbered like the pass-through's keys (readUnnamedStop, below), and
-- reserved with them: a module does not use keys that begin "com.platform.overlay ". The corners
-- these reads take are cut by readCorners, beside O:toScreenRect.
local focusReadKeys = 0

-- Text a reading says, as one phrase: its rows joined with a space. host.ocr.recognize joins rows
-- with a line break, and a read-out of two lines is still one value to say.
local function oneLine(text)
  return (text:gsub("%s*[\r\n]+%s*", " "))
end

-- What an OCR control's value says, from its region's reading (nil: a region with nothing of it
-- left on screen). Text is said as read. A region with nothing drawn in it, or nothing the
-- recogniser read, says the control's `fallback` — "no text" without one — as an empty read said
-- before the read moved off the event loop. A read that FAILED says "cannot be read now", as a
-- region that cannot be placed does: "no text" would describe a read that was never made.
local function valueWords(c, reading)
  if reading == nil then return c.fallback or "no text" end
  if reading.status == "text" and reading.text ~= "" then return oneLine(reading.text) end
  if reading.status == "failed" then return "cannot be read now" end
  return c.fallback or "no text"
end

-- What a reading's `[read]` line says after its rectangle, for a reading that is not text.
local READ_NOTE = {
  blank = "nothing drawn there",
  none = "the recogniser read nothing",
  stale = "not recognised",
}

-- What control `c`'s hook `hook` — its `text`, a tab control's `current` or `verticalName` —
-- answers now, called with the overlay and `...`: its value, or nil when it raised.
--
-- Guarded, as every hook here is, and no longer silently. A raise was swallowed where the sentence
-- is built, and before a step or an activation it came back as the VALUE — the error's own text —
-- so a stepper's watch compared one error message with the next and announced on a difference
-- between two messages. Now a raise is no value, which the watch takes as "cannot read right now",
-- and is logged once per control and hook: a hook that fails on every announcement does not bury
-- the log it is trying to explain itself in. That is also where a hook whose host call raises — a
-- region that is neither form, say — shows up, rather than as a value that went missing without a
-- word.
local function hookValue(self, c, hook, ...)
  local f = c[hook]
  if f == nil then return nil end
  local ok, v = pcall(f, self, ...)
  if ok then return v end
  c._hookRaised = c._hookRaised or {}
  if not c._hookRaised[hook] then
    c._hookRaised[hook] = true
    host.log.info(string.format("[overlay] '%s': its %s raised: %s", tostring(c.label), hook, tostring(v)))
  end
  return nil
end

-- What a sentence asks of its mark after each of its control's hooks (speakControl): the place,
-- the focus, the keys, the announcements, and a menu opened since. Not a menu open now: one open
-- before the hook is no news after it — the sentence was asked for with it open, and is said as it
-- always was (a stepper's value after a step chosen in a menu, still counted open for a tick) —
-- and an OCR read's answer asks about it itself, as it always has.
local AFTER_HOOK = { focus = true, keys = true, said = true, menusSince = true }

-- `queued` says this announcement was not asked for by a keypress, so it must not cut off
-- whatever is already being said.
--
-- Every other caller here follows a key the user just pressed, and interrupting is then
-- exactly right: they asked for this and not for the last thing. Arrival is the exception,
-- and the macOS tester found it — pressing the shortcut that puts the keyboard back in a
-- plug-in window, he heard "the first control that gets focus interrupts the notification of
-- where keyboard focus has gone". The sentence being cut off is the one that answered the
-- question he pressed the key to ask.
--
-- What this can and cannot do: `interrupt` governs OUR queue only. Whether VoiceOver cuts
-- itself off is VoiceOver's decision (see `speech/voiceover.rs`), so this fixes the case
-- where both lines are still ours — which is this one, the two being about 350 ms apart —
-- and cannot promise anything about a line VoiceOver has already taken.
--
-- `andThen(placed, dropped)` is what an activation does once an OCR control's sentence is settled
-- — said, or dropped as no longer true — and at once when there was nothing to read or the read
-- was refused: the OCR button's click (see activate), which so comes after the value it said, as
-- it did while the read held the event loop. `placed` says whether the value region could be
-- placed for the read; `dropped`, given only when a hook of the control's has outlived the
-- sentence (below), why. Only an OCR control's activation passes it.
local function speakControl(self, index, moveMouse, withHint, queued, andThen)
  local c = self.controls[index]
  if not c then return end
  -- Every announcement counts, whatever it says: an answer to a read asked for an earlier one
  -- describes a moment that has been talked over since (see the read below).
  self._said += 1
  -- WHERE THE OVERLAY IS AS THE SENTENCE IS ASKED FOR, before any of the control's hooks runs: a
  -- hook — `text`, a tab control's `current` and `verticalName` — runs in the module's handler and
  -- may wait for a read there, with the module's other events waiting behind it. What it waited
  -- through is checked against this mark after each hook: the stay, the window, the focus and this
  -- control, the keys of its own, the announcements (this one counted above) and a menu opened
  -- since. A sentence its hook has outlived is not said — not the hook's value, nor what the
  -- sentence would have been without it — and an activation's continuation is told so, and clicks
  -- nothing. Not a window comparison and no epoch: O:hwnd() resolves the window live for an
  -- attached overlay and per epoch for an embedded one, and the epoch turns before every answer.
  -- The read below asks the same mark.
  local mark = markAt(self, index)
  local function stillThere(hook)
    local fine, why = stillHereAt(self, mark, AFTER_HOOK)
    if not fine then
      host.log.info(string.format("[read] '%s' not spoken after its %s: %s", tostring(c.label or ""), hook, why))
      if andThen then andThen(false, why) end
    end
    return fine
  end
  -- A tab control announces WHICH tab is current, then that it is a tab control (so the
  -- user knows Left/Right — or Ctrl+Tab, Ctrl+number — switch it). Its current tab's
  -- controls are the ones Tab then steps into (see isFocusable's `when` gating).
  if c.kind == "tabcontrol" then
    if c.current then
      local n = hookValue(self, c, "current")
      if not stillThere("current") then return end
      if type(n) == "number" and c.tabs[n] then c._current = n end
    end
    local tab = c.tabs[c._current]
    if c.verticalName then
      local name = hookValue(self, c, "verticalName", c._current)
      if not stillThere("verticalName") then return end
      if type(name) == "string" and name ~= "" then
        tab = { label = name, hotkeyLabel = tab and tab.hotkeyLabel, hotkey = tab and tab.hotkey }
      end
    end
    host.speech.output(tabMessage(tab, c.typeLabel or "tab control"), { interrupt = true })
    self._spoken += 1
    return
  end
  -- A pass-through announces that Tab goes INTO the plugin from here, because nothing else
  -- would tell the user that: what follows is the plugin's own focus ring, announced by
  -- the screen reader rather than by us (bar what an unnamed stop shows, read off the
  -- screen), and it is not obvious that Tab will come back out again at the end. Landing on
  -- it also restarts the lap, so entering it, leaving by hotkey and coming back walks the
  -- whole thing again rather than resuming mid-way — and Shift+Tab from here goes back
  -- through the overlay, never into the plugin.
  if c.kind == "passthrough" then
    c._net, c._at, c._n, c._leaveOn = nil, nil, nil, nil
    host.speech.output((c.label or "") .. ", Tab to step through them", { interrupt = true })
    self._spoken += 1
    return
  end
  -- `ocrLabel`: read the control's NAME off the screen instead of announcing a fixed one,
  -- for a control whose meaning the plugin itself changes. A sample library can put a mixer
  -- strip in one place whose five channels are mic positions for one patch and orchestral
  -- sections for another — same buttons, same pixels, different names. A fixed label is
  -- then confidently wrong, which is worse than slow: it tells a user who cannot see the
  -- screen that they toggled something they did not. Falls back to the static label when
  -- OCR reads nothing, so a failed read never leaves a control nameless. Read with the value,
  -- below. A name region that cannot be placed now leaves the static label, as a read of nothing
  -- does.
  local labelRegion, labelAr = nil, nil
  if c.ocrLabel then labelRegion, labelAr = regionOf(self, c, "ocrLabel") end
  -- Everything the sentence says between the name and the value, worked out now, at the key,
  -- as it always was: whatever the read takes, it describes this moment.
  local rest = {}
  if c.text then
    local v = hookValue(self, c, "text")
    if not stillThere("text") then return end
    if v ~= nil and v ~= "" then rest[#rest + 1] = tostring(v) end
  end
  -- A `readOnly` OCR control is NOT a button, and must not say it is.
  --
  -- `readOnly` already suppresses the click, so the control does nothing when activated — but
  -- the announcement still ended in "button", which is a promise of an affordance that is not
  -- there. A screen-reader user hears "button", presses it, and nothing happens; the only
  -- honest reading of that is that the tool is broken. Announced like a static text instead
  -- (no type word at all), because that is what it is: a value you can read and not press.
  -- `typeLabel` overrides the word for what this control is, because the KIND a control is
  -- built from and the THING it stands for are not always the same. A custom button that puts
  -- the caret into a text field is an edit box to everyone using it; announcing "button"
  -- describes our implementation to somebody who cannot see the screen and has no interest in
  -- it — and worse, it says the wrong thing about what typing will do next. The tab control
  -- has had this for the same reason (a row of tool icons is not "a tab control").
  local tl = c.typeLabel or TYPE_LABEL[c.kind]
  if c.kind == "ocr" and c.readOnly then tl = "" end
  if tl ~= "" then rest[#rest + 1] = tl end
  -- The KEY, spoken on arrival. Without it a hotkey is only discoverable by being told, and
  -- the whole point of these overlays is that a plug-in with no accessible tree can still be
  -- learned from itself. Only on a focus read — repeating it when the user has just pressed
  -- the key is noise, and they plainly know it. A key the host refuses is not said: it is never
  -- registered, and the bind-time report has named it in the log.
  if moveMouse and c.hotkey and not hotkeyRefusal(c.hotkey) then rest[#rest + 1] = spokenKey(c.hotkey) end
  -- The region is placed first: a region function with no answer, or a scaled overlay with no
  -- factor, has nothing to read, and says "cannot be read now" — not "no text", which would
  -- describe a read that was never made.
  local region, ar = nil, nil
  if (c.kind == "ocr" or c.kind == "ocredit") and c.region then
    region, ar = regionOf(self, c)
    if not region then
      rest[#rest + 1] = "cannot be read now"
      host.log.info(string.format("[read] '%s' not read: %s", tostring(c.label or ""), tostring(ar)))
    end
  end
  if region and moveMouse and self.hoverToRead then
    -- Move the mouse onto the control on focus — only for overlays that opt in.
    -- Some plugin UIs reveal the value on hover; others (like sforzando) show a
    -- *different* value while hovered, so this is off by default. Before the read is asked,
    -- so its picture is taken with the pointer there.
    local hx, hy = rectCentre(self, c, ar)
    if hx then host.input.move(hx, hy) end
  end
  -- What follows the value: a hotspot toggle's state, read off one pixel now, as it always was.
  local after = {}
  if c.kind == "hotspottoggle" then
    local st = hotspotToggleState(self, c)
    if st then
      after[#after + 1] = st
      local h = withHint and hintFor(c, st) or nil
      if h then after[#after + 1] = h end
    end
  end
  -- The IMAGE-read values — a graphic toggle's state, a slider's position — are announced
  -- in TWO utterances rather than one, and the name goes out first.
  --
  -- Reading them means a screen capture, which costs a fixed compositor frame: measured at
  -- 20-42 ms per focus step, and that whole time the user hears nothing at all. Nothing is
  -- gained by their waiting for it — the name is known immediately, and the value is what
  -- takes the time. So the name is spoken at once and the value appended when it arrives,
  -- queued rather than interrupting, which reads as one phrase when it lands quickly.
  --
  -- This is the shape a screen reader already has, and it is why the read is worth doing
  -- ASYNCHRONOUSLY: the capture and the match move to the worker, off the thread that
  -- speech and hotkeys share, so the step costs what the speech costs.
  --
  -- Guarded on the focus not having moved on: two fast Tabs must not have the first
  -- control's state read out while the second one is focused, which would be a plain lie
  -- about what is in front of the user.
  local needsValue = c.kind == "gtoggle" or c.kind == "slider" or c.kind == "gbutton"
  -- Every late part of the sentence is checked against the mark taken above, before the hooks
  -- (O:stillHere): the stay, the window, the focus and this control, the keys of its own, the
  -- announcements — this one counted above — and the menus.
  local at, pinned = mark.at, mark.hwnd
  -- The sentence, once its name and its value are known, and then an image-read value.
  local function say(name, value)
    local parts = { name }
    for _, part in ipairs(rest) do parts[#parts + 1] = part end
    if value then parts[#parts + 1] = value end
    for _, part in ipairs(after) do parts[#parts + 1] = part end
    host.speech.output(table.concat(parts, ", "), { interrupt = not queued })
    -- Said, not only begun: what waits to follow the sentence asks this (O:stillHere, `spoken`).
    self._spoken += 1
    if not needsValue then return end
    valueOf(self, c, function(value)
      -- The utterance the user actually hears, logged for the one control type whose value
      -- has been in dispute. Everything upstream said "hit"; the ear said "not found". Only
      -- the words that leave here can settle which of the two the code really produced.
      if c.kind == "gbutton" then
        host.log.info(string.format(
          "[gbutton] '%s' value callback -> %s (focus %s/%s, active %s)",
          tostring(c.label), value == nil and "nil (say nothing)" or ("\"" .. tostring(value) .. "\""),
          tostring(self.focus), tostring(at), tostring(self.active)
        ))
      end
      if value and self.active and self.focus == at and self:hwnd() == pinned then
        -- A graphic toggle's value IS its state ("on"/"off"), so the hint attaches here,
        -- to the same late utterance. A slider's is a percentage and matches no hint key.
        local h = withHint and hintFor(c, value) or nil
        host.speech.output(h and (value .. ", " .. h) or value, { interrupt = false })
      end
    end)
  end
  -- THE READ IS OFF THE EVENT LOOP, and the sentence waits for it.
  --
  -- It was a read on the event loop, and on a Mac that loop also carries the keyboard's event
  -- tap. An Intel MacBook Air (2020) spent 143 to 786 ms in each focus read of sforzando, 1.5 s in
  -- the first, with every key, timer and hotkey of the application waiting behind it, and a key
  -- reached the tap 854 ms late. Now the name and the value are read with one
  -- host.ocr.recognize with a callback — one picture for both, so they agree — and the sentence is
  -- said when the answer comes: as late as it was, but nothing waits for it, not even the module. One sentence, as it always was,
  -- not the name now and the value later: two lines that fast Tabs can cross.
  --
  -- The answer is spoken only while it is still about what is in front of the user — the
  -- pass-through's unnamed stops are checked the same way (readUnnamedStop): not superseded by a
  -- later announcement's read (`newer`, `"stale"`); the overlay still active, on the same window,
  -- in the same stay in front; the focus where it was; no key of the overlay's own since (ownKey
  -- counts them), no other announcement since (counted above); and no menu open over the overlay,
  -- nor one opened since the read was asked and closed again (_menuChange counts them) — a menu
  -- the plug-in put up itself, or a choice made in it, after which the value read is the one
  -- from before. All but the first are the mark's (stillHereAt, with every option), the checks a
  -- module's own late answer asks through O:stillHere. The overlay's own `opensMenu` press puts
  -- its menu up only after its sentence (activate). Nothing waits on a timer: an answer is taken
  -- when it comes, or dropped as no longer true.
  --
  -- What the read says, reading by reading: see valueWords. The language is the user's, as for
  -- every read without `lang`; the read on the event loop without one used the platform's own
  -- default, which on Windows is the same language and on a Mac was Vision's (English).
  --
  -- The words the recogniser returned are logged per read, for the same reason as the gbutton
  -- line above. A read-out that said "550." where the screen showed "DEF" left only its timing
  -- in the log, and timing cannot tell a dialog's pixels over the region from a font-size
  -- change from the recogniser itself. The region is the authored one, in content coordinates,
  -- so the line can be held against the module's source; an empty read is logged as "" rather
  -- than skipped, because reading nothing is the case being reported. Reads here follow a
  -- keystroke — a focus step or an activation — or arrival, never a timer tick, so one line per
  -- read is one line per key. The words counted are the ones the recogniser placed; words whose
  -- place the host estimated are counted apart (`approx`: on Windows the second recogniser's
  -- answer, which is how a lone digit is read), as the read on the event loop told the two
  -- recognisers apart by "0 words". The language the read was made in closes the brackets.
  local labelAt = labelRegion and readCorners(labelRegion) or nil
  local valueAt = region and readCorners(region) or nil
  if region and not valueAt then
    host.log.info(string.format("[read] '%s' not read: its region is empty on screen (%s,%s to %s,%s)",
      tostring(c.label or ""), tostring(region[1]), tostring(region[2]), tostring(region[3]), tostring(region[4])))
  end
  -- After the sentence: an OCREdit drops the caret into the real edit control on focus, so the
  -- user can type to change the value right away. Read first (clean), then click — once the read
  -- is answered, so its picture is from before the click whatever the capture costs — and only
  -- while the focus is still where the click belongs: the click is the focus's, and a focus that
  -- has moved on, or a key of the overlay's own since, takes it along. `moveMouse` is true only on
  -- a focus read, so navigating away doesn't re-click. Then the activation's continuation.
  local function finish()
    if region and c.kind == "ocredit" and moveMouse then
      if stillHereAt(self, mark, { focus = true, keys = true }) then
        local cx, cy = rectCentre(self, c, ar)
        if cx then host.input.click(cx, cy) end
      else
        host.log.info(string.format("[read] '%s': the focus moved on before the answer; not clicking into "
          .. "the field", tostring(c.label or "")))
      end
    end
    if andThen then andThen(region ~= nil) end
  end
  local entries = {}
  if labelAt then entries[#entries + 1] = { name = "label", region = labelAt } end
  if valueAt then entries[#entries + 1] = { name = "value", region = valueAt } end
  if #entries > 0 then
    if not self._focusReadKey then
      focusReadKeys += 1
      self._focusReadKey = "com.platform.overlay focus " .. focusReadKeys
    end
    local t0 = host.now and host.now() or 0
    local ok, err = pcall(host.ocr.recognize, entries, { key = self._focusReadKey }, function(_, byName)
      local ms = (host.now and host.now() or 0) - t0
      local lr, vr = byName.label, byName.value
      local name = c.label or ""
      if lr and lr.status == "text" and lr.text ~= "" then name = oneLine(lr.text) end
      local value = region and valueWords(c, vr) or nil
      local why = nil
      if (lr and lr.status == "stale") or (vr and vr.status == "stale") then
        why = "a later announcement's read replaced it before it was recognised"
      elseif (lr and lr.newer) or (vr and vr.newer) then
        why = "a later announcement asked for another read"
      else
        local fine, because = stillHereAt(self, mark, { focus = true, keys = true, said = true, menus = true })
        if not fine then why = because end
      end
      local outcome = why and ("; not spoken: " .. why) or "; spoken"
      -- One line per region read: the name's first, when there is a value line to carry the outcome.
      local function line(head, r, authored, onScreen, last)
        local placed, estimated = 0, 0
        for _, w in ipairs(r.words or {}) do
          if w.approx then estimated += 1 else placed += 1 end
        end
        local note = READ_NOTE[r.status]
        if r.status == "failed" then note = "the read failed (" .. tostring(r.error) .. ")" end
        host.log.info(string.format(
          "%s = \"%s\" (region %s,%s %sx%s, %d ms, %d %s%s%s)%s%s%s",
          head, forLog(r.text), tostring(authored[1]), tostring(authored[2]),
          tostring(authored[3] - authored[1]), tostring(authored[4] - authored[2]),
          ms, placed, placed == 1 and "word" or "words",
          estimated > 0 and string.format(", %d placed by estimate", estimated) or "",
          (type(r.lang) == "string" and r.lang ~= "") and (", " .. r.lang) or "",
          (onScreen and scaledFor(self, c)) and string.format(" at %d,%d %dx%d on screen", onScreen[1],
            onScreen[2], onScreen[3] - onScreen[1], onScreen[4] - onScreen[2]) or "",
          note and (": " .. note) or "", last and outcome or ""
        ))
      end
      if lr then line(string.format("[read] '%s' name", tostring(c.label or "")), lr, labelAr, labelRegion, vr == nil) end
      if vr then line(string.format("[read] '%s'", tostring(name)), vr, ar, region, true) end
      if not why then say(name, value) end
      finish()
    end)
    if not ok then
      -- A read refused at the call: said at once, with what is known — the static name, and for a
      -- value "cannot be read now" — and the reason in the log.
      host.log.info(string.format("[read] '%s' could not be read: %s", tostring(c.label or ""), tostring(err)))
      say(c.label or "", region and "cannot be read now" or nil)
      finish()
    end
  else
    -- Nothing to read: said now, as it always was. A value region with nothing of it left on
    -- screen is answered as an empty read was.
    say(c.label or "", region and valueWords(c, nil) or nil)
    finish()
  end
end

-- ANNOUNCE WHAT THE CONTROL BECAME, not what we asked it to become.
--
-- A stepper's value is what it PRINTS, so that is what is watched: the announcement waits
-- until the printed text is no longer the one that was there before the key, and gives up at
-- the control's own settle time. A stepper that was already at its limit does not change, and
-- is announced then — which is the truth about it. `settle` therefore stops meaning "wait this
-- long" and starts meaning "wait AT MOST this long", the only version of it that can be right.
--
-- One helper rather than one per caller, because there are two ways to move a stepper —
-- pressing it and stepping it — and they must not drift apart. They were written separately
-- and were identical for it; the next change to either would have been the moment one of them
-- started announcing something the other did not.
--
-- `was` is the reading taken BEFORE the action, which the caller has and this cannot get.
local function announceWhenChanged(self, index, c, was)
  local say = function()
    -- Guarded on the focus not having moved on: an answer that arrives after two fast Tabs
    -- must not be read out against whichever control is focused by then.
    if self.active and self.focus == index then speakControl(self, index, false, true) end
  end
  self:watch({
    read = function() return c.text and hookValue(self, c, "text") or nil end,
    was = was,
    within = c.settle or 600,
    onDone = say,
    onGiveUp = say,
  })
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

-- Where the ring WOULD start, without moving focus there.
--
-- On arrival the overlay says what is in front of the user, and saying it must not be the same
-- act as standing on it: a focused control takes the keys it needs (see _syncNativeKeys), and
-- on arrival those keys still belong to the plug-in.
local function firstFocusable(self)
  -- One pass over the `when`s (see "Where a hook runs").
  return scanPass(self, function()
    for i = 1, #self.controls do
      if isFocusable(self, i) then return i end
    end
    return 0
  end)
end

-- A Tab step, timed in its two halves.
--
-- This is THE interaction path: everything the user does is a focus move followed by an
-- announcement, and any millisecond here is one they wait with nothing to hear. The two
-- halves cost for different reasons and want different fixes, so they are measured apart —
-- SCAN is the `when` predicates over every control (a screen pixel or a UIA search, per
-- control, per step), ANNOUNCE is reading the focused control's state (a pixel for a hotspot
-- toggle; an image search for a graphic toggle and a text read for an OCR value or an
-- `ocrLabel` are only asked for here — the read's own time is in its `[read]` line).
local function focusStep(self, advance, dir)
  local n = #self.controls
  if n == 0 then return end
  local t0 = host.now and host.now() or 0
  -- The scan, and only the scan, in one pass (see "Where a hook runs"): the announcement after
  -- it runs in the handler, where its `text` may wait for a read.
  local found = scanPass(self, function()
    for _ = 1, n do
      self.focus = advance(self.focus, n)
      if isFocusable(self, self.focus) then return true end
    end
    return false
  end)
  local tScan = host.now and host.now() or 0
  -- Nothing focusable (e.g. a control set that is only unfilled slots): don't
  -- announce a phantom empty stop with interrupt.
  if found then
    self:_syncNativeKeys()
    speakControl(self, self.focus, true)
  end
  local t1 = host.now and host.now() or 0
  if t1 - t0 >= 20 then
    local c = self.controls[self.focus]
    -- Direction and the input-epoch are in the line because both are hypotheses somebody
    -- has already voiced and neither can be checked without them: whether Shift+Tab is
    -- genuinely slower than Tab (it walks the same ring the other way, but the controls it
    -- has to SKIP differ), and what keeps invalidating the cached view probe when nothing
    -- has been clicked. A guess about either is cheap to state and expensive to act on.
    host.log.info(string.format(
      "[focus] %s %d ms to reach '%s' (scan %d over %d control(s), announce %d, gen %d)",
      dir, t1 - t0, tostring(c and c.label or "?"), tScan - t0, n, t1 - tScan,
      host.inputEpoch and host.inputEpoch() or -1
    ))
  end
end

function Overlay:focusNext()
  if self:_stepPassThrough(1) then return end
  focusStep(self, function(i, n) return (i % n) + 1 end, "Tab      ")
end

function Overlay:focusPrev()
  if self:_stepPassThrough(-1) then return end
  focusStep(self, function(i, n)
    -- From OUTSIDE the ring (focus 0, nobody in it yet), backwards means the LAST control.
    -- The modular step would answer n-1 from 0 and quietly skip the end of the ring, which
    -- is the one place a user reaches for when they suspect they missed something.
    if i == 0 then return n end
    return ((i - 2) % n) + 1
  end, "Shift+Tab")
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
-- plugin's OWN focus ring and walks it (host.element.focusStep, which raises a real focus
-- event, so the screen reader announces each element and we do not name it again); once
-- every stop has been visited, Tab carries on through the rest of the overlay's controls,
-- and Shift+Tab walks back out the way it came. For a plugin window that does not move
-- focus on Tab by itself (Kontakt standalone), this is the only way into its native controls.
--
-- `readUnnamed` (default on; only `false` turns it off): a stop that publishes no name is
-- read off the screen, in its own rectangle, and what it shows is spoken without cutting off
-- what is being said — see readUnnamedStop.
function Overlay:addPassThrough(opts)
  return add(self, {
    kind = "passthrough",
    label = (opts and opts.label) or "Plugin controls",
    when = opts and opts.when,
    readUnnamed = not (opts and opts.readUnnamed == false),
  })
end

-- Kept for the old spelling; it now adds the control rather than switching on a mode.
function Overlay:enableUiaPassThrough(opts)
  assertUnbound(self, "enableUiaPassThrough")
  self:addPassThrough(opts)
  return self
end

-- The characters a name can be made of and still show nothing, as UTF-8 byte patterns: Luau's
-- %s knows only ASCII white space, and a toolkit can put any of these where a label was left
-- blank — a no-break space most often, a zero-width space or a byte-order mark from text that
-- was pasted in. Each is a lead byte and what follows it, and a lead byte is never the middle
-- of another character, so no pattern can match inside a visible one.
local INVISIBLE = {
  "\194[\160\173]",       -- U+00A0 no-break space, U+00AD soft hyphen
  "\225\154\128",         -- U+1680 ogham space mark
  "\225\160\142",         -- U+180E mongolian vowel separator
  "\226\128[\128-\143]",  -- U+2000–U+200F spaces, zero-width space and joiners, direction marks
  "\226\128[\168-\175]",  -- U+2028–U+202F separators, direction embeddings, narrow no-break space
  "\226\129[\159-\164]",  -- U+205F–U+2064 mathematical space, word joiner, invisible operators
  "\226\129[\166-\175]",  -- U+2066–U+206F direction isolates, deprecated format characters
  "\227\128\128",         -- U+3000 ideographic space
  "\239\187\191",         -- U+FEFF zero-width no-break space (a byte-order mark)
}

-- Whether a name gives the screen reader anything to say. nil, "" and a name of nothing but
-- white space, control characters and the INVISIBLE ones above do not.
local function hasVisibleText(s)
  if type(s) ~= "string" then return false end
  for _, p in ipairs(INVISIBLE) do s = s:gsub(p, " ") end
  return s:find("[^%s%c]") ~= nil
end

local function trim(s)
  return (s:gsub("^%s+", ""):gsub("%s+$", ""))
end

-- How many rows of an unnamed stop's text are spoken. See readUnnamedStop.
local UNNAMED_ROWS = 2

-- One `host.ocr.recognize` key per overlay, so a later step's read supersedes an earlier one that has
-- not been recognised yet. Numbered rather than made from the label, which two overlays of one
-- module may share. Keys are the module's — this code runs in the VM of the module that owns
-- the overlay, and a read's key supersedes that module's own reads with the same key — so each
-- is named after this runtime, where a module's own key will not be: overlay.md reserves them.
local passThroughKeys = 0

-- A stop in the plugin's own ring that publishes NO NAME, read off the screen and spoken.
--
-- The screen reader announces every stop from the focus event, and for a named one that is the
-- whole announcement: the overlay says nothing over it. A stop without a name, though, is
-- announced as its role and nothing else. A Mac tester's log of Kontakt 7 standalone
-- (2026-09-27) had the ring mostly made of those — "[passthrough] Kontakt 7: Tab -> stop 15 of
-- 29 '', …", with FILE, LIBRARY, VIEW, SHOP, Search, Brand, Sound Type and Character the ones
-- that had a name — and the verdict was that the ring was unlabelled and not much use. What
-- such a stop does have is a place: focusStep hands back the landed element's rectangle, and
-- whatever the plugin drew there — a preset's name, a library's title — is what a sighted user
-- reads it by. So that rectangle is read, and its text is said after the screen reader's own
-- announcement, or beside it.
--
-- * Off the event loop (host.ocr.recognize with a callback): the Tab returns at once, and the
--   screen reader starts on the focus while the text is being recognised. The line is QUEUED
--   (interrupt = false), so it does not cut that announcement off. Whether it then waits for it is the speech
--   path's: a screen reader that takes the line queues it or not as it does; the Mac's own
--   voice is a second voice beside VoiceOver, and can overlap it (overlay.md, per platform).
-- * Cut to the plugin window's rectangle before it is read: an element can reach past the
--   window (a list row scrolled out of its view is still published), and what is drawn there
--   is another window's text — announcing that as this stop's would be a plain lie about what
--   is in front of the user. The cut also keeps a region inside the read's pixel limit, where
--   a raise would end this Tab's handling half way. Under 2 across or down, nothing is read.
--   What the cut cannot catch is a row scrolled out of its list's view but still inside the
--   window: the Mac ring admits such rows (TODO.md, the Mac mini session), and what is drawn
--   where one lies is another control. Whether that happens, and whether the focus scrolls it into
--   view first, only the live check can say; the host reports the element's own rectangle.
-- * The first two rows only, top to bottom: a stop is a control, and a control's text is a
--   line or two. A large unnamed element — a whole list — reads as many rows as it shows, and
--   the rest is dropped rather than read out on every step through it.
-- * Spoken only while nothing the overlay can see has moved on: not superseded by a later read
--   (`newer`), the overlay still active on the same window and in front without a break since
--   (`_stay`), the pass-through still focused and still inside the plugin, and no later key on
--   it since. Two fast Tabs must not have the first stop's text read out while the second is
--   focused. Nothing here waits on a timer: an answer is taken when it comes, or dropped as no
--   longer true. What it cannot see: a key that goes to the plugin's element itself (Space,
--   Return, an arrow on the stop are the plugin's, not the overlay's), and the speech queue —
--   the checks are made when the answer arrives, and a line queued behind a long announcement
--   is heard when that ends, after a Tab that came in between. TODO.md has both.
-- * One `[passthrough]` line per stop read, whatever became of it, so a log says what each
--   unnamed stop showed and why anything was not said. It gives the stop's control type, so a
--   log can say which kinds of element have no name and what they read as.
--
-- `ctrl` is the overlay's origin the step was made in: the plugin window.
local function readUnnamedStop(self, c, r, ctrl)
  local label = tostring(self.label)
  local head = string.format("[passthrough] %s: stop %d of %d (type %s) has no name", label,
    r.index, r.count, tostring(r.ctype))
  local b = r.bounds
  if type(b) ~= "table" or type(b.x) ~= "number" or type(b.y) ~= "number"
      or type(b.w) ~= "number" or type(b.h) ~= "number" then
    host.log.info(head .. " and no rectangle; nothing to read")
    return
  end
  -- Whole numbers covering the element: a point's fraction on a Mac widens the region rather
  -- than cutting the element's edge off.
  local x1, y1 = math.floor(b.x), math.floor(b.y)
  local x2, y2 = math.ceil(b.x + b.w), math.ceil(b.y + b.h)
  local whole = { x1, y1, x2, y2 }
  local wb = ctrl.bounds
  if type(wb) == "table" and type(wb.x) == "number" and type(wb.y) == "number"
      and type(wb.w) == "number" and type(wb.h) == "number" then
    x1, y1 = math.max(x1, math.floor(wb.x)), math.max(y1, math.floor(wb.y))
    x2, y2 = math.min(x2, math.ceil(wb.x + wb.w)), math.min(y2, math.ceil(wb.y + wb.h))
  end
  local where = string.format("%d,%d %dx%d", x1, y1, x2 - x1, y2 - y1)
  if x2 - x1 < 2 or y2 - y1 < 2 then
    -- The element's own rectangle, and — when the window's edge is what made it too small — the
    -- part of it inside the window, so the line does not call a 120-wide rectangle under 2.
    local cut = x1 ~= whole[1] or y1 ~= whole[2] or x2 ~= whole[3] or y2 ~= whole[4]
    local why
    if x2 <= x1 or y2 <= y1 then
      why = "outside the plugin's window"
    elseif cut then
      why = "under 2 across or down inside the plugin's window (" .. where .. ")"
    else
      why = "under 2 across or down"
    end
    host.log.info(string.format("%s; its rectangle %s,%s %sx%s is %s, nothing to read", head,
      tostring(b.x), tostring(b.y), tostring(b.w), tostring(b.h), why))
    return
  end
  if not self._passThroughKey then
    passThroughKeys += 1
    self._passThroughKey = "com.platform.overlay passthrough " .. passThroughKeys
  end
  local seq, at, pinned, stay = c._seq, self.focus, ctrl.id, self._stay
  local t0 = host.now and host.now() or 0
  local ok, err = pcall(host.ocr.recognize, { x1, y1, x2, y2 }, { key = self._passThroughKey }, function(reading)
    local ms = (host.now and host.now() or 0) - t0
    local function note(what)
      host.log.info(string.format("%s; read at %s in %d ms: %s", head, where, ms, what))
    end
    if reading.status == "stale" then
      note("not recognised, a later step's read replaced it")
      return
    end
    if reading.status == "failed" then
      note("the read failed (" .. tostring(reading.error) .. ")")
      return
    end
    if reading.status == "blank" then
      note("nothing drawn there")
      return
    end
    if reading.status == "none" then
      note("the recogniser read nothing")
      return
    end
    if reading.status ~= "text" then
      note("no text (" .. tostring(reading.status) .. ")")
      return
    end
    local rows, n = {}, 0
    for _, line in ipairs(reading.lines or {}) do
      local t = trim(tostring(line.text or ""))
      if t ~= "" then
        n += 1
        if #rows < UNNAMED_ROWS then rows[#rows + 1] = t end
      end
    end
    local text = trim(table.concat(rows, ", "))
    if text == "" then
      note("no text (only white space)")
      return
    end
    local quoted = string.format("\"%s\"%s", forLog(text),
      n > UNNAMED_ROWS and string.format(" (%d of %d rows)", UNNAMED_ROWS, n) or "")
    local why = nil
    if reading.newer then
      why = "a later step asked for another read"
    elseif not self.active then
      why = "the overlay is no longer active"
    elseif self._stay ~= stay then
      -- Out of the front and back on the same window — Alt+Tab, another overlay holding the
      -- slot for a while, a recheck that lost the match and found it again. Everything below
      -- still holds then: the focus resumes on the pass-through and the lap is not cleared until
      -- the stop is announced again, ANNOUNCE_DELAY later. An answer from before is about a
      -- stop the user heard before leaving, and would be said over their return.
      why = "the overlay left the front and came back since"
    elseif self:hwnd() ~= pinned then
      why = "the overlay is on another window now"
    elseif self.focus ~= at or self.controls[at] ~= c then
      why = "the focus has left the pass-through"
    elseif c._net == nil then
      why = "the keyboard has left the plugin's controls"
    elseif c._seq ~= seq then
      why = "a later key moved on"
    end
    if why then
      note(quoted .. ", not spoken: " .. why)
      return
    end
    host.speech.output(text, { interrupt = false })
    note(quoted .. ", spoken")
  end)
  if not ok then
    host.log.info(string.format("%s; its rectangle %s could not be read: %s", head, where, tostring(err)))
  end
end

-- One Tab inside the focused pass-through control. True when it consumed the key (focus
-- moved within the plugin, or back out onto this stop), false to let the key carry on
-- through the overlay's controls.
--
-- The plugin's controls are a LINE hung off this stop, not a ring: Tab goes in, Shift+Tab
-- comes back out the way it went in, and Tab leaves forwards once every stop has been
-- visited. The lap is counted in STEPS (`_net`, net steps forward since going in), not by
-- waiting for an index to come round again.
--
-- It was a ring whose only exit was coming back round to the entry index, and on a Mac
-- Kontakt 8 standalone that trapped the tester for ten minutes. Its ring has 90 stops, and
--   * Shift+Tab on this stop went INTO the plugin from its far end, so from 'Kontakt
--     controls' there was no way back to 'Kontakt file menu' — the only other stop — short
--     of a whole lap;
--   * Shift+Tab straight after going in wrapped round to stop 90, the long way;
--   * every return to the window re-announces this stop, which restarted the lap, and at
--     one press a second a lap of 90 never fitted between two of them;
--   * a count that changed restarted the lap as well.
-- ReaHotkey's own Kontakt 8 pass-through is linear (Kontakt8.ahk HandlePassThrough), and so
-- is this now. Leaving never touches the plugin's focus, so the screen reader never names a
-- plugin element over the overlay's own announcement of where the key went.
function Overlay:_stepPassThrough(dir)
  local c = self.controls[self.focus]
  if not (c and c.kind == "passthrough" and isVisible(self, c)) then return false end
  local ctrl = self:origin()
  if not (ctrl and ctrl.id) then return false end
  -- Every key this stop handles counts, whatever it does, so an unnamed stop's text that
  -- arrives after another press is known to be about a stop the user has already left.
  c._seq = (c._seq or 0) + 1
  local inside = c._net ~= nil
  local key = dir >= 0 and "Tab" or "Shift+Tab"
  local function clear()
    c._net, c._at, c._n, c._leaveOn = nil, nil, nil, nil
  end
  -- On the stop itself, Shift+Tab is an ordinary step back through the overlay's controls.
  if not inside and dir < 0 then return false end
  -- Every stop visited: the next Tab leaves, without touching the plugin's focus.
  if c._leaveOn == dir then
    host.log.info(string.format("[passthrough] %s: %s leaves after %d stop(s) of %d",
      tostring(self.label), key, c._net or 0, c._n or 0))
    clear()
    return false
  end
  -- Shift+Tab on the element Tab went in on (or before it, if a stop that refused the focus
  -- was stepped over on the way back) comes back out onto this stop. The plugin keeps its
  -- focus, so the next Tab goes in one element further on; that is cosmetic, not a trap.
  if inside and dir < 0 and c._net <= 1 then
    host.log.info(string.format("[passthrough] %s: Shift+Tab backs out onto '%s'",
      tostring(self.label), tostring(c.label)))
    speakControl(self, self.focus, true) -- clears the lap and says "…, Tab to step through them"
    return true
  end
  local r = host.element.focusStep(ctrl.id, dir)
  if not r then
    host.log.info(string.format("[passthrough] %s: %s found nothing that takes the focus",
      tostring(self.label), key))
    clear()
    return false -- nothing focusable in there: just move on
  end
  -- How far that step went. Normally one; more when stops in between refused the focus and
  -- were stepped over, which keeps such a lap from repeating stops — with one exception: stops
  -- skipped by the FIRST step, on the way in, are not counted (focusStep does not say where it
  -- started from), so a refusing stop just before the entry element costs one repeat of the
  -- entry element before Tab leaves. Harmless, and bounded. Only
  -- while the count holds: a count that changed means the plugin's tree changed shape (a panel
  -- shown or hidden), the old index means nothing in the new one, and one is the honest guess.
  -- Landing where it already was means nothing else would take the focus, which is a whole
  -- lap — counted as one, it would never end.
  local d = 1
  if inside and r.count == c._n then
    d = ((r.index - c._at) * dir) % r.count
    if d == 0 then d = r.count end
  end
  c._net = inside and (c._net + (dir >= 0 and d or -d)) or 1
  c._at, c._n = r.index, r.count
  -- Measured against the LATEST count, so a ring that shrinks can only end the lap early and
  -- one that grows can only make it longer — never endless.
  c._leaveOn = (dir >= 0 and c._net >= r.count) and 1 or nil
  host.log.info(string.format("[passthrough] %s: %s -> stop %d of %d '%s', %d into the lap%s",
    tostring(self.label), key, r.index, r.count, tostring(r.name), c._net,
    c._leaveOn and ", the next Tab leaves" or ""))
  -- A named stop is the screen reader's to announce, and it does. One with no name is read.
  if c.readUnnamed and not hasVisibleText(r.name) then readUnnamedStop(self, c, r, ctrl) end
  return true
end

-- A press's late announcement — a toggle's state once it has settled — not made, because a key of
-- the overlay's own came after the press: said in the log, as every announcement not made is.
local function notAnnounced(self, c)
  host.log.info(string.format("[watch] %s: '%s' not announced — a later key reached the overlay",
    tostring(self.label), tostring(c.label)))
end

function Overlay:activate(index)
  local i = index or self.focus
  local c = self.controls[i]
  if not c then return end
  if not isVisible(self, c) then
    if index ~= nil then return end -- a hidden control's hotkey no-ops
    -- Space or Return on the focused control, which has hidden itself since focus came to it:
    -- a view toggle does exactly that (Kontakt's 'Switch to classic view' and 'Switch to play
    -- view' share one hotkey, and pressing either hides it and shows the other), and now that
    -- a hotkey moves the focus, it is where the focus is after every such press. The control
    -- that took its place under the same key is the one meant; with none, say so — this used
    -- to return without a word, which from the keyboard is a key that does nothing.
    local found = false
    if c.hotkey then
      local key = keyOf(c.hotkey)
      -- One pass over the `when`s (see "Where a hook runs"); the move after it, outside.
      local j = scanPass(self, function()
        for j, d in ipairs(self.controls) do
          if j ~= i and d.hotkey and keyOf(d.hotkey) == key and isVisible(self, d) then return j end
        end
        return nil
      end)
      if j then
        i, c, found = j, self.controls[j], true
        self.focus = j
        self:_syncNativeKeys()
      end
    end
    if not found then
      host.speech.output((c.label or "That control") .. " is not available now", { interrupt = true })
      return
    end
  end
  -- A control that chooses an item from its menu, on an overlay with nothing to see a menu
  -- with, would open the menu and leave it there — with the overlay holding every key, because
  -- no test says a menu is up. So it does not open it at all. The bind-time report has named
  -- the control; this says it at the press.
  if c.kind == "hotspot" and c.menuItem ~= nil and not self._menu then
    host.log.info(string.format("[menu item] '%s': '%s' has a menuItem, but the overlay lists no "
      .. "menu tests to see its menu with — not opening it", tostring(self.label), tostring(c.label)))
    host.speech.output((c.label or "That control") .. " is not available now", { interrupt = true })
    return
  end
  -- Pressed again while its item is still waiting for its menu (a double Return, a held key):
  -- clicking the opener a second time would close the menu the first press is opening, and
  -- newWindow's new list would already hold it. The first press stands; this one is logged.
  if c.kind == "hotspot" and c.menuItem ~= nil and self:_pickRepeat(c) then return end
  -- Before the click, not after: a test that compares with the moment before the click
  -- (newWindow) needs its starting point from before the menu can exist. A single point counts
  -- as a press only once it has been placed and checked, just before its click: a press whose
  -- opener is never clicked must not set the tests running, take newWindow's list again or use
  -- up one of the control's menu shots. An OCR button counts likewise just before its click,
  -- which waits for its read and may not be made (below). A sequence counts from its first step,
  -- and any other control from its activation — its click is the module's own code.
  local onePoint = c.kind == "hotspot" and c.at ~= nil and not c.points
  local ocrPress = (c.kind == "ocr" or c.kind == "ocredit") and c.region ~= nil
  if c.opensMenu and not onePoint and not ocrPress then self:_menuOpening(c) end
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
      local x, y, why = placePoint(self, c, p[1], p[2])
      -- A step that cannot be placed now ends the sequence where it is, said once.
      if not x then return unplaced(self, c, why) end
      if refuseOutside(self, c, x, y) then return end
      host.input.click(x, y)
      -- No trailing delay after the last click: there is nothing left to settle for.
      if c.points[k + 1] then host.timer.after(settle, function() step(k + 1) end) end
    end
    step(1)
  elseif c.kind == "hotspot" and c.at then
    local px, py, why = hotspotPoint(self, c)
    if not px then
      -- A point that exists but cannot be placed now is said; an `at` function answering nil is
      -- the silent "no point" it has always been.
      if why then unplaced(self, c, why) end
      return
    end
    if refuseOutside(self, c, px, py) then return end
    if c.opensMenu then self:_menuOpening(c) end
    -- WHERE it clicked, in the log. A hotspot that presses nothing and a hotspot that presses
    -- the wrong thing are the same experience for someone who cannot see the screen — the
    -- control says "activated" either way — and telling them apart cost a whole test round
    -- because this line was not here. The image-searching button next door has always logged
    -- its outcome; these two, the ones with authored coordinates and therefore the ones most
    -- likely to be aimed wrong, did not.
    host.log.info(string.format("%s: '%s' activated, clicking (%d,%d)",
      tostring(self.label), tostring(c.label), px, py))
    -- `button`: a hotspot that opens its menu with the right button (VPS Avenger's redo list
    -- is a right click on UNDO). Passed only when given, so every other hotspot's click is the
    -- call it always was.
    if c.button ~= nil then
      host.input.click(px, py, { button = c.button })
    else
      host.input.click(px, py)
    end
    host.speech.output((c.label or "") .. ", activated", { interrupt = true })
    -- The menu this click opened has an item to choose: chosen when a menu test sees it (see
    -- "Choosing an item" in the Menus section).
    if c.menuItem ~= nil then self:_armPick(c, nil, c.retries) end
  elseif c.kind == "static" then
    -- Nothing to press, so activating a static text says it: its hotkey is pressed to hear it.
    -- Space and Return never get here, since a static text does not take them.
    speakControl(self, i, false)
  elseif (c.kind == "custom" or c.kind == "stepper") and c.onActivate then
    -- A stepper can be ACTIVATED as well as stepped, where the control it stands for has
    -- something a press means. ON:EAR's two are the case that asked for it: a double-click
    -- puts either of them back to its default, which is a thing worth one keystroke and was
    -- otherwise twenty. Without this, Space and Return were captured on a stepper — it is not
    -- an inert control — and then did nothing at all, which is the promise-without-an-action
    -- this runtime refuses everywhere else.
    --
    -- The module's own code, so it is called guarded. A control whose action throws was
    -- silent before this: the host caught the error at the key dispatch and wrote a line, and
    -- the person who pressed it heard nothing at all and had no way to tell that from a
    -- control that had simply done its job quietly. Reported as "the overlay crashed", which
    -- from the keyboard is exactly what it looks like.
    -- A stepper's value is read BEFORE the action, because afterwards is too late to know what it
    -- was: its announcement waits for that value to change (announceWhenChanged). Nobody else's
    -- `text` is read here. It was, and nothing used the answer: the read only held the press back
    -- — ON:EAR's "Search" read its field before clicking into it — or did the press a service on
    -- the side, which the module now asks for itself (ON:EAR's tiles learn where their grid sits).
    local was = (c.kind == "stepper" and c.text) and hookValue(self, c, "text") or nil
    local ok, err = pcall(c.onActivate, self) -- the overlay, so a header control can reach it
    if not ok then
      host.log.info(string.format("%s: '%s' failed while activating: %s",
        tostring(self.label), tostring(c.label), tostring(err)))
      host.speech.output((c.label or "That control") .. " could not be used",
        { interrupt = true })
    elseif c.kind == "stepper" then
      -- Activating a stepper changes its value, so the value is read again and announced —
      -- from the control, not from what the activation intended.
      announceWhenChanged(self, i, c, was)
    end
  elseif (c.kind == "ocr" or c.kind == "ocredit") and c.region then
    -- An OCR control re-reads its label on activate. An OCRButton then clicks the region
    -- centre (it sits over a clickable element), like ReaHotkey — unless it is `readOnly`
    -- (a pure value/label announcer, safe next to a destructive control). An OCREdit
    -- always clicks: activating it re-focuses the edit field for typing.
    --
    -- The click comes AFTER the sentence, once the read is answered, in the order it had while
    -- the read held the event loop: the value said is the one from before the click, however
    -- long the picture takes; the sentence does not cut off what the click opens (a screen
    -- reader's announcement of a dialog or a field — speech that interrupts stops it); and a
    -- menu the click opens cannot be up before the value is said, so its answer is not dropped
    -- for it. The click is the key's, so it is made only while the overlay is where the key was
    -- pressed — active, on the same window, in the same stay, the control still its own — and
    -- no other key of the overlay's own has come since: a user who pressed on has moved on, and
    -- a click landing after that, a menu opening over the control they went to, is the mistake
    -- pressing again cannot undo. A click not made is logged.
    local clicks = c.kind == "ocredit" or not c.readOnly
    -- Where the overlay is as the key is pressed. Not the focus: a hotkey can press an OCR button
    -- the focus is not on, and its click is the control's, wherever the focus went.
    local mark = markAt(self, i)
    speakControl(self, i, false, nil, nil, function(placed, dropped)
      if not clicks then return end
      -- A hook of the control's outlived the sentence (speakControl): the press is over.
      local fine, why = false, dropped
      if not dropped then fine, why = stillHereAt(self, mark, { control = true, keys = true }) end
      if not fine then
        host.log.info(string.format("[read] '%s': not clicking — %s", tostring(c.label), why))
        return
      end
      -- The press counts here, just before its click, as a single point's does: newWindow's
      -- starting point and a calibrating run's menu shots belong to the moment before the click.
      if c.opensMenu then self:_menuOpening(c) end
      local cx, cy, cwhy = ocrCenter(self, c)
      -- Quiet when the read has just said "cannot be read now" about the same region.
      if not cx then return unplaced(self, c, cwhy, not placed) end
      host.input.click(cx, cy)
    end)
  elseif c.kind == "gtoggle" and c.region then
    -- The press's keys, for its announcement below (see the hotspot toggle's).
    local keys = self._keys
    -- Reveal first, for the same reason the state read does: a click aimed at a control
    -- inside a collapsed panel lands on the panel, not the control.
    ensureRevealed(self, c, function()
      local cx, cy, why = ocrCenter(self, c)
      if not cx then return unplaced(self, c, why) end
      host.input.click(cx, cy)
      -- Re-read the new state once the UI redraws. `withHint`: this is the transition, the
      -- one moment a state hint is worth hearing (see hintFor).
      if host.timer then
        host.timer.after(150, function()
          if self._keys ~= keys then return notAnnounced(self, c) end
          speakControl(self, i, false, true)
        end)
      end
    end)
  elseif c.kind == "gbutton" and c.image then
    host.log.info(string.format("[gbutton] '%s' activated", tostring(c.label)))
    -- Located, then clicked, and never clicked unlocated. The search is asynchronous, so this
    -- is a continuation: it re-checks that the overlay is still active on the SAME window,
    -- exactly as the hotspot `points` sequence and the panel reveal do, because a click
    -- arriving after the world moved is the one mistake pressing the key again cannot undo.
    local pinned = self:hwnd()
    -- Where to look, placed before anything is said: a region that cannot be placed now is a
    -- search that cannot be made, and "activated" would promise one.
    local rect, rwhy = searchRect(self, c)
    if not rect then return unplaced(self, c, rwhy) end
    host.speech.output((c.label or "") .. ", activated", { interrupt = true })
    host.screen.imageSearchAsync(asList(c.image), {
      region = rect,
      tolerance = c.tolerance or MATCH_TOLERANCE,
      scales = c.scales or MATCH_SCALES,
    }, function(hit, why)
      if not (self.active and self:hwnd() == pinned) then
        -- Found, and deliberately NOT clicked, which was silent until it cost a diagnosis:
        -- a refusal that says nothing is indistinguishable from a search that never answered.
        host.log.info(string.format(
          "[gbutton] '%s': the overlay moved on before the search answered; not clicking",
          tostring(c.label)
        ))
        return
      end
      if not hit and why ~= nil then
        -- No search was made (the capture failed, or too many were waiting), so "not found"
        -- would be a guess. Said all the same: the acknowledgement already went out.
        couldNotLook(c, why)
        host.speech.output((c.label or "Control") .. " could not be looked for, try again", { interrupt = true })
        return
      end
      if not hit then
        -- Said, not silent: the acknowledgement above already went out, so leaving it here
        -- would report an action that did not happen.
        host.speech.output((c.label or "Control") .. " not found", { interrupt = true })
        return
      end
      local cx, cy = hit.x + math.floor(hit.w / 2), hit.y + math.floor(hit.h / 2)
      if c.clickOffset then
        -- An authored distance from where the template was found: scaled in a scaled overlay.
        local ox, oy, owhy = placeFrom(self, c, hit.x, hit.y, c.clickOffset[1], c.clickOffset[2])
        if not ox then return unplaced(self, c, owhy) end
        cx, cy = ox, oy
      end
      if refuseOutside(self, c, cx, cy) then return end
      if c.dragBy then
        -- `dragBy` may be a FUNCTION of where the handle was just found, not only a fixed
        -- offset. Kontakt's resize grip is why: dragging it down until the plug-in's corner
        -- passes under the taskbar leaves the grip unreachable, and a window that can be made
        -- bigger but never smaller again is worse than one that refuses to grow. A caller that
        -- knows its own limits can compute the step from the handle's actual position, and
        -- return nothing to decline — with a reason, so declining is not silence.
        --
        -- A function's answer is in screen pixels (it is worked out from `hit`, which is); a
        -- fixed {dx, dy} is authored, and scaled like any authored distance.
        local dx, dy, why
        if type(c.dragBy) == "function" then
          dx, dy, why = c.dragBy(self, hit)
        else
          dx, dy = c.dragBy[1], c.dragBy[2]
          if scaledFor(self, c) then
            local ex, ey, ewhy = placeFrom(self, c, 0, 0, dx, dy)
            if not ex then return unplaced(self, c, ewhy) end
            dx, dy = ex, ey
          end
        end
        if not dx or (dx == 0 and dy == 0) then
          host.speech.output(why or ((c.label or "Control") .. " cannot go further"),
            { interrupt = true })
          return
        end
        -- The DESTINATION is not checked against the origin: a drag that resizes a window is
        -- meant to end outside the window it started in, which is exactly what that guard
        -- forbids. The starting point is checked, and that is the one that must be right.
        host.input.drag(cx, cy, cx + dx, cy + dy)
      else
        host.input.click(cx, cy)
      end
    end)
  elseif c.kind == "hotspottoggle" and c.at then
    local px, py, why = hotspotPoint(self, c)
    if not px then
      if why then unplaced(self, c, why) end
      return
    end
    -- The same check the plain hotspot has had. A toggle clicks a coordinate too, and there is
    -- no reason its click should be the one allowed to land on somebody else's window.
    if refuseOutside(self, c, px, py) then return end
    -- The pixel BEFORE and the pixel AFTER, either side of the click.
    --
    -- This is the difference between a diagnosis and a guess. A toggle that reports the same
    -- state after being pressed has two completely different explanations — the click missed,
    -- or the control is one of a radio group and pressing the selected member is meant to do
    -- nothing — and from the outside they sound identical. Two pixel readings separate them:
    -- unchanged means nothing happened where we clicked, changed means the application
    -- responded and the announcement is simply telling the truth.
    --
    -- The reads cost a compositor frame each, which is affordable here and nowhere near the
    -- keystroke path: this runs when somebody deliberately pressed something.
    --
    -- A read of the screen can wait, so the overlay must still be where the key was pressed
    -- before the click: one landing in a window that came to the front meanwhile is the mistake
    -- pressing again cannot undo. Not clicked, it is logged.
    local mark = markAt(self, i)
    local before = host.screen.pixel(px, py)
    local fine, why = stillHereAt(self, mark)
    if not fine then
      host.log.info(string.format("[overlay] %s: '%s' not clicking after reading its pixel — %s",
        tostring(self.label), tostring(c.label), why))
      return
    end
    -- And the point asked again after the read: a tooltip or a notification drawn over it
    -- meanwhile leaves the overlay where it was and takes the click all the same.
    if refuseOutside(self, c, px, py) then return end
    host.input.click(px, py)
    -- Announced when the state has actually CHANGED, not a fixed moment later. A control that
    -- redraws in thirty milliseconds is announced in thirty; one that takes half a second is
    -- announced correctly instead of early. A control that does not change at all — the
    -- already-chosen member of a radio group — is announced at the deadline, which is the
    -- truth about it.
    --
    -- And announced only while it is still the user's last word: a key of the overlay's own since
    -- the press — a Tab on to the next toggle, a hotkey — has been answered by that key, and the
    -- press's sentence would take its place, its read the place of that control's read (one key
    -- for the overlay's announcement reads, see speakControl). Not announced, it is logged.
    if host.timer then
      local keys = self._keys
      local say = function()
        local after = host.screen.pixel(px, py)
        local function name(q) return q and string.format("%d,%d,%d", q.r, q.g, q.b) or "?" end
        host.log.info(string.format(
          "%s: '%s' pressed at (%d,%d) — pixel %s -> %s%s",
          tostring(self.label), tostring(c.label), px, py, name(before), name(after),
          (before and after and before.r == after.r and before.g == after.g
            and before.b == after.b) and " (unchanged: the click changed nothing there)" or ""
        ))
        if self._keys ~= keys then return notAnnounced(self, c) end
        speakControl(self, i, false, true)
      end
      self:watch({
        read = function() return hotspotToggleState(self, c) end,
        every = 120,
        within = 900,
        onDone = say,
        onGiveUp = say,
      })
    end
  end
end

-- The first of `candidates` (control indices) that is currently visible, or nil when
-- none is — i.e. which control a shared hotkey belongs to right now.
-- One pass over their `when`s (see "Where a hook runs"); what the caller does with the answer —
-- a hotkey's activation — runs after it, outside.
function Overlay:_visibleClaimant(candidates)
  return scanPass(self, function()
    for _, i in ipairs(candidates) do
      if isVisible(self, self.controls[i]) then return i end
    end
    return nil
  end)
end

-- A control's hotkey also puts the overlay's focus on that control, silently — ReaHotkey's
-- TriggerHotkey does the same (ActivateControlByID sets the current control, then activates
-- it). Without it, Ctrl+L for 'Load instrument' opened the dialog, and closing the dialog
-- brought the overlay back on whatever the tester had been on before ('Plugin size'), so a
-- second go needed Tab first. Silent, because the activation says what happened; the new
-- focus is heard when the window comes back, or on the next Tab.
--
-- `hotkeyKeepsFocus` opts out, for a hotkey that should act without moving the user —
-- Melodyne's 'Menu bar': focus on it would hold Space, which is Melodyne's play/stop.
function Overlay:_focusForHotkey(i)
  local c = self.controls[i]
  if not (self.active and c) or i == self.focus or c.hotkeyKeepsFocus then return end
  local was = self.controls[self.focus]
  host.log.info(string.format("[focus] '%s': a hotkey moved focus from '%s' to '%s'",
    tostring(self.label), tostring(was and was.label), tostring(c.label)))
  self.focus = i
  self:_syncNativeKeys()
end

-- Runs `fn` once the modifier keys are up, or after a short grace period regardless.
--
-- A hotkey callback fires while its own combination is still HELD — you are still holding Alt
-- when Alt+V arrives. Anything the callback then synthesises inherits those modifiers: the
-- view toggle sent F10 and the application received Alt+F10, which is not Kontakt's shortcut,
-- so the view stayed put while the DAW's screen-reader extension announced something else.
-- The same trap applies to a synthesised CLICK, where Alt+click is a different gesture
-- entirely in a lot of UIs.
--
-- ReaHotkey solves it by waiting (AccessibilityOverlay.ahk:1073-1075: `KeyWait` for every key
-- of the combination), and this is that, asynchronously — we cannot block the thread that
-- carries speech and the keyboard hook. Bounded, and it acts ANYWAY when the bound is reached:
-- a key held down unusually long must not mean the control silently never fires.
--
-- What made this findable, after a wrong diagnosis: pressing Enter on the same control always
-- worked, and Enter is a captured key with no modifier attached. Same callback, same F10, one
-- difference — what was still under the user's finger.
local MODIFIER_GRACE_TICKS = 40 -- x 15 ms ≈ 600 ms

local function afterModifiersReleased(fn, tries)
  if not (host.keys and host.keys.modifiersDown and host.timer) then return fn() end
  if not host.keys.modifiersDown() then return fn() end
  tries = (tries or 0) + 1
  if tries > MODIFIER_GRACE_TICKS then return fn() end
  host.timer.after(15, function() afterModifiersReleased(fn, tries) end)
end

-- A key of the overlay's own, wrapped: before it does its work, the menu tests hear that the
-- user is back in the overlay (Overlay:_menuOwnKey, in the Menus section). While a menu counts
-- as open these keys are not delivered at all — they belong to the menu — so reaching this
-- means no menu took the keys since the last press.
--
-- One exception to "before": while an item is still waiting for its menu (O:chooseMenuItem, a
-- hotspot's `menuItem`), the key is looked at first. A second press of the same control — a
-- double Return, a held arrow on a stepper over a zoom list — arrives before the first press's
-- menu has been seen; taken as "the user moved on", it dropped the first press, and newWindow's
-- list, taken again for the second, already held the first press's popup, while the second
-- click on the opener closed it. So the key runs first: a repeat of the waiting control is
-- dropped there (Overlay:_pickRepeat), and any other key — or a handler that raises — moves on
-- from the waiting press as before, once the key has run, unless the key itself made a new
-- press, which has taken over from it already.
local function ownKey(self, fn)
  return function(...)
    -- Counted first, whatever the key does: a focus read's answer that arrives after it is
    -- about a moment the user has moved on from (speakControl).
    self._keys += 1
    local p = self._pick
    if not p or p.clicked then
      self:_menuOwnKey()
      return fn(...)
    end
    p.keyPending = true
    local function after(...)
      if self._pick == p and p.keyPending then
        p.keyPending = nil
        self:_menuOwnKey()
      end
      return ...
    end
    local ok, a, b, c, d = pcall(fn, ...)
    if not ok then
      after()
      error(a, 0)
    end
    return after(a, b, c, d)
  end
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
  -- While a plug-in menu is open this overlay owns no keys at all (see
  -- _suspendControlHotkeys), and a re-sync from any other cause must not quietly take them
  -- back — a tab switch or an origin change while a menu is up would otherwise undo it.
  if self._hotkeysSuspended then return end
  for _, id in ipairs(self.controlHotkeys or {}) do
    host.hotkey.unregister(id)
  end
  self.controlHotkeys = {}
  local o = self:origin()
  self._hotkeyOriginId = (o and o.id) or false
  -- Keyed by the key each spec stands for on this platform, not by its spelling — see keyOf.
  local claim, specs = {}, {}
  for i, c in ipairs(self.controls) do
    if c.hotkey then
      local key = keyOf(c.hotkey)
      local held = claim[key]
      if held then
        held[#held + 1] = i
      else
        claim[key] = { i }
        specs[#specs + 1] = key
      end
    end
  end
  local took, skipped = {}, {}
  for _, spec in ipairs(specs) do
    local candidates = claim[spec]
    -- A combination the host refuses (see hotkeyRefusal) is left out rather than asked for:
    -- asking raises, and raising here would take the rest of the activation with it. The
    -- declaration report has already said why. The registration is still guarded, so a refusal
    -- this list does not know about costs one key and says so, not the activation.
    local refused = hotkeyRefusal(spec)
    if refused then
      skipped[#skipped + 1] = spec .. " (" .. refused .. ")"
    elseif self:_visibleClaimant(candidates) then
      local ok, id = pcall(host.hotkey.register, spec, ownKey(self, function()
        -- The claimant is resolved AFTER the wait, not before: the point of resolving at press
        -- time is that the visible set can have changed, and waiting is time in which it can.
        afterModifiersReleased(function()
          local i = self:_visibleClaimant(candidates)
          if i then self:_focusForHotkey(i) end -- visible already, so its `when` is not asked again
          self:activate(i or candidates[1])
        end)
      end))
      if ok then
        took[#took + 1] = spec
        self.controlHotkeys[#self.controlHotkeys + 1] = id
      else
        skipped[#skipped + 1] = spec .. " (refused: " .. tostring(id) .. ")"
      end
    else
      -- Every claimant hidden: the combination is deliberately NOT taken, so it reaches the
      -- application instead of being swallowed by a control that cannot appear here.
      skipped[#skipped + 1] = spec
    end
  end
  -- Said out loud, and only when it CHANGES, because "I pressed the key and something else
  -- happened" has three causes that look identical from outside: we never asked for the key,
  -- we asked and the OS refused, or we had it and gave it up (see _suspendControlHotkeys).
  -- Only the middle one was ever logged.
  --
  -- Not said at all by an overlay that declares no hotkey control. `took` and `skipped`
  -- partition its hotkey specs, so both are empty exactly when there are none — and then
  -- there is nothing that a visible-set change could move between them. The only line such
  -- an overlay could print is `holds:  | not taken: `, once, at its first sync: a change
  -- from nothing to nothing, one per hotkey-less overlay in the start-up log.
  if #specs == 0 then return end
  local line = table.concat(took, " ") .. " | not taken: " .. table.concat(skipped, " ")
  if line ~= self._hotkeyReport then
    self._hotkeyReport = line
    host.log.info(string.format("[keys] '%s' holds: %s", tostring(self.label), line))
  end
end

-- ReaHotkey's HotkeyMode 0: while a plug-in menu is open, this overlay owns NO keys.
--
-- The captured navigation keys were already handled — Tab, Return and Space fall through to
-- an open menu (see host.keys.menuOpen). The per-control hotkeys were not, and they are the
-- ones that matter, because they are RegisterHotKey claims and those OUTRANK the
-- application: with Kontakt's file or snapshot menu up, Alt+P and Alt+M never reached the
-- menu at all. The overlay ate them and re-ran its own action against a UI that already had
-- a menu open, acknowledging a keystroke the menu never received.
--
-- Dropping the registration rather than merely ignoring the press is the point. Ignoring
-- would still swallow the key — the OS hands it to us and we would discard it, which is
-- indistinguishable from a broken tool. Unregistered, the key reaches the menu, which is
-- where the user was aiming and what ReaHotkey does.
--
-- Idempotent: the menu timer calls this on every tick, for every overlay of the module in
-- front (see applyMenuState in the Menus section).
function Overlay:_suspendControlHotkeys(suspend)
  if suspend == (self._hotkeysSuspended == true) then return end
  self._hotkeysSuspended = suspend
  -- Logged on every transition, because this is the state in which the overlay deliberately
  -- owns none of its keys — and an overlay that has silently stopped responding to its own
  -- hotkeys is indistinguishable from a broken one. It cost a round trip to find once (Alt+V
  -- reaching REAPER instead of us, because the menu probe was answering a system-wide
  -- question); it should never cost that again.
  host.log.info(string.format(
    "[keys] '%s' %s its per-control hotkeys (menu %s)",
    tostring(self.label), suspend and "gave up" or "took back", suspend and "open" or "closed"
  ))
  if suspend then
    for _, id in ipairs(self.controlHotkeys or {}) do
      host.hotkey.unregister(id)
    end
    self.controlHotkeys = {}
  elseif self.active then
    self:_syncControlHotkeys()
  end
end

-- `suspended`: the per-control hotkeys start given up, and applyMenuState takes them back — an
-- overlay whose hold over its own menu's window ended with the plug-in having the keyboard again
-- (see the Menus section, "holding its place"), so that the hand-back is the one transition that
-- says so in the log, whenever the menu closed. They also start given up while a menu still counts
-- as open; at an ordinary activation none does.
function Overlay:_registerHotkeys(suspended)
  self.registered = self.registered or {}
  self.capturedKeys = self.capturedKeys or {}
  self.capturedSpecs = {}
  self._typing = false
  self._hotkeysSuspended = suspended == true or (self._menu ~= nil and self._menu.open == true)
  self:_syncControlHotkeys()
  for _, c in ipairs(self.controls) do
    -- A tab control's individual TABS may each carry a hotkey (ReaHotkey's Tab.SetHotkey):
    -- pressing it brings that tab to the front from anywhere in the overlay (like
    -- Ctrl+<n>), and its label is spoken in the tab's announcement (see tabMessage). KK
    -- doesn't bind any, but other plugins (e.g. Dubler) do — so the mechanism is here.
    if c.kind == "tabcontrol" then
      for tn, tab in ipairs(c.tabs) do
        local refused = tab.hotkey and hotkeyRefusal(tab.hotkey)
        if refused then
          -- Refused by the host (see _syncControlHotkeys), so not asked for: asking would raise
          -- out of the activation.
          host.log.info(string.format("[keys] '%s': tab '%s' hotkey %s is not registered (%s)",
            tostring(self.label), tostring(tab.label), tostring(tab.hotkey), refused))
        elseif tab.hotkey then
          local n = tn
          local ok, id = pcall(host.hotkey.register, tab.hotkey, ownKey(self, function()
            afterModifiersReleased(function() self:gotoTab(n) end)
          end))
          if ok then
            self.registered[#self.registered + 1] = id
          else
            host.log.info(string.format("[keys] '%s': tab '%s' hotkey %s is not registered: %s",
              tostring(self.label), tostring(tab.label), tostring(tab.hotkey), tostring(id)))
          end
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
    -- Kept alongside the token so the whole set can be handed back and taken again without
    -- rebuilding the overlay's registration (see _suspendCapturedKeys).
    self.capturedSpecs[#self.capturedSpecs + 1] = { spec = spec, fn = fn }
  end
  -- Calibration keys, only while calibration is on (see the Calibration section). Captured,
  -- so they act on the overlay that is active — the one that knows its own coordinates.
  if calibrating then
    -- Say so once per VM: whether the setting took is otherwise invisible, and "I pressed
    -- the key and nothing happened" has two very different causes.
    if not calibrationAnnounced then
      calibrationAnnounced = true
      host.log.info(string.format(
        "[calibrate] armed — %s shot, +T template, +V verify (active overlay)",
        host.keys.describe("Ctrl+Alt+Shift+S", { style = "short" })
      ))
    end
    -- NOT the Ctrl+Alt+F-keys: graphics drivers commonly claim those system-wide, and a
    -- key another process holds never reaches our hook at all.
    --
    -- On a Mac these are Command+Option+Shift (Ctrl is the Command role there), which is off
    -- Control+Option — VoiceOver's modifier, taken before our event tap ever sees it.
    cap("Ctrl+Alt+Shift+S", function() self:calibrationShot() end)
    cap("Ctrl+Alt+Shift+T", function() self:calibrationCrop() end)
    cap("Ctrl+Alt+Shift+V", function() self:calibrationVerify() end)
  end
  -- Tab moves *between* elements; arrow keys are left for value changes within a
  -- control (standard interaction scheme), so we don't capture them here.
  do
    cap("Tab", ownKey(self, function() self:focusNext() end))
    cap("Shift+Tab", ownKey(self, function() self:focusPrev() end))
    -- Return is captured per FOCUS, not statically: while the pass-through control has
    -- the focus, the plugin's own control is what should receive it (open a combo, toggle
    -- a check box). Same reasoning as Space below. See _syncNativeKeys.
    -- Tab controls (ReaHotkey): Left/Right switch the tab when the tab control itself
    -- is focused (LeftRightHK); Ctrl+Tab / Ctrl+Shift+Tab switch it from anywhere in
    -- the overlay (ControlTab/ControlShiftTabHK); Ctrl+<n> jumps straight to tab n (our
    -- addition). Captured only when this overlay has a tab control, so these keys stay
    -- free everywhere else.
    if self._hasTabControl then
      -- Ctrl+Tab and Ctrl+<n> stay STATIC: they are ours everywhere in the overlay, and no
      -- application uses them for anything a user would miss. Plain Left and Right do NOT —
      -- they moved to _syncNativeKeys, so the tab control owns them only while it is focused.
      --
      -- Statically was wrong and it cost a real application its keyboard: with the overlay up
      -- on Melodyne, its arrow keys stopped reaching the editor entirely, and the user's own
      -- diagnosis is the right rule — everything should pass through except what the CONTROL
      -- IN FOCUS explicitly wants. A tab control three stops away wanting Left is not a reason
      -- to take Left away from the thing the user is actually working in.
      --
      -- Control+Tab on a Mac as well, which is the Mac's own next-tab key: there the spec's Ctrl
      -- is Command, and Command+Tab is the application switcher, which the system keeps (the
      -- host's `check` says `reserved`) and which an event tap must never take from a blind
      -- user. So the Mac's pick names the Control key by its role, Win (`Meta`). Ctrl+<n> is
      -- Command+<n> there, the tab key of the Mac's own tabbed windows.
      cap(host.os.pick {
        windows = "Ctrl+Tab",
        macos = "Meta+Tab",
        linux = "Ctrl+Tab",
      }, ownKey(self, function() self:cycleTab(1) end))
      cap(host.os.pick {
        windows = "Ctrl+Shift+Tab",
        macos = "Meta+Shift+Tab",
        linux = "Ctrl+Shift+Tab",
      }, ownKey(self, function() self:cycleTab(-1) end))
      local _, tcCtrl = findTabControl(self)
      local ntabs = tcCtrl and #tcCtrl.tabs or 0
      for n = 1, math.min(ntabs, 9) do
        cap("Ctrl+" .. n, ownKey(self, function() self:gotoTab(n) end))
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
  -- Cleared with the registrations: a suspension is a statement about keys this overlay
  -- currently holds, and it holds none now. Left set, the next activation would find itself
  -- suspended for a menu that closed long ago.
  self._hotkeysSuspended = false
  for _, token in ipairs(self.capturedKeys or {}) do
    host.keys.release(token)
  end
  self.capturedKeys = {}
  self.capturedSpecs = {}
  self._typing = false
  if self._spaceToken then
    host.keys.release(self._spaceToken)
    self._spaceToken = false
  end
  if self._returnToken then
    host.keys.release(self._returnToken)
    self._returnToken = false
  end
  -- EVERY per-focus capture, and the list is the hazard. These are held outside
  -- `capturedKeys` because they come and go with the focused control, so each one has to be
  -- named here as well — and the vertical pair was added without being added here, which left
  -- Up and Down captured globally after the overlay went away. The symptom was arrow keys
  -- misbehaving in an unrelated application's text box, which points nowhere near this file.
  for _, field in ipairs({ "_arrowTokens", "_verticalTokens" }) do
    if self[field] then
      for _, t in ipairs(self[field]) do host.keys.release(t) end
      self[field] = false
    end
  end
  -- Forget what was last reported, not "report nothing": the next sync must be free to say
  -- whatever it takes, and a stale record here would let it stay silent about it.
  self._nativeKeyReport = nil
end

-- Hands back EVERY key this overlay holds, or takes them all again.
--
-- Letting named keys through one at a time is a patch on a hole whose shape is not known: any
-- key nobody thought of stays swallowed, and for a person who cannot see the screen a swallowed
-- key in a text box is indistinguishable from the application having frozen. The tester put it
-- exactly right — the fix is to recognise WHEN the field has the keyboard, and hold nothing at
-- all for as long as it does, whatever the key.
--
-- The way back is the application's own: Tab moves the focus out of its text editor, the
-- overlay's condition goes false on the next check, and the keys come back. Nothing here can
-- lock, because while it is on there is nothing left to lock with. The worst it can do is go
-- inert, which announces itself the moment Tab does not move the ring.
function Overlay:_suspendCapturedKeys(on)
  if on then
    for _, token in ipairs(self.capturedKeys or {}) do
      host.keys.release(token)
    end
    self.capturedKeys = {}
  else
    local specs = self.capturedSpecs or {}
    self.capturedKeys = {}
    for _, e in ipairs(specs) do
      self.capturedKeys[#self.capturedKeys + 1] = host.keys.capture(e.spec, e.fn)
    end
  end
end

-- The condition under which this overlay holds nothing: `fn() -> boolean`, asked whenever
-- focus moves and on every recheck. A module supplies it because only the module can tell —
-- ON:EAR's answer is that its accessibility tree goes dark exactly while the caret is in its
-- search box, which its chooser gate is measuring anyway.
function Overlay:typingWhen(fn)
  self._typingWhen = fn
  return self
end

-- Keeps the two keys that ACTIVATE a control — Space and Return — in sync with what is
-- focused, because for two kinds of control they belong to something else:
--
--   ocredit       an editable field: Space must reach it, or the user cannot type a space
--                 in a preset name. Return still activates.
--   editable      the same statement made by a control of any kind — its activation puts the
--                 caret in a text field. ON:EAR's speaker search is one: it has to be found
--                 by coordinate rather than by an OCR region, because the panel it lives in
--                 scales with the window, and being built as a custom button is no reason for
--                 a space in "White 80s" to be swallowed by the overlay instead of typed.
--   passthrough   the plugin's OWN control has the real keyboard focus, so both belong to
--                 it — Space toggles its check box, Return opens its combo. Capturing
--                 either here would swallow the interaction the pass-through exists for.
--
-- Idempotent; called on every focus move and at registration. No-op unless active, and while
-- the overlay holds its place over its own menu's window, when it holds no keys at all (see the
-- Menus section, "holding its place").
function Overlay:_syncNativeKeys()
  if not self.active or self:_heldOnMenu() then return end
  -- Everything, or nothing at all: while the field has the keyboard this overlay is not
  -- entitled to any key, and the question is asked before any of the per-control ones below.
  if self._typingWhen then
    local ok, v = pcall(self._typingWhen, self)
    local typing = ok and v == true
    if typing ~= (self._typing == true) then
      self._typing = typing
      -- Counted BEFORE, because suspending empties the list — and the count is the point.
      -- The line used to state the intention only, which on a platform where the key layer is
      -- a different mechanism entirely is not the same as the outcome: "the overlay let go and
      -- the field still cannot take a space" and "it worked" wrote the identical line. A zero
      -- here says the suspension had nothing to hand back and the swallowing comes from
      -- somewhere else; a number followed by a still-dead Space puts it in the key layer.
      local held = #(typing and (self.capturedKeys or {}) or (self.capturedSpecs or {}))
      self:_suspendCapturedKeys(typing)
      host.log.info(string.format("%s: %s (%d key(s) %s)",
        tostring(self.label),
        typing and "a text field has the keyboard — holding no keys until it gives it back"
          or "the text field gave the keyboard back — navigation keys taken again",
        held,
        typing and "handed back" or "taken again"))
    end
    if typing then
      if self._spaceToken then
        host.keys.release(self._spaceToken)
        self._spaceToken = false
      end
      if self._returnToken then
        host.keys.release(self._returnToken)
        self._returnToken = false
      end
      return
    end
  end
  -- Nil while the ring has not been entered (focus 0, the state an overlay arrives in): every
  -- `want` below is then false and every branch releases, which is exactly right. The plug-in
  -- keeps its keyboard until a Tab says otherwise.
  local c = self.controls[self.focus]
  local passthrough = c ~= nil and c.kind == "passthrough"
  -- Only where pressing actually DOES something. A static text has no activation, and taking
  -- Space from an application to give it to a control that ignores it is pure loss — in
  -- Melodyne, Space is transport play/stop, so the overlay was silently disabling the one key
  -- that gets used most.
  local inert = c == nil or passthrough or c.kind == "static"
    or (c.kind == "ocr" and c.readOnly)
  local wantSpace = not inert and c.kind ~= "ocredit" and not c.editable
  local wantReturn = not inert
  if wantSpace and not self._spaceToken then
    self._spaceToken = host.keys.capture("Space", ownKey(self, function() self:activate() end))
  elseif not wantSpace and self._spaceToken then
    host.keys.release(self._spaceToken)
    self._spaceToken = false
  end
  if wantReturn and not self._returnToken then
    self._returnToken = host.keys.capture("Return", ownKey(self, function() self:activate() end))
  elseif not wantReturn and self._returnToken then
    host.keys.release(self._returnToken)
    self._returnToken = false
  end
  -- Left/Right belong to a focused SLIDER, and only then. Arrows are otherwise left alone
  -- on purpose (the standard scheme keeps them for value changes inside whatever control
  -- has focus), which is exactly what this is — the overlay's slider IS that control.
  -- Skipped when the overlay also has a tab control, which claims Left/Right statically;
  -- the declaration report says so out loud rather than leaving a slider silently inert.
  local wantArrows = c ~= nil
    and (c.kind == "slider" or c.kind == "tabcontrol" or c.kind == "stepper")
  if wantArrows and not self._arrowTokens then
    -- The same two keys mean different things to the two controls that want them, so which
    -- one is focused decides what they do.
    local isTab = c.kind == "tabcontrol"
    self._arrowTokens = {
      host.keys.capture("Left", ownKey(self, function()
        if isTab then self:switchTab(-1) else self:adjust(-1) end
      end)),
      host.keys.capture("Right", ownKey(self, function()
        if isTab then self:switchTab(1) else self:adjust(1) end
      end)),
    }
  elseif not wantArrows and self._arrowTokens then
    for _, t in ipairs(self._arrowTokens) do host.keys.release(t) end
    self._arrowTokens = false
  end
  -- Up and Down, only while a tab control that HAS a second axis is focused. Left alone
  -- otherwise, on the same principle as everything else here: the application keeps every key
  -- the focused control does not itself need.
  local wantVertical = c ~= nil and c.kind == "tabcontrol" and c.verticalStep ~= nil
  if wantVertical and not self._verticalTokens then
    self._verticalTokens = {
      host.keys.capture("Up", ownKey(self, function() self:stepTabVertical(-1) end)),
      host.keys.capture("Down", ownKey(self, function() self:stepTabVertical(1) end)),
    }
  elseif not wantVertical and self._verticalTokens then
    for _, t in ipairs(self._verticalTokens) do host.keys.release(t) end
    self._verticalTokens = false
  end
  -- Said out loud, like the hotkey report and for the same reason: a key this overlay holds is
  -- a key the plug-in does not get, and from the outside that is indistinguishable from the
  -- plug-in ignoring the key. The defect this report was added for — the tool bar holding all
  -- four arrows from the moment Melodyne came forward — would have been one line here instead
  -- of an evening of hypotheses. Only when the set CHANGES, so Tab does not fill the log.
  local held = {}
  if self._spaceToken then held[#held + 1] = "Space" end
  if self._returnToken then held[#held + 1] = "Return" end
  if self._arrowTokens then held[#held + 1] = "Left Right" end
  if self._verticalTokens then held[#held + 1] = "Up Down" end
  local report = string.format("%s (on '%s')",
    #held > 0 and table.concat(held, " ") or "none",
    tostring(c and c.label or "nothing focused — the plug-in has the keyboard"))
  if report ~= self._nativeKeyReport then
    self._nativeKeyReport = report
    host.log.info(string.format("[keys] '%s' focus keys: %s", tostring(self.label), report))
  end
end

-- One step of the focused control's value, from Left/Right. Only a slider has one today.
function Overlay:adjust(dir)
  local c = self.controls[self.focus]
  if not (c and isVisible(self, c)) then return end
  if c.kind == "slider" then
    sliderStep(self, c, dir)
  elseif c.kind == "stepper" and c.onStep then
    local i = self.focus
    local was = c.text and hookValue(self, c, "text") or nil
    local ok, err = pcall(c.onStep, dir, self)
    if not ok then
      host.log.info(string.format("%s: '%s' failed while stepping: %s",
        tostring(self.label), tostring(c.label), tostring(err)))
      host.speech.output((c.label or "That control") .. " could not be moved",
        { interrupt = true })
      return
    end
    -- `false` from onStep: this press moved nothing and has nothing of its own to say — a press
    -- that came while an earlier step is still being carried out, whose own announcement is on
    -- its way. Watching the value for it too would say that announcement twice, or the old value
    -- first.
    if err == false then return end
    -- What the value BECAME is the only honest thing to announce; what we asked it to become
    -- is a guess that sounds exactly as confident. `c.settle` is read here, at each step, so an
    -- onStep that knows nothing will change (the end of its range) sets it short before it
    -- returns, and the value is said at once.
    announceWhenChanged(self, i, c, was)
  end
end

-- Deprecated name; the capture covers Return as well now.
function Overlay:_syncSpaceCapture()
  return self:_syncNativeKeys()
end

-- WAIT FOR SOMETHING TO CHANGE, rather than for a length of time.
--
-- Almost every `host.timer.after` in this project is a guess about how long an application
-- takes to do something, and a guess is wrong in both directions. Too short and the value read
-- back is the one from before the action — which is the worst answer an overlay can give,
-- because it is a real value announced with confidence for the wrong moment; ON:EAR's speaker
-- grid did exactly that, naming the previously loaded speaker each time a tile was pressed.
-- Too long and every action feels slow. Komplete Kontrol says the quiet part out loud: it
-- schedules the same recheck at 250, 600 and 1000 milliseconds, because nobody knows which.
--
-- So: read what matters, do the thing, then look again until the reading is different — or
-- until a deadline says the application is not going to change it. What comes back is what the
-- window actually says, and it comes back as soon as it is true rather than when a number
-- guessed it would be.
--
--   read      what to look at. Called with the overlay. nil means "cannot read right now",
--             which is not the same as "unchanged" and never counts as done.
--   was       the reading from before the action. Omitted, `read` is called once to get it —
--             which is only correct if nothing has happened yet.
--   done      (now, was) -> boolean. Default: it changed.
--   every     how often to look. Each look costs whatever `read` costs, and a screen pixel is
--             a compositor frame, so this is not free.
--   within    give up after this long and call `onGiveUp` with the last reading. Not an error:
--             a radio button that was already chosen is meant not to change.
--
-- Bound to the overlay, so it stops the moment the overlay is no longer active or the window
-- it was watching is no longer the one in front. A callback arriving after the world moved is
-- the mistake that cannot be undone by pressing the key again.
-- ONE ACTION PER BURST OF KEYSTROKES, not one per keystroke.
--
-- `key` names the thing being coalesced, `ms` is how long a quiet spell has to be, and `fn`
-- runs once when one arrives. Every call with the same key restarts the clock, so a run of
-- presses produces exactly one call at the end of it.
--
-- The reason is a specific failure, found from both directions on one control. Two clicks
-- close together in time and space are a DOUBLE-CLICK, and on ON:EAR's Width slider a
-- double-click resets it — so arrow keys pressed quickly enough were two clicks two pixels
-- apart, and the slider jumped back to its default in the middle of an adjustment. Waiting out
-- the double-click time between presses instead would make holding an arrow useless. So the
-- presses accumulate in the caller's own state and the gesture goes out once they stop.
--
-- Bound to the overlay: a pending action is dropped if the overlay is no longer active or the
-- window it was aimed at is no longer in front. A click that lands after the world has moved
-- is the mistake pressing the key again cannot undo, and it is the reason this is a method
-- rather than a bare timer the caller sets up for itself.
function Overlay:afterIdle(key, ms, fn)
  if type(fn) ~= "function" then return end
  if not host.timer then
    -- No timer: there is nothing to coalesce WITH, so the alternative to acting now is never
    -- acting at all.
    pcall(fn, self)
    return
  end
  self._idle = self._idle or {}
  local gen = (self._idle[key] or 0) + 1
  self._idle[key] = gen
  local pinned = self:hwnd()
  host.timer.after(ms, function()
    if self._idle[key] ~= gen then return end       -- another press came after this one
    if not self.active or self:hwnd() ~= pinned then return end
    self._idle[key] = nil
    local ok, err = pcall(fn, self)
    if not ok then
      host.log.info(string.format("%s: coalesced '%s' failed: %s",
        tostring(self.label), tostring(key), tostring(err)))
    end
  end)
end

function Overlay:watch(spec)
  if type(spec.read) ~= "function" or not host.timer then return end
  local was = spec.was
  if was == nil then
    local ok, v = pcall(spec.read, self)
    was = ok and v or nil
  end
  local done = spec.done or function(now) return now ~= nil and now ~= was end
  local every = spec.every or 100
  local left = spec.within or 900
  local pinned = self:hwnd()
  -- What this cost and how it ended, in one line at each exit.
  --
  -- Without it the two ways of ending are indistinguishable afterwards, and they mean opposite
  -- things. "It changed" is the control answering; "it gave up" is either a control that was
  -- already where it was asked to go — a radio button already chosen — or a reading that never
  -- worked at all. The third is the one that matters on a platform nobody here can run: if
  -- `read` returns nil every time, nothing is ever done, every watch burns to its deadline, and
  -- the announcement is the value from BEFORE the key was pressed, said with full confidence.
  -- Printing the last reading separates those, because a nil prints as nil.
  --
  -- The elapsed figure is wall clock rather than the deadline arithmetic, because those two are
  -- not the same number: `left` counts in whole `every`s while the timer that drives it is
  -- drained by the GUI toolkit's own tick, and on a platform where that tick is slower a "900
  -- ms" watch is quietly something else.
  local t0 = (host.now and host.now()) or 0
  local function report(finished, now)
    if not host.log then return end
    host.log.info(string.format(
      "[watch] %s: %s after %d ms — %s -> %s",
      tostring(self.label),
      finished and "changed" or "gave up",
      ((host.now and host.now()) or 0) - t0,
      tostring(was), tostring(now)
    ))
  end
  local function look()
    if not self.active or self:hwnd() ~= pinned then return end
    local ok, now = pcall(spec.read, self)
    if not ok then now = nil end
    local finished = false
    if now ~= nil then
      local okd, v = pcall(done, now, was)
      finished = okd and v == true
    end
    if finished then
      report(true, now)
      if spec.onDone then pcall(spec.onDone, now, was) end
      return
    end
    left -= every
    if left <= 0 then
      report(false, now)
      if spec.onGiveUp then pcall(spec.onGiveUp, now, was) end
      return
    end
    host.timer.after(every, look)
  end
  host.timer.after(every, look)
  return was
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

-- How long the announcement waits after activation, so it does not land on top of the
-- screen reader's own window-title announcement and get clobbered. Named because it is a
-- FIXED part of every activation the user experiences, and any measurement that does not
-- separate it out is measuring a deliberate pause and calling it slowness.
-- LOOK AGAIN SHORTLY AFTER ACTIVATING, once.
--
-- Every screen probe in every module has the same exposure, and it is not a flaky reading — it
-- is the wrong window. `host.screen.pixel` returns the COMPOSITED desktop, and for a moment
-- after a focus change that still shows whatever was in front before. Measured on Kontakt's
-- view probe: with REAPER behind a browser the same point reads 255,255,255 from the screen and
-- 99,99,99 from the window rendered by itself.
--
-- Activation is exactly when this bites, because that IS the focus change, and it is also when
-- a user is listening hardest. Rejecting improbable values does not save it: Kontakt's two
-- states read 24 and 99, and 24 is what almost any dark chrome reads, so a stale glance is
-- accepted as a confident answer.
--
-- So the fix has to be temporal, and it belongs here rather than in each module: 250 ms after an
-- overlay comes up, the generation counter turns over and the overlay reconsiders. Anything that
-- caches a screen reading keys on `O.stateGeneration()` and is re-read exactly once; anything
-- that reads fresh each time is re-read by the recheck itself. One extra probe per activation.
local stateGen = 0
function O.stateGeneration()
  return stateGen
end

local CONFIRM_DELAY = 250

local ANNOUNCE_DELAY = 350

-- Whether an overlay of this VM other than `self` is active. The key scope and the menu flag are
-- the MODULE's, one of each for all its overlays (keys.md, "Your module's own"): an overlay that
-- comes to the front pins the scope to its window, and only the last one to leave sets it back to
-- every window. Komplete Kontrol's standalone overlay comes back before its Preferences overlay
-- leaves — they are rechecked in the order they were bound — and the Preferences overlay's
-- `scope(false)` left Tab, Space and the arrows captured in every window until the standalone one
-- went again.
local function anotherActive(self)
  for o in pairs(overlaysMade) do
    if o ~= self and o.active then return true end
  end
  return false
end

function Overlay:_activate()
  if self.active then return end
  self.active = true
  -- Which stay in front this is. An answer that was asked for in an earlier one — an unnamed
  -- pass-through stop's text (readUnnamedStop) — is not spoken in this one, even when every
  -- other check says the overlay is where it was.
  self._stay += 1
  if host.timer then
    host.timer.after(CONFIRM_DELAY, function()
      if not self.active then return end
      stateGen = stateGen + 1
      self:_recheck()
    end)
  end
  -- The other half of the interaction path, timed like the Tab step already is.
  --
  -- A Tab step is a focus move inside an overlay that is already up; this is everything
  -- BEFORE that — the window changed, and the question is how long until anything speaks.
  -- The two halves cost for different reasons: a focus step pays for reading one control's
  -- state, whereas activation pays for DECIDING WHICH OVERLAY, which today means every
  -- landmark search on the slot resolving before the arbiter can settle. Nothing measured
  -- that, so "the KK overlay takes longer than the Kontakt one" had no number attached.
  --
  -- `gate` is from the moment a CONTEXT matched (the window/control was found) to now: the
  -- landmark searches, the arbiter, and any polling ticks in between. It is the part that
  -- grows with the number of overlays, which is why it is reported on its own.
  if host.now and self._ctxSince then
    host.log.info(string.format(
      "[activate] '%s' gate %d ms (%d recheck(s)), then %d ms before it speaks",
      tostring(self.label), host.now() - self._ctxSince, self._ctxChecks or 0, ANNOUNCE_DELAY
    ))
  end
  -- The overlay starts FRESH — focus back on the first control, any tab control back on
  -- its first tab — when it becomes active on a genuinely NEW window, but resumes where
  -- the user was when the SAME still-open window merely regained the foreground (Alt+Tab
  -- out and back). We tell them apart by the origin window's HWND (`.id`): closing then
  -- reopening a window yields a new id → reset; Alt+Tab keeps the same id → resume. (A
  -- window reusing the very same HWND across close/reopen would resume too, but these
  -- plugin/dialog windows are destroyed and recreated, so their id changes.) Clearing
  -- focus here lets the `if self.focus == 0` block below re-pick the first focusable.
  --
  -- Guarded, as the `identify` the origin may ask is: `active` and the stay are set already, and
  -- an origin that raised here left an overlay that was active and held no keys. A module's
  -- `control` function can still raise; that is said, and the overlay comes up whole, as on a
  -- window whose handle it does not know.
  local okOrigin, origin = pcall(self.origin, self)
  if not okOrigin then
    host.log.info(("[overlay] %s: its origin raised as it came to the front: %s")
      :format(tostring(self.label), tostring(origin)))
    origin = nil
    self._originRaised = host.epoch and host.epoch() or nil
  end
  local id = (origin and origin.id) or false
  if id ~= self._lastActiveId or not self._resume then
    self.focus = 0
    for _, c in ipairs(self.controls) do
      if c.kind == "tabcontrol" then c._current = 1 end
    end
  end
  self._lastActiveId = id
  -- THE FIRST FOCUSABLE CONTROL IS WHERE THE USER ARRIVES, and it is focused for real —
  -- spoken, and holding whatever keys it needs. A resumed activation (Alt+Tab out and back to
  -- the same window) keeps wherever the user was instead.
  --
  -- This was briefly not so, for half a day. Arriving left the focus at 0 — "not in the ring" —
  -- on the theory that a first control which claims Space and all four arrows must not claim
  -- them before the user has asked for the overlay at all. Two things were wrong with it. The
  -- behaviour was rubbish: you came into the plug-in, heard the overlay name a control, pressed
  -- the key that control had just described, and nothing happened — being told where you are
  -- and not being there is worse than either alone. And the premise was wrong too. It came from
  -- a report of arrow keys not reaching Melodyne, which I read as the focused tab control
  -- holding them; the user then established that the arrows were dead with the focus on a
  -- STATIC TEXT three stops away, which no focus-scoped capture can explain. Whatever holds
  -- them is elsewhere, and moving the arrival position only hid the question.
  if self.focus == 0 then self.focus = firstFocusable(self) end
  self:_refocusIfHidden()
  -- Scope key suppression to this (now-foreground) window: when a control opens a menu
  -- (another window comes forward), Tab/Enter pass to it natively, and resume here once
  -- the plugin window is focused again. Pinned BEFORE the keys are taken: the hook runs on a
  -- thread of its own, and a capture made first would take its key in every window until the pin.
  host.keys.scope(true)
  self:_registerHotkeys()
  -- Guarded, because a module's own activate hook must not be able to stop the overlay from
  -- coming up. One did: a bad key spec threw out of it, activation aborted half-done, and the
  -- symptom reported was "the plug-in is not detected any more" — which is a long way from the
  -- cause. The error is still reported, it just no longer takes the overlay with it.
  if self._onActivate then
    local ok, err = pcall(self._onActivate, self)
    if not ok then
      host.log.info(("[overlay] %s: onActivate failed: %s"):format(tostring(self.label), tostring(err)))
    end
  end
  -- Announce the focused control shortly after the window gains focus, once the
  -- screen reader's window-title announcement has passed (so it isn't clobbered).
  if host.timer and self.focus > 0 then
    -- How LATE the announcement actually is, which is a different question from how long it
    -- was asked to wait. Timer callbacks, speech and hotkeys share one thread with the
    -- landmark poll, and the poll currently asks for more time than a tick contains — so a
    -- delay of 350 ms can be served at 800 without a single thing being "slow". Reported
    -- only when it overruns, because a punctual timer is not news.
    local asked = host.now and host.now() or nil
    -- A key of the overlay's own before the announcement — a Tab in the first moments — has said
    -- where the user is, with the key's own sentence, and the arrival's would take its place:
    -- its read under the same key as that sentence's, its words queued and without the key.
    local keys = self._keys
    host.timer.after(ANNOUNCE_DELAY, function()
      if asked then
        local late = host.now() - asked - ANNOUNCE_DELAY
        if late > 20 then
          host.log.info(string.format(
            "[activate] '%s' spoke %d ms late — the timer thread was busy",
            tostring(self.label), late
          ))
        end
      end
      if self.active and self._keys ~= keys then
        host.log.info(string.format("[activate] '%s': a key reached it before its arrival was announced; "
          .. "not announcing the arrival", tostring(self.label)))
        return
      end
      -- Queued, not interrupting: see `speakControl`. Nobody pressed a key to hear this.
      if self.active then
        -- Again, for a view that changed during the delay.
        if self:_refocusIfHidden() then self:_syncNativeKeys() end
        if self.focus > 0 then speakControl(self, self.focus, false, false, true) end
      end
    end)
  end
end

-- A RESUMED focus on a control that has hidden itself since: moved to the visible control that
-- shares its hotkey, else to the first focusable one. True when it moved.
--
-- Now that a hotkey moves the focus, this is the ordinary state after a view toggle: Alt+V
-- leaves focus on 'Switch to classic view', Kontakt switches, and that control is hidden a
-- moment later. Alt+Tab out and back then announced the hidden control — "Switch to classic
-- view" while Kontakt was already in classic view — and Return on it ran the other one.
function Overlay:_refocusIfHidden()
  if self.focus <= 0 then return false end
  local c = self.controls[self.focus]
  -- Both searches in one pass (see "Where a hook runs"); the move and its line after it, outside.
  local hidden, to = scanPass(self, function()
    if isFocusable(self, self.focus) then return false end
    if c and c.hotkey then
      local key = keyOf(c.hotkey)
      for j, d in ipairs(self.controls) do
        if j ~= self.focus and d.hotkey and keyOf(d.hotkey) == key and isFocusable(self, j) then
          return true, j
        end
      end
    end
    return true, firstFocusable(self)
  end)
  if not hidden then return false end
  local was = c and c.label
  self.focus = to
  local now = self.controls[self.focus]
  host.log.info(string.format("[focus] '%s': '%s' is hidden now; resuming on '%s'",
    tostring(self.label), tostring(was), tostring(now and now.label or "nothing")))
  return true
end

function Overlay:_deactivate()
  if not self.active then return end
  self.active = false
  -- One line, naming what took over. A Mac Kontakt 8 session came back to the overlay seven
  -- times in ten minutes, each return restarting the pass-through's lap, and nothing in the
  -- log said what had taken it away in between — the tester switching apps, or the overlay
  -- losing its own window.
  local a = host.window.active()
  -- With nothing in front, what host.window.foreground() says: active() drops a hidden window,
  -- which can still have the keyboard.
  local f = ""
  if not a then
    local read, fg = pcall(host.window.foreground)
    if not read then
      f = (" — foreground could not be read (%s)"):format(tostring(fg))
    elseif type(fg) ~= "table" then
      -- nil is no foreground window, or one the platform cannot name: a macOS application that
      -- does not answer, a read that timed out. The words the menu's own reading uses.
      f = " — foreground: no window the platform names"
    else
      f = (" — foreground %s, pid %s, shown %s"):format(
        type(fg.id) == "number" and ("0x%x"):format(fg.id) or tostring(fg.id),
        tostring(fg.pid), tostring(fg.shown))
    end
  end
  host.log.info(string.format("[deactivate] '%s' — in front now: '%s' (%s)%s",
    tostring(self.label), tostring(a and a.title or "nothing"),
    tostring(a and a.app and a.app.exe or "?"), f))
  -- Its keys go FIRST, while the scope still pins them to the window it is leaving: unpinned
  -- first, they would take a key pressed in the window that came forward — the hook runs on a
  -- thread of its own — and the key would then be dropped, its capture released.
  self:_unregisterHotkeys()
  -- The scope and the flag are the module's (see anotherActive): the last overlay to leave sets
  -- them back. While another is still in front, its pin stands, and the flag is what the ones in
  -- front say — this one's no longer counts.
  if anotherActive(self) then
    self:_menuFlagLeft()
  else
    host.keys.scope(false)
    host.keys.menuOpen(false)
  end
  -- The menu state belongs to this stay in front: every test's word and any answer still
  -- outstanding are dropped, and the tests forget the press they were comparing against (see
  -- the Menus section).
  self:_menuLeave()
  if self._onDeactivate then self._onDeactivate(self) end
end

-- Re-evaluates the overlay's contexts against the current state (active window +
-- focus chain): the first matching context wins and supplies the coordinate
-- origin. In arbiter mode (attached with a `slot`) the match is reported to the
-- host arbiter, which decides — by specificity, across every overlay on that slot
-- — whether this overlay activates; otherwise the overlay activates/deactivates
-- itself directly (the legacy single-overlay path).
-- A recheck, timed IN ITS PARTS.
--
-- Seven attempts to name the cost from outside — the arbiter, timer resolution, the menu
-- watch, uncoalesced events, the template cache, Lua table conversion, screen pixels — and
-- every one of them was wrong, while the dispatch measurement kept saying the same thing:
-- eight callbacks, forty to sixty milliseconds each. The mistake was reasoning about WHICH
-- host call must be expensive instead of asking the loop itself. So it is asked here, split
-- where the work actually divides: finding the window this overlay lives in, and deciding
-- whether what is loaded inside it is ours.
local RECHECK_SLOW_MS = 15
-- The bar for a recheck made by the landmark POLL rather than by an event. 15 ms is what the
-- dispatch measurement above was made to catch, and it stays for focus- and key-driven
-- rechecks. A poll tick is nobody's keystroke, and on a Mac every Kontakt poll recheck cost
-- 21-41 ms — so at 15 all of them were logged, 1869 lines in one session, and the slow ones
-- that matter (up to a second against a hung application) were lost among them.
local RECHECK_SLOW_POLL_MS = 50
-- Set only while the poll loop runs its overlays' rechecks; see joinPoll.
local inPoll = false

function Overlay:_recheck()
  -- Nothing to learn: something at least as specific already holds the slot (see
  -- _outranked). The arbiter keeps whatever this overlay last reported, which is at most one
  -- poll interval old and is corrected the moment the winner lets go — the same contract the
  -- landmark gate has always had when it returns its last known result.
  if self:_outranked() then return end
  local t0 = host.now and host.now() or nil
  local matched = false
  for _, ctx in ipairs(self.contexts) do
    if ctx.match() then
      self.activeCtx = ctx
      matched = true
      break
    end
  end
  local tCtx = t0 and host.now() or nil
  if not matched then self.activeCtx = false end
  -- An overlay whose own menu is a window in front holds its place although its context does not
  -- match (see the Menus section, "holding its place"). A context that matches ends the hold.
  local held = false
  if self._menu then
    if matched then
      self:_menuHoldEnds()
    elseif self.active then
      local h = self:_menuHold()
      -- The hold ended with the keyboard still in the menu's own window, not shown, and the
      -- window of the press was brought back (see _menuRefocus): the ordinary path decides, on
      -- the world after that change.
      if h == "refocused" then return self:_recheck() end
      held = h == "menu"
      matched = held
    end
  end
  -- Stamped BEFORE the gate, because the two answer different questions: a context match
  -- means "this window is mine", the gate means "and what is loaded in it is mine too". The
  -- gap between them is the landmark search plus however many polling ticks it took, and
  -- that gap is what grows as overlays are added — see the report in _activate.
  if matched then
    if not self._ctxSince then
      self._ctxSince = host.now and host.now() or 0
      self._ctxChecks = 0
    end
    self._ctxChecks = (self._ctxChecks or 0) + 1
  else
    self._ctxSince = nil
  end
  -- An extra gate (e.g. a library landmark) further narrows the match, evaluated
  -- against the matched context's coordinate origin. Not while the overlay's own menu is in
  -- front: what the plug-in has loaded is not what that window shows.
  if matched and self._gate and not held then
    matched = self._gate(self:origin()) == true
    if not matched then self.activeCtx = false end
  end
  if t0 then
    local total = host.now() - t0
    if total >= (inPoll and RECHECK_SLOW_POLL_MS or RECHECK_SLOW_MS) then
      host.log.info(string.format(
        "[recheck] '%s' %d ms (context %d, gate %d) over %d context(s)",
        tostring(self.label), total, tCtx - t0, host.now() - tCtx, #self.contexts
      ))
    end
  end
  -- While a text field holds the keyboard nothing moves the focus, so this is the only thing
  -- that ever asks again whether it still does.
  if self.active and self._typingWhen then self:_syncNativeKeys() end
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
    -- A region or a name region is corners or a function answering them; anything else fails
    -- the first time it is read, inside an announcement, naming nothing.
    for _, field in ipairs({ "region", "ocrLabel" }) do
      local r = c[field]
      if r ~= nil and type(r) ~= "function"
        and not (type(r) == "table" and type(r[1]) == "number" and type(r[2]) == "number"
          and type(r[3]) == "number" and type(r[4]) == "number") then
        bad("%s: `%s` must be {x1, y1, x2, y2} or a function(overlay) answering that", who, field)
      end
    end
    if c.button ~= nil and c.button ~= "left" and c.button ~= "right" and c.button ~= "middle" then
      bad("%s: `button` is \"%s\" — host.input.click reads anything but \"right\" and \"middle\" "
        .. "as the left button", who, tostring(c.button))
    end
    if c.menuItem ~= nil then
      local k = type(c.menuItem)
      if k ~= "function" and not (k == "table" and type(c.menuItem[1]) == "number"
        and type(c.menuItem[2]) == "number") then
        bad("%s: `menuItem` must be {dx, dy} or a function(overlay, menu) answering that", who)
      end
      if c.at == nil then
        bad("%s: `menuItem` needs `at`, the point that opens the menu — a `points` sequence "
          .. "cannot open one", who)
      end
      if not self._menu then
        bad("%s chooses a menu item, but the overlay lists no menu tests (`menus`) — nothing can "
          .. "see the menu, so pressing it opens nothing and says it is not available", who)
      end
    end
    if c.kind == "hotspottoggle" then
      if c.at == nil then bad("%s has no `at`", who) end
      if c.onColor == nil or c.offColor == nil then
        bad("%s needs both `onColor` and `offColor` to tell its state apart", who)
      else
        -- A malformed colour reads as nil channels and every distance becomes an error or a
        -- nonsense number — checked here because the control would otherwise just stop
        -- reporting a state, which now looks exactly like a correctly-refused reading.
        for _, field in ipairs({ "onColor", "offColor" }) do
          for _, col in ipairs(asColors(c[field])) do
            if type(col) ~= "table" or #col ~= 3
              or type(col[1]) ~= "number" or type(col[2]) ~= "number" or type(col[3]) ~= "number"
            then
              bad("%s: `%s` must be {r, g, b} (or a list of them)", who, field)
            end
          end
        end
      end
    end
    if c.kind == "gbutton" then
      -- `region` is optional here: without one the search covers the whole origin.
      if c.image == nil then bad("%s has no `image` to look for", who) end
      for _, path in ipairs(asList(c.image)) do
        if not isAbsolutePath(path) then
          bad("%s: `image` is not an absolute path — use host.path()", who)
        end
      end
      -- `dragBy` is read on a worker callback, long after this control was declared, so a
      -- wrong type there surfaces as a Lua error inside an imageSearchAsync continuation with
      -- nothing in it naming the control. Caught here instead, where the message can.
      if c.dragBy ~= nil then
        local k = type(c.dragBy)
        if k ~= "function" and not (k == "table" and #c.dragBy == 2) then
          bad("%s: `dragBy` must be {dx, dy} or a function(overlay, hit)", who)
        end
      end
    end
    if c.kind == "gtoggle" and c.onImage == nil and c.offImage == nil then
      bad("%s has neither `onImage` nor `offImage`", who)
    end
    -- A half-declared reveal is the worst of both: it neither opens the panel nor says so,
    -- and the control then reports a state read off a panel that is not showing it.
    if c.reveal ~= nil then
      if c.kind ~= "gtoggle" then
        bad("%s: `reveal` is only honoured by a graphical toggle", who)
      elseif type(c.reveal) ~= "table" or c.reveal.at == nil or c.reveal.closedColor == nil then
        bad("%s: `reveal` needs `at` (the point that both reports and opens the panel) and "
          .. "`closedColor` (what that point looks like while it is shut)", who)
      end
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
    -- A hint is spoken by exactly two control kinds, and only for a state they can be in.
    -- Both mistakes are silent otherwise: on any other control it is simply never read, and
    -- a mistyped key ("enabled" for "on") is a sentence the user never hears and no error
    -- ever mentions.
    if c.hint ~= nil then
      if type(c.hint) ~= "table" then
        bad("%s: `hint` must be a table keyed by state, e.g. { on = \"…\" }, got %s",
          who, type(c.hint))
      elseif c.kind ~= "hotspottoggle" and c.kind ~= "gtoggle" then
        bad("%s: only a toggle has the on/off state a `hint` is keyed by, so this one is "
          .. "never spoken", who)
      else
        for k in pairs(c.hint) do
          if k ~= "on" and k ~= "off" then
            bad("%s: `hint` key '%s' is not a state this control reports — use `on` / `off`",
              who, tostring(k))
          end
        end
      end
    end
    if c.when ~= nil and type(c.when) ~= "function" then
      bad("%s: `when` must be a function, got %s", who, type(c.when))
    end
    if c.hotkey ~= nil and type(c.hotkey) ~= "string" then
      bad("%s: `hotkey` must be a string like \"Alt+P\", got %s", who, type(c.hotkey))
    end
    -- A control that says it opens a menu, on an overlay with nothing to see a menu with. The
    -- declaration alone gives no key to the menu — only a test does — so while that menu is up
    -- the overlay keeps Tab, Return and its hotkeys, and nothing else would ever say why.
    if c.opensMenu and not self._menu and c.menuItem == nil then
      bad("%s opens a menu, but the overlay lists no menu tests (`menus`) — nothing can see "
        .. "that menu, so the overlay keeps its keys while it is up", who)
    end
  end

  -- A hotkey spec claimed by TWO controls, where at least one of them is always visible.
  --
  -- Sharing a spec is legitimate and the runtime is built for it — a Kontakt nested in
  -- Komplete Kontrol offers KK's menu on Alt+M, a bare one offers Kontakt's snapshot menu on
  -- the same key — but only under the condition _syncControlHotkeys states in prose: the
  -- claimants must never be visible at once, because a press goes to the FIRST visible one
  -- and declaration order decides the rest. A claimant with no `when` is visible always, so
  -- it necessarily overlaps every other claimant, and the invariant is broken.
  --
  -- This is the check that would have caught Impact Soundworks' Preset taking Alt+P, which
  -- the inherited Kontakt header already owned for "Previous snapshot": the header won, and
  -- the key stepped a snapshot — changing the loaded sound — instead of reading a preset. It
  -- took a fourteen-agent audit to find, and a blind user could not have told the difference
  -- from hearing it, because both controls announce themselves correctly. Two `when`-gated
  -- claimants are NOT reported: whether two predicates are mutually exclusive cannot be
  -- decided here, and guessing would report every legitimate case.
  --
  -- Keyed by the key each spec stands for on this platform (keyOf), so "Cmd+S" and an inherited
  -- "Ctrl+S" are caught as the one key they are.
  local claim, order = {}, {}
  for i, c in ipairs(self.controls) do
    if type(c.hotkey) == "string" then
      local key = keyOf(c.hotkey)
      if claim[key] then
        table.insert(claim[key], i)
      else
        claim[key] = { i }
        order[#order + 1] = key
      end
    end
  end
  -- A spec the host will refuse to register, said once here with the control's name, because
  -- at activation it is only left out of the holds line. The informational reasons (a character
  -- the layout types with the key, VoiceOver's layer on a Mac) are not findings: the first costs
  -- a character and still works, and the second is a setting the module may never meet.
  local REFUSAL_WORDS = {
    parse = "is not a key spec",
    reserved = "is kept by the system",
    ["no-keycode"] = "has no key code on this platform",
    tap = "is a modifier tap, which only host.keys.capture can watch",
  }
  for _, spec in ipairs(order) do
    local why = hotkeyRefusal(spec)
    if why then
      local c = self.controls[claim[spec][1]]
      bad("control '%s': hotkey %s %s — it will not be registered", tostring(c.label or ""), spec,
        REFUSAL_WORDS[why] or why)
    end
  end
  for _, spec in ipairs(order) do
    local idxs = claim[spec]
    if #idxs > 1 then
      local names, ungated = {}, false
      for _, i in ipairs(idxs) do
        local c = self.controls[i]
        names[#names + 1] = string.format("%d '%s'", i, tostring(c.label or ""))
        if c.when == nil then ungated = true end
      end
      if ungated then
        bad("hotkey %s is claimed by controls %s, and at least one of them is visible at "
          .. "all times — a press activates whichever comes FIRST, so the later one may "
          .. "never fire. Gate them on mutually exclusive `when`s, or give one another key",
          spec, table.concat(names, " and "))
      end
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

-- ONE poll timer per interval per VM, not one per overlay.
--
-- Each library overlay used to start its own `host.timer.every(500, …)`, and since they
-- start at different moments their ticks are spread across the interval. That is invisible
-- until you look at what a tick COSTS: every landmark search then arrives alone, so the
-- host's fan-out batching — one screen capture serving all the searches of a tick — never
-- has anything to fan out over, and neither does the thread pool. Measured with twelve
-- libraries: twelve batches of one, each paying its own ~25 ms capture for a ~5 ms match.
-- Twelve captures where the whole design says one.
--
-- Sharing the timer makes the ticks coincide, which is what makes the batching real. It
-- also matches how the menu tests are run here: one timer per VM, not one per
-- overlay that has to remember to ask for it.
local pollGroups = {}

-- Whether the poll rechecks overlay `o` on this tick: always, unless its binding gave `pollWhen`,
-- which is then asked — for a match that can change without a window event only while the module
-- expects it to (VPS Avenger's warning box, looked for only after a Save or Initialize of its
-- own). Every other tick costs one call of it, not a context match: on a Mac in a DAW a match
-- reads the focus chain, and Logic's accessibility is the slow one. Guarded: one that raises is
-- said once and rechecked, as it would be without it.
local function pollWanted(o)
  local fn = o._pollWhen
  if not fn then return true end
  local ok, v = pcall(fn, o)
  if ok then return v == true end
  if not o._pollWhenSaid then
    o._pollWhenSaid = true
    host.log.info(string.format("[poll] '%s': pollWhen failed: %s — rechecked on every poll",
      tostring(o.label), tostring(v)))
  end
  return true
end

local function joinPoll(self, ms)
  local group = pollGroups[ms]
  if not group then
    group = {}
    pollGroups[ms] = group
    -- Disable-safe recurring poll, like the menu tests' tick: it survives a disable/enable cycle
    -- where a self-rescheduling host.timer.after would not.
    host.timer.every(ms, function()
      -- Timed, because this tick runs on the MAIN thread and everything else waits behind
      -- it: speech, hotkeys, the arbiter. Each overlay's context check re-resolves the
      -- plugin control, and Kontakt's cells cannot cache their identity (it is relational),
      -- so this is a UIA subtree search per overlay per tick. With one library module
      -- declaring eleven products across six cells that is sixty-six of them, and if the
      -- tick spreads over more than a few ms the landmark searches it starts no longer
      -- arrive together — which is exactly what "batch of 1" was telling us.
      local t0 = host.now and host.now() or 0
      -- Not guarded: a recheck that throws leaves this set until the next tick ends, which only
      -- moves a log threshold for half a second, where a pcall would cost the error its
      -- traceback.
      inPoll = true
      for _, o in ipairs(group) do
        if pollWanted(o) then o:_recheck() end
      end
      inPoll = false
      local dt = (host.now and host.now() or 0) - t0
      -- 50 ms, not 25: on a Mac every Kontakt tick costs 25-41 ms, so at 25 this was a line
      -- per tick (1178 in one session) and said nothing the slow ones did not.
      if dt >= 50 then
        host.log.info(string.format(
          "[poll] %d ms rechecking %d overlay(s) on the %d ms poll", dt, #group, ms
        ))
      end
    end)
  end
  group[#group + 1] = self
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
-- The overlay's first evaluation after a binding, on the module's next turn rather than inside the
-- call: a binding is made at a module's top level, where nothing can wait, and the evaluation can
-- ask a stored `identify` that reads — in the handler of the timer it may wait (see "Where a hook
-- runs"). `after(0)` is "run this as an event of my module", on the next tick (about 15 ms); no
-- timer decides anything here.
local function recheckOnNextTurn(self)
  host.timer.after(0, function()
    if self._bound then self:_recheck() end
  end)
end

function Overlay:_join(slot, specificity, pollMs)
  report(self, { slot = slot, specificity = specificity })
  -- From here on the overlay is LIVE: its first _recheck is asked for below, so anything that
  -- changes what the overlay is must already be in place. See the guard below.
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
        joinPoll(self, pollMs)
      end
      -- Log the slot's roster as it grows. A slot is a plain string typed independently
      -- in modules that never see each other, and a typo does not error — it quietly
      -- creates a private slot where this overlay wins every time, or never competes.
      -- A slot that stays at one participant while its sibling shows several is the only
      -- visible symptom, so make it visible.
      --
      -- One line per join, naming the newcomer and the count it makes — not the roster. The
      -- roster used to be printed whole on every join, so a slot that ends at N participants
      -- was listed N times with N(N+1)/2 names in all; in the tester's last session that was
      -- 286 of 1985 lines, and lines with 36 names in them. Nor is a repeated name in it a
      -- fault to be read out: a module id appearing many times in one slot is the design.
      -- Every overlay is its own claim, Kontakt declares one per cell of
      -- version-by-environment, and a library module one per cell PER PRODUCT — ten
      -- products are forty claims all owned by com.platform.audio-imperia (see
      -- kontakt/main.luau, `library`). A claim is registered once per overlay (`self._slot`
      -- guards the re-join above), so a repeat is never the same overlay twice.
      -- There is no moment to print the settled roster once instead: modules load one VM
      -- at a time and nothing tells this one that the last has joined, so the sequence of
      -- joins IS the roster. The newcomer is the last entry — the host appends claims in
      -- registration order, and the register call above has just returned.
      if host.arbiter.participants then
        local roster = host.arbiter.participants(self._slot)
        local me = roster[#roster]
        host.log.info(string.format(
          "arbiter slot '%s': +%s(%d) -> %d participant(s)",
          self._slot, me and me.module or "?", self._specificity, #roster
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
  -- A second binding of an overlay that is already live (sforzando binds its standalone window,
  -- then its embedded context) is evaluated now, as the first was: its triggers are the ones
  -- already registered, and without this its context was first asked at the next window or
  -- focus event. On a Mac there may be none — moving into a plug-in view that publishes nothing
  -- raises no notification — so a module loaded with its plug-in already in front stayed silent
  -- until Cmd+Shift+F6. One recheck per extra binding, on the first tick after it.
  if self.triggerSet then
    recheckOnNextTurn(self)
    return
  end
  self.triggerSet = true
  -- Foreground changes AND focus changes (the latter catches focus moving into
  -- an embedded plugin without a foreground change).
  host.window.onTrigger({}, { on = "activate" }, function() self:_recheck() end)
  host.window.onFocus(function() self:_recheck() end)
  recheckOnNextTurn(self)
end

-- Sets the interaction flags from `opts`, then joins the arbiter + arms the trigger.
local function ensureTrigger(self, opts)
  if opts then
    self.hoverToRead = opts.hoverToRead == true
    -- `menus = { test, … }`: how this overlay's plug-in menus are seen, so the keys go to a
    -- menu while one is open (see the Menus section). `false` or absent: no tests. `true` is
    -- refused there, naming the building blocks.
    if opts.menus ~= nil and opts.menus ~= false then self:_watchMenus(opts.menus) end
    -- `pollWhen = function(overlay) -> boolean`: the `pollMatch` poll rechecks this overlay only
    -- while it answers true (see joinPoll). Raises here for anything but a function.
    if opts.pollWhen ~= nil then
      if type(opts.pollWhen) ~= "function" then
        error(("overlay '%s': `pollWhen` must be a function(overlay) -> boolean, got %s")
          :format(tostring(self.label), type(opts.pollWhen)), 0)
      end
      self._pollWhen = opts.pollWhen
    end
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
--
-- It keeps the MEMO_CAP windows asked about most recently and forgets the rest: a window that
-- closed is never asked about again, and the application runs for days, one entry per plug-in
-- window ever opened (and per size, with `opts.key`). A window forgotten and asked about again
-- is simply worked out again.
local MEMO_CAP = 64

-- How many times in a row an embedded binding's `identify` may answer nil ("could not tell
-- yet") for one control before that is taken as no — see attachEmbedded.
local IDENTIFY_UNDECIDED_MAX = 8

-- A table of at most `cap` entries that forgets the one used least recently when a new one
-- comes. `nil` is never stored — every caller treats it as "not known". Forgetting costs one
-- pass over the `cap` entries, only when a new key arrives at a full table.
local function boundedCache(cap)
  local values, used, n, clock = {}, {}, 0, 0
  local c = {}
  function c.get(k)
    local v = values[k]
    if v ~= nil then
      clock += 1
      used[k] = clock
    end
    return v
  end
  function c.set(k, v)
    if values[k] == nil then
      if n >= cap then
        local oldest, at = nil, math.huge
        for key, u in pairs(used) do
          if u < at then oldest, at = key, u end
        end
        if oldest ~= nil then
          values[oldest], used[oldest] = nil, nil
          n -= 1
        end
      end
      n += 1
    end
    clock += 1
    values[k], used[k] = v, clock
  end
  return c
end

function O.memoByOrigin(fn, opts)
  local cache = boundedCache(MEMO_CAP)
  local keyOf = opts and opts.key
  return function(origin, ...)
    if not (origin and origin.id) then return fn(origin, ...) end
    local k = origin.id
    if keyOf then k = tostring(k) .. "|" .. tostring(keyOf(origin)) end
    local hit = cache.get(k)
    if hit ~= nil then return hit end
    local v = fn(origin, ...)
    if v ~= nil then cache.set(k, v) end
    return v
  end
end

-- Memoizes `fn(…) -> value` for the CURRENT EPOCH only.
--
-- The sibling above caches per window, for a property fixed for that window's lifetime.
-- This is for the other kind: a question whose answer holds only until the world moves,
-- typically because it is RELATIONAL — "is this Kontakt wrapped by a Komplete Kontrol?"
-- depends on what else is in the window, so a per-window cache would pin the answer from
-- before the Komplete Kontrol loaded. Those questions are exactly the ones declared
-- `cacheIdentity = false`, and exactly the ones that then get asked over and over.
--
-- An epoch means "the world may have changed", so within one, asking twice is duplication
-- by definition. Same rule as the other memo: `nil` means "could not tell yet" and is not
-- cached, `false` is a real answer and is.
--
-- Only the CURRENT epoch is held — one slot, not a growing table — so nothing accumulates
-- and a stale answer cannot be read after the world has moved.
--
-- The answer is kept under the epoch it was given in, read after `fn` returns. An `fn` that
-- waits — a read without a callback — lets other events turn the epoch over meanwhile, and
-- kept under the epoch it was asked in, its answer would be worked out again at the next call.
function O.memoByEpoch(fn)
  local epoch, value = nil, nil
  return function(...)
    if value ~= nil and epoch == (host.epoch and host.epoch() or 0) then return value end
    local v = fn(...)
    if v ~= nil then
      epoch, value = host.epoch and host.epoch() or 0, v
    end
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
      -- A list has an array part and a single entry does not. This asked for `title`, `app` or
      -- `windows` until daw-hosts had an entry with none of them — Logic's, a `macos` block
      -- and its plug-in origin — which it would have read as an empty list and dropped. An
      -- empty table is neither, and adds nothing: taken as an entry, it would match every window.
      if list[1] ~= nil then
        for _, h in ipairs(list) do out[#out + 1] = h end
      elseif next(list) ~= nil then
        out[#out + 1] = list -- a single entry, not a list of them
      end
    end
  end
  return out
end

-- One resolved origin per (spec, epoch), shared by every overlay bound to that spec.
-- See the note in attachEmbedded for why this is not per overlay. Weak keys: an entry
-- lives exactly as long as the spec it answers for.
local originMemo = setmetatable({}, { __mode = "k" })
-- A panel's identity verdict per spec, for the same reason and with the same weak keys: see
-- panelIdentity in attachEmbedded.
local panelVerdicts = setmetatable({}, { __mode = "k" })

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

-- ── The DAW's plug-in panel ──────────────────────────────────────────────────
-- On a Mac a plug-in inside a DAW publishes no control of its own, nothing an embedded binding
-- could match. Measured more than once: the third Mac session found Kontakt 7's 34 elements
-- straight in REAPER's FX window, with nothing that IS the plug-in — no class, no group, no
-- identifier; sforzando in REAPER publishes nothing at all; and a VPS Avenger, in REAPER and in
-- Logic on 2026-09-27, the same. So the one answer there to "where is the plug-in?" is the
-- DAW's: where it draws a plug-in inside its plug-in window. That is the `pluginOrigin` a
-- daw-hosts entry carries —
-- `(window) -> { dx, dy }` from the window's content origin, the panel reaching the window's
-- right and bottom edges — and from it this runtime builds the plug-in's control, for every
-- module alike:
--
--   { id = the window's id, app = its application, class = "host-panel",
--     bounds = client = the panel, stay = see below }
--
-- The window's id, so an element query still asks the right window; the panel as both
-- rectangles, since it has no frame of its own, as a child control on Windows has none. From
-- there every authored coordinate, `rawOrigin`, `fromRight` and O.contentSize behave as they do
-- against a real control, which is what lets a module write its control set once and have it
-- work in every DAW that has an entry.
--
-- Which is the point: a DAW is added by adding its entry, and no plug-in module names a DAW
-- (the maintainer's decision of 2026-09-27). Before this each module did the DAW's part itself
-- on a Mac — sforzando attached to REAPER's bundle with a frame from REAPER's geometry, Kontakt
-- asked for "Kontakt 7" in REAPER's window title — and neither could come up in Logic, whose
-- plug-in window is titled by the channel strip ("Inst 1").
--
-- The panel says WHERE a plug-in is and nothing about WHICH, so a binding takes it only when it
-- has an `identify` to ask. A module whose window class was its whole identity (u-he's) would
-- otherwise come up over every plug-in in every DAW.
--
-- The origin is memoised per window and client size, because an origin function may walk the
-- window's accessibility tree (REAPER's reads its FX list, Logic's its header) and would
-- otherwise run at every evaluation, in every VM. The size is in the key because a DAW lays a
-- window out again when it is resized; the trade is the focus key's own, which keys its memo
-- the same way — a divider dragged inside a window that keeps its size is followed only once
-- the window is resized, or closed and opened again as a new window (a new id; whether REAPER
-- on a Mac reuses the window it closed is not measured).
local HOST_PANEL = "host-panel"
local hostOrigins = boundedCache(MEMO_CAP)
-- Windows and sizes whose origin function failed, said once each.
local hostOriginSaid = boundedCache(MEMO_CAP)

-- Entry `entry`'s origin function on this platform, or nil. `pluginOrigin` is a function or an
-- OS-keyed table of them: REAPER's was measured on a Mac, and on Windows every module finds a
-- control of its own, so daw-hosts gives it for macOS alone.
local function originFnOf(entry)
  local fn = entry and host.os.pick(entry.pluginOrigin)
  return type(fn) == "function" and fn or nil
end

-- The DAW's plug-in panel in window `win`, as a control, or nil when `entry` gives no origin on
-- this platform or the origin leaves the panel no width or height.
local function hostPanelOf(entry, win)
  local originOf = originFnOf(entry)
  if not (originOf and win and win.id and win.client) then return nil end
  local cw, ch = win.client.w or 0, win.client.h or 0
  local key = tostring(win.id) .. "|" .. cw .. "x" .. ch
  local p = hostOrigins.get(key)
  if p == nil then
    local ok, v = pcall(originOf, win)
    if not (ok and type(v) == "table" and type(v[1]) == "number" and type(v[2]) == "number") then
      -- Not kept, so a failure that was momentary is asked again at the next evaluation; said
      -- once per window and size, so one that is not does not fill the log.
      if not hostOriginSaid.get(key) then
        hostOriginSaid.set(key, true)
        host.log.info(string.format(
          "[overlay] the DAW entry's pluginOrigin for '%s' (id=%s, %dx%d) %s — no plug-in panel there",
          tostring(win.title), tostring(win.id), cw, ch,
          ok and ("answered a " .. type(v) .. ", not { dx, dy }") or ("raised: " .. tostring(v))
        ))
      end
      return nil
    end
    p = { v[1], v[2] }
    hostOrigins.set(key, p)
  end
  local x, y = win.client.x + p[1], win.client.y + p[2]
  local w, h = win.client.x + cw - x, win.client.y + ch - y
  if w <= 0 or h <= 0 then return nil end
  return {
    id = win.id,
    app = win.app,
    class = HOST_PANEL,
    bounds = { x = x, y = y, w = w, h = h },
    client = { x = x, y = y, w = w, h = h },
  }
end

-- Where the keyboard is, against panel `b` of window `active`: "inside", "outside" or
-- "unknown". This is the Win32 chrome gate's question asked geometrically, because the chrome
-- classes are Win32 vocabulary and mean nothing on a Mac. The window itself as the focused
-- element — all a view that publishes nothing leaves on the chain — is inside, which is the rule
-- sforzando's own macOS binding settled on before this runtime asked it for everyone; a focused
-- element whose centre lies outside the panel is the DAW's own chrome (REAPER's FX list, its
-- preset popup, Logic's header); and an EMPTY chain is the backend saying it could not read
-- where the keyboard is. Unknown, not inside: the reading daw-hosts' focus key makes too, and an
-- overlay that took it as "inside" would start capturing Tab and Return on a guess while the
-- keyboard may well be in the FX list.
--
-- A chain that does not END in `active` is unknown as well, which is daw-hosts' reading again.
-- The two are separate questions to the system — the window in front is the frontmost
-- application's, the focused element is the system-wide one — so a chain read in between
-- describes some other window, and an element of it whose centre happened to lie over the
-- panel's rectangle on screen would have started a stay and had `identify` asked on its
-- strength. The backend ends every chain in a window, the frontmost one when the walk reaches
-- none (a menu), so a chain about `active` always ends in it. And a focused element other than
-- the window that carries no rectangle says nothing about where it is: unknown too. The host
-- gives every element one; a control a module made by hand may not.
local function chainKnown(active, chain)
  local last = chain and chain[#chain]
  return chain ~= nil and chain[1] ~= nil and last ~= nil and last.id == active.id
end

local function keyboardIn(active, chain, b)
  if not chainKnown(active, chain) then return "unknown" end
  local focused = chain[1]
  if focused.id == active.id then return "inside" end
  if not (focused.bounds and b) then return "unknown" end
  local fx = focused.bounds.x + focused.bounds.w / 2
  local fy = focused.bounds.y + focused.bounds.h / 2
  if fx < b.x or fx > b.x + b.w or fy < b.y or fy > b.y + b.h then return "outside" end
  return "inside"
end

-- ── Which plug-in the panel shows: the stay ──────────────────────────────────
-- A child control on Windows IS one plug-in for as long as it exists, which is why its
-- `identify` verdict is kept per control. A panel is not: one REAPER FX chain window shows
-- whichever FX is selected in its list, in the same window — at the same rectangle whenever the
-- two plug-ins are the same size, since REAPER sizes the window to the plug-in — and a Logic
-- plug-in window can most likely be switched to another insert from its header (not measured
-- yet: TODO.md, "The stay, live"). A verdict kept per window would go on calling the second
-- plug-in by the first one's name.
--
-- The rule's premise is that what changes which plug-in a panel shows is the DAW's own chrome —
-- the FX list, the header's plug-in menu — and that the keyboard has to be there to use it;
-- whether a visit there is always observed on a Mac is a Mac session's question (below). So a
-- panel's verdict is kept for a STAY: the keyboard inside one window's panel, from the
-- evaluation that first finds it there to the first observation that it is anywhere else — on
-- the DAW's chrome, in another window, nowhere anybody can read (an empty chain, or one that is
-- another window's) — or that the window's title or size changed (a REAPER window retitled for
-- another track). The next time the keyboard is inside, `identify` is asked again, once, and its
-- answer kept for that stay. Changing plug-in costs one identify; staying in one costs none.
--
-- What the rule cannot see, said so nobody reads the stay as more than it is:
--   * A switch made with the keyboard still inside the plug-in — no DAW measured so far has a
--     key for that — keeps the old verdict until the keyboard next leaves.
--   * A visit to the chrome that no evaluation SAMPLES. On a Mac, AXFocusedUIElementChanged only
--     marks the focus dirty, and the round that follows reads the chain as it is by then
--     (backend/macos/queue.rs), while moving into a view that publishes nothing raises no
--     notification at all. So Shift+Tab to the FX list, Down, Tab back, all done before a busy
--     event loop drains its queue, is one round that sees "inside" — and the old verdict goes on,
--     over the newly selected plug-in. Telling that apart needs the host to count the
--     notifications it merged into a round, a general primitive nobody has measured the need for
--     yet: TODO.md, "The stay, live", asks a Mac session for a quick switch first.
--   * A definite "no" is kept for the stay like a "yes": an `identify` that has not been able to
--     LOOK yet (a view not painted, a read that failed) answers nil, which is asked again at the
--     next evaluation up to IDENTIFY_UNDECIDED_MAX times, and not false. Cmd+Shift+F6 with the
--     keyboard already inside is one more evaluation (daw-hosts asks every overlay to look
--     again); it does not end the stay, so it undoes a nil, never a false.
--
-- Observed by every evaluation that asks where the keyboard is, AND by a watcher of its own on
-- every focus and activation event, per VM. The watcher is what makes the rule hold across the
-- arbiter: an overlay outranked on its slot skips its rechecks (_outranked), so the VM of a
-- Kontakt header outranked by a library overlay in another module's VM would never see the visit
-- to the FX list, and would bring its old verdict back the moment the library let go — over
-- whatever plug-in is selected by then.
--
-- The stay's number is on the panel (`stay`), for a module whose own evidence is costly and
-- whose `identify` is relational and so asked at every evaluation (Kontakt's cells): it can keep
-- its evidence per stay the way this runtime keeps a verdict.
local stay = { n = 0, win = nil }

local function endStay()
  if stay.win ~= nil then
    stay.n += 1
    stay.win = nil
  end
end

local function inStayWindow(a)
  return a ~= nil and stay.win ~= nil and a.id == stay.win and a.title == stay.title
    and a.client ~= nil and a.client.w == stay.cw and a.client.h == stay.ch
end

-- The stay the keyboard is in, inside `panel` of `active`: the one in progress, or a new one.
local function stayIn(active, panel)
  if inStayWindow(active) then return stay.n end
  stay.n += 1
  stay.win, stay.title = active.id, active.title
  stay.cw, stay.ch = active.client.w, active.client.h
  stay.dx, stay.dy = panel.client.x - active.client.x, panel.client.y - active.client.y
  return stay.n
end

local function watchStay()
  if stay.win == nil then return end
  local a = host.window.active()
  if not inStayWindow(a) then return endStay() end
  local x, y = a.client.x + stay.dx, a.client.y + stay.dy
  local b = { x = x, y = y, w = a.client.x + a.client.w - x, h = a.client.y + a.client.h - y }
  if keyboardIn(a, host.window.focusChain(), b) ~= "inside" then endStay() end
end

-- Registered once per VM, by the first binding that can take a panel or has a function for the
-- platform. On Windows no binding can take a panel (no daw-hosts entry gives an origin there)
-- and no module ships a function for it, so it is not registered; one that did would register
-- it for nothing, since a stay only begins on a panel.
local stayWatched = false
local function watchStays()
  if stayWatched then return end
  stayWatched = true
  host.window.onTrigger({}, { on = "activate" }, watchStay)
  host.window.onFocus(watchStay)
end

-- Patterns already explained as Win32 classes on a Mac, so each is said once per VM however
-- many overlays share it.
local win32PatternsSaid = {}
-- `then_` is what the binding does when the pattern finds nothing, as the line says it.
local function warnWin32PatternOnMac(self, pattern, then_)
  if host.os.current ~= "macos" or type(pattern) ~= "string" then return end
  if string.find(pattern, "AX", 1, true) or string.find(pattern, "/", 1, true) then return end
  if win32PatternsSaid[pattern] then return end
  win32PatternsSaid[pattern] = true
  host.log.info(string.format(
    "[%s] embedded control pattern '%s' looks like a Win32 window class, and on macOS a control's "
      .. "class is AXRole/AXSubrole/AXIdentifier — it probably never matches here, and then %s. "
      .. "Written as `control = { windows = ... }` it is not tried here at all",
    tostring(self.label or "overlay"), pattern, then_
  ))
end

-- DAW entries that matched a window with a binding wanting a panel and gave no origin on this
-- platform, said once each per VM: a DAW entry added without one brings up no embedded overlay
-- in that DAW, and the bind-time line cannot say so — it is decided over ALL of a binding's
-- hosts, and `daw.all` has entries with origins.
local noOriginSaid = setmetatable({}, { __mode = "k" })

-- One line per VM saying that the panel is in use, naming the first binding that took it.
local panelModeSaid = false

-- Context-bound (embedded): active while the keyboard focus is *inside* a plugin
-- control — a window whose class matches `spec.control` (a Luau pattern) on the
-- focus-ancestor chain — within a DAW host window (any of `spec.hosts`). Keying
-- on focus-in-plugin (not just host-foreground) means we don't capture the
-- host's own navigation keys. Coordinates are relative to that control's client
-- area, so the same regions work standalone and embedded.
--
-- Where the plug-in has no control of its own — a Mac — the control is the DAW's plug-in panel
-- (above), and the same holds of it.
function Overlay:attachEmbedded(spec, opts)
  local hosts = spec.hosts or { spec.host }
  -- The one thing in an embedded binding that cannot be written once. A host names its
  -- plugin surface with a Win32 window class on Windows and an accessibility role on
  -- macOS, and no amount of abstraction makes "Qt5152QWindowIcon" and "AXGroup" the same
  -- string. So this accepts an OS-keyed table — `control = { windows = …, macos = … }` —
  -- and everything else about the binding stays shared.
  --
  -- Resolved in three steps, per platform (the maintainer's decision of 2026-09-27):
  --   (a) the module's own `control` for this platform, as it always was: a pattern over the
  --       window's child controls, or a function;
  --   (b) otherwise — no entry for this platform, or a pattern that finds nothing — the DAW's
  --       plug-in panel, when the entry of the window in front has a `pluginOrigin` and the
  --       binding has an `identify` to tell which plug-in the panel shows;
  --   (c) otherwise nothing: the binding is inert here, and says so once.
  -- So a module writes `control = { windows = "<its own class>" }` and works on a Mac without a
  -- macOS entry, in every DAW daw-hosts has an entry for; a module whose plug-in does publish a
  -- container of its own on a Mac can still name it. `false` for a platform is the module's
  -- word that the binding does not apply there at all — neither a control nor the panel
  -- (Kontakt's Content Missing, a dialog of its own and never a plug-in panel).
  --
  -- A pattern that finds nothing falls through to the panel, rather than only a missing entry,
  -- so that a plain-string pattern — Win32 vocabulary, almost always, which on a Mac is compared
  -- with the AXRole/AXSubrole/AXIdentifier triple and practically never matches — takes the
  -- panel there instead of leaving the binding inert. Where it DOES match (a Mac pattern that
  -- names the plug-in's container) it is used, exactly as before.
  --
  -- A platform's entry may be a FUNCTION: `(activeWindow, hostPanel) -> control | nil`, where
  -- `hostPanel` is the DAW's panel as above, or nil when the window's entry gives none. That
  -- was the macOS answer before the panel existed, and it stays for a plug-in that knows its
  -- own corner better than the DAW's origin does: Kontakt finds its FILE button at a fixed
  -- offset from its panel's corner and hands back the panel with that corner. What it returns
  -- is a control table the way `host.window.controls` would have made one — `id` (the window's,
  -- so element queries still work), `class`, `bounds` and `client` set to the plug-in's PANEL —
  -- and from there every authored coordinate behaves exactly as it does against a real control
  -- on Windows. `identify` still runs on what it returns, and the keyboard is tested against its
  -- `bounds`, so a control without them never has the keyboard inside.
  --
  -- So `own`, the binding's `control` for the platform we are on, is one of:
  --   a string     a pattern over the window's child controls and the focus chain — plain (the
  --                same on every platform) or one entry of an OS-keyed table
  --   a function   (activeWindow, hostPanel) -> control | nil
  --   false        the module's word that the binding does not apply on this platform
  --   nil          nothing of the module's own: the DAW's plug-in panel, where there is one
  local own = host.os.pick(spec.control)
  local identify = spec.identify
  -- Whether a panel is to be had here at all: some host of this binding has an origin on this
  -- platform. On Windows, where daw-hosts gives origins for macOS only, never.
  local anyPanel = false
  for _, h in ipairs(hosts) do
    if originFnOf(h) then anyPanel = true break end
  end
  -- Whether this binding takes the panel as its control, when it has nothing of its own.
  local panelFallback = anyPanel and identify ~= nil and own ~= false and type(own) ~= "function"
  if own == false or (own == nil and not panelFallback) then
    -- `self.label`, not `self.name`: O.new sets `label` and nothing in this runtime ever
    -- sets `name`, so every one of these lines said "[overlay]" and a log full of them could
    -- not say WHICH cell had declared itself inert. Kontakt alone writes several.
    local why
    if own == false then
      why = "the module declares no control for this platform"
    elseif anyPanel then
      why = "the module gives no control pattern for this platform, and the DAW's plug-in panel "
        .. "does not say which plug-in it shows — the binding has no identify to ask"
    else
      why = "neither the module (no control pattern for this platform) nor any of its hosts "
        .. "(no plug-in panel, `pluginOrigin`, on this platform) gives a control here"
    end
    host.log.info(string.format(
      "[%s] embedded binding is inactive on %s: %s",
      self.label or "overlay", host.os.current, why
    ))
    return self
  end
  -- A plain-string pattern is a Win32 window class almost always ("^Plugin%x+$",
  -- "Qt%d+.-QWindowIcon"), and on a Mac it is matched against the AXRole/AXSubrole/AXIdentifier
  -- triple a control's `class` is there — so it almost never matches. Said once per pattern,
  -- with what the binding does then. A macOS pattern usually names an AX role or has the
  -- triple's `/` in it; one that matches an AXIdentifier alone ("^com%.u%-he") is told too,
  -- which is why the line says "probably".
  warnWin32PatternOnMac(self, own,
    panelFallback and "the binding takes the DAW's plug-in panel wherever the DAW's entry gives one"
      or identify == nil and "the binding stays inert: it has no `identify` to ask of the DAW's plug-in panel"
      or "the binding stays inert: none of its hosts gives a plug-in panel (`pluginOrigin`) on this platform")
  local panels = panelFallback or type(own) == "function"
  if panels then watchStays() end
  if own == nil and not panelModeSaid then
    panelModeSaid = true
    host.log.info(string.format(
      "[overlay] on %s an embedded binding with no control of its own takes the DAW's plug-in "
        .. "panel (its daw-hosts entry's pluginOrigin) as the plug-in's control; '%s' is the first here",
      host.os.current, tostring(self.label or "overlay")
    ))
  end
  -- Optional identity check: the control class often matches *any* plugin in a
  -- host (e.g. "Plugin<hex>", which several vendors' plug-ins share — ReaHotkey matches
  -- sforzando, Engine 2 and Zampler by it), and a DAW's panel matches every plug-in, so
  -- `spec.identify(control)` confirms it is really this plugin (via UIA / OCR landmark /
  -- image search — the module's choice). Cached per control HWND, since such a check is costly.
  --
  -- identify may be RELATIONAL — its verdict depends on sibling controls, not just
  -- this one (e.g. "this Kontakt is wrapped by a Komplete Kontrol, so defer to KK").
  -- Such a spec sets cacheIdentity = false so the per-HWND cache below (which assumes
  -- identify is a pure function of the single control) can't pin a stale verdict when
  -- the neighbourhood changes — e.g. a Kontakt loaded into an already-open KK, whose
  -- container HWND is stable, would otherwise keep its first (host-less) verdict and
  -- the overlay would never attach.
  local cacheId = spec.cacheIdentity ~= false
  -- Per control id, the verdict of `identify` — true or false. `nil` ("could not tell yet":
  -- UIA not ready the instant a plug-in appears) is NOT a verdict and is asked again at the next
  -- recheck: stored as false, as it once was, it kept an overlay off a Kontakt it would have
  -- recognised a moment later, for as long as that window lived. Bounded like memoByOrigin.
  --
  -- But only IDENTIFY_UNDECIDED_MAX times in a row. A plain Lua `identify` says "no" by
  -- returning nothing (`if found then return true end`), and asked again at every recheck —
  -- every focus event and every pollMatch tick — its UIA search or OCR read (a UIA search of an
  -- Electron window measured 0.65-0.88 s) would run on the event loop for as long as the window
  -- is open. A count, not a clock: the rechecks a window gets, not how long it has been open.
  local idCache = boundedCache(MEMO_CAP)
  -- Per control id, how many "could not tell yet" in a row; the first is logged.
  local idUndecided = boundedCache(MEMO_CAP)

  -- WHERE `identify` RUNS (see "Where a hook runs"). Kept per control — a pattern's control, or a
  -- panel for a stay — it is asked once for a new control, in the handler, where it may wait for a
  -- read; that is the stored `identify`. Asked at every evaluation — relational (`cacheIdentity =
  -- false`), or a function's control where the DAW gives no panel — it must not read, and runs in
  -- a pass: inline in a scan's, in one of its own otherwise. Either is guarded: a raise is logged
  -- once per control id, and is no verdict — a stored one counts as "could not tell yet" without
  -- counting against IDENTIFY_UNDECIDED_MAX, an asked-every-time one as "no" this time.
  --
  -- A stored `identify` reached INSIDE a pass — another overlay's `when` asking this one's origin,
  -- say — is not asked there, where it could not wait: "not now", not counted either, and the
  -- origin is not kept for the epoch (origin, below), so the next question outside a pass asks
  -- again. This overlay looks again on its module's next turn, once per epoch, through a recheck
  -- of its own.
  local idRaised = boundedCache(MEMO_CAP)
  local function identifyRaised(c, err, taken)
    if idRaised.get(c.id) then return end
    idRaised.set(c.id, true)
    host.log.info(string.format("attachEmbedded: [%s] id=%s, identify raised: %s — taken as %s — '%s'",
      tostring(c.class), tostring(c.id), tostring(err), taken, tostring(self.label or "overlay")))
  end
  -- Set by a stored identify not asked inside a pass; read and cleared by origin().
  local deferred = false
  local deferredAt = nil
  local notNowSaid = boundedCache(MEMO_CAP)
  local function notNow(c)
    deferred = true
    if not notNowSaid.get(c.id) then
      notNowSaid.set(c.id, true)
      host.log.info(string.format("attachEmbedded: [%s] id=%s, identify is not asked inside a scan of the "
        .. "overlay's hooks — '%s' looks again on its next turn", tostring(c.class), tostring(c.id),
        tostring(self.label or "overlay")))
    end
    local e = host.epoch and host.epoch() or -1
    if deferredAt ~= e then
      deferredAt = e
      recheckOnNextTurn(self)
    end
  end
  -- An `identify` asked at every evaluation: true, false — or false when it raised.
  local function askedEveryTime(c)
    local okCall, verdict = pcall(noWait, identify, c)
    if not okCall then
      identifyRaised(c, verdict, "no this time")
      return false
    end
    return verdict == true
  end

  -- The module's own control among the window's children, by pattern, or nil. Unchanged from
  -- before the panel existed, and on Windows the whole story.
  local function ownByPattern(focusChain)
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
      if string.match(c.class, own) then
        if not identify then return c end
        local ok
        if cacheId then
          ok = idCache.get(c.id)
          if ok == nil and passDepth > 0 then
            -- Inside a pass: not asked there (above).
            notNow(c)
            ok = false
          elseif ok == nil then
            local okCall, verdict = pcall(identify, c)
            if not okCall then
              identifyRaised(c, verdict, "could not tell yet, and not counted")
              ok = false
            elseif verdict ~= nil then
              ok = verdict == true
              idCache.set(c.id, ok)
              idUndecided.set(c.id, 0)
              if host.log then
                host.log.info(string.format(
                  "attachEmbedded: matched control [%s] id=%d, identify=%s", c.class, c.id, tostring(ok)
                ))
              end
            else
              -- Not now; asked again at the next recheck — up to IDENTIFY_UNDECIDED_MAX in a row.
              ok = false
              local n = (idUndecided.get(c.id) or 0) + 1
              idUndecided.set(c.id, n)
              if n >= IDENTIFY_UNDECIDED_MAX then
                idCache.set(c.id, false)
                if host.log then
                  host.log.info(string.format(
                    "attachEmbedded: matched control [%s] id=%d, identify could not tell %d times in a row, "
                      .. "so it is taken as no, as false would be, for as long as this control is kept "
                      .. "(an identify that means no should return false)",
                    c.class, c.id, n
                  ))
                end
              elseif n == 1 and host.log then
                host.log.info(string.format(
                  "attachEmbedded: matched control [%s] id=%d, identify could not tell yet — asked again at the next recheck, up to %d times in a row",
                  c.class, c.id, IDENTIFY_UNDECIDED_MAX
                ))
              end
            end
          end
        else
          ok = askedEveryTime(c) -- relational spec: re-evaluate every recheck
        end
        if ok then return c end
      end
    end
    return nil
  end

  -- The panel's verdict for the stay in progress (see "the stay", above): one slot per SPEC —
  -- one window is in front at a time — kept beside the spec's resolved control (panelVerdicts,
  -- next to originMemo) rather than per overlay, because every overlay bound to one `O.embedded`
  -- value asks the same `identify` the same question, and whichever resolves the control first
  -- in an epoch asks it for all of them. Per overlay, an `identify` that reads the screen would
  -- run once per overlay per stay. `rect` is the control's place in its window, so a control a
  -- module moved (Kontakt's corner) is asked about again when it moves. The nil rule is a child
  -- control's, counted per stay: IDENTIFY_UNDECIDED_MAX "could not tell yet" in a row, and the
  -- rest of the stay takes it as no — the next stay asks again, which is what a Kontakt that
  -- refuses every accessibility question for its first minute needs.
  --
  -- The lines name the overlay that asked (`self.label`): `c.class` is "host-panel" for every
  -- module, and host.log adds no module name, so without it a log with two modules binding
  -- panels could not say whose `identify` answered.
  local pv = panelVerdicts[spec]
  if not pv then
    pv = { held = nil, undecided = { stay = -1, n = 0 }, said = nil }
    panelVerdicts[spec] = pv
  end
  local function panelIdentity(c, active)
    local rect = string.format("%d,%d %dx%d", c.client.x - active.client.x,
      c.client.y - active.client.y, c.client.w, c.client.h)
    local s = c.stay
    local held = pv.held
    if held and held.stay == s and held.rect == rect then return held.value end
    -- Inside a pass: not asked there; and one that raised is no verdict, not counted (above).
    if passDepth > 0 then
      notNow(c)
      return false
    end
    local label = tostring(self.label or "overlay")
    local okCall, verdict = pcall(identify, c)
    if not okCall then
      identifyRaised(c, verdict, "could not tell yet, and not counted")
      return false
    end
    if verdict == nil then
      if pv.undecided.stay ~= s then pv.undecided = { stay = s, n = 0 } end
      local n = pv.undecided.n + 1
      pv.undecided.n = n
      if n >= IDENTIFY_UNDECIDED_MAX then
        pv.held = { stay = s, rect = rect, value = false }
        host.log.info(string.format(
          "attachEmbedded: [%s] panel of '%s' id=%s, identify could not tell %d times in a row, so "
            .. "it is taken as no until the keyboard next leaves the panel — '%s'",
          c.class, tostring(active.title), tostring(active.id), n, label
        ))
      elseif n == 1 then
        host.log.info(string.format(
          "attachEmbedded: [%s] panel of '%s' id=%s, identify could not tell yet — asked again at "
            .. "the next recheck, up to %d times in a row — '%s'",
          c.class, tostring(active.title), tostring(active.id), IDENTIFY_UNDECIDED_MAX, label
        ))
      end
      return false
    end
    pv.held = { stay = s, rect = rect, value = verdict == true }
    -- The line a Mac session reads to see where the panel came out and whether the plug-in was
    -- recognised in it. Said when the verdict for this window and place differs from the one
    -- said last, so going back and forth between the FX list and one plug-in says it once.
    local said = tostring(active.id) .. "|" .. rect .. "|" .. tostring(pv.held.value)
    if said ~= pv.said then
      pv.said = said
      host.log.info(string.format(
        "attachEmbedded: [%s] panel of '%s' id=%s at %s from its content origin, identify=%s — '%s'",
        c.class, tostring(active.title), tostring(active.id), rect, tostring(pv.held.value), label
      ))
    end
    return pv.held.value
  end

  local function pluginControl()
    local active = host.window.active()
    if not active then
      if panels then endStay() end
      return nil
    end
    -- Another window in front than the one a stay is in: whatever this binding makes of it,
    -- the keyboard is not in that panel any more.
    if panels and stay.win ~= nil and not inStayWindow(active) then endStay() end
    local entry = nil
    for _, h in ipairs(hosts) do
      if host.window.test(h, active) then
        entry = h
        break
      end
    end
    if not entry then return nil end
    local chrome = entry.chrome
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
          if string.match(focused.class, pat) then
            if panels then endStay() end
            return nil
          end
        end
      end
    end
    if type(own) == "string" then
      local c = ownByPattern(focusChain)
      if c or not panelFallback then return c end
    end
    -- (b), or a function's answer: the panel. The chrome gate above cannot help here — it
    -- matches Win32 classes — so the same question is asked geometrically (keyboardIn), first
    -- of the DAW's panel, which is what the stay is about, and then of whatever a module's
    -- function made of it.
    --
    -- With the keyboard nowhere anybody can say — an empty chain, or another window's — the
    -- answer is no, and it is given BEFORE the origin is asked for. On a Mac an empty chain is
    -- the host's busy quarantine, in which `host.window.controls` answers nothing too: an
    -- origin function that reads the window's surfaces would answer its fallback then, and the
    -- memo above would keep that for the window's life at that size.
    if not chainKnown(active, focusChain) then
      endStay()
      return nil
    end
    local panel = hostPanelOf(entry, active)
    if not panel and not originFnOf(entry) and not noOriginSaid[entry] then
      noOriginSaid[entry] = true
      host.log.info(string.format(
        "[overlay] '%s' is a plug-in window of %s, whose daw-hosts entry gives no pluginOrigin on "
          .. "%s — no plug-in panel there, so no binding without a control of its own comes up in it",
        tostring(active.title), tostring(entry.daw or "a DAW"), host.os.current
      ))
    end
    if panel then
      if keyboardIn(active, focusChain, panel.bounds) ~= "inside" then
        endStay()
        return nil
      end
      panel.stay = stayIn(active, panel)
    end
    local c = panel
    -- What a function returns is gated as well, so the keyboard has to be inside BOTH: the
    -- DAW's panel, which is the DAW's word for where its chrome ends and what the stay is about,
    -- and the function's control. Not their union, although a control that begins above or left
    -- of the DAW's panel (Kontakt's corner, up to 24 points from the DAW's origin) then has a
    -- strip the keyboard can be on and read as chrome: widened into the chrome, the stay would
    -- no longer end on a visit to Logic's header or REAPER's list, which is the rule's whole
    -- point. A DAW origin that far off is a wrong entry, and the line Kontakt logs for its
    -- corner ("… puts Kontakt's corner dx,dy from the DAW's plug-in origin") is what shows it.
    if type(own) == "function" then
      c = own(active, panel)
      if c and c ~= panel then
        if keyboardIn(active, focusChain, c.bounds) ~= "inside" then return nil end
        if panel then c.stay = panel.stay end
      end
    end
    if not c then return nil end
    if not identify then return c end
    -- A function's control where the DAW gives no panel, and a relational identify: asked at
    -- every evaluation, as a function's control always was. A relational verdict can change
    -- inside one stay (a Kontakt loaded into a Komplete Kontrol through its own browser), so
    -- the stay does not pin it either. Asked at every evaluation, so it must not read.
    if not (cacheId and panel) then
      if not askedEveryTime(c) then return nil end
      return c
    end
    if not panelIdentity(c, active) then return nil end
    return c
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
  -- 150 ms from the menu tests. All of those want the SAME answer.
  --
  -- So memoize it against host.epoch(), which changes whenever an OS event reaches a
  -- module, a timer fires, or the module drives input — every point at which the answer
  -- could differ. Within one such pass the origin is resolved once; a new pass always
  -- re-resolves. Not a time-based cache: a stale origin means clicking the wrong place.
  -- Memoized per EPOCH and per SPEC — shared by every overlay bound to the same spec,
  -- not one cache each.
  --
  -- Resolving an origin is the most expensive thing this runtime does: a window
  -- enumeration plus, for a relational spec like Kontakt's, a UIA subtree search per
  -- candidate control (measured: ~11 ms). A per-overlay cache means every overlay pays it
  -- separately, and they all ask the IDENTICAL question — a cell's `binding` is one table
  -- reused by that cell's base overlay and by every library overlay inheriting it, so
  -- sixty-six overlays across eleven products were re-deriving the same six answers.
  -- Measured: 681 ms of main thread per 500 ms poll, with the plugin merely focused. That
  -- is over its own interval, so the loop never caught up — and speech, hotkeys and the
  -- arbiter all queue behind it.
  --
  -- Keyed on the spec TABLE, so overlays that share a binding share the answer while
  -- overlays with their own binding stay independent. Weak keys, so a spec that goes away
  -- takes its entry with it.
  --
  -- An answer made without asking a stored `identify` — reached inside a pass, where it is not
  -- asked (see "Where `identify` runs", above) — is not kept: the next question outside a pass
  -- in the same epoch resolves again and asks it, rather than finding "no control" kept.
  local function origin(opts)
    local e = host.epoch and host.epoch() or -1
    local memo = originMemo[spec]
    if not memo then
      memo = { epoch = -1, value = nil }
      originMemo[spec] = memo
    end
    if not (opts and opts.fresh) and e == memo.epoch then return memo.value end
    deferred = false
    local c = pluginControl()
    if deferred then return c end
    memo.value = c
    memo.epoch = e
    return memo.value
  end
  -- The match asks the same question ("is the plugin focused?") and so shares the memo:
  -- a recheck plus the activation and coordinate work that follows it resolve once
  -- between them, where each used to enumerate the window afresh.
  --
  -- `present` is asked at every evaluation, so it must not read: it runs in a pass, like a `when`
  -- (see "Where a hook runs"). One that raises is taken as not matching, and said once per binding;
  -- the recheck asking it goes on to its end.
  local presentSaid = false
  local function matched()
    local c = origin()
    if not c then return false end
    if spec.present then
      local okCall, present = pcall(noWait, spec.present, c)
      if not okCall then
        if not presentSaid then
          presentSaid = true
          host.log.info(string.format("attachEmbedded: '%s': its binding's present raised: %s — taken as "
            .. "not matching", tostring(self.label or "overlay"), tostring(present)))
        end
        return false
      end
      if not present then return false end
    end
    return true
  end
  self.contexts[#self.contexts + 1] = { match = matched, origin = origin }
  ensureTrigger(self, opts)
end

-- ── Menus ────────────────────────────────────────────────────────────────────
-- While a plug-in has a menu open, the keys belong to the menu. The overlay gives up its
-- per-control hotkeys — they are RegisterHotKey claims, which outrank the application, so with
-- a menu up Alt+M would re-fire the control under it instead of reaching it — and tells the key
-- hook through host.keys.menuOpen to let its captured navigation keys through. ReaHotkey does
-- the same: a regular check, and the overlay switched off while `WinExist("ahk_class #32768")`.
--
-- HOW a menu is seen is the module's business, not this runtime's. A plug-in that paints its
-- menu inside its own window has nothing in common with one whose menu is a native popup, and
-- the one that knows which it is, is the module written against it. So an overlay names its
-- MENU TESTS — `menus = { … }` on its binding — and this runtime runs them:
--
--   test(o, answer)   "is a menu open over this overlay's plug-in right now?" — calls
--                     answer(seen) exactly once per call, at once or later (from an
--                     imageSearchAsync or snapshotAsync callback). Any value but nil and false
--                     counts as seen, so a search's hit can be handed straight on. `o` is the
--                     overlay.
--
-- The building blocks in O.menuTests are tests like any other; a module lists the ones that
-- see its plug-in's menus, and writes its own (a pixel colour, an image) where none does.
--
-- NO TIMER IN THIS RUNTIME DECIDES THE STATE. Each test has a WORD: from the answer that saw a
-- menu, it says one is open, and it stops saying so once it has answered no twice in a row —
-- one missed read is not a close (an accessibility answer can be the previous tick's, a picture
-- can catch the menu mid-redraw). A menu counts as open while any test's word is that it is.
-- A test that has not answered yet is not asked again until it does, and its word stands
-- meanwhile: a slow test does not make a menu it sees flicker, and one that is outstanding holds
-- up none of the others. So a test that never answers again after seeing a menu keeps that menu
-- open until the overlay leaves the front; the log names it after MENU_STALL_TICKS ticks.
-- Time only sets how OFTEN the tests are asked:
--   • every test on every tick for MENU_LOOK_MS after a control with `opensMenu` fired, and for
--     as long as a menu counts as open;
--   • otherwise a test that declares itself `cheap` on every tick, and each of the others once
--     MENU_BACKSTOP_TICKS ticks have passed since it was last asked, so a menu opened some other
--     way — the user's screen reader, a key that went to the plug-in — is still noticed;
--   • while a cheap test sees the menu, the others are not asked at all — they could only
--     agree, and a walk of the accessibility tree is what makes a test expensive — except while
--     an item waits to be chosen (see "Choosing an item"), when the one that says where the menu
--     is need not be the cheap one.
-- A menu no test sees gets no pass-through: the overlay keeps its keys. That replaced an
-- eight-second stopwatch that assumed a menu after every press, and a thirty-second cap that
-- stopped believing in a menu the user was still reading (2026-09-26). The host's own
-- sixty-second rule for native menus on macOS went the same way the same day.
--
-- A menu that is a WINDOW coming to the front does not take the overlay out of the front: the
-- overlay holds its place while it is up, and when that ends the ordinary match decides again
-- (see "holding its place", below).
--
-- A test that raises has answered no, and says so once in the log. When the overlay leaves the
-- front every word is dropped, and an answer that arrives later for a question asked before
-- that is ignored.
--
-- ONE timer per VM, not one per overlay: the overlays of a module share it. While a menu counts
-- as open over any of them, every one of them that is in front gives up its hotkeys, because
-- the pass-through is one flag for the module, which lets every capture in the window it counts
-- for through.
--
-- `host.keys.menuOpen` is this module's flag, shared by every overlay of the VM and counted for
-- the window the VM's key scope pins, so it is only written while an overlay of this VM that has
-- tests is actually active; otherwise this would clobber what an overlay of this module set while
-- it was live. Another module's flag is its own, and nothing here can touch it.
local MENU_TICK_MS = 150
-- How long a test that is not cheap waits between two questions while no menu is open and none
-- was just opened: 8 ticks x 150 ms, a worst case of ~1.2 s to notice a menu opened by some
-- other route. The accessibility walk is the reason — on Windows it walks the plug-in's whole
-- tree across a process boundary, measured at 50-194 ms, which on every tick was more than the
-- tick. Counted from the test's own last question, so a sibling that takes a while to answer
-- cannot push it off the beat.
local MENU_BACKSTOP_TICKS = 8
-- How long after a control with `opensMenu` fired every test runs on every tick — a cadence, and
-- no more than that for whether a menu is open. It also bounds how long an item may wait for its
-- menu to be seen (see "Choosing an item"): a bound on a wait, after which the item is not chosen
-- and the user hears it, never a guess that the menu is up by then.
local MENU_LOOK_MS = 8000
-- How many more times an item is clicked when the menu it was clicked in is still there on a
-- later tick (see "Choosing an item"): a count of answers, not a time.
local MENU_ITEM_RETRIES = 3
-- Only for a log line: how many ticks a test's answer is outstanding before the log says so.
local MENU_STALL_TICKS = 20
local menuWatched = {}
local menuTimer = false
local menuTick = 0

-- The building blocks, by name. Indexing a name that is not one raises, because a mistyped
-- block in `menus = { … }` would otherwise be a nil that `ipairs` stops at — a test silently
-- missing from the list, and a menu the module believes it covers.
O.menuTests = setmetatable({}, {
  __index = function(_, k)
    error(("O.menuTests has no '%s' — the building blocks are nativePopup, newWindow, "
      .. "accessibility and accessibilityAfterPress"):format(tostring(k)), 2)
  end,
})

-- A menu the OPERATING SYSTEM drew is open in the application in front. ReaHotkey's
-- `WinExist("ahk_class #32768")`, scoped to the foreground application. One flag read, no
-- traversal: cheap enough for every tick. See host.keys.nativeMenuOpen for what each platform
-- counts. u-he's preset menu, Komplete Kontrol's and Melodyne's menu bars are this kind.
O.menuTests.nativePopup = {
  name = "nativePopup",
  cheap = true,
  test = function(_, answer)
    answer(host.keys.nativeMenuOpen() == true)
  end,
}

-- A WINDOW of the plug-in's process that was not there when a control with `opensMenu` was
-- pressed: a popup menu drawn as a window of its own is in the window manager's list from the
-- moment it opens to the moment it closes, whatever the accessibility tree says of it. The
-- macOS sessions measured sforzando's menus this way (a window at level 101 per menu); nothing
-- else there could see them.
--
-- The PROCESS is settled at the press and kept: an embedded origin is a control, which carries
-- no application, so the process is then the active window's — and asked again on a later tick
-- that could be whichever program has just come to the front, every window of which would be
-- "new" (a browser's window was once logged as the plug-in's menu that way).
--
-- It compares against the list taken at the press, so it says nothing about a menu no control
-- of the overlay opened, and it stops comparing — answering false until the next press — once
-- the windows that appeared have all gone (the menu closed), when the user presses one of the
-- overlay's own keys with no menu open (the press opened nothing that took the keys), when the
-- overlay leaves the front, and when its hold over a menu that was a window in front ends with
-- the plug-in having the keyboard again (see "holding its place"). Without an end, any window
-- the process opened later — a dialog that does not take the front, a tip under the parked
-- pointer — would be the menu.
-- While it compares, ANY new window of the process counts; on Windows the host leaves
-- tooltips out of the list, on macOS it filters neither tooltips nor window levels.
--
-- Cost: one window list of one process per question while it is comparing (local and cheap on
-- both platforms — see host.window.windowsOf), nothing at all otherwise. A menu painted inside
-- the plug-in's own window is invisible to it: the window list does not change.
do
  -- The process of the overlay's plug-in right now, or nil when it cannot be told.
  local function processOf(o)
    local origin = o:origin()
    local pid = origin and origin.app and origin.app.pid
    if not pid then
      local a = host.window.active()
      pid = a and a.app and a.app.pid
    end
    return pid
  end

  -- The on-screen windows of process `pid`, keyed by id.
  local function windowsOf(pid)
    local ids = {}
    for _, w in ipairs(host.window.windowsOf(pid)) do ids[w.id] = w end
    return ids
  end

  local watching = setmetatable({}, { __mode = "k" })

  O.menuTests.newWindow = {
    name = "newWindow",
    cheap = true,
    pressed = function(o)
      local pid = processOf(o)
      watching[o] = pid and { pid = pid, before = windowsOf(pid), said = {}, seen = false } or nil
      if not pid then
        host.log.info(("[menu] '%s': newWindow could not tell the plug-in's process at the "
          .. "press, so it cannot see this menu"):format(tostring(o.label)))
      end
    end,
    forget = function(o)
      watching[o] = nil
    end,
    test = function(o, answer)
      local w = watching[o]
      if not w then return answer(false) end
      -- The answer is the MENU, not just "yes": where it is, for an item to be chosen in it (see
      -- "Choosing an item" below). Of several windows that appeared, the largest — a toolkit
      -- can draw a popup's shadow as windows of their own, strips a few pixels wide around it
      -- (JUCE does on Windows) — and of two the same size, the one the list names first, which
      -- on both platforms is the one in front.
      local menu = nil
      for _, win in ipairs(host.window.windowsOf(w.pid)) do
        local id = win.id
        if not w.before[id] then
          if not w.said[id] then
            w.said[id] = true
            host.log.info(("[menu] '%s': newWindow: a window appeared for the plug-in since the "
              .. "press — id %s%s, layer %d, %dx%d at %d,%d"):format(tostring(o.label), tostring(id),
              (win.class ~= nil and win.class ~= "") and (" class '" .. win.class .. "'") or "",
              win.layer or 0, win.w or 0, win.h or 0, win.x or 0, win.y or 0))
          end
          if menu == nil or (win.w or 0) * (win.h or 0) > (menu.w or 0) * (menu.h or 0) then
            menu = win
          end
        end
      end
      if menu then
        w.seen = true
        -- `window`: the same window as a host.window id, which host.window.ownsPoint takes. On
        -- Windows the window manager's id IS the HWND every host.window call names; on macOS
        -- windowsOf lists the window server's CGWindowID, a different numbering, so there is
        -- none to give — `id` is asked about there, with ownsPoint's `listed` option.
        return answer({
          kind = "window",
          id = menu.id,
          window = (host.os.current == "windows") and menu.id or nil,
          layer = menu.layer, class = menu.class,
          x = menu.x, y = menu.y, w = menu.w, h = menu.h,
        })
      end
      if w.seen then
        -- Every window that appeared has gone: the menu this press opened has closed, and this
        -- test has nothing more to say until the next press.
        host.log.info(("[menu] '%s': newWindow: the window that appeared has gone"):format(tostring(o.label)))
        watching[o] = nil
      end
      answer(false)
    end,
  }
end

-- A MENU ELEMENT anywhere in the plug-in's accessibility tree:
-- `host.element.find(origin, "", host.element.type.Menu)`. Not cheap — see
-- MENU_BACKSTOP_TICKS — so it runs on every tick only after a press and while a menu is open.
-- Windows: any element of the Menu type in the plug-in control's UI Automation subtree, with
-- no visibility test, which is how Kontakt's Qt menus are seen. macOS: the call is
-- special-cased to "is a menu open in this application", and an application the host has
-- found busy is left alone for 5 s, during which the answer is no. Answered once per epoch,
-- and a timer tick does not turn the epoch over, so a tick can be handed the previous tick's
-- answer; the one-miss rule absorbs that. Kept for the transition to modules' own tests: an
-- element that is in the tree whether or not anything is drawn counts as a menu for as long
-- as it is there.
local function menuElementIn(o)
  local ctrl = o:origin()
  return ctrl ~= nil and ctrl.id ~= nil
    and host.element.find(ctrl.id, "", host.element.type.Menu) == true
end

O.menuTests.accessibility = {
  name = "accessibility",
  cheap = false,
  test = function(o, answer)
    answer(menuElementIn(o))
  end,
}

-- The same question, asked only from a press of a control with `opensMenu` until that press is
-- done with: the menu it opened was seen and has closed (missed twice in a row, the one-miss
-- rule), the user pressed one of the overlay's own keys with no menu open, the overlay left the
-- front, or its hold over a menu that was a window in front ended with the plug-in having the
-- keyboard again. Until the next press it answers no without walking anything.
--
-- For a plug-in whose tree can hold a Menu element while nothing is open. Kontakt 8 in a DAW
-- did on 2026-09-26: an element appeared under the snapshot bar on Cinematic Studio Strings and
-- stayed for over a minute, through a switch to another application. Counted by `accessibility`
-- such an element keeps the overlay out for as long as it exists; counted by this one, only
-- until the user leaves the plug-in and comes back. What it gives up is a menu no control of
-- the overlay opened.
do
  local armed = setmetatable({}, { __mode = "k" })

  O.menuTests.accessibilityAfterPress = {
    name = "accessibilityAfterPress",
    cheap = false,
    pressed = function(o)
      armed[o] = { seen = false, misses = 0 }
    end,
    forget = function(o)
      armed[o] = nil
    end,
    test = function(o, answer)
      local a = armed[o]
      if not a then return answer(false) end
      local seen = menuElementIn(o)
      if seen then
        a.seen, a.misses = true, 0
      elseif a.seen then
        a.misses += 1
        if a.misses >= 2 then
          -- The menu this press opened has closed: nothing more to say until the next press.
          armed[o] = nil
        end
      end
      answer(seen)
    end,
  }
end

local MENU_TESTS_HELP = "name the menu tests instead: O.menuTests.nativePopup (a menu the "
  .. "operating system drew), O.menuTests.newWindow (a window that appeared since the click), "
  .. "O.menuTests.accessibility (a Menu element in the plug-in's accessibility tree), "
  .. "O.menuTests.accessibilityAfterPress (the same, asked only after a click), or a "
  .. "function(o, answer) of the module's own — see docs/api/overlay.md, 'Menus'"

-- `menus` as the runtime keeps it: one record per test, `{ name, cheap, test, pressed?,
-- forget? }` plus its word and its outstanding question (see resetMenuTest), fresh per overlay.
-- Raises for anything that is not a list of tests, naming the overlay — at bind time, where the
-- mistake is, rather than as a menu that silently keeps the keys.
local function menuTestsOf(self, list)
  local who = tostring(self.label)
  if list == true then
    error(("overlay '%s': `menus = true` no longer says how a menu is seen — %s")
      :format(who, MENU_TESTS_HELP), 0)
  end
  if type(list) ~= "table" then
    error(("overlay '%s': `menus` must be a list of menu tests, got %s — %s")
      :format(who, type(list), MENU_TESTS_HELP), 0)
  end
  local out = {}
  for i, t in ipairs(list) do
    if type(t) == "function" then
      out[i] = { name = "test " .. i, cheap = false, test = t }
    elseif type(t) == "table" and type(t.test) == "function" then
      for _, hook in ipairs({ "pressed", "forget" }) do
        if t[hook] ~= nil and type(t[hook]) ~= "function" then
          error(("overlay '%s': menu test %d's `%s` must be a function(o), got %s")
            :format(who, i, hook, type(t[hook])), 0)
        end
      end
      out[i] = {
        name = t.name ~= nil and tostring(t.name) or ("test " .. i),
        cheap = t.cheap == true,
        test = t.test,
        pressed = t.pressed,
        forget = t.forget,
      }
    else
      error(("overlay '%s': menu test %d is a %s, not a function(o, answer) or a table with "
        .. "`test` — %s"):format(who, i, type(t), MENU_TESTS_HELP), 0)
    end
  end
  -- A gap in the list: `ipairs` stopped before a later entry, which would drop it in silence.
  for k in pairs(list) do
    if type(k) ~= "number" or k < 1 or k > #out or k % 1 ~= 0 then
      error(("overlay '%s': `menus` has an entry at %s that is not in the list — a nil before "
        .. "it (a mistyped name?) or a named key"):format(who, tostring(k)), 0)
    end
  end
  return out
end

-- A test's state for one stay in front: its word (`saw`, and the misses since it last saw),
-- the question it has not answered yet (`asking`, a token; a later answer to an earlier
-- question does not match it), how many ticks that has been outstanding, and the tick it was
-- last asked on.
local function resetMenuTest(t)
  t.saw, t.misses, t.asking, t.waited, t.lastAsked = false, 0, nil, 0, nil
end

-- Is overlay `o` holding its place over its own menu's window (see "holding its place")? It
-- then holds no keys at all, and has no say in the pass-through flag: its window is not in front.
local function heldOnMenu(o)
  return o._menu ~= nil and o._menu.hold ~= nil
end

function Overlay:_heldOnMenu()
  return heldOnMenu(self)
end

-- Whether a menu counts as open over this overlay now: some test's word is that one is (see
-- above). For a module's own code that tidies up after a menu of its own — VPS Avenger clicks a
-- popup's opener again to close one a choice left open, and must not open one that has gone.
-- false for an overlay with no menu tests, and after it left the front.
function Overlay:menuOpen()
  return self._menu ~= nil and self._menu.open == true
end

-- Is a menu open over any overlay of this VM that is in front? What host.keys.menuOpen is set to.
local function anyMenuOpen()
  for _, o in ipairs(menuWatched) do
    if o.active and not heldOnMenu(o) and o._menu.open then return true end
  end
  return false
end

-- The menu state, applied: the hook's pass-through flag, and the hotkeys of every overlay of
-- this VM in front — given up by all of them while a menu is open over any, because the
-- pass-through lets every captured key in the window it counts for through, not one overlay's.
-- Idempotent (see _suspendControlHotkeys), so the tick re-applies it: an overlay that came to
-- the front between two transitions falls into step, and a transition that failed half-way is
-- completed on the next tick. An overlay holding its place over its menu's window is left out:
-- it holds no keys, and the flag is not written for it — another overlay of this module may be
-- in front, and the flag is one for the module.
local function applyMenuState()
  local open, inFront = anyMenuOpen(), false
  for _, o in ipairs(menuWatched) do
    if o.active and not heldOnMenu(o) then inFront = true end
  end
  if inFront then host.keys.menuOpen(open) end
  for _, o in ipairs(menuWatched) do
    if o.active and not heldOnMenu(o) then o:_suspendControlHotkeys(open) end
  end
end

-- An overlay of this VM left the front while another stays (see _deactivate): the flag is what
-- the ones in front say now. Not `false` outright — a menu still open over another of them would
-- lose its keys to the captures — nor left as it was, which may be the leaver's own `true`.
function Overlay:_menuFlagLeft()
  host.keys.menuOpen(anyMenuOpen())
end

-- Calls one hook of every test that has it, guarded: a module's hook must not take the
-- overlay's key handling or its deactivation with it.
local function menuHook(self, hook)
  for _, t in ipairs(self._menu.tests) do
    if t[hook] then
      local ok, err = pcall(t[hook], self)
      if not ok then
        host.log.info(("[menu] '%s': menu test '%s' failed in `%s`: %s")
          :format(tostring(self.label), t.name, hook, tostring(err)))
      end
    end
  end
end

-- The menu opened or closed: the overlay goes out (hotkeys given up, keys to the menu) or
-- comes back in. Called on the transition only, so each is said and done exactly once. The
-- state is written first, so nothing after it can leave it half-changed.
function Overlay:_menuChange(open, by)
  local st = self._menu
  local now = host.now()
  st.open = open
  if open then
    -- Counted, so that an announcement's answer can tell a menu that opened and closed again
    -- while it was being read (see speakControl).
    self._menuOpens += 1
    st.since, st.escape = now, nil
    -- Where the keyboard is when the menu is first seen: whether keys that pass through can
    -- reach the menu at all is the question the log could not answer (2026-09-26). Guarded: a
    -- log line must not cost the hand-over.
    local ok, f = pcall(function()
      return (host.window.focusChain() or {})[1]
    end)
    host.log.info(("[menu] '%s': a menu is open — test '%s' sees it, %s; keyboard focus on %s")
      :format(tostring(self.label), tostring(by),
        st.pressed and ("after '" .. st.pressed .. "' was pressed")
          or "and no control of this overlay opened it",
        not ok and ("an unknown element (" .. tostring(f) .. ")")
          or f and ("[" .. tostring(f.class) .. "] id=" .. tostring(f.id))
          or "nothing the platform reports"))
  else
    host.log.info(("[menu] '%s': the menu has closed — no test sees it any more (it counted as "
      .. "open for %d ms%s)"):format(tostring(self.label), now - (st.since or now),
      st.escape and (", and was seen on " .. st.escape .. " tick(s) after Escape went through")
        or ""))
    st.since, st.escape, st.pressed = nil, nil, nil
    -- An item still waiting when its menu closes was never chosen: the menu never said where it
    -- was, or the item was not inside it. Said now — the menu that could have held it is gone.
    if self._pick then self:_pickEnd("closed") end
  end
  applyMenuState()
end

-- A transition, guarded: the state is settled whatever happens, and a hand-over that fails is
-- said once per failure and completed by the next tick's applyMenuState.
function Overlay:_menuSet(open, by)
  local ok, err = pcall(self._menuChange, self, open, by)
  if not ok then
    self._menu.open = open
    host.log.info(("[menu] '%s': handing the keys %s failed: %s — the next tick tries again")
      :format(tostring(self.label), open and "to the menu" or "back", tostring(err)))
  end
end

-- One answer of test `t`: its word, and the transition when it is the first to see the menu or
-- the last to stop seeing it. `value` is what the test answered, handed to an item waiting to be
-- chosen (see "Choosing an item").
function Overlay:_menuAnswer(t, seen, value)
  local st = self._menu
  if seen then
    t.saw, t.misses = true, 0
    st.seenThisTick = true
    -- Counted, so that holding its place can ask whether a test sees a menu NOW rather than read
    -- the word, which stays "open" for a miss after the menu has gone (see _menuHold).
    st.sightings = (st.sightings or 0) + 1
    if not st.open then self:_menuSet(true, t.name) end
    if self._pick then self:_pickSeen(t, value) end
    return
  end
  if not t.saw then return end
  -- One missed read is not a close: the test's next answer decides.
  t.misses += 1
  if t.misses < 2 then return end
  t.saw, t.misses = false, 0
  if not st.open then return end
  for _, u in ipairs(st.tests) do
    if u.saw then return end
  end
  self:_menuSet(false)
end

-- The menu test being asked inline in a pass, while it is asked, and how it fails. Its own
-- `coroutine.yield` goes past _menuAsk's guard to the pass, which ends with it (noWait), and
-- whoever opened the pass finds the test here (menuPassEnded).
local askingInline = nil

-- A pass of menu questions ended with `err`. When a test yielded in it: that test has failed, as
-- one that raises has — said once, counted as not seeing a menu, its question answered — and is
-- asked in a pass of its own from then on, so that it fails alone and the others are asked. True
-- then; false for any other error, which the caller raises again.
local function menuPassEnded(err)
  local a = askingInline
  askingInline = nil
  if not a then return false end
  a.t.yields = true
  a.failed(err)
  return true
end

-- Asks test `t` once. The answer is taken whenever it comes, as long as it answers this
-- question: a question asked before the overlay left the front is not.
function Overlay:_menuAsk(t)
  local q = {}
  t.asking, t.waited, t.lastAsked = q, 0, menuTick
  local answered = false
  local function answer(v)
    if answered then
      if not t.saidTwice then
        t.saidTwice = true
        host.log.info(("[menu] '%s': menu test '%s' answered twice to one question — the "
          .. "second answer is ignored"):format(tostring(self.label), t.name))
      end
      return
    end
    answered = true
    if t.asking ~= q then return end -- asked before the overlay left the front
    t.asking = nil
    self:_menuAnswer(t, v ~= nil and v ~= false, v)
  end
  local function failed(err)
    if not t.saidFailed then
      t.saidFailed = true
      host.log.info(("[menu] '%s': menu test '%s' failed: %s — counted as not seeing a menu")
        :format(tostring(self.label), t.name, tostring(err)))
    end
    if not answered then answer(false) end
  end
  -- A test is asked on every tick, so it must not read: it runs in a pass (see "Where a hook
  -- runs"), inline in the round's. One that yields there has failed, as one that raises: the
  -- pass ends with the yield, and its opener says so (menuPassEnded); from then on the test is
  -- asked in a pass of its own.
  local ok, err
  if t.yields then
    ok, err = pcall(ownPass, t.test, self, answer)
  else
    local outer = askingInline
    askingInline = { t = t, failed = failed }
    ok, err = pcall(noWait, t.test, self, answer)
    askingInline = outer
  end
  if not ok then failed(err) end
end

-- The tick that has just ended, looked back on before this one's keys and questions: an Escape
-- went through to the menu, and the menu was seen again on that tick. Said once, at two: a
-- menu that closes on Escape is gone by then.
function Overlay:_menuFollowEscape()
  local st = self._menu
  if st.open and st.escape and st.seenThisTick then
    st.escape += 1
    if st.escape == 2 then
      host.log.info(("[menu] '%s': the menu is still seen two ticks after Escape went "
        .. "through — the key did not reach it, or it ignores Escape"):format(tostring(self.label)))
    end
  end
  st.seenThisTick = false
end

-- Asks the tests that are due on this tick: the cheap ones first, and the others only when no
-- cheap one sees the menu. A test whose answer is outstanding is not asked again. `tick`: this is
-- the timer's round, which counts how long an answer has been outstanding; a round asked by a
-- window event (see _menuHold) does not.
--
-- The round is one pass (see "Where a hook runs"): the tick's for all its overlays, or one of its
-- own when a window event asks it (_menuHold, _menuKeysBack).
function Overlay:_menuRound(now, tick)
  if passDepth == 0 then
    local ok, err = pcall(scanPass, self, self._menuRound, self, now, tick)
    if not ok and not menuPassEnded(err) then error(err, 0) end
    return
  end
  local st = self._menu
  for _, t in ipairs(st.tests) do
    if t.asking and tick then
      t.waited += 1
      if t.waited == MENU_STALL_TICKS then
        host.log.info(("[menu] '%s': menu test '%s' has not answered for %d ticks — it is not "
          .. "asked again until it does, and its word stands meanwhile (%s)")
          :format(tostring(self.label), t.name, t.waited,
            t.saw and "that a menu is open" or "no menu"))
      end
    end
  end
  for _, t in ipairs(st.tests) do
    if not self.active then return end
    if t.cheap and not t.asking then self:_menuAsk(t) end
  end
  -- Not while an item waits to be chosen, or to be seen taken: a cheap test that sees the menu
  -- without saying where (nativePopup) would otherwise keep the one test that can place the item
  -- from ever being asked, and the item would end as "the menu does not say where it is".
  if not self._pick then
    for _, t in ipairs(st.tests) do
      if t.cheap and t.saw then return end
    end
  end
  local all = st.open or now < st.lookUntil
  for _, t in ipairs(st.tests) do
    if not self.active then return end
    if not t.cheap and not t.asking
      and (all or t.lastAsked == nil or menuTick - t.lastAsked >= MENU_BACKSTOP_TICKS) then
      self:_menuAsk(t)
    end
  end
end

local function menuTickFn()
  menuTick += 1
  local anyActive = false
  for _, o in ipairs(menuWatched) do
    if o.active then
      anyActive = true
      break
    end
  end
  if not anyActive then return end
  for _, o in ipairs(menuWatched) do
    if o.active then o:_menuFollowEscape() end
  end
  -- The keys the hook let through to a menu since the last tick, logged against the overlay
  -- whose menu is open — they are user-paced, so a line each is affordable, and an Escape is
  -- followed to see whether the menu then closes (see _menuFollowEscape). Drained here every
  -- tick an overlay is active, so a key noted for an earlier menu is not reported against a
  -- later one.
  local passed = host.keys.passedThrough()
  if #passed > 0 then
    for _, o in ipairs(menuWatched) do
      local st = o._menu
      if o.active and st.open then
        local names = {}
        for _, k in ipairs(passed) do
          names[#names + 1] = tostring(k.key)
          if k.key == "Escape" then st.escape = 0 end
        end
        host.log.info(("[menu] '%s': went through to the menu: %s")
          :format(tostring(o.label), table.concat(names, ", ")))
      end
    end
  end
  local now = host.now()
  -- The tests' asking, and only that, in ONE pass for every overlay (see "Where a hook runs"): a
  -- menu test must not read. Each overlay's origin first, outside the pass, so that a stored
  -- `identify` asked for it runs in the tick's handler. What follows — the items given up, the
  -- rechecks of overlays holding their place, the state applied — runs outside it: a recheck
  -- resolves the origin, and with it a stored `identify` that may wait.
  for _, o in ipairs(menuWatched) do
    originFirst(o)
  end
  -- A test that yields ends the pass (see _menuAsk): it has failed, and the overlays after it are
  -- asked on the next tick, by when it is asked alone. The rest of this tick goes on.
  local ok, err = pcall(noWait, function()
    for _, o in ipairs(menuWatched) do
      if o.active then o:_menuRound(now, true) end
    end
  end)
  if not ok and not menuPassEnded(err) then error(err, 0) end
  -- An item whose press no test has answered with a menu it could be chosen in, for as long as
  -- the tests are asked on every tick after a press: dropped, and said (see "Choosing an item").
  -- One the user has left for another window of the same overlay (a second Avenger) is dropped
  -- quietly, as a pick seen on another window is: nothing is said about a window they left.
  for _, o in ipairs(menuWatched) do
    local p = o._pick
    if p and o.active and now >= p.untilMs then
      o:_pickEnd(o:hwnd() ~= p.pinned and "moved" or "due")
    end
  end
  -- An overlay holding its place over its own menu's window whose tests no longer see a menu:
  -- whatever is in front now decides again, as a window event would have (see _menuHold). Its
  -- tests have just been asked, and none saw a menu (the word would still say open), so they are
  -- not asked a second time on this tick.
  for _, o in ipairs(menuWatched) do
    local st = o._menu
    if o.active and st.hold and not st.open then
      st.askedThisTick = true
      local ok, err = pcall(o._recheck, o)
      st.askedThisTick = nil
      if not ok then error(err, 0) end
    end
  end
  applyMenuState()
end

-- Runs `tests` for this overlay — what `menus = { … }` on a binding does. Returns the overlay.
-- An empty list runs nothing. Raises for `true`, for anything that is not a list of tests, and
-- for a gap in the list (see menuTestsOf).
function Overlay:_watchMenus(tests)
  local list = menuTestsOf(self, tests)
  if #list == 0 then return self end
  for _, t in ipairs(list) do resetMenuTest(t) end
  self._menu = self._menu or { open = false, lookUntil = 0, seenThisTick = false }
  self._menu.tests = list
  local known = false
  for _, o in ipairs(menuWatched) do
    if o == self then
      known = true
      break
    end
  end
  if not known then menuWatched[#menuWatched + 1] = self end
  if not menuTimer then
    menuTimer = true
    -- Disable-safe recurring poll (survives a disable/enable cycle, unlike a Lua
    -- self-rescheduling host.timer.after).
    host.timer.every(MENU_TICK_MS, menuTickFn)
  end
  return self
end

-- A control with `opensMenu` is being activated, before it acts: the tests run on every tick
-- for a while, and a test that compares against this moment (newWindow) takes its starting
-- point. Counts no menu as open — only a test does that. In a calibrating run it also takes the
-- menu shots (see the Calibration section).
function Overlay:_menuOpening(c)
  if not self.active then return end
  local st = self._menu
  -- A new press: an item the last one was waiting to choose is not this press's menu.
  if self._pick then self:_pickEnd("press") end
  if st then
    st.lookUntil = host.now() + MENU_LOOK_MS
    st.pressed = c and c.label or "a control"
    -- The PRESS, for holding its place if the menu comes up as a window in front (see "holding
    -- its place", below): the window in front and its process, the binding's own context the
    -- overlay is active through — not a stand-in it uses while it holds its place — the origin
    -- that resolves to now, and the control. The next press starts it again.
    local a = host.window.active()
    local pid = a and a.app and a.app.pid
    local base = self.activeCtx and (self.activeCtx.base or self.activeCtx) or nil
    st.press = nil
    if a and pid and base then
      st.press = { window = a.id, front = a, pid = pid, base = base, origin = self:origin(),
        control = st.pressed }
    end
    menuHook(self, "pressed")
  end
  -- Whether this press was photographed, so the item it chooses is photographed with it (see
  -- _pickShot): the same first two openings per control, not a count of its own.
  self._pressShot = false
  if calibrating then self._pressShot = self:_menuShots(c) == true end
end

-- One of the overlay's own keys arrived with no menu counted open: the user is back in the
-- overlay, so whatever the last press was expected to open did not take the keys. The tests
-- forget that press (newWindow stops comparing against it), and so does holding its place: a
-- window that comes to the front after this is not that press's menu. See ownKey.
function Overlay:_menuOwnKey()
  local st = self._menu
  if not st or st.open then return end
  st.pressed, st.press = nil, nil
  menuHook(self, "forget")
  -- The user moved on before any menu was seen: the item is not chosen, and nothing is said —
  -- the key they pressed is about to say what it does.
  if self._pick then self:_pickEnd("key") end
end

-- The overlay left the front: the state starts again from nothing when it is back — the press
-- and a hold over its menu's window with it.
function Overlay:_menuLeave()
  local st = self._menu
  if not st then return end
  if st.open then
    host.log.info(("[menu] '%s': left the front with a menu counted open — the tests start "
      .. "again from nothing when it is back"):format(tostring(self.label)))
  end
  st.open = false
  for _, t in ipairs(st.tests) do resetMenuTest(t) end
  st.escape, st.pressed, st.since, st.seenThisTick = nil, nil, nil, false
  st.press, st.hold = nil, nil
  st.lookUntil = 0
  menuHook(self, "forget")
  if self._pick then self:_pickEnd("left") end
end

-- ── Choosing an item ─────────────────────────────────────────────────────────
-- A plug-in whose menu is a popup it draws itself — every JUCE plug-in's is, and VPS Avenger's
-- MENU, zoom list and redo list are by the avenger_control project's account — publishes no item
-- anybody can press. What a sighted user does is click the opener and then the item, and the
-- obvious way to say that here, a hotspot with `points = { opener, item }`, fails twice over:
--
--   * the second click is refused. Every click of a sequence asks host.window.ownsPoint of the
--     PLUG-IN's window first, and a popup is a window of its own drawn over it, so the item point
--     answers "another window is drawn there" — "something else is covering …", every time;
--   * and `settle` would be a timer deciding that the menu is up by now. It is exactly the guess
--     this project has stopped making: too short and the click lands on the plug-in under a menu
--     that has not appeared, too long and every choice is slow.
--
-- So an item is chosen when a MENU TEST SEES THE MENU, and at a place the test's answer says:
--
--   1. the opener (a hotspot's `at`, with its `button`) is placed and checked like any hotspot's
--      click; then it counts as the press of a control with `opensMenu` — the tests run on every
--      tick from here, newWindow takes its list — and it is clicked. Placed first, so that a press
--      whose opener cannot be clicked starts nothing;
--   2. the first answer that sees a menu AND says where it is — `{ x, y, w, h }` on screen, which
--      newWindow answers with the window that appeared — places the item. A table `menuItem` is
--      its offset from the menu's corner, authored and scaled like every other distance of the
--      overlay, by the factor the OPENER was placed with (the popup is drawn at the zoom it opened
--      at, and a factor read off the plug-in's screen is least likely to be readable while a popup
--      covers it). A function is handed the overlay, the menu and that factor, and answers the
--      offset in screen pixels — it works from the menu's rectangle, which is in screen pixels,
--      so its answer is not scaled again, as a `dragBy` function's is not; `false` for "this
--      window is not my menu, look at the next answer" (a tooltip), nil for "cannot be placed";
--   3. the item must lie inside that rectangle, and must not be covered: a menu that is a window
--      is asked host.window.ownsPoint about ITSELF — by its host.window id (`window`, Windows), or
--      by its id in windowsOf's numbering with ownsPoint's `listed` option (macOS); one drawn
--      inside the plug-in (no `kind`) gets the check every click of the overlay gets;
--   4. then it is clicked — on that answer, in that pass, with no wait;
--   5. and it counts as CHOSEN only once the menu is seen to have taken it. JUCE's popup ignores
--      a mouse-up that comes too soon after the popup's window was created — a quarter of a second
--      in `juce_PopupMenu.cpp` as it was read for this, from memory, and not measured against the
--      JUCE any plug-in here ships — while the first tick that lists a new window comes 0–150 ms
--      after it appeared. An ignored click leaves the menu open, the keys its, and nothing said.
--      So the item keeps waiting after its click: an answer to a question asked on a LATER tick
--      that still shows the same menu — the same window, at the same place and size — is a click
--      the menu did not take, and the item is clicked again, at most MENU_ITEM_RETRIES more times
--      (a count of answers, not a time: three more ticks cover a quarter of a second at any
--      cadence of 150 ms or slower); the menu closing, an answer showing another window in its
--      place, or the overlay leaving the front after the click is the item taken, and only then
--      is `onDone` told it was chosen. Detection, never a wait before the click. A menu whose
--      item keeps it open — a tick box in a menu — would take the second click as a second
--      choice, so it opts out with `retries = 0`: chosen at its click, and not followed.
--
-- Relative to the MENU rather than to the opener, because a toolkit puts a popup where it fits:
-- below its opener, flipped above it when the screen ends first, shifted left at the right edge
-- (JUCE does all three). An offset from the opener is right only where the popup had room — and
-- wrong exactly when the plug-in sits low on the screen, which a sighted user never notices and a
-- blind one cannot. The menu's own corner moves with it. A `menuItem` function is handed the
-- menu, and can place its item from anything else it knows.
--
-- What ends it without a choice, and what is said:
--   * the time the tests are asked on every tick after a press (MENU_LOOK_MS) runs out — a bound
--     on how long a press may wait for its menu, not a guess that the menu is up: a later menu is
--     another press's. Said: no menu was seen; or none it could be chosen in; or the menu did not
--     say where it is; or the item was not inside it;
--   * the menu closes before the item's click — the same sentences;
--   * one of the overlay's own keys, the overlay leaving the front, another press, or the overlay
--     on another window, first — the user moved on; logged, not said;
--   * the item cannot be placed (a function with no answer), or it is covered — said, like any
--     control that cannot act;
--   * the menu is still there after the last click on the item — said: it did not take it.
-- A second press of the same control while its item is still waiting for its menu is not a new
-- press: the opener is not clicked again (see ownKey), and the log says so.
local PICK_LOG = "[menu item] '%s': '%s' "

-- The rectangle a menu test's answer gives, or nil.
local function menuRectOf(v)
  if type(v) == "table" and type(v.x) == "number" and type(v.y) == "number"
    and type(v.w) == "number" and type(v.h) == "number" and v.w > 0 and v.h > 0 then
    return v
  end
  return nil
end

-- Whether answer `m` shows menu `was`, the one an item was clicked in: the same place and size,
-- and the same window where both answers name one — two tests on the list can answer the same
-- menu, one with a window's id and one with only its rectangle.
local function sameMenu(m, was)
  if m.id ~= nil and was.id ~= nil and m.id ~= was.id then return false end
  return m.x == was.x and m.y == was.y and m.w == was.w and m.h == was.h
end

-- Control `c`'s opener has just been clicked: its item is chosen when a test sees the menu.
-- `onDone(overlay, chosen, why)` is O:chooseMenuItem's, `retries` its count (default
-- MENU_ITEM_RETRIES). The factor the opener was placed with is kept for the item (step 2).
function Overlay:_armPick(c, onDone, retries)
  local k = nil
  if scaledFor(self, c) then k = factorNow(self) end
  self._pick = {
    c = c,
    pinned = self:hwnd(),
    untilMs = host.now() + MENU_LOOK_MS,
    onDone = onDone,
    k = k,
    retries = retries or MENU_ITEM_RETRIES,
    shoot = self._pressShot == true,
    noWhere = nil, -- the test that saw the menu without saying where it is
    outside = nil, -- where the item fell, outside the menu that was seen
    other = nil,   -- a window the `menuItem` function said is not its menu
    clicked = nil, -- once the item is clicked: { x, y, menu, tick, clicks }
    keyPending = nil, -- one of the overlay's own keys is running (see ownKey)
  }
  host.log.info((PICK_LOG .. "clicked its opener — the item is chosen when a menu test sees the "
    .. "menu, for up to %d ms"):format(tostring(self.label), tostring(c.label), MENU_LOOK_MS))
end

-- A press of control `c` while its item is still waiting for its menu, from the first press: the
-- opener is not clicked again (see ownKey). Returns whether the press is dropped.
function Overlay:_pickRepeat(c)
  local p = self._pick
  if not (p and p.c == c and not p.clicked) then return false end
  p.keyPending = nil
  host.log.info((PICK_LOG .. "pressed again while its menu is still being waited for — the opener "
    .. "is not clicked again; the first press stands"):format(tostring(self.label), tostring(c.label)))
  return true
end

-- Calls the pick's `onDone`, guarded: a module's hook must not take the menu tick with it.
function Overlay:_pickDone(p, chosen, why)
  if not p.onDone then return end
  local ok, err = pcall(p.onDone, self, chosen, why)
  if not ok then
    host.log.info((PICK_LOG .. "onDone failed: %s"):format(tostring(self.label),
      tostring(p.c.label), tostring(err)))
  end
end

-- The pick is over. `why` is one of:
--   press / key / left / moved — the user or the world moved on first: logged only;
--   due / closed — no answer placed the item in time, or the menu closed: said, in the words of
--                  what was seen;
--   unplaced / covered — the item could not be placed, or something is drawn over it: said;
--   refused — refuseOutside has said it already: logged only;
--   untaken — the menu was still there after the last click on the item: said.
-- Once the item has been clicked, every other ending — the menu closed, another window in its
-- place ("taken"), the overlay left the front or moved on — is the item CHOSEN: onDone(true).
function Overlay:_pickEnd(why, detail)
  local p = self._pick
  if not p then return end
  self._pick = nil
  local label = tostring(p.c.label or "That control")
  local k = p.clicked
  if k and why ~= "untaken" and why ~= "covered" and why ~= "refused" then
    local how
    if why == "closed" then
      how = "the menu has closed"
    elseif why == "taken" then
      how = tostring(detail)
    elseif why == "due" then
      how = "the menu was not seen again after it"
    else
      how = ({ left = "the overlay left the front after it", moved = "the overlay is on another "
        .. "window after it", key = "the overlay's own keys are back, no menu open",
        press = "another press came after it" })[why] or tostring(why)
    end
    host.log.info((PICK_LOG .. "chosen — clicked %d time(s) at (%d,%d); %s"):format(
      tostring(self.label), tostring(p.c.label), k.clicks, k.x, k.y, how))
    self:_pickDone(p, true, nil)
    return
  end
  local said, logged
  if why == "press" then
    logged = "not chosen — another press came first"
  elseif why == "key" then
    logged = "not chosen — one of the overlay's own keys came first, with no menu open"
  elseif why == "left" then
    logged = "not chosen — the overlay left the front first"
  elseif why == "moved" then
    logged = "not chosen — the overlay is on another window now"
  elseif why == "due" or why == "closed" then
    if p.outside then
      said = label .. " is not where it should be"
      logged = "not chosen — the item, at " .. p.outside .. ", is outside the menu that was seen"
    elseif p.other then
      said = label .. ": no menu it can be chosen in was seen, nothing was chosen"
      logged = "not chosen — its menuItem function took no window seen for its menu (the last at "
        .. p.other .. ")"
    elseif p.noWhere then
      said = label .. ": the menu does not say where it is, nothing was chosen"
      logged = "not chosen — test '" .. p.noWhere .. "' sees a menu but does not say where it is, "
        .. "and no test that does saw it"
    elseif why == "closed" then
      said = label .. ": the menu closed before anything was chosen"
      logged = "not chosen — the menu closed before a test placed the item"
    else
      said = label .. ": no menu was seen, nothing was chosen"
      logged = ("not chosen — no menu test saw a menu within %d ms of the press"):format(MENU_LOOK_MS)
    end
  elseif why == "unplaced" then
    said = label .. " is not available now"
    logged = "not chosen — the item cannot be placed now: " .. tostring(detail)
  elseif why == "covered" then
    said = "something else is covering " .. label
    logged = "not chosen — " .. tostring(detail)
  elseif why == "untaken" then
    said = label .. ": the menu did not take the choice"
    logged = ("not chosen — the menu was still there after %d click(s) on the item at (%d,%d)")
      :format(k and k.clicks or 0, k and k.x or 0, k and k.y or 0)
  else
    logged = "not chosen — " .. tostring(detail or why)
  end
  host.log.info((PICK_LOG .. "%s"):format(tostring(self.label), tostring(p.c.label), logged))
  if said then host.speech.output(said, { interrupt = true }) end
  self:_pickDone(p, false, why)
end

-- What is drawn at (x, y) over menu `m`, for control `c`: nil and how it was asked when it is the
-- menu (or nothing can say), the reason when another window is, or "refused" when the overlay's
-- own check refused the point and said so (step 3).
function Overlay:_pickCovered(c, m, x, y)
  if m.window ~= nil then
    local ok, mine = pcall(host.window.ownsPoint, m.window, x, y)
    if ok and mine == false then
      return ("(%d,%d) is inside the menu's window id %s, but another window is drawn there")
        :format(x, y, tostring(m.window))
    end
    return nil, "its window id " .. tostring(m.window) .. " is asked what is drawn there"
  end
  if m.kind == "window" and m.id ~= nil then
    local ok, mine = pcall(host.window.ownsPoint, m.id, x, y, { listed = true })
    if ok and mine == false then
      return ("(%d,%d) is inside the menu's window %s of the window list, but another window is "
        .. "drawn there"):format(x, y, tostring(m.id))
    end
    return nil, "its window " .. tostring(m.id) .. " of the window list is asked what is drawn there"
  end
  -- Drawn inside the plug-in: the check any click of this overlay gets.
  if refuseOutside(self, c, x, y) then return "refused" end
  return nil, "drawn inside the plug-in, checked like any click of the overlay"
end

-- Test `t` sees a menu and answered `value`: the item is chosen in it, if the answer says where
-- it is and the item lies inside; once the item is clicked, the answer says whether the menu took
-- it (step 5).
function Overlay:_pickSeen(t, value)
  local p = self._pick
  local c = p.c
  local m = menuRectOf(value)
  if p.clicked then return self:_pickFollow(t, m) end
  if not m then
    -- nativePopup and the accessibility tests say only "a menu is open": nowhere to place an
    -- item. Another test on the list may say where; this one's word is noted once.
    if p.noWhere == nil then
      p.noWhere = t.name
      host.log.info((PICK_LOG .. "— test '%s' sees a menu but does not say where it is; waiting for "
        .. "one that does"):format(tostring(self.label), tostring(c.label), t.name))
    end
    return
  end
  if not self.active or self:hwnd() ~= p.pinned then return self:_pickEnd("moved") end
  local where = ("%d,%d %dx%d"):format(m.x, m.y, m.w, m.h)
  local item = c.menuItem
  local x, y
  if type(item) == "function" then
    local ok, v, vwhy = pcall(item, self, m, p.k)
    if not ok then return self:_pickEnd("unplaced", "its menuItem function raised: " .. tostring(v)) end
    if v == false then
      -- Not this window: something else that appeared (a tooltip). The next answer is looked at.
      if p.other ~= where then
        p.other = where
        host.log.info((PICK_LOG .. "— test '%s' sees a window at %s, which its menuItem function "
          .. "says is not its menu%s; waiting for the next answer"):format(tostring(self.label),
          tostring(c.label), t.name, where, vwhy ~= nil and (" (" .. tostring(vwhy) .. ")") or ""))
      end
      return
    end
    if type(v) ~= "table" or type(v[1]) ~= "number" or type(v[2]) ~= "number" then
      return self:_pickEnd("unplaced", "its menuItem answered " .. tostring(v) .. ", not {dx, dy}")
    end
    -- Screen pixels from the menu's corner: not scaled again. Rounded once in a scaled overlay,
    -- the plain sum in one without, as every point there is.
    item = v
    if scaledFor(self, c) then
      x, y = math.floor(m.x + v[1] + 0.5), math.floor(m.y + v[2] + 0.5)
    else
      x, y = m.x + v[1], m.y + v[2]
    end
  elseif scaledFor(self, c) then
    -- An authored distance, at the factor the opener was placed with.
    if not p.k then return self:_pickEnd("unplaced", "no factor was known at the press") end
    x, y = math.floor(m.x + p.k * item[1] + 0.5), math.floor(m.y + p.k * item[2] + 0.5)
  else
    x, y = m.x + item[1], m.y + item[2]
  end
  if x < m.x or x >= m.x + m.w or y < m.y or y >= m.y + m.h then
    -- Not clicked, and not given up either: a later answer may be the menu itself where this one
    -- was something else that appeared (a tooltip under the pointer is a window too, on a Mac).
    local at = ("(%d,%d)"):format(x, y)
    if p.outside ~= at .. " of " .. where then
      p.outside = at .. " of " .. where
      host.log.info((PICK_LOG .. "— test '%s' sees a menu at %s, and item (%s,%s) lands at %s, "
        .. "outside it; not clicking"):format(tostring(self.label), tostring(c.label), t.name, where,
        tostring(item[1]), tostring(item[2]), at))
    end
    return
  end
  local covered, checked = self:_pickCovered(c, m, x, y)
  if covered == "refused" then return self:_pickEnd("refused", "refused, and said") end
  if covered then return self:_pickEnd("covered", covered) end
  -- In a calibrating run, the menu with a crosshair on the item, before the click takes it away.
  local writeShot = p.shoot and self:_pickShot(c, m, x, y) or nil
  host.log.info((PICK_LOG .. "— test '%s' sees the menu at %s%s (%s); choosing item (%s,%s) at "
    .. "(%d,%d)"):format(tostring(self.label), tostring(c.label), t.name, where,
    m.kind == "window" and (" (window id " .. tostring(m.id) .. ")") or "", checked,
    tostring(item[1]), tostring(item[2]), x, y))
  host.input.click(x, y)
  -- Written on a later pass: a PNG takes the event loop for hundreds of milliseconds in a debug
  -- build, and the click is what the user is waiting for.
  if writeShot then host.timer.after(0, writeShot) end
  if p.retries <= 0 then
    -- Not followed (`retries = 0`): chosen at its click.
    self._pick = nil
    self:_pickDone(p, true, nil)
    return
  end
  p.clicked = { x = x, y = y, menu = m, tick = menuTick, clicks = 1 }
end

-- An answer after the item's click, `m` its rectangle or nil (step 5). The same menu, on a
-- question asked on a tick after the last click, is a click the menu did not take: clicked again,
-- after the same checks, while the pick has retries left, and "untaken" after that. Another window
-- in its place is the item taken. An answer that does not say where says nothing either way.
function Overlay:_pickFollow(t, m)
  local p = self._pick
  local k = p.clicked
  if not m then return end
  if not sameMenu(m, k.menu) then
    return self:_pickEnd("taken", ("test '%s' sees another window in its place, at %d,%d %dx%d")
      :format(t.name, m.x, m.y, m.w, m.h))
  end
  -- Asked on the click's own tick or before it: the menu cannot have had the click yet.
  if (t.lastAsked or -1) <= k.tick then return end
  if k.clicks > p.retries then return self:_pickEnd("untaken") end
  if not self.active or self:hwnd() ~= p.pinned then return self:_pickEnd("moved") end
  local covered = self:_pickCovered(p.c, m, k.x, k.y)
  if covered == "refused" then return self:_pickEnd("refused", "refused, and said") end
  if covered then return self:_pickEnd("covered", covered) end
  k.clicks += 1
  k.tick = menuTick
  host.log.info((PICK_LOG .. "— test '%s' still sees the menu at %d,%d %dx%d a tick after the "
    .. "item's click, which it did not take: clicking (%d,%d) again, %d of %d"):format(
    tostring(self.label), tostring(p.c.label), t.name, m.x, m.y, m.w, m.h, k.x, k.y, k.clicks,
    p.retries + 1))
  host.input.click(k.x, k.y)
end

-- Opens a menu and chooses an item in it, for a module's own code — a stepper whose step is a
-- choice from the plug-in's list (Avenger's zoom). `spec` is what a hotspot with `menuItem` takes:
-- `label`, `at` (a table or a function of the overlay), `fromRight`, `rawOrigin`, `button`,
-- `menuItem`, `retries`, and `onDone(overlay, chosen, why)`. Returns whether the opener was
-- clicked. A spec that cannot work — no `at`, a `menuItem` that is neither {dx, dy} nor a
-- function, a `button` the click does not know, a `retries` that is not a whole number from 0 —
-- raises here, at the call, rather than after the menu has opened.
function Overlay:chooseMenuItem(spec)
  local who = tostring(self.label)
  if type(spec) ~= "table" or (type(spec.at) ~= "table" and type(spec.at) ~= "function")
    or spec.menuItem == nil then
    error(("overlay '%s': chooseMenuItem takes { label, at, menuItem, … }, with `at` {x, y} or a "
      .. "function(overlay)"):format(who), 2)
  end
  local mi = spec.menuItem
  if type(mi) ~= "function" and not (type(mi) == "table" and type(mi[1]) == "number"
    and type(mi[2]) == "number") then
    error(("overlay '%s': chooseMenuItem's `menuItem` must be {dx, dy} or a function(overlay, menu, "
      .. "factor), got %s"):format(who, type(mi)), 2)
  end
  local b = spec.button
  if b ~= nil and b ~= "left" and b ~= "right" and b ~= "middle" then
    error(("overlay '%s': chooseMenuItem's `button` must be \"left\", \"right\" or \"middle\", got %s")
      :format(who, tostring(b)), 2)
  end
  local r = spec.retries
  if r ~= nil and (type(r) ~= "number" or r < 0 or r % 1 ~= 0) then
    error(("overlay '%s': chooseMenuItem's `retries` must be a whole number from 0, got %s")
      :format(who, tostring(r)), 2)
  end
  local label = spec.label or "Menu item"
  local function fail(why)
    if spec.onDone then pcall(spec.onDone, self, false, why) end
    return false
  end
  if not self.active then
    host.log.info((PICK_LOG .. "not opened — the overlay is not in front"):format(who, label))
    return fail("inactive")
  end
  if not self._menu then
    host.log.info((PICK_LOG .. "not opened — the overlay lists no menu tests to see the menu with")
      :format(who, label))
    host.speech.output(label .. " is not available now", { interrupt = true })
    return fail("no tests")
  end
  -- One control per label, kept: the menu shots count a control's openings by the control, and
  -- a fresh table per call would be photographed at every one.
  self._pickControls = self._pickControls or {}
  local c = self._pickControls[label]
  if not c then
    c = { kind = "pick", label = label }
    self._pickControls[label] = c
  end
  if self:_pickRepeat(c) then return fail("busy") end
  c.at, c.fromRight, c.rawOrigin, c.button, c.menuItem =
    spec.at, spec.fromRight, spec.rawOrigin, spec.button, spec.menuItem
  local px, py, why = hotspotPoint(self, c)
  if not px then
    unplaced(self, c, why or "its `at` answered nil")
    return fail("unplaced")
  end
  if refuseOutside(self, c, px, py) then return fail("refused") end
  -- Placed and checked: now it is a press (step 1).
  self:_menuOpening(c)
  host.log.info((PICK_LOG .. "opening its menu, clicking (%d,%d)%s"):format(who,
    label, px, py, c.button and (" with the " .. tostring(c.button) .. " button") or ""))
  if c.button ~= nil then
    host.input.click(px, py, { button = c.button })
  else
    host.input.click(px, py)
  end
  self:_armPick(c, spec.onDone, r)
  return true
end

-- ── Holding its place ────────────────────────────────────────────────────────
-- A menu can be a WINDOW that comes to the front. Komplete Kontrol's own menu in REAPER is one
-- (2026-09-26): its hamburger opens a popup titled "Komplete Kontrol", a window of reaper.exe,
-- and it takes the front. The overlay's context — the plug-in's window in front, the keyboard in
-- the plug-in — then no longer matched, so the overlay left the front, and its tests, which run
-- only for an overlay in front, never saw the menu.
--
-- So a press of a control with `opensMenu` is recorded (see _menuOpening), and:
--   • ON ITS MENU. The window in front is not the press's, but belongs to the same process, and a
--     test sees a menu NOW — asked there and then, and its answer counts, not the word, which
--     still says "open" for a miss after a menu has closed: a dialog that an item of a native
--     menu opened is no menu of the overlay's, and holding it would hold another overlay's
--     window. The overlay stays in front, but holds NO KEYS AT ALL while that window is: no
--     navigation key, no hotkey, no say in the pass-through flag. The window's keys are its own,
--     whatever it leads to — a dialog opened from the menu may be another overlay's. Its tests
--     run on, and it keeps its claim on the arbiter's slot.
--   • Anything else ends the hold, and the ordinary match decides, exactly as for any other
--     window change: the plug-in with the keyboard again (its keys come back, see
--     _menuHoldEnds); another application (Alt+Tab); a window of the same one that no test sees
--     a menu in; the host's window with the keyboard on the host's own chrome, which the
--     embedded focus gate counts as the host's. ReaHotkey has no more than that: its plug-in
--     context is REAPER's FX window active with the plug-in's control found in it.
-- The press is forgotten on the overlay's next own key with no menu open, when it leaves the
-- front, when the plug-in has the keyboard again after a hold, and at the next press — so a
-- window that appears with no press behind it, or belongs to another process, is never held.
--
-- ONE STEP BEYOND ReaHotkey, so that the first key after Escape is not lost (the NVDA test and a
-- diagnostic run of 2026-09-27). Komplete Kontrol's menu is an invisible full-screen window of
-- reaper.exe; after Escape the user's first key was lost, and the overlay came back about four
-- seconds later. The likeliest reading of the log, not measured: KK only HIDES that window, and it
-- stayed the foreground window, with the keyboard in it — no foreground event came in those four
-- seconds, which a destroyed window would most likely have caused. host.window.active() cannot
-- say so: it reports a hidden window as none, and it answers from what was read in the current
-- epoch, which a tick does not turn over. host.window.foreground() can: the window that gets the
-- keyboard, shown or not, read afresh. So when a hold ends because no test sees the menu any more,
-- and the window that gets the keyboard is still the menu's own — the first window the overlay
-- held its place over after the press, of the press's process — and is not shown, the window that
-- was in front at the press is brought back, if it is still listed: once per press, with
-- host.window.focus, its answer logged and nothing announced. Then the ordinary match decides (see
-- _menuRefocus). When nothing is brought back, the reading is logged instead, once per press.
--
-- No timer decides any of it: the window in front, the press, the tests' answers and the window
-- that gets the keyboard do. Nothing in front — the moment between a popup closing and its owner
-- coming back, or on a Mac an application with no window — keeps a hold over a menu the tests
-- still see and ends any other.
--
-- A process test rather than an owner test: the window table carries no owner, and a plug-in's
-- popups are windows of the process that runs it, on both platforms. A plug-in bridged into
-- another process whose own window is not the one in front at the press — its window reparented
-- into the host's — is the case this does not hold: its popup belongs to the bridge, and the
-- overlay leaves the front for it as it did before.

-- The window in front, for a log line.
local function frontName(a)
  if not a then return "nothing" end
  return ("'%s' (%s)"):format(tostring(a.title), tostring(a.app and a.app.exe or "?"))
end

-- Not held: a hold there was ends here, and says so. Returns nil.
function Overlay:_menuNotHeld(a)
  local st = self._menu
  if st.hold and self.active then
    host.log.info(("[menu] '%s': no longer holding its place — in front now: %s")
      :format(tostring(self.label), frontName(a)))
  end
  st.hold = nil
  return nil
end

-- Called by _recheck when the context of an overlay in front does not match. Returns "menu" — it
-- holds its place, and self.activeCtx is set to what its origin resolves through meanwhile —
-- "refocused" — the hold ended and the window of the press was brought back (see _menuRefocus),
-- so the ordinary match is to be asked again — or nil: it is not held.
--
-- `a` can be the answer read earlier in this epoch (see host.window.active), so a window that
-- hid since then without an event can still be `a`; the step after the hold does not go by it.
function Overlay:_menuHold()
  local st = self._menu
  local h = st.hold
  local a = host.window.active()
  if not a then
    -- Nothing in front. The moment between a popup closing and its owner coming back: a hold
    -- over a menu the tests still see stands until a window is in front. On a Mac it is also an
    -- application with no window (Command+Tab to the Finder), so nothing else is held through
    -- it; and once the tests stop seeing the menu, the tick decides again (menuTickFn). On
    -- Windows it is also a hidden foreground window — most likely the menu's own, after Escape.
    if h and st.open then
      self.activeCtx = h.ctx
      return "menu"
    end
    -- No test sees the menu any more (the word has closed).
    if h and self:_menuRefocus() then return "refocused" end
    return self:_menuNotHeld(a)
  end
  local pid = a.app and a.app.pid
  local p = st.press
  if p and a.id ~= p.window and pid ~= nil and pid == p.pid then
    -- The origin as it was at the press: the plug-in is still there, behind its menu, and a
    -- test that reads it (o:origin()) is asked about the plug-in, not about the menu's window.
    local was = p.origin
    local ctx = {
      base = p.base,
      match = function() return true end,
      origin = function() return was end,
    }
    self.activeCtx = ctx
    -- A test must see a menu NOW. The tests are asked there and then — so the overlay lets its
    -- keys go in the same breath as it keeps its place, not a tick later — unless this is the
    -- tick deciding again, which has just asked them, and none saw one.
    local before = st.sightings or 0
    if not st.askedThisTick then self:_menuRound(host.now()) end
    if (st.sightings or 0) > before and st.open and self.active then
      if not h then
        host.log.info(("[menu] '%s': %s came to the front after '%s' was pressed and a test "
          .. "sees a menu — the overlay holds its place while it is up, holding no keys"):format(
          tostring(self.label), frontName(a), tostring(st.pressed or "a control")))
        -- NO KEYS AT ALL while that window is in front: not the navigation keys, not a hotkey,
        -- and no say in the pass-through flag. They are the menu's — and a window the menu
        -- leads to can be another overlay's (Komplete Kontrol's Preferences, opened from its
        -- menu, has one): an overlay still holding Tab and Return behind it would take them
        -- wherever its scope reaches, because the host hands a captured key to the earliest
        -- capture whose module's scope is the window in front.
        self:_unregisterHotkeys()
        self._hotkeysSuspended = true
        host.keys.menuOpen(false)
        -- The menu's own window: the one the hold starts over, the first for this press (a hold
        -- that ends takes the press with it). Only it can have the step after the hold (see
        -- _menuRefocus) — not a dialog the menu leads to, which the hold moves on to below.
        p.menuWindow, p.menuFront = a.id, a
      end
      st.hold = { ctx = ctx }
      return "menu"
    end
    self.activeCtx = false
    -- No test sees the menu any more. On a recheck that asked the tests itself this can be their
    -- first miss, the word still saying open (see _menuRefocus).
    if h and self:_menuRefocus() then return "refocused" end
  end
  return self:_menuNotHeld(a)
end

-- Where host.window.foreground() says the keyboard goes, for a log line: `read` and `f` are what
-- pcall returned.
local function keyboardName(read, f)
  if not read then return ("a window that could not be read (%s)"):format(tostring(f)) end
  if type(f) ~= "table" then return "no window the platform names" end
  return ("id=%s of pid %s, %s"):format(tostring(f.id), tostring(f.pid),
    f.shown == false and "not shown" or "shown")
end

-- A hold ends because no test sees the menu any more. When the window that gets the keyboard
-- (host.window.foreground, read afresh — never host.window.active, which drops a hidden window
-- and may answer from before it hid) is still the menu's own window — the first one held over
-- after this press, of the press's process — and it is not shown, the keyboard is in a window
-- nobody can see, and the window that was in front at the press is brought back with
-- host.window.focus: once per press, whatever the answer, and only if that window is still
-- listed. Nothing is said: accepted is not the keyboard in the plug-in, and whether the overlay
-- is back is the ordinary match's to decide, which _recheck asks again at once. Returns whether
-- the platform accepted.
--
-- Not when foreground() names another window, one that is shown, nothing, or cannot be read (it
-- raises, or on a Mac the application is not answering: nil). Only past those is anything else
-- asked, so wherever the window that gets the keyboard is shown — on a Mac, any focused window that
-- is not minimised — the step costs one reading and no more. That reading is logged, once per
-- press, when nothing is brought back: the `in front now:` line after it names what
-- host.window.active() answers, which can be kept from earlier in the epoch, so this line is the
-- one that says where the keyboard was.
--
-- Reached from _menuHold whenever a hold ends with no test seeing the menu: on the tick that
-- closes the menu, or — a recheck that asks the tests itself, such as a pollMatch poll reading a
-- window active() kept from before the menu hid — at the first miss, while the word still says
-- open. foreground()'s `shown = false` is what keeps a single missed reading of a menu that is
-- still up from bringing anything back.
function Overlay:_menuRefocus()
  local st = self._menu
  local p = st.press
  if not p or p.refocused or p.menuWindow == nil then return false end
  local read, f = pcall(host.window.foreground)
  -- Once per press, brought back or not: the hold ends here either way, and the press with it.
  p.refocused = true
  if not (read and type(f) == "table" and f.id == p.menuWindow and f.pid == p.pid
    and f.shown == false) then
    host.log.info(("[menu] '%s': no test sees the menu any more; the keyboard goes to %s, and the "
      .. "menu's window %s is id=%s of pid %s — nothing is brought back"):format(
      tostring(self.label), keyboardName(read, f), frontName(p.menuFront), tostring(p.menuWindow),
      tostring(p.pid)))
    return false
  end
  local why = ("[menu] '%s': no test sees the menu any more, but its window %s still gets the "
    .. "keyboard and is not shown"):format(tostring(self.label), frontName(p.menuFront))
  -- Windows reuses a window's handle once the window is gone: the window of the press must still
  -- be listed for its process, with its class, before it is brought forward. A menu item can close
  -- it or unload the plug-in. host.window.list lists titled windows only, so an untitled window of
  -- the press is never brought back.
  local listed, list = pcall(host.window.list, { pids = { p.pid } })
  local there = false
  if listed and type(list) == "table" then
    for _, w in ipairs(list) do
      if w.id == p.window and w.class == p.front.class then
        there = true
        break
      end
    end
  end
  if not listed then
    host.log.info(("%s — the window of the press, %s, could not be looked for (%s), so nothing "
      .. "is brought back"):format(why, frontName(p.front), tostring(list)))
    return false
  end
  if not there then
    host.log.info(("%s — the window of the press, %s, is not listed for its process with its "
      .. "class (gone, untitled, or its id now another window's), so nothing is brought back")
      :format(why, frontName(p.front)))
    return false
  end
  local ok, answer = pcall(host.window.focus, p.window)
  local accepted = ok and answer == true
  host.log.info(("%s — bringing back %s, where '%s' was pressed: %s"):format(why,
    frontName(p.front), tostring(p.control),
    accepted and "accepted" or (ok and "declined" or ("declined (" .. tostring(answer) .. ")"))))
  return accepted
end

-- The hold over its menu's window ended with the plug-in having the keyboard again: its keys
-- again, scoped to its window, which another overlay may have scoped them away from meanwhile.
-- The tests forget the press — the menu it opened is over — and are asked at once, so they start
-- counting it closed. The per-control hotkeys come back given up, and applyMenuState takes them
-- back when the menu counts as closed — now, or on a later tick — with the log line that says so.
function Overlay:_menuKeysBack()
  local st = self._menu
  st.press = nil
  menuHook(self, "forget")
  -- The menu's window is over and the plug-in has the keyboard: an item still waiting was never
  -- chosen in it.
  if self._pick then self:_pickEnd("closed") end
  -- Pinned before its keys are taken, as in _activate.
  host.keys.scope(true)
  self:_registerHotkeys(true)
  applyMenuState()
  self:_menuRound(host.now())
end

-- The overlay's context matches again — the plug-in has the keyboard: the hold is over, and the
-- overlay's keys come back. A plug-in that takes the keyboard back itself, or the window of the
-- press brought back with the keyboard in the plug-in.
function Overlay:_menuHoldEnds()
  local st = self._menu
  if not st.hold then return end
  st.hold = nil
  if not self.active then return end
  host.log.info(("[menu] '%s': the plug-in has the keyboard again"):format(tostring(self.label)))
  self:_menuKeysBack()
end

-- ── Calibration ──────────────────────────────────────────────────────────────
-- Armed only while calibration is switched on (O.calibrating, this runtime's own setting), at
-- the next activation, and captured (not registered as global hotkeys) so the keys belong to
-- whichever overlay is ACTIVE — which is also the only overlay that can answer where its
-- controls land, since the coordinate frame (origin, frame offset, rawOrigin, landmark
-- anchoring) is its own. A separate tool would have to reimplement that resolution, and would
-- then be measuring something other than what the overlay clicks.
--
--   Ctrl+Alt+Shift+S  a screenshot of the coordinate window with a CROSSHAIR on every
--                     control where it will actually click, plus the pixel read there
--   Ctrl+Alt+Shift+T  crop a template around the focused control, straight into the module
--   Ctrl+Alt+Shift+V  count every match of the focused control's template in the region
--
-- Command+Option+Shift on a Mac, where the spec's Ctrl is Command — off Control+Option, which
-- is VoiceOver's.
--
-- And with no key at all: activating a control with `opensMenu` takes the three menu shots,
-- which delay the control by one screen capture and nothing more (see _menuShots below).
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
  -- Same reasoning as the slider: a graphical button's region is the area it is HUNTED in, so
  -- the region centre is a point the control has nothing to do with. Mark where the template
  -- actually matched, and fall through to the centre only when it cannot be found — which is
  -- itself the useful thing to see in the picture.
  if c.kind == "gbutton" then
    local bx, by = graphicalButtonPoint(self, c)
    if bx then return bx, by end
  end
  if c.region then
    -- nil when the region cannot be placed now: listed as "no position", with no crosshair.
    return ocrCenter(self, c)
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
local function shotName(base, k)
  return base .. (k > 1 and ("-" .. k) or "")
end

local shotsTaken = {}
-- `suffix` is what follows the number in the file the existence check looks for: nothing for
-- the calibration shot (`<overlay>.png`, `<overlay>-2.png`), "-menu-before" for the menu shots
-- (`<overlay>-<control>-menu-before.png`, `<overlay>-<control>-2-menu-before.png`).
local function shotPath(label, suffix)
  suffix = suffix or ""
  local base = safeName(label)
  local key = base .. suffix
  local n = shotsTaken[key]
  -- Seeded from what is already on disk, not merely counted per run. A counter that lives
  -- only in memory resets on restart, and restarting between two shots of the same overlay
  -- is not the exception while a module is being written — it is the loop. That is exactly
  -- how the first Diva shot got overwritten by the second: the very mistake this numbering
  -- was added to prevent, defeated by the workflow it was meant to serve.
  if n == nil then
    n = 0
    while host.resource.exists("calibration/" .. shotName(base, n + 1) .. suffix .. ".png") do
      n = n + 1
    end
  end
  n = n + 1
  shotsTaken[key] = n
  return shotName(base, n)
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
  -- WHO HAS THE KEYBOARD, which the roster above cannot tell you.
  --
  -- Every control here is driven by a synthetic mouse click, so an overlay can be entirely
  -- correct — right origin, right coordinates, right pixels — while the plug-in never
  -- receives a single keystroke. That distinction is invisible until the moment the
  -- plug-in's OWN keys are what matter: u-he's preset browser is stepped with the arrow
  -- keys, which this runtime deliberately does not capture, and whether they arrive depends
  -- entirely on whether the plug-in window (rather than the host's dialog) holds the focus.
  -- Printed innermost-first, as GetGUIThreadInfo reports it, with the overlay's own origin
  -- marked — so "the plug-in is not on the focus chain at all" is readable at a glance.
  local chain = host.window.focusChain()
  if #chain == 0 then
    lines[#lines + 1] = "  keyboard focus: none reported"
  else
    local parts, onChain = {}, false
    for _, c in ipairs(chain) do
      local mine = origin ~= nil and c.id == origin.id
      if mine then onChain = true end
      parts[#parts + 1] = string.format("%s[%s] id=%d", mine and "->" or "", tostring(c.class), c.id)
    end
    lines[#lines + 1] = "  keyboard focus (innermost first): " .. table.concat(parts, "  <  ")
    lines[#lines + 1] = "    the overlay's origin is " ..
      (onChain and "ON the focus chain — keys the overlay does not capture reach the plug-in"
        or "NOT on the focus chain — keys go to the host, not to the plug-in")
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
  for _, e in ipairs(host.element.rawDump(origin.id) or {}) do
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

-- Where every VISIBLE control will act, as crosshair marks, and each control's own entry:
-- `points[i]` is `{ x, y, n }` (n = its mark's number) for a visible control with a position,
-- `false` for a visible one without, nil for a hidden one. No pixel is read here, but a
-- control's `when` and its point can cost a lookup each (an image search for a slider's thumb),
-- which is why the menu shots, taken before a click, do not size their picture with it.
local function calibrationMarks(self)
  local marks, points = {}, {}
  -- Which controls are shown, in one pass over their `when`s (see "Where a hook runs").
  local shown = scanPass(self, function()
    local v = {}
    for i, c in ipairs(self.controls) do v[i] = isVisible(self, c) end
    return v
  end)
  for i, c in ipairs(self.controls) do
    -- A control that is currently HIDDEN gets no crosshair: drawing one would say the user
    -- can reach something they cannot, which is the opposite of what this picture is for.
    if shown[i] then
      local x, y = calibrationPoint(self, c)
      if x then
        marks[#marks + 1] = { x = x, y = y }
        points[i] = { x = x, y = y, n = #marks }
      else
        points[i] = false
      end
    end
  end
  -- The openers O:chooseMenuItem has clicked for a module's own code (a stepper over a zoom
  -- list): points no control's `at` names, and the module clicks them all the same. Each is known
  -- once it has been used, at the `at` it was last given; by label, so the numbering is stable.
  local openers = {}
  local labels = {}
  for label in pairs(self._pickControls or {}) do labels[#labels + 1] = label end
  table.sort(labels)
  for _, label in ipairs(labels) do
    local x, y = hotspotPoint(self, self._pickControls[label])
    if x then
      marks[#marks + 1] = { x = x, y = y }
      openers[#openers + 1] = { label = label, x = x, y = y, n = #marks }
    else
      openers[#openers + 1] = { label = label }
    end
  end
  return marks, points, openers
end

-- The region a calibration picture of origin `o` covers: its CONTENT rectangle, grown to take
-- in every mark, clamped to the window.
local function calibrationRegion(o, marks)
  -- The CONTENT rectangle, which is the one the controls are measured against — using the
  -- window rect instead made a standalone shot overshoot by its border and include a strip
  -- of whatever was behind it, so a picture meant to settle where a coordinate lands
  -- disagreed with the coordinate by 16 px.
  local ow, oh = O.contentSize(o)
  local region = { o.client.x, o.client.y, o.client.x + ow, o.client.y + oh }
  -- …but GROWN to cover any control that resolves outside it, because that is precisely the
  -- control whose picture you need. Komplete Kontrol's standalone menu bar sits 10 px above
  -- the client top, in the window's non-client band, so the shot meant to prove where those
  -- five points land cut every one of them off — and produced a picture with no crosshairs
  -- at all, which reads as "nothing was marked" rather than "the marks are off-frame".
  -- Clamped to the window's own rectangle, so a genuinely stray coordinate still cannot pull
  -- another application's pixels into the evidence.
  local pad = 12
  for _, m in ipairs(marks) do
    region[1] = math.max(o.bounds.x, math.min(region[1], m.x - pad))
    region[2] = math.max(o.bounds.y, math.min(region[2], m.y - pad))
    region[3] = math.min(o.bounds.x + o.bounds.w, math.max(region[3], m.x + pad))
    region[4] = math.min(o.bounds.y + o.bounds.h, math.max(region[4], m.y + pad))
  end
  return region
end

-- What a control's line in a calibration shot adds. In a SCALED overlay, where the authored
-- numbers are not where anything lands, every region it reads, as the screen rectangle; an
-- unscaled overlay's regions are its source's numbers plus the origin printed above them, and its
-- lines are what they always were. And for a control that chooses a menu item, the item — placed
-- only once its menu is seen, so given as authored; its crosshair is in the menu item shot.
function Overlay:_calibrationPlaces(c)
  local out = {}
  if self._scale then
    for _, field in ipairs({ "region", "ocrLabel" }) do
      if c[field] ~= nil then
        local r, why = regionOf(self, c, field)
        out[#out + 1] = r and string.format("  %s (%d,%d)-(%d,%d)", field, r[1], r[2], r[3], r[4])
          or ("  " .. field .. " not placed (" .. tostring(why) .. ")")
      end
    end
  end
  if c.menuItem ~= nil then
    out[#out + 1] = type(c.menuItem) == "table"
      and string.format("  then item (%s,%s) of its menu", tostring(c.menuItem[1]), tostring(c.menuItem[2]))
      or "  then an item of its menu"
  end
  return table.concat(out)
end

function Overlay:calibrationShot()
  local o = self:origin()
  if not (o and o.client and o.bounds) then
    host.log.info("[calibrate] no active origin")
    return false
  end
  local marks, points, openers = calibrationMarks(self)
  -- The pixel under every crosshair, the controls' and the openers', in one read of the screen:
  -- one capture where they lie close together, where a read per point took one each. Cut toward
  -- zero, as a single pixel's read cuts a point. All or nothing: a read that fails leaves every
  -- pixel "?", and says why first.
  local function cut(v) return v < 0 and math.ceil(v) or math.floor(v) end
  local asked, slot = {}, {}
  for i in ipairs(self.controls) do
    local p = points[i]
    if p then
      asked[#asked + 1] = { cut(p.x), cut(p.y) }
      slot[p] = #asked
    end
  end
  for _, op in ipairs(openers) do
    if op.n then
      asked[#asked + 1] = { cut(op.x), cut(op.y) }
      slot[op] = #asked
    end
  end
  local read, unread = host.screen.pixels(asked)
  local function pixelAt(p)
    local px = read and read[slot[p]]
    return px and string.format("%d,%d,%d", px.r, px.g, px.b) or "?"
  end
  local lines = {}
  if not read then lines[1] = "  the pixels could not be read: " .. tostring(unread) end
  for i, c in ipairs(self.controls) do
    local p = points[i]
    -- A hidden control is still listed, marked, so its absence is visible rather than
    -- mysterious.
    if p == nil then
      lines[#lines + 1] = string.format("  -- %-24s hidden (its `when` is false)", tostring(c.label))
    elseif p then
      lines[#lines + 1] = string.format(
        "  %2d %-24s screen (%d,%d)  pixel %s%s%s",
        p.n, tostring(c.label), p.x, p.y,
        pixelAt(p),
        c.rawOrigin and "  [rawOrigin]" or "",
        self:_calibrationPlaces(c)
      )
    else
      lines[#lines + 1] = string.format("  -- %-24s no position%s", tostring(c.label),
        self:_calibrationPlaces(c))
    end
  end
  for _, op in ipairs(openers) do
    if op.n then
      lines[#lines + 1] = string.format("  %2d %-24s screen (%d,%d)  pixel %s  [menu opener]",
        op.n, op.label, op.x, op.y, pixelAt(op))
    else
      lines[#lines + 1] = string.format("  -- %-24s no position  [menu opener]", op.label)
    end
  end
  -- Written under the OVERLAY RUNTIME's own directory, not the module whose overlay this
  -- is: host.path resolves against the module that DEFINES the running code, which here is
  -- the runtime. That rule is what makes an inherited overlay find its own images, so it
  -- stays -- calibration output just gets one known home instead of a per-module one. The
  -- log line prints the absolute path, and the file is named after the overlay.
  local region = calibrationRegion(o, marks)
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
  -- The captured region, not the window size: the two differ (content vs frame, and the
  -- growth above), and printing the one that is NOT in the picture is how half an hour goes
  -- into wondering why a coordinate inside "1044x633" was off the edge of the image.
  -- A scaled overlay says the factor its crosshairs were placed with, and the frame: the two
  -- numbers a tester's picture is checked against at another zoom.
  local scaleNote = ""
  if self._scale then
    local k, why = factorNow(self)
    local fx, fy = 0, 0
    if self._originOffset then fx, fy = self._originOffset(o) end
    scaleNote = k and string.format("  scale %.4f about (%s,%s), frame (%s,%s)", k,
      tostring(self._scale.about[1]), tostring(self._scale.about[2]), tostring(fx), tostring(fy))
      or ("  no scale now (" .. tostring(why) .. ")")
  end
  host.log.info(string.format(
    "[calibrate] %s — origin (%d,%d) captured (%d,%d)-(%d,%d) %dx%d%s%s -> %s (%s)",
    tostring(self.label), o.client.x, o.client.y,
    region[1], region[2], region[3], region[4], region[3] - region[1], region[4] - region[2],
    self._landmarkHit
      and string.format("  landmark at (%d,%d)", self._landmarkHit.x, self._landmarkHit.y)
      or "",
    scaleNote,
    path, tostring(ok)
  ))
  for _, l in ipairs(calibrationContext(o)) do host.log.info("[calibrate]" .. l) end
  for _, l in ipairs(calibrationUia(o)) do host.log.info("[calibrate]" .. l) end
  for _, l in ipairs(lines) do host.log.info("[calibrate]" .. l) end
  return ok == true
end

-- The MENU SHOTS, in a calibrating run: a control with `opensMenu` was activated, so its menu
-- is about to appear over the plug-in. Three pictures of the origin's whole rectangle — the
-- plug-in's control, or the window with its frame — just before the control acts and about
-- 600 ms and 1500 ms after:
--
--   calibration/<overlay>-<control>-menu-before.png
--   calibration/<overlay>-<control>-menu-after-600.png
--   calibration/<overlay>-<control>-menu-after-1500.png
--
-- numbered like the other shots (`<overlay>-<control>-2-menu-before.png` for the next press),
-- so nothing is overwritten. They are what a module's own menu test is written from: a menu
-- the plug-in paints inside its own window has no window, no element and no notification, and
-- the pixels are the only thing that changes. The rectangle is taken once, before the click,
-- so the three pictures line up pixel for pixel; the part of a popup that reaches past it is
-- not in them.
--
-- NOTHING BUT ONE CAPTURE STANDS BEFORE THE CLICK. The first version wrote the "before" PNG on
-- the press and worked out every visible control's point to size the picture, and in a debug
-- build that put about 700 ms between the key and the click — Diva's preset menu felt slow
-- (2026-09-26). Now the press takes a snapshot of a rectangle the origin already carries and
-- nothing else. The later two are taken off the event loop at their time (snapshotAsync with
-- `at`) and written when they arrive; the "before" one is written last, once both are in, and
-- its snapshot kept until then. Not on the next pass after the click: that is when the menu's
-- first answer, its window coming to the front and the user's first key arrive, and a PNG takes
-- the event loop for hundreds of milliseconds in a debug build. So a key pressed while the menu
-- opens waits for no capture and no PNG; one pressed at 600 or 1500 ms waits for one or two
-- PNGs. The timing is a measuring instrument, not menu state: nothing reads these pictures but
-- the person calibrating.
--
-- TWICE PER CONTROL PER SESSION. The question the pictures answer — what this menu looks like
-- open — does not change on the tenth opening, and a session of days calibrating wrote three
-- PNGs of a plug-in window at every one: 85 MB in `calibration/` in two days. So the first two
-- openings of each control (per overlay, per VM) are photographed, and the next says once that
-- the rest are not. A reload of the module starts the count again.
--
-- Counted per overlay and control OBJECT, not per label: Kontakt builds an overlay per version
-- and per library from the same names, and two overlays that share a label would otherwise
-- share one count of two. Keys held weakly, so the count never keeps an overlay alive.
local MENU_SHOTS_PER_CONTROL = 2
local menuShotsTaken = setmetatable({}, { __mode = "k" })

-- Takes the menu shots of one press, as above. Returns true when this press is photographed, so
-- that the item a control chooses in its menu on this press is photographed too (_pickShot).
function Overlay:_menuShots(c)
  local who, what = tostring(self.label), tostring(c and c.label or "control")
  local o = self:origin()
  if not (o and o.client and o.bounds) then
    host.log.info(("[calibrate] '%s' '%s': no active origin, so no menu shots"):format(who, what))
    return false
  end
  local mine = menuShotsTaken[self]
  if not mine then
    mine = setmetatable({}, { __mode = "k" })
    menuShotsTaken[self] = mine
  end
  -- A press with no control known is counted under the overlay itself.
  local key = c or self
  local taken = mine[key] or 0
  mine[key] = taken + 1
  if taken >= MENU_SHOTS_PER_CONTROL then
    if taken == MENU_SHOTS_PER_CONTROL then
      host.log.info(("[calibrate] '%s' '%s': menu shots were taken of its first %d openings; later "
        .. "ones are not photographed until the module is reloaded"):format(who, what, MENU_SHOTS_PER_CONTROL))
    end
    return
  end
  local b = o.bounds
  local region = { b.x, b.y, b.x + b.w, b.y + b.h }
  local stem = shotPath(who .. "-" .. what, "-menu-before")
  -- Writes one picture, and lets its pixels go. A shot that fails says why and stops nothing.
  local function write(suffix, when, snap, why)
    local path = host.path("calibration/" .. stem .. suffix .. ".png")
    local ok = false
    if snap then
      local fine, saved, reason = pcall(host.screen.save, path, { snapshot = snap })
      ok, why = fine and saved == true, fine and reason or saved
      pcall(snap.release, snap)
    end
    host.log.info(("[calibrate] '%s' '%s': menu shot %s, (%d,%d)-(%d,%d), content at (%d,%d) "
      .. "-> %s (%s)"):format(who, what, when, region[1], region[2], region[3], region[4],
      o.client.x, o.client.y, path,
      ok and "true" or ("false: " .. tostring(why or "no reason given"))))
  end
  local pressed = host.now()
  local fine, snap, why = pcall(host.screen.snapshot, { region = region })
  if not fine then snap, why = nil, snap end
  -- The "before" picture once the later two are in (see above).
  local waiting = 2
  local function oneIn()
    waiting -= 1
    if waiting == 0 then write("-menu-before", "just before the control acts", snap, why) end
  end
  for _, ms in ipairs({ 600, 1500 }) do
    local suffix, when = "-menu-after-" .. ms, ms .. " ms after"
    local asked, err = pcall(host.screen.snapshotAsync, { region = region, at = pressed + ms },
      function(later, reason)
        write(suffix, when, later, reason)
        oneIn()
      end)
    if not asked then
      write(suffix, when, nil, err)
      -- Counted on a later pass, not now: now would write the "before" picture ahead of the click.
      host.timer.after(0, oneIn)
    end
  end
  return true
end

-- The MENU ITEM SHOT, in a calibrating run: a control that chooses a menu item has just been
-- handed the menu by a test, and is about to click. One picture of the menu — its rectangle as the
-- test gave it, 12 pixels round — with a crosshair on the point about to be clicked:
--
--   calibration/<overlay>-<control>-menu-item.png
--
-- numbered like the menu shots, and taken on the presses those are taken on (the first two
-- openings of each control). It is the only picture that can show where an item lands: the menu
-- shots' later two come after the click has closed the menu, and the calibration shot is of the
-- plug-in, with no menu open. What a tester sends back so an item offset can be measured.
--
-- The capture is made here, before the click — one snapshot — and the PNG is written by the
-- function returned, which the caller runs on a later pass, after the click.
function Overlay:_pickShot(c, m, x, y)
  local who, what = tostring(self.label), tostring(c and c.label or "control")
  local pad = 12
  local region = { m.x - pad, m.y - pad, m.x + m.w + pad, m.y + m.h + pad }
  local fine, snap, why = pcall(host.screen.snapshot, { region = region })
  if not fine then snap, why = nil, snap end
  local stem = shotPath(who .. "-" .. what, "-menu-item")
  return function()
    local path = host.path("calibration/" .. stem .. "-menu-item.png")
    local ok = false
    if snap then
      local okc, saved, reason = pcall(host.screen.saveMarked, path,
        { snapshot = snap, marks = { { x = x, y = y } } })
      ok, why = okc and saved == true, okc and reason or saved
      pcall(snap.release, snap)
    end
    host.log.info(("[calibrate] '%s' '%s': menu item shot, (%d,%d)-(%d,%d), the item marked at "
      .. "(%d,%d) -> %s (%s)"):format(who, what, region[1], region[2], region[3], region[4], x, y,
      path, ok and "true" or ("false: " .. tostring(why or "no reason given"))))
  end
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
      local region, why = regionOf(self, c)
      if not region then
        host.log.info("captureControl '" .. tostring(label) .. "': no region now (" .. tostring(why) .. ")")
        return false
      end
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
  local r, why = placeRect(self, nil, region)
  if not r then
    host.log.info("captureRegion: cannot be placed now (" .. tostring(why) .. ")")
    return false
  end
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
-- (`content-full.png`, where pixel (x,y) == authored content coordinate (x,y) — in an overlay
-- without O:scale; a scaled one's picture is the authored rectangle placed through the scale, so
-- its pixels are authored coordinates times the factor) plus a small box around every positioned
-- control, into `dir` (an absolute path), so re-derived coordinates can be verified against the
-- live plugin in one shot. Skips rawOrigin (chrome-frame) controls. Active only.
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
      elseif type(c.region) == "table" then
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
