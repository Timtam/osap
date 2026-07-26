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
-- OPTIONAL dependency: Komplete Kontrol, if it's installed. Present → its exports (incl.
-- the standalone-window matcher) are imported into this VM; absent → nil, and Kontakt-in-
-- standalone-KK detection is simply off (bare Kontakt in a DAW never needs KK).
local kk = host.tryRequire("com.platform.komplete-kontrol")

local SLOT = "com.platform.kontakt"
-- A SEPARATE arbiter slot for the STANDALONE Kontakt window (its own top-level window,
-- not a plugin embedded in a host). The standalone header and every library's standalone
-- variant compete here, exactly as the embedded base + libraries compete on SLOT.
local STANDALONE_SLOT = "com.platform.kontakt-standalone"
local U = host.uia.type
local BUTTON, MENUITEM = U.Button, U.MenuItem
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
-- Locate an element inside the FOCUSED Kontakt, exactly the way ReaHotkey does
-- (GetPluginUIAElement + MainElement.FindElement): root at the foreground window, find
-- the element that IS the plugin ("Kontakt N", Window/Pane — the ni::qt::QuickWindow
-- scene root in preference to the content-empty QWindowIcon host), and search within it
-- over the RAW tree. The raw walker is what makes a DAW-embedded Kontakt reachable at
-- all: the condition-based search stops at the Qt fragment boundary, which is why an
-- embedded Kontakt looked like it exposed no accessible content whatsoever.
local function pluginLocate(name, ctype)
  local fg = host.window.active()
  if not (fg and fg.id) then return nil end
  -- Once the identity lookup names the plugin, that IS the container — a miss then means
  -- the element has no reachable content (the embedded case, where the plugin is a UIA
  -- leaf), not that we asked about the wrong version. Retrying the other version's name
  -- would walk the whole DAW window again for nothing, on the main thread, before every
  -- coordinate fallback click. The loop is only for the case it was written for: no
  -- identity resolved, because the condition-based lookup stopped at a Qt fragment
  -- boundary that the raw-walking pluginLocate can still cross.
  local via = kontaktPaneName(fg.id)
  if via then return host.uia.pluginLocate(fg.id, via, name, ctype) end
  for _, v in ipairs(VARIANTS) do
    local p = host.uia.pluginLocate(fg.id, v.name, name, ctype)
    if p then return p end
  end
  return nil
end
local function uiaClick(_, name, ctype)
  local p = pluginLocate(name, ctype)
  if p then
    host.input.click(p.x, p.y)
    return true
  end
  return false
end

-- The update dialog's close (✕) glyph, for the image-based dismiss. Two templates: the
-- Kontakt 8 "What's New" screen, and Kontakt 7's "New version Kontakt 8 available"
-- promo, whose ✕ is drawn differently (the K8 template misses it by a wide margin).
local WHATSNEW_CLOSE = {
  host.path("images/whatsnew-close.png"),
  host.path("images/whatsnew-close-k7.png"),
}
-- Bound the image-based search: the dialog is one-time (shows when Kontakt loads a
-- fresh update) and modal, so a handful of attempts per plugin instance covers the
-- load window; after that we stop scanning so the worker isn't probed on every
-- focus-in for a dialog that will never reappear. Keyed by the Kontakt control id;
-- a reloaded Kontakt gets a new id and a fresh budget. Set high once dismissed.
local whatsNewTries = {}
local WHATSNEW_MAX_TRIES = 6

-- Dismiss Kontakt's one-time update dialog ("What's New" in Kontakt 8, "New version
-- Kontakt 8 available" in Kontakt 7). It matters beyond being a nuisance: it covers the
-- loaded library's UI, so its wordmark can't be found and the library overlay never
-- takes over. Two strategies:
--   • UIA, ReaHotkey-style: find the WhatsNewScreen pane on the Kontakt control and
--     click its close child (WalkTree(2)). Works where Kontakt is accessible at all —
--     i.e. running standalone.
--   • IMAGE search for the close glyph within the Kontakt content area, then one click.
--     Async + single-scale + low tolerance (NOT a multi-scale full-region scan, which
--     would starve the worker); a miss = no dialog up (safe). Each template is tried in
--     turn, since the two versions draw the ✕ differently.
-- The image path runs on ANY UIA miss, not just where UIA is unreachable: Kontakt 7's
-- upsell dialog is not a WhatsNewScreen at all, so the UIA lookup misses it even on a
-- Kontakt 7 embedded in a DAW, which — unlike Kontakt 8 — does expose a UIA tree. A UIA
-- miss therefore never proves the absence of a dialog. Matching demands every pixel
-- within tolerance, and both templates were checked to match exactly once (K7) and never
-- (K8) across a full plugin screenshot, so this cannot stray onto Kontakt's own ✕
-- controls; the per-instance try budget bounds the cost when no dialog is up.
-- No-op when no dialog is up either way.
local function closeWhatsNew(_)
  local kc = findNestedKontakt()
  if not kc then return end
  -- Announce it: a dialog vanishing on its own is otherwise silent, and the user has no
  -- way to tell that something was in the way at all. QUEUED, never interrupting — the
  -- overlay that takes over once the dialog is gone (the library's, now that its wordmark
  -- is visible) announces itself right after, and that announcement must not be stomped.
  local function announce()
    host.speech.output("Update dialog closed", { interrupt = false })
  end
  local p = host.uia.classNavPoint(kc.id, "WhatsNewScreen", U.Pane, 2, 0)
  if p then
    host.input.click(p.x, p.y)
    host.log.info("Kontakt: dismissed update dialog (UIA)")
    announce()
    return
  end
  if not kc.bounds then return end
  local tries = whatsNewTries[kc.id] or 0
  if tries >= WHATSNEW_MAX_TRIES then return end
  whatsNewTries[kc.id] = tries + 1
  local b = kc.bounds
  -- Both glyphs against ONE captured frame (the search takes a template list), rather
  -- than a chain of searches that would each pay a fresh region capture.
  host.screen.imageSearchAsync(
    WHATSNEW_CLOSE,
    { region = { b.x, b.y, b.x + b.w, b.y + b.h }, tolerance = 8 },
    function(hit)
      if not hit then return end
      host.input.click(hit.x + math.floor(hit.w / 2), hit.y + math.floor(hit.h / 2))
      host.log.info("Kontakt: dismissed update dialog (image, template " .. hit.n .. ")")
      announce()
      whatsNewTries[kc.id] = WHATSNEW_MAX_TRIES -- done; stop scanning for this instance
    end
  )
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
local EDIT = U.Edit -- KK's library-browser search field
-- Is the overlay anchored on a KK container (rather than a bare Kontakt)? Memoized per
-- window: the answer is a property of the control, and it is asked on every `when`
-- evaluation, i.e. on every focus move.
local isNested = O.memoByOrigin(isKK)
local function nested(o)
  return isNested(o:origin()) == true
end
-- The inverse: shown only on a BARE Kontakt (no KK wrapping it). Kontakt's file menu /
-- load is the bare way to load instruments; under KK you use KK's browser instead, so
-- the Kontakt header is hidden when nested.
local function bare(o)
  return not nested(o)
end
local function addKKHeader(ov)
  ov:group(nested, function(ov)
    ov:addStaticText("Komplete Kontrol")
    -- KK's menu is a fixed hotspot in KK's chrome at (297,17) relative to KK's RAW
    -- client. It lives in KK's frame, NOT Kontakt's content frame, so `rawOrigin` makes
    -- the overlay skip the content offset for it (the two-frame case): content controls
    -- get the KK→content offset, this chrome control stays in KK's raw client.
    ov:addHotspotButton({
      label = "Komplete Kontrol menu",
      at = { 297, 17 },
      rawOrigin = true,
      hotkey = "Alt+M",
    })
    ov:addCustomButton({
      label = "Search library browser",
      hotkey = "Alt+S",
      onActivate = function(o)
        local h = o:hwnd()
        local p = h and host.uia.locate(h, "", EDIT)
        if p then host.input.click(p.x, p.y) end
      end,
    })
  end)
end

-- Adds the generic, library-independent header (UIA-driven, robust across versions)
-- to `ov` with the functional builders — the same way any overlay is built. The
-- onActivate closures take the ACTIVE overlay `o` (passed at activation), so they
-- work unchanged in a derived/library overlay too.
-- The DAW-embedded Kontakt exposes NO accessible UIA content — not the header buttons,
-- and the file menu it opens is neither a #32768 nor a UIA element, so NVDA can't read
-- it and we can't navigate it by UIA. ReaHotkey's fallback (and now ours) drives the
-- header + menu by COORDINATE, relative to the plugin control client: the KONTAKT menu
-- button (ReaHotkey's 86,19), and the menu items at their fixed positions in the opened
-- menu (measured against the live 8.11 menu). A menu item's action (e.g. "Load...") opens
-- Kontakt's own native, ACCESSIBLE dialog — so the user never needs to read the Qt menu.
-- Which Kontakt is focused right now, by name ("Kontakt 7" / "Kontakt 8"), or nil.
-- ReaHotkey keeps two entirely separate overlays with their own hard-coded numbers;
-- here ONE header serves both and resolves the version live, so a library overlay
-- inherits it unchanged whichever Kontakt it is loaded into. Cached per control id
-- (a Kontakt's version never changes under a fixed handle).
-- Memoized per window, and nil (UIA not ready yet) is deliberately NOT cached — pinning
-- that would leave the overlay permanently wrong about which Kontakt it is driving.
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
local function variantOf(o)
  local ctrl = o and o:origin()
  return ctrl and variantOfOrigin(ctrl) or nil
end

-- Header geometry lives in its own file: pure per-version coordinate tables, which is
-- reference data rather than logic and was the bulk of this module's length.
local geo = host.include("src/geometry.luau")
local GEOMETRY, MENU_ITEM, UIA_FILE_MENU = geo.GEOMETRY, geo.MENU_ITEM, geo.UIA_FILE_MENU

-- A ready-made `when` predicate: shown only on this Kontakt version.
local function isVersion(name)
  return function(o) return variantOf(o) == name end
end

-- Kontakt 8 is the fallback when the version isn't resolved yet (it is the tested one).
local function geom(o, key)
  local g = GEOMETRY[variantOf(o) or "Kontakt 8"]
  return g and g[key]
end

-- Opens the Kontakt file menu: the UIA file-menu button when it's reachable (a
-- standalone / full-UIA Kontakt), else a coordinate click on the header. Returns the
-- origin control + whether UIA was used (so a follow-up item click picks its method).
-- The UIA button's name is version-specific (see geometry.luau).
local function openFileMenu(o)
  local ctrl = o:origin()
  local uiaName = UIA_FILE_MENU[variantOf(o) or "Kontakt 8"]
  if uiaClick(ctrl and ctrl.id, uiaName, BUTTON) then
    host.keys.menuOpen(true)
    return ctrl, true
  end
  local at = geom(o, "fileMenu")
  if ctrl and ctrl.client and at then
    host.input.click(ctrl.client.x + at[1], ctrl.client.y + at[2])
    return ctrl, false
  end
  return nil
end

-- A header control that acts through UIA where that works (a standalone Kontakt is
-- fully accessible) and falls back to ReaHotkey's coordinate for that version.
local function headerAction(uiaName, key)
  return function(o)
    local ctrl = o:origin()
    if uiaClick(ctrl and ctrl.id, uiaName, BUTTON) then return end
    local at = geom(o, key)
    if ctrl and ctrl.client and at then
      host.input.click(ctrl.client.x + at[1], ctrl.client.y + at[2])
    end
  end
end

-- Opens the file menu and selects `item` after it appears — via UIA (standalone) or a
-- coordinate click on the item's fixed menu position (DAW-embedded). Kontakt's own native,
-- accessible dialog then opens, so the user never has to read the (inaccessible) Qt menu.
local function invokeMenuItem(o, item)
  local ctrl, uia = openFileMenu(o)
  if not ctrl then return end
  host.timer.after(250, function()
    if uia then
      uiaClick(ctrl.id, item, MENUITEM)
      return
    end
    local base, at = geom(o, "fileMenu"), MENU_ITEM[item]
    if ctrl.client and base and at then
      host.input.click(ctrl.client.x + base[1] + at[1], ctrl.client.y + base[2] + at[2])
    end
  end)
end

-- The instrument / multi / snapshot arrows, straight from ReaHotkey's Kontakt 8
-- coordinate fallback (MoveToPluginInstrumentButton / MultiButton / SnapshotButton).
-- All are `rawOrigin` — they sit in the PLUGIN CONTROL's own frame, so they must skip
-- both the content offset and a library overlay's landmark anchoring (ReaHotkey measures
-- them off ControlGetPos of the plugin control, whatever library is loaded). Those with
-- `fromRight` are anchored to the control's right edge (ReaHotkey's ControlWidth − N).
local function addHeader(ov, embedded)
  -- ReaHotkey's Kontakt 8 plugin header. Its "Load" / "Save as" / "Reset multi" drive the
  -- file menu; the arrows are direct coordinate hotspots. (Its UIA-first path is dead on a
  -- DAW-embedded Kontakt — verified: the plugin element is a UIA LEAF with no content at
  -- all, even when queried directly — so everything here takes ReaHotkey's fallback route.)
  ov:group(bare, function(ov)
    ov:addCustomButton({
      label = "Load instrument",
      hotkey = "Ctrl+L",
      onActivate = function(o) invokeMenuItem(o, "Load...") end,
    })
    ov:addCustomButton({
      label = "Save multi as",
      hotkey = "Ctrl+S",
      onActivate = function(o) invokeMenuItem(o, "Save multi as...") end,
    })
    ov:addCustomButton({
      label = "Reset multi",
      hotkey = "Ctrl+R",
      onActivate = function(o) invokeMenuItem(o, "Reset multi") end,
    })
    if not embedded then return end
    for _, h in ipairs(geo.ARROWS) do
      ov:addHotspotButton({
        label = h.label,
        -- A fixed pair, or looked up per focused version when the two differ.
        at = h.at or function(o) return geom(o, h.key) end,
        fromRight = h.fromRight,
        rawOrigin = true,
        hotkey = h.hotkey,
      })
    end
    ov:group(isVersion("Kontakt 7"), function(ov)
      for _, h in ipairs(geo.K7_CONTROLS) do
        if h.embedded then
          ov:addHotspotButton({
            label = h.label,
            at = function(o) return geom(o, h.key) end,
            rawOrigin = true,
            hotkey = h.hotkey,
          })
        end
      end
    end)
  end)
end

-- ── Where a Kontakt can be ───────────────────────────────────────────────────
-- Two BINDINGS, declared once as values. `HOSTED` is a Kontakt embedded in a host window;
-- `STANDALONE` is Kontakt running as its own application. Every overlay here — the base
-- header, each library variant, the Content Missing dialog — binds to one of these, or to
-- a `:with{…}` variant of one, instead of rebuilding an attach spec per attachment.
--
-- Hosts = the DAWs (from daw-hosts) PLUS, when Komplete Kontrol is installed, KK's own
-- standalone window (imported via the optional `kk` dependency): KK running on its own
-- hosts a Kontakt exactly like a DAW does (verified live — the KK NINormalWindow has the
-- SAME two Qt children as DAW-embedded KK: the NI-versioned KK container control and,
-- inside it, the plain-Qt nested Kontakt — so the same relational identity + dynamic
-- KK→content offset apply). We reuse KK's OWN window matcher (kk.standaloneWindow) rather
-- than treating KK as a DAW or duplicating its identity; if KK isn't installed, `kk` is
-- nil and O.hosts simply omits it. In a DAW the KK lives inside a non-KK host, so
-- daw.all still handles the DAW-embedded case (with its chrome gate) unchanged.
local HOSTED = O.embedded({
  hosts = O.hosts(daw.all, kk and kk.standaloneWindow),
  control = QT_PLUGIN,
  identify = identifyHost,
  -- identifyHost is RELATIONAL (a bare Kontakt defers to a wrapping KK, a KK only
  -- counts when it hosts Kontakt), so it must NOT be cached per HWND — KK's
  -- container HWND is stable while a Kontakt is loaded into it.
  cacheIdentity = false,
}, { slot = SLOT, menus = true })

-- Kontakt's "Content Missing" dialog — shown when a library's samples aren't where
-- Kontakt expects them, and a hard stop until you point it at the folder. ReaHotkey
-- gives it its own overlay with the one control that matters, and so do we. Two shapes:
--   • embedded — the dialog is an NIChildWindow CONTROL inside the host window (the
--     enumeration only reports VISIBLE controls, so its presence IS the dialog being up);
--   • standalone — a top-level #32770 titled "Content Missing".
-- Both join their Kontakt slot at the highest specificity, so the dialog overlay takes
-- over from the header/library overlay while it is up and hands back when it closes.
local CONTENT_MISSING_CONTROL = "^NIChildWindow%x+$"
local CONTENT_MISSING_WINDOW = {
  title = { pattern = "^Content Missing" },
  app = { exe = { contains = "Kontakt" } },
  windows = { class = { prefix = "#32770" } },
}
local function contentMissingOverlay()
  local ov = O.new("Content Missing")
  -- ReaHotkey's hotspot, relative to the dialog's own top-left.
  ov:addHotspotButton({ label = "Browse For Folder", at = { 218, 341 }, hotkey = "Alt+B" })
  return ov
end

-- The Kontakt STANDALONE application window (Kontakt N.exe running on its own, not
-- embedded in a DAW): a top-level NINormalWindow titled "Kontakt 7" / "Kontakt 8".
-- Mirrors ReaHotkey's Standalone.Register criteria. Unlike a KK-nested Kontakt (a UIA
-- leaf), a standalone Kontakt is fully UIA-accessible.
local KONTAKT_STANDALONE = {
  title = { pattern = "^Kontakt %d" },
  app = { exe = { contains = "Kontakt" } },
  windows = { class = { prefix = "NINormalWindow" } },
}
local STANDALONE = O.window(KONTAKT_STANDALONE, { slot = STANDALONE_SLOT, menus = true })

-- Builds a library overlay — the inherited Kontakt header plus the library's own controls,
-- gated on `landmark` — and binds it via `cfg.attach(ov)`. Shared by the EMBEDDED variant
-- (in a DAW / nested in KK: the KK chrome header, the dynamic host/version offset) and the
-- STANDALONE variant (Kontakt's own window: bare header, plain version offset). `build(ov)`
-- adds the library's controls the same way for both. Runs in the LIBRARY module's VM.
local function buildLibraryOverlay(name, landmark, build, cfg)
  local ov = O.new("Kontakt — " .. (name or "library"))
  if cfg.kkHeader then addKKHeader(ov) end
  addHeader(ov, cfg.embedded)
  ov._originOffset = cfg.offset
  ov:onActivate(onPluginActive)
  if build then build(ov) end
  -- Anchor coordinates to the landmark: a library's controls are authored relative to
  -- its on-screen wordmark, so they stay correct across every host (DAW / KK / standalone)
  -- and Kontakt browser state, without any content/version/chrome offset.
  if landmark then ov:landmark(landmark, { anchor = true }) end
  cfg.attach(ov)
  return ov
end

return {
  -- Standalone side effect, run only in Kontakt's own VM: the base overlay.
  activate = function()
    local ov = O.new("Kontakt")
    addKKHeader(ov)
    addHeader(ov, true)
    -- On focus: dismiss the one-time What's New dialog + auto-close KK's browser.
    ov:onActivate(onPluginActive)
    -- Specificity 1: above the Komplete Kontrol chrome overlay (0, com.platform
    -- .komplete-kontrol) on the shared slot, below a library (2).
    ov:bind(HOSTED, { specificity = O.layer.base })
    host.log.info("Kontakt base overlay ready.")

    -- STANDALONE overlay: Kontakt running as its OWN application (Kontakt N.exe), not
    -- embedded in a DAW — a top-level NINormalWindow titled "Kontakt N". A SEPARATE
    -- overlay from the embedded one, attached to the top-level window via attach().
    -- Mirrors ReaHotkey's Kontakt StandaloneHeader: the Kontakt file menu (UIA-driven,
    -- reusing the same "Kontakt File Menu" UIA button as the embedded header) plus a Tab
    -- pass-through of the native UIA tree — a standalone Kontakt is fully UIA-accessible,
    -- unlike a KK-nested one (which has no UIA tree). The pass-through (below) drives
    -- Kontakt's own keyboard focus via host.uia.focusStep.
    local sa = O.new("Kontakt")
    sa:addCustomButton({
      label = "Kontakt file menu",
      hotkey = "Alt+F",
      -- Version-aware: the UIA button is "FILE" in Kontakt 7, "Kontakt File Menu" in
      -- Kontakt 8. Unlike embedded, a standalone Kontakt's menu IS accessible, so the
      -- menu watch hands Tab/arrows straight to it — this button is useful here.
      onActivate = function(o) openFileMenu(o) end,
    })
    -- Kontakt 7's standalone header carries three more buttons that Kontakt 8's does
    -- not (ReaHotkey lists them for K7 only); shown only when a Kontakt 7 is focused.
    sa:group(isVersion("Kontakt 7"), function(ov)
      for _, h in ipairs(geo.K7_CONTROLS) do
        ov:addCustomButton({
          label = h.label,
          hotkey = h.hotkey,
          onActivate = headerAction(h.uia, h.key),
        })
      end
    end)
    -- Tab / Shift+Tab step through Kontakt's OWN focusable controls (its window doesn't
    -- move focus on Tab itself) — ReaHotkey's Standalone Tab pass-through.
    sa:enableUiaPassThrough()
    -- Join the standalone slot at the LOWEST specificity, so a library's standalone
    -- variant (specificity 2, gated on its landmark) takes over when its library is
    -- loaded, and this header is the resting default otherwise.
    sa:bind(STANDALONE, { specificity = O.layer.chrome })
    host.log.info("Kontakt standalone overlay ready.")

    -- The Content Missing dialog, embedded + standalone. Specificity 3 beats the header
    -- (0/1) and any library (2) on the same slot, so while the dialog is up it IS the
    -- overlay — and the arbiter hands back automatically once it closes.
    contentMissingOverlay():bind(
      HOSTED:with({
        control = CONTENT_MISSING_CONTROL,
        identify = function() return kontaktPresent() end,
        opts = { menus = false }, -- nothing here opens a plugin menu
      }),
      { specificity = O.layer.dialog }
    )
    contentMissingOverlay():bind(
      O.window(CONTENT_MISSING_WINDOW, { slot = STANDALONE_SLOT }),
      { specificity = O.layer.dialog }
    )
    host.log.info("Kontakt Content Missing overlays ready.")
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
    -- EMBEDDED variant (in a DAW, or nested in Komplete Kontrol) — the ReaHotkey-faithful
    -- case. Includes the KK chrome header (shown only while nested) and the dynamic host
    -- offset. Match = Kontakt focused (embedded context) AND this library's landmark
    -- visible (re-checked on the poll, since a library loads/unloads with no window
    -- event). Specificity 2 wins over the Kontakt base (1) and the KK chrome (0).
    local ov = buildLibraryOverlay(name, landmark, build, {
      kkHeader = true,
      embedded = true,
      offset = resolveOffset,
      attach = function(o)
        o:bind(HOSTED, { specificity = O.layer.content, pollMatch = 500 })
      end,
    })

    -- STANDALONE variant (Kontakt N.exe on its own) — BEYOND ReaHotkey, which shows only a
    -- header there. The SAME library controls over the top-level Kontakt window: bare
    -- header (never nested in KK), origin = the window client + the K7/K8 version offset.
    -- Its own slot; specificity 2 wins over the standalone header (0) whenever this
    -- library's landmark is on screen, else the header is the resting default.
    buildLibraryOverlay(name, landmark, build, {
      kkHeader = false,
      offset = standaloneOffset,
      attach = function(o)
        o:bind(STANDALONE, { specificity = O.layer.content, pollMatch = 500 })
      end,
    })

    host.log.info("Kontakt library overlay ready: " .. (name or "library"))
    return ov
  end,
}
