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

-- KK standalone's Preferences dialog: a standard Win32 dialog frame (#32770) titled
-- "Preferences", owned by Komplete Kontrol.exe. Its content is custom-drawn Qt with NO
-- UIA at all (the whole tree is two nameless Panes), so — exactly like ReaHotkey — it is
-- driven purely by client-relative PIXEL COORDINATES. Those below were measured on the
-- current KK version (ReaHotkey's have already drifted) against a ~600x496 client, and
-- may need re-calibration after a KK update. Verified reachable via PrintWindow capture.
local PREFS = {
  title = { exact = "Preferences" },
  app = { exe = { contains = "Komplete Kontrol" } },
  windows = { class = "#32770" },
}

-- An UNTITLED #32770 dialog owned by Komplete Kontrol.exe. Both the "Save preset" and the
-- "Scanning in progress" dialogs are untitled #32770s (only Preferences carries a title),
-- so one window matcher fits both; they are told apart on a shared ARBITER slot by an
-- image landmark + specificity — Save As has the "Save Preset" header and wins, Scanning
-- is the fallback for any other untitled KK dialog. The `not "Preferences"` guard keeps
-- both off the Preferences dialog (so the Save As image search never runs there).
local KK_UNTITLED_DIALOG = {
  title = { ["not"] = "Preferences" },
  app = { exe = { contains = "Komplete Kontrol" } },
  windows = { class = "#32770" },
}

-- Arbiter slot the two untitled-dialog overlays compete on, so at most one is active and
-- the more specific (Save As, with its landmark) wins over the Scanning fallback.
local KK_DIALOG_SLOT = "com.platform.kk-dialog"

-- Opt-out setting (default ON): auto-close KK's library browser when this KK overlay
-- becomes active with the browser open. An open browser HIDES a loaded instrument (that
-- is exactly why the KK overlay — not the Kontakt/library overlay — is the active one),
-- so closing it reveals the instrument and lets Kontakt/library detection take over.
-- This lives HERE (not in the Kontakt module) because the Kontakt overlay cannot
-- auto-close it: while the browser hides Kontakt, the Kontakt overlay isn't even active.
-- Turn OFF in the manager's Settings dialog if you keep the browser open on purpose
-- (e.g. to browse for another instrument). (ReaHotkey's CloseKKBrowser defaults off; we
-- default it on by choice.)
-- The setting is DEFINED in activate(), NOT here at module level: this module is a
-- `code_module`, so its code is evaluated inside every dependent's VM (e.g. Kontakt
-- imports its standalone-window matcher). A module-level define would then register the
-- setting in those VMs too; activate() runs only in this module's OWN VM.
local CLOSE_KK_BROWSER = "closeKKBrowser"

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

-- Builds an onActivate that clicks a SEQUENCE of client-relative points in the active
-- window, with a settle delay between each — the way the tabbed, UIA-less Preferences
-- dialog is driven: switch to the owning tab (and sub-tab), then click the control. So
-- every action is self-contained: the user need not first navigate to the right tab.
-- The origin (dialog client, in screen coords) is read once at activation.
local function prefsAction(label, points)
  return function(o)
    local ctx = o.activeCtx and o.activeCtx.origin()
    local c = ctx and ctx.client
    if not c then return end
    for i, p in ipairs(points) do
      local sx, sy = c.x + p[1], c.y + p[2]
      if i == 1 then
        host.input.click(sx, sy)
      else
        host.timer.after(400 * (i - 1), function() host.input.click(sx, sy) end)
      end
    end
    host.speech.output(label .. ", activated", { interrupt = true })
  end
end

return {
  -- The KK standalone window matcher, EXPORTED so a dependent (Kontakt, which optionally
  -- requires this module) can reuse it as a host — recognising a Kontakt nested inside a
  -- standalone KK — without duplicating KK's window identity or depending on daw-hosts.
  standaloneWindow = STANDALONE,

  -- Standalone side effect (this module's own VM only — activate never runs when this
  -- module is imported as a code dependency into another VM): the KK base overlay + the
  -- standalone overlays + dialogs.
  activate = function()
    -- Defined here (not at module level) so it only registers in this module's own VM.
    host.settings.define(CLOSE_KK_BROWSER, true, {
      label = "Automatically close Komplete Kontrol's library browser",
    })
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
    ov:attachEmbedded(spec(), { slot = SLOT, specificity = 0 })
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
    -- Join the shared Kontakt arbiter slot at the LOWEST specificity, so a Kontakt (1) or
    -- library (2) loaded inside this standalone KK takes over the same slot — exactly like
    -- the embedded KK chrome (0) does in a DAW. (The Kontakt module adds this KK window as
    -- a host, so its nested overlays can match here.) When the browser hides the nested
    -- Kontakt, this overlay is the active one and its onKKActive still auto-closes it.
    sa:attach(STANDALONE, { slot = SLOT, specificity = 0 })
    sa:_startMenuWatch()
    host.log.info("Komplete Kontrol standalone overlay ready.")

    -- STANDALONE Preferences dialog overlay (#32770 "Preferences"). Robust core: switch
    -- the five tabs, close, and the actionable items ReaHotkey exposes (library / plug-in
    -- rescans, add user directory). The tabs' content is not UIA-readable, so each action
    -- auto-navigates to its own tab/sub-tab first (prefsAction). The Plug-ins/Library
    -- checkboxes (pixel-state) are a follow-up. Coordinates are client-relative to the
    -- ~600x496 dialog (measured on the current KK version — re-calibrate after updates).
    local prefs = O.new("Komplete Kontrol Preferences")
    -- One tab control for the five left-column tabs (ReaHotkey's TabControl): Left/Right
    -- switch tabs, and only the CURRENT tab's controls are Tab-reachable. Each tab's `at`
    -- is the client-relative point clicked to switch the underlying dialog.
    local tc = prefs:addTabControl({ tabs = {
      { label = "Audio", at = { 62, 78 } },
      { label = "MIDI", at = { 62, 115 } },
      { label = "General", at = { 62, 156 } },
      { label = "Library", at = { 62, 196 } },
      { label = "Plug-ins", at = { 62, 237 } },
    } })
    -- Audio / MIDI / General expose no controls of their own (only the global Close).
    -- Library tab (4): rescans + add-directory. The user is already on the Library tab
    -- (the tab control clicked it), so each action only needs its Factory/User sub-tab
    -- (163,74 / 240,74 — not UIA-readable) then the control.
    prefs:addCustomButton({
      label = "Rescan factory library",
      onActivate = prefsAction("Rescan factory library", { { 163, 74 }, { 548, 420 } }),
    }).when = tc:onTab(4)
    prefs:addCustomButton({
      label = "Rescan user library",
      onActivate = prefsAction("Rescan user library", { { 240, 74 }, { 548, 420 } }),
    }).when = tc:onTab(4)
    prefs:addCustomButton({
      label = "Add user library directory",
      onActivate = prefsAction("Add user library directory", { { 240, 74 }, { 163, 420 } }),
    }).when = tc:onTab(4)
    -- Plug-ins tab (5): rescan (on its Manager sub-tab, 163,74).
    prefs:addCustomButton({
      label = "Rescan plug-ins",
      onActivate = prefsAction("Rescan plug-ins", { { 163, 74 }, { 548, 398 } }),
    }).when = tc:onTab(5)
    -- Close is available on every tab.
    prefs:addHotspotButton({ label = "Close", at = { 548, 473 } })
    prefs:attach(PREFS)
    host.log.info("Komplete Kontrol standalone Preferences overlay ready.")

    -- STANDALONE "Save preset" dialog overlay (#32770, empty title, ~360x160). Custom-
    -- drawn with no UIA, so — like ReaHotkey — an OCR read-out for the name field plus
    -- hotspot Save/Cancel, coordinates client-relative (measured on the current KK).
    -- Identified by the "Save Preset" header image (landmark), which also disambiguates
    -- it from the Scanning dialog (the other untitled #32770).
    local saveas = O.new("Komplete Kontrol Save preset")
    -- OCR region kept strictly INSIDE the field's dark interior (x22-334, y68-85 on the
    -- current KK) — its four corners must land on the dark background, not the medium-grey
    -- frame, or the OCR `tighten` step reads the whole input box as content and shrinks the
    -- text to an unreadable, clipped sliver (it then returned just "c").
    saveas:addOCREdit({ label = "Preset name", region = { 26, 70, 330, 83 } })
    saveas:addHotspotButton({ label = "Save", at = { 218, 135 } })
    saveas:addHotspotButton({ label = "Cancel", at = { 293, 135 } })
    saveas:landmark(host.path("images/saveas-header.png"))
    -- Specificity 1 on the shared slot: with its "Save Preset" landmark present it beats
    -- the Scanning fallback (0) whenever this really is the Save-preset dialog.
    saveas:attach(KK_UNTITLED_DIALOG, { slot = KK_DIALOG_SLOT, specificity = 1 })
    host.log.info("Komplete Kontrol standalone Save preset overlay ready.")

    -- STANDALONE "Scanning in progress" dialog overlay: shown while KK rescans its
    -- library/plug-ins. It is another untitled #32770 with no distinguishing image, so it
    -- is the FALLBACK on the shared slot (specificity 0) — active on an untitled KK dialog
    -- only when the Save As overlay (1) does NOT claim it. Mirrors ReaHotkey: a static
    -- "scanning in progress" text, re-announced every ~2 s while open, and "scanning
    -- stopped" when it closes. Built to spec — could not be live-tested (needs a scan
    -- running), so detection/timing may need tuning against a real scan.
    local scan = O.new("Komplete Kontrol Scanning")
    scan:addStaticText("Scanning in progress")
    scan:onActivate(function(o)
      -- The fallback overlay is briefly claimed on the shared slot while Save As's async
      -- landmark is still resolving (opening/closing the Save preset dialog) — a transient
      -- that must stay silent. So "commit" to being a real scan only after the overlay has
      -- stayed active past that window; a transient claim deactivates first and commits
      -- nothing. The first "scanning in progress" comes from the static text's focus
      -- announcement; the commit only starts a self-rescheduling ~2 s progress ping that
      -- winds itself down when the dialog closes.
      o._scanCommitted = false
      host.timer.after(800, function()
        if not o.active then return end
        o._scanCommitted = true
        local function ping()
          if not o.active then return end
          host.speech.output("Scanning in progress", { interrupt = true })
          host.timer.after(2000, ping)
        end
        host.timer.after(2000, ping)
      end)
    end)
    scan:onDeactivate(function(o)
      -- Only announce the end of a scan that actually started (committed) — never for a
      -- transient claim during a Save As open/close.
      if o._scanCommitted then
        host.speech.output("Scanning stopped", { interrupt = true })
      end
      o._scanCommitted = false
    end)
    scan:attach(KK_UNTITLED_DIALOG, { slot = KK_DIALOG_SLOT, specificity = 0 })
    host.log.info("Komplete Kontrol standalone Scanning overlay ready.")
  end,
}
