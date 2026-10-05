-- Avenger's header, read: which plug-in the panel shows, and at which zoom.
--
-- ONE READ ANSWERS BOTH. Avenger publishes no accessibility element at all — not in REAPER, not
-- in Logic (the tester's probe of 2026-09-27) — so the only evidence of what a DAW's panel holds
-- is what is drawn in it. Avenger's header row draws three things that say who it is and how big
-- it is drawn: its zoom field, with the zoom as text ("80%"), and the captions MENU and UNDO. One
-- text read of a band across the top of the panel finds all three:
--
--   * IDENTITY: MENU and UNDO in capitals on one row, at distances from Avenger's corner in the
--     table's proportion (262 to 305, geometry.luau), at the header's height for the factor those
--     distances give — and a factor that FITS them (below). The proportion does not depend on the
--     zoom, so it is asked before the zoom is known — and it is what a Logic window titled "Inst 1"
--     leaves to go on: the title never names the plug-in there (the maintainer's rule: identity is
--     the plug-in's own evidence inside the panel, never the window title). A plug-in with "Menu"
--     and "Undo" of its own in a similar place is not taken for Avenger: mixed case is not
--     Avenger's, and the zoom field's caption or the header's size has to agree with the captions'
--     distances on one of Avenger's steps. It also guards against an Avenger update that moves the
--     header: a MENU that is not where the table has it is not clicked.
--   * THE ZOOM: the zoom field's caption, read as text, one of Avenger's 21 steps — not worked
--     out from the window's width: the project's width formula reads 80 % as 75 %, which puts a
--     click on UNDO 30 points off, and inside a DAW the plug-in's width carries an offset nobody
--     has measured. The zoom is what the Zoom control says and which entry of the zoom list is
--     "the one beside it".
--   * THE FACTOR every coordinate is scaled by (O:scale), from both — see "The factor" below.
--
-- Read with a callback (host.ocr.recognize), as the runtime's identity rule asks of a panel: on a
-- Mac a read that finds nothing takes a quarter of a second, a wait for it would hold this
-- module's keys, and `identify` is asked in every plug-in window of every DAW the keyboard comes
-- into. So `identify` answers nil —
-- "could not tell yet" — until the answer lands, and then asks every overlay to look again
-- (host.window.recheck), exactly as sforzando's wordmark and Kontakt 7's top row do. A read that
-- saw nothing (blank, none, failed) is no answer and is asked again, and so is one that is only
-- half an answer: one of MENU and UNDO read and not the other, captions read only with boxes the
-- recogniser guessed, or Avenger's captions with no factor that fits them. Text without Avenger's
-- header is a real "no". The runtime keeps a "no" — and bounds the "could not tell yet" at eight
-- in a row — for the stay on a Mac's panel, and for as long as it keeps the control on Windows,
-- where a JUCE child has no stay (docs/api/overlay.md, O.memoByOrigin): a "no" there is final for
-- that editor, which is why only text without the header is one.
--
-- Kept per window, size and STAY (the runtime's word for the keyboard inside one window's panel,
-- which a visit to the DAW's own controls ends): one REAPER FX chain window shows whichever FX is
-- selected, and Logic's plug-in window can most likely be switched to another insert in place
-- (Link mode, not measured), so nothing here is kept per window alone. On Windows a control has
-- no stay: kept per control and size. The runtime asks `identify` there once per CONTROL, and a
-- zoom step changes the child's size, not the control, so a new size of a control this module has
-- read as Avenger's is Avenger's still: its first read there is a read for the zoom, and a read
-- that misses the header there is "lost", read again, never a "no".

local G = host.include("src/geometry.luau")

local H = {}

-- The factor, from the zoom and from where MENU and UNDO are READ.
--
--   factor = zoom / 50 × display factor
--
-- The display factor is 1 on a DAW's plug-in panel — a coordinate there is a point of the DAW's
-- window (docs/api/overlay.md, attachEmbedded on macOS), and the zoom alone is the whole factor,
-- Retina or not. On Avenger's own child window (Windows) it is not known: a coordinate there is a
-- physical pixel, Avenger is drawn 1.25 or 1.5 times as large at 125 or 150 % display scaling, and
-- nothing in the host says which scaling a window is drawn at (docs/api/overlay.md, O:scale,
-- Windows). There it is the ratio between the factor MENU and UNDO are read at and the one the
-- zoom alone gives, rounded to a quarter and never under 1. The read header then has to FIT:
-- MENU and UNDO within FIT_TOL of where the factor puts them, and nearer that zoom's step than any
-- other, or there is no factor — a zoom field misread as another step, a DAW origin far off, a
-- display scaled to 110 % (a quarter from anything standard) all end there, said in the log,
-- rather than in a click at a place that looked right.
--
-- FIT_TOL is what reading the captions can be off by. Their distances from the corner are 262
-- and 305 points at 50 %, and a word's centre read a few points off, with the corner off by the
-- two points of geometry.luau's difference, is one to two per cent there, less at larger zooms;
-- 3 % holds that, and a header read 12 points right at 80 % (2.6 %) still fits. On a panel a
-- caption misread as another step is always refused: the nearest two steps are 5.3 % apart. On
-- Windows the display factor absorbs some misreads — 60 % read as "50%" at 150 % display is
-- 1.8 against 1.75, 2.9 % — which the step check does not catch either: that residue is in TODO.md.
--
-- A zoom field that cannot be read (the project's recogniser mangled small text at 50 %) leaves
-- the header's own size. On a panel, where the display factor is 1, the zoom is the step the size
-- lands within SEEN_TOL of — taken as seen rather than read — and there is none when it lands
-- near no step: SEEN_TOL is under half the smallest gap between two steps (5.3 %), so at most one
-- is that near. On Windows the size alone cannot tell 80 % at a display of 125 % from 100 % at 100
-- %: the factor is the header's size, and the zoom is NOT known — the Zoom control says it cannot
-- be read and opens nothing, rather than choosing a row of the list for a zoom it guessed.
local RATIO_TOL = 0.06
local FIT_TOL = 0.03
local SEEN_TOL = 0.015

-- How wide and tall the band is that the header is read in, as a share of Avenger's width at the
-- panel's size — UNDO ends at about 312 of 871, and the header's text at 22 + 2 of 871 below the
-- corner — with that width taken as at least Avenger's at 50 %. Relative to the width because the
-- header grows with the zoom and Avenger fills its panel across (a DAW sizes its plug-in window to
-- the plug-in), so one proportion covers 50 % and 200 % alike, and Windows at any display scaling.
-- Avenger's width is the panel's, or what the panel's height gives in Avenger's proportions when
-- that is more: a DAW that keeps its window narrower than a large plug-in (not measured: TODO.md)
-- would otherwise cut UNDO off the band.
local BAND_ACROSS = 0.45
local BAND_DOWN = 32 / G.WIDTH_AT_50

-- Reads of one window, size and stay that brought no zoom at all (nothing read, no zoom field in
-- the text, only half the header, no factor): after this many the header is not read again for
-- it — a count, not a clock — and the overlay places nothing there until the window changes size
-- or the keyboard comes in again. The runtime's own cap on "could not tell yet" is eight
-- evaluations.
local READS_MAX = 6

-- A zoom step of ours has been chosen in the zoom list: the zoom field is read again until it
-- shows another zoom, at most this many times — Avenger may not have redrawn by the first read.
-- The step's own `settle` (main.luau) bounds how long the Zoom control waits for it.
local STEP_READS_MAX = 8

-- A stay taken as Avenger's because our own popup or zoom step ended the last one (`carried`,
-- below) is confirmed by its first read that finds the header, and overturned by this many reads
-- in a row that find text without it: one could be a frame drawn mid-redraw, two are what the
-- panel shows. Overturned for this module; the runtime keeps a panel's verdict for the stay, so
-- the overlay leaves the front only when the stay ends — until then nothing is placed in it.
local CARRIED_MISSES = 2

-- One entry per window, size and stay, the last STATES_MAX asked about: one window is in front
-- at a time, and a window that closed is never asked about again.
local STATES_MAX = 16
local states, order = {}, {}

-- The window whose header was last read as Avenger's, and at which size: for the stay our own
-- popup or zoom step ends (below, `carried`). One slot — one window is in front at a time, and a
-- step is made in it.
local lastAvenger, lastSize = nil, nil

-- The zoom step in flight: { id = window, from = zoom, reads = n }, or nil.
H.step = nil

-- The window in which this module has opened one of Avenger's popups since the overlay last came
-- to the front, or nil; and whether that popup is the zoom list, whose choice resizes the window
-- (below, `carried`).
local popupIn, popupZoom = nil, false

local function sizeOf(ctrl)
  return string.format("%dx%d", ctrl.client.w, ctrl.client.h)
end

local function keyOf(ctrl)
  return string.format("%s|%s|%s", tostring(ctrl.id), sizeOf(ctrl), tostring(ctrl.stay))
end

-- A state for `ctrl`'s window, size and stay. A new one INHERITS "Avenger" from another state of
-- the same control in the same stay that was read as Avenger's: on Windows, where there is no
-- stay, that is the control the runtime already holds as Avenger's at another size (see the top
-- of this file); on a Mac a new stay never finds one — a resize ends the stay there.
local function stateFor(ctrl)
  local key = keyOf(ctrl)
  local st = states[key]
  if st then return st end
  st = { key = key, id = ctrl.id, size = sizeOf(ctrl), stay = tostring(ctrl.stay), reads = 0,
    verdict = nil, zoom = nil, k = nil, stale = false }
  for _, other in pairs(states) do
    if other.id == st.id and other.stay == st.stay and other.verdict == true then
      st.verdict, st.inherited = true, true
      break
    end
  end
  states[key] = st
  order[#order + 1] = key
  if #order > STATES_MAX then
    states[table.remove(order, 1)] = nil
  end
  return st
end

-- Lines said once per kind and key, so an evaluation per focus event does not repeat them. What
-- says the plug-in is NOT Avenger, or that nothing could be read, is keyed on the window and its
-- size alone: every plug-in window of every DAW is asked once per stay, and a line per visit to
-- another plug-in would be a line per Tab back from the DAW's own controls, for days. What says it
-- IS Avenger is said at each stay: it is the measurement a session is after.
local said = {}
local saidOrder = {}
local function sayOnce(kind, key, fmt, ...)
  local k = kind .. "|" .. key
  if said[k] then return end
  said[k] = true
  saidOrder[#saidOrder + 1] = k
  if #saidOrder > 4 * STATES_MAX then said[table.remove(saidOrder, 1)] = nil end
  host.log.info(string.format(fmt, ...))
end

local function titleOf(ctrl)
  local a = host.window.active()
  if a and a.id == ctrl.id then return tostring(a.title) end
  return "window " .. tostring(ctrl.id)
end

local function forLog(text)
  local s = string.gsub(text or "", "[\r\n]+", " ")
  if #s > 80 then s = string.gsub(string.sub(s, 1, 80), "[\192-\255][\128-\191]*$", "") .. "…" end
  return s
end

-- ── Reading the header ───────────────────────────────────────────────────────

-- The language every read of this module asks for: English — Avenger's words are English — where
-- the platform's recogniser reads it, and otherwise the user's own. Asking for English where it is
-- not installed (a German Windows without English OCR) fails every read outright, which would keep
-- the overlay down for good; the user's own language reads these capitals, digits and names well
-- (docs/api/ocr.md, "Recognition language"). Resolved to one tag, so that the name read on the
-- event loop and the ones read off it are read by the same engine and can be compared. nil only
-- while the platform's list is not known yet, in the first moments after the application starts.
function H.lang()
  local ok, v = pcall(function() return host.ocr.resolveLanguage("en") end)
  if ok and v then return v end
  ok, v = pcall(function() return host.ocr.resolveLanguage(nil) end)
  return ok and v or nil
end

-- `word` is `caption` — MENU or UNDO, in capitals as Avenger draws them — or the caption with one
-- character misread ("MFNU", "UNDo"): the captions are short, and a recogniser's one slip should
-- not cost the overlay. A zero for an O is no slip at all ("UND0"). A lower-case letter is a
-- character that differs, so another plug-in's "Menu" and "Undo" are three off, not Avenger's.
-- Two words one character apart are still MENU or UNDO only where the table puts them, which the
-- geometry below checks.
local function isCaption(word, caption)
  local w = string.gsub(word or "", "[^%w]", "")
  if #w ~= #caption then return false end
  local off = 0
  for i = 1, #w do
    local a, b = string.sub(w, i, i), string.sub(caption, i, i)
    if a ~= b and not (b == "O" and a == "0") then off += 1 end
  end
  return off <= 1
end

-- The zoom in a caption such as "80%", "80 %" or "8O%", when it is one of Avenger's steps.
local function zoomOfText(text)
  local s = string.gsub(text or "", "%s+", "")
  s = string.gsub(s, "[Oo]", "0")
  local digits = string.match(s, "^(%d%d%d?)%%")
  local z = digits and tonumber(digits)
  if z and G.zoomIndex(z) then return z end
  return nil
end

local function centre(w) return w.x + w.w / 2, w.y + w.h / 2 end

-- MENU and UNDO where the table puts them, against Avenger's corner (cx, cy): the factor their
-- distances from the corner give, or nil and why not.
local function fitCaptions(m, u, cx, cy)
  local mx, my = centre(m)
  local ux, uy = centre(u)
  local dm, du = mx - cx, ux - cx
  if dm <= 0 or du <= dm then return nil, "UNDO is not right of MENU" end
  if math.abs(my - uy) > math.max(m.h, u.h) / 2 + 2 then return nil, "MENU and UNDO are not on one row" end
  local ratio = (dm / G.MENU[1]) / (du / G.UNDO[1])
  if math.abs(ratio - 1) > RATIO_TOL then
    return nil, string.format("MENU and UNDO are %.0f and %.0f from Avenger's corner, not in the "
      .. "table's proportion of %d to %d", dm, du, G.MENU[1], G.UNDO[1])
  end
  local kx = (dm * G.MENU[1] + du * G.UNDO[1]) / (G.MENU[1] ^ 2 + G.UNDO[1] ^ 2)
  local row = (my + uy) / 2 - cy
  if math.abs(row - G.MENU[2] * kx) > 4 + 3 * kx then
    return nil, string.format("MENU and UNDO are %.0f below Avenger's corner, where the header is "
      .. "at %.0f for their size", row, G.MENU[2] * kx)
  end
  return kx
end

-- The header in `words`, against Avenger's corner (cx, cy): { menu, undo, kx, zoomWord, zoom } —
-- or nil, why not, and whether that is only half an answer ("partial"): one of MENU and UNDO read
-- and not the other, or either read only in a box the recogniser shared out by the text's length
-- (`approx`, the neural fallback on Windows: docs/api/ocr.md) rather than one it located. Such a
-- box can sit anywhere along its line, and a distance measured from it is a factor made up, so
-- those words are no evidence of where anything is — not of a MENU, not of the zoom's caption.
local function parseHeader(words, cx, cy)
  local menus, undos, zooms = {}, {}, {}
  local guessed = false
  for i, w in ipairs(words or {}) do
    if w.approx then
      if isCaption(w.text, "MENU") or isCaption(w.text, "UNDO") then guessed = true end
    else
      if isCaption(w.text, "MENU") then menus[#menus + 1] = w end
      if isCaption(w.text, "UNDO") then undos[#undos + 1] = w end
      local z = zoomOfText(w.text)
      local box = w
      -- "80 %" can come back as two words: the number, and the sign on its own just right of it.
      if not z and string.match(w.text or "", "^%s*%d%d%d?%s*$") then
        local nx = words[i + 1]
        if nx and not nx.approx and string.match(nx.text or "", "^%s*%%") and nx.x - (w.x + w.w) < w.h then
          z = zoomOfText((w.text or "") .. "%")
          box = { x = w.x, y = w.y, w = nx.x + nx.w - w.x, h = math.max(w.h, nx.h) }
        end
      end
      if z then zooms[#zooms + 1] = { box = box, zoom = z, text = w.text } end
    end
  end
  if #menus == 0 or #undos == 0 then
    if #menus == 0 and #undos == 0 then
      if guessed then
        return nil, "MENU and UNDO were read only in boxes the recogniser guessed", "partial"
      end
      return nil, "neither MENU nor UNDO was read"
    end
    return nil, #menus == 0 and "no MENU was read" or "no UNDO was read", "partial"
  end
  local why = nil
  for _, m in ipairs(menus) do
    for _, u in ipairs(undos) do
      local kx, no = fitCaptions(m, u, cx, cy)
      if kx then
        local h = { menu = m, undo = u, kx = kx }
        -- The zoom field is the first thing on the header row: LEFT of MENU (a CPU meter right of
        -- UNDO prints a percentage too), left of where the preset's name begins (a preset may
        -- well be called "Pad 100%"), and on MENU's row; of several, the one nearest where the
        -- table puts the field.
        local mx, my = centre(m)
        local nameStarts = cx + G.NAME_REGION[1] * kx
        local best
        for _, z in ipairs(zooms) do
          local zx, zy = centre(z.box)
          if zx < mx and zx < nameStarts and math.abs(zy - my) <= math.max(z.box.h, m.h) / 2 + 2 then
            local off = math.abs(zx - (cx + G.ZOOM_FIELD[1] * kx))
            if not best or off < best.off then best = { z = z, off = off } end
          end
        end
        if best then h.zoomWord, h.zoom = best.z.box, best.z.zoom end
        return h
      end
      why = why or no
    end
  end
  return nil, why
end

-- The step of Avenger's zoom that factor `k` lands nearest, and how far off it, as a share.
local function nearestStep(k)
  local best
  for _, z in ipairs(G.ZOOMS) do
    local off = math.abs(k / (z / G.AUTHORED_ZOOM) - 1)
    if not best or off < best.off then best = { z = z, off = off } end
  end
  return best.z, best.off
end

-- The display factor of `ctrl` where it is known without reading: 1 on a DAW's plug-in panel
-- (see "The factor"), nil on Avenger's own window.
local function knownDisplay(ctrl)
  if ctrl.class == "host-panel" then return 1 end
  return nil
end

-- The factor for header `h` in `ctrl` (see "The factor" above): k, zoom (nil when it is not
-- known), how ("read" or "seen"), the display factor (nil when it is not known) — or nil and why
-- not.
local function factorOf(h, ctrl)
  local fixed = knownDisplay(ctrl)
  if h.zoom then
    local k0 = h.zoom / G.AUTHORED_ZOOM
    local q = fixed or math.max(1, math.floor(h.kx / k0 * 4 + 0.5) / 4)
    local k = k0 * q
    if math.abs(h.kx / k - 1) > FIT_TOL then
      return nil, string.format("the zoom field reads %d %%, and MENU and UNDO are read where a factor of "
        .. "%.3f puts them, %.0f %% off the %.3f that zoom gives at a display factor of %.2f",
        h.zoom, h.kx, math.abs(h.kx / k - 1) * 100, k, q)
    end
    -- Neighbouring steps are 5 to 10 % apart, so a caption misread as the step beside it ("85%"
    -- for 80 %) might still fit — and would put UNDO 30 points off at 80 %. The step the header's
    -- own size lands nearest has to be the one the caption says.
    local seen = nearestStep(h.kx / q)
    if seen ~= h.zoom then
      return nil, string.format("the zoom field reads %d %%, but MENU and UNDO sit where %d %% puts them "
        .. "at a display factor of %.2f", h.zoom, seen, q)
    end
    return k, h.zoom, "read", q
  end
  if fixed then
    local z, off = nearestStep(h.kx / fixed)
    if off <= SEEN_TOL then
      return z / G.AUTHORED_ZOOM * fixed, z, "seen", fixed
    end
    return nil, string.format("the zoom field could not be read, and MENU and UNDO are read where a "
      .. "factor of %.3f puts them, %.1f %% from %d %%, the nearest step of Avenger's zoom", h.kx,
      off * 100, z)
  end
  -- Avenger's own window, its display's scaling not known, its zoom not read: the size is the
  -- factor, and the zoom is not known.
  return h.kx, nil, "seen", nil
end

-- In a calibrating run, one picture per zoom and window size: the header band, with a crosshair
-- where the overlay clicks each of the zoom field, ◀, ▶, MENU and UNDO (marks 1 to 5), then on
-- the centre of each word read — the zoom caption when there is one, MENU, UNDO. Side by side,
-- the first five and the last three are the origin question of geometry.luau answered in a
-- picture, at every zoom the tester visits.
local shots = {}
local function headerShot(ctrl, band, k, zoom, h)
  if not host.calibrating then return end
  local name = string.format("header-%dpct-%dx%d", zoom or 0, ctrl.client.w, ctrl.client.h)
  if shots[name] then return end
  shots[name] = true
  local marks = {}
  for _, p in ipairs({ G.ZOOM_FIELD, G.PREV, G.NEXT, G.MENU, G.UNDO }) do
    local x, y = G.place(ctrl, k, p[1], p[2])
    marks[#marks + 1] = { x = x, y = y }
  end
  for _, w in ipairs({ h.zoomWord or false, h.menu, h.undo }) do
    if w then
      local x, y = centre(w)
      marks[#marks + 1] = { x = math.floor(x + 0.5), y = math.floor(y + 0.5) }
    end
  end
  local path = host.path("calibration/" .. name .. ".png")
  -- Guarded: a path that cannot be written raises, and a measuring instrument must not take the
  -- answer it was taken with — here, the identification — down with it.
  local fine, ok, why = pcall(host.screen.saveMarked, path, { region = band, marks = marks })
  if not fine then ok, why = false, ok end
  host.log.info(string.format("[avenger] [calibrate] header picture, (%d,%d)-(%d,%d): marks 1-5 where the "
    .. "overlay clicks the zoom field, previous, next, MENU and UNDO at factor %.4f; the rest the words "
    .. "read -> %s (%s)", band[1], band[2], band[3], band[4], k, path,
    ok and "true" or ("false: " .. tostring(why))))
end

-- The band the header is read in, on screen: from the panel's corner, BAND_ACROSS of Avenger's
-- width across and BAND_DOWN of it down — or nil when the panel leaves no room.
local function bandOf(ctrl)
  local c = ctrl.client
  local w = math.max(c.w, c.h * G.WIDTH_AT_50 / G.HEIGHT_AT_50, G.WIDTH_AT_50)
  local x1, y1 = math.floor(c.x), math.floor(c.y)
  local bw = math.min(c.w, math.ceil(w * BAND_ACROSS))
  local bh = math.min(c.h, math.ceil(w * BAND_DOWN))
  if bw < 2 or bh < 2 then return nil end
  return { x1, y1, x1 + bw, y1 + bh }
end

-- The zoom and the factor read for `st`, unless a zoom step of ours is still waiting for the zoom
-- field to show another zoom.
local function takeZoom(st, k, zoom, how)
  local p = H.step
  if p and p.id == st.id then
    if zoom == nil or zoom == p.from then
      p.reads += 1
      if p.reads < STEP_READS_MAX then
        st.k, st.zoom, st.stale = nil, nil, true
        return false
      end
      host.log.info(string.format("[avenger] the zoom field still reads %s after %d reads since the zoom "
        .. "step: taken as what it is", zoom and (zoom .. " %") or "no zoom", p.reads))
    end
    H.step = nil
  end
  st.k, st.zoom, st.how, st.stale = k, zoom, how, false
  return true
end

-- Where MENU and UNDO were read against where the table, factor `k` and Avenger's corner (cx, cy)
-- put them: THE measurement line. A difference that holds at two zooms is an origin; one that
-- grows with the zoom is the tables (geometry.luau). On Avenger's own window (Windows), where no
-- difference is added, the same against the corner with geometry.luau's (-2, +2) added as well, so
-- that one Windows session says which of the two the difference is.
local function measured(ctrl, h, k, cx, cy)
  local mx, my = centre(h.menu)
  local ux, uy = centre(h.undo)
  local fx, fy = G.frameOf(ctrl)
  local function against(ax, ay)
    return string.format("(%+.1f,%+.1f) and (%+.1f,%+.1f)", mx - (ax + k * G.MENU[1]),
      my - (ay + k * G.MENU[2]), ux - (ax + k * G.UNDO[1]), uy - (ay + k * G.UNDO[2]))
  end
  local line = string.format("MENU read at (%.0f,%.0f), UNDO at (%.0f,%.0f): %s from where the table "
    .. "puts them with Avenger's corner at the panel's %+d,%+d", mx, my, ux, uy, against(cx, cy), fx, fy)
  if fx == 0 and fy == 0 then
    local d = G.ORIGIN_DIFF
    line = line .. string.format(", %s with it at %+d,%+d", against(cx + d[1], cy + d[2]), d[1], d[2])
  end
  return line
end

local function answered(ctrl, st, token, band, cx, cy, title, r)
  if states[st.key] ~= st or st.reading ~= token then return end
  st.reading = nil
  -- A newer read of this module's header replaced this one before it was recognised; that one
  -- answers for itself, and until then nothing is known that was not known before.
  if r.status == "stale" then return end
  st.reads += 1
  local winKey = string.format("%s|%dx%d", tostring(ctrl.id), ctrl.client.w, ctrl.client.h)
  local area = string.format("%d,%d %dx%d", band[1], band[2], band[3] - band[1], band[4] - band[2])
  local where = string.format("'%s' (%dx%d)", title, ctrl.client.w, ctrl.client.h)
  if r.status ~= "text" then
    -- Nothing read is no answer — a view that has not painted yet looks exactly like this — so
    -- the verdict stays open, and the next evaluation reads again.
    sayOnce("nothing", winKey, "[avenger] %s: the header band at %s read nothing (%s%s) — asked again "
      .. "at the next evaluation", where, area, tostring(r.status), r.error and (": " .. r.error) or "")
    if H.step and H.step.id == st.id then takeZoom(st, nil, nil, nil) end
    return
  end
  local undecided = st.verdict == nil
  local h, why, partial = parseHeader(r.words, cx, cy)
  if not h then
    if undecided and not partial then
      st.verdict = false
      if lastAvenger == st.id then lastAvenger = nil end
      sayOnce("no", winKey, "[avenger] %s: not VPS Avenger — %s in the header band at %s: \"%s\"",
        where, tostring(why), area, forLog(r.text))
      host.window.recheck()
    elseif undecided then
      -- Half an answer is no answer: read again, up to READS_MAX, and the runtime's own bound on
      -- "could not tell yet" after that.
      host.log.info(string.format("[avenger] %s: %s in the header band at %s: \"%s\" — read again",
        where, tostring(why), area, forLog(r.text)))
    elseif st.carried then
      st.misses = (st.misses or 0) + 1
      if st.misses >= CARRIED_MISSES and not partial then
        -- The stay we carried over is something else after all: the panel shows another plug-in.
        st.verdict, st.carried = false, nil
        if lastAvenger == st.id then lastAvenger = nil end
        takeZoom(st, nil, nil, nil)
        host.log.info(string.format("[avenger] %s: not VPS Avenger after all — %s in %d reads in a row "
          .. "since the stay was carried over: \"%s\"", where, tostring(why), st.misses, forLog(r.text)))
        host.window.recheck()
      else
        host.log.info(string.format("[avenger] %s: the header is not read where it was (%s): \"%s\" — "
          .. "read again", where, tostring(why), forLog(r.text)))
        takeZoom(st, nil, nil, nil)
      end
    else
      sayOnce("lost", st.key, "[avenger] %s: the header is not read where it was (%s): \"%s\" — nothing "
        .. "is placed until it is", where, tostring(why), forLog(r.text))
      takeZoom(st, nil, nil, nil)
    end
    return
  end
  st.misses = 0
  -- The factor and the zoom — or nil and, in the zoom's place, why there is no factor.
  local k, zoom, how, q = factorOf(h, ctrl)
  local line = measured(ctrl, h, k or h.kx, cx, cy)
  local field = h.zoomWord and string.format("the zoom field reads %d %%", h.zoom) or "the zoom field was not read"
  if not k then
    local noFactor = zoom
    takeZoom(st, nil, nil, nil)
    if undecided then
      -- Avenger's captions where the table has them, but nothing Avenger's zoom can be fits them:
      -- not taken as Avenger's (it would take Tab, Return and Space from whatever this is, to
      -- place nothing), and read again.
      host.log.info(string.format("[avenger] %s: the header reads like VPS Avenger's, but no factor: %s — "
        .. "not taken as VPS Avenger until one fits; read again. %s", where, tostring(noFactor), line))
    else
      host.log.info(string.format("[avenger] %s: the header again — but no factor: %s; nothing is "
        .. "clicked or read there until there is one. %s", where, tostring(noFactor), line))
    end
    return
  end
  if undecided then st.verdict = true end
  lastAvenger, lastSize = st.id, st.size
  -- A carried stay confirmed: the popup that ended the last one is over. Kept only while a zoom
  -- step is still to resize the window.
  if st.carried and not popupZoom and not (H.step and H.step.id == st.id) then popupIn = nil end
  st.carried = nil
  if takeZoom(st, k, zoom, how) then
    local what
    if zoom then
      what = string.format("zoom %d %% (%s), factor %.4f, display factor %.2f", zoom,
        how == "read" and "read" or "seen from the header's size, not read", k, q)
    else
      what = string.format("zoom not known (the display's scaling is not known, and the zoom field "
        .. "was not read), factor %.4f from the header's size", k)
    end
    host.log.info(string.format("[avenger] %s: %s, %s — %s. %s", where,
      undecided and "VPS Avenger" or "the header again", field, what, line))
    headerShot(ctrl, band, k, zoom, h)
  end
  if undecided then host.window.recheck() end
end

-- Starts a read of `ctrl`'s header for `st`, unless one is out or there is nothing to learn.
-- True while one is out.
local function request(ctrl, st)
  if st.reading then return true end
  local stepping = H.step ~= nil and H.step.id == st.id
  if st.verdict ~= nil and st.k and not st.stale then return false end
  if st.verdict == false then return false end
  if st.reads >= READS_MAX and not stepping then
    sayOnce("cap", st.key, "[avenger] '%s' (%dx%d): %d reads of the header brought no zoom — not read "
      .. "again until the window changes size or the keyboard comes in again", titleOf(ctrl),
      ctrl.client.w, ctrl.client.h, st.reads)
    return false
  end
  local band = bandOf(ctrl)
  if not band then return false end
  local fx, fy = G.frameOf(ctrl)
  local cx, cy = ctrl.client.x + fx, ctrl.client.y + fy
  local title = titleOf(ctrl)
  local token = {}
  st.reading = token
  local ok, err = pcall(host.ocr.recognize, band, { lang = H.lang(), key = "vps-avenger header" }, function(r)
    answered(ctrl, st, token, band, cx, cy, title, r)
  end)
  if not ok then
    st.reading = nil
    sayOnce("refused", st.key, "[avenger] '%s': the header band could not be read: %s", title, tostring(err))
    return false
  end
  return true
end

-- A stay that OUR OWN popup ended. The runtime ends a stay whenever it sees anything but the
-- keyboard inside the panel — another window in front, the window's size or title changed — and
-- asks `identify` again for the next. Two things this module does itself end one while the
-- overlay stays in front:
--   * one of Avenger's popups — MENU, the zoom list, the redo list — becoming the window in front,
--     which the overlay holds its place over (docs/api/overlay.md, O.menuTests: "a menu that is a
--     window in front"); whether a JUCE popup does that on a Mac is not measured;
--   * a zoom step, after which the DAW resizes its window to Avenger's new size.
-- In both, what the panel shows has not changed: our popup cannot put another plug-in there.
-- Answering nil until a new read of the header lands would take the overlay out of the front
-- after every menu, and in the middle of a zoom step, whose Zoom control would then never say
-- what the zoom became. So while the overlay has stayed in front since this module CLICKED one of
-- Avenger's openers in a window whose header was Avenger's, a new stay there is Avenger's at once
-- — and its header is read all the same: for the zoom, for the log, and to check it.
--
-- As narrowly as that: at the size the header was last read at, unless the popup was the zoom
-- list or a zoom step of ours is still to show, since only those resize the window; and once
-- only, the first read that finds the header ending it, unless a zoom step still waits. A window
-- switched to another plug-in in place (Logic's Link mode, unmeasured) retitles or resizes it and
-- ends the stay too, and a carry that outlived its popup would take that plug-in for Avenger. What
-- a carried stay's reads find decides the rest: CARRIED_MISSES reads in a row of text without the
-- header overturn it — nothing is placed there from then on, and the next stay asks the header as
-- always. Anything that takes the overlay out of the front (the keyboard on the DAW's own
-- controls, another window, a file dialog it does not hold its place over) ends the carry
-- (H.left).
local function carried(ctrl, st)
  if not (popupIn == ctrl.id and lastAvenger == ctrl.id) then return false end
  local resizing = popupZoom or (H.step ~= nil and H.step.id == ctrl.id)
  if not resizing and sizeOf(ctrl) ~= lastSize then return false end
  st.verdict, st.carried, st.misses = true, true, 0
  local p = H.step
  sayOnce("carried", st.key, "[avenger] '%s' (%dx%d): a new stay after %s — still VPS Avenger; its "
    .. "header is read again", titleOf(ctrl), ctrl.client.w, ctrl.client.h,
    (p and p.id == ctrl.id) and string.format("the zoom step from %d %%", p.from) or "a popup of ours")
  request(ctrl, st)
  return true
end

-- ── What the overlay asks ────────────────────────────────────────────────────

-- The binding's `identify`: true, false, or nil while the header's read is out.
function H.identify(ctrl)
  if not (ctrl and ctrl.id and ctrl.client) then return false end
  local st = stateFor(ctrl)
  if st.verdict ~= nil then return st.verdict end
  if carried(ctrl, st) then return true end
  request(ctrl, st)
  return nil
end

-- The overlay's scale (O:scale): the factor for `origin`'s window, size and stay, or nil —
-- "cannot tell now" — while its header is being read, after a zoom step until the zoom field
-- shows the new zoom, and when the header gave none. Called at every resolution; a lookup, and a
-- read started when there is none out and something to learn.
function H.factor(origin)
  if not (origin and origin.id and origin.client) then return nil end
  local st = stateFor(origin)
  if st.k and not st.stale then return st.k end
  request(origin, st)
  return nil
end

-- The zoom for `origin`: the zoom and "known"; nil and "reading" while a read is out; nil and
-- "unread" when the header gave none (or a factor without a zoom) and is not read again.
function H.zoom(origin)
  if not (origin and origin.id and origin.client) then return nil, "unread" end
  local st = stateFor(origin)
  if st.zoom and not st.stale then return st.zoom, "known" end
  if request(origin, st) then return nil, "reading" end
  return nil, "unread"
end

-- A zoom step of ours, from `from`, has been chosen in `origin`'s zoom list: its zoom and factor
-- are not known until the zoom field is read again showing another zoom (or has shown the same
-- one STEP_READS_MAX times: the step did not take).
function H.stepped(origin, from)
  popupZoom = false
  if not (origin and origin.id and origin.client) then return end
  H.step = { id = origin.id, from = from, reads = 0 }
  local st = stateFor(origin)
  st.k, st.zoom, st.stale = nil, nil, true
  host.log.info(string.format("[avenger] zoom step from %d %% chosen: the zoom field is read again "
    .. "until it shows another zoom, up to %d reads; nothing is placed until then", from, STEP_READS_MAX))
end

-- Whether a zoom step of ours in `origin`'s window is still waiting for the zoom field.
function H.stepping(origin)
  return origin ~= nil and H.step ~= nil and H.step.id == origin.id
end

-- This module has just clicked the opener of one of Avenger's popups over `origin` — the zoom
-- list when `zoom` (see `carried`). Only once it has: an opener refused as covered opened nothing.
function H.popup(origin, zoom)
  if origin and origin.id then
    popupIn = origin.id
    popupZoom = zoom == true
  end
end

-- The zoom list's choice is over without a step: nothing will resize the window for it.
function H.zoomOver()
  popupZoom = false
end

-- The overlay left the front: a step still waiting is over, and the next stay asks the header.
-- A size of a window held as Avenger's whose reads ran out without a factor is read again when
-- the keyboard comes back — on Windows, where a control has no stay, nothing else would ever ask
-- it again.
function H.left()
  H.step = nil
  popupIn, popupZoom = nil, false
  for _, st in pairs(states) do
    if st.verdict == true and not st.k and not st.reading then st.reads = 0 end
  end
end

return H
