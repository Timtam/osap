-- VPS Avenger's layout, as this overlay clicks and reads it.
--
-- ── Where the numbers come from ──────────────────────────────────────────────
-- Every coordinate in this file is a STARTING VALUE taken, with permission, from the
-- avenger_control project: a third-party tool that makes VPS Avenger usable with VoiceOver in
-- Logic Pro, and the only prior art there is. Its tables are
--   * written at 50 % of Avenger's zoom, in points, from the top-left corner of Avenger's own
--     interface — not from any DAW's window, whose chrome above the plug-in does not zoom;
--   * fitted to button CENTRES by clicking until a press landed, in Logic Pro 11 with Avenger 2 at
--     50 % — click points that worked, not measured edges;
-- and its model is that every one of them grows linearly with the zoom: a point written at
-- (x, y) lies at (x, y) × zoom / 50 from Avenger's corner. That model is this overlay's scale
-- (O:scale, whose factor is the zoom read off Avenger's own zoom field — see header.luau) over
-- the DAW's panel, which daw-hosts places.
--
-- Nobody working on this module has an Avenger, so none of these numbers has been checked here.
-- Every one of them is clicked or read through the overlay and so lands in a calibration shot,
-- and the header's are drawn into a picture of their own at every identification in a
-- calibrating run (header.luau): the pictures a tester sends back are what settle them. TODO.md,
-- "VPS Avenger".

local G = {}

-- The zoom the tables are written at: a factor of 1 is 50 %.
G.AUTHORED_ZOOM = 50

-- The zooms Avenger offers, in the order its zoom list shows them: 50 to 100 in fives, then 110
-- to 200 in tens, 21 entries (the project's zoom table). A zoom field that reads anything else is
-- not taken for a zoom: "85%" is one, "87%" is a misread.
G.ZOOMS = {}
for z = 50, 100, 5 do G.ZOOMS[#G.ZOOMS + 1] = z end
for z = 110, 200, 10 do G.ZOOMS[#G.ZOOMS + 1] = z end

-- Where zoom `z` is in the list, or nil for a zoom Avenger does not offer.
function G.zoomIndex(z)
  for i, v in ipairs(G.ZOOMS) do
    if v == z then return i end
  end
  return nil
end

-- ── The header row ───────────────────────────────────────────────────────────
-- All at y 14 (the project's header table). Left to right: the zoom field — which opens the zoom
-- list, and whose second click closes it again — the preset name, ◀ and ▶, MENU and UNDO. UNDO
-- answers both buttons: a left click undoes, a right click opens the redo list. Right of UNDO
-- Avenger shows its voice count and CPU meter, which this module never reads — which matters,
-- because a CPU meter prints a percentage too (header.luau takes the zoom only LEFT of MENU).
G.ZOOM_FIELD = { 21, 14 }
G.PREV = { 220, 14 }
G.NEXT = { 232, 14 }
G.MENU = { 262, 14 }
G.UNDO = { 305, 14 }

-- The preset name, read and never clicked: the project's recognition box for it, x 34 to 214 and
-- y 7 to 22 — clear of the zoom field on its left and of ◀ on its right. The project's own
-- recogniser read "Init Preset" as "hit Presst" in it at 50 % (not measured here: TODO.md).
G.NAME_REGION = { 34, 7, 214, 22 }
-- Where the name is double-clicked to rename it in Avenger's own editor: the project's
-- `preset_name` point (its header_map.py), which it clicks for the same.
G.NAME = { 123, 14 }

-- ── MENU's items ─────────────────────────────────────────────────────────────
-- From the project, all at x 257: Load at y 32, Save as at 55, Initialize at 68 (its MENU table,
-- header_map.py's MENU_ITEMS). Its Save, "at about 43", it leaves out on purpose — Avenger's
-- overwrite warning was out of VoiceOver's reach — and so does this module since 0.16 (main.luau,
-- the maintainer's decision of 2026-10-31). Load and Save as open the DAW's file dialog and are
-- clicked at these points as the project clicks them (main.luau, "Load preset and Save preset as");
-- Initialize opens a warning box inside Avenger (dialog.luau), and is chosen in the popup seen. The
-- developer's Mac test of 2026-10-31 photographed MENU's popup at 80 %: its rows 19 pixels apart,
-- Load's from y 43 to 61 below Avenger's corner and Save as's from 81 to 99, Initialize's text from
-- 107 to 116 under a separator at 100, the popup from x 382 to 474 — and these points land at
-- (411, 51), (411, 88) and (411, 109), on each item.
G.LOAD = { 257, 32 }
G.SAVE_AS = { 257, 55 }
G.INITIALIZE = { 257, 68 }

-- ── The zoom list ────────────────────────────────────────────────────────────
-- Opened by the zoom field. Its first entry, 50 %, is centred at (27, 24), and the entries are
-- 11.5 apart — the project measured the rows alternating 11 and 12 points at 50 %, which is 23
-- per entry at 100 %, drawn at half size — all 21 of them in the order of G.ZOOMS. The list is
-- drawn at the zoom that was current when it opened, so its rows are 11.5 × the factor at that
-- moment (the project clicks entries "at the old zoom" for the same reason).
G.ZOOM_LIST_FIRST = { 27, 24 }
G.ZOOM_LIST_PITCH = 11.5

-- ── Initialize's warning box ─────────────────────────────────────────────────
-- "Warning / This will initialize all settings / Are you sure?" with YES and NO, drawn INSIDE
-- Avenger's own window (the project's words, and it clicks the buttons as ordinary points of the
-- plug-in): YES at (386, 310), NO at (487, 310) — centred on Avenger, whose middle at 50 % is
-- x 436. This module clicks the two words where it READS them (dialog.luau), and the table's pair
-- is only compared with that in the log and the picture.
G.YES = { 386, 310 }
G.NO = { 487, 310 }
-- Where the box is looked for: generous round the pair, and over the rows of text above them.
G.DIALOG_BAND = { 280, 200, 600, 336 }

-- ── The plug-in's size ───────────────────────────────────────────────────────
-- Avenger is about 871 points wide at 50 %: the tester's floating REAPER window on a Mac was 1394
-- wide at 80 % and Logic's 1398 with a two-point border each side (2026-09-27), and Avenger's own
-- child window on Windows 1394 at 80 % too (the first Windows session, 2026-10-30, the display at
-- 100 %), which is 871.25 at 50 %; the project's fit of several screenshots has a slope of 17.43
-- per percent, 871.5 at 50 %. Used for one thing only: how wide and tall a band the header is read
-- in (header.luau). It is never a factor, because Avenger's width inside a DAW carries an offset
-- nobody has measured, and the project's width formula reads 80 % as 75 %.
G.WIDTH_AT_50 = 871
-- The width those measurements give at 50 %, 1394 / 1.6. On Avenger's own window (Windows) the
-- factor the window's width gives — its width over this — is LOGGED beside the factor read off the
-- header, and never used (header.luau; the maintainer's decision of 2026-10-30): the child's width
-- holds no DAW frame there, so a session at a second zoom says whether it follows the zoom — at 75
-- % it would be 1307 wide, give or take one. Not on a DAW's panel, whose width is the DAW's
-- window's and holds whatever it draws beside the plug-in.
G.WIDTH_AT_50_MEASURED = 871.25
-- And about 561 points tall at 50 %: 897 at 80 % in the same log of the tester's, which is 560.6 —
-- worked out from where Logic's header ends and its footer begins, not measured on its own. Used
-- beside the width for the header's band only (header.luau), for a DAW that keeps its window
-- narrower than the plug-in. The Windows child was 894 tall at 80 % (2026-10-30), 558.75 at 50 %;
-- left at 561 for now, since it moves the band wherever the height decides it (TODO.md).
G.HEIGHT_AT_50 = 561

-- ── Avenger's corner ─────────────────────────────────────────────────────────
-- Where the tables start, relative to the DAW's plug-in panel: 2 points left of the panel's
-- corner and 2 below it. ONE constant, and unverified. On Avenger's own window (Windows), 2 pixels
-- left of the child's corner and 2 above it: G.OWN_WINDOW_DIFF, below.
--
-- The evidence is a difference that is the same in the two hosts the project has figures for:
--   * Logic: the project's tables start at the window's frame + (0, 90); daw-hosts' panel starts
--     at the frame + (2, 88) — Logic's content origin is its frame today (modules/daw-hosts).
--   * REAPER's floating window: the project's tables start at the frame + (-2, 52); daw-hosts'
--     panel at the content origin + (0, 22), which is the frame + (0, 50) under the 28-point
--     title bar docs/macos-port.md measured. The project gives its REAPER figures no provenance.
-- Tool minus ours is (-2, +2) both times. The same difference in two unrelated hosts has one of
-- two causes, and they exclude each other:
--   (a) the TABLES: fitted to button centres by clicking until a press landed, their points sit a
--       little off Avenger's real corner. Then the difference is in the tables' own units, written
--       at 50 %, so it grows with the zoom, and it holds on every origin — Avenger's own child
--       window on Windows as well;
--   (b) the ORIGINS: daw-hosts' panel is 2 points off Avenger's corner in both DAWs (Logic's 88 was
--       derived from REAPER's 22 through the same "80%" caption read in both, so one error can be
--       in both). Then it is the same at every zoom, it is daw-hosts' to correct — Kontakt and
--       sforzando on a Mac panel are placed from the same origin — and it does not apply on
--       Windows.
-- Neither is known yet. This module takes it as (b) for now — unscaled, and on a DAW's panel only
-- — because that is what the two numbers were measured as, and keeps it HERE, one named constant,
-- until the pictures say which: moving it into daw-hosts on a guess would move every other
-- plug-in's clicks with it. 2 points is a sixth of the 12 between ◀ and ▶ at 50 %.
--
-- Added in the overlay's FRAME, which the scale does not touch: a point is placed at the panel's
-- corner + (-2, 2) + factor × (x, y). Only on a DAW's panel (class "host-panel",
-- docs/api/overlay.md, attachEmbedded on macOS); on Avenger's own window, below. Every
-- identification logs how far MENU and UNDO were read from where this puts them, and from where
-- they would be with no difference added (header.luau), at every zoom it is read at: the same
-- offset at two zooms is (b), one that grows with the zoom is (a). TODO.md, "The origin
-- difference".
G.ORIGIN_DIFF = { -2, 2 }

-- On Avenger's OWN window — the child whose class begins with "KalliGuiWin" (src/main.luau's
-- `control`), a property of the window, which only Windows shows — Avenger is drawn 2 pixels left
-- of the child's corner and 2 above it: (-2, -2), and ONE constant again, measured at one zoom
-- only. The third Windows session (2026-11-01, Avenger at 80 % on a display at 100 %) gave four
-- measures that agree within about a pixel:
--   * the strip's first dot, its middle 393.5 from the child's corner against the 395.6 the table
--     puts it at (T.DOT_MIDDLE 247.25 × 1.6, tables/strip.luau) — the developer's Mac read it at
--     395.5 from his frame — and its rows 2 higher than the Mac's from his;
--   * the macro knobs' centre of brightness on Init Preset, by the same reader: (-1.6, -1.2) from
--     the Mac's;
--   * the oscillator's pointer on Init Preset: 258° against the Mac's 267°, a middle about 2 off at
--     a pointer about 12 long — which read its Level 90 % and its macros 10 % where the Mac reads
--     100 % and 0 % (read/knob.luau rounds both a step up);
--   * MENU's and UNDO's words, from other recognisers: 3 to 3.5 left and 1 to 2.5 up.
-- The avenger_control developer's own Windows origin, the FX window's corner + (215, 102), agrees in
-- y; its x is either -3 or +5 off, by which of his two left edges is taken. With (0, 0) the grid's
-- bottom scrollbar click landed on the very end of its track and moved nothing (the session's Open
-- of Vocal Trance), where the second session's, pulled 2.4 px up and left by a factor wrong by
-- 0.7 %, had landed on the Mac's pixel. Whether it is the same at every zoom is not known; a
-- constant 2 leaves less error than 0 either way — 0.5 px at 100 % and 0.75 at 50 % if it grows
-- with the zoom, against 2.5 and 1.25. TODO.md, "The third Windows session (2026-11-01)".
G.OWN_WINDOW_DIFF = { -2, -2 }

-- The frame: `O:frame`'s function, and what every read of this module that is made before the
-- overlay is active (header.luau, dialog.luau) places its regions with. Any other origin, which
-- this module is never bound to, adds nothing.
function G.frameOf(origin)
  if origin and origin.class == "host-panel" then return G.ORIGIN_DIFF[1], G.ORIGIN_DIFF[2] end
  if origin and string.find(tostring(origin.class or ""), "^KalliGuiWin") then
    return G.OWN_WINDOW_DIFF[1], G.OWN_WINDOW_DIFF[2]
  end
  return 0, 0
end

-- Authored point (x, y) on screen against `origin`, with factor `k`: the runtime's own formula
-- (origin + frame + k × authored, rounded once, half up — docs/api/overlay.md, O:scale), for a
-- read made while the overlay is not active yet and `o:toScreen` has no origin to place it with.
function G.place(origin, k, x, y)
  local fx, fy = G.frameOf(origin)
  return math.floor(origin.client.x + fx + k * x + 0.5), math.floor(origin.client.y + fy + k * y + 0.5)
end

-- Authored rectangle `r` on screen, likewise, cut to `origin`'s own rectangle: a read reaching
-- past the plug-in reads another window's pixels. nil when nothing of it is left.
function G.placeRect(origin, k, r)
  local x1, y1 = G.place(origin, k, r[1], r[2])
  local x2, y2 = G.place(origin, k, r[3], r[4])
  local c = origin.client
  x1, y1 = math.max(x1, c.x), math.max(y1, c.y)
  x2, y2 = math.min(x2, c.x + c.w), math.min(y2, c.y + c.h)
  if x2 - x1 < 2 or y2 - y1 < 2 then return nil end
  return { x1, y1, x2, y2 }
end

return G
