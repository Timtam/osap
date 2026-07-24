-- Kontakt framework, as a code module (com.platform.kontakt). It provides:
--   • a BASE overlay — the generic, library-independent header (open the file
--     menu, load an instrument) — active whenever Kontakt is focused in a DAW.
--     Created by activate(), which the host calls only in Kontakt's own VM.
--   • library(spec) — called by a sample-library module that DEPENDS on Kontakt
--     (so this file is evaluated in that library's VM): it builds an INHERITING
--     overlay (the same header plus the library's controls) that takes over from
--     the base whenever the library's landmark is on screen.
-- Base and every library overlay compete on one arbiter slot; the library (higher
-- specificity) wins when its landmark is present, otherwise the base. Library
-- controls are authored in the Kontakt 7 coordinate frame; Kontakt 8 shifts
-- content down 29 px and the per-control origin offset adapts them automatically.

local O = host.require("com.platform.overlay")
local daw = host.require("com.platform.daw-hosts")

local SLOT = "com.platform.kontakt"
local BUTTON, MENUITEM = 50000, 50011

-- (The KK library-browser auto-close + its opt-out setting now live in the Komplete
-- Kontrol module: an open browser hides the nested Kontakt, so the KK overlay — not
-- this one — is the overlay active then, and is where the close belongs.)

-- A Kontakt version's identity (UIA name + Window/Pane control type) + the content
-- offset its libraries need.
local function isKontakt(version)
  return function(c)
    return host.uia.find(c.id, version, 50032) or host.uia.find(c.id, version, 50033)
  end
end
local VARIANTS = {
  { name = "Kontakt 8", offset = { 0, 29 }, identify = isKontakt("Kontakt 8") },
  { name = "Kontakt 7", offset = { 0, 0 }, identify = isKontakt("Kontakt 7") },
}
local function anyKontakt(c)
  for _, v in ipairs(VARIANTS) do
    if v.identify(c) then return true end
  end
  return false
end

-- ── Komplete Kontrol nesting ──────────────────────────────────────────────────
-- KK can itself host a Kontakt instance (DAW → KK → Kontakt → library). KK and
-- Kontakt share the Qt control class and differ only by UIA name. EnumChildWindows
-- enumerates the nested Kontakt control too, but ReaHotkey anchors a nested library
-- on KK's control and crosses KK's chrome with a fixed content offset — we do the
-- same, so the SAME library layout (authored in the bare-Kontakt frame) lands
-- correctly without the library knowing anything about KK.
local function isKK(c)
  return host.uia.find(c.id, "Komplete Kontrol", 50032) or host.uia.find(c.id, "Komplete Kontrol", 50033)
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
    if string.match(c.class, "Qt%d+.-QWindowIcon") and test(c) then return true end
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
    if string.match(c.class, "Qt%d+.-QWindowIcon") and anyKontakt(c) and not isKK(c) then
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
      local off = { kc.client.x - c.client.x, kc.client.y - c.client.y }
      offsetCache[key] = off
      host.log.info(string.format(
        "Kontakt nested in Komplete Kontrol — dynamic content offset (%d, %d) "
          .. "[Kontakt.client (%d,%d) − KK.client (%d,%d)]",
        off[1], off[2], kc.client.x, kc.client.y, c.client.x, c.client.y
      ))
      return off[1], off[2]
    end
    return KK_OFFSET[1], KK_OFFSET[2] -- not resolvable yet: transient, NOT cached
  end
  local cached = offsetCache[c.id]
  if cached then return cached[1], cached[2] end
  for _, v in ipairs(VARIANTS) do
    if v.identify(c) then
      offsetCache[c.id] = v.offset
      return v.offset[1], v.offset[2]
    end
  end
  return 0, 0 -- version not yet identified (UIA not ready): transient, NOT cached
end

-- The plugin control to anchor on: prefer a KK container that is hosting Kontakt
-- (so the KK offset places the library), else a bare Kontakt with no KK wrapping it.
local function identifyHost(c)
  if isKK(c) then return kontaktPresent() end
  if anyKontakt(c) then return not kkPresent() end
  return false
end

-- The focused Kontakt plugin window handle, from an overlay's active context.
local function pluginHwnd(ov)
  local ctrl = ov.activeCtx and ov.activeCtx.origin()
  return ctrl and ctrl.id
end
local function uiaClick(hwnd, name, ctype)
  if not hwnd then return false end
  local p = host.uia.locate(hwnd, name, ctype)
  if p then
    host.input.click(p.x, p.y)
    return true
  end
  return false
end

-- The What's New dialog's close (✕) glyph, for the image-based nested dismiss.
local WHATSNEW_CLOSE = host.path("images/whatsnew-close.png")
-- Bound the image-based search: the dialog is one-time (shows when Kontakt loads a
-- fresh update) and modal, so a handful of attempts per plugin instance covers the
-- load window; after that we stop scanning so the worker isn't probed on every
-- focus-in for a dialog that will never reappear. Keyed by the Kontakt control id;
-- a reloaded Kontakt gets a new id and a fresh budget. Set high once dismissed.
local whatsNewTries = {}
local WHATSNEW_MAX_TRIES = 6

-- Dismiss Kontakt's one-time "What's New" update dialog. Two strategies:
--   • Bare Kontakt — ReaHotkey-style UIA: find the WhatsNewScreen pane on the Kontakt
--     control and click its close child (WalkTree(2)).
--   • Nested in KK — Kontakt exposes NO UIA tree (its "Kontakt N" control is a UIA leaf,
--     the same reason nested detection is image-based), so UIA finds nothing; fall back
--     to an IMAGE search for the close glyph within the nested Kontakt content area and
--     click it. Async + single-scale + low tolerance + one click (NOT a multi-scale
--     full-region scan, which would starve the worker); a miss = no dialog up (safe).
-- No-op when no dialog is up either way.
local function closeWhatsNew(o)
  local kc = findNestedKontakt()
  if not kc then return end
  local p = host.uia.classNavPoint(kc.id, "WhatsNewScreen", 50033, 2, 0)
  if p then
    host.input.click(p.x, p.y)
    host.log.info("Kontakt: dismissed What's New dialog (UIA)")
    return
  end
  -- Image fallback ONLY when nested in KK: there UIA can't reach Kontakt, so a UIA
  -- miss is inconclusive and the dialog must be found by image. Bare Kontakt HAS a
  -- UIA tree, so a UIA miss means there is genuinely no dialog — don't run a scan
  -- (avoids needless worker probes and any image false-click) on the bare path.
  local matched = o and o.activeCtx and o.activeCtx.origin()
  if matched and isKK(matched) and kc.bounds then
    local tries = whatsNewTries[kc.id] or 0
    if tries >= WHATSNEW_MAX_TRIES then return end
    whatsNewTries[kc.id] = tries + 1
    local b = kc.bounds
    host.screen.imageSearchAsync(
      WHATSNEW_CLOSE,
      { region = { b.x, b.y, b.x + b.w, b.y + b.h }, tolerance = 8 },
      function(hit)
        if hit then
          host.input.click(hit.x + math.floor(hit.w / 2), hit.y + math.floor(hit.h / 2))
          host.log.info("Kontakt: dismissed What's New dialog (image)")
          whatsNewTries[kc.id] = WHATSNEW_MAX_TRIES -- done; stop scanning for this instance
        end
      end
    )
  end
end

-- Fired whenever the Kontakt base/library overlay becomes focused: dismiss the one-time
-- What's New dialog (no-op when none is up). Closing KK's library browser is handled by
-- the Komplete Kontrol overlay, which is the one active while the browser hides Kontakt.
local function onPluginActive(o)
  closeWhatsNew(o)
end

-- KK chrome controls, prepended to the Kontakt/library overlay but SHOWN ONLY while
-- nested in KK (`when = nested`) — so while a library is loaded you can still reach
-- KK's menu and library browser to load a DIFFERENT (incl. non-Kontakt) instrument.
-- This is ReaHotkey's "KK header on top of the plugin overlay" composition, realized
-- as conditional controls in one nav list (KK → Kontakt → library). They act on the
-- matched control (the KK container when nested).
local EDIT = 50004 -- UIA Edit control type: KK's library-browser search field
local nestedCache = {}
local function nested(o)
  local ctrl = o.activeCtx and o.activeCtx.origin()
  if not ctrl then return false end
  local v = nestedCache[ctrl.id]
  if v == nil then
    v = isKK(ctrl)
    nestedCache[ctrl.id] = v
  end
  return v == true
end
-- The inverse: shown only on a BARE Kontakt (no KK wrapping it). Kontakt's file menu /
-- load is the bare way to load instruments; under KK you use KK's browser instead, so
-- the Kontakt header is hidden when nested.
local function bare(o)
  return not nested(o)
end
local function addKKHeader(ov)
  ov:addStaticText("Komplete Kontrol").when = nested
  -- KK's menu is a fixed hotspot in KK's chrome at (297,17) relative to KK's RAW
  -- client. It lives in KK's frame, NOT Kontakt's content frame, so `rawOrigin` makes
  -- the overlay skip the content offset for it (the two-frame case): content controls
  -- get the KK→content offset, this chrome control stays in KK's raw client.
  ov:addHotspotButton({
    label = "Komplete Kontrol menu",
    at = { 297, 17 },
    rawOrigin = true,
    hotkey = "Alt+M",
  }).when = nested
  ov:addCustomButton({
    label = "Search library browser",
    hotkey = "Alt+S",
    onActivate = function(o)
      local h = pluginHwnd(o)
      local p = h and host.uia.locate(h, "", EDIT)
      if p then host.input.click(p.x, p.y) end
    end,
  }).when = nested
end

-- Adds the generic, library-independent header (UIA-driven, robust across versions)
-- to `ov` with the functional builders — the same way any overlay is built. The
-- onActivate closures take the ACTIVE overlay `o` (passed at activation), so they
-- work unchanged in a derived/library overlay too.
local function addHeader(ov)
  ov:addCustomButton({
    label = "Kontakt file menu",
    hotkey = "Alt+F",
    onActivate = function(o)
      if uiaClick(pluginHwnd(o), "Kontakt File Menu", BUTTON) then
        host.keys.menuOpen(true) -- snappier than waiting for the menu-watch poll
      end
    end,
  }).when = bare
  ov:addCustomButton({
    label = "Load instrument",
    hotkey = "Ctrl+L",
    onActivate = function(o)
      local hwnd = pluginHwnd(o)
      if uiaClick(hwnd, "Kontakt File Menu", BUTTON) then
        host.timer.after(200, function() uiaClick(hwnd, "Load...", MENUITEM) end)
      end
    end,
  }).when = bare
end

-- The embedded-attach spec shared by the base and every library overlay. A
-- library's landmark is now an overlay-level gate (ov:landmark), not part of the
-- spec, so this is identical for the base and every library.
local function spec()
  return {
    hosts = daw.all,
    -- NI Qt plugin control class — plain ("Qt652QWindowIcon{GUID}1", e.g. Kontakt)
    -- OR NI-versioned ("Qt671NI_6_7_1_R3QWindowIcon{GUID}", e.g. Komplete Kontrol);
    -- the ".-" spans the optional NI_<version> segment. Loose match; the UIA name
    -- (identifyHost) disambiguates Kontakt — and a KK that hosts it — from other NI
    -- Qt plugins, and anchors a KK-nested Kontakt on the KK container.
    control = "Qt%d+.-QWindowIcon",
    identify = identifyHost,
    -- identifyHost is RELATIONAL (a bare Kontakt defers to a wrapping KK, a KK only
    -- counts when it hosts Kontakt), so it must NOT be cached per HWND — KK's
    -- container HWND is stable while a Kontakt is loaded into it.
    cacheIdentity = false,
  }
end

return {
  -- Standalone side effect, run only in Kontakt's own VM: the base overlay.
  activate = function()
    local ov = O.new("Kontakt")
    addKKHeader(ov)
    addHeader(ov)
    -- On focus: dismiss the one-time What's New dialog + auto-close KK's browser.
    ov:onActivate(onPluginActive)
    -- Specificity 1: above the Komplete Kontrol chrome overlay (0, com.platform
    -- .komplete-kontrol) on the shared slot, below a library (2).
    ov:attachEmbedded(spec(), { slot = SLOT, specificity = 1, naturalNav = true })
    ov:_startMenuWatch()
    host.log.info("Kontakt base overlay ready.")
  end,

  -- Called from a sample-library module (this file runs in its VM): builds an
  -- inheriting overlay that wins over the base when `landmark` is on screen inside
  -- Kontakt. The library author adds its own controls in `build(ov)` with the same
  -- functional builders used everywhere — ov:addStaticText / addHotspotButton /
  -- addGraphicalToggle / addCustomButton — on top of the inherited header; no
  -- declarative control tables. Controls are authored in the Kontakt 7 coordinate
  -- frame; the per-control offset adapts them to the focused Kontakt version.
  --
  -- `landmark` (and any control on/offImage) MUST be an ABSOLUTE path (host.path) —
  -- imageSearch runs in Kontakt's identity scope, so a relative path would resolve
  -- under Kontakt's root, not the library's.
  library = function(name, landmark, build)
    local ov = O.new("Kontakt — " .. (name or "library"))
    addKKHeader(ov)
    addHeader(ov)
    ov._originOffset = resolveOffset
    ov:onActivate(onPluginActive)
    if build then build(ov) end
    -- Match = Kontakt focused (the embedded context) AND this library's landmark
    -- visible (re-checked on the poll below, since a library loads/unloads inside
    -- Kontakt with no window event).
    if landmark then ov:landmark(landmark) end
    -- Specificity 2: a library wins over both the Kontakt base (1) and the KK chrome
    -- (0) on the shared slot whenever its landmark is present.
    ov:attachEmbedded(spec(), { slot = SLOT, specificity = 2, naturalNav = true, pollMatch = 500 })
    ov:_startMenuWatch()
    host.log.info("Kontakt library overlay ready: " .. (name or "library"))
    return ov
  end,
}
