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
-- Detection (which window is a Kontakt, which version, is a Komplete Kontrol wrapping it,
-- where its content begins), the header ACTIONS (drive the plugin by UIA or coordinate,
-- work the file menu, dismiss the update dialog) and the per-version GEOMETRY each live in
-- their own file. What stays here is the wiring: which overlays exist, what they contain,
-- and where they attach.
local D = host.include("src/detect.luau")
local A = host.include("src/actions.luau")
local geo = host.include("src/geometry.luau")

local QT_PLUGIN, IDENTITY_TYPES = D.QT_PLUGIN, D.IDENTITY_TYPES
local isKK, kontaktPresent, identifyHost = D.isKK, D.kontaktPresent, D.identifyHost
local resolveOffset, standaloneOffset = D.resolveOffset, D.standaloneOffset
local uiaClick, onPluginActive = A.uiaClick, A.onPluginActive
local GEOMETRY, MENU_ITEM, UIA_FILE_MENU = geo.GEOMETRY, geo.MENU_ITEM, geo.UIA_FILE_MENU


-- KK chrome controls, prepended to the Kontakt/library overlay but SHOWN ONLY while
-- nested in KK (`when = nested`) — so while a library is loaded you can still reach
-- KK's menu and library browser to load a DIFFERENT (incl. non-Kontakt) instrument.
-- This is ReaHotkey's "KK header on top of the plugin overlay" composition, realized
-- as conditional controls in one nav list (KK → Kontakt → library). They act on the
-- matched control (the KK container when nested).
local EDIT = U.Edit -- KK's library-browser search field
local function nested(o)
  return D.isNested(o:origin()) == true
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
local function variantOf(o)
  local ctrl = o and o:origin()
  return ctrl and D.variantOfOrigin(ctrl) or nil
end

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
