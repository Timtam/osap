-- Komplete Kontrol base overlay (com.platform.komplete-kontrol). KK is an NI Qt
-- plug-in that hosts OTHER instruments (Massive, Kontakt + its libraries, …). This
-- gives KK its own accessible header so you can drive KK itself — open its menu and
-- focus its library-browser search field to load an instrument.
--
-- It competes on the SHARED Kontakt arbiter slot at the LOWEST specificity, so once
-- a Kontakt library loads inside KK the Kontakt / library overlay (higher
-- specificity) takes over the same slot: KK chrome (0) < Kontakt header (1) <
-- library (2). Ported from ReaHotkey's KompleteKontrol overlay (Menu hotspot at
-- 297,17; search field = the first UIA Edit, type 50004).

local O = host.require("com.platform.overlay")
local daw = host.require("com.platform.daw-hosts")

local SLOT = "com.platform.kontakt" -- shared with kontakt so the arbiter composes
local EDIT = 50004 -- UIA control type: Edit (KK's browser search field)

-- A Komplete Kontrol Qt control (same Qt class as Kontakt; the UIA name is what
-- distinguishes it). Non-relational, so the identity cache is safe here.
local function isKK(c)
  return host.uia.find(c.id, "Komplete Kontrol", 50032) or host.uia.find(c.id, "Komplete Kontrol", 50033)
end

local function spec()
  return {
    hosts = daw.all,
    -- KK's control is the NI-versioned Qt class (e.g. "Qt671NI_6_7_1_R3QWindowIcon
    -- {GUID}"); ".-" spans the NI_<version> segment that sits before QWindowIcon.
    control = "Qt%d+.-QWindowIcon",
    identify = isKK,
  }
end

-- The focused KK plugin window handle, from the overlay's active context.
local function pluginHwnd(ov)
  local ctrl = ov.activeCtx and ov.activeCtx.origin()
  return ctrl and ctrl.id
end

-- Close KK's library browser (ReaHotkey's CloseKKBrowser): find the browser pane —
-- a FileTypeSelector (ControlType 50018) or TagCloudAccordionWithBrands (50033) —
-- and click the toggle that precedes it (its previous raw-view sibling). No-op when
-- the browser isn't open. Closing it reveals a loaded instrument (e.g. Kontakt + its
-- library) so that overlay can take over.
local function closeBrowser(hwnd)
  if not hwnd then return end
  local p = host.uia.classNavPoint(hwnd, "FileTypeSelector", 50018, 0, -1)
    or host.uia.classNavPoint(hwnd, "TagCloudAccordionWithBrands", 50033, 0, -1)
  if p then host.input.click(p.x, p.y) end
end

return {
  -- Standalone side effect (this module's own VM): the KK base overlay.
  activate = function()
    local ov = O.new("Komplete Kontrol")
    ov:addStaticText("Komplete Kontrol")
    -- KK's hamburger menu — a fixed hotspot in KK's chrome (ReaHotkey: 297,17),
    -- origin-relative to the KK control.
    ov:addHotspotButton({ label = "Komplete Kontrol menu", at = { 297, 17 }, hotkey = "Alt+M" })
    -- Focus KK's library-browser search field (the first UIA Edit) so you can type
    -- an instrument/library name; KK filters and you pick it with KK's own keys.
    ov:addCustomButton({
      label = "Search library browser",
      hotkey = "Alt+S",
      onActivate = function(o)
        local hwnd = pluginHwnd(o)
        if not hwnd then return end
        local p = host.uia.locate(hwnd, "", EDIT)
        if p then host.input.click(p.x, p.y) end
      end,
    })
    ov:addCustomButton({
      label = "Close library browser",
      hotkey = "Alt+L",
      onActivate = function(o) closeBrowser(pluginHwnd(o)) end,
    })
    -- Auto-close KK's browser whenever KK becomes focused (ReaHotkey's CloseKKBrowser),
    -- so a loaded instrument is revealed and its overlay can take over. Set before
    -- attach so it is armed for the first activation.
    ov:onActivate(function(o)
      -- Small delay: at the instant of activation KK's browser UIA may not be ready.
      -- Skip if Kontakt already took the slot by then.
      host.timer.after(500, function()
        if o.active then closeBrowser(pluginHwnd(o)) end
      end)
    end)
    -- Tab / Shift+Tab navigate the header (like the Kontakt overlay), Enter
    -- activates; Alt+M / Alt+S are also global. The menu-watch hands the captured
    -- nav keys to KK's own Qt menu (opened by Alt+M) while it is open.
    ov:attachEmbedded(spec(), { slot = SLOT, specificity = 0, naturalNav = true })
    ov:_startMenuWatch()
    host.log.info("Komplete Kontrol base overlay ready.")
  end,
}
