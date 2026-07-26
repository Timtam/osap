-- What a Kontakt HEADER CONTROL actually does: reach into the plugin by UIA where that
-- works, fall back to the coordinate ReaHotkey uses for that version, drive the file menu,
-- and dismiss the one-time update dialog. Takes the version of the focused Kontakt as an
-- argument wherever the answer differs, so nothing here has to know how versions are
-- resolved.

local U = host.uia.type
local BUTTON, MENUITEM = U.Button, U.MenuItem
local D = host.include("src/detect.luau")
local geo = host.include("src/geometry.luau")
local VARIANTS, kontaktPaneName, findNestedKontakt =
  D.VARIANTS, D.kontaktPaneName, D.findNestedKontakt

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

return {
  pluginLocate = pluginLocate,
  uiaClick = uiaClick,
  closeWhatsNew = closeWhatsNew,
  onPluginActive = onPluginActive,
}
