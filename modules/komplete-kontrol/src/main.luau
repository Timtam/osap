-- Komplete Kontrol base overlay (com.platform.komplete-kontrol). KK is an NI Qt
-- plug-in that hosts OTHER instruments (Massive, Kontakt + its libraries, …). This
-- gives KK its own accessible header so you can drive KK itself — open its menu, focus
-- its library-browser search field to load an instrument, and close the browser.
--
-- It competes on the SHARED Kontakt arbiter slot at the LOWEST specificity, so once a
-- Kontakt library loads inside KK the Kontakt / library overlay (higher specificity)
-- takes over the same slot: KK chrome (0) < Kontakt header (1) < library (2). Ported
-- from ReaHotkey's KompleteKontrol overlay (Menu hotspot at 297,17; search field = the
-- first UIA Edit, type 50004).

local O = host.require("com.platform.overlay")
local daw = host.require("com.platform.daw-hosts")

local SLOT = "com.platform.kontakt" -- shared with kontakt so the arbiter composes
local EDIT = 50004 -- UIA control type: Edit (KK's browser search field)

-- The Komplete Kontrol STANDALONE application window (KK running on its own, not
-- embedded in a DAW): a top-level NINormalWindow titled "Komplete Kontrol", exe
-- "Komplete Kontrol.exe". Mirrors ReaHotkey's Standalone.Register criteria. Its native
-- menu bar (File/Edit/View/Controller/Help) sits in the window chrome just above the
-- client area, so those hotspots are authored at client-relative Y = -10.
local STANDALONE = {
  title = { exact = "Komplete Kontrol" },
  app = { exe = { contains = "Komplete Kontrol" } },
  windows = { class = { prefix = "NINormalWindow" } },
}

-- Opt-out setting (default ON): auto-close KK's library browser when this KK overlay
-- becomes active with the browser open. An open browser HIDES a loaded instrument (that
-- is exactly why the KK overlay — not the Kontakt/library overlay — is the active one),
-- so closing it reveals the instrument and lets Kontakt/library detection take over.
-- This lives HERE (not in the Kontakt module) because the Kontakt overlay cannot
-- auto-close it: while the browser hides Kontakt, the Kontakt overlay isn't even active.
-- Turn OFF in the manager's Settings dialog if you keep the browser open on purpose
-- (e.g. to browse for another instrument). (ReaHotkey's CloseKKBrowser defaults off; we
-- default it on by choice.)
local CLOSE_KK_BROWSER = "closeKKBrowser"
host.settings.define(CLOSE_KK_BROWSER, true, {
  label = "Automatically close Komplete Kontrol's library browser",
})

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

-- The toggle that closes KK's library browser — the previous-sibling of the browser
-- pane (FileTypeSelector / TagCloudAccordionWithBrands) — or nil when NO browser is
-- open. Doubles as the "is the browser open?" probe (nil ⇒ closed).
local function browserToggle(hwnd)
  if not hwnd then return nil end
  return host.uia.classNavPoint(hwnd, "FileTypeSelector", 50018, 0, -1)
    or host.uia.classNavPoint(hwnd, "TagCloudAccordionWithBrands", 50033, 0, -1)
end

-- `when` predicate for the manual close control + the auto-close: is a browser open?
local function browserOpen(o)
  return browserToggle(pluginHwnd(o)) ~= nil
end

-- Fired when the KK overlay becomes the active overlay: if the browser is open (hiding a
-- loaded instrument) and the setting is on, close it — revealing the instrument so the
-- higher-specificity Kontakt/library overlay can detect it and take over.
local function onKKActive(o)
  local enabled = host.settings.get(CLOSE_KK_BROWSER)
  local p = enabled and browserToggle(pluginHwnd(o)) or nil
  host.log.info(string.format(
    "KK overlay active: auto-close=%s browserToggle=%s",
    tostring(enabled), p and (p.x .. "," .. p.y) or "nil (no browser open)"
  ))
  if p then
    host.input.click(p.x, p.y)
    -- Closing the browser reveals the nested Kontakt a moment later, with NO OS focus
    -- event to notice it — so poke a few cross-VM re-checks over the next second, to hand
    -- over to the Kontakt/library overlay promptly instead of waiting on its ~500 ms poll
    -- (and to catch the control whenever it re-appears). Idempotent + coalesced.
    host.timer.after(250, host.window.recheck)
    host.timer.after(600, host.window.recheck)
    host.timer.after(1000, host.window.recheck)
  end
end

return {
  -- Standalone side effect (this module's own VM): the KK base overlay.
  activate = function()
    local ov = O.new("Komplete Kontrol")
    ov:addStaticText("Komplete Kontrol")
    -- KK's hamburger menu — a fixed hotspot in KK's chrome (ReaHotkey: 297,17),
    -- origin-relative to the KK control.
    ov:addHotspotButton({ label = "Komplete Kontrol menu", at = { 297, 17 }, hotkey = "Alt+M" })
    -- Focus KK's library-browser search field (the first UIA Edit) so you can type an
    -- instrument/library name; KK filters and you pick it with KK's own keys.
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
    -- Manual close, shown ONLY while a browser is actually open (no point offering it
    -- otherwise). The auto-close above covers most cases; this is the fallback when the
    -- setting is off, or to close a browser you opened yourself.
    local closeBtn = ov:addCustomButton({
      label = "Close library browser",
      hotkey = "Alt+L",
      onActivate = function(o)
        local p = browserToggle(pluginHwnd(o))
        if p then host.input.click(p.x, p.y) end
      end,
    })
    closeBtn.when = browserOpen
    -- Auto-close on activate (opt-out): reveals an instrument the browser is hiding.
    ov:onActivate(onKKActive)
    -- Tab / Shift+Tab navigate the header (like the Kontakt overlay), Enter activates;
    -- Alt+M / Alt+S / Alt+L are also global. The menu-watch hands the captured nav keys
    -- to KK's own Qt menu (opened by Alt+M) while it is open.
    ov:attachEmbedded(spec(), { slot = SLOT, specificity = 0, naturalNav = true })
    ov:_startMenuWatch()
    host.log.info("Komplete Kontrol base overlay ready.")

    -- STANDALONE overlay: Komplete Kontrol running as its OWN application (not embedded
    -- in a DAW). A SEPARATE overlay from the embedded one — its controls are KK's native
    -- menu bar (ReaHotkey's StandaloneHeader). Attached to the top-level window via
    -- attach() (origin = the window's client area); the menu bar sits in the chrome just
    -- above the client, so the hotspots use Y = -10. Auto-closes the library browser on
    -- activate like the embedded overlay (the opt-out setting is shared).
    local sa = O.new("Komplete Kontrol")
    sa:addHotspotButton({ label = "File menu", at = { 16, -10 }, hotkey = "Alt+F" })
    sa:addHotspotButton({ label = "Edit menu", at = { 52, -10 }, hotkey = "Alt+E" })
    sa:addHotspotButton({ label = "View menu", at = { 83, -10 }, hotkey = "Alt+V" })
    sa:addHotspotButton({ label = "Controller menu", at = { 138, -10 }, hotkey = "Alt+C" })
    sa:addHotspotButton({ label = "Help menu", at = { 194, -10 }, hotkey = "Alt+H" })
    sa:onActivate(onKKActive)
    sa:attach(STANDALONE, { naturalNav = true })
    sa:_startMenuWatch()
    host.log.info("Komplete Kontrol standalone overlay ready.")
  end,
}
