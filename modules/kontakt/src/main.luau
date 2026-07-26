-- Kontakt framework, as a code module (com.platform.kontakt). It provides:
--   • one overlay per CELL — {Kontakt 7, Kontakt 8} × {bare in a DAW, nested in Komplete
--     Kontrol, standalone} — created by activate(), which the host calls only in Kontakt's
--     own VM. A cell knows its version, so its coordinates are plain numbers and its
--     control set is fixed; nothing works out at runtime where it is running.
--   • library(name, landmark, build) — called by a sample-library module that DEPENDS on
--     Kontakt (so this file is evaluated in THAT module's VM): it builds an inheriting
--     overlay per cell — the cell's header plus the library's controls — each taking over
--     from its cell's plain header while the library's landmark is on screen.
--
-- Within a slot the arbiter picks the most specific overlay that matches: Komplete
-- Kontrol's own chrome overlay (layer chrome) < a Kontakt header (base) < a library
-- (content) < the Content Missing dialog (dialog). Only one cell of the matrix can match
-- a given window, so cells never compete with each other.
--
-- The pieces live in their own files: detect (which window is what), cells (the matrix),
-- geometry (per-version coordinates), actions (what a control does), header (what a cell
-- contains). What is left here is the wiring.

local O = host.require("com.platform.overlay")
local D = host.include("src/detect.luau")
local A = host.include("src/actions.luau")
local C = host.include("src/cells.luau")
local header = host.include("src/header.luau")

-- ── Content Missing ──────────────────────────────────────────────────────────
-- Shown when a library's samples aren't where Kontakt expects them, and a hard stop until
-- you point it at the folder. ReaHotkey gives it its own overlay with the one control that
-- matters, and so do we. Two shapes:
--   • embedded — the dialog is an NIChildWindow CONTROL inside the host window. That
--     class is NOT enough on its own: Komplete Kontrol wraps its plugin in one too, and
--     matching on it alone made this overlay win permanently inside KK, hiding every other
--     control. See the identify below.
--   • standalone — a top-level #32770 titled "Content Missing".
-- Version-independent, so it is not part of the cell matrix. Both join their slot at the
-- dialog layer, so the dialog takes over from whatever header or library overlay was
-- active and hands back when it closes.
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

-- Builds one overlay for `cell`: the cell's header, then whatever `build` adds on top.
-- `landmark`, when given, gates the overlay on that image being on screen AND anchors the
-- controls to it — a library's controls are authored relative to its on-screen wordmark,
-- so they stay correct across every host and browser state without any offset.
local function overlayFor(cell, opts)
  local ov = O.new(opts.name or cell.version)
  header.add(ov, cell, opts.build ~= nil)
  ov:frame(cell.frame)
  ov:onActivate(A.onPluginActive)
  if opts.build then opts.build(ov) end
  if opts.landmark then ov:landmark(opts.landmark, { anchor = true }) end
  ov:bind(cell.binding, { specificity = opts.layer, pollMatch = opts.pollMatch })
  return ov
end

return {
  -- Standalone side effect, run only in Kontakt's own VM: one header overlay per cell.
  activate = function()
    for _, cell in ipairs(C.all) do
      overlayFor(cell, { layer = O.layer.base })
    end
    host.log.info("Kontakt header overlays ready: " .. #C.all .. " cells.")

    contentMissingOverlay():bind(
      O.embedded({
        hosts = C.HOSTS,
        control = CONTENT_MISSING_CONTROL,
        -- The window class alone is NOT enough: Komplete Kontrol wraps its plugin in an
        -- NIChildWindow too, so matching on the class made this dialog overlay win
        -- permanently inside KK and hide every other control. Reject the container that
        -- IS the plugin host; see detect.isPluginHostContainer.
        identify = function(c)
          return D.kontaktPresent() and not D.isPluginHostContainer(c)
        end,
        cacheIdentity = false,
      }, { slot = C.SLOT }),
      { specificity = O.layer.dialog }
    )
    contentMissingOverlay():bind(
      O.window(CONTENT_MISSING_WINDOW, { slot = C.STANDALONE_SLOT }),
      { specificity = O.layer.dialog }
    )
    host.log.info("Kontakt Content Missing overlays ready.")
  end,

  -- Called from a sample-library module (this file runs in ITS VM): one inheriting overlay
  -- per cell, each winning over that cell's plain header while `landmark` is on screen
  -- inside Kontakt. The library adds its controls with the same builders used everywhere
  -- — ov:addStaticText / addHotspotButton / addHotspotToggle / addCustomButton — on top of
  -- the inherited header.
  --
  -- `landmark` (and any control's on/off image) MUST be an ABSOLUTE path (host.path): the
  -- image search runs in Kontakt's identity scope, so a relative path would resolve under
  -- Kontakt's root, not the library's.
  --
  -- `build(ov)` runs once PER CELL — it must only add controls, not perform side effects.
  library = function(name, landmark, build)
    local made = {}
    for _, cell in ipairs(C.all) do
      made[#made + 1] = overlayFor(cell, {
        name = "Kontakt — " .. (name or "library"),
        layer = O.layer.content,
        -- A library loads and unloads with no window event, so its landmark gate is
        -- re-checked on a poll rather than only on focus changes.
        pollMatch = 500,
        landmark = landmark,
        build = build,
      })
    end
    host.log.info(string.format(
      "Kontakt library overlays ready: %s (%d cells)", name or "library", #made
    ))
    return made
  end,
}
