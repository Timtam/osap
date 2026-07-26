-- Kontakt DETECTION: which window is a Kontakt, which version, whether a Komplete
-- Kontrol is wrapping it, and where its content begins. No overlays and no controls
-- here — just the questions every Kontakt overlay has to ask about a window.

local O = host.require("com.platform.overlay")
local U = host.uia.type

-- A plugin identifies itself through a UIA element named after it, which NI exposes as a
-- Window on some hosts and a Pane on others — so every identity question here asks about
-- both types at once.
local IDENTITY_TYPES = { U.Window, U.Pane }
-- NI Qt plugin control class — plain ("Qt652QWindowIcon{GUID}1", e.g. Kontakt) OR
-- NI-versioned ("Qt671NI_6_7_1_R3QWindowIcon{GUID}", e.g. Komplete Kontrol); the ".-"
-- spans the optional NI_<version> segment. A loose match: the UIA NAME is what tells
-- these plugins apart (see identifyHost).
local QT_PLUGIN = "Qt%d+.-QWindowIcon"

-- (The KK library-browser auto-close + its opt-out setting now live in the Komplete
-- Kontrol module: an open browser hides the nested Kontakt, so the KK overlay — not
-- this one — is the overlay active then, and is where the close belongs.)

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
local function versionOffsetOf(c)
  local i = variantIndex(c)
  if i then return VARIANTS[i].offset[1], VARIANTS[i].offset[2] end
  return 0, 0
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
-- This is computed DYNAMICALLY at runtime (= nested Kontakt.client − KK.client, the
-- exact inner content origin) by resolveOffset; the constant is only a transient
-- fallback for the brief window before the nested Kontakt control is enumerable
-- (e.g. while KK's library browser is still open and hides it). Live value ≈ 198,222
-- (KK 6.7.1 + Kontakt 8.11); the old hardcoded 190,111 landed ~111 px too high.
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

-- Resolve a control's content offset: when anchored on a KK container, the KK→content
-- offset = the nested Kontakt control's client − KK's client, computed LIVE (the old
-- hardcoded constant was wrong and unstable); otherwise the bare Kontakt version
-- offset (K7 = 0, K8 = +29). Both branches cache only once the offset is positively
-- resolved, so a transient miss (KK browser still hiding Kontakt; UIA not ready right
-- after activation) returns a fallback WITHOUT pinning it. The KK cache key includes
-- the container's size: the offset is invariant under window MOVES (both client
-- origins shift together, so it stays valid and cached) but KK can reposition the
-- embedded plugin on a RESIZE/reflow, which changes the size and so re-derives.
local offsetCache = {}
local function resolveOffset(c)
  if isKK(c) then
    local key = c.id .. "|" .. (c.bounds and (c.bounds.w .. "x" .. c.bounds.h) or "?")
    local cached = offsetCache[key]
    if cached then return cached[1], cached[2] end
    local kc = findNestedKontakt(c)
    if kc and kc.client and c.client then
      -- KK chrome offset (nested Kontakt.client − KK.client) PLUS the nested Kontakt's
      -- version offset — libraries are authored in the K7 frame, so K8 needs +29 here
      -- exactly as in the bare/standalone paths (this branch used to omit it, so a
      -- KK-nested library sat 29 px too high on Kontakt 8).
      local vx, vy = versionOffsetOf(kc)
      local off = { kc.client.x - c.client.x + vx, kc.client.y - c.client.y + vy }
      offsetCache[key] = off
      host.log.info(string.format(
        "Kontakt nested in Komplete Kontrol — dynamic content offset (%d, %d) "
          .. "[Kontakt.client (%d,%d) − KK.client (%d,%d) + version (%d,%d)]",
        off[1], off[2], kc.client.x, kc.client.y, c.client.x, c.client.y, vx, vy
      ))
      return off[1], off[2]
    end
    return KK_OFFSET[1], KK_OFFSET[2] -- not resolvable yet: transient, NOT cached
  end
  local cached = offsetCache[c.id]
  if cached then return cached[1], cached[2] end
  local i = variantIndex(c)
  if i then
    offsetCache[c.id] = VARIANTS[i].offset
    return VARIANTS[i].offset[1], VARIANTS[i].offset[2]
  end
  return 0, 0 -- version not yet identified (UIA not ready): transient, NOT cached
end

-- Content offset for a STANDALONE Kontakt window (Kontakt N.exe on its own). Measured
-- live: the Kontakt content Qt control's client coincides with the window client (no NI
-- menu-bar chrome offset), so the only shift a library authored in the Kontakt 7 frame
-- needs is the bare version offset (K7 = 0, K8 = +29) — the same as resolveOffset's bare
-- branch. The version is read from the "^Kontakt N" title (which the standalone matcher
-- guarantees).
local function standaloneOffset(o)
  local name = string.match((o and o.title) or "", "^(Kontakt %d)")
  for _, v in ipairs(VARIANTS) do
    if v.name == name then return v.offset[1], v.offset[2] end
  end
  return 0, 0
end

-- The plugin control to anchor on: prefer a KK container that is hosting Kontakt
-- (so the KK offset places the library), else a bare Kontakt with no KK wrapping it.
local function identifyHost(c)
  if isKK(c) then return kontaktPresent() end
  if anyKontakt(c) then return not kkPresent() end
  return false
end

-- The "Kontakt N" identity element's name under `hwnd`, or nil. Used to scope a
-- two-level UIA search into the plugin's real UI (see pluginLocate).
local function kontaktPaneName(hwnd)
  local i = hwnd and host.uia.findAny(hwnd, VARIANT_NAMES, IDENTITY_TYPES)
  return i and VARIANTS[i].name or nil
end

-- Which Kontakt the overlay is currently on, by name, or nil. Memoized per window, and
-- nil (UIA not ready yet) is deliberately NOT cached — pinning that would leave the
-- overlay permanently wrong about which Kontakt it is driving.
local variantOfOrigin = O.memoByOrigin(function(ctrl)
  -- Standalone: the window title carries the version. Embedded: identify by UIA, on the
  -- Kontakt itself — or, when anchored on a KK container, on the Kontakt nested in it.
  local name = string.match(tostring(ctrl.title or ""), "^(Kontakt %d)")
  if name then return name end
  local kc = ctrl
  if isKK(ctrl) then kc = findNestedKontakt(ctrl) or ctrl end
  local i = variantIndex(kc)
  return i and VARIANTS[i].name or nil
end)

-- Is the overlay anchored on a KK container rather than a bare Kontakt? Also memoized:
-- it is asked on every `when` evaluation, i.e. on every focus move.
local isNested = O.memoByOrigin(isKK)

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
  identifyHost = identifyHost,
  resolveOffset = resolveOffset,
  standaloneOffset = standaloneOffset,
  kontaktPaneName = kontaktPaneName,
  variantOfOrigin = variantOfOrigin,
  isNested = isNested,
}
