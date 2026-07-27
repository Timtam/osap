-- What a Kontakt HEADER CONTROL actually does: reach into the plugin by UIA where that
-- works, fall back to the coordinate ReaHotkey uses, drive the file menu, and dismiss the
-- one-time update dialog.
--
-- Everything version-dependent arrives as a CELL — the overlay's own declaration, which
-- already knows its version and carries that version's geometry (see cells.luau). Nothing
-- here resolves a version at runtime.

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

-- Opens the file menu: the UIA file-menu button when it is reachable (a standalone /
-- full-UIA Kontakt), else a coordinate click on the header. Returns the origin control
-- plus whether UIA was used, so a follow-up item click picks the same method.
local function openFileMenu(cell, o)
  local ctrl = o:origin()
  if uiaClick(nil, cell.uiaFileMenu, BUTTON) then
    host.keys.menuOpen(true)
    return ctrl, true
  end
  local at = cell.geometry.fileMenu
  if ctrl and ctrl.client and at then
    host.input.click(ctrl.client.x + at[1], ctrl.client.y + at[2])
    return ctrl, false
  end
  return nil
end

-- Opens the file menu and selects `item` once it appears — by UIA (standalone) or by a
-- coordinate click on the item's fixed position within the opened menu (DAW-embedded).
-- The item's action opens Kontakt's own native, ACCESSIBLE dialog, so the user never has
-- to read the Qt menu, which carries no accessibility information at all.
-- Groups OCR words into menu ROWS (words within a few pixels of each other vertically),
-- returning { text = "save multi as...", y = <centre>, x = <left edge> } per row, in
-- reading order. OCR gives words with positions; a menu is rows, so this is the join.
local function ocrRows(words)
  local rows = {}
  for _, w in ipairs(words or {}) do
    local mid = w.y + math.floor(w.h / 2)
    local row
    for _, r in ipairs(rows) do
      if math.abs(r.y - mid) <= 7 then row = r break end
    end
    if not row then
      row = { y = mid, x = w.x, parts = {} }
      rows[#rows + 1] = row
    end
    row.x = math.min(row.x, w.x)
    row.parts[#row.parts + 1] = { x = w.x, text = w.text }
  end
  for _, r in ipairs(rows) do
    table.sort(r.parts, function(p, q) return p.x < q.x end)
    local t = {}
    for _, part in ipairs(r.parts) do t[#t + 1] = part.text end
    r.text = string.lower(table.concat(t, " "))
  end
  return rows
end

-- Opens the file menu and selects `item` by READING the menu and clicking the row whose
-- text matches — not by clicking a measured offset.
--
-- The offsets came first and were wrong twice: the menu's contents vary (one entry more or
-- less above shifts every row below it), and a one-row slip put "Reset multi" onto "Save as
-- default multi", which overwrites the user's default multi. A number measured against one
-- state of a menu is a number that will be wrong again. Reading the menu costs one OCR per
-- press — a user action, not a loop — and cannot drift.
--
-- If no row matches, nothing is clicked: the menu is closed and the failure is announced,
-- which is what ReaHotkey does through UIA ("menu item not found") where it can see the
-- menu at all. An embedded Kontakt's menu carries no accessibility information whatsoever,
-- so OCR is the only way to know what is actually on screen.
local function invokeMenuItem(cell, o, item)
  local ctrl, uia = openFileMenu(cell, o)
  if not ctrl then return end
  host.timer.after(250, function()
    if uia then
      uiaClick(nil, item, MENUITEM)
      return
    end
    local base = cell.geometry.fileMenu
    if not (ctrl.client and base) then return end
    local bx, by = ctrl.client.x + base[1], ctrl.client.y + base[2]
    -- Generously around where the menu drops from its button; the exact bounds do not
    -- matter, only that the menu is inside them.
    local seen = host.ocr.recognize({ region = { bx - 80, by + 14, bx + 170, by + 460 } })
    local want = string.lower(string.gsub(item, "%.%.%.$", ""))
    local hit
    for _, r in ipairs(ocrRows(seen and seen.words)) do
      -- The row must CONTAIN the wanted text and not be a longer, different entry:
      -- "save multi as" must not match "save as default multi". Anchoring at the start
      -- and allowing only a trailing shortcut column does that.
      if string.sub(r.text, 1, #want) == want then
        hit = r
        break
      end
    end
    if not hit then
      host.log.info(string.format(
        "Kontakt: no menu row starting with '%s' — read: %s",
        item, string.gsub(tostring(seen and seen.text or ""), "%s+", " ")
      ))
      host.speech.output("Menu item not found", { interrupt = true })
      host.input.send("Escape")
      return
    end
    host.input.click(hit.x + 10, hit.y)
  end)
end

-- Kontakt's snapshot bar is a second row in the instrument header that the camera button
-- reveals, and the snapshot controls have no target until it is up. Rather than offering
-- them only while it happens to be showing — which makes them come and go under the user —
-- each one REVEALS the bar and then clicks its target, the same shape as the file-menu
-- actions: open the thing, then choose from it.
--
-- Whether the bar is already up is read from the camera icon itself, which lights up while
-- it is (measured white against dark chrome). That read only decides whether to press the
-- camera, and it is taken again after pressing: if the bar still is not there, nothing is
-- clicked and the user is told, instead of a click landing on the "Output / Voices" row
-- that occupies the same place when the bar is hidden.
local CAMERA_FROM_RIGHT, CAMERA_Y = 305, 87

local function snapshotBarUp(ctrl)
  local p = host.screen.pixel(
    ctrl.client.x + ctrl.bounds.w - CAMERA_FROM_RIGHT,
    ctrl.client.y + CAMERA_Y
  )
  return p ~= nil and (p.r + p.g + p.b) / 3 > 150
end

local function snapshotAction(fromRight, y)
  return function(o)
    local ctrl = o:origin()
    if not (ctrl and ctrl.client and ctrl.bounds) then return end
    local function clickTarget()
      host.input.click(ctrl.client.x + ctrl.bounds.w - fromRight, ctrl.client.y + y)
    end
    if snapshotBarUp(ctrl) then
      clickTarget()
      return
    end
    host.input.click(
      ctrl.client.x + ctrl.bounds.w - CAMERA_FROM_RIGHT,
      ctrl.client.y + CAMERA_Y
    )
    host.timer.after(250, function()
      if snapshotBarUp(ctrl) then
        clickTarget()
      else
        host.log.info("Kontakt: snapshot bar did not appear — not clicking")
        host.speech.output("Snapshots not available", { interrupt = true })
      end
    end)
  end
end

-- A header control that acts through UIA where that works (a standalone Kontakt is fully
-- accessible) and falls back to this cell's coordinate for that control.
local function headerAction(cell, uiaName, key)
  return function(o)
    local ctrl = o:origin()
    if uiaClick(nil, uiaName, BUTTON) then return end
    local at = cell.geometry[key]
    if ctrl and ctrl.client and at then
      host.input.click(ctrl.client.x + at[1], ctrl.client.y + at[2])
    end
  end
end

return {
  pluginLocate = pluginLocate,
  uiaClick = uiaClick,
  closeWhatsNew = closeWhatsNew,
  onPluginActive = onPluginActive,
  openFileMenu = openFileMenu,
  invokeMenuItem = invokeMenuItem,
  headerAction = headerAction,
  snapshotAction = snapshotAction,
}
