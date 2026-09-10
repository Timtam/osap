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

-- How long after a control that OPENS a menu fires the menu watch keeps looking for a
-- self-drawn one at full rate. Declared here rather than beside the watch because
-- _menuOpening (far above it) is what sets the deadline.
local MENU_PLAUSIBLE_MS = 8000
-- How long a control's own claim that it opened a menu is believed without confirmation.
--
-- **Ended early wherever detection works**, so this number only ever governs a plugin whose
-- menu nothing can see. On Windows the detector confirms within a tick or two and its word
-- that the menu has gone is what closes the hold; there this is a floor for the moment a menu
-- takes to appear, and nothing more.
--
-- It was 2500 ms, and that was a safety-net number being asked to do a mechanism's job. The
-- macOS tester's log settles it: every single menu he opened logged "a control opened a menu
-- and no detector ever saw one" — sforzando's popup is not an `AXMenu`, so nothing on that
-- platform can see it, and the stopwatch IS the mechanism there. He reported the consequence
-- without recognising it as one: the pitchbend menu "doesn't track when I use arrows and
-- Enter anymore", while polyphony's still did. Not two menus behaving differently — two
-- durations. Under 2.5 seconds his Enter reached the menu; over it, the overlay had taken
-- Enter back and only VoiceOver's own VO+Space still committed. A longer list read out by ear
-- takes longer, which is why the longer menu was the one that broke.
--
-- 8000 ms is a guess, and deliberately the same guess as `MENU_PLAUSIBLE_MS` above: both are
-- answering "how long is a control's claim worth believing", and two different numbers for
-- one question would be two things to maintain. What it costs when wrong is bounded and
-- recoverable — a menu cancelled at once leaves the overlay's per-control keys quiet for up
-- to eight seconds. What the old number cost was not: a choice that silently never committed,
-- with nothing to tell the user why.
local MENU_HOLD_MS = 8000
local menuPlausibleUntil = 0
-- How long after Return or Escape went through to a menu that nothing can see the hold
-- keeps the keys given up. The menu closes on that key; the grace is for the plugin to take
-- it down before Tab and Return are claimed again. Only applies where no detector has
-- confirmed the menu — see watchMenus — so where one can see it, its word governs instead.
local MENU_CLOSE_GRACE_MS = 300

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
  -- does not bury the log it is trying to explain itself in.
  local ok, v = pcall(c.when, self)
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
  -- overlay cannot tell — and the moment a menu is up the overlay must stop owning keys, so
  -- that Alt+P and the rest reach the menu instead of re-firing the control (see
  -- Overlay:_menuOpening). The menu watch would notice within 150 ms anyway; this closes
  -- that window at the only moment it is open.
  return add(self, {
    kind = "hotspot",
    label = opts.label,
    ocrLabel = opts.ocrLabel,
    text = opts.text,
    at = opts.at,
    points = opts.points,
    settle = opts.settle,
    hotkey = opts.hotkey,
    opensMenu = opts.opensMenu,
    rawOrigin = opts.rawOrigin,
    fromRight = opts.fromRight,
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
    hotkey = opts.hotkey,
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
    hotkey = opts.hotkey, opensMenu = opts.opensMenu, when = opts.when,
  })
end

function Overlay:addOCRButton(opts)
  -- `readOnly` makes the control a pure announcer: focusing/activating re-reads the
  -- region via OCR but does NOT click it. For value/label readouts sitting over a
  -- drag knob or near a destructive control, where a click would change state.
  -- It is also announced without the word "button" (see speakControl) — a control that
  -- cannot be pressed must not describe itself as one.
  return add(self, {
    kind = "ocr", label = opts.label, region = opts.region, hotkey = opts.hotkey,
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
    hotkey = opts.hotkey,
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
    hotkey = opts.hotkey,
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
    hotkey = opts.hotkey,
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
  -- cannot judge must not block what it cannot judge, which is also why macOS, where this is
  -- not implemented yet, behaves exactly as it did before.
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

-- The rectangle a graphical button hunts in, in SCREEN coordinates.
--
-- `region` is optional, and omitting it means THE WHOLE ORIGIN — which is the right default
-- far more often than a written-out rectangle. A found control is used precisely when its
-- position is not fixed, and a static region then has to guess how far the thing can wander:
-- Kontakt's resize grip escaped its region on the second press, because dragging it made the
-- window taller and moved the grip past the lower bound that had looked generous. The origin
-- grows with the window, so it cannot go stale that way.
local function searchRect(self, c)
  local ox, oy = originOf(self, c)
  if c.region then
    return { ox + c.region[1], oy + c.region[2], ox + c.region[3], oy + c.region[4] }
  end
  local o = self:origin()
  if not o then return { ox, oy, ox, oy } end
  local w, h = O.contentSize(o)
  return { ox, oy, ox + w, oy + h }
end

-- Where a graphical button's template currently is, as a click point, or nil.
local function graphicalButtonPoint(self, c)
  if not c.image then return nil end
  local _, hit = host.screen.imageSearchMulti(asList(c.image), {
    region = searchRect(self, c),
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
-- world: the continuation re-checks that the overlay is still active on the SAME window,
-- exactly as the hotspot `points` sequence does, so a reveal started in one plug-in cannot
-- deliver its answer into another.
--
-- Fails OPEN. If the probe cannot be read at all we carry on rather than refuse: a control
-- that silently stops responding is worse than one that acts on a panel that was already
-- open, and clicking the probe when it is open is what ReaHotkey's own colour test avoids.
local function ensureRevealed(self, c, done)
  local r = c.reveal
  if not r or not r.at or not r.closedColor then return done() end
  local ox, oy = originOf(self, c)
  local px, py = ox + r.at[1], oy + r.at[2]
  local p = host.screen.pixel(px, py)
  if not p or colorDist(p, r.closedColor) > REVEAL_TOLERANCE then return done() end
  if refuseOutside(self, c, px, py) then return end
  local pinned = self:hwnd()
  host.input.click(px, py)
  host.timer.after(r.settle or 250, function()
    if self.active and self:hwnd() == pinned then done() end
  end)
end

-- The value of an image-read control, delivered to `done` when the worker answers.
--
-- Same measurement as the synchronous versions above and the same verdict, only off the
-- main thread: one capture of the control's own region, every template matched against
-- that single frame. `done(nil)` when there is nothing to say — a toggle whose templates
-- both miss has no state to report, and inventing one is worse than silence.
local function valueOfNow(self, c, done)
  local ox, oy = originOf(self, c)
  local opts = {
    region = c.region
      and { ox + c.region[1], oy + c.region[2], ox + c.region[3], oy + c.region[4] }
      or searchRect(self, c),
    tolerance = c.tolerance or MATCH_TOLERANCE,
    scales = c.scales or MATCH_SCALES,
  }
  if c.kind == "gtoggle" then
    local templates, labels = {}, {}
    addTemplates(templates, labels, c.onImage, "on")
    addTemplates(templates, labels, c.offImage, "off")
    if #templates == 0 then return done(nil) end
    host.screen.imageSearchAsync(templates, opts, function(hit)
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
    opts.region = searchRect(self, c)
    host.screen.imageSearchAsync(asList(c.image), opts, function(hit)
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
        if host.calibrating and not c._savedMiss then
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
  host.screen.imageSearchAsync(asList(c.thumbImage), opts, function(hit)
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
local function speakControl(self, index, moveMouse, withHint, queued)
  local c = self.controls[index]
  if not c then return end
  -- A tab control announces WHICH tab is current, then that it is a tab control (so the
  -- user knows Left/Right — or Ctrl+Tab, Ctrl+number — switch it). Its current tab's
  -- controls are the ones Tab then steps into (see isFocusable's `when` gating).
  if c.kind == "tabcontrol" then
    if c.current then
      local ok, n = pcall(c.current, self)
      if ok and type(n) == "number" and c.tabs[n] then c._current = n end
    end
    local tab = c.tabs[c._current]
    if c.verticalName then
      local ok, name = pcall(c.verticalName, self, c._current)
      if ok and type(name) == "string" and name ~= "" then
        tab = { label = name, hotkeyLabel = tab and tab.hotkeyLabel }
      end
    end
    host.speech.output(tabMessage(tab, c.typeLabel or "tab control"), { interrupt = true })
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
  if c.text then
    local ok, v = pcall(c.text, self)
    if ok and v ~= nil and v ~= "" then parts[#parts + 1] = tostring(v) end
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
  if tl ~= "" then parts[#parts + 1] = tl end
  -- The KEY, spoken on arrival. Without it a hotkey is only discoverable by being told, and
  -- the whole point of these overlays is that a plug-in with no accessible tree can still be
  -- learned from itself. Only on a focus read — repeating it when the user has just pressed
  -- the key is noise, and they plainly know it.
  if moveMouse and c.hotkey then parts[#parts + 1] = tostring(c.hotkey) end
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
    parts[#parts + 1] = (r.text ~= "" and r.text) or c.fallback or "no text"
    -- An OCREdit drops the caret into the real edit control on focus, so the user can type
    -- to change the value right away. Read first (clean), then click. `moveMouse` is true
    -- only on a focus read, so navigating away doesn't re-click.
    if c.kind == "ocredit" and moveMouse then
      host.input.click(ox + math.floor((c.region[1] + c.region[3]) / 2),
        oy + math.floor((c.region[2] + c.region[4]) / 2))
    end
  end
  if c.kind == "hotspottoggle" then
    local st = hotspotToggleState(self, c)
    if st then
      parts[#parts + 1] = st
      local h = withHint and hintFor(c, st) or nil
      if h then parts[#parts + 1] = h end
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
  host.speech.output(table.concat(parts, ", "), { interrupt = not queued })
  if needsValue then
    local at = self.focus
    local pinned = self:hwnd()
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
    read = function() return c.text and select(2, pcall(c.text, self)) or nil end,
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
  for i = 1, #self.controls do
    if isFocusable(self, i) then return i end
  end
  return 0
end

-- A Tab step, timed in its two halves.
--
-- This is THE interaction path: everything the user does is a focus move followed by an
-- announcement, and any millisecond here is one they wait with nothing to hear. The two
-- halves cost for different reasons and want different fixes, so they are measured apart —
-- SCAN is the `when` predicates over every control (a screen pixel or a UIA search, per
-- control, per step), ANNOUNCE is reading the focused control's state (an image search for
-- a graphic toggle, OCR for an `ocrLabel`, a pixel for a hotspot toggle).
local function focusStep(self, advance, dir)
  local n = #self.controls
  if n == 0 then return end
  local t0 = host.now and host.now() or 0
  local found = false
  for _ = 1, n do
    self.focus = advance(self.focus, n)
    if isFocusable(self, self.focus) then
      found = true
      break
    end
  end
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
-- plugin's OWN focus ring and walks it (host.element.focusStep, which raises a real UIA focus
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
  local r = host.element.focusStep(ctrl.id, dir)
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
  -- Before the click, not after: the menu can be up by the time the click returns, and from
  -- that instant the keys belong to it.
  if c.opensMenu then self:_menuOpening() end
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
    -- WHERE it clicked, in the log. A hotspot that presses nothing and a hotspot that presses
    -- the wrong thing are the same experience for someone who cannot see the screen — the
    -- control says "activated" either way — and telling them apart cost a whole test round
    -- because this line was not here. The image-searching button next door has always logged
    -- its outcome; these two, the ones with authored coordinates and therefore the ones most
    -- likely to be aimed wrong, did not.
    host.log.info(string.format("%s: '%s' activated, clicking (%d,%d)",
      tostring(self.label), tostring(c.label), px, py))
    host.input.click(px, py)
    host.speech.output((c.label or "") .. ", activated", { interrupt = true })
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
    -- Read BEFORE the action, because afterwards is too late to know what it was.
    local was = c.text and select(2, pcall(c.text, self)) or nil
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
    speakControl(self, i, false)
    if c.kind == "ocredit" or not c.readOnly then
      local cx, cy = ocrCenter(self, c)
      host.input.click(cx, cy)
    end
  elseif c.kind == "gtoggle" and c.region then
    -- Reveal first, for the same reason the state read does: a click aimed at a control
    -- inside a collapsed panel lands on the panel, not the control.
    ensureRevealed(self, c, function()
      local ox, oy = originOf(self, c)
      host.input.click(ox + math.floor((c.region[1] + c.region[3]) / 2),
        oy + math.floor((c.region[2] + c.region[4]) / 2))
      -- Re-read the new state once the UI redraws. `withHint`: this is the transition, the
      -- one moment a state hint is worth hearing (see hintFor).
      if host.timer then host.timer.after(150, function() speakControl(self, i, false, true) end) end
    end)
  elseif c.kind == "gbutton" and c.image then
    host.log.info(string.format("[gbutton] '%s' activated", tostring(c.label)))
    -- Located, then clicked, and never clicked unlocated. The search is asynchronous, so this
    -- is a continuation: it re-checks that the overlay is still active on the SAME window,
    -- exactly as the hotspot `points` sequence and the panel reveal do, because a click
    -- arriving after the world moved is the one mistake pressing the key again cannot undo.
    local pinned = self:hwnd()
    host.speech.output((c.label or "") .. ", activated", { interrupt = true })
    host.screen.imageSearchAsync(asList(c.image), {
      region = searchRect(self, c),
      tolerance = c.tolerance or MATCH_TOLERANCE,
      scales = c.scales or MATCH_SCALES,
    }, function(hit)
      if not (self.active and self:hwnd() == pinned) then
        -- Found, and deliberately NOT clicked, which was silent until it cost a diagnosis:
        -- a refusal that says nothing is indistinguishable from a search that never answered.
        host.log.info(string.format(
          "[gbutton] '%s': the overlay moved on before the search answered; not clicking",
          tostring(c.label)
        ))
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
        cx, cy = hit.x + c.clickOffset[1], hit.y + c.clickOffset[2]
      end
      if refuseOutside(self, c, cx, cy) then return end
      if c.dragBy then
        -- `dragBy` may be a FUNCTION of where the handle was just found, not only a fixed
        -- offset. Kontakt's resize grip is why: dragging it down until the plug-in's corner
        -- passes under the taskbar leaves the grip unreachable, and a window that can be made
        -- bigger but never smaller again is worse than one that refuses to grow. A caller that
        -- knows its own limits can compute the step from the handle's actual position, and
        -- return nothing to decline — with a reason, so declining is not silence.
        local dx, dy, why
        if type(c.dragBy) == "function" then
          dx, dy, why = c.dragBy(self, hit)
        else
          dx, dy = c.dragBy[1], c.dragBy[2]
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
    local px, py = hotspotPoint(self, c)
    if not px then return end
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
    local before = host.screen.pixel(px, py)
    host.input.click(px, py)
    -- Announced when the state has actually CHANGED, not a fixed moment later. A control that
    -- redraws in thirty milliseconds is announced in thirty; one that takes half a second is
    -- announced correctly instead of early. A control that does not change at all — the
    -- already-chosen member of a radio group — is announced at the deadline, which is the
    -- truth about it.
    if host.timer then
      local say = function()
        local after = host.screen.pixel(px, py)
        local function name(q) return q and string.format("%d,%d,%d", q.r, q.g, q.b) or "?" end
        host.log.info(string.format(
          "%s: '%s' pressed at (%d,%d) — pixel %s -> %s%s",
          tostring(self.label), tostring(c.label), px, py, name(before), name(after),
          (before and after and before.r == after.r and before.g == after.g
            and before.b == after.b) and " (unchanged: the click changed nothing there)" or ""
        ))
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
function Overlay:_visibleClaimant(candidates)
  for _, i in ipairs(candidates) do
    local c = self.controls[i]
    if isVisible(self, c) then return i end
  end
  return nil
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
  local took, skipped = {}, {}
  for _, spec in ipairs(specs) do
    local candidates = claim[spec]
    if self:_visibleClaimant(candidates) then
      took[#took + 1] = spec
      self.controlHotkeys[#self.controlHotkeys + 1] = host.hotkey.register(spec, function()
        -- The claimant is resolved AFTER the wait, not before: the point of resolving at press
        -- time is that the visible set can have changed, and waiting is time in which it can.
        afterModifiersReleased(function()
          self:activate(self:_visibleClaimant(candidates) or candidates[1])
        end)
      end)
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
-- Idempotent: the watch polls at 150 ms and calls this on every tick.
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

-- The on-screen windows of the overlay's own process, keyed by the platform's window id,
-- or nil when the origin is not known. What the menu watch compares before and after a
-- control opens a menu — see watchMenus for why the window list, of all things.
local function windowIdsOf(o)
  local origin = o:origin()
  local pid = origin and origin.app and origin.app.pid
  if not pid then return nil end
  local ids = {}
  for _, w in ipairs(host.window.windowsOf(pid)) do ids[w.id] = w end
  return ids
end

-- Called the moment a control that OPENS a menu is activated, because the watch's 150 ms
-- poll is too slow to be the only answer: ReaHotkey does the same eagerly (Kontakt8.ahk:97
-- checks for the menu immediately after clicking the snapshot dropdown) rather than waiting
-- for its own timer.
function Overlay:_menuOpening()
  if not self.active then return end
  host.keys.menuOpen(true)
  self:_suspendControlHotkeys(true)
  -- The window list as it stands BEFORE the click (this runs before it — see the call
  -- site), so a window that is there afterwards and was not now can be told from the ones
  -- that were always there. The watch does the comparing.
  self._menuBaseline = windowIdsOf(self)
  self._menuPopup = nil
  self._menuHoldCut = nil
  -- Tells the menu watch that a self-drawn menu is now plausible, so its expensive
  -- accessibility walk is worth running at full rate for a while. Outside that window the
  -- walk drops to a backstop cadence — see watchMenus for what it costs.
  menuPlausibleUntil = host.now() + MENU_PLAUSIBLE_MS
  -- And a FLOOR, not only a hint. The watch lowers the flag on the first tick that sees no
  -- menu, so on a platform where nothing can see this plugin's menu the eager suspension
  -- lasts 150 ms and the menu is unusable — which is exactly what the macOS tester met:
  -- arrows moved through the menu, Enter re-fired the control underneath and reopened it,
  -- and no choice ever stuck. A control that says it opens a menu is a better witness than
  -- a detector that cannot see one, so its word holds for a few seconds unless a detector
  -- positively says otherwise.
  self._menuHoldUntil = host.now() + MENU_HOLD_MS
  self._menuHoldConfirmed = false
  -- The watch is what LOWERS the flag again, by seeing that no menu is there. An overlay
  -- that isn't watched has nothing to lower it — and a stranded suspension is an overlay
  -- whose every hotkey has silently stopped working, with nothing to tell the user why. So
  -- an unwatched overlay gets a one-shot restore: the eager suspension is worth having for
  -- the moment a menu takes to appear, not for the rest of the session.
  if not self._menuWatch then
    host.timer.after(600, function()
      host.keys.menuOpen(false)
      self:_suspendControlHotkeys(false)
    end)
  end
end

function Overlay:_registerHotkeys()
  self.registered = self.registered or {}
  self.capturedKeys = self.capturedKeys or {}
  self.capturedSpecs = {}
  self._typing = false
  self._hotkeysSuspended = false
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
          self.registered[#self.registered + 1] = host.hotkey.register(tab.hotkey, function()
            afterModifiersReleased(function() self:gotoTab(n) end)
          end)
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
      -- Ctrl+Tab and Ctrl+<n> stay STATIC: they are ours everywhere in the overlay, and no
      -- application uses them for anything a user would miss. Plain Left and Right do NOT —
      -- they moved to _syncNativeKeys, so the tab control owns them only while it is focused.
      --
      -- Statically was wrong and it cost a real application its keyboard: with the overlay up
      -- on Melodyne, its arrow keys stopped reaching the editor entirely, and the user's own
      -- diagnosis is the right rule — everything should pass through except what the CONTROL
      -- IN FOCUS explicitly wants. A tab control three stops away wanting Left is not a reason
      -- to take Left away from the thing the user is actually working in.
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
-- Idempotent; called on every focus move and at registration. No-op unless active.
function Overlay:_syncNativeKeys()
  if not self.active then return end
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
  local wantArrows = c ~= nil
    and (c.kind == "slider" or c.kind == "tabcontrol" or c.kind == "stepper")
  if wantArrows and not self._arrowTokens then
    -- The same two keys mean different things to the two controls that want them, so which
    -- one is focused decides what they do.
    local isTab = c.kind == "tabcontrol"
    self._arrowTokens = {
      host.keys.capture("Left", function()
        if isTab then self:switchTab(-1) else self:adjust(-1) end
      end),
      host.keys.capture("Right", function()
        if isTab then self:switchTab(1) else self:adjust(1) end
      end),
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
      host.keys.capture("Up", function() self:stepTabVertical(-1) end),
      host.keys.capture("Down", function() self:stepTabVertical(1) end),
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
    local was = c.text and select(2, pcall(c.text, self)) or nil
    local ok, err = pcall(c.onStep, dir, self)
    if not ok then
      host.log.info(string.format("%s: '%s' failed while stepping: %s",
        tostring(self.label), tostring(c.label), tostring(err)))
      host.speech.output((c.label or "That control") .. " could not be moved",
        { interrupt = true })
      return
    end
    -- What the value BECAME is the only honest thing to announce; what we asked it to become
    -- is a guess that sounds exactly as confident.
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

function Overlay:_activate()
  if self.active then return end
  self.active = true
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
  local origin = self:origin()
  local id = (origin and origin.id) or false
  if id ~= self._lastActiveId then
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
  self:_registerHotkeys()
  -- Scope key suppression to this (now-foreground) window: when a control opens a menu
  -- (another window comes forward), Tab/Enter pass to it natively, and resume here once
  -- the plugin window is focused again.
  host.keys.scope(true)
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
      -- Queued, not interrupting: see `speakControl`. Nobody pressed a key to hear this.
      if self.active then speakControl(self, self.focus, false, false, true) end
    end)
  end
end

function Overlay:_deactivate()
  if not self.active then return end
  self.active = false
  host.keys.scope(false)
  host.keys.menuOpen(false)
  self:_unregisterHotkeys()
  -- A hold belongs to the moment a control opened a menu, not to the overlay's whole life.
  self._menuHoldUntil = nil
  self._menuHoldConfirmed = false
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
  -- against the matched context's coordinate origin.
  if matched and self._gate then
    matched = self._gate(self:origin()) == true
    if not matched then self.activeCtx = false end
  end
  if t0 then
    local total = host.now() - t0
    if total >= RECHECK_SLOW_MS then
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
  local claim, order = {}, {}
  for i, c in ipairs(self.controls) do
    if type(c.hotkey) == "string" then
      if claim[c.hotkey] then
        table.insert(claim[c.hotkey], i)
      else
        claim[c.hotkey] = { i }
        order[#order + 1] = c.hotkey
      end
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
-- also matches how the menu watch is already done here: one watch per VM, not one per
-- overlay that has to remember to ask for it.
local pollGroups = {}
local function joinPoll(self, ms)
  local group = pollGroups[ms]
  if not group then
    group = {}
    pollGroups[ms] = group
    -- Disable-safe recurring poll, like the menu watch: it survives a disable/enable cycle
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
      for _, o in ipairs(group) do
        o:_recheck()
      end
      local dt = (host.now and host.now() or 0) - t0
      if dt >= 25 then
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
function O.memoByEpoch(fn)
  local epoch, value = nil, nil
  return function(...)
    local now = host.epoch and host.epoch() or 0
    if epoch == now and value ~= nil then return value end
    local v = fn(...)
    if v ~= nil then
      epoch, value = now, v
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
      if list.title or list.app or list.windows then
        out[#out + 1] = list -- a single matcher, not a list of them
      else
        for _, h in ipairs(list) do out[#out + 1] = h end
      end
    end
  end
  return out
end

-- One resolved origin per (spec, epoch), shared by every overlay bound to that spec.
-- See the note in attachEmbedded for why this is not per overlay. Weak keys: an entry
-- lives exactly as long as the spec it answers for.
local originMemo = setmetatable({}, { __mode = "k" })

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
  -- The one thing in an embedded binding that cannot be written once. A host names its
  -- plugin surface with a Win32 window class on Windows and an accessibility role on
  -- macOS, and no amount of abstraction makes "Qt5152QWindowIcon" and "AXGroup" the same
  -- string. So this accepts an OS-keyed table — `control = { windows = …, macos = … }` —
  -- and everything else about the binding stays shared.
  local pattern = host.os.pick(spec.control)
  -- No pattern for this platform means this binding has nothing to say here. Returning
  -- rather than matching against nil is the difference between a module that is honestly
  -- inert on a platform nobody has measured yet and one that throws on the first focus
  -- change; and an OS-keyed `control` with no entry for the current system is exactly how a
  -- module declares "not here, not yet".
  if pattern == nil then
    host.log.info(string.format(
      "[%s] embedded binding is inactive on %s: no control pattern for this platform",
      self.name or "overlay", host.os.current
    ))
    return self
  end
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
  local function origin(opts)
    local e = host.epoch and host.epoch() or -1
    local memo = originMemo[spec]
    if not memo then
      memo = { epoch = -1, value = nil }
      originMemo[spec] = memo
    end
    if not (opts and opts.fresh) and e == memo.epoch then return memo.value end
    memo.value = pluginControl()
    memo.epoch = e
    return memo.value
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
-- How often the expensive Qt-menu check runs when no menu is plausible. 8 ticks x 150 ms is
-- a worst case of ~1.2 s to notice a menu opened by some other route, against the permanent
-- 100 %+ duty cycle it cost before.
local MENU_BACKSTOP_TICKS = 8
local menuTick = 0
function Overlay:watchMenus()
  for _, o in ipairs(menuWatched) do
    if o == self then return self end
  end
  menuWatched[#menuWatched + 1] = self
  self._menuWatch = true
  if menuTimer then return self end
  menuTimer = true
  -- Disable-safe recurring poll (survives a disable/enable cycle, unlike a Lua
  -- self-rescheduling host.timer.after).
  host.timer.every(150, function()
    local anyActive, open = false, false
    for _, o in ipairs(menuWatched) do
      if o.active then anyActive = true break end
    end
    if not anyActive then return end
    -- THE CHEAP QUESTION FIRST: is a native popup menu on screen? One window-class lookup.
    -- Every menu these overlays open themselves turns out to be a native one — u-he's preset
    -- menu (verified live), Komplete Kontrol's menu bar, and the plug-in menus generally —
    -- so this answers the common case outright.
    open = host.keys.nativeMenuOpen() == true
    -- THE EXPENSIVE ONE only when it can still pay. `host.element.find(hwnd, "", Menu)` means
    -- "any menu element anywhere in this tree" and therefore walks the plug-in's ENTIRE
    -- accessibility tree, across a process boundary: measured at 50-194 ms per call, on a
    -- 150 ms timer — more than its own interval, permanently, on the thread that owns the
    -- keyboard hook. That was the single largest load in the whole runtime, and it was
    -- being spent almost entirely on answering "no".
    --
    -- It is still asked, because a self-drawn Qt menu is invisible to the cheap check. But
    -- only when a menu is PLAUSIBLE — just after a control that opens one fired, which the
    -- runtime now knows from `opensMenu` — with a slow backstop the rest of the time so a
    -- menu opened some other way is still noticed within about a second rather than never.
    if not open then
      menuTick = menuTick + 1
      local plausible = host.now() < menuPlausibleUntil
      if plausible or menuTick % MENU_BACKSTOP_TICKS == 0 then
        for _, o in ipairs(menuWatched) do
          if o.active then
            local ctrl = o:origin()
            if ctrl ~= nil and ctrl.id ~= nil
              and host.element.find(ctrl.id, "", host.element.type.Menu) == true
            then
              open = true
              break
            end
          end
        end
      end
    end
    -- THE THIRD DETECTOR: a menu drawn as a window of its own. Neither of the two above can
    -- see a popup that posts no accessibility notification and is not an AXMenu — which is
    -- what every menu the macOS tester opened looked like, though that session could not
    -- say so for certain (its observer was never armed on the process). A popup that is
    -- its own window is in the window manager's list, though, whatever the accessibility
    -- tree says of it: compared against the list _menuOpening took before the click, a
    -- window that is there now and was not then is the menu, and its going is the menu
    -- closing. Asked only while a hold is running, so it costs nothing the rest of the
    -- time, and logged the first time it answers either way — "does this plugin's menu
    -- appear in the window list" is the question that decides whether this detector is the
    -- mechanism for that plugin or the stopwatch still is.
    for _, o in ipairs(menuWatched) do
      if o.active and o._menuHoldUntil and o._menuBaseline then
        local now = windowIdsOf(o)
        if now then
          if o._menuPopup then
            local still = false
            for id in pairs(o._menuPopup) do
              if now[id] then still = true break end
            end
            if still then
              open = true
            else
              host.log.info(("[overlay] '%s': the menu window is gone — the menu closed")
                :format(tostring(o.label)))
              o._menuPopup = nil
              -- Detection has spoken: the hold below was confirmed by this window and ends
              -- now that it has gone.
            end
          else
            local appeared, n = {}, 0
            for id, w in pairs(now) do
              if not o._menuBaseline[id] then
                appeared[id] = w
                n += 1
              end
            end
            if n > 0 then
              o._menuPopup = appeared
              open = true
              for id, w in pairs(appeared) do
                host.log.info(("[overlay] '%s': a window appeared for the plugin while a menu "
                  .. "was plausible — id %s%s, layer %d, %dx%d at %d,%d — treated as the menu")
                  :format(tostring(o.label), tostring(id),
                    w.class ~= "" and (" class '" .. w.class .. "'") or "",
                    w.layer, w.w, w.h, w.x, w.y))
              end
            end
          end
        end
      end
    end
    -- THE KEY THAT CLOSED IT. Return and Escape end a menu — Return commits the item under
    -- the cursor, Escape cancels — so while a hold is running and no detector has confirmed
    -- a menu, a Return or Escape that the hook let through to the plugin is the best
    -- evidence there is that the menu is now closing, and the hold is cut to a short grace
    -- instead of running its full course. That course is what left Tab dead for the rest
    -- of eight seconds after every choice the macOS tester made. Unconfirmed holds only:
    -- where a detector can see the menu, its word beats a guess about what a key did
    -- (Return can open a submenu, and a detector sees that; this cannot).
    local passed = host.keys.passedThrough()
    if #passed > 0 then
      for _, o in ipairs(menuWatched) do
        if o.active and o._menuHoldUntil and not o._menuHoldConfirmed then
          for _, k in ipairs(passed) do
            if k.key == "Return" or k.key == "Escape" then
              local grace = host.now() + MENU_CLOSE_GRACE_MS
              if grace < o._menuHoldUntil then
                o._menuHoldUntil = grace
                o._menuHoldCut = k.key
                host.log.info(("[overlay] '%s': %s went through to the menu; the hold ends "
                  .. "in %d ms unless something sees the menu still open")
                  :format(tostring(o.label), k.key, MENU_CLOSE_GRACE_MS))
              end
            end
          end
        end
      end
    end
    -- A control's own word that it opened a menu, honoured until it expires. Detection wins
    -- when it finds something — it is the more specific answer — but never overrules the
    -- hold by finding nothing, because "found nothing" is the failure mode being covered.
    if open then
      for _, o in ipairs(menuWatched) do
        if o._menuHoldUntil and not o._menuHoldConfirmed then
          o._menuHoldConfirmed = true
        end
      end
    else
      for _, o in ipairs(menuWatched) do
        if o.active and o._menuHoldUntil then
          if o._menuHoldConfirmed then
            -- Detection has already PROVED it can see this plugin's menu, so its word that
            -- the menu has gone beats a stopwatch that was only ever standing in for it.
            -- Without this the hold went on forcing "menu open" until its deadline even where
            -- the answer was known — which is what made lengthening the deadline unsafe, and
            -- is why it stayed too short for the platform that has no detector at all.
            o._menuHoldUntil = nil
          elseif host.now() < o._menuHoldUntil then
            open = true
          else
            -- Logged, not spoken, and once: this line is the answer to "is this plugin's
            -- menu something the accessibility layer can see at all", which nobody here can
            -- ask any other way. It now also says the hold RAN OUT rather than merely that
            -- nothing was seen — because a key arriving just after this line is the evidence
            -- that the duration above is still too short, and the two used to be
            -- indistinguishable in a log. And it says whether the window list moved: a hold
            -- that ran its course with the plugin's windows unchanged is a menu drawn inside
            -- the plugin's own window, which no detector this runtime has can see.
            local windows = o._menuBaseline
              and " — and the plugin's window list did not change, so its menu is drawn inside its own window"
              or ""
            if o._menuHoldCut then
              host.log.info(
                ("[overlay] '%s': the hold ended %d ms after %s went through to a menu no "
                  .. "detector ever saw%s"):format(tostring(o.label), MENU_CLOSE_GRACE_MS,
                  o._menuHoldCut, windows)
              )
            else
              host.log.info(
                ("[overlay] '%s': a control opened a menu, no detector ever saw one, and the "
                  .. "%d ms hold has run out — a key claimed by the overlay from here on was "
                  .. "meant for that menu%s"):format(tostring(o.label), MENU_HOLD_MS, windows)
              )
            end
            o._menuHoldUntil = nil
            o._menuHoldCut = nil
          end
        end
      end
    end
    if anyActive then
      host.keys.menuOpen(open)
      -- And the other half of what "a menu is open" has to mean: an active overlay owns no
      -- per-control hotkeys while one is up, so they reach the menu instead of re-firing
      -- the control. This loop is also what LOWERS an eager suspension (_menuOpening) when
      -- the menu never appeared.
      for _, o in ipairs(menuWatched) do
        if o.active then o:_suspendControlHotkeys(open) end
      end
    end
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
  -- Same reasoning as the slider: a graphical button's region is the area it is HUNTED in, so
  -- the region centre is a point the control has nothing to do with. Mark where the template
  -- actually matched, and fall through to the centre only when it cannot be found — which is
  -- itself the useful thing to see in the picture.
  if c.kind == "gbutton" then
    local bx, by = graphicalButtonPoint(self, c)
    if bx then return bx, by end
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
local function shotName(base, k)
  return base .. (k > 1 and ("-" .. k) or "")
end

local shotsTaken = {}
local function shotPath(label)
  local base = safeName(label)
  local n = shotsTaken[base]
  -- Seeded from what is already on disk, not merely counted per run. A counter that lives
  -- only in memory resets on restart, and restarting between two shots of the same overlay
  -- is not the exception while a module is being written — it is the loop. That is exactly
  -- how the first Diva shot got overwritten by the second: the very mistake this numbering
  -- was added to prevent, defeated by the workflow it was meant to serve.
  if n == nil then
    n = 0
    while host.resource.exists("calibration/" .. shotName(base, n + 1) .. ".png") do
      n = n + 1
    end
  end
  n = n + 1
  shotsTaken[base] = n
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
  host.log.info(string.format(
    "[calibrate] %s — origin (%d,%d) captured (%d,%d)-(%d,%d) %dx%d%s -> %s (%s)",
    tostring(self.label), o.client.x, o.client.y,
    region[1], region[2], region[3], region[4], region[3] - region[1], region[4] - region[2],
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
