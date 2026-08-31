-- u-he synthesizers — Diva, Hive 2, Repro-1, Zebra2 and ZebraHZ, from one table.
--
-- Ported from ReaHotkey's Diva.ahk, Hive2.ahk, Repro.ahk and ZebraLegacy.ahk. Those four
-- files are the same file: token for token identical apart from a window class, a display
-- name and six coordinates. Whatever is genuinely shared belongs in one place, so the only
-- thing a synth contributes below is its own measurements.
--
-- ── Why these are their own overlays, not library overlays ────────────────────
-- Every port so far has been a sample library inside Kontakt, reached through Kontakt's
-- framework. These are ordinary VST plug-ins with a window class of their own, which makes
-- them the simplest case the runtime has: the class IS the identity, so there is no
-- landmark to search for, nothing to disambiguate, and no `identify` callback. A u-he
-- window is a Diva because Windows says its class is com.u-he.Diva.vst.window.
--
-- ── The trailing digit ReaHotkey's classes carry, and we must not ─────────────
-- ReaHotkey registers these as "^com.u-he.Diva.vst.window1$". That final 1 is NOT part of
-- the class: AutoHotkey matches against ClassNN, which is the class name with a per-window
-- INSTANCE NUMBER appended, while this runtime matches the raw class from GetClassName.
-- Copied verbatim, the pattern could never match anything, and it failed the way everything
-- in this system fails — in silence. Enumerating REAPER's window tree settled it in one
-- command: [com.u-he.Repro1.vst.window] 'Window' 1120x680.
--
-- The `%d*` below therefore matches the real class AND anything that appends an instance
-- number, so this is correct whichever convention a host reports. Note also that AHK's "."
-- is a regex any-character; in a Lua pattern it has to be written "%.", or the pattern
-- would match classes it was never meant to.
--
-- ── What the overlay gives you ────────────────────────────────────────────────
-- The preset header, which is the part of these synths a screen-reader user cannot reach at
-- all: which preset is loaded (OCR — nothing else in the UI reports it), the previous/next
-- buttons, save, the u-he logo menu, and on three of the five a toggle for the preset
-- browser. The sound-design controls are hundreds of unlabelled knobs and are not the
-- point; getting from preset to preset is.

local O = host.require("com.platform.overlay")
local daw = host.require("com.platform.daw-hosts")

-- One arbiter slot for the whole family. Two u-he plug-ins on the same track are children
-- of one DAW plug-in-chain window, so both overlays would match it at once; sharing a slot
-- means the arbiter picks one instead of leaving two overlays fighting over Tab. Which one
-- it picks is then arbitrary — but one overlay on the wrong synth is recoverable, and two
-- overlays on one keyboard is not. (The same ambiguity already exists ACROSS modules — a
-- Kontakt and a Diva in one chain — and is a limitation of matching on the host window
-- rather than on which plug-in has focus. Noted in TODO.md, not introduced here.)
local SLOT = "com.platform.u-he"

-- All coordinates are relative to the plug-in control's own client top-left, which is what
-- ReaHotkey's CompensatePluginCoordinates computes and what this runtime's origin already
-- is — so its numbers transfer unchanged. They are ReaHotkey's, and they assume the synth
-- is at its DEFAULT window size: every u-he plug-in has a zoom setting (70 %-200 %), and at
-- any other setting the whole header scales and every number here is wrong. There is no
-- landmark that fixes this — anchoring moves a coordinate frame, it does not scale it — so
-- a mis-sized window has to be caught by calibration and, for now, corrected by setting the
-- synth back to its default size.
--
-- `browser` is the preset-browser toggle, present on three of the five. Its state is read
-- from ONE pixel at its click point against two reference colours (ReaHotkey's
-- HotspotToggleButton), so those colours are as much a measurement as the position is.
local SYNTHS = {
  {
    name = "Diva",
    class = "^com%.u%-he%.Diva%.vst%.window%d*$",
    -- MEASURED against a calibration shot (Diva 1.4.5, 1200x670). All five positions land
    -- where ReaHotkey says, and both toggle colours read back EXACTLY (unlit 126,128,134 =
    -- 0x7E8086, lit 170,168,159 = 0xAAA89F), which also settles that AutoHotkey v2's
    -- PixelGetColor is plain RGB with no channel swap.
    logo = { 1038, 30 },
    prev = { 458, 30 }, -- the left arrow inside the preset pill
    -- The preset name, and the ONE number here that is not ReaHotkey's. Its region ends at
    -- y 44, which cuts the descenders off the loaded name ("HS Gongfission" measured at
    -- y 24-45) — survivable for the Tesseract it was tuned against, but our OCR crops to the
    -- glyphs and upscales before reading, so a row of sliced descenders is content it will
    -- faithfully magnify. Widened to the pill's own dark interior, which spans y 16-54: all
    -- four corners of 20..48 sit inside it, clear of the red panel above and below.
    preset = { 480, 20, 720, 48 },
    next = { 738, 30 }, -- the right arrow
    save = { 88, 30 },
    browser = { at = { 807, 647 }, on = { 0xAA, 0xA8, 0x9F }, off = { 0x7E, 0x80, 0x86 } },
  },
  {
    name = "Hive 2",
    -- The class says "Hive", not "Hive2" — u-he kept the identifier across the major
    -- version. ReaHotkey names the plug-in "Hive 2" and matches the same class.
    class = "^com%.u%-he%.Hive%.vst%.window%d*$",
    logo = { 1174, 28 },
    prev = { 492, 28 },
    preset = { 560, 12, 720, 32 },
    next = { 756, 28 },
    save = { 990, 28 },
    browser = { at = { 936, 25 }, on = { 0xCC, 0xDF, 0xEC }, off = { 0x81, 0x87, 0x8D } },
  },
  {
    name = "Repro-1",
    class = "^com%.u%-he%.Repro1%.vst%.window%d*$",
    logo = { 64, 44 },
    prev = { 440, 53 },
    preset = { 460, 36, 670, 70 },
    next = { 730, 53 },
    save = { 808, 53 },
    -- ReaHotkey registers only this class but then IMAGE-CHECKS whether the panel on
    -- screen is Repro-1 or Repro-5 before offering the toggle, and refuses it for Repro-5.
    -- That check only makes sense if the class does not always discriminate the two. It
    -- cannot be settled here: Repro-5 is not installed on this machine, so its class, its
    -- layout and its toggle colours are all unmeasured, and building a guard against a
    -- layout nobody has seen would be guesswork wearing the clothes of caution. If a
    -- Repro-5 ever turns up under this class, this one control has to be gated on a
    -- Repro-1 landmark before it can be trusted — a toggle that clicks the right pixel of
    -- the wrong synth is worse than no toggle.
    browser = { at = { 310, 87 }, on = { 0xB3, 0x58, 0x4E }, off = { 0x99, 0x8D, 0x80 } },
  },
  -- The two legacy Zebras have no preset-browser toggle at all, in ReaHotkey or here:
  -- their preset browser is a separate page rather than something toggled from the header.
  {
    name = "Zebra2",
    class = "^com%.u%-he%.Zebra2%.vst%.window%d*$",
    logo = { 1056, 28 },
    prev = { 420, 28 },
    preset = { 440, 16, 670, 40 },
    next = { 760, 28 },
    save = { 922, 28 },
  },
  {
    name = "ZebraHZ",
    class = "^com%.u%-he%.ZebraHZ%.vst%.window%d*$",
    logo = { 1048, 28 },
    prev = { 420, 28 },
    preset = { 440, 16, 670, 40 },
    next = { 690, 28 },
    save = { 760, 28 },
  },
}

-- What three of these synths say after their browser is switched ON — and the one line in
-- this module that deliberately contradicts ReaHotkey.
--
-- ReaHotkey says "use the arrow keys to select a preset". Tested live on Diva and Repro-1,
-- that is not true here: with the PRESETS page open the arrow keys select nothing. (They
-- are not ours to blame — the runtime captures Left/Right only for a focused slider and
-- never touches Up/Down, and this overlay has no slider.) What DOES work is the preset name
-- itself: activating it opens a NATIVE context menu, which a screen reader reads out of the
-- box, and that is how a preset actually gets chosen.
--
-- Repeating ReaHotkey's sentence would be worse than saying nothing. A user who cannot see
-- the screen has no way to discover that the instruction failed rather than that they did
-- it wrong, so they would keep pressing keys into a UI that is ignoring them and conclude
-- the overlay is broken. The hint exists to remove a dead end, not to describe one — so it
-- names the path that works.
local BROWSER_ON_HINT = "presets are picked from the preset menu, Alt+M"

local function build(ov, s)
  ov:addStaticText(s.name)
  -- The u-he badge, which opens the global menu (size/zoom, preferences, the preset
  -- directories). Ctrl+U and Ctrl+S below are ReaHotkey's own bindings, kept so a
  -- ReaHotkey user's fingers still work — but note that while this overlay is active it
  -- OWNS them, so Ctrl+S saves a u-he preset rather than the DAW project.
  ov:addHotspotButton({ label = "u-he logo menu", at = s.logo, hotkey = "Ctrl+U", opensMenu = true })
  if s.browser then
    ov:addHotspotToggle({
      label = "Preset browser toggle",
      at = s.browser.at,
      onColor = s.browser.on,
      offColor = s.browser.off,
      hotkey = "Alt+B",
      hint = { on = BROWSER_ON_HINT },
    })
  end
  ov:addHotspotButton({ label = "Previous preset", at = s.prev, hotkey = "Alt+P" })
  -- The name of the loaded preset, and — as it turns out — the most important control here.
  --
  -- Focusing it READS the name, which OCR is not a fallback for but the only route to: u-he
  -- draws its entire UI itself and exposes no accessible text anywhere. Activating it CLICKS
  -- the name, like ReaHotkey's OCRButton, and that opens a NATIVE context menu — verified
  -- live on Diva and Repro-1. Native is the whole point: a screen reader announces it
  -- without help from us, so this one control is the accessible path to every preset, and
  -- the reason the browser toggle's hint points here.
  ov:addOCRButton({ label = "Preset menu", region = s.preset, hotkey = "Alt+M", opensMenu = true })
  ov:addHotspotButton({ label = "Next preset", at = s.next, hotkey = "Alt+N" })
  ov:addHotspotButton({ label = "Save preset", at = s.save, hotkey = "Ctrl+S" })
end

return {
  activate = function()
    for _, s in ipairs(SYNTHS) do
      local ov = O.new(s.name)
      build(ov, s)
      -- Embedded in a DAW only. None of these five ships a standalone application on
      -- Windows — they exist solely as plug-ins — so there is no window to attach to
      -- besides the host's. Komplete Kontrol can host them too (they are NKS-ready, and
      -- ReaHotkey supports that case); that is a follow-up, because a u-he overlay inside
      -- KK has to join KK's shared arbiter slot to beat KK's own chrome, and getting that
      -- ranking wrong is exactly the kind of change that must be verified against a real
      -- window rather than reasoned about.
      --
      -- `menus = true`, and it is load-bearing rather than precautionary: the preset menu
      -- these overlays open is a REAL Windows menu, not part of u-he's self-drawn UI, so
      -- while it is up the captured navigation keys have to reach it instead of steering the
      -- overlay. Without this, Return would never arrive at the one menu that chooses a
      -- preset.
      ov:attachEmbedded({ hosts = daw.all, control = s.class },
        { slot = SLOT, specificity = O.layer.base, menus = true })
    end
    host.log.info(string.format("u-he: %d synth overlays ready.", #SYNTHS))
  end,
}
