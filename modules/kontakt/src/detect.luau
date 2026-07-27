-- Kontakt DETECTION: which window is a Kontakt, which version, whether a Komplete
-- Kontrol is wrapping it, and where its content begins. No overlays and no controls
-- here — just the questions every Kontakt overlay has to ask about a window.
--
-- Note what is NOT here any more: a "which version am I on?" lookup. Each overlay is
-- declared for ONE cell of the matrix (version × where it runs), so it knows its version
-- at declaration time and never asks at runtime. See cells.luau.

local O = host.require("com.platform.overlay")
local U = host.uia.type

-- A plugin identifies itself through a UIA element named after it, which NI exposes as a
-- Window on some hosts and a Pane on others — so every identity question here asks about
-- both types at once.
local IDENTITY_TYPES = { U.Window, U.Pane }
-- NI Qt plugin control class — plain ("Qt652QWindowIcon{GUID}1", e.g. Kontakt) OR
-- NI-versioned ("Qt671NI_6_7_1_R3QWindowIcon{GUID}", e.g. Komplete Kontrol); the ".-"
-- spans the optional NI_<version> segment. A loose match: the UIA NAME is what tells
-- these plugins apart.
local QT_PLUGIN = "Qt%d+.-QWindowIcon"

-- (The KK library-browser auto-close + its opt-out setting live in the Komplete Kontrol
-- module: an open browser hides the nested Kontakt, so the KK overlay — not this one — is
-- the overlay active then, and is where the close belongs.)

-- The Kontakt versions, in the order host.uia.findAny reports them. `offset` is the
-- content shift a library needs: libraries are authored in the Kontakt 7 frame and
-- Kontakt 8 renders content 29 px lower, so EVERY host path adds this on top of its own
-- chrome offset.
local VARIANTS = {
  { name = "Kontakt 8", offset = { 0, 29 } },
  { name = "Kontakt 7", offset = { 0, 0 } },
}
local VARIANT_NAMES = {}
for i, v in ipairs(VARIANTS) do VARIANT_NAMES[i] = v.name end

-- WHICH Kontakt a control is, as an index into VARIANTS, or nil for "not a Kontakt".
-- One question, one answer: a single UIA pass over both names and both types, so asking
-- "is this a Kontakt?" and "which one?" costs the same as the old "is this a Kontakt 8?"
-- alone did — and this runs on every detection pass, per candidate control.
local function variantIndex(c)
  return c and c.id and host.uia.findAny(c.id, VARIANT_NAMES, IDENTITY_TYPES) or nil
end
local function anyKontakt(c)
  return variantIndex(c) ~= nil
end

-- ── Komplete Kontrol nesting ──────────────────────────────────────────────────
-- KK can itself host a Kontakt instance (DAW → KK → Kontakt → library). KK and
-- Kontakt share the Qt control class and differ only by UIA name. EnumChildWindows
-- enumerates the nested Kontakt control too, but ReaHotkey anchors a nested library
-- on KK's control and crosses KK's chrome with a fixed content offset — we do the
-- same, so the SAME library layout (authored in the bare-Kontakt frame) lands
-- correctly without the library knowing anything about KK.
local KK_NAME = { "Komplete Kontrol" }
local function isKK(c)
  return c ~= nil and c.id ~= nil and host.uia.findAny(c.id, KK_NAME, IDENTITY_TYPES) ~= nil
end
-- KK chrome offset, from KK's control top-left to where Kontakt's content begins.
-- Computed DYNAMICALLY at runtime (= nested Kontakt.client − KK.client, the exact inner
-- content origin) by kkContentFrame; this constant is only a transient fallback for the
-- brief window before the nested Kontakt control is enumerable (e.g. while KK's library
-- browser is still open and hides it). Live value ≈ 198,222 (KK 6.7.1 + Kontakt 8.11);
-- the old hardcoded 190,111 landed ~111 px too high.
local KK_OFFSET = { 198, 222 }
-- Does the active host window contain a control passing `test` (a KK or a Kontakt
-- container)? The two compete: a bare Kontakt defers to a wrapping KK, and a KK
-- only counts as a host when it actually contains a Kontakt.
local function hasControl(test)
  for _, c in ipairs(host.window.controls()) do
    if string.match(c.class, QT_PLUGIN) and test(c) then return true end
  end
  return false
end
local function kkPresent() return hasControl(isKK) end
local function kontaktPresent() return hasControl(anyKontakt) end

-- True if `inner`'s centre lies within `outer` (both are {x,y,w,h} screen rects).
local function centreInside(outer, inner)
  if not (outer and inner) then return false end
  local cx, cy = inner.x + inner.w / 2, inner.y + inner.h / 2
  return cx >= outer.x and cx <= outer.x + outer.w and cy >= outer.y and cy <= outer.y + outer.h
end

-- The nested Kontakt control (the plain Qt class, "Kontakt N" UIA name) among the
-- host window's controls. A KK container also reports the nested "Kontakt N" name in
-- its subtree, so anyKontakt alone matches it too; the REAL Kontakt control is a
-- Kontakt that is NOT a KK (its plain Qt class, not the NI-versioned KK class). Its
-- client rect IS the content origin, and its bounds are the content area on screen.
-- With a `container` (the KK control being resolved), pick the Kontakt whose bounds
-- sit inside it — so with two KK instances in one host window each KK pairs with ITS
-- own nested Kontakt; an ambiguous match (none, or several, contained) returns nil so
-- the caller uses a transient fallback instead of caching a wrong pairing. Without a
-- container (the bare / standalone case) the first such control.
local function findNestedKontakt(container)
  local candidates = {}
  for _, c in ipairs(host.window.controls()) do
    if string.match(c.class, QT_PLUGIN) and anyKontakt(c) and not isKK(c) then
      candidates[#candidates + 1] = c
    end
  end
  if not container then return candidates[1] end
  local within = {}
  for _, c in ipairs(candidates) do
    if centreInside(container.bounds, c.bounds) then within[#within + 1] = c end
  end
  if #within == 1 then return within[1] end
  if #within == 0 and #candidates == 1 then return candidates[1] end
  return nil -- ambiguous: don't guess (caller falls back, uncached)
end

-- ── Kontakt's own dialogs ────────────────────────────────────────────────────
-- When Kontakt runs natively in REAPER, its dialogs are TOP-LEVEL windows of the HOST
-- process. Measured live with the calibrator: "Content Missing" is a `#32770` owned by
-- reaper.exe, containing an NIChildWindow frame and, at the identical rect, the Qt window
-- that draws it — whose own accessible element is named "Kontakt 7", type Window.
--
-- Every gate we have waves that through. Our REAPER host matcher takes any `#32770` of
-- that process as a plugin host (REAPER's FX window is one), so the dialog counts as a
-- host window; its Qt child matches the plugin's window class, because Kontakt draws both
-- with the same Qt build; and `variantIndex` finds "Kontakt 7" without descending a single
-- level, because the dialog IS Kontakt. attachEmbedded takes the first candidate that
-- passes, so the header overlay attached to the dialog and measured its whole geometry
-- against a 600x440 rectangle: the instrument arrows landed in the empty file list, and
-- the snapshot probe read dialog background and reported "Snapshots not available".
--
-- So it is asked at the WINDOW level, by title, which is where the answer actually lives —
-- and which is what ReaHotkey does for this same case (Kontakt7.ahk:163 gates its dialog
-- handler on `WinGetTitle("A") = "content Missing"`). Cheap: no UIA, no screen.
local DIALOG_TITLES = { "^Content Missing" }
local function inOwnDialog()
  local a = host.window.active()
  if not (a and a.title) then return false end
  for _, p in ipairs(DIALOG_TITLES) do
    if string.match(a.title, p) then return true end
  end
  return false
end

-- Is this control the container a HOST puts the plugin in, rather than something in its
-- own right? Komplete Kontrol hosts its plugin inside an `NIChildWindow` — the same window
-- class Kontakt's "Content Missing" dialog uses — so that class alone cannot tell the
-- dialog from an ordinary KK session. Observed live: KK's plugin container and the nested
-- Kontakt control have IDENTICAL bounds (both (448,350) 633×418), because one is simply
-- the frame around the other.
--
-- That test has a blind spot, found the hard way: Kontakt's own dialog frame ALSO holds a
-- Qt window at exactly its own rect (both (660,307) 600×440 live), so the dialog looks
-- like a plugin container and the Content Missing overlay REJECTED the very dialog it
-- exists for — which is how the header overlay came to be the one active over it. Callers
-- must therefore settle `inOwnDialog` FIRST; this answers only the question it can.
-- (A DAW does not hit this at all: REAPER's container is a `reaperPluginHostWrapProc`,
-- so no NIChildWindow exists there in normal use.)
local function isPluginHostContainer(c)
  local kc = findNestedKontakt()
  local a, b = c and c.bounds, kc and kc.bounds
  return a ~= nil
    and b ~= nil
    and a.x == b.x
    and a.y == b.y
    and a.w == b.w
    and a.h == b.h
end

-- The coordinate frame for a library nested in Komplete Kontrol: KK's control top-left to
-- where Kontakt's content begins = the nested Kontakt control's client − KK's client,
-- computed LIVE (the old hardcoded constant was wrong and unstable), PLUS the version's
-- own content shift, which the CELL supplies — libraries are authored in the K7 frame, so
-- K8 needs +29 here exactly as in the bare and standalone paths.
--
-- Cached only once positively resolved, so a transient miss (KK browser still hiding
-- Kontakt; UIA not ready right after activation) returns the fallback WITHOUT pinning it.
-- The key includes the container's SIZE: the offset is invariant under window moves (both
-- client origins shift together, so it stays valid and cached) but KK can reposition the
-- embedded plugin on a resize/reflow, which changes the size and so re-derives.
local function kkContentFrame(vx, vy)
  local cache = {}
  return function(c)
    local key = c.id .. "|" .. (c.bounds and (c.bounds.w .. "x" .. c.bounds.h) or "?")
    local cached = cache[key]
    if cached then return cached[1], cached[2] end
    local kc = findNestedKontakt(c)
    if kc and kc.client and c.client then
      local off = { kc.client.x - c.client.x + vx, kc.client.y - c.client.y + vy }
      cache[key] = off
      host.log.info(string.format(
        "Kontakt nested in Komplete Kontrol — dynamic content offset (%d, %d) "
          .. "[Kontakt.client (%d,%d) − KK.client (%d,%d) + version (%d,%d)]",
        off[1], off[2], kc.client.x, kc.client.y, c.client.x, c.client.y, vx, vy
      ))
      return off[1], off[2]
    end
    return KK_OFFSET[1], KK_OFFSET[2] -- not resolvable yet: transient, NOT cached
  end
end

-- A constant frame — the version's own content shift, which is the whole story for a bare
-- Kontakt in a DAW and for a standalone one (measured live: the standalone window's client
-- coincides with the Kontakt content control's, with no NI menu-bar chrome offset).
local function constantFrame(vx, vy)
  return function() return vx, vy end
end

-- Which VIEW Kontakt is in — the modern one, or the classic rack.
--
-- Nothing accessible can be asked, so it is read from a single pixel in Kontakt's OWN top
-- bar: an icon that is present there only in the classic view. Measured across two
-- calibration shots of the same window, (24,24,24) modern against (99,99,99) classic. Its
-- being Kontakt chrome rather than the loaded library's UI is the point — the library
-- changes with every instrument, this does not. It belongs to the right-hand cluster
-- (Update, CPU, …), so it is measured from the right edge.
--
-- Memoized per dispatch pass: this backs `when` predicates, which run on every focus move,
-- and one screen pixel costs a compositor frame (~17 ms) — a handful of controls asking
-- separately would add a tenth of a second to every keypress.
--
-- A single pixel is a weak signal, so it is used only for what is safe to get wrong: which
-- LABEL a view-toggle carries, and whether a control that needs the classic view is
-- offered. Never to decide where to click.
local VIEW_PROBE_FROM_RIGHT, VIEW_PROBE_Y = 262, 24
local viewKey, viewCache = nil, false
local function isClassicView(origin)
  if not (origin and origin.client and origin.bounds) then return false end
  -- Keyed on `inputEpoch`, NOT `epoch`. Which view Kontakt is in can only change when
  -- something acts — our F10, or a click in its menu — and both go through host.input,
  -- which turns that counter over. `epoch` also turns over on every timer tick and every
  -- key the USER presses, and this probe gates which header controls exist, so it is
  -- consulted on every focus move: against `epoch` that meant one fresh screen read per
  -- Tab, measured at 21-38 ms of a ~70 ms step, to re-answer a question whose answer
  -- pressing Tab cannot have changed.
  local gen = host.inputEpoch and host.inputEpoch() or (host.epoch and host.epoch() or 0)
  local key = tostring(origin.id) .. "@" .. tostring(gen)
  if viewKey == key then return viewCache end
  -- Against the CONTENT's right edge, not the window frame's: standalone, the frame is
  -- 16 px wider than the plugin, which is enough to read a neighbouring icon instead.
  local p = host.screen.pixel(
    origin.client.x + O.contentSize(origin) - VIEW_PROBE_FROM_RIGHT,
    origin.client.y + VIEW_PROBE_Y
  )
  viewKey = key
  viewCache = p ~= nil and (p.r + p.g + p.b) / 3 > 60
  return viewCache
end

-- The "Kontakt N" identity element's name under `hwnd`, or nil. Used to scope a
-- two-level UIA search into the plugin's real UI (see actions.luau's pluginLocate).
local function kontaktPaneName(hwnd)
  local i = hwnd and host.uia.findAny(hwnd, VARIANT_NAMES, IDENTITY_TYPES)
  return i and VARIANTS[i].name or nil
end

-- Kontakt 7's equivalent question — is the RACK on screen, rather than a single
-- instrument? Its arrows, multi and snapshot controls exist only in the rack, exactly as
-- Kontakt 8's do only in the classic view, and ReaHotkey gates them the same way
-- (Kontakt7.ahk:76-100, `If This.GetPluginView() = "rack"`).
--
-- Unlike Kontakt 8 this needs no pixel: Kontakt 7 exposes its menu bar as real UIA
-- buttons — a dump of a live embedded instance lists FILE, LIBRARY, VIEW, SHOP — and
-- ReaHotkey reads the view off exactly that (GetView: find the SHOP button, take its
-- PREVIOUS sibling, "VIEW" means rack). We ask the shorter form of the same question,
-- because the raw walker can search by name but not step siblings by name.
--
-- Fails OPEN, and that is deliberate: a wrong "no" here HIDES the controls, which is the
-- failure the user actually hit — all seven silently gone. So "the plugin's UIA is not
-- answering at all" (no Kontakt pane resolves) counts as rack, and only a plugin that
-- answers but has no VIEW button counts as single-instrument view.
local function isRackView()
  local fg = host.window.active()
  if not (fg and fg.id) then return true end
  local via = kontaktPaneName(fg.id)
  if not via then return true end -- cannot tell: keep the controls
  return host.uia.pluginLocate(fg.id, via, "VIEW", U.Button) ~= nil
end

return {
  VARIANTS = VARIANTS,
  VARIANT_NAMES = VARIANT_NAMES,
  QT_PLUGIN = QT_PLUGIN,
  IDENTITY_TYPES = IDENTITY_TYPES,
  variantIndex = variantIndex,
  anyKontakt = anyKontakt,
  isKK = isKK,
  kkPresent = kkPresent,
  kontaktPresent = kontaktPresent,
  findNestedKontakt = findNestedKontakt,
  isPluginHostContainer = isPluginHostContainer,
  kkContentFrame = kkContentFrame,
  constantFrame = constantFrame,
  kontaktPaneName = kontaktPaneName,
  isClassicView = isClassicView,
  isRackView = isRackView,
  inOwnDialog = inOwnDialog,
}
