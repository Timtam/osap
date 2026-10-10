-- Kontakt DETECTION: which window is a Kontakt, which version, whether a Komplete
-- Kontrol is wrapping it, and where its content begins. No overlays and no controls
-- here — just the questions every Kontakt overlay has to ask about a window.
--
-- Note what is NOT here any more: a "which version am I on?" lookup. Each overlay is
-- declared for ONE cell of the matrix (version × where it runs), so it knows its version
-- at declaration time and never asks at runtime. See cells.luau.

local O = host.require("com.platform.overlay")
local U = host.element.type

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

-- The Kontakt versions, in the order host.element.findAny reports them. `offset` is the
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
  -- A control that already knows what it is. The macOS panel below is found by Kontakt's own
  -- file-menu button or its header text, not by an identity element — nothing inside a DAW's
  -- plug-in window on a Mac is NAMED Kontakt 7 — so the accessibility question has no answer
  -- there, and the thing that found the panel says instead.
  if c and c.variant then
    for i, v in ipairs(VARIANTS) do
      if v.name == c.variant then return i end
    end
  end
  return c and c.id and host.element.findAny(c.id, VARIANT_NAMES, IDENTITY_TYPES) or nil
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
  return c ~= nil and c.id ~= nil and host.element.findAny(c.id, KK_NAME, IDENTITY_TYPES) ~= nil
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
-- THE SCENE: what is in the active window right now, worked out ONCE per epoch.
--
-- Six cells ask overlapping questions about the same window on every recheck — is there a
-- Komplete Kontrol, is there a Kontakt, which version, is this one nested in that one — and
-- each used to walk the control list and re-interrogate every candidate itself. Measured,
-- one cell's context match cost 15-96 ms with every underlying OS answer already served
-- from a host-side cache: not one expensive call to find, but thousands of cheap ones. So
-- the count is what has to come down, and the way down is to derive the whole picture once
-- and let all six read it.
--
-- Per EPOCH rather than per window, because these verdicts are relational: "this Kontakt is
-- bare" is only true until a Komplete Kontrol wraps it, and a per-window cache would pin the
-- answer from before that happened. That is the same reason these cells declare
-- `cacheIdentity = false`, and this is how to honour it without paying for it every time.
local scene = O.memoByEpoch(function()
  local kk, kontakt, byId = {}, {}, {}
  for _, c in ipairs(host.window.controls()) do
    if string.match(c.class, QT_PLUGIN) then
      -- Both questions asked once per candidate, here and nowhere else.
      local asKK = isKK(c)
      local variant = variantIndex(c)
      byId[c.id] = { control = c, isKK = asKK, variant = variant }
      if asKK then
        kk[#kk + 1] = c
      elseif variant then
        kontakt[#kontakt + 1] = c
      end
    end
  end
  return { kk = kk, kontakt = kontakt, byId = byId }
end)

local function kkPresent() return #scene().kk > 0 end
-- "Any control that reports a Kontakt name", which INCLUDES a Komplete Kontrol — a KK
-- exposes its nested Kontakt's name in its own subtree, and the old hasControl(anyKontakt)
-- counted that. Read off the recorded variant rather than the two lists, because a KK with
-- nothing loaded in it is in `kk` and is NOT a Kontakt.
local function kontaktPresent()
  for _, e in pairs(scene().byId) do
    if e.variant ~= nil then return true end
  end
  return false
end

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
  -- The real Kontakt controls are exactly the scene's non-KK Kontakts; no second walk.
  local candidates = scene().kontakt
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

-- ── Kontakt in a DAW's plug-in panel, on macOS ───────────────────────────────
-- There is no control to find there. The third Mac session probed Kontakt 7 inside REAPER on an
-- M1 mini: the plug-in's 34 accessibility elements are direct children of REAPER's FX window —
-- no container, no class, no identifier — the same 34 the standalone window publishes, shifted.
-- So the overlay runtime gives the in-DAW cells the DAW's plug-in panel instead (daw-hosts'
-- `pluginOrigin`, the "host panel"), in REAPER, Logic and any DAW with an entry, and this module
-- says nothing about any DAW: not its title, not its geometry. What it does say is Kontakt's.
--
-- WHETHER the panel shows a Kontakt of this version, from Kontakt's own evidence inside it:
--   * its file-menu button by accessibility — Kontakt 7's "FILE", Kontakt 8's "Kontakt File
--     Menu" — NEAR where Kontakt's geometry puts it in the panel: FILE's centre sits at the
--     authored {175,19} from Kontakt's corner, measured to 2 px in the standalone window and in
--     REAPER alike (the third session: (176,19)). "Near" is ANCHOR_NEAR points either way, a
--     margin for a DAW origin that is a few points off (Logic's is estimated from one probe).
--     A button of that name anywhere else in the window is some other plug-in's, which is the
--     case a Logic window titled "Inst 1" leaves nothing else to tell apart. "FILE" is a
--     generic name, so for Kontakt 7 its neighbour is asked too: a button named LIBRARY 35 to
--     90 points to its right on the same row, which the third session found published beside
--     it (FILE, LIBRARY, VIEW and SHOP, in REAPER as standalone). The version's own name —
--     "Kontakt File Menu" — is not generic, and gets a box of its own, wider across
--     (geometry.fileMenuNear): its {86,19} was authored as a click point, not measured as a
--     centre, and refusing the one name only a Kontakt 8 publishes for missing a guessed point
--     by 25 would leave the next session with no Kontakt 8 overlay at all. Still ANCHOR_NEAR
--     down and up, which is what refuses a Kontakt 8 nested in Komplete Kontrol (about 198,222
--     from the DAW's origin);
--   * for Kontakt 7, when it publishes nothing (a Mac log of 2026-09-27: no FILE button and "0
--     focusable candidates" on three presses over 73 s, in REAPER's FX chain, while the
--     standalone Kontakt 7 on the same Mac published its header half a minute after it
--     started), its top row read as text: FILE with LIBRARY to its right, in a small region
--     around where FILE belongs — off the event loop (`host.ocr.recognize` with a callback), in
--     English, whose words these are. Measured on the Windows calibration shot of Kontakt 7,
--     whose chrome Kontakt draws itself: FILE's letters span x 164–188, y 15–23, a centre of
--     (176, 19) — the button's own — and LIBRARY's centre is 56 to its right (35 to 90
--     accepted). A word Vision boxes is within a point or two of its ink (the probe's
--     text-against-elements lines: 0 to 2 points on buttons). Until the read answers, the panel
--     is not Kontakt's; an answer that anchors asks every overlay to look again
--     (`host.window.recheck`), because the answer is not a focus change and nothing else would.
-- Kontakt 8 has no text fallback: its file-menu button is a wordmark, not a word.
--
-- WHERE Kontakt's corner is: FILE's centre minus the authored offset — Kontakt's own geometry,
-- the same in every DAW — so the panel handed back starts there, and every header coordinate
-- lands as it does on Windows. The difference between that corner and the DAW's origin is
-- logged, because it is the measurement a Mac session is after: how far each DAW's origin is
-- from where a plug-in actually starts. The overlay runtime still gates the keyboard on the
-- DAW's panel as well as on this one, so a DAW origin more than 19 points too LOW — Kontakt's
-- top row then above the DAW's panel — has the keyboard on FILE read as the DAW's chrome: the
-- anchor line shows that origin, and the entry in daw-hosts is what to correct.
--
-- Kept per STAY — the runtime's word for the keyboard inside one window's panel, which ends
-- when it visits the DAW's chrome, where another plug-in could have been selected (see the
-- overlay runtime, "the stay") — not per window: one REAPER FX chain window shows whichever FX
-- is selected. Evidence found is kept for the stay; the cells' `identify` is relational (it
-- asks after Komplete Kontrol too) and runs at every evaluation, and a walk of the window's
-- accessibility tree at every one of those — every 500 ms per library module — is what this
-- keeps it from costing. A MISS is not kept at once: a Kontakt that refuses every accessibility
-- question while it starts (the third session: its first minutes) or whose header has not been
-- drawn yet looks exactly like a window that is no Kontakt. So a miss is counted, and asked
-- again at the next evaluation — a library module's 500 ms poll, a focus event, Cmd+Shift+F6 —
-- until MISSES_MAX evaluations of one stay have found nothing; then "not this version" is kept
-- for the rest of the stay, the way the runtime keeps an `identify` that could not tell eight
-- times in a row. A count, not a clock. Text read at FILE's place that is not Kontakt's top row
-- is a real answer — some other plug-in draws there — and kept at once; a read that failed
-- (pushed out, the recogniser not answering) is no answer at all and is not even counted. What
-- is kept is the corner's OFFSET from the DAW's origin, never a screen rectangle: the panel
-- moves with the window, and a rectangle kept here would have put every click one window-move
-- off.
--
-- `name` and `at` come from the cell: the button to look for and where the authored geometry
-- puts its centre. A version without a measured anchor passes nil and stays inert here.
-- `header` is the pair of words the text fallback looks for — first where `at` is, the second
-- to its right — or nil for a version whose top row has not been measured as text (Kontakt 8).
-- `near` is how far from `at` the button may be, { across, down }; ANCHOR_NEAR both ways when
-- the cell gives none.
--
-- Whether this VM writes the anchor lines. Kontakt's code runs in its own VM and again inside
-- every library module that depends on it (four today), each with its own copy of this memo, so
-- one window change wrote every anchor line five times. Only Kontakt's own VM logs — see
-- `setLogAnchors`, called from its `activate`, which no dependent runs.
--
-- The cost, stated so nobody reads silence as "nothing changed": while a LIBRARY overlay holds
-- the slot, Kontakt's own overlays are outranked and skip their rechecks, so a panel that is
-- anchored then is anchored by the library's VM, which does not log. The line comes once
-- Kontakt's VM evaluates that window again. Telling the winner apart would need
-- `host.arbiter`, which this module does not declare.
local logAnchors = false
local ANCHOR_NEAR = 24
local MISSES_MAX = 8
-- How far right of the first header word its neighbour's centre lies (FILE to LIBRARY: 55).
local NEIGHBOUR_GAP_MIN, NEIGHBOUR_GAP_MAX = 35, 90

-- FILE and LIBRARY side by side on one row, among `words`: FILE's centre, or nil.
local function findHeaderWord(words, first, second)
  for _, a in ipairs(words or {}) do
    if string.upper(a.text or "") == first then
      local ay = a.y + a.h / 2
      for _, b in ipairs(words) do
        if string.upper(b.text or "") == second then
          local gap = (b.x + b.w / 2) - (a.x + a.w / 2)
          local by = b.y + b.h / 2
          if gap >= NEIGHBOUR_GAP_MIN and gap <= NEIGHBOUR_GAP_MAX
            and math.abs(by - ay) <= math.max(a.h, b.h) / 2 then
            return math.floor(a.x + a.w / 2 + 0.5), math.floor(ay + 0.5)
          end
        end
      end
    end
  end
  return nil
end

local function onHostPanel(version, name, at, header, near)
  near = near or { ANCHOR_NEAR, ANCHOR_NEAR }
  -- The evidence for the stay in progress, one slot (one window is in front at a time):
  -- { key, corner = { dx, dy } from the DAW's origin to Kontakt's corner, or false }.
  local held = nil
  -- Evaluations of the stay `misses.key` that found no evidence yet.
  local misses = { key = nil, n = 0 }
  -- The stay the last evaluation was about, and the one whose header read is out (nil: none).
  -- One read at a time: while it is out nothing is searched, and an answer that comes back for a
  -- stay that is over is dropped — the stay that is on reads for itself.
  local current, reading = nil, nil
  -- What was last said of each kind, so a line comes once per window and size and not once per
  -- stay; `ANCHOR` is keyed on the corner too, so a corner that moves is said again.
  local said = {}
  local function sayOnce(kind, key, fmt, ...)
    if not logAnchors or said[kind] == key then return end
    said[kind] = key
    host.log.info(string.format(fmt, ...))
  end

  -- One more evaluation of stay `key` without evidence. True when that makes MISSES_MAX, and
  -- "not this version" is then kept for the rest of the stay.
  local function missed(key)
    if misses.key ~= key then misses.key, misses.n = key, 0 end
    misses.n += 1
    if misses.n < MISSES_MAX then return false end
    held = { key = key, corner = false }
    return true
  end

  -- The panel from Kontakt's corner, `d` from the DAW's origin, to the window's edges.
  local function refined(active, panel, d)
    local x, y = panel.client.x + d[1], panel.client.y + d[2]
    local w, h = active.client.x + active.client.w - x, active.client.y + active.client.h - y
    if w <= 0 or h <= 0 then return nil end
    return {
      id = panel.id,
      app = panel.app,
      class = panel.class,
      variant = version,
      bounds = { x = x, y = y, w = w, h = h },
      client = { x = x, y = y, w = w, h = h },
    }
  end

  -- The anchor line, the evidence a Mac session is after.
  local function sayAnchor(active, panel, winKey, how, px, py, d)
    sayOnce("anchor", winKey .. "|" .. d[1] .. "," .. d[2],
      "[kontakt] %s in '%s' (%dx%d): %s at %d,%d puts Kontakt's corner %d,%d from the DAW's plug-in origin, "
        .. "which is %d,%d from the content origin",
      version, tostring(active.title), active.client.w, active.client.h, how, px, py, d[1], d[2],
      panel.client.x - active.client.x, panel.client.y - active.client.y)
  end

  -- For a generic name, the header's second word published beside the button `p` too.
  local function neighbourOf(active, p)
    local q = host.element.locate(active.id, header[2], host.element.type.Button)
    if not q then return false end
    local gap = q.x - p.x
    return gap >= NEIGHBOUR_GAP_MIN and gap <= NEIGHBOUR_GAP_MAX and math.abs(q.y - p.y) <= 12
  end

  -- Starts the read of Kontakt 7's top row for stay `key`; the answer is kept for that stay, or
  -- counted as a miss when nothing was read.
  local function readHeader(active, panel, key, winKey)
    local ex, ey = panel.client.x + at[1], panel.client.y + at[2]
    local cx1, cy1 = active.client.x, active.client.y
    local cx2, cy2 = cx1 + active.client.w, cy1 + active.client.h
    local x1 = math.floor(math.max(cx1, ex - 30 - ANCHOR_NEAR))
    local y1 = math.floor(math.max(cy1, ey - 12 - ANCHOR_NEAR))
    local x2 = math.floor(math.min(cx2, ex + 96 + ANCHOR_NEAR))
    local y2 = math.floor(math.min(cy2, ey + 12 + ANCHOR_NEAR))
    if x2 <= x1 or y2 <= y1 then
      -- No room for Kontakt's top row in this window at all: a real answer.
      held = { key = key, corner = false }
      return
    end
    reading = key
    sayOnce("read", winKey,
      "[kontakt] no %s button of %s's found by accessibility in '%s' (%dx%d) — reading %s %s at "
        .. "%d,%d %dx%d instead",
      name, version, tostring(active.title), active.client.w, active.client.h, header[1], header[2],
      x1, y1, x2 - x1, y2 - y1)
    host.ocr.recognize({ x1, y1, x2, y2 }, { lang = "en", key = "kontakt-header:" .. winKey }, function(r)
      if reading == key then reading = nil end
      -- Superseded by a read of a later stay of the same window, or a stay that is over by now:
      -- the stay that is on reads for itself.
      if r.status == "stale" or current ~= key then return end
      local fx, fy = nil, nil
      if r.status == "text" then fx, fy = findHeaderWord(r.words, header[1], header[2]) end
      if fx and math.abs(fx - ex) <= near[1] and math.abs(fy - ey) <= near[2] then
        local d = { fx - ex, fy - ey }
        held = { key = key, corner = d }
        sayAnchor(active, panel, winKey, "its header text " .. header[1], fx, fy, d)
        host.window.recheck()
        return
      end
      if r.status == "failed" then return end
      local kept = r.status == "text"
      if kept then
        held = { key = key, corner = false }
      else
        kept = missed(key)
      end
      if kept then
        sayOnce("unread", winKey,
          "[kontakt] no %s %s row where %s's would be in '%s' (%s%s) — not %s; asked again when the "
            .. "keyboard next comes into the plug-in",
          header[1], header[2], version, tostring(active.title), tostring(r.status),
          r.error and (": " .. r.error) or "", version)
      end
    end)
  end

  return function(active, panel)
    if not (name and at and panel and active and active.id and active.client) then return nil end
    local winKey = tostring(active.id) .. "|" .. tostring(active.client.w) .. "x" .. tostring(active.client.h)
    local key = winKey .. "|" .. tostring(active.title) .. "|"
      .. (panel.client.x - active.client.x) .. "," .. (panel.client.y - active.client.y)
      .. "|" .. tostring(panel.stay)
    current = key
    if held and held.key == key then
      return held.corner and refined(active, panel, held.corner) or nil
    end
    if reading == key then return nil end
    local ex, ey = panel.client.x + at[1], panel.client.y + at[2]
    local p = host.element.locate(active.id, name, host.element.type.Button)
    if p then
      local d = { p.x - ex, p.y - ey }
      if math.abs(d[1]) <= near[1] and math.abs(d[2]) <= near[2] then
        if not header or neighbourOf(active, p) then
          held = { key = key, corner = d }
          sayAnchor(active, panel, winKey, name, p.x, p.y, d)
          return refined(active, panel, d)
        end
        sayOnce("alone", winKey,
          "[kontakt] the button named %s in '%s' is where %s's would be, but no button named %s "
            .. "beside it — not taken for %s's",
          name, tostring(active.title), version, header[2], version)
      else
        sayOnce("elsewhere", winKey,
          "[kontakt] the button named %s in '%s' is at %d,%d, %d,%d from where %s's would be in the "
            .. "DAW's plug-in panel — not taken for %s's",
          name, tostring(active.title), p.x, p.y, d[1], d[2], version, version)
      end
    end
    if header then
      readHeader(active, panel, key, winKey)
      return nil
    end
    if not p then
      sayOnce("miss", winKey,
        "[kontakt] '%s' (%dx%d) publishes no button named %s — no %s there yet; asked again at the "
          .. "next evaluation, up to %d in a row",
        tostring(active.title), active.client.w, active.client.h, name, version, MISSES_MAX)
    end
    missed(key)
    return nil
  end
end

-- Which VIEW Kontakt is in — the modern one, or the classic rack.
--
-- Nothing accessible can be asked, so it is read from a single pixel of Kontakt's OWN chrome,
-- just below the top bar at the right edge. The classic view draws its rack's header bar there,
-- and the bar's last button (the rack's collapse button; in the instrument editor, its arrow to
-- the next instrument) has its face 10..26 px in from the edge; the play view draws its
-- instrument's scroll bar, 3..17 px in, with the thumb at 7..12 and the track either side of it.
-- The point is on the button's face, below its glyph, and on the track left of the thumb:
-- (86,86,86) classic against (26,26,26) modern. Its being Kontakt chrome rather than the
-- loaded library's UI is the point — the library changes with every instrument, this does not.
-- The rack and the play view's instrument panel both hang off the right edge, with the side pane
-- added to their left, so it is measured from the right edge.
--
-- The margin across is 2 px, the track's: 3 px right of the point the play view draws the thumb,
-- 71, wherever the instrument is scrolled to its top (19 of the 21 shots), and 3 px left the
-- library's own UI, which can read anything. Down the column there is more: rows 54..62 in the
-- rack (its glyph above, the button's border below), 46..62 in the editor, and 38 to 571 or
-- further in the play view. The play view has only ever been captured 679 px high, where every
-- instrument needed its scroll bar; one that fits a taller window may be drawn without one.
--
-- It used to be an icon in the top bar, 262 px from the right edge on row 24, that only the
-- classic view draws: 99 against the bar's 24. That icon is the leftmost of the bar's right-hand
-- cluster, and the cluster starts with Kontakt's "Update" button while an update is waiting. With
-- Kontakt up to date (the two shots of 2026-10-10) the button is gone, the icon sits 69 px
-- further right, and the old point read the bar's own 24 in BOTH views, which is the play view's
-- answer: the overlay never saw the classic view, though F10 had switched it. Nothing in the top
-- bar moves the rack's header.
--
-- A PROBE of the overlay runtime (O:probe), not a read of its own: this backs `when` predicates,
-- which run on every focus move in the runtime's own coroutine, where nothing may read the
-- screen. The runtime reads it in the handler before a scan, the editor's point below in the same
-- capture, once something acted since the last reading — our F10, a click in Kontakt's menu, a
-- window coming forward — or the check 250 ms after an overlay came to the front turned the state
-- generation over. Tabs in a row read nothing. But the first key after our F10 may come before
-- Kontakt has redrawn its header, and read once per act, that reading stood for the whole act: the
-- ring kept offering "Switch to classic view" in the classic view, with the rack's controls hidden,
-- and a second Return switched Kontakt back (NVDA session, 2026-10-10). So the view toggle and the
-- instrument editor's button say that they change what these probes show (O:expectChange), and
-- the runtime reads them again at every key until a reading shows the change: a reading taken too
-- early is replaced at the next key, and a Return or an Alt+V reads before it settles which toggle
-- it means. Once the change is read, Tabs in a row read nothing again. As the arbiter brings the
-- overlay to the front nothing can be read, and the last reading stands until the check 350 ms
-- later. The user's own F10, which turns over no input epoch, is read only after the next act or
-- arrival, as before probes.
--
-- A single pixel is a weak signal, so it is used only for what is safe to get wrong: which
-- LABEL a view-toggle carries, and whether a control that needs the classic view is
-- offered. Never to decide where to click.
local VIEW_PROBE_FROM_RIGHT, VIEW_PROBE_Y = 15, 59
local VIEW_PLAY, VIEW_CLASSIC, VIEW_TOLERANCE = 26, 86, 10

-- EDIT MODE, the third view the classic/play question cannot express.
--
-- Kontakt's instrument editor reads as CLASSIC to the probe above — measured, all ten edit shots
-- give it 86, exactly what the rack gives — so every rack control was being offered there, with
-- its coordinates pointing at whatever the editor happens to draw in that spot. That is the
-- defect this exists to fix, and it is not the probe being wrong: two states genuinely share
-- one answer, so a second question is needed rather than a better first one.
--
-- Also right-anchored, and for a reason worth stating: Kontakt's instrument panel has a FIXED
-- width and hangs off the right edge, with the side pane added to its left. So a point measured
-- from the right is at the same place on the panel whether that pane is open or shut, which a
-- left-anchored one is not — 652 px in lands on the "Edit Mode" caption at 1010 wide and at
-- 664 alike. Across 37 classic shots it reads 134 in all 10 of the editor and 71 in all 27 of
-- the rack, spanning 664..1209 px of width and 679..987 of height; the rack of 2026-10-10, with
-- Kontakt up to date, reads 71 too, while its editor has not been captured. The play-view
-- readings vary (14, 24, 36, 200) and do not matter: this question is only asked once the first
-- one has said classic.
local EDIT_PROBE_FROM_RIGHT, EDIT_PROBE_Y = 652, 46
local EDIT_PROBE_VALUE, RACK_PROBE_VALUE, EDIT_PROBE_TOLERANCE = 134, 71, 25

-- Where the view probe reads on screen, against the CONTENT's right edge, not the window frame's:
-- standalone, the frame is 16 px wider than the plugin, which would put the point outside the
-- plug-in, on the window's own frame. nil while the overlay has no origin.
local function viewPoint(o)
  local origin = o:origin()
  if not (origin and origin.client and origin.bounds) then return nil end
  return { origin.client.x + O.contentSize(origin) - VIEW_PROBE_FROM_RIGHT, origin.client.y + VIEW_PROBE_Y }
end

-- What a reading of the view probe says: true for classic, false for play, nil for neither — and
-- the brightness it read.
--
-- MEASURED, and better than its own comment used to claim: the probe point reads brightness 26
-- in the play view and 86 in the classic one, in 58 calibration shots of Kontakt 8 — 21 of the
-- play view (649..1353 px wide, all 679 high), 27 of the rack (664..1209 wide, 679..987 high),
-- 10 of the editor (664..1010 wide, 679..794 high), the two of 2026-10-10 without the Update
-- button and the rest with it — and so does every pixel of the 5x5 around it, so an origin a
-- point or two off reads the same. That is not a weak signal, and
-- it does not flap — the alternation once blamed on it was the VIEW genuinely toggling twice,
-- from a change that fired two mechanisms at once. Recorded so nobody reopens this: a UIA
-- replacement was tried and is WORSE. "Status bar" and "Navigator" do
-- appear in one view each in the calibrator's dump, but that walks the RAW tree, whereas `findAny`
-- is a condition-based search and finds "Status bar" in BOTH — so the named probe answered
-- "classic" always and left the user unable to leave the classic view. Verifying with one
-- traversal and deciding with another is the mistake; the numbers above are from the thing that
-- actually runs.
--
-- ACCEPT ONLY A RECOGNISED READING. This was `> 60`, and a threshold answers which SIDE a number
-- is on without ever asking whether it came from the right world. The first read after our app
-- starts can happen before the plug-in has painted, or while something else is still in front —
-- and the answer was then kept until the user clicked something, which is exactly the "toggle it
-- twice and it comes right" they reported. The two states read 26 and 86 across every calibration
-- shot. Anything else is not this probe's question being answered wrongly, it is a different
-- picture. What the rule cannot catch is a layout that puts one view's value under the point in
-- the other view, and that is how the top bar's icon failed: its play reading was the bar's own
-- background, which the bar has wherever the icon is not.
--
-- WITHIN 10, not the old point's 20. Each of the 1,450 pixels measured (the 5x5 in each of the 58
-- shots) reads exactly 26 or 86, so 10 leaves room only for drift, and it is what keeps the play
-- view's thumb, 71 and 3 px from the point, from counting as the classic view: within 20 of 86 it
-- did, and a point 3 px off in the play view said classic outright; within 10 it is neither, and
-- the last view stands. The rack's own background, also 71, reads as neither too. At 10, nothing
-- within 11 px of the point reads as the other view in any of the 58 shots: in the classic view
-- the nearest is 12 rows down, the rack header's lower edge (34) above the rack's own scroll bar
-- (26); in the play view it is the library's UI, 11..15 px away in 7 shots and not within 20 in
-- the rest.
local function viewOf(p)
  if not p then return nil, nil end
  local v = (p.r + p.g + p.b) / 3
  if math.abs(v - VIEW_PLAY) > VIEW_TOLERANCE and math.abs(v - VIEW_CLASSIC) > VIEW_TOLERANCE then
    return nil, v
  end
  return math.abs(v - VIEW_CLASSIC) <= VIEW_TOLERANCE, v
end

-- The two probes of a Kontakt 8 overlay that offers the view's controls, kept in `ov.state`. An
-- unrecognised reading is not kept: the previous answer stands, and the next scan looks again.
local function viewProbes(ov)
  local view = ov:probe("view", function(o)
    local at = viewPoint(o)
    return at and { at } or nil
  end, { read = function(c, last, pts)
    local classic, v = viewOf(c[1])
    if classic == nil then
      -- WITH THE POINT AND THE WINDOW, because "unrecognised" on its own says only that the
      -- number is wrong, not where it came from. A reading of 255 is not a misread of Kontakt's
      -- chrome, it is a different surface; the only way to tell a stale origin from an unpainted
      -- window from something on top is to record where the pixel was taken.
      local origin = ov:origin() or {}
      local w, h = O.contentSize(origin)
      local cl = origin.client or { x = 0, y = 0 }
      host.log.info(string.format(
        "[view] unrecognised reading %s at (%d,%d) — origin %d,%d %dx%d, foreground %s — keeping '%s'",
        v and string.format("%d", v) or "nothing", pts[1][1], pts[1][2], cl.x, cl.y, w, h,
        tostring((host.window.active() or {}).id), last and "classic" or "play"))
      return nil
    end
    -- Only on a CHANGE of answer, so this cannot log per focus move — but with the reading in it,
    -- because "the detection is wrong" and "the detection is right about a state I did not expect
    -- it to see" are different problems and the number tells them apart. Nothing read yet counts
    -- as the play view, as the answer meanwhile does.
    if classic ~= (last == true) then
      local p = c[1]
      host.log.info(string.format("[view] %s (probe at %d,%d reads %d,%d,%d)",
        classic and "classic" or "play", pts[1][1], pts[1][2], p.r, p.g, p.b))
    end
    return classic
  end })
  -- The editor's point, with the view's point first: this question is only asked once the first
  -- one has said classic, and in the play view its answer is "closed" whatever the point reads.
  local edit = ov:probe("editor", function(o)
    local at = viewPoint(o)
    if not at then return nil end
    local origin = o:origin()
    local w = O.contentSize(origin)
    if w <= EDIT_PROBE_FROM_RIGHT then return { at } end
    return { at, { origin.client.x + w - EDIT_PROBE_FROM_RIGHT, origin.client.y + EDIT_PROBE_Y } }
  end, { read = function(c, last, pts)
    local classic = viewOf(c[1])
    -- The view itself not recognised: the view probe has said so, and both look again.
    if classic == nil then return nil end
    if not classic or c[2] == nil then return false end
    -- Same rule as the view probe, and for the same failure: "not 134" was being treated as "the
    -- editor is closed", so a reading taken before the plug-in painted said closed and stuck. The
    -- rack reads 71 and the editor 134; anything else is a different picture.
    local q = c[2]
    local v = (q.r + q.g + q.b) / 3
    if math.abs(v - EDIT_PROBE_VALUE) > EDIT_PROBE_TOLERANCE
      and math.abs(v - RACK_PROBE_VALUE) > EDIT_PROBE_TOLERANCE then
      local origin = ov:origin() or {}
      local w, h = O.contentSize(origin)
      local cl = origin.client or { x = 0, y = 0 }
      host.log.info(string.format(
        "[view] unrecognised editor reading %d at (%d,%d) — origin %d,%d %dx%d — keeping '%s'",
        v, pts[2][1], pts[2][2], cl.x, cl.y, w, h, last and "open" or "closed"))
      return nil
    end
    local open = math.abs(v - EDIT_PROBE_VALUE) <= EDIT_PROBE_TOLERANCE
    if open ~= (last == true) then
      host.log.info(string.format("[view] instrument editor %s (probe reads %d)",
        open and "open" or "closed", v))
    end
    return open
  end })
  ov.state.kontaktView = { view = view, edit = edit }
end

-- Whether overlay `ov` sees Kontakt's classic view (the rack or the editor), and whether that is
-- the instrument editor. Nothing read yet is the play view and a closed editor, as an overlay
-- without the probes (a Kontakt 7's) always answers.
local function isClassicView(ov)
  local p = ov and ov.state and ov.state.kontaktView
  return p ~= nil and p.view() == true
end

local function isEditView(ov)
  local p = ov and ov.state and ov.state.kontaktView
  return p ~= nil and p.edit() == true
end

-- The "Kontakt N" identity element's name under `hwnd`, or nil. Used to scope a
-- two-level UIA search into the plugin's real UI (see actions.luau's pluginLocate).
local function kontaktPaneName(hwnd)
  local i = hwnd and host.element.findAny(hwnd, VARIANT_NAMES, IDENTITY_TYPES)
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
  return host.element.pluginLocate(fg.id, via, "VIEW", U.Button) ~= nil
end

return {
  setLogAnchors = function() logAnchors = true end,
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
  onHostPanel = onHostPanel,
  kontaktPaneName = kontaktPaneName,
  viewProbes = viewProbes,
  isClassicView = isClassicView,
  isEditView = isEditView,
  isRackView = isRackView,
  inOwnDialog = inOwnDialog,
}
