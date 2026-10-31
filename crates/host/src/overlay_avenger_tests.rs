//! VPS Avenger's overlay module (`modules/vps-avenger`), loaded for real against the scripted host
//! of `overlay_menu_tests.rs` with the window prelude's real matcher and daw-hosts, as
//! `overlay_host_panel_tests.rs` loads sforzando and Kontakt.
//!
//! Written for the maintainer's decisions of 2026-09-28, the first stage of VPS Avenger: the
//! header — the preset's name read and said on arrival, ◀ and ▶ with the new name said once it is
//! on screen, MENU's four items chosen in Avenger's own menu, UNDO and its redo list, and a zoom
//! stepper over Avenger's zoom list — at every zoom from 50 to 200 %, in every DAW daw-hosts
//! knows. Avenger publishes no accessibility element anywhere, so everything is a click and a text
//! read: which plug-in the panel shows and at which zoom are ONE read of its header (MENU and UNDO
//! where the table puts them, the zoom field's caption as text), off the event loop; the factor
//! every coordinate is scaled by is that zoom over 50, times a display factor the header's own
//! size says (1 on a Mac, 1.5 on a Windows display at 150 %); and Avenger's corner is the DAW's
//! panel's plus one named constant, (-2, +2), on a panel only.
//!
//! The scenarios answer the module's reads with words placed where Avenger would draw them — at
//! the table's points, from the corner the module is meant to take, at the factor of the zoom —
//! so what they check is the module's arithmetic and its decisions, not Avenger: nothing here has
//! met a real one. Where a number is written out, it is worked out in the comment beside it.
//!
//! And for the review that followed: identity takes MENU and UNDO in capitals, only in boxes a
//! recogniser located, and a factor that fits them, and half an answer is read again rather than
//! taken for a "no"; on a DAW's panel the display factor is 1, on Windows an unread zoom is not
//! guessed, and a zoom step that resizes Avenger's child window keeps it Avenger's; a stay carried
//! over after our own popup is overturned by what is read; the warning box is given up only on two
//! readings without its buttons, starts on its own text every time it comes, and is not opened
//! where it would lie off the screen; a popup a choice left open is closed; a press while a step is
//! still being carried out sends nothing; and a name with nothing read before the click is never
//! said as new before the watch is over.
//!
//! And for the preset database, the same day's later decision: the module may orient itself on the
//! avenger_control project's database, installed as an optional data module
//! (`modules/vps-avenger-presets`) — and since 2026-10-26 says what it gives plainly, marking only
//! a name the database put where the screen's read is not. Its scenarios load the data module's
//! real reader against a small fixture catalog (`T.FIXTURE`), never the real data: without the
//! module the ring and the steps are stage 1's; a database that cannot be read changes nothing but
//! Preset info's answer; a name read is placed in the catalog the tool's way; ◀ and ▶ say what was
//! read and add the database's neighbour, marked, when the read failed or disagrees; and the
//! position is forgotten when the preset may have changed.
//!
//! And for the first Mac test of the page (2026-10-14): ◀ and ▶ are unavailable where the name read
//! is Init Preset, which Avenger steps no preset from — said as Tab reaches them, and a press
//! clicks nothing and says why — until a read of a preset, and claim nothing where the preset may
//! have changed with nothing read since (Undo, a button of the module's own, among them).
//!
//! And for the first Windows session (2026-10-30), where Avenger's editor was a KalliGuiWin child
//! rather than a JUCE window: on Avenger's own window no read is a "no" — `identify` raises while
//! it cannot tell, which the runtime does not count, every undecided read asks for the next at
//! once, and the module reads at most eight times a visit — while a Mac's panel keeps its rule;
//! and the factor the window's width gives is logged there, and used for nothing.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mlua::{Function, Lua, Table, VmState};

use crate::backend::frame::Area;
use crate::backend::CapturedImage;
use crate::cells::{of_capture, to_hex, CellSpec, Predicate};

use super::overlay_host_panel_tests::{HP, SPECIFICITY_ARBITER, WINDOW_PRELUDE};
use super::overlay_menu_tests::{finish, harness, RUNTIME};

/// The helpers, added on top of the host-panel ones.
const AV: &str = r##"
local S = T.S
-- Every click, in order; S.onClick(n), when set, runs at the n-th: Avenger answering it at once.
-- S.browserClick(x, y, button), when set, runs first at every click: the expansion browser's model
-- (avenger_browser_tests.rs) answering it.
S.clicks = {}
S.onClick = nil
S.browserClick = nil
-- Every turn of the wheel, in order: { x, y, notches }; S.onScroll(n, notches, x, y), when set, runs
-- at the n-th.
S.scrolls = {}
S.onScroll = nil
S.owns = true
S.ownsAsked = {}
S.marked = {}

-- The ring's controls of the oscillator strip, the Drums tab and the macros, after Zoom
-- (src/ring.luau): the Drums tab's group shown only while the strip reads the Drums tab, and DRUM
-- SQ's in it only while the central tab reads DRUM SQ; each knob followed by its reset, the tool's.
T.RING_LABELS = "Oscillator|Mute|Solo|Add oscillator|Drum pattern|Drum mode|Drum lock|Drum slot|Slot mute|"
  .. "Slot solo|Central tab|Pattern chain|Auto follow|Pattern|Pattern B|Pattern C|Pattern D|Level|Reset level|"
  .. "Pan|Reset pan|Transpose|Reset transpose|Fine tune|Reset fine tune|Random pitch|Reset random pitch|"
  .. "Macro tab|Add macro tab|Delete last macro tab|Macro 1|Reset macro 1|Macro 2|Reset macro 2|Macro 3|"
  .. "Reset macro 3|Macro button 1|Macro button 2"
T.RING_CONTROLS = 38

-- The arbiter is the shared one with the rule a module with a dialog layer needs
-- (`overlay_host_panel_tests::SPECIFICITY_ARBITER`), its claims Avenger's.
T.claimModule = "com.platform.vps-avenger"

rawset(T.host.window, "ownsPoint", function(id, x, y)
  S.ownsAsked[#S.ownsAsked + 1] = { id, x, y }
  if type(S.owns) == "function" then return S.owns(id, x, y) end
  return S.owns
end)
-- Every process's windows, for newWindow: the harness's answers for REAPER's pid only.
rawset(T.host.window, "windowsOf", function()
  S.windowLists += 1
  return S.windows
end)
-- The recognition languages: English and German, or German alone with S.english = false (a German
-- Windows without English OCR); every read records the language it asked for.
rawset(T.host.ocr, "resolveLanguage", function(l)
  if l == nil then return "de-DE" end
  if l == "en" and S.english ~= false then return "en-US" end
  return nil
end)
-- The page key's first text read (read/open.luau, Open.read with `done`: key "vps-avenger page"),
-- kept apart in S.pageReads and answered on the next turn, after the screen's answers of that turn,
-- as the host answers a read with a callback — from the snapshot it names, as a waited read is
-- (T.readWhat) — unless S.holdPageReads keeps it out until a scenario lets it go.
S.pageReads = {}
do
  local read = T.submit
  T.submit = function(what, opts, cb)
    local n = #S.reads
    read(what, opts, cb)
    -- Only a read that stayed in S.reads: the runtime's focus reads go to S.focusReads.
    if #S.reads > n then
      S.reads[#S.reads].lang = opts and opts.lang
      if opts and opts.key == "vps-avenger page" then
        local r = table.remove(S.reads)
        r.what = what
        S.pageReads[#S.pageReads + 1] = r
      end
    end
  end
end
function T.answerPageReads()
  local due = S.pageReads
  if #due == 0 or S.holdPageReads then return end
  S.pageReads = {}
  S.epoch += 1
  for _, r in ipairs(due) do
    r.answered = true
    T.call("answer", r.cb, T.readWhat(r.what, r.snapshot))
  end
end
-- A shot with its marks: written at once where a handler waits for it, and with a callback
-- answered on the next turn (T.runDue) as the page's pictures are (S.screenOut).
rawset(T.host.screen, "saveMarked", function(path, opts, cb)
  if cb == nil then T.screenWait("host.screen.saveMarked") end
  assert(opts.snapshot == nil or not opts.snapshot.released, "a released snapshot was saved")
  local marks = {}
  for _, m in ipairs(opts.marks or {}) do
    assert(m.x == math.floor(m.x) and m.y == math.floor(m.y), "a mark in whole pixels")
    marks[#marks + 1] = { m.x, m.y }
  end
  S.marked[#S.marked + 1] = { path = path, region = opts.region, marks = marks, snapshot = opts.snapshot }
  if cb then
    S.screenOut[#S.screenOut + 1] = { cb = cb, value = true }
    return
  end
  return true
end)

-- ── The page's picture (read/open.luau) ──────────────────────────────────────
-- What the page's reads see: S.picture, its shapes painted over its `bg` in order — { kind =
-- "rect", x1, y1, x2, y2, c }, { kind = "ray", cx, cy, angle, r1, r2, width, c } (a pointer from
-- r1 to r2 out along `angle` degrees, 0 right and 90 down), { kind = "disc", cx, cy, r, c } — and
-- reduced by the host's own code (T.paintCells, T.paintProfile, T.paintPixels). Asked as the
-- page's reads ask, of a snapshot: a grid of cells and a column profile waited for in a handler,
-- answered at once (and where the host could not wait, raising its message: T.screenWait), or
-- with a callback, answered on the next turn (T.runDue); the points of a snapshot at once. Each
-- checked as the host checks it, and a region outside the snapshot answered as the host answers
-- it. A request with a key is out (host.screen.pending, S.out) until its callback runs, and
-- T.dropScreen drops the requests out as disabling the module does.
S.picture = { bg = { 24, 24, 24 }, shapes = {} }
S.screenOut = {}   -- { cb, value, why }: answers to deliver on the next turn
S.cellsAsked, S.cellsTotal, S.profilesAsked = 0, 0, 0
S.pointLists = {}  -- the length of every list of points read off a snapshot
S.words = {}       -- what the page's text read reads, by the region's name

local function heldSnapshot(s, call)
  assert(type(s) == "table" and s.region and not s.released, call .. ": a snapshot the module holds")
  return s
end

local function whole(r, call)
  assert(type(r) == "table" and #r == 4, call .. ": corners { x1, y1, x2, y2 }")
  for k = 1, 4 do assert(r[k] == math.floor(r[k]), call .. ": corner " .. k .. " is not a whole number: " .. tostring(r[k])) end
  assert(r[3] > r[1] and r[4] > r[2], call .. ": corners empty or turned around")
end

local function within(r, s)
  local g = s.region
  return r[1] >= g[1] and r[2] >= g[2] and r[3] <= g[3] and r[4] <= g[4]
end

rawset(T.host.screen, "predicate", function(src) return T.predicateCanonical(src) end)

rawset(T.host.screen, "cells", function(opts, cb)
  if cb == nil then T.screenWait("host.screen.cells") end
  for k in pairs(opts) do
    assert(k == "region" or k == "cols" or k == "rows" or k == "predicate" or k == "snapshot" or k == "key",
      "host.screen.cells: opts has a key '" .. tostring(k) .. "'")
  end
  whole(opts.region, "host.screen.cells")
  local snap = heldSnapshot(opts.snapshot, "host.screen.cells")
  S.cellsAsked += 1
  S.cellsTotal += opts.cols * opts.rows
  local r = opts.region
  local answer = { cb = cb, key = opts.key }
  if within(r, snap) then
    answer.value = { cells = T.paintCells(snap.picture or S.picture, r, opts.cols, opts.rows, opts.predicate),
      x = r[1], y = r[2], w = r[3] - r[1], h = r[4] - r[2] }
  else
    answer.why = T.E("outside", "the region is not inside the snapshot")
  end
  if cb == nil then return answer.value, answer.why end
  if opts.key then S.out[opts.key] = (S.out[opts.key] or 0) + 1 end
  S.screenOut[#S.screenOut + 1] = answer
end)

S.screenProfiles = 0 -- profiles of the screen, not of a snapshot: the drawn menu test's
rawset(T.host.screen, "profile", function(opts, cb)
  if cb == nil then T.screenWait("host.screen.profile") end
  for k in pairs(opts) do
    assert(k == "region" or k == "axes" or k == "snapshot" or k == "key" or k == "dark",
      "host.screen.profile: opts has a key '" .. tostring(k) .. "'")
  end
  assert(opts.axes == "columns" or opts.axes == "both", "the scripted host profiles columns, or both axes")
  whole(opts.region, "host.screen.profile")
  local r = opts.region
  local answer = { cb = cb, key = opts.key }
  if opts.snapshot == nil then
    -- The screen as it is now: S.picture.
    S.screenProfiles += 1
    answer.value = T.paintProfile(S.picture, r, opts.axes == "both")
    if cb == nil then return answer.value end
    if opts.key then S.out[opts.key] = (S.out[opts.key] or 0) + 1 end
    S.screenOut[#S.screenOut + 1] = answer
    return
  end
  local snap = heldSnapshot(opts.snapshot, "host.screen.profile")
  S.profilesAsked += 1
  if within(r, snap) then
    answer.value = T.paintProfile(snap.picture or S.picture, r, opts.axes == "both")
  else
    answer.why = T.E("outside", "the region does not overlap the snapshot")
  end
  if cb == nil then return answer.value, answer.why end
  if opts.key then S.out[opts.key] = (S.out[opts.key] or 0) + 1 end
  S.screenOut[#S.screenOut + 1] = answer
end)

do
  local pixels = rawget(T.host.screen, "pixels")
  rawset(T.host.screen, "pixels", function(points, opts, cb)
    if not (type(opts) == "table" and opts.snapshot ~= nil) then return pixels(points, opts, cb) end
    assert(cb == nil, "the scripted host reads a snapshot's points without a callback")
    local snap = heldSnapshot(opts.snapshot, "host.screen.pixels")
    assert(#points <= 4096, "host.screen.pixels: points has " .. #points .. " entries; the limit is 4096")
    for i, p in ipairs(points) do
      assert(type(p) == "table" and #p == 2 and p[1] == math.floor(p[1]) and p[2] == math.floor(p[2]),
        "host.screen.pixels: points[" .. i .. "] is not { x, y } in whole numbers")
      local g = snap.region
      if p[1] < g[1] or p[2] < g[2] or p[1] >= g[3] or p[2] >= g[4] then
        return nil, T.E("outside", "the point is not inside the snapshot")
      end
    end
    S.pointLists[#S.pointLists + 1] = #points
    return T.paintPixels(snap.picture or S.picture, points)
  end)
end

-- The answers of the page's grids, profiles and pictures, on the next turn, the epoch turned over
-- first, each as an event of the module — unless S.holdScreenOut, which keeps them out (pending)
-- until a scenario lets them go. Then the page key's first text read (T.answerPageReads).
do
  local runDue = T.runDue
  T.runDue = function()
    runDue()
    local out = S.screenOut
    if #out > 0 and not S.holdScreenOut then
      S.screenOut = {}
      if S.turnsAtAnswers == false then S.turnsAtAnswers = T.interrupts() end
      S.epoch += 1
      for _, a in ipairs(out) do
        if a.key then S.out[a.key] -= 1 end
        T.call("answer", a.cb, a.value, a.why, {})
      end
    end
    T.answerPageReads()
  end
end

-- The module disabled and enabled again while requests are out: they are dropped, their callbacks
-- never run, and none of their keys is pending any more — the page key's first text read too.
function T.dropScreen()
  for _, a in ipairs(S.screenOut) do
    if a.key then S.out[a.key] -= 1 end
  end
  S.screenOut = {}
  S.pageReads = {}
end

-- A snapshot of corners against the module's budget, as the host keeps it (docs/api/screen.md):
-- w * h * 4 bytes, five times that on a Mac, on top of S.snapHeld — what the module's other
-- snapshots hold, nothing unless a scenario says — within 128 MiB, or the call answers nil and the
-- host's error value (`too_large`). S.snapFails is the message of the error value (`failed`) a
-- capture that fails answers. (The primary screen is
-- S.screenSize, 2560 x 1440 unless a scenario says: overlay_host_panel_tests.rs, T.modules.)
S.snapHeld = 0
-- The change waits out (host.screen.snapshot with `change`): each answered on the turn after the
-- picture has changed from its baseline — another S.picture than `from`'s, which T.paintAvenger or
-- T.repaint makes — with `changed = true`, or at its timeout with what is there, `changed = false`.
-- Each keeps its options (`minPixels`, `settle`, `tolerance`, `watch`) for a scenario to look at;
-- with S.honourMinPixels a changed picture counts only where at least `minPixels` of the watched
-- pixels (the region without `watch`) differ from the baseline by more than `tolerance`
-- (T.paintDiff), as the host counts them; `settle` is not played: a change stands still at once.
-- S.onSnapshotWait, when set, runs inside a picture a handler waits for, before it is answered:
-- the user going elsewhere meanwhile.
S.changeWaits = {}
S.changeWaitsAsked = 0
S.onSnapshotWait = nil
S.sent = {} -- every host.input.send, in order
-- A press held, its moves and its release (src/amount.luau): every one in S.input, in order —
-- { "down", x, y, button = b }, { "move", x, y }, { "up", x, y, button = b } — and the buttons the
-- module holds as the host keeps them (host.input.held), which a disable empties as the host lets
-- go of them (S.hostReleases, T.disable). S.onMove(n), when set, runs at the n-th move: Avenger
-- answering it; S.moveRaises, a message, raises at the next move.
S.input, S.heldButtons, S.hostReleases, S.moves = {}, {}, {}, 0
S.onMove, S.moveRaises = nil, nil
-- Every snapshot handle the scripted host has handed out that the module has not released yet
-- (T.held): a picture held past its action holds the module's budget.
S.live = {}
do
  local snap = T.snap
  T.snap = function(region)
    local s = snap(region)
    S.live[s] = true
    local release = s.release
    s.release = function(h)
      S.live[h] = nil
      return release(h)
    end
    return s
  end
end
function T.held()
  local n = 0
  for _ in pairs(S.live) do n += 1 end
  return n
end
do
  local snapshot = rawget(T.host.screen, "snapshot")
  rawset(T.host.screen, "snapshot", function(opts, cb)
    local r = opts and opts.region
    if cb == nil and type(r) == "table" and #r == 4 then
      local bytes = (r[3] - r[1]) * (r[4] - r[2]) * 4 * (T.host.os.current == "macos" and 5 or 1)
      if S.snapHeld + bytes > 128 * 1024 * 1024 then
        return nil, T.E("too_large", string.format("this module's snapshots would hold %.1f MiB; the limit is 128 MiB "
          .. "per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()",
          (S.snapHeld + bytes) / 1048576))
      end
      if S.snapFails then return nil, T.E("failed", S.snapFails) end
    end
    if cb ~= nil and opts.change ~= nil then
      local c = opts.change
      whole(r, "host.screen.snapshot")
      for k in pairs(c) do
        assert(k == "from" or k == "watch" or k == "timeout" or k == "tolerance" or k == "minPixels" or k == "settle",
          "host.screen.snapshot: opts.change has a key '" .. tostring(k) .. "'")
      end
      assert(c.timeout == nil or (c.timeout >= 1 and c.timeout <= 2000 and c.timeout == math.floor(c.timeout)),
        "opts.change.timeout must be a whole number from 1 to 2000, got " .. tostring(c.timeout))
      assert(c.from == nil or not c.from.released, "a released snapshot as a change wait's baseline")
      for _, w in ipairs(c.watch or {}) do
        whole(w, "a watched region")
        assert(w[1] < r[3] and w[3] > r[1] and w[2] < r[4] and w[4] > r[2], "a watch region does not overlap the region")
      end
      S.changeWaitsAsked += 1
      S.changeWaits[#S.changeWaits + 1] = { region = r, cb = cb, asked = S.now, timeout = c.timeout or 500,
        base = c.from and c.from.picture or S.picture, watch = c.watch, minPixels = c.minPixels or 4,
        settle = c.settle or 0, tolerance = c.tolerance or 16 }
      return
    end
    if cb == nil then
      local s, why = snapshot(opts, cb)
      if s then s.picture = S.picture end
      if s and S.onSnapshotWait then S.onSnapshotWait(opts) end
      return s, why
    end
    -- A plain picture with a callback: of the picture as it is when it is answered; with a key,
    -- out (host.screen.pending) until its callback runs, or a disable drops it (T.disable).
    local key = opts and opts.key
    if key then S.out[key] = (S.out[key] or 0) + 1 end
    snapshot(opts, function(s, why, info)
      if key then S.out[key] -= 1 end
      if s then s.picture = S.picture end
      cb(s, why, info)
    end)
    S.asyncs[#S.asyncs].key = key
  end)
  local runDue = T.runDue
  T.runDue = function()
    runDue()
    local due = {}
    local i = 1
    while i <= #S.changeWaits do
      local w = S.changeWaits[i]
      local changed = S.picture ~= w.base
      if changed and S.honourMinPixels then
        local n = 0
        for _, g in ipairs(w.watch or { w.region }) do
          local c = { math.max(g[1], w.region[1]), math.max(g[2], w.region[2]), math.min(g[3], w.region[3]),
            math.min(g[4], w.region[4]) }
          if c[3] > c[1] and c[4] > c[2] then n += T.paintDiff(w.base, S.picture, c, w.tolerance) end
        end
        changed = n >= w.minPixels
      end
      if changed or S.now >= w.asked + w.timeout then
        table.remove(S.changeWaits, i)
        due[#due + 1] = { w = w, changed = changed }
      else
        i += 1
      end
    end
    if #due == 0 then return end
    S.epoch += 1
    for _, d in ipairs(due) do
      local s = T.snap(d.w.region)
      s.picture = S.picture
      T.call("answer", d.w.cb, s, nil, { time = S.now, waited = S.now - d.w.asked, frames = 1,
        changed = d.changed, settled = d.changed, rebased = false })
    end
  end
end

-- Avenger redrawn as `spec` says (T.paintAvenger's), after a click: a new picture, which a change
-- wait sees on its next turn. The same spec again is a picture that changed in nothing a read
-- tells — a hover drawn and gone.
function T.repaint(spec)
  T.paintAvenger(S.painted.cx, S.painted.cy, S.painted.k, spec)
end

-- Avenger as a picture shows it, for the page's reads: shapes at the points of the module's own
-- tables (src/tables), from Avenger's corner (cx, cy) at factor k, over a dark background. What
-- is drawn is `spec`'s:
--   osc = n            n OSC tabs, each with its green dot
--   dots               { { x, w, colour }, … } at 50 %: the dots as listed, in place of the green ones
--   selected, index    the tab whose bottom strip is dark: "osc" (and which), "drums" or "trig"
--   strips             { [i] = shade, drums = shade, trig = shade }: those bottom strips in shades of
--                      their own, as a picture taken while Avenger redraws them
--   mute, solo         { [i] = true } for the OSC tabs lit red
--   drumsMute, drumsSolo, trigMute, trigSolo   lit red
--   pattern, lock, mode   the Drums tab: its pattern switch red, its lock green, and "sq" or
--                      "drum" by which caption is the brighter (the project's rule: DRUMKIT's
--                      brighter is SEQUENCE)
--   central            the central tab whose bottom strip is dark
--   sq = { chain, follow, on = { b = true }, selected }   DRUM SQ's boxes
--   sqDrawn = { on = { a = true }, selected }   DRUM SQ's patterns as Avenger draws them
--   macroShown         the macro tab whose bottom strip is dark
--   buttons = { on, on }, amounts = { a, a, a }   the macro buttons lit, the knobs' pointers
--   volume, transpose, fine   the selected oscillator's knobs' pointers
--   fav                FAV lit
--   lasting            { x1, y1, x2, y2 } at 50 %: a panel a click drew that outlasts its popup
--   slots = { selected, lighter, mute, solo, lock, loop }   the drumkit table: the selected slot's
--                      name cell (and `lighter`'s) lighter, the slots' buttons lit red ({ [i] = true })
--   popup              { x1, y1, x2, y2, pointer = { x, y }? } at 50 %: Avenger's popup over it all,
--                      painted inside it; `pointer`, the mouse pointer drawn over it with its tip at
--                      x, y, as a Mac's picture of the popup can show it after a click
--   submenu            { x1, y1, x2, y2 } at 50 %: a popup of its own, painted after `popup`
--   editor             { x1, y1, x2, y2, fill = colour?, outline = colour?, selection = colour? } at
--                      50 %: Avenger's text editor open over a name field, light by default, over
--                      everything but a popup
--   sampleTab          the sample editor's tab whose caption is the brightest, over the editor's
--                      grey tab row: "sample", "amp", "pitch" or "eq"
--   sampleOn           the shown tab's switch lit blue, the tool's blue (EQ's box on "eq")
--   fields             { key = shade }: a field's box (tables/centre.luau's valueRect, or the
--                      envelope's name with "envelope_name") in grey `shade`, so that another shade
--                      is a change the change wait sees there
--   steps              a shade: DRUM SQ's step area in it
--   sqDark             DRUM SQ's patterns' row painted black over `sq`: too dark to be DRUM SQ's
-- A pointer is a light ray out from the knob's middle, at the angle its value puts it: 135°
-- (down and left) for nothing, 270° more clockwise for everything.
function T.paintAvenger(cx, cy, k, spec)
  local function tables(name) return T.source("modules/vps-avenger/src/tables/" .. name .. ".luau")(T.host) end
  local ST, DR, CE, MA, EX = tables("strip"), tables("drums"), tables("centre"), tables("macros"), tables("expansion")
  local shapes = {}
  local function rect(x1, y1, x2, y2, c)
    shapes[#shapes + 1] = { kind = "rect", x1 = cx + k * x1, y1 = cy + k * y1, x2 = cx + k * x2, y2 = cy + k * y2, c = c }
  end
  local function box(p, w, h, c) rect(p[1] - w / 2, p[2] - h / 2, p[1] + w / 2, p[2] + h / 2, c) end
  -- Round a point half a pixel right of and below the knob's middle as placed: there its pixels lie
  -- evenly round it as the project measures them, from each pixel's corner (read/knob.luau).
  local function pointer(p, angle, r1, r2, width)
    shapes[#shapes + 1] = { kind = "ray", cx = cx + k * p[1] + 0.5, cy = cy + k * p[2] + 0.5, angle = angle, r1 = r1 * k,
      r2 = r2 * k, width = width * k, c = { 235, 235, 235 } }
  end
  local RED, LIT, DARK, GREY = { 210, 40, 40 }, { 190, 190, 190 }, { 15, 15, 15 }, { 90, 90, 90 }
  -- The strip: the tabs' band, the selected tab's dark bottom, the dots, Mute and Solo.
  local n = spec.osc
  rect(ST.PANEL_LEFT, ST.SELECTION_TOP, ST.TRIG_SELECTION[2], ST.SELECTION_BOTTOM, GREY)
  if spec.selected == "osc" then
    local t = ST.tabSpan(spec.index, n, 0, 0)
    rect(t[1], 36, t[3], 40, DARK)
  elseif spec.selected == "drums" then
    rect(ST.DRUMS_SELECTION[1], 36, ST.DRUMS_SELECTION[2], 40, DARK)
  elseif spec.selected == "trig" then
    rect(ST.TRIG_SELECTION[1], 36, ST.TRIG_SELECTION[2], 40, DARK)
  end
  for key, shade in pairs(spec.strips or {}) do
    local c = { shade, shade, shade }
    if key == "drums" then
      rect(ST.DRUMS_SELECTION[1], 36, ST.DRUMS_SELECTION[2], 40, c)
    elseif key == "trig" then
      rect(ST.TRIG_SELECTION[1], 36, ST.TRIG_SELECTION[2], 40, c)
    else
      local t = ST.tabSpan(key, n, 0, 0)
      rect(t[1], 36, t[3], 40, c)
    end
  end
  for i = 1, n or 0 do
    local t = ST.tabSpan(i, n, 0, 0)
    if not spec.dots then rect(t[1] + 2, 34.5, t[1] + 5, 37.5, { 100, 220, 180 }) end
    local x = ST.tabX(i, n)
    if spec.mute and spec.mute[i] then box({ x + ST.MUTE_X_OFFSET, ST.MUTE_Y }, 13, 8, RED) end
    if spec.solo and spec.solo[i] then box({ x + ST.SOLO_X_OFFSET, ST.MUTE_Y }, 13, 8, RED) end
  end
  -- `dots` in place of the tabs' green ones: { x, w, colour } at 50 % — a dot's left and its width,
  -- as tall from 34 — as Avenger's are drawn (read/strip.luau).
  for _, d in ipairs(spec.dots or {}) do rect(d[1], 34, d[1] + d[2], 34 + d[2], d[3]) end
  if spec.drumsMute then box(ST.DRUMS_MUTE, 13, 8, RED) end
  if spec.drumsSolo then box(ST.DRUMS_SOLO, 13, 8, RED) end
  if spec.trigMute then box(ST.TRIG_MUTE, 13, 8, RED) end
  if spec.trigSolo then box(ST.TRIG_SOLO, 13, 8, RED) end
  -- The Drums tab.
  if spec.pattern then box(DR.PATTERN, 10, 10, RED) end
  local bright, dim = { 200, 200, 200 }, { 110, 110, 110 }
  if spec.mode then
    rect(DR.MODE_DRUM_SELECTION[1], DR.MODE_SELECTION_TOP, DR.MODE_DRUM_SELECTION[2], DR.MODE_SELECTION_BOTTOM,
      spec.mode == "sq" and bright or dim)
    rect(DR.MODE_SQ_SELECTION[1], DR.MODE_SELECTION_TOP, DR.MODE_SQ_SELECTION[2], DR.MODE_SELECTION_BOTTOM,
      spec.mode == "sq" and dim or bright)
  end
  if spec.lock then box(DR.MODE_LOCK, 10, 10, { 60, 230, 60 }) end
  -- The central tabs and DRUM SQ.
  rect(CE.TABS.editor[1] - 18, 239, CE.TABS.step_sq[1] + 18, 249, GREY)
  if spec.central then
    local p = CE.TABS[spec.central]
    rect(p[1] - 18, 245, p[1] + 18, 249, DARK)
  end
  local sq = spec.sq
  if sq then
    box(CE.SQ_PATTERN_PLAYER, 14, 8, sq.chain and { 160, 160, 160 } or { 40, 40, 40 })
    box(CE.SQ_AUTO_FOLLOW, 11, 8, sq.follow and { 170, 170, 170 } or { 40, 40, 40 })
    for _, key in ipairs({ "b", "c", "d" }) do
      box(CE.SQ_PATTERNS[key], 30, 8, sq.on and sq.on[key] and RED or { 40, 40, 40 })
    end
    -- The selected pattern outlined, its inside red where it is on.
    if sq.selected then
      box(CE.SQ_PATTERNS[sq.selected], 30, 9, LIT)
      box(CE.SQ_PATTERNS[sq.selected], 28, 7, sq.on and sq.on[sq.selected] and RED or { 30, 30, 30 })
    end
  end
  -- DRUM SQ's patterns as Avenger draws them (`sqDrawn`, in place of `sq`'s): each box from 254 to
  -- 261.5 over the dark rows between them, its fill 128 when on and 56 when off, its letter's bars
  -- lighter (156 and 126), so that a box's brightest rows lead by about 7 when on and 17 when off —
  -- an off one about 0.4 of the lead of a selected one that is on, as the letters do on the
  -- pictures; and the selected one's top and bottom rows 172 and 132 — the second Mac test's
  -- pictures' (read/drumsq.luau). `also`: a second box outlined, as while Avenger redraws.
  local drawn = spec.sqDrawn
  if drawn then
    rect(330, 250, 490, 268, { 35, 35, 35 })
    for _, key in ipairs({ "a", "b", "c", "d" }) do
      local p = CE.SQ_PATTERNS[key]
      local on = drawn.on and drawn.on[key]
      local fill = on and 128 or 56
      rect(p[1] - 15, 254, p[1] + 15, 261.5, { fill, fill, fill })
      local letter = on and 156 or 126
      rect(p[1] - 3, 255.5, p[1] + 3, 256.5, { letter, letter, letter })
      rect(p[1] - 3, 258.5, p[1] + 3, 259.5, { letter, letter, letter })
      if drawn.selected == key or drawn.also == key then
        local edge = on and 172 or 132
        rect(p[1] - 15, 254, p[1] + 15, 255, { edge, edge, edge })
        rect(p[1] - 15, 260.5, p[1] + 15, 261.5, { edge, edge, edge })
      end
    end
  end
  if spec.sqDark then rect(330, 250, 490, 268, { 5, 5, 5 }) end
  -- The macros: the tabs' band and the one shown, the buttons, the knobs' pointers.
  rect(MA.TAB_SPANS[1][1], 429, MA.TAB_SPANS[4][3], 443, GREY)
  if spec.macroShown then
    local t = MA.TAB_SPANS[spec.macroShown]
    rect(t[1], 439, t[3], 443, DARK)
  end
  for i, on in ipairs(spec.buttons or {}) do
    if on then
      local b = MA.BUTTON_BOUNDS[i]
      rect(b[1], b[2], b[3], b[4], { 130, 130, 130 })
    end
  end
  for i, a in ipairs(spec.amounts or {}) do
    pointer({ MA.FIELDS["macro" .. i][1], MA.AMOUNT_Y }, 135 + a / 100 * 270, 5, 13, 2.5)
  end
  -- The selected oscillator's knobs.
  -- 2 wide: a bare pointer of about 12 bright pixels at 50 % and 48 at 100 %, as Avenger's (the
  -- project's PAN_BASE_PIXELS, 12.5 at 50 %).
  if spec.volume then pointer({ 346, 68 }, 135 + spec.volume / 200 * 270, 1.5, 7.5, 2) end
  local function semitones(p, st) pointer(p, 135 + 5 + (st + 48) / 96 * 260, 1.5, 7.5, 2) end
  if spec.transpose then semitones({ 346, 114 }, spec.transpose) end
  if spec.fine then semitones({ 346, 160 }, spec.fine / 100 * 48) end
  if spec.fav then box(EX.FAVORITE_VIEW, 25, 10, { 220, 220, 220 }) end
  if spec.lasting then
    local q = spec.lasting
    rect(q[1], q[2], q[3], q[4], { 60, 60, 66 })
  end
  -- The sample editor: its tab row, the shown tab's dark bottom strip, its switch; and the fields.
  if spec.steps then
    local q = CE.SQ_STEPS
    rect(q[1], q[2], q[3], q[4], { spec.steps, spec.steps, spec.steps })
  end
  if spec.sampleTab then
    local q = CE.EDITOR_TABS_ROW
    rect(q[1], q[2], q[3], q[4], GREY)
    -- As Avenger draws them (the second Mac test's `Sample Editor/…` pictures): each tab's caption,
    -- over bottom strips that darken from Sample to EQ whatever is selected. A caption is letters:
    -- strokes a unit wide, 3 apart, over softer pixels round them — the selected one's strokes the
    -- brightest (205 against 145) and its soft pixels the darker (105 against 125), so that the
    -- mean of every pixel brighter than 95 leads by about 5 and only its brightest pixels tell it
    -- (read/sample.luau, topLumas).
    for key, shade in pairs({ sample = 62, amp = 57, pitch = 45, eq = 34 }) do
      local t = CE.EDITOR_TABS[key]
      rect(t[1] - 18, 261, t[1] + 18, 265, { shade, shade, shade })
      local selected = key == spec.sampleTab
      local soft, core = selected and 105 or 125, selected and 205 or 145
      rect(t[1] - 8, 256.5, t[1] + 8, 260.5, { soft, soft, soft })
      for x = t[1] - 7, t[1] + 6, 3 do rect(x, 256.5, x + 1, 260.5, { core, core, core }) end
    end
    if spec.sampleOn then
      box(spec.sampleTab == "eq" and CE.EQ_ENABLE_STATE or CE.ENABLE_STATE, 5, 5, { 120, 160, 210 })
    end
  end
  for key, shade in pairs(spec.fields or {}) do
    local q = key == "envelope_name" and CE.ENVELOPE_NAME or CE.valueRect(key)
    rect(q[1], q[2], q[3], q[4], { shade, shade, shade })
  end
  -- The drumkit table (`slots`): the selected slot's name cell lighter than the others', and each
  -- slot's Mute, Solo, Lock and Loop lit red; `lighter` lists more slots drawn as the selected one.
  local slots = spec.slots
  if slots then
    local function light(i)
      local y = DR.rowY(i)
      rect(DR.NAME_COLUMN[1], y - DR.SLOT_HALF, DR.NAME_COLUMN[2], y + DR.SLOT_HALF, { 90, 90, 90 })
    end
    if slots.selected then light(slots.selected) end
    for _, i in ipairs(slots.lighter or {}) do light(i) end
    for _, col in ipairs({ "mute", "solo", "lock", "loop" }) do
      for i, on in pairs(slots[col] or {}) do
        if on then box(DR.columnPoint(i, col), 13, 8, RED) end
      end
    end
  end
  -- Avenger's editor over a name field (`editor`): its outline half a point outside the field, its
  -- fill, and its selection inside.
  if spec.editor then
    local q = spec.editor
    if q.outline then rect(q[1] - 0.5, q[2] - 0.5, q[3] + 0.5, q[4] + 0.5, q.outline) end
    rect(q[1], q[2], q[3], q[4], q.fill or { 235, 235, 235 })
    if q.selection then rect(q[1] + 2, q[2] + 2, q[3] - 2, q[4] - 2, q.selection) end
  end
  -- Avenger's popup over it, painted inside its window: { x1, y1, x2, y2 } at 50 % (`popup`).
  local function popup(q)
    rect(q[1], q[2], q[3], q[4], { 70, 70, 76 })
    for row = q[2] + 4, q[4] - 6, 11.5 do rect(q[1] + 4, row, q[3] - 8, row + 3, { 210, 210, 210 }) end
  end
  if spec.popup then popup(spec.popup) end
  local tip = spec.popup and spec.popup.pointer
  if tip then rect(tip[1], tip[2], tip[1] + 5, tip[2] + 7, { 250, 250, 250 }) end
  -- A submenu of it, painted the same way, after it (`submenu`).
  if spec.submenu then popup(spec.submenu) end
  -- The expansion browser's model, where a scenario has one (avenger_browser_tests.rs): its shapes
  -- under the popups, and the words its reads read, kept with the picture.
  if T.browserShapes then T.browserShapes(shapes, cx, cy, k, spec) end
  S.picture = { bg = { 40, 40, 40 }, shapes = shapes }
  if T.browserReads then S.picture.browser = T.browserReads(cx, cy, k) end
  S.painted = { cx = cx, cy = cy, k = k, spec = spec }
end

-- What the page's text read reads: `words`, by the region's name (read/open.luau's names).
function T.words(words)
  S.words = words
  S.ocr = function(_, name) return S.words[name] or "" end
end

-- Whether picture `p` (S.picture's form) draws anything in screen rectangle `r` but its
-- background: a shape whose box meets it.
function T.drawnIn(p, r)
  for _, s in ipairs(p and p.shapes or {}) do
    local x1, y1, x2, y2
    if s.kind == "rect" then
      x1, y1, x2, y2 = s.x1, s.y1, s.x2, s.y2
    else
      local reach = (s.r2 or s.r or 0) + (s.width or 0)
      x1, y1, x2, y2 = s.cx - reach, s.cy - reach, s.cx + reach, s.cy + reach
    end
    if x1 < r[3] and x2 > r[1] and y1 < r[4] and y2 > r[2] then return true end
  end
  return false
end

-- A text read the module waits for, answered at once: a region's text is S.ocr(region, name, snap)
-- — a string, or a whole reading; `snap` the snapshot read, if any — and a list is answered (list,
-- byName), as the host answers one. An empty string is "blank" where the picture draws nothing in
-- the region — the host's crop finds no content (docs/api/ocr.md) — and "none" where it does.
-- Every region asked is noted in S.recognized, with its name.
-- One region's reading, as the host answers it: region `r`, named `name`, of snapshot `snap`
-- (nil: the screen). Noted in S.recognized.
function T.readOne(r, name, snap)
  S.recognized[#S.recognized + 1] = { r[1], r[2], r[3], r[4], name = name, snapshot = snap }
  -- Cut to the snapshot, as the host reads one: a region it holds nothing of is "failed".
  local g = snap and snap.region
  if g and (r[3] <= g[1] or r[4] <= g[2] or r[1] >= g[3] or r[2] >= g[4]) then
    return { text = "", words = {}, lines = {}, skipped = false, status = "failed",
      error = T.E("outside", "the region does not overlap the snapshot"), name = name }
  end
  local said = S.ocr and S.ocr(r, name, snap) or ""
  local reading
  if type(said) == "table" then
    reading = table.clone(said)
  else
    local words = said ~= "" and { { text = said, x = r[1], y = r[2], w = r[3] - r[1], h = r[4] - r[2] } } or {}
    local status = said ~= "" and "text" or (T.drawnIn(snap and snap.picture or S.picture, r) and "none" or "blank")
    reading = { text = said, words = words, lines = {}, skipped = status == "blank", status = status }
  end
  reading.name = name
  return reading
end

-- What a read of `what` (a region, an entry or a list of them) of snapshot `snap` reads: one
-- reading, or (list, byName) for a list, as the host answers one.
function T.readWhat(what, snap)
  if type(what[1]) == "table" then
    local list, byName = {}, {}
    for i, e in ipairs(what) do
      local reading = T.readOne(e.region or e, e.name, snap)
      list[i] = reading
      if e.name then byName[e.name] = reading end
    end
    return list, byName
  end
  return T.readOne(what.region or what, what.name, snap)
end

function T.listReads()
  T.answerWaits(function(what, opts)
    return T.readWhat(what, opts and opts.snapshot)
  end)
end

-- Avenger's size at `zoom` % on a display of factor `d`: 871.25 across and 561 down at 50 %.
function T.size(zoom, d)
  local k = zoom / 50 * (d or 1)
  return math.floor(871.25 * k + 0.5), math.floor(561 * k + 0.5)
end

-- REAPER's floating window on a Mac, sized to Avenger at `zoom`: the plug-in 22 below the content
-- origin (daw-hosts), so the panel is at (100, 82) and Avenger's corner at (98, 84).
function T.float(zoom)
  local w, h = T.size(zoom)
  return { id = 31, title = "VST3i: VPS Avenger - Track 1", class = "AXWindow/AXStandardWindow/",
    app = T.MAC_REAPER, bounds = { x = 100, y = 38, w = w, h = h + 50 },
    client = { x = 100, y = 60, w = w, h = h + 22 } }
end

-- Logic's plug-in window, titled by the channel strip: the panel 2 in and 88 down from the frame
-- (daw-hosts' fallback, with no header published here), at (2, 118); Avenger's corner at (0, 120).
function T.logic(zoom)
  local w, h = T.size(zoom)
  return { id = 41, title = "Inst 1", class = "AXWindow/AXDialog/", app = T.MAC_LOGIC,
    bounds = { x = 0, y = 30, w = w + 4, h = h + 117 }, client = { x = 0, y = 30, w = w + 4, h = h + 117 } }
end

-- REAPER's FX chain on Windows, with Avenger's own child window (its class as the first Windows
-- session's probe read it) at (248, 60) sized for `zoom` on a display of factor `d`, the keyboard
-- in it.
T.WFX = { id = 60, title = 'FX: Track 1 "VPS Avenger"', class = "#32770",
  app = { pid = 4242, exe = "reaper.exe" },
  bounds = { x = 0, y = 0, w = 3800, h = 2600 }, client = { x = 8, y = 30, w = 3784, h = 2562 } }
T.WLIST = { id = 61, class = "SysListView32", bounds = { x = 8, y = 60, w = 230, h = 600 },
  client = { x = 8, y = 60, w = 230, h = 600 } }
function T.kalliGui(zoom, d)
  local w, h = T.size(zoom, d)
  return { id = 70, class = "KalliGuiWin18360-000000000D72C850", bounds = { x = 248, y = 60, w = w, h = h },
    client = { x = 248, y = 60, w = w, h = h } }
end
function T.onWindows(zoom, d)
  local j = T.kalliGui(zoom, d)
  S.listed = { T.WFX }
  S.front, S.controls, S.chain = T.WFX, { T.WLIST, j }, { j, T.WFX }
  T.turn()
  return j
end

-- Avenger's header as a recogniser would hand it back: words centred where the table puts them,
-- from Avenger's corner (cx, cy) at factor k — the zoom caption (unless `zoom` is false), the
-- preset's name, ◀ ▶ as nothing a recogniser reads, MENU and UNDO; `shift` moves MENU and UNDO.
function T.header(cx, cy, k, zoom, shift)
  local dx, dy = shift and shift[1] or 0, shift and shift[2] or 0
  local function word(text, ax, ay, aw, ah, sx, sy)
    local w, h = aw * k, ah * k
    return { text = text, x = cx + ax * k - w / 2 + (sx or 0), y = cy + ay * k - h / 2 + (sy or 0), w = w, h = h }
  end
  local words = {}
  if zoom then words[#words + 1] = word(zoom .. "%", 21, 14, 18, 9) end
  words[#words + 1] = word("Init", 110, 14, 18, 9)
  words[#words + 1] = word("Preset", 132, 14, 26, 9)
  words[#words + 1] = word("MENU", 262, 14, 22, 9, dx, dy)
  words[#words + 1] = word("UNDO", 305, 14, 22, 9, dx, dy)
  local text = {}
  for _, w in ipairs(words) do text[#text + 1] = w.text end
  return { status = "text", text = table.concat(text, " "), words = words }
end

-- Answers read `r` with `reading`, as the host delivers one: the epoch turns over first.
function T.answer(r, reading)
  assert(r and not r.answered, "no read waiting to be answered")
  r.answered = true
  S.epoch += 1
  T.call("answer", r.cb, reading)
end

-- The last read asked for with `key`.
function T.lastRead(key)
  for i = #S.reads, 1, -1 do
    if S.reads[i].key == key then return S.reads[i] end
  end
  return nil
end

-- How many reads have been asked for with `key`.
function T.readsOf(key)
  local n = 0
  for _, r in ipairs(S.reads) do
    if r.key == key then n += 1 end
  end
  return n
end

-- Counts the onFocus callbacks the module's header registers in S.headerFocus — told apart from
-- the runtime's by the chunk the registration comes from — from here on: called before
-- T.avenger(), whose overlays bind and ask `identify` the first time.
function T.countHeaderFocus()
  local onFocus = rawget(T.host.window, "onFocus")
  S.headerFocus = 0
  rawset(T.host.window, "onFocus", function(cb)
    if string.find(debug.info(2, "s") or "", "vps-avenger/src/header.luau", 1, true) then
      S.headerFocus += 1
    end
    return onFocus(cb)
  end)
end

-- ── The page's host (src/page.luau) ──────────────────────────────────────────
-- The module's own host calls for its page, apart from the runtime's, so that the runtime's
-- settings stay as the menu tests count them: host.gui.webview as a page object that records what
-- it is asked and answers what a scenario scripts — as page_example_tests.rs does for the example
-- — the module's settings as the host keeps them, its sounds and the key checks. And two of the
-- runtime's own, since the page key and the trips are the runtime's (O:addPage): its
-- host.hotkey.register, which keeps the page key's callback (S.pageKeys), and host.window.focus,
-- noted in the page's order.
S.pages = {}        -- every page described, in order: { opts, shows, hides, posts, prepares, closed, page }
S.pageAnswers = {}  -- what the next shows answer, in turn: { ok, err }; then `not_ready` until loaded
S.pageLoaded = false
S.pageOrder = {}    -- "post <kind>", "show", "hide", "focus <id>", "sound <path>", in order
S.postRaises = nil  -- a message: the next page:post raises it, as the host's does for what it cannot write
S.settings, S.settingDefs, S.settingCbs = {}, {}, {}
S.settingSets = 0   -- every host.settings.set of the module's, counted
S.pageKeys, S.pageKeyIds = {}, {}  -- spec -> callback; id -> spec, while registered
S.badKeys = {}      -- spec -> the reasons host.keys.check gives for it; `ok` as the host's (keys.md)
S.registerRefuses = {} -- spec -> the error value host.hotkey.register answers with nil: a key the system keeps
S.registerNotHeld = {} -- spec -> the one it answers beside the id: a claim made that another program holds now
S.sounds = {}

-- The page's window as host.window reports it while the page is in front: one of this
-- application's own.
T.PAGE_WIN = { id = 4000, title = "Avenger Control", class = "AXWindow/AXFloatingWindow/", own = true,
  app = { pid = 9, exe = "automation-platform", name = "automation-platform", bundleId = "" },
  bounds = { x = 1500, y = 100, w = 520, h = 640 }, client = { x = 1500, y = 128, w = 520, h = 612 } }

function T.pageHost(AH)
  local function webview(opts)
    local st = { opts = opts, shows = {}, hides = 0, posts = {}, prepares = 0, closed = false, shown = false }
    S.pages[#S.pages + 1] = st
    local p = {}
    st.page = p
    function p.show(self, o)
      assert(self == p and not st.closed, "page:show on the page in use")
      st.shows[#st.shows + 1] = { focus = o and o.focus, avoid = o and o.avoid }
      S.pageOrder[#S.pageOrder + 1] = "show"
      local a = table.remove(S.pageAnswers, 1) or (S.pageLoaded and { true } or { false, T.E("not_ready", "not ready yet") })
      if a[1] or T.is(a[2], "refused") then
        st.shown = true
        p.window = T.PAGE_WIN.id
      end
      if a[1] and not (o and o.focus == false) then
        S.front, S.chain = T.PAGE_WIN, { T.PAGE_WIN }
        S.epoch += 1
      end
      return a[1], a[2]
    end
    function p.hide(self)
      assert(self == p, "page:hide is a method")
      st.hides += 1
      st.shown = false
      S.pageOrder[#S.pageOrder + 1] = "hide"
    end
    function p.post(self, v)
      assert(self == p, "page:post is a method")
      -- Written as the host writes it, by its own encoder, raising where it raises: a value JSON
      -- cannot hold (a list too sparse, a function) is refused, as the module's page meets it.
      if S.postRaises then
        local why = S.postRaises
        S.postRaises = nil
        error("page:post: " .. why, 0)
      end
      local written, why = pcall(T.jsonEncode, v)
      if not written then error((string.gsub(tostring(why), "host%.json%.encode", "page:post", 1)), 0) end
      if v.kind == "state" and S.turnsAtState == false then S.turnsAtState = T.interrupts() end
      st.posts[#st.posts + 1] = v
      S.pageOrder[#S.pageOrder + 1] = "post " .. tostring(v.kind)
    end
    function p.prepare(self)
      assert(self == p, "page:prepare is a method")
      st.prepares += 1
      if S.pageLoaded then return true end
      return false, T.E("not_ready", "not ready yet")
    end
    function p.isShown() return st.shown end
    function p.focus() return st.shown end
    function p.close(self)
      assert(self == p, "page:close is a method")
      st.closed, st.shown = true, false
      p.window = nil
    end
    return p
  end
  AH.gui = T.strict("host.gui", { webview = webview })
  AH.settings = T.strict("host.settings", {
    define = function(key, default, opts)
      assert(S.settingDefs[key] == nil, "defined once: " .. key)
      assert(type(opts) == "table" and type(opts.label) == "string", "a label for the dialog: " .. key)
      S.settingDefs[key] = { default = default, opts = opts }
      if type(S.settings[key]) ~= type(default) then S.settings[key] = default end
      return S.settings[key]
    end,
    get = function(key)
      assert(S.settingDefs[key] ~= nil, "never defined: " .. tostring(key))
      return S.settings[key]
    end,
    -- The module's own set: checked as the host checks it, and its callbacks run inside it.
    set = function(key, v)
      local d = S.settingDefs[key]
      assert(d ~= nil and type(v) == type(d.default), "set: " .. tostring(key))
      if d.opts.oneOf then assert(table.find(d.opts.oneOf, v) ~= nil, "not one of its choices") end
      if d.opts.min then assert(v >= d.opts.min and v <= d.opts.max, "out of its bounds") end
      S.settingSets += 1
      local old = S.settings[key]
      S.settings[key] = v
      for _, cb in ipairs(S.settingCbs[key] or {}) do cb(v, old) end
    end,
    onChange = function(key, cb)
      S.settingCbs[key] = S.settingCbs[key] or {}
      table.insert(S.settingCbs[key], cb)
    end,
  })
  local register, unregister = T.host.hotkey.register, T.host.hotkey.unregister
  rawset(T.host.hotkey, "register", function(spec, fn)
    assert(S.badKeys[spec] == nil, "registered a key check refuses: " .. spec)
    if S.registerRefuses[spec] then return nil, S.registerRefuses[spec] end
    local id = register(spec, fn)
    S.pageKeyIds[id] = spec
    S.pageKeys[spec] = fn
    return id, S.registerNotHeld[spec]
  end)
  rawset(T.host.hotkey, "unregister", function(id)
    local spec = S.pageKeyIds[id]
    if spec then S.pageKeys[spec] = nil end
    S.pageKeyIds[id] = nil
    return unregister(id)
  end)
  AH.sound = T.strict("host.sound", { play = function(path)
    S.sounds[#S.sounds + 1] = path
    S.pageOrder[#S.pageOrder + 1] = "sound " .. path
    return true
  end })
  -- `ok` false only for the reasons that mean the key cannot work here; a tap, `altgr` and
  -- `composes` leave it true (docs/api/keys.md, host.keys.check).
  local CANNOT = { parse = true, reserved = true, no_keycode = true, voiceover = true }
  -- host.keys.normalize as the host's, for the specs these tests write: the modifiers by their role,
  -- in the platform's order and words, and the key with its first letter capitalised; nil where a
  -- modifier is none of the spec's words (docs/api/keys.md, host.keys.normalize).
  local ROLE = { ctrl = "ctrl", cmd = "ctrl", alt = "alt", option = "alt", shift = "shift", win = "win", meta = "win" }
  local ORDER = T.host.os.pick({
    macos = { { "win", "Meta" }, { "alt", "Option" }, { "shift", "Shift" }, { "ctrl", "Cmd" } },
    windows = { { "ctrl", "Ctrl" }, { "alt", "Alt" }, { "shift", "Shift" }, { "win", "Win" } },
  })
  AH.keys = setmetatable({
    check = function(spec)
      local reasons, ok = S.badKeys[spec] or {}, true
      for _, why in ipairs(reasons) do
        if CANNOT[why] then ok = false end
      end
      return { ok = ok, resolved = spec, reasons = reasons }
    end,
    normalize = function(spec)
      local parts = {}
      for part in string.gmatch(tostring(spec), "[^+]+") do
        parts[#parts + 1] = string.lower((string.gsub(part, "^%s*(.-)%s*$", "%1")))
      end
      local key = table.remove(parts)
      if key == nil or key == "" then return nil end
      local held = {}
      for _, m in ipairs(parts) do
        if not ROLE[m] then return nil end
        held[ROLE[m]] = true
      end
      local out = {}
      for _, o in ipairs(ORDER) do
        if held[o[1]] then out[#out + 1] = o[2] end
      end
      out[#out + 1] = string.upper(string.sub(key, 1, 1)) .. string.sub(key, 2)
      return table.concat(out, "+")
    end,
  }, { __index = T.host.keys })
  local focus = T.host.window.focus
  rawset(T.host.window, "focus", function(id)
    S.pageOrder[#S.pageOrder + 1] = "focus " .. tostring(id)
    return focus(id)
  end)
end

-- The page key as the platform picks it, pressed: its callback runs as the hotkey's handler.
function T.pageKey(spec)
  spec = spec or T.host.os.pick({ macos = "Ctrl+Shift+Win+F1", windows = "Ctrl+Shift+Alt+F1" })
  local fn = S.pageKeys[spec]
  assert(fn ~= nil, "no page key " .. tostring(spec))
  S.epoch += 1
  local got = T.call("hotkey", fn)
  assert(got == "Ran", "the key's handler: " .. got)
end

-- The page key pressed on an identified Avenger — its reads of the picture (read/open.luau) are
-- waited for in the key's handler, and the page shown there — and the next turn.
function T.openPage(spec)
  T.pageKey(spec)
  T.tick()
end

-- The page in use (the last described), and what it was last posted of `kind`.
function T.page() return S.pages[#S.pages] end
function T.lastPost(kind)
  local posts = T.page().posts
  for i = #posts, 1, -1 do
    if kind == nil or posts[i].kind == kind then return posts[i] end
  end
  return nil
end
-- The page sends `m`; the page has loaded; the page's window goes ("user" alone, or "failed" with
-- the host's error value beside it); the settings dialog's OK for `key`, its callbacks run as the
-- module's handlers.
function T.pageMsg(m)
  local got = T.call("page message", T.page().opts.onMessage, m)
  assert(got == "Ran", "the page's message: " .. got)
end
function T.pageReady()
  S.pageLoaded = true
  local got = T.call("page ready", T.page().opts.onReady)
  assert(got == "Ran", "the page's onReady: " .. got)
end
function T.pageClosed(how)
  local got = T.call("page close", T.page().opts.onClose, how, how == "failed" and T.E("failed", "the page failed") or nil)
  assert(got == "Ran", "the page's onClose: " .. got)
end
function T.setting(key, v)
  local old = S.settings[key]
  S.settings[key] = v
  for _, cb in ipairs(S.settingCbs[key] or {}) do T.call("setting", cb, v, old) end
end
-- S.pageOrder from entry `from` on, as one string.
function T.pageSince(from)
  local out = {}
  for i = from + 1, #S.pageOrder do out[#out + 1] = S.pageOrder[i] end
  return table.concat(out, ",")
end

-- A trip from the page (actions.luau): the page, with the keyboard, sends `m`; the runtime asks
-- Avenger's window `W` forward, which lands (T.land) with its foreground event; and the act runs on
-- the module's next turn.
function T.tripFrom(m, W)
  S.focusAtOnce = false
  S.focusLands = function() S.front, S.chain = W, { W } end
  T.pageMsg(m)
  T.land()
  T.runDue()
end

-- The name's reads after a click, as the watch makes them, each answered `name` (nil: nothing
-- read), for `ms` of the scenario's time (default 3750: the watch looks once a tick here, so its
-- twenty looks take twenty ticks, past its bound).
function T.nameReads(name, ms)
  for _ = 1, math.floor((ms or 3750) / 150) do
    T.run(150)
    local r = T.lastRead("vps-avenger name")
    if r and not r.answered then
      T.answer(r, name and T.nameRead(name) or { status = "none", text = "", words = {} })
    end
  end
end

-- The same, with the reads as slow as on a Mac, where one takes longer than the watch's 100 ms
-- between looks: each read is answered a tick after the one it was asked in, so every answer is
-- handed to two looks, as the tester's log has it. `answer(i)` gives the i-th read's answer: a
-- name, or nil for nothing read.
function T.slowNameReads(answer, ms)
  local i = 0
  for _ = 1, math.floor((ms or 3750) / 150) do
    T.run(150)
    local r = T.lastRead("vps-avenger name")
    if r and not r.answered then
      if r.waited then
        i += 1
        local name = answer(i)
        T.answer(r, name and T.nameRead(name) or { status = "none", text = "", words = {} })
      else
        r.waited = true
      end
    end
  end
end

-- Loads daw-hosts and the module as the host loads them, and hands back its two overlays: the
-- header and the warning box. The module gets a host of its own whose `host.path` is its own
-- folder, as the host scopes a module's identity; the runtime's pictures stay in the runtime's
-- (the harness's `host.path`), and everything else is the scripted host. Then the first tick,
-- which runs each overlay's first evaluation, asked for as it bound (`host.timer.after(0)`).
function T.avenger()
  local made = T.collect()
  T.modules("modules/vps-avenger/")
  local function point(call, x, y)
    assert(type(x) == "number" and type(y) == "number" and x == math.floor(x) and y == math.floor(y),
      call .. ": a point in whole pixels, got " .. tostring(x) .. "," .. tostring(y))
  end
  local function button(opts)
    local b = opts and opts.button or "left"
    assert(b == "left" or b == "right" or b == "middle", "a button: " .. tostring(b))
    return b
  end
  rawset(T.host, "input", T.strict("host.input", {
    -- S.clickAnswers, when set: one entry per click, `true` for a click made or the error value a
    -- refused one is answered with, nothing clicked (host.input.click's `false, err`). A
    -- double-click (`clicks = 2`) as the host makes one on a thread of its own: its first click at
    -- the call — the entry it takes the call's answer, refused there with nothing clicked — and its
    -- second, and its callback, 100 ms later, as long as the host's gesture takes — the entry it
    -- takes the callback's: an error value sends no second click and is what the callback hears.
    -- Each click is an entry of S.clicks, marked `double` with its number for a double-click's. A
    -- callback is answered once, on a later tick, as the host answers it.
    click = function(x, y, opts, cb)
      local function later(fn, ms) S.after[#S.after + 1] = { at = S.now + (ms or 0), fn = fn } end
      local double = opts and opts.clicks == 2 or nil
      local function made(n)
        S.clicks[#S.clicks + 1] = { x, y, button = opts and opts.button, double = double and n }
        if S.browserClick then S.browserClick(x, y, opts and opts.button) end
        if S.onClick then S.onClick(#S.clicks) end
      end
      local answer = S.clickAnswers and table.remove(S.clickAnswers, 1)
      if answer ~= nil and answer ~= true then
        if cb then later(function() cb(false, answer) end) end
        return false, answer
      end
      made(1)
      if double then
        later(function()
          local second = S.clickAnswers and table.remove(S.clickAnswers, 1)
          if second ~= nil and second ~= true then
            if cb then cb(false, second) end
            return
          end
          made(2)
          if cb then cb(true) end
        end, 100)
      elseif cb then
        later(function() cb(true) end)
      end
    end,
    -- The wheel, as the host turns it (docs/api/input.md): at whole pixels, by `notches`, positive up.
    scroll = function(x, y, notches)
      point("host.input.scroll", x, y)
      assert(type(notches) == "number" and notches ~= 0, "host.input.scroll: notches is a number, not 0")
      S.scrolls[#S.scrolls + 1] = { x, y, notches }
      if S.onScroll then S.onScroll(#S.scrolls, notches, x, y) end
    end,
    send = function(combo) S.sent[#S.sent + 1] = combo end,
    mouseDown = function(x, y, opts)
      point("host.input.mouseDown", x, y)
      local b = button(opts)
      S.input[#S.input + 1] = { "down", x, y, button = b }
      if not table.find(S.heldButtons, b) then S.heldButtons[#S.heldButtons + 1] = b end
    end,
    move = function(x, y)
      point("host.input.move", x, y)
      if S.moveRaises then
        local why = S.moveRaises
        S.moveRaises = nil
        error(why, 0)
      end
      S.input[#S.input + 1] = { "move", x, y }
      S.moves += 1
      if S.onMove then S.onMove(S.moves) end
    end,
    mouseUp = function(x, y, opts)
      point("host.input.mouseUp", x, y)
      local b = button(opts)
      S.input[#S.input + 1] = { "up", x, y, button = b }
      local i = table.find(S.heldButtons, b)
      if i then table.remove(S.heldButtons, i) end
    end,
    held = function() return table.clone(S.heldButtons) end,
    -- A drag, as the host makes one (docs/api/input.md): recorded as { "drag", x1, y1, x2, y2,
    -- button = b }; S.onDrag(n), when set, runs at the n-th: Avenger answering it.
    drag = function(x1, y1, x2, y2, opts)
      point("host.input.drag", x1, y1)
      point("host.input.drag", x2, y2)
      S.input[#S.input + 1] = { "drag", x1, y1, x2, y2, button = button(opts) }
      S.drags = (S.drags or 0) + 1
      if S.onDrag then S.onDrag(S.drags) end
    end,
  }))
  local AH = setmetatable({}, { __index = T.host })
  AH.path = function(p) return "C:/modules/vps-avenger/" .. p end
  -- Its optional dependency: the preset database T.presets loaded, or nil — not installed.
  AH.tryRequire = function(id)
    if id == "com.platform.vps-avenger-presets" then return S.presets end
    return nil
  end
  T.pageHost(AH)
  T.listReads()
  -- Each file's value, by its path: a scenario reaches a file's table here (page.luau's lists).
  local included = {}
  S.included = included
  -- An included file's top level runs through `Function::call`, as the host runs it (T.hostCall).
  AH.include = function(rel)
    local key = "modules/vps-avenger/" .. rel
    if included[key] == nil then included[key] = { T.hostCall(T.source(key), AH) } end
    return included[key][1]
  end
  T.source("modules/vps-avenger/src/main.luau")(AH)
  assert(#made == 2, "two overlays: " .. #made)
  T.runDue()
  return made[1], made[2]
end

-- The module disabled, as the host disables it (lib.rs, after_toggle): its page out of sight, its
-- timers, its pictures and reads out dropped (the pictures it holds stay its own), its captures out
-- of the hook (S.disabled), its scope and flag forgotten, its claims let go — and every mouse button
-- it holds let go of by the host, where the pointer is, with the host's line (S.hostReleases).
function T.disable()
  T.page().shown = false
  for _, r in ipairs(S.asyncs) do
    if r.key then S.out[r.key] -= 1 end
  end
  S.after, S.asyncs, S.reads = {}, {}, {}
  T.dropScreen()
  S.changeWaits = {}
  S.disabled = true
  S.scopedTo = false
  S.menuOpen[#S.menuOpen + 1] = false
  for _, c in ipairs(T.claims) do
    c.matching = false
    if c.active then
      c.active = false
      T.hostCall(c.off)
    end
  end
  for _, b in ipairs(S.heldButtons) do
    S.hostReleases[#S.hostReleases + 1] = b
    S.logs[#S.logs + 1] = "[input] [com.platform.vps-avenger] released the " .. b
      .. " mouse button it had pressed with host.input.mouseDown: it was disabled"
  end
  S.heldButtons = {}
end
-- Enabled again: its captures back in every window, with no flag (docs/api/keys.md, scope).
function T.enable()
  S.disabled = false
  S.scopedTo = false
end
-- S.input's entries of `kind` ("down", "move", "up"), from entry `from` on; and how many times a
-- button was let go of, by the module and by the host.
function T.inputs(kind, from)
  local out = {}
  for i = (from or 0) + 1, #S.input do
    if S.input[i][1] == kind then out[#out + 1] = S.input[i] end
  end
  return out
end
function T.ups() return #T.inputs("up") + #S.hostReleases end
function T.xy(e) return e and (e[2] .. "," .. e[3]) or "none" end

-- A reading of `words` (each { text, x, y, w, h, approx? }), as host.ocr.recognize hands one back.
function T.reading(words, lines)
  local text = {}
  for _, w in ipairs(words) do text[#text + 1] = w.text end
  return { status = "text", text = table.concat(text, " "), words = words, lines = lines or {} }
end

-- MENU's entry `i` of `header`, chosen in a popup at (x, y) w x h that opens and then closes, as
-- one that takes the choice does: the press, the tick the popup is seen and the item clicked, and
-- the two ticks that see it gone.
function T.choose(header, i, x, y, w, h)
  T.pressAt(header, i)
  T.popup(300, x, y, w, h)
  T.tick(1)
  T.closePopups()
  T.tick(2)
end

-- The box's reading: `text` above YES and NO where Avenger at 80 % in the floating window has them.
function T.boxReading(text, yes, no)
  local function w(t, x, y, ww, hh) return { text = t, x = x, y = y, w = ww, h = hh } end
  return { status = "text", text = text .. "\n" .. (yes or "YES") .. " " .. (no or "NO"),
    lines = { w(text, 600, 490, 250, 16) },
    words = { w(yes or "YES", 703.6, 573, 28, 16), w(no or "NO", 867.2, 572, 20, 16) } }
end

-- A reading of the plug-in with no box in it.
function T.noBox()
  return { status = "text", text = "EDITOR ARP", lines = {},
    words = { { text = "EDITOR", x = 600, y = 420, w = 60, h = 16 }, { text = "ARP", x = 680, y = 420, w = 40, h = 16 } } }
end

-- The focused control `i` of overlay `o`, pressed with Return as the captured key delivers it.
function T.pressAt(o, i)
  o.focus = i
  o:_syncNativeKeys()
  assert(S.holding["Return"], "Return is not captured on control " .. i)
  S.epoch += 1
  T.call("key", S.captured["Return"])
end

-- Right (dir 1) or Left on control `i`, a stepper.
function T.stepAt(o, i, dir)
  o.focus = i
  o:_syncNativeKeys()
  local key = dir > 0 and "Right" or "Left"
  assert(S.holding[key], key .. " is not captured on control " .. i)
  S.epoch += 1
  T.call("key", S.captured[key])
end

function T.clickAt(i)
  local c = S.clicks[i or #S.clicks]
  return c and (c[1] .. "," .. c[2]) or "none"
end

function T.said(from)
  local out = {}
  for i = from or 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end

function T.lastSaid() return S.speech[#S.speech] and S.speech[#S.speech].text end

-- A popup window of the DAW's process appears: id `id`, at x, y, w x h.
function T.popup(id, x, y, w, h)
  S.windows[#S.windows + 1] = { id = id, layer = 101, class = "", x = x, y = y, w = w, h = h }
end
function T.closePopups() S.windows = { S.windows[1] } end

-- An assertion whose failure prints the whole log and everything said to the test's output: a Luau
-- error message is cut at about 500 bytes, which is a few log lines.
function T.ok(cond, what)
  if cond then return end
  print(T.dump())
  print("said: " .. T.said())
  error(tostring(what or "assertion failed"), 2)
end

-- ── The preset database ──────────────────────────────────────────────────────
-- A small catalog in the data module's format (modules/vps-avenger-presets/README.md), NOT the
-- real data: four expansions whose presets carry the cases the lookup has to get right — a name
-- in three of them ("BA Deep Sub"), one differing from another only in case ("BA Reese", "Ba
-- Reese"), the acute accent as an apostrophe ("LD Palm´s Lead 1"), a trailing space and a double
-- one, a capital outside ASCII ("PL Üsch"); oscillators and macros with Avenger's default names
-- ("" in the file), and a preset with two macro tabs. Alpha's list is in category blocks that are
-- not in the categories' alphabetical order: Arp, Bass, Pads, Leads. Delta is a numbered series,
-- whose names are all similar to each other.
T.FIXTURE = {
  ["data/index.json"] = [=[{"format":1,"exported":"2026-09-26 18:57","factoryStandard":["Alpha"],
"expansions":[
{"name":"Alpha","file":"data/expansions/alpha.json","presets":["AR Band Pass Slide","AR Deep Arp","BA Deep Sub","BA Reese","PD Warm Pad","LD Palm´s Lead 1"]},
{"name":"Beta","file":"data/expansions/beta.json","presets":["BA Deep Sub","SQ Feel It","PD Soft Pad ","FX  Riser"]},
{"name":"Gamma","file":"data/expansions/gamma.json","presets":["Ba Reese","BA Deep Sub","PL Üsch"]},
{"name":"Delta","file":"data/expansions/delta.json","presets":["LD Lead 1","LD Lead 2","LD Lead 3"]}
]}]=],
  ["data/expansions/alpha.json"] = [=[{"format":1,"name":"Alpha",
"categories":[{"name":"Arp","start":1,"count":2},{"name":"Bass","start":3,"count":2},{"name":"Leads","start":6,"count":1},{"name":"Pads","start":5,"count":1}],
"presets":[
{"name":"AR Band Pass Slide","osc":["Arp","","Lead"],"macros":[["Tone","","Drive","+12",""]]},
{"name":"AR Deep Arp","osc":["One Osc"],"macros":[["Cut","Res","Env","Hold","Gate"],["","Wobble","","",""]]},
{"name":"BA Deep Sub","osc":["Sub","Bass"],"macros":[["Tone","Chorus","Drive","+12","Crush"]]},
{"name":"BA Reese","osc":["Reese"],"macros":[["","","","",""]]},
{"name":"PD Warm Pad","osc":["Pad"],"macros":[["Air","","","",""]]},
{"name":"LD Palm´s Lead 1","osc":["Lead","Sub"],"macros":[["Glide","","","",""]]}
]}]=],
  ["data/expansions/beta.json"] = [=[{"format":1,"name":"Beta",
"categories":[{"name":"Bass","start":1,"count":1},{"name":"Effects","start":4,"count":1},{"name":"Pads","start":3,"count":1},{"name":"Sequences","start":2,"count":1}],
"presets":[
{"name":"BA Deep Sub","osc":["Deep"],"macros":[["Tone","","","",""]]},
{"name":"SQ Feel It","osc":["Seq"],"macros":[["Gate","","","",""]]},
{"name":"PD Soft Pad ","osc":["Soft"],"macros":[["Air","","","",""]]},
{"name":"FX  Riser","osc":["Noise"],"macros":[["Rise","","","",""]]}
]}]=],
  ["data/expansions/gamma.json"] = [=[{"format":1,"name":"Gamma",
"categories":[{"name":"Bass","start":1,"count":2},{"name":"Plucked","start":3,"count":1}],
"presets":[
{"name":"Ba Reese","osc":["Reese"],"macros":[["","","","",""]]},
{"name":"BA Deep Sub","osc":["Sub"],"macros":[["","","","",""]]},
{"name":"PL Üsch","osc":["Pluck"],"macros":[["","","","",""]]}
]}]=],
  ["data/expansions/delta.json"] = [=[{"format":1,"name":"Delta",
"categories":[{"name":"Leads","start":1,"count":3}],
"presets":[
{"name":"LD Lead 1","osc":["Lead"],"macros":[["","","","",""]]},
{"name":"LD Lead 2","osc":["Lead"],"macros":[["","","","",""]]},
{"name":"LD Lead 3","osc":["Lead"],"macros":[["","","","",""]]}
]}]=],
}

-- The preset database installed: its own reader (modules/vps-avenger-presets/src/main.luau), run
-- as the host runs a code module in a dependent's VM — with a host of its own, whose
-- host.resource.read reads `files` (a published path -> its text; default T.FIXTURE) and raises
-- as the host does for one that is not there, and the host's real JSON decoder. Every read is
-- recorded in S.presetReads. Call it before T.avenger, which hands it to the module.
function T.presets(files)
  files = files or T.FIXTURE
  S.presetReads = {}
  local PH = setmetatable({}, { __index = T.host })
  PH.resource = { read = function(rel)
    S.presetReads[#S.presetReads + 1] = rel
    local text = files[rel]
    if text == nil then error("The system cannot find the file specified. (os error 2)", 0) end
    return text
  end }
  PH.json = { decode = T.jsonDecode }
  S.presets = T.source("modules/vps-avenger-presets/src/main.luau")(PH)
  return S.presets
end

-- REAPER's floating window at 80 % with Avenger identified, the name reading `name` on the event
-- loop (nil: nothing), arrival said: the header overlay, the box overlay.
function T.avengerAt80(name)
  local W = T.float(80)
  S.listed = { W }
  T.at(W)
  local header, box = T.avenger()
  T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
  T.event()
  T.nameIs(name)
  T.run(400)
  return header, box, W
end

-- What the preset's name reads, on the event loop (the region the module reads it in at 80 %).
function T.nameIs(name)
  S.ocr = function(g) return T.region(g) == "152,95,440,119" and (name or "") or "" end
end

-- A read of the name off the event loop, answered.
function T.nameRead(text) return { status = "text", text = text, words = {} } end

-- ◀ or ▶ (control `i`) with the name reading `before` on the event loop, then the reads after the
-- click answered with `after`: a name, or nil for none at all. Runs the watch to its end, answering
-- every read it makes, and gives back what was said.
function T.step(header, i, before, after)
  T.nameIs(before)
  local said = #S.speech
  T.pressAt(header, i)
  for _ = 1, 25 do
    T.run(150)
    local r = T.lastRead("vps-avenger name")
    if r and not r.answered then
      T.answer(r, after and T.nameRead(after) or { status = "none", text = "", words = {} })
    end
  end
  T.ok(#S.speech == said + 1, "one thing said per step: " .. T.said(said + 1))
  return T.lastSaid()
end

-- Whether `s` begins with `prefix`.
function T.starts(s, prefix) return string.sub(s or "", 1, #prefix) == prefix end

-- Preset info (control 4 with the database) with the name reading `name`: what it said.
function T.info(header, name)
  T.nameIs(name)
  T.pressAt(header, 4)
  return T.lastSaid()
end
"##;

/// A fresh VM with the scripted host, the runtime, the prelude's matcher, the host-panel helpers
/// and the ones above; the host plays `os`.
pub(crate) fn run_on(os: &str, scenario: &str) {
    let lua = Lua::new();
    let t: Table = harness(&lua);
    let host: Table = t.get("host").unwrap();
    let os_table: Table = host.raw_get("os").unwrap();
    os_table.raw_set("current", os).unwrap();
    let wrapped: Function = lua
        .load(format!("return function(host) {RUNTIME}\nend"))
        .set_name("overlay-runtime/src/main.luau")
        .eval()
        .expect("the runtime compiles");
    let o: Table = wrapped.call(host).expect("the runtime loads against the scripted host");
    t.set("O", o).unwrap();
    let prelude: Function = lua
        .load(format!("return function(host) {WINDOW_PRELUDE}\nend"))
        .set_name("window_prelude.luau")
        .eval()
        .expect("the window prelude compiles");
    t.set("windowPrelude", prelude).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // A file of the repository as text: the shipped preset database's, for the scenarios that run
    // on it (avenger_browser_tests.rs).
    let files = root.clone();
    t.set(
        "repoText",
        lua.create_function(move |_, rel: String| std::fs::read_to_string(files.join(&rel)).map_err(mlua::Error::external))
            .unwrap(),
    )
    .unwrap();
    let source = lua
        .create_function(move |lua, rel: String| {
            let src = std::fs::read_to_string(root.join(&rel)).map_err(mlua::Error::external)?;
            let src = src.strip_prefix('\u{feff}').unwrap_or(&src).to_string();
            lua.load(format!("return function(host) {src}\nend"))
                .set_name(rel)
                .eval::<Function>()
        })
        .unwrap();
    t.set("source", source).unwrap();
    // The host's own JSON decoder, for the preset database's reader (T.presets), and its encoder,
    // which page:post writes with.
    t.set("jsonDecode", lua.create_function(crate::json::decode).unwrap()).unwrap();
    t.set("jsonEncode", lua.create_function(crate::json::encode).unwrap()).unwrap();
    // The page's picture (T.paint…, AV): painted from shapes and reduced by the host's own code.
    t.set("paintCells", lua.create_function(paint_cells).unwrap()).unwrap();
    t.set("paintProfile", lua.create_function(paint_profile).unwrap()).unwrap();
    t.set("paintPixels", lua.create_function(paint_pixels).unwrap()).unwrap();
    t.set("paintDiff", lua.create_function(paint_diff).unwrap()).unwrap();
    t.set(
        "predicateCanonical",
        lua.create_function(|_, src: String| {
            Predicate::parse(&src)
                .map(|p| p.to_string())
                .map_err(|e| mlua::Error::external(format!("host.screen.predicate: {e}")))
        })
        .unwrap(),
    )
    .unwrap();
    // Luau's interrupts — at calls and at the turns of loops — counted, so that a scenario can
    // bound the work a stretch of the module did (T.interrupts()).
    let ticks = Arc::new(AtomicU64::new(0));
    let counted = ticks.clone();
    lua.set_interrupt(move |_| {
        counted.fetch_add(1, Ordering::Relaxed);
        Ok(VmState::Continue)
    });
    t.set("interrupts", lua.create_function(move |_, ()| Ok(ticks.load(Ordering::Relaxed))).unwrap()).unwrap();
    lua.globals().set("T", t).unwrap();
    for (name, chunk) in [("host-panel helpers", HP), ("specificity arbiter", SPECIFICITY_ARBITER), ("avenger helpers", AV)] {
        if let Err(e) = lua.load(chunk).set_name(name).exec() {
            panic!("{e}");
        }
    }
    if let Err(e) = lua.load(scenario).set_name("scenario").exec() {
        panic!("{e}");
    }
    // A message page:post refused is logged by the module (page.luau) and never reaches the page:
    // no scenario ends with one, unless it says it may.
    let refused = r#"
        local S = T.S
        if not S.postsMayFail then
          for _, l in ipairs(S.logs) do
            assert(not string.find(l, "message could not be posted", 1, true), "a post was refused: " .. l)
          end
        end
    "#;
    if let Err(e) = lua.load(refused).set_name("the end of the scenario").exec() {
        panic!("{e}");
    }
    finish(&lua);
}

pub(crate) fn run(scenario: &str) {
    run_on("windows", scenario)
}

// ---------------------------------------------------------------------------------------------
// The page's picture: shapes painted in Rust, reduced by the host's own cells and profile code.
// ---------------------------------------------------------------------------------------------

/// A shape of a scripted picture, in screen coordinates; a pixel is in it when its middle is.
enum Shape {
    Rect { x1: f64, y1: f64, x2: f64, y2: f64, c: [u8; 3] },
    /// A pointer: from `r1` to `r2` along (ux, uy) from (cx, cy), `half` either side of it.
    Ray { cx: f64, cy: f64, ux: f64, uy: f64, r1: f64, r2: f64, half: f64, c: [u8; 3] },
    Disc { cx: f64, cy: f64, r: f64, c: [u8; 3] },
}

impl Shape {
    fn holds(&self, x: f64, y: f64) -> Option<[u8; 3]> {
        let (inside, c) = match *self {
            Shape::Rect { x1, y1, x2, y2, c } => (x >= x1 && x < x2 && y >= y1 && y < y2, c),
            Shape::Ray { cx, cy, ux, uy, r1, r2, half, c } => {
                let (dx, dy) = (x - cx, y - cy);
                let along = dx * ux + dy * uy;
                (along >= r1 && along <= r2 && (dx * uy - dy * ux).abs() <= half, c)
            }
            Shape::Disc { cx, cy, r, c } => ((x - cx).powi(2) + (y - cy).powi(2) <= r * r, c),
        };
        inside.then_some(c)
    }
}

/// `S.picture`: `bg`, and `shapes` painted over it in order.
struct Picture {
    bg: [u8; 3],
    shapes: Vec<Shape>,
}

impl Picture {
    fn at(&self, x: i64, y: i64) -> [u8; 3] {
        let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
        self.shapes.iter().rev().find_map(|s| s.holds(px, py)).unwrap_or(self.bg)
    }

    /// The `w` x `h` pixels from (x, y), as a capture holds them.
    fn image(&self, x: i64, y: i64, w: u32, h: u32) -> CapturedImage {
        let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
        for j in 0..h as i64 {
            for i in 0..w as i64 {
                let c = self.at(x + i, y + j);
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        CapturedImage { w, h, rgba }
    }
}

fn colour_of(t: &Table) -> mlua::Result<[u8; 3]> {
    Ok([t.get(1)?, t.get(2)?, t.get(3)?])
}

fn picture_of(p: &Table) -> mlua::Result<Picture> {
    let bg = colour_of(&p.get::<Table>("bg")?)?;
    let mut shapes = Vec::new();
    for s in p.get::<Table>("shapes")?.sequence_values::<Table>() {
        let s = s?;
        let c = colour_of(&s.get::<Table>("c")?)?;
        let kind: String = s.get("kind")?;
        shapes.push(match kind.as_str() {
            "rect" => Shape::Rect { x1: s.get("x1")?, y1: s.get("y1")?, x2: s.get("x2")?, y2: s.get("y2")?, c },
            "ray" => {
                let a = s.get::<f64>("angle")?.to_radians();
                Shape::Ray {
                    cx: s.get("cx")?,
                    cy: s.get("cy")?,
                    ux: a.cos(),
                    uy: a.sin(),
                    r1: s.get("r1")?,
                    r2: s.get("r2")?,
                    half: s.get::<f64>("width")? / 2.0,
                    c,
                }
            }
            "disc" => Shape::Disc { cx: s.get("cx")?, cy: s.get("cy")?, r: s.get("r")?, c },
            other => return Err(mlua::Error::external(format!("the scripted picture has no shape '{other}'"))),
        });
    }
    Ok(Picture { bg, shapes })
}

/// Corners `{ x1, y1, x2, y2 }` as the top left and the size.
fn area_of(r: &Table) -> mlua::Result<(i64, i64, u32, u32)> {
    let (x1, y1, x2, y2): (i64, i64, i64, i64) = (r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
    if x2 <= x1 || y2 <= y1 {
        return Err(mlua::Error::external("an empty region"));
    }
    Ok((x1, y1, (x2 - x1) as u32, (y2 - y1) as u32))
}

/// `T.paintCells(picture, region, cols, rows, predicate)`: the grid as hex, as host.screen.cells
/// answers it.
fn paint_cells(_: &Lua, (p, region, cols, rows, pred): (Table, Table, u32, u32, String)) -> mlua::Result<String> {
    let (x, y, w, h) = area_of(&region)?;
    let pred = Predicate::parse(&pred).map_err(|e| mlua::Error::external(e.to_string()))?;
    let spec = CellSpec::new(cols, rows, pred).map_err(mlua::Error::external)?;
    let img = picture_of(&p)?.image(x, y, w, h);
    let cells = of_capture(&img, w as i32, h as i32, &spec).map_err(|e| mlua::Error::external(e.to_string()))?;
    Ok(to_hex(&cells))
}

/// `T.paintProfile(picture, region, rows)`: the profile host.screen.profile answers for it — its
/// columns, and its rows as well with `rows` (`axes = "both"`).
fn paint_profile(lua: &Lua, (p, region, rows): (Table, Table, Option<bool>)) -> mlua::Result<Table> {
    let (x, y, w, h) = area_of(&region)?;
    let img = picture_of(&p)?.image(x, y, w, h);
    let rows = rows.unwrap_or(false);
    let (cols, rws) = crate::profile::profile(&img, Area { x: 0, y: 0, w, h }, true, rows, 128)
        .ok_or_else(|| mlua::Error::external("the profile could not be made"))?;
    let axis = |a: crate::profile::Axis| -> mlua::Result<Table> {
        let t = lua.create_table()?;
        t.set("min", a.min)?;
        t.set("max", a.max)?;
        t.set("dark", a.dark)?;
        t.set("mean", a.mean)?;
        t.set("r", a.r)?;
        t.set("g", a.g)?;
        t.set("b", a.b)?;
        Ok(t)
    };
    let out = lua.create_table()?;
    out.set("x", x)?;
    out.set("y", y)?;
    out.set("w", w)?;
    out.set("h", h)?;
    out.set("columns", axis(cols.ok_or_else(|| mlua::Error::external("no columns"))?)?)?;
    if rows {
        out.set("rows", axis(rws.ok_or_else(|| mlua::Error::external("no rows"))?)?)?;
    }
    Ok(out)
}

/// `T.paintPixels(picture, points)`: the colour at every point, as host.screen.pixels answers.
fn paint_pixels(lua: &Lua, (p, points): (Table, Table)) -> mlua::Result<Table> {
    let pic = picture_of(&p)?;
    let out = lua.create_table()?;
    for pt in points.sequence_values::<Table>() {
        let pt = pt?;
        let c = pic.at(pt.get(1)?, pt.get(2)?);
        let col = lua.create_table()?;
        col.set("r", c[0])?;
        col.set("g", c[1])?;
        col.set("b", c[2])?;
        col.set("hex", format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]))?;
        out.raw_push(col)?;
    }
    Ok(out)
}

/// `T.paintDiff(a, b, region, tolerance)`: how many pixels of `region` differ between pictures `a`
/// and `b` by more than `tolerance` in a channel, as a change wait counts them.
fn paint_diff(_: &Lua, (a, b, region, tolerance): (Table, Table, Table, u8)) -> mlua::Result<u32> {
    let (x, y, w, h) = area_of(&region)?;
    let (pa, pb) = (picture_of(&a)?, picture_of(&b)?);
    let mut n = 0;
    for j in 0..i64::from(h) {
        for i in 0..i64::from(w) {
            let (ca, cb) = (pa.at(x + i, y + j), pb.at(x + i, y + j));
            if ca.iter().zip(cb.iter()).any(|(p, q)| p.abs_diff(*q) > tolerance) {
                n += 1;
            }
        }
    }
    Ok(n)
}

pub(crate) fn run_mac(scenario: &str) {
    run_on("macos", scenario)
}

// ---------------------------------------------------------------------------------------------
// Which plug-in the panel shows, and at which zoom: one read of the header.
// ---------------------------------------------------------------------------------------------

/// In REAPER's floating window on a Mac, whose title names Avenger but is never asked: the panel
/// is not Avenger's until its header is READ, off the event loop — MENU and UNDO where the table
/// puts them, the zoom field's "80%". Then the overlay comes up, scaled by 80 / 50, from Avenger's
/// corner at the panel's + (-2, 2), and arriving says the preset's name read there.
#[test]
fn avenger_is_recognised_by_its_header_in_a_reaper_floating_window() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header, box = T.avenger()
        T.ok(not header.active and not box.active, "nothing before the header is read: " .. T.dump())
        local r = T.lastRead("vps-avenger header")
        -- The band: from the panel's corner (100, 82), 0.45 of Avenger's width across — the panel's
        -- 1394, or 898 x 871/561 = 1394.3 by its height, the larger — 627.4 -> 628, and 32/871 of
        -- it down, 51.2 -> 52.
        T.ok(r and T.region(r.region) == "100,82,728,134", r and T.region(r.region))
        T.ok(r.lang == "en-US", "in English, which Avenger's words are: " .. tostring(r.lang))
        T.ok(#S.recognized == 0, "nothing read on the event loop")
        T.event()
        T.ok(#S.reads == 1, "one read per stay, not one per evaluation: " .. #S.reads)
        T.answer(r, T.header(98, 84, 1.6, 80))
        -- The answer asks the overlays of the binding to look again on the module's next turn,
        -- through the function the runtime hands `identify`: no window event is needed, and no
        -- round of every module's overlays is asked for.
        T.ok(S.rechecks == 0 and not header.active, "the answer asks nobody else to look again")
        T.runDue()
        T.ok(header.active and not box.active)
        T.ok(T.count("[avenger] 'VST3i: VPS Avenger - Track 1' (1394x898): VPS Avenger, the zoom field "
          .. "reads 80 % — zoom 80 % (read), factor 1.6000, display factor 1.00. MENU read at (517,106), UNDO "
          .. "at (586,106): (+0.0,+0.0) and (+0.0,+0.0) from where the table puts them with Avenger's "
          .. "corner at the panel's -2,+2") == 1)
        -- Arriving says the preset, read at the name's box: (34, 7)-(214, 22) x 1.6 from (98, 84) =
        -- (152.4, 95.2)-(440.4, 119.2), rounded once.
        S.ocr = function(g) return T.region(g) == "152,95,440,119" and "Init Preset" or "" end
        T.run(400)
        T.ok(T.lastSaid() == "Preset, Init Preset", T.said())
        T.ok(T.count("[scale] 'VPS Avenger': factor 1.6000 about (0,0)") == 1)
    "##);
}

/// In Logic, whose plug-in window is titled by its channel strip ("Inst 1") and names no plug-in,
/// at 50 %: the panel 2 in and 88 down, Avenger's corner at the frame's (0, 90) — where the
/// project's tables start — and a factor of 1. Another plug-in shown in the same window after the
/// keyboard has been on Logic's header is asked about afresh, and its text is a real "no"; a read
/// that saw nothing is no answer, and is made again.
#[test]
fn avenger_is_recognised_in_a_logic_window_titled_inst_1_and_another_plugin_is_not() {
    run_mac(r##"
        local S = T.S
        local L = T.logic(50)
        S.listed = { L }
        T.at(L)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        -- The panel at (2, 118), 873 x 590 — Logic's footer makes it taller than Avenger, so its
        -- height gives the larger width, 590 x 871/561 = 916.0: 412.2 -> 413 across, 33.7 -> 34
        -- down.
        T.ok(r and T.region(r.region) == "2,118,415,152", r and T.region(r.region))
        T.answer(r, T.header(0, 120, 1, 50))
        T.event()
        T.ok(header.active)
        T.ok(T.count("panel of 'Inst 1' id=41 at 2,88 873x590 from its content origin, identify=true — 'VPS Avenger'") == 1)
        T.ok(T.count("zoom 50 % (read), factor 1.0000, display factor 1.00") == 1)
        S.ocr = function(g) return T.region(g) == "34,127,214,142" and "Init Preset" or "" end
        T.run(400)
        T.ok(T.lastSaid() == "Preset, Init Preset", T.said())
        -- The keyboard on Logic's header, where another insert can be picked; back in, and the
        -- panel shows Logic's own EQ now.
        local headerButton = { id = 42, class = "AXButton//", bounds = { x = 20, y = 70, w = 20, h = 20 } }
        T.show(L, { headerButton, L })
        T.ok(not header.active)
        T.show(L)
        local again = T.lastRead("vps-avenger header")
        T.ok(again ~= r and not header.active, "asked afresh in the new stay")
        -- Nothing read: no answer, read again at the next evaluation.
        T.answer(again, { status = "none", text = "", words = {} })
        T.event()
        local third = T.lastRead("vps-avenger header")
        T.ok(third ~= again and not header.active, "nothing read is asked again")
        T.ok(T.count("read nothing (none) — asked again at the next evaluation") == 1)
        T.answer(third, { status = "text", text = "Channel EQ 32 Hz",
          words = { { text = "Channel", x = 10, y = 125, w = 50, h = 10 }, { text = "EQ", x = 64, y = 125, w = 16, h = 10 } } })
        T.event()
        T.event()
        T.ok(not header.active and T.lastRead("vps-avenger header") == third, "text that is not Avenger's header is kept as no")
        T.ok(T.count("[avenger] 'Inst 1' (873x590): not VPS Avenger — neither MENU nor UNDO was read") == 1)
    "##);
}

// ---------------------------------------------------------------------------------------------
// The first Mac test of the Avenger page (2026-10-14): Logic at 80 %, the keyboard on nothing
// after every click into Avenger's own view.
// ---------------------------------------------------------------------------------------------

/// The header's read lands after a click into Avenger's view, with the keyboard on nothing anybody
/// can read: the stay goes on in the same window, and the read's answer asks the overlays to look
/// again through the function the runtime handed `identify` — no window event, no round of every
/// module's overlays — so the header comes up on the module's next turn.
#[test]
fn a_header_read_that_lands_after_the_keyboard_went_unreadable_brings_the_overlay_up() {
    run_mac(r##"
        local S = T.S
        local L = T.logic(80)
        S.listed = { L }
        T.at(L)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        T.ok(r and not header.active, "nothing before the header is read: " .. T.dump())
        T.show(L, {})
        T.answer(r, T.header(0, 120, 1.6, 80))
        T.ok(not header.active, "on the next turn, not inside the answer")
        T.runDue()
        T.ok(header.active, "up with the keyboard on nothing: " .. T.dump())
        T.ok(S.rechecks == 0, "asked again through identify's own function: " .. S.rechecks)
        T.ok(T.count("[overlay] 'VPS Avenger': where the keyboard is in 'Inst 1' cannot be read (an empty "
          .. "focus chain) — the stay goes on there") == 1, T.dump())
    "##);
}

/// Logic at 80 %, the tester's sequence: a click into Avenger that is not the module's (l.772 of the
/// log; whose it was, the log does not say), then the ring's ◀ with Logic reporting the keyboard on
/// nothing after its click (l.2160-2167):
/// the header stays in front — no `[deactivate]`, the database's position not forgotten, no second
/// header read — and the watch ends by saying the new name.
#[test]
fn in_logic_the_ring_stays_up_after_its_own_click_with_the_keyboard_on_nothing() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local L = T.logic(80)
        S.listed = { L }
        T.at(L)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(0, 120, 1.6, 80))
        T.event()
        T.ok(header.active, T.dump())
        -- The name's box at 80 % from Avenger's corner (0, 120): (34, 7)-(214, 22) x 1.6.
        S.ocr = function(g) return T.region(g) == "54,131,342,155" and "LD Lead 2" or "" end
        T.run(400)
        local headerRead = T.lastRead("vps-avenger header")
        T.show(L, {})
        T.ok(header.active, "a click not of the module's: " .. T.dump())
        local said = #S.speech
        T.pressAt(header, 2)
        T.ok(T.clickAt() == "352,142", "◀ where the log has it: " .. T.clickAt())
        T.show(L, {})
        T.ok(header.active and T.count("[deactivate]") == 0, "still in front after its click: " .. T.dump())
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, T.nameRead("LD Lead 1")) end
        end
        T.ok(T.lastSaid() == "LD Lead 1" and #S.speech == said + 1, T.said(said + 1))
        T.ok(header.active and T.count("[deactivate]") == 0 and T.count("forgotten: the overlay left the front") == 0, T.dump())
        T.ok(T.lastRead("vps-avenger header") == headerRead, "no second header read")
    "##);
}

/// The ring's Load and Initialize in Logic, Logic reporting the keyboard on nothing after MENU's
/// click (l.2228-2233 and l.2667-2672 of the log): each item is chosen in Avenger's popup, and
/// Initialize goes on to look for the warning box.
#[test]
fn in_logic_load_and_initialize_from_the_ring_are_chosen_after_logic_reports_nothing() {
    run_mac(r##"
        local S = T.S
        local L = T.logic(80)
        S.listed = { L }
        T.at(L)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(0, 120, 1.6, 80))
        T.event()
        T.run(400)
        T.ok(header.active, T.dump())
        -- MENU: 262 x 1.6 = 419.2 -> 419, 120 + 14 x 1.6 = 142.4 -> 142. Its popup (245, 20) from the
        -- corner in 50 % units: (392, 152), 192 x 96; Load's row 120 + 32 x 1.6 = 171.2 -> 171, across
        -- its middle 392 + 96 = 488.
        T.pressAt(header, 4)
        T.ok(T.clickAt() == "419,142", T.clickAt())
        T.show(L, {})
        T.ok(header.active, T.dump())
        T.popup(300, 392, 152, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "488,171", T.clickAt())
        T.closePopups()
        T.tick(2)
        -- Clicked once and not followed: the item opens a file dialog (main.luau, ONE_CLICK).
        T.ok(T.count("[menu item] 'VPS Avenger': 'Load preset' — test ") == 1
          and T.count("choosing item (96,19) at (488,171)") == 1, T.dump())
        -- Initialize's row: 120 + 68 x 1.6 = 228.8 -> 229.
        T.pressAt(header, 7)
        T.show(L, {})
        T.popup(301, 392, 152, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "488,229", T.clickAt())
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[avenger] 'Initialize preset' chosen: Avenger's warning box is looked for, up to 8 reads") == 1, T.dump())
        T.ok(T.count("not chosen — the overlay left the front first") == 0 and T.count("[deactivate]") == 0, T.dump())
    "##);
}

// ---------------------------------------------------------------------------------------------
// A step's name, settled: two readings alike, or a catalog name.
// ---------------------------------------------------------------------------------------------

/// O:watch counts READINGS, not looks: a `read` that hands back the reading it answers from as its
/// second value is a new reading only when that changes, and `done` sees each reading once, with
/// how many in a row have read the same value; a look that reads nothing neither counts nor breaks
/// the run, and the count reaches `onDone`, `onGiveUp` and the `[watch]` line. A `read` without a
/// second value makes every look a reading, as before.
#[test]
fn o_watch_counts_readings_not_looks() {
    run(r##"
        local S = T.S
        local o = T.overlay(nil, "Watched")
        local seen, done, gaveUp = {}, nil, nil
        S.v, S.id = nil, nil
        o:watch({
          read = function() return S.v, S.id end,
          was = "a",
          done = function(now, _, n) seen[#seen + 1] = now .. ":" .. n; return n >= 2 end,
          within = 2000,
          onDone = function(now, was, n) done = { now, was, n } end,
          onGiveUp = function() gaveUp = true end,
        })
        S.v, S.id = "b", 1
        T.tick(3)
        T.ok(table.concat(seen, ",") == "b:1", "one reading, however many looks: " .. table.concat(seen, ","))
        S.v, S.id = nil, nil
        T.tick(2)
        S.v, S.id = "b", 2
        T.tick(1)
        T.ok(table.concat(seen, ",") == "b:1,b:2", table.concat(seen, ","))
        T.ok(done and done[1] == "b" and done[2] == "a" and done[3] == 2 and not gaveUp, T.dump())
        T.ok(T.count("[watch] Watched: changed after") == 1 and T.count("— a -> b (2 readings alike)") == 1, T.dump())
        -- Another value in between breaks the run.
        seen, done = {}, nil
        o:watch({
          read = function() return S.v, S.id end,
          was = "a",
          done = function(now, _, n) seen[#seen + 1] = now .. ":" .. n; return false end,
          within = 600,
          onGiveUp = function(now, was, n) gaveUp = { now, was, n } end,
        })
        S.v, S.id = "c", 3
        T.tick(1)
        S.v, S.id = "d", 4
        T.tick(1)
        S.v, S.id = "c", 5
        T.tick(4)
        T.ok(table.concat(seen, ",") == "c:1,d:1,c:1", table.concat(seen, ","))
        T.ok(type(gaveUp) == "table" and gaveUp[1] == "c" and gaveUp[3] == 1, T.dump())
        T.ok(T.count("[watch] Watched: gave up after") == 1, T.dump())
        -- Nothing read at the end: n is 0.
        S.v, S.id = nil, nil
        o:watch({ read = function() return S.v, S.id end, was = "a", within = 300,
          onGiveUp = function(now, _, n) gaveUp = { now, n } end })
        T.tick(4)
        T.ok(gaveUp[1] == nil and gaveUp[2] == 0, T.dump())
        -- Without a reading of its own, every look is one.
        seen = {}
        S.v = "e"
        o:watch({ read = function() return S.v end, was = "a",
          done = function(_, _, n) seen[#seen + 1] = n; return n >= 3 end })
        T.tick(4)
        T.ok(table.concat(seen, ",") == "1,2,3", table.concat(seen, ","))
        T.ok(T.count("— a -> e (3 readings alike)") == 1, T.dump())
    "##);
}

/// ◀ or ▶ reads Avenger's loading display first, which is no name — the tester's log has
/// "cYanmaGentaYellowb", "PY" and "M" said for three of the page's ▶ (l.1797, l.1823 and l.1850),
/// composed here into one step — and each read is handed to two of the watch's looks, as on a Mac:
/// without the database the name counts once two READINGS agree, so the step says "AR Detroit
/// 303" and never what was read on the way.
#[test]
fn a_step_says_the_name_once_two_reads_agree_and_not_avengers_loading_display() {
    run_mac(r##"
        local S = T.S
        local header = T.avengerAt80("AR Catch Me")
        local said = #S.speech
        T.pressAt(header, 3)
        local answers = { "cYanmaGentaYellowb", "PY", "M", "AR Detroit 303" }
        T.slowNameReads(function(i) return answers[math.min(i, #answers)] end)
        T.ok(#S.speech == said + 1 and T.lastSaid() == "AR Detroit 303", T.said(said + 1))
        T.ok(T.count("[watch] VPS Avenger: changed after") == 1
          and T.count("— AR Catch Me -> AR Detroit 303 (2 readings alike)") == 1, T.dump())
    "##);
}

/// With the preset database a name the catalog has counts at its first reading — the step says
/// it then — and a reading the catalog does not have ("M") waits for a second one.
#[test]
fn with_the_database_a_catalog_name_counts_at_its_first_read() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("LD Lead 1")
        local said = #S.speech
        T.pressAt(header, 3)
        local answers = { "M", "LD Lead 2" }
        T.slowNameReads(function(i) return answers[math.min(i, #answers)] end)
        T.ok(#S.speech == said + 1 and T.lastSaid() == "LD Lead 2", T.said(said + 1))
        T.ok(T.count("— LD Lead 1 -> LD Lead 2") == 1 and T.count("(2 readings alike)") == 0,
          "at its first reading: " .. T.dump())
    "##);
}

/// A reading that changed, was read once by the watch's bound and is no catalog name is not taken:
/// "the preset name cannot be read now" without the database, the neighbour with it, marked.
#[test]
fn a_name_read_once_by_the_watchs_bound_is_not_taken() {
    for db in [false, true] {
        let scenario = r##"
            local S = T.S
            if @DB@ then T.presets() end
            local header = T.avengerAt80(@DB@ and "LD Lead 1" or "AR Catch Me")
            local said = #S.speech
            T.pressAt(header, 3)
            T.slowNameReads(function(i) return i == 1 and "cYanmaGentaYellowb" or nil end)
            local want = @DB@ and "LD Lead 2, from the database" or "the preset name cannot be read now"
            T.ok(#S.speech == said + 1 and T.lastSaid() == want, T.said(said + 1))
            T.ok(T.count("[avenger] 'Next preset': 'cYanmaGentaYellowb' was read once and is no catalog "
              .. "name — not taken") == 1, T.dump())
        "##;
        run_mac(&scenario.replace("@DB@", if db { "true" } else { "false" }));
    }
}

/// ▶ ▶ within half a second on a Mac: the first step's name read once, then the second press. The
/// first step's watch, overtaken, reads nothing more and says nothing at its bound; the second says
/// the name, and it is the last thing said — not "the preset name cannot be read now" after it.
#[test]
fn a_step_overtaken_by_a_second_press_leaves_the_last_word_to_it() {
    run_mac(r##"
        local S = T.S
        local header = T.avengerAt80("AR Band Pass Slide")
        local said = #S.speech
        T.pressAt(header, 3)
        T.run(150)
        T.answer(T.lastRead("vps-avenger name"), T.nameRead("AR Deep Arp"))
        T.run(150)
        T.ok(#S.speech == said, "read once, not said yet: " .. T.said(said + 1))
        T.nameIs("AR Deep Arp")
        T.pressAt(header, 3)
        T.nameReads("BA Deep Sub")
        T.ok(T.said(said + 1) == "BA Deep Sub", T.said(said + 1))
        T.ok(T.count("[watch] VPS Avenger: gave up after") == 1 and T.count("[watch] VPS Avenger: changed after") == 1,
          "the first watch ran out, unsaid: " .. T.dump())
    "##);
}

/// On Windows Avenger is its own child window, matched by its class's KalliGuiWin prefix and
/// identified by the same header read; its corner is the child's, with no DAW difference added —
/// the log line gives the offsets with the panel's (-2, +2) as well, so one Windows session says
/// which of the two it is. At 150 % display scaling Avenger is drawn half again as large at the
/// same zoom, which the header says: the zoom field reads 80 %, MENU and UNDO sit where a factor of
/// 2.4 puts them, and the display factor is 1.5. A display scaled to 110 % — no quarter step —
/// fits nothing: the captions are Avenger's, but it is not taken as Avenger until a factor fits.
/// Where English OCR is not installed the header is read in the user's own language.
#[test]
fn on_windows_avenger_is_its_kalligui_child_and_a_display_factor_is_learned_from_the_header() {
    run(r##"
        local S = T.S
        local J = T.onWindows(80, 1.5)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        -- The child at (248, 60), 2091 across: 940.95 -> 941, and 76.8 -> 77 down.
        T.ok(r and T.region(r.region) == "248,60,1189,137", r and T.region(r.region))
        T.ok(not header.active)
        T.answer(r, T.header(248, 60, 2.4, 80))
        -- Picked up through the `again` the runtime hands `identify` on the pattern's path too: on
        -- the module's next turn, with no window event and no round of every module's overlays.
        T.ok(S.rechecks == 0 and not header.active, "asked again through identify's own function: " .. S.rechecks)
        T.runDue()
        T.ok(header.active and header:origin().id == 70)
        T.ok(T.count("attachEmbedded: matched control [KalliGuiWin18360-000000000D72C850] id=70, identify=true") == 1)
        T.ok(T.count("zoom 80 % (read), factor 2.4000, display factor 1.50") == 1)
        T.ok(T.count("from where the table puts them with Avenger's corner at the panel's +0,+0, "
          .. "(+2.0,-2.0) and (+2.0,-2.0) with it at -2,+2") == 1, "no DAW difference on the plug-in's own window")
        -- Undo: (305, 14) x 2.4 from (248, 60) = (980, 93.6), rounded once: (980, 94).
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "980,94", T.clickAt())
        T.ok(S.ownsAsked[#S.ownsAsked][1] == 70, "asked of Avenger's window")
        T.ok(T.lastSaid() == "Undo, activated", T.said())
    "##);
    run(r##"
        local S = T.S
        -- A Windows without English OCR: the user's own language, not a read that fails.
        S.english = false
        local J = T.onWindows(80, 1.1)
        local header = T.avenger()
        T.ok(T.lastRead("vps-avenger header").lang == "de-DE", tostring(T.lastRead("vps-avenger header").lang))
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 1.76, 80))
        T.event()
        T.ok(not header.active, "not taken as Avenger with no factor: it would take Tab, Return and Space to place nothing")
        T.ok(T.count("the header reads like VPS Avenger's, but no factor: the zoom field reads 80 %, and MENU and UNDO are read "
          .. "where a factor of 1.760 puts them, 10 % off the 1.600 that zoom gives at a display factor of 1.00 — "
          .. "not taken as VPS Avenger until one fits; read again") == 1)
        local r = T.lastRead("vps-avenger header")
        T.ok(r.answered == false, "read again at that evaluation")
        T.ok(T.count("not VPS Avenger") == 0, "and never a no")
    "##);
}

/// On Windows the runtime asks `identify` once per CONTROL, and a zoom step resizes Avenger's child
/// window without making it another control: the new size is Avenger's still, a first read there
/// that misses the header is read again ("lost"), never a "no" that would leave the overlay up with
/// every control dead. On Windows with the zoom field unread, the size is the factor and the zoom is
/// not guessed: the Zoom control says it cannot be read and opens nothing.
#[test]
fn on_windows_a_zoom_step_keeps_the_control_avengers_and_an_unread_zoom_is_not_guessed() {
    run(r##"
        local S = T.S
        T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.run(400)
        T.stepAt(header, 10, 1)
        -- The zoom field: 248 + 2.4 x 21 = 298.4 -> 298, 60 + 33.6 -> 94.
        T.ok(T.clickAt() == "298,94", T.clickAt())
        -- The list, rows of 27.6, 584 tall: a border of 2.2; entry 8 at 110 + 2.2 + 27.6 x 7.5 =
        -- 319.2 -> 319, across 270 + 75 = 345.
        T.popup(500, 270, 110, 150, 584)
        T.tick(1)
        T.ok(T.clickAt() == "345,319", T.clickAt())
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[avenger] zoom step from 80 % chosen") == 1, T.dump())
        -- REAPER resizes the child to Avenger at 85 %: 2222 x 1431.
        T.onWindows(85, 1.5)
        T.event()
        T.tick(1)
        local r = T.lastRead("vps-avenger header")
        T.ok(r and not r.answered and T.region(r.region) == "248,60,1248,142", r and T.region(r.region))
        T.answer(r, T.reading({ { text = "Loading", x = 300, y = 80, w = 60, h = 20 } }))
        T.ok(header.active, "still up")
        T.ok(T.count("not VPS Avenger") == 0, "a miss at a new size of Avenger's own control is no 'no'")
        T.ok(T.count("the header is not read where it was (neither MENU nor UNDO was read)") == 1, T.dump())
        T.tick(1)
        local r2 = T.lastRead("vps-avenger header")
        T.ok(r2 ~= r and not r2.answered, "read again")
        T.answer(r2, T.header(248, 60, 2.55, 85))
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom, 85 percent, slider", T.said())
        -- Undo at the new factor: 248 + 2.55 x 305 = 1025.75 -> 1026, 60 + 35.7 -> 96.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "1026,96", T.clickAt())
    "##);
    run(r##"
        local S = T.S
        T.onWindows(80, 1.25)
        local header = T.avenger()
        -- 80 % at a display of 125 %: the size of 100 % at 100 %. The caption unread.
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2, false))
        T.event()
        T.ok(header.active)
        T.ok(T.count("zoom not known (the display's scaling is not known, and the zoom field was not read), "
          .. "factor 2.0000 from the header's size") == 1, T.dump())
        T.ok(header.controls[10].text(header) == "cannot be read now")
        local clicks = #S.clicks
        T.stepAt(header, 10, 1)
        T.tick(1)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Zoom, cannot be read now, slider", "nothing opened: " .. T.said())
        -- Undo at the size's factor: 248 + 610 = 858, 60 + 28 = 88.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "858,88", T.clickAt())
    "##);
}

// ---------------------------------------------------------------------------------------------
// The first Windows session (2026-10-30): on Avenger's own window no read is a "no", `identify`
// raises while it cannot tell, every undecided read asks for the next at once, and the module
// bounds itself at eight such reads per visit — the maintainer's decisions of the same day.
// ---------------------------------------------------------------------------------------------

/// A first read of "Loading" — Avenger still painting its header — is "not yet" on Avenger's own
/// window, never the "no" the runtime would keep for as long as the child lives: no `not VPS
/// Avenger`, and the next read asked for at once through the runtime's `again`, with no window
/// event; the header then brings the overlay up.
#[test]
fn on_windows_a_first_read_without_the_header_is_not_yet_and_read_again_at_once() {
    run(r##"
        local S = T.S
        T.onWindows(80, 1.5)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        T.answer(r, T.reading({ { text = "Loading", x = 300, y = 80, w = 60, h = 20 } }))
        T.ok(not header.active and T.count("not VPS Avenger") == 0, "no 'no': " .. T.dump())
        T.ok(T.count("[avenger] 'FX: Track 1 \"VPS Avenger\"' (2091x1346): neither MENU nor UNDO was read in "
          .. "the header band at 248,60 941x77: \"Loading\" — no verdict yet (read 1 of 8 in this visit)") == 1, T.dump())
        T.runDue()
        local r2 = T.lastRead("vps-avenger header")
        T.ok(r2 ~= r and not r2.answered and T.readsOf("vps-avenger header") == 2,
          "read again on the module's next turn, no window event: " .. T.dump())
        T.answer(r2, T.header(248, 60, 2.4, 80))
        T.runDue()
        T.ok(header.active and header:origin().id == 70, T.dump())
        T.ok(T.count("identify raised: VPS Avenger's header has given no verdict yet — taken as could not tell "
          .. "yet, and not counted") >= 1, T.dump())
        T.ok(T.count("identify could not tell") == 0, "nothing counted towards the runtime's eight")
    "##);
}

/// While a read of Avenger's own window is out `identify` raises one fixed message, which the
/// runtime does not count: twelve evaluations meanwhile — each asks it twice here, the control
/// being among the window's children and on the focus chain — are never taken as a "no", and the
/// header, when it lands, brings the overlay up.
#[test]
fn on_windows_evaluations_while_a_read_is_out_are_never_taken_as_no() {
    run(r##"
        local S = T.S
        T.onWindows(80, 1.5)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        for _ = 1, 12 do T.event() end
        T.ok(T.readsOf("vps-avenger header") == 1 and not r.answered and not header.active,
          "one read out, asked once: " .. T.readsOf("vps-avenger header"))
        T.ok(T.count("identify could not tell") == 0 and T.count("identify=false") == 0, "never a no: " .. T.dump())
        T.answer(r, T.header(248, 60, 2.4, 80))
        T.runDue()
        T.ok(header.active, T.dump())
        T.ok(T.count("attachEmbedded: matched control [KalliGuiWin18360-000000000D72C850] id=70, identify=true") == 1)
    "##);
}

/// A read that saw nothing (blank) and one that read only half the header (MENU without UNDO) are
/// no answer on Avenger's own window either: each is read again at once through `again`, with no
/// window event between — where before nothing read again until the user's next focus change.
#[test]
fn on_windows_a_blank_or_half_read_is_read_again_through_again() {
    run(r##"
        local S = T.S
        T.onWindows(80, 1.5)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        T.answer(r, { status = "blank", text = "", words = {} })
        T.runDue()
        local r2 = T.lastRead("vps-avenger header")
        T.ok(r2 ~= r and not r2.answered, "a blank read is read again at once: " .. T.dump())
        local half = T.header(248, 60, 2.4, 80)
        table.remove(half.words, 5)
        T.answer(r2, half)
        T.ok(T.count("no UNDO was read in the header band at 248,60 941x77") == 1
          and T.count("no verdict yet (read 2 of 8 in this visit)") == 1, T.dump())
        T.runDue()
        local r3 = T.lastRead("vps-avenger header")
        T.ok(r3 ~= r2 and not r3.answered, "half the header is read again at once: " .. T.dump())
        T.answer(r3, T.header(248, 60, 2.4, 80))
        T.runDue()
        T.ok(header.active and T.count("not VPS Avenger") == 0, T.dump())
    "##);
}

/// What the runtime no longer counts, the module bounds: a KalliGui editor whose header reads
/// "Menu" and "Undo" in mixed case — another plug-in of the same toolkit — is never taken for
/// Avenger and is read eight times in one visit, each read asked for at once, then not again in
/// that visit, said once. Another window in front and REAPER's FX window again is a new visit:
/// read again, eight more at most, the re-arming said once. The visits are counted by an onFocus
/// callback of the module's, registered once, at the first `identify` about Avenger's own window.
#[test]
fn on_windows_an_editor_that_is_not_avenger_is_read_eight_times_a_visit() {
    run(r##"
        local S = T.S
        T.countHeaderFocus()
        local J = T.onWindows(80, 1.5)
        local header = T.avenger()
        T.ok(S.headerFocus == 1, "one onFocus callback, at the first identify: " .. S.headerFocus)
        local function notAvenger()
          local reading = T.header(248, 60, 2.4, 80)
          reading.words[4].text, reading.words[5].text = "Menu", "Undo"
          return reading
        end
        -- Every read answered as it is asked, and the module's turns played: no window event.
        local function answerAll(turns)
          for _ = 1, turns do
            local r = T.lastRead("vps-avenger header")
            if r and not r.answered then T.answer(r, notAvenger()) end
            T.runDue()
          end
        end
        answerAll(12)
        T.ok(T.readsOf("vps-avenger header") == 8, "eight reads in the visit: " .. T.readsOf("vps-avenger header"))
        T.ok(not header.active and T.count("not VPS Avenger") == 0, T.dump())
        T.ok(T.count("[avenger] 'FX: Track 1 \"VPS Avenger\"' (2091x1346): 8 reads of the header in this visit "
          .. "gave no verdict — not read again until another window has been in front and this one is again") == 1, T.dump())
        -- More events in the same visit read nothing, and say nothing more.
        T.event()
        T.event()
        T.ok(T.readsOf("vps-avenger header") == 8 and T.count("gave no verdict") == 1, T.dump())
        -- A moment with nothing in front, and the same FX window again, is no visit.
        T.hide(T.WFX)
        T.event()
        T.show(T.WFX, { J, T.WFX })
        T.ok(T.readsOf("vps-avenger header") == 8 and T.count("another visit") == 0,
          "nothing in front is no window: " .. T.dump())
        -- Another window, then REAPER's FX window again: a new visit.
        T.show(T.OTHER)
        T.ok(T.readsOf("vps-avenger header") == 8, "nothing read while another window is in front")
        T.show(T.WFX, { J, T.WFX })
        T.ok(T.count("[avenger] 'FX: Track 1 \"VPS Avenger\"' (2091x1346): another visit — the header is "
          .. "read again, up to 8 times") == 1, T.dump())
        answerAll(12)
        T.ok(T.readsOf("vps-avenger header") == 16, "eight more: " .. T.readsOf("vps-avenger header"))
        T.ok(T.count("gave no verdict") == 2 and T.count("another visit") == 1 and not header.active, T.dump())
        T.ok(T.count("identify could not tell") == 0 and T.count("identify=false") == 0, T.dump())
        T.ok(S.headerFocus == 1, "still one callback: " .. S.headerFocus)
    "##);
}

/// The visit's eighth read out while the user goes to another window and back: it lands in the
/// new visit and counts there — read 1 of 8 in this visit, the next read asked for at once —
/// instead of ending, unread, a visit that has only begun (the review of 2026-10-31). The new
/// visit then has its eight reads and no more, said once, and no stop was said that had to end.
#[test]
fn on_windows_a_read_that_lands_in_the_next_visit_counts_there() {
    run(r##"
        local S = T.S
        local J = T.onWindows(80, 1.5)
        local header = T.avenger()
        local function notAvenger()
          local reading = T.header(248, 60, 2.4, 80)
          reading.words[4].text, reading.words[5].text = "Menu", "Undo"
          return reading
        end
        for _ = 1, 7 do
          T.answer(T.lastRead("vps-avenger header"), notAvenger())
          T.runDue()
        end
        local r8 = T.lastRead("vps-avenger header")
        T.ok(T.readsOf("vps-avenger header") == 8 and not r8.answered, "the eighth out: " .. T.dump())
        -- Away and back while it is out: a new visit, and nothing more read meanwhile.
        T.show(T.OTHER)
        T.show(T.WFX, { J, T.WFX })
        T.ok(T.readsOf("vps-avenger header") == 8, T.dump())
        T.answer(r8, notAvenger())
        T.ok(T.count("no verdict yet (read 1 of 8 in this visit)") == 2 and T.count("gave no verdict") == 0,
          "counted in the visit it landed in: " .. T.dump())
        T.runDue()
        T.ok(T.readsOf("vps-avenger header") == 9, "the next read at once, no window event: " .. T.dump())
        for _ = 1, 12 do
          local r = T.lastRead("vps-avenger header")
          if not r.answered then T.answer(r, notAvenger()) end
          T.runDue()
        end
        T.ok(T.readsOf("vps-avenger header") == 15, "eight in the new visit: " .. T.readsOf("vps-avenger header"))
        T.ok(T.count("gave no verdict") == 1 and T.count("another visit") == 0 and not header.active, T.dump())
    "##);
}

/// READS_MAX bounds only the reads of a state already held as Avenger's: seven reads of "Loading"
/// and then the header use none of it. A zoom step to 85 % leaves 80 %'s state without a factor;
/// the child back at 80 % with no step of ours (the zoom changed some other way) reads its header
/// again and places at 80 %'s factor — where the seven "Loading" reads, counted there, would have
/// stopped it unread until the overlay left the front.
#[test]
fn on_windows_undecided_reads_leave_a_held_state_its_rereads() {
    run(r##"
        local S = T.S
        T.onWindows(80, 1.5)
        local header = T.avenger()
        for _ = 1, 7 do
          T.answer(T.lastRead("vps-avenger header"), T.reading({ { text = "Loading", x = 300, y = 80, w = 60, h = 20 } }))
          T.runDue()
        end
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.runDue()
        T.ok(header.active and T.readsOf("vps-avenger header") == 8, T.dump())
        T.event()
        T.run(400)
        T.stepAt(header, 10, 1)
        T.popup(500, 270, 110, 150, 584)
        T.tick(1)
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[avenger] zoom step from 80 % chosen") == 1, T.dump())
        -- REAPER resizes the child to Avenger at 85 %, whose header ends the step.
        T.onWindows(85, 1.5)
        T.event()
        T.tick(1)
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.55, 85))
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom, 85 percent, slider", T.said())
        -- Back at 80 %, no step of ours: its state, without a factor since the step, is read again
        -- when a control is placed there — Undo pressed, and nothing clicked until the read lands.
        T.onWindows(80, 1.5)
        T.event()
        local clicks = #S.clicks
        T.pressAt(header, 8)
        local r = T.lastRead("vps-avenger header")
        T.ok(r and not r.answered and T.region(r.region) == "248,60,1189,137", "read again: " .. T.dump())
        T.ok(#S.clicks == clicks and T.count("brought no zoom") == 0, T.dump())
        T.answer(r, T.header(248, 60, 2.4, 80))
        T.tick(1)
        -- Undo at 80 %'s factor again: (980, 94).
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "980,94", T.clickAt())
    "##);
}

/// On a DAW's panel (a Mac) the rule is the one it was: `identify` answers nil while a read is out
/// — no raise — text without Avenger's captions is a "no" kept for the stay, and the module
/// registers no onFocus callback of its own: the visits are Avenger's own window's.
#[test]
fn on_a_mac_panel_text_without_the_captions_stays_no_for_the_stay() {
    run_mac(r##"
        local S = T.S
        T.countHeaderFocus()
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        T.event()
        T.ok(T.count("identify raised") == 0, "nil on a panel, not a raise: " .. T.dump())
        T.answer(r, T.reading({ { text = "Loading", x = 300, y = 100, w = 60, h = 20 } }))
        T.runDue()
        T.event()
        T.event()
        T.ok(not header.active and T.lastRead("vps-avenger header") == r, "kept as no for the stay: " .. T.dump())
        T.ok(T.count("[avenger] 'VST3i: VPS Avenger - Track 1' (1394x898): not VPS Avenger — neither MENU nor "
          .. "UNDO was read in the header band") == 1, T.dump())
        T.ok(T.count("no verdict yet") == 0 and T.count("by the window's width") == 0, T.dump())
        T.ok(S.headerFocus == 0, "no onFocus of the module's on a Mac: " .. S.headerFocus)
    "##);
}

/// The factor Avenger's own window's width gives is logged beside the one read, and used for
/// nothing (the maintainer's decision of 2026-10-30): a child 1307 wide — 75 % by its width —
/// whose header reads 80 % is placed at 80 %'s factor, 1.6. On a DAW's panel the line has no such
/// part: the panel's width is the DAW's window's.
#[test]
fn the_width_factor_of_avengers_own_window_is_logged_and_nothing_else() {
    run(r##"
        local S = T.S
        local J = T.onWindows(80, 1)
        -- 1307 / 871.25 = 1.50014: Avenger's width at 75 %.
        J.bounds.w, J.client.w = 1307, 1307
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 1.6, 80))
        T.runDue()
        T.ok(header.active, T.dump())
        T.ok(T.count("[avenger] 'FX: Track 1 \"VPS Avenger\"' (1307x898): VPS Avenger, the zoom field reads 80 % — "
          .. "zoom 80 % (read), factor 1.6000, display factor 1.00; by the window's width 1.5001 (1307 / 871.25), "
          .. "logged only. MENU read at") == 1, T.dump())
        -- Undo at 80 %'s factor: 248 + 1.6 x 305 = 736, 60 + 22.4 -> 82.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "736,82", T.clickAt())
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.runDue()
        T.ok(header.active and T.count("zoom 80 % (read), factor 1.6000, display factor 1.00. MENU read at") == 1, T.dump())
        T.ok(T.count("by the window's width") == 0, "not on a panel")
    "##);
}

/// Identity is strict. A caption misread as another step fits nothing on a DAW's panel, where the
/// display factor is 1 (60 % read as "50%"), and is read again rather than taken; another
/// plug-in's "Menu" and "Undo" in mixed case are not Avenger's; captions only in boxes the
/// recogniser guessed are no evidence and are read again, not a "no"; and a percentage in the
/// preset's name is not the zoom.
#[test]
fn identity_takes_capitals_located_boxes_and_a_factor_that_fits() {
    run_mac(r##"
        local S = T.S
        local W = T.float(60)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local reading = T.header(98, 84, 1.2, 60)
        reading.words[1].text = "50%"
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(not header.active)
        T.ok(T.count("the header reads like VPS Avenger's, but no factor: the zoom field reads 50 %, and MENU and "
          .. "UNDO are read where a factor of 1.200 puts them, 20 % off the 1.000 that zoom gives at a display "
          .. "factor of 1.00") == 1, T.dump())
        -- Read again at the next evaluation, and read right this time.
        T.event()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.2, 60))
        T.event()
        T.ok(header.active, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local reading = T.header(98, 84, 1.6, 80)
        reading.words[4].text, reading.words[5].text = "Menu", "Undo"
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(not header.active)
        T.ok(T.count("not VPS Avenger — neither MENU nor UNDO was read") == 1, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local reading = T.header(98, 84, 1.6, 80)
        reading.words[4].approx, reading.words[5].approx = true, true
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(not header.active)
        T.ok(T.count("MENU and UNDO were read only in boxes the recogniser guessed in the header band") == 1, T.dump())
        T.ok(T.count("not VPS Avenger") == 0, "no answer, not a no")
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.ok(header.active)
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        -- The caption unread, and the preset called "Pad 100%": its "100%" on the header's row,
        -- right of where the name begins (98 + 34 x 1.6 = 152.4).
        local reading = T.header(98, 84, 1.6, false)
        table.insert(reading.words, 3, { text = "100%", x = 328, y = 99.4, w = 20, h = 14 })
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(header.active)
        T.ok(T.count("the zoom field was not read — zoom 80 % (seen from the header's size, not read)") == 1, T.dump())
    "##);
}

/// Every control's click point at 50, 80 and 200 %, in REAPER's floating window: the table's
/// point x zoom / 50 from Avenger's corner, (98, 84) — the panel's (100, 82) + (-2, 2) — rounded
/// once.
#[test]
fn every_click_lands_where_the_table_puts_it_at_50_80_and_200_percent() {
    run_mac(r##"
        local S = T.S
        local header
        -- zoom -> { previous, next, MENU, UNDO, zoom field } as "x,y", worked out:
        --   50 %:  98 + 220 = 318, 98 + 232 = 330, 98 + 262 = 360, 98 + 305 = 403, 98 + 21 = 119;
        --          y 84 + 14 = 98
        --   80 %:  98 + 352 = 450, 98 + 371.2 -> 469, 98 + 419.2 -> 517, 98 + 488 = 586,
        --          98 + 33.6 -> 132; y 84 + 22.4 -> 106
        --   200 %: 98 + 880 = 978, 98 + 928 = 1026, 98 + 1048 = 1146, 98 + 1220 = 1318,
        --          98 + 84 = 182; y 84 + 56 = 140
        local expected = {
          [50] = { "318,98", "330,98", "360,98", "403,98", "119,98" },
          [80] = { "450,106", "469,106", "517,106", "586,106", "132,106" },
          [200] = { "978,140", "1026,140", "1146,140", "1318,140", "182,140" },
        }
        for _, zoom in ipairs({ 50, 80, 200 }) do
          local W = T.float(zoom)
          S.listed = { W }
          if not header then
            T.at(W)
            header = T.avenger()
          else
            T.show(W)
          end
          local k = zoom / 50
          T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, k, zoom))
          T.event()
          T.ok(header.active, zoom .. " %: active")
          S.ocr = function() return "BA Deep Sub" end
          local e = expected[zoom]
          T.pressAt(header, 2)
          T.ok(T.clickAt() == e[1], zoom .. " % previous: " .. T.clickAt())
          T.pressAt(header, 3)
          T.ok(T.clickAt() == e[2], zoom .. " % next: " .. T.clickAt())
          T.pressAt(header, 4)
          T.ok(T.clickAt() == e[3] and T.lastSaid() == "Load preset, activated", zoom .. " % MENU: " .. T.clickAt())
          T.pressAt(header, 8)
          T.ok(T.clickAt() == e[4] and S.clicks[#S.clicks].button == nil, zoom .. " % undo: " .. T.clickAt())
          T.pressAt(header, 9)
          T.ok(T.clickAt() == e[4] and S.clicks[#S.clicks].button == "right", zoom .. " % redo list: " .. T.clickAt())
          -- A step that has an entry beside it: up from 50 and 80, down from 200.
          T.stepAt(header, 10, zoom == 200 and -1 or 1)
          T.ok(T.clickAt() == e[5], zoom .. " % zoom field: " .. T.clickAt())
          T.run(9000)
          T.closePopups()
        end
    "##);
}

// ---------------------------------------------------------------------------------------------
// What the plug-in did, read back.
// ---------------------------------------------------------------------------------------------

/// ▶ is clicked, and then nothing is said until the preset's name READ on screen differs from the
/// one read just before the click: a read that still shows the old name is not said, the new one
/// is. ◀ that changes nothing is said as it is, "unchanged", when the watch gives up. Something
/// drawn over ▶ is not clicked.
#[test]
fn previous_and_next_say_the_new_name_once_it_is_on_screen() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        S.ocr = function(g) return T.region(g) == "152,95,440,119" and "AR Catch Me" or "" end
        T.run(400)
        local function name(text) return { status = "text", text = text, words = {} } end
        local said = #S.speech
        T.pressAt(header, 3)
        T.ok(T.clickAt() == "469,106", T.clickAt())
        T.ok(#S.speech == said, "nothing is said at the click: " .. T.said(said + 1))
        T.run(150)
        local r1 = T.lastRead("vps-avenger name")
        T.ok(r1 and T.region(r1.region) == "152,95,440,119", "the name's box, off the event loop")
        T.answer(r1, name("AR Catch Me"))
        T.run(150)
        T.ok(#S.speech == said, "the old name, not redrawn yet, is not said: " .. T.said(said + 1))
        local r2 = T.lastRead("vps-avenger name")
        T.ok(r2 ~= r1, "read again")
        T.answer(r2, name("BA  Deep Sub"))
        T.run(150)
        T.ok(#S.speech == said, "a name read once is not said yet: " .. T.said(said + 1))
        local r3 = T.lastRead("vps-avenger name")
        T.ok(r3 ~= r2, "read again")
        T.answer(r3, name("BA Deep Sub"))
        T.run(150)
        T.ok(T.lastSaid() == "BA Deep Sub", T.said())
        T.ok(T.count("[watch] VPS Avenger: changed after") == 1 and T.count("-> BA Deep Sub (2 readings alike)") == 1, T.dump())
        -- ◀ at the first preset: the name stays, and is said as it is when the watch gives up.
        S.ocr = function(g) return T.region(g) == "152,95,440,119" and "BA Deep Sub" or "" end
        T.pressAt(header, 2)
        T.ok(T.clickAt() == "450,106", T.clickAt())
        -- The watch looks every 100 ms for 2000: twenty looks, one per tick of 150.
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, name("BA Deep Sub")) end
        end
        T.ok(T.lastSaid() == "BA Deep Sub, unchanged", T.said())
        -- Something drawn over ▶: not clicked, and said.
        S.owns = function(id) return id ~= 31 end
        local clicks = #S.clicks
        T.pressAt(header, 3)
        T.ok(#S.clicks == clicks and T.lastSaid() == "something else is covering Next preset, or its window is gone", T.said())
    "##);
}

/// The name before the click cannot be read (small text at 50 %): with no name read before at all,
/// nothing counts as a change — the first reading after the click is taken before Avenger redraws,
/// and would be the OLD name said as new — and the watch says the last name it read when it is
/// over, without "unchanged". Once a name has been read, it stands in for the one that could not be.
#[test]
fn a_name_before_the_click_that_cannot_be_read_is_not_taken_for_a_change() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        S.ocr = function() return "" end
        T.run(400)
        local function name(text) return { status = "text", text = text, words = {} } end
        local said = #S.speech
        T.pressAt(header, 3)
        local answers = { "Init Preset", "BA Deep Sub" }
        for i = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, name(answers[math.min(i, 2)])) end
          if i < 13 then
            T.ok(#S.speech == said, "nothing said before the watch is over: " .. T.said(said + 1))
          end
        end
        T.ok(T.lastSaid() == "BA Deep Sub", "the last name read, with no claim of a change: " .. T.said(said + 1))
        for i = said + 1, #S.speech do
          T.ok(S.speech[i].text ~= "Init Preset", "the old name is never said as new")
        end
        -- The next press: the name before is the last one read.
        said = #S.speech
        T.pressAt(header, 3)
        T.run(150)
        T.answer(T.lastRead("vps-avenger name"), name("BA Deep Sub"))
        T.run(150)
        T.ok(#S.speech == said, "the last name, not redrawn yet, is not said")
        T.answer(T.lastRead("vps-avenger name"), name("CH Chord Stack"))
        T.run(150)
        T.ok(#S.speech == said, "a name read once is not said yet: " .. T.said(said + 1))
        T.answer(T.lastRead("vps-avenger name"), name("CH Chord Stack"))
        T.run(150)
        T.ok(T.lastSaid() == "CH Chord Stack", T.said(said + 1))
    "##);
}

/// MENU's items are chosen in Avenger's own popup once newWindow sees it — not after a wait —
/// across the popup's middle, at the row the table puts the item from Avenger's corner, and on a
/// Mac asked of the popup's own window by its number in the window list. A popup that opened above
/// MENU, where JUCE puts one with no room below (JUCE-like behaviour, unmeasured on Avenger's
/// Windows toolkit, KalliGui), is not chosen in, and is closed by clicking MENU again. On Windows
/// the popup's own window is asked whether it is drawn at the item.
#[test]
fn menu_items_are_chosen_in_avengers_popup_once_it_is_seen() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.pressAt(header, 4)
        T.ok(T.clickAt() == "517,106" and T.lastSaid() == "Load preset, activated", T.clickAt())
        local clicks = #S.clicks
        T.tick(3)
        T.ok(#S.clicks == clicks, "no popup, no item: nothing on a timer")
        -- MENU's popup below MENU: (245, 20) from Avenger's corner in 50 % units, 120 x 60. Load's row
        -- is y 32: 84 + 51.2 -> 135; the middle of the popup across, 490 + 96 = 586.
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,135", T.clickAt())
        T.ok(T.count("[avenger] 'Load preset': MENU's popup at (245.0,20.0) 120.0x60.0 from Avenger's "
          .. "corner in 50 % units; the item 11.9 below its top, across its middle") == 1)
        T.ok(T.count("its window 300 of the window list is asked what is drawn there") == 1)
        T.closePopups()
        T.tick(2)
        -- Save as, and a popup moved up above MENU, as JUCE moves one: (28 - 84) / 1.6 = -35. Said,
        -- and closed with a second click on MENU.
        T.pressAt(header, 6)
        T.popup(301, 490, 28, 192, 96)
        T.tick(1)
        T.ok(T.lastSaid() == "Save preset as is not available now", T.said())
        T.ok(T.count("[avenger] 'Save preset as': MENU's popup opened at (245.0,-35.0) 120.0x60.0 from "
          .. "Avenger's corner in 50 % units — not below MENU, where the table's rows are; nothing chosen") == 1)
        T.ok(T.clickAt() == "517,106", "MENU again, to close it: " .. T.clickAt())
        T.ok(T.count("[avenger] 'Save preset as': its popup was left open (unplaced) — clicking its opener again to close it") == 1)
        T.closePopups()
        T.tick(2)
        -- A tooltip-sized window first is passed over; the popup after it is chosen in.
        T.pressAt(header, 4)
        T.popup(302, 520, 120, 40, 14)
        T.tick(1)
        T.ok(T.count("which its menuItem function says is not its menu (under two rows tall)") == 1, T.dump())
        T.popup(303, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,135", T.clickAt())
    "##);
    run(r##"
        local S = T.S
        T.onWindows(80, 1)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 1.6, 80))
        T.event()
        T.pressAt(header, 4)
        -- MENU: 248 + 419.2 -> 667, 60 + 22.4 -> 82.
        T.ok(T.clickAt() == "667,82", T.clickAt())
        T.popup(300, 640, 92, 192, 96)
        T.tick(1)
        -- Load's row: 60 + 51.2 -> 111; across, 640 + 96 = 736.
        T.ok(T.clickAt() == "736,111", T.clickAt())
        local asked = S.ownsAsked[#S.ownsAsked]
        T.ok(asked[1] == 300 and asked[2] == 736 and asked[3] == 111, "asked of the popup's own window")
    "##);
}

/// The zoom stepper opens Avenger's zoom list and chooses the entry beside the zoom READ — placed by
/// the list's own height, 21 rows of 11.5 x the factor — and says the zoom the field reads once it
/// shows another: a read that still shows the old zoom is read again. The new factor places every
/// click from then on. At 200 % there is nothing above, and nothing is opened. A list too short for
/// its 21 rows scrolls: nothing is chosen in it, and it is closed with a second click on the field.
/// A press while a step is still being carried out sends nothing.
#[test]
fn the_zoom_stepper_chooses_the_entry_beside_the_zoom_read_and_says_the_new_one() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.stepAt(header, 10, 1)
        T.ok(T.clickAt() == "132,106", "the zoom field: " .. T.clickAt())
        -- Right again before the list is seen: not sent.
        local clicks = #S.clicks
        T.stepAt(header, 10, 1)
        T.ok(#S.clicks == clicks, "the field is not clicked a second time")
        -- The list below the field, 96 x 390 at (114, 114): rows of 18.4, a border of
        -- (390 - 386.4) / 2 = 1.8. Entry 8, 85 %: 114 + 1.8 + 18.4 x 7.5 = 253.8 -> 254; across, the
        -- middle, 114 + 48 = 162. The table's row would be 84 + 1.6 x 104.5 = 251.2 -> 251.
        T.popup(310, 114, 114, 96, 390)
        T.tick(1)
        T.ok(T.clickAt() == "162,254", T.clickAt())
        T.ok(T.count("[avenger] 'Zoom': 80 % to 85 %, entry 8 of the list at (10.0,18.8) 60.0x243.8 from "
          .. "Avenger's corner in 50 % units: at y 254 by the list's height, 251 by the table") == 1)
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[avenger] zoom step from 80 % chosen") == 1)
        -- Right again while the field has not been read again: not sent either.
        clicks = #S.clicks
        T.stepAt(header, 10, 1)
        T.ok(#S.clicks == clicks and T.count("the last step still waits for the zoom field — this press is not sent") == 1, T.dump())
        local r1 = T.lastRead("vps-avenger header")
        T.ok(r1 and not r1.answered, "the header is read again")
        T.answer(r1, T.header(98, 84, 1.6, 80))
        T.tick(1)
        T.ok(not string.find(T.lastSaid() or "", "Zoom", 1, true), "the old zoom is not said: " .. T.said())
        local r2 = T.lastRead("vps-avenger header")
        T.ok(r2 ~= r1 and not r2.answered, "read again")
        T.answer(r2, T.header(98, 84, 1.7, 85))
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom, 85 percent, slider", T.said())
        local says = 0
        for _, s in ipairs(S.speech) do if s.text == "Zoom, 85 percent, slider" then says += 1 end end
        T.ok(says == 1, "said once, not once per press: " .. T.said())
        -- Undo at the new factor: 98 + 1.7 x 305 = 616.5 -> 617, 84 + 23.8 -> 108.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "617,108", T.clickAt())
        -- A list that scrolls: 200 tall where 21 rows of 19.55 are 410.6. Said, and closed with the
        -- field: 98 + 1.7 x 21 = 133.7 -> 134, 84 + 23.8 -> 108.
        T.stepAt(header, 10, 1)
        T.popup(311, 114, 114, 96, 200)
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom is not available now", T.said())
        T.ok(T.count("the popup is 200 tall where the list's 21 rows are 411 — cut short, so it scrolls; nothing chosen") == 1)
        T.ok(T.clickAt() == "134,108", "the field again, to close it: " .. T.clickAt())
        T.closePopups()
        T.run(3000)
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(200)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 4, 200))
        T.event()
        T.run(400)
        local clicks = #S.clicks
        T.stepAt(header, 10, 1)
        T.tick(1)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Zoom, 200 percent, slider", "at once, nothing opened: " .. T.said())
        T.ok(T.count("[avenger] 'Zoom': no step up from 200 % — nothing opened") == 1)
    "##);
}

/// REAPER resizes its floating window to Avenger's new size after a zoom step, which on a Mac ends
/// the runtime's stay and asks `identify` again: the header of a window that was Avenger's, whose
/// popup this module opened, is still Avenger's, so the overlay stays up through its own step and
/// the stepper says the new zoom.
#[test]
fn the_overlay_stays_up_through_its_own_zoom_step_when_the_window_resizes() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.stepAt(header, 10, 1)
        T.popup(310, 114, 114, 96, 390)
        T.tick(1)
        T.closePopups()
        -- 871.25 x 1.7 = 1481.1 -> 1481 across, 561 x 1.7 = 953.7 -> 954 down: the resize comes
        -- before the menu tests have seen the list go.
        local W85 = T.float(85)
        S.listed = { W85 }
        T.show(W85)
        T.ok(header.active, "still up")
        T.ok(T.count("[avenger] 'VST3i: VPS Avenger - Track 1' (1481x954): a new stay after a popup of ours — "
          .. "still VPS Avenger; its header is read again") == 1, T.dump())
        local r = T.lastRead("vps-avenger header")
        T.ok(r and T.region(r.region) == "100,82,767,137", r and T.region(r.region))
        T.answer(r, T.header(98, 84, 1.7, 85))
        T.tick(1)
        T.ok(T.lastSaid() == "Zoom, 85 percent, slider", T.said())
        -- The step is over: a later stay asks the header as always.
        T.tick(2)
        T.show(W85, { { id = 50, class = "AXButton//", bounds = { x = 110, y = 62, w = 20, h = 16 } }, W85 })
        T.ok(not header.active)
        T.show(W85)
        T.ok(not header.active and not T.lastRead("vps-avenger header").answered, "asked afresh")
    "##);
}

/// A stay is carried over only after our own popup, at the size the header was read at, and once:
/// the redo list in front and back is Avenger's at once, and the read that stay starts confirms
/// it; the window retitled in place after that — Logic's Link mode switching the insert, say — is
/// asked afresh. A carried stay whose reads find text without the header twice in a row is
/// overturned: nothing is placed there from then on, and the next stay asks the header.
#[test]
fn a_stay_is_carried_over_once_after_our_own_popup_and_checked_by_its_read() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        local POP = { id = 77, title = "", class = "AXWindow/AXUnknown/", app = T.MAC_REAPER,
          bounds = { x = 560, y = 116, w = 200, h = 150 }, client = { x = 560, y = 116, w = 200, h = 150 } }
        T.pressAt(header, 9)
        T.popup(77, 560, 116, 200, 150)
        T.show(POP)
        T.closePopups()
        T.show(W)
        T.ok(header.active, "carried over")
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        -- Retitled in place, the keyboard still inside: a new stay, asked afresh.
        local W2 = T.float(80)
        W2.title = "VST3i: Channel EQ - Track 1"
        S.listed = { W2 }
        T.show(W2)
        T.ok(not header.active, "not carried a second time")
        T.ok(not T.lastRead("vps-avenger header").answered, "asked afresh")
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        local POP = { id = 77, title = "", class = "AXWindow/AXUnknown/", app = T.MAC_REAPER,
          bounds = { x = 560, y = 116, w = 200, h = 150 }, client = { x = 560, y = 116, w = 200, h = 150 } }
        T.pressAt(header, 9)
        T.popup(77, 560, 116, 200, 150)
        T.show(POP)
        T.closePopups()
        T.show(W)
        T.ok(header.active)
        local eq = T.reading({ { text = "Channel", x = 110, y = 90, w = 50, h = 12 }, { text = "EQ", x = 170, y = 90, w = 16, h = 12 } })
        T.answer(T.lastRead("vps-avenger header"), eq)
        T.ok(T.count("not VPS Avenger after all") == 0, "one read without the header is not enough")
        header.controls[10].text(header)
        T.answer(T.lastRead("vps-avenger header"), eq)
        T.ok(T.count("not VPS Avenger after all — neither MENU nor UNDO was read in 2 reads in a row since "
          .. "the stay was carried over") == 1, T.dump())
        local clicks = #S.clicks
        T.pressAt(header, 8)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Undo is not available now", T.said())
    "##);
}

/// A zoom field the recogniser cannot read leaves the header's own size: on a DAW's panel the step
/// it lands on is taken as the zoom, said to be seen rather than read. A header read 12 points
/// right of where the table puts it still fits its zoom, and the line says by how much — the
/// measurement a tester's log brings back. A caption misread as the step beside it ("85%" at
/// 80 %) fits no factor on a panel, where the display factor is 1 and the nearest two steps are
/// more than the 3 % apart a reading may be off by: not taken as Avenger until one fits.
#[test]
fn an_unread_zoom_field_takes_the_step_the_header_lands_on_and_the_offset_is_logged() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, false))
        T.event()
        T.ok(header.active)
        T.ok(T.count("the zoom field was not read — zoom 80 % (seen from the header's size, not read), factor 1.6000") == 1)
        header.focus = 10
        T.ok(header.controls[10].text(header) == "80 percent")
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80, { 12, 0 }))
        T.event()
        T.ok(header.active)
        T.ok(T.count("factor 1.6000, display factor 1.00. MENU read at (529,106), UNDO at (598,106): (+12.0,+0.0) "
          .. "and (+12.0,+0.0) from where the table puts them") == 1)
    "##);
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        local reading = T.header(98, 84, 1.6, 80)
        reading.words[1].text = "85%"
        T.answer(T.lastRead("vps-avenger header"), reading)
        T.event()
        T.ok(not header.active, "no factor, not taken as Avenger")
        T.ok(T.count("the header reads like VPS Avenger's, but no factor: the zoom field reads 85 %, and MENU and "
          .. "UNDO are read where a factor of 1.600 puts them, 6 % off the 1.700 that zoom gives at a display "
          .. "factor of 1.00") == 1)
    "##);
}

// ---------------------------------------------------------------------------------------------
// Avenger's warning box.
// ---------------------------------------------------------------------------------------------

/// Initialize is chosen in MENU; once the popup has taken the choice, the warning box's overlay
/// looks for the box, and only then — its poll does not even run before. Once YES and NO are read,
/// the box overlay takes the keyboard: its first stop is the box's own text, and YES is clicked
/// where it was read. When the box has gone — two readings without YES and NO — the header is
/// back, on the control the user was on.
#[test]
fn the_warning_box_is_an_overlay_of_its_own_while_it_is_up() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header, box = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.poll(500)
        T.ok(T.lastRead("vps-avenger dialog") == nil, "not looked for before it is expected")
        T.pressAt(header, 7)
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        -- Initialize's row: 84 + 1.6 x 68 = 192.8 -> 193.
        T.ok(T.clickAt() == "586,193", T.clickAt())
        T.ok(T.count("Avenger's warning box is looked for") == 0, "not before the popup has taken it")
        T.closePopups()
        T.tick(2)
        T.ok(T.count("[avenger] 'Initialize preset' chosen: Avenger's warning box is looked for, up to 8 reads") == 1)
        T.poll(500)
        local r = T.lastRead("vps-avenger dialog")
        -- The box's band, (280, 200)-(600, 336) x 1.6 from (98, 84).
        T.ok(r and T.region(r.region) == "546,404,1058,622", r and T.region(r.region))
        T.ok(header.active and not box.active)
        local function w(text, x, y, ww, hh) return { text = text, x = x, y = y, w = ww, h = hh } end
        T.answer(r, { status = "text", text = "Warning\nThis will initialize all settings\nAre you sure?\nYES NO",
          lines = {
            w("Warning", 640, 430, 90, 16), w("This will initialize all settings", 560, 460, 330, 16),
            w("Are you sure?", 630, 490, 150, 16), w("YES NO", 703, 572, 194, 17),
          },
          words = { w("Warning", 640, 430, 90, 16), w("YES", 703.6, 573, 28, 16), w("NO", 867.2, 572, 20, 16) } })
        T.ok(T.count("[avenger] 'Initialize preset': Avenger's warning box is up: \"Warning, This will "
          .. "initialize all settings, Are you sure?\"; YES read at (718,581), NO at (877,580) — (+2,+1) and "
          .. "(+0,+0) from where the table puts them") == 1)
        T.event()
        T.ok(box.active and not header.active)
        T.run(400)
        T.ok(T.lastSaid() == "Avenger asks, Warning, This will initialize all settings, Are you sure?", T.said())
        -- YES where it was read: 717.6 - 100 -> 618 and 581 - 82 = 499 into the plug-in.
        T.pressAt(box, 2)
        T.ok(T.clickAt() == "718,581" and T.lastSaid() == "Yes, activated", T.clickAt())
        -- The box has gone: one reading without it is not enough.
        T.poll(500)
        local gone = T.lastRead("vps-avenger dialog")
        T.ok(gone ~= r)
        T.answer(gone, T.noBox())
        T.ok(T.count("the warning box has gone") == 0, "one reading is not enough")
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.noBox())
        T.ok(T.count("[avenger] 'Initialize preset': the warning box has gone") == 1)
        T.event()
        T.ok(header.active and not box.active)
        T.run(400)
        T.ok(T.lastSaid() == "Initialize preset, button", "back where the user was: " .. T.said())
        local last = T.lastRead("vps-avenger dialog")
        T.poll(500)
        T.ok(T.lastRead("vps-avenger dialog") == last, "not looked for any more")
    "##);
}

/// A second box on the same window — Save's after Initialize's — starts on its own text again, not
/// on the button the first was answered with: a user who counts stops from the text lands where
/// they expect. While it is up, a reading of nothing, a reading with one of the buttons misread
/// ("N0") and one text reading without them do not give it up.
#[test]
fn a_second_warning_box_starts_on_its_text_and_is_not_given_up_on_one_miss() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header, box = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.choose(header, 7, 490, 116, 192, 96)
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.boxReading("Are you sure?"))
        T.event()
        T.run(400)
        T.ok(box.active and T.lastSaid() == "Avenger asks, Are you sure?", T.said())
        -- No, the third stop: 877.2 - 100 -> 777 and 580 - 82 = 498 into the plug-in.
        T.pressAt(box, 3)
        T.ok(T.clickAt() == "877,580", T.clickAt())
        for _ = 1, 2 do
          T.poll(500)
          T.answer(T.lastRead("vps-avenger dialog"), T.noBox())
        end
        T.event()
        T.run(400)
        T.ok(header.active and T.lastSaid() == "Initialize preset, button", T.said())
        -- Save's box: row 43.5, 84 + 69.6 -> 154.
        T.pressAt(header, 5)
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,154", T.clickAt())
        T.closePopups()
        T.tick(2)
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.boxReading("Overwrite preset?", "YES", "N0"))
        T.event()
        T.run(400)
        T.ok(box.active and box.focus == 1, "on its first stop: " .. tostring(box.focus))
        T.ok(T.lastSaid() == "Avenger asks, Overwrite preset?", T.said())
        -- Nothing read, then text without the buttons once: still up.
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), { status = "none", text = "", words = {} })
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.noBox())
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.boxReading("Overwrite preset?"))
        T.event()
        T.ok(box.active, "not given up on one miss: " .. T.dump())
        T.ok(T.count("the warning box has gone") == 1, "only the first box's")
    "##);
}

/// A box that does not come is looked for eight reads, and no more.
#[test]
fn a_warning_box_that_does_not_come_is_looked_for_eight_reads() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.pressAt(header, 5)
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        -- Save's row, 43.5 (the project's "about 43", between Load's 32 and Save as's 55): 84 + 69.6
        -- -> 154.
        T.ok(T.clickAt() == "586,154", T.clickAt())
        T.closePopups()
        T.tick(2)
        local reads = 0
        for _ = 1, 12 do
          T.poll(500)
          local r = T.lastRead("vps-avenger dialog")
          if r and not r.answered then
            reads += 1
            T.answer(r, { status = "text", text = "EDITOR", lines = {}, words = { { text = "EDITOR", x = 600, y = 420, w = 60, h = 16 } } })
          end
        end
        T.ok(reads == 8, reads)
        T.ok(T.count("[avenger] 'Save preset': no warning box in 8 reads after it was chosen — not looked for any more") == 1)
    "##);
}

/// Avenger at 200 % in Logic reaches past the bottom of the screen, and its warning box is drawn in
/// its middle: Initialize and Save would open a modal box no read and no key of ours reaches. They
/// are refused, said with the reason; Load, which opens no box, is not.
#[test]
fn initialize_and_save_are_refused_where_their_box_would_lie_off_the_screen() {
    run_mac(r##"
        local S = T.S
        local L = T.logic(200)
        S.listed = { L }
        T.at(L)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(0, 120, 4, 200))
        T.event()
        T.ok(header.active)
        local clicks = #S.clicks
        -- The box's band, (280, 200)-(600, 336) x 4 from (0, 120): down to 1464, past the 1440 of
        -- the screen.
        T.pressAt(header, 7)
        T.ok(#S.clicks == clicks, "nothing clicked")
        T.ok(T.lastSaid() == "Initialize preset: Avenger's warning box would open off the screen at 200 percent — zoom out first", T.said())
        T.ok(T.count("[avenger] 'Initialize preset' not chosen: Avenger's warning box would lie at (1120,920)-(2400,1464), past the screen's 2560x1440") == 1, T.dump())
        T.pressAt(header, 5)
        T.ok(#S.clicks == clicks)
        T.pressAt(header, 4)
        T.ok(#S.clicks == clicks + 1 and T.lastSaid() == "Load preset, activated", T.said())
    "##);
}

// ---------------------------------------------------------------------------------------------
// Calibration.
// ---------------------------------------------------------------------------------------------

/// In a calibrating run the header is photographed at each zoom and size it is read at: crosshairs
/// where the overlay clicks the zoom field, ◀, ▶, MENU and UNDO, then on the words read — the
/// origin question in one picture. The runtime's own shot marks the name's box at the scaled place,
/// and the warning box is photographed with YES and NO read and as the table has them. The
/// module's pictures go to its own folder, the runtime's to the runtime's.
#[test]
fn a_calibrating_run_draws_where_the_overlay_clicks_and_reads() {
    run_mac(r##"
        local S = T.S
        T.calibrate(true)
        rawset(T.host.element, "rawDump", function() return {} end)
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header, box = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        local shot = S.marked[1]
        T.ok(shot and shot.path == "C:/modules/vps-avenger/calibration/header-80pct-1394x898.png", shot and shot.path)
        T.ok(T.region(shot.region) == "100,82,728,134")
        local marks = {}
        for _, m in ipairs(shot.marks) do marks[#marks + 1] = m[1] .. "," .. m[2] end
        T.ok(table.concat(marks, " ") == "132,106 450,106 469,106 517,106 586,106 132,106 517,106 586,106",
          table.concat(marks, " "))
        T.ok(T.handled(header.calibrationShot, header) == true)
        T.ok(T.count("scale 1.6000 about (0,0), frame (-2,2)") == 1)
        T.ok(T.count(string.format("  %2d %-24s screen (%d,%d)  pixel %s", 1, "Preset", 296, 107, "20,20,20")
          .. "  region (152,95)-(440,119)") == 1)
        -- The controls the module clicks itself — Undo and the redo list among them, at UNDO — are in
        -- the header picture above, not in the runtime's.
        T.ok(T.count(string.format("  -- %-24s no position", "Undo")) == 1, T.dump())
        T.ok(T.count("-> C:/modules/overlay-runtime/calibration/VPS-Avenger.png") == 1, "the runtime's shot in the runtime's folder")
        -- The warning box, after Initialize.
        T.choose(header, 7, 490, 116, 192, 96)
        T.poll(500)
        local function w(text, x, y, ww, hh) return { text = text, x = x, y = y, w = ww, h = hh } end
        T.answer(T.lastRead("vps-avenger dialog"), { status = "text", text = "Are you sure?\nYES NO",
          lines = { w("Are you sure?", 630, 490, 150, 16) },
          words = { w("YES", 703.6, 573, 28, 16), w("NO", 867.2, 572, 20, 16) } })
        local last = S.marked[#S.marked]
        T.ok(last.path == "C:/modules/vps-avenger/calibration/dialog-Initialize-preset-80pct.png", last.path)
        marks = {}
        for _, m in ipairs(last.marks) do marks[#marks + 1] = m[1] .. "," .. m[2] end
        T.ok(table.concat(marks, " ") == "718,581 877,580 716,580 877,580", table.concat(marks, " "))
    "##);
}

/// One of Avenger's popups that becomes the window in front — the redo list here — is held over by
/// the overlay, and ends the runtime's stay; back in the plug-in, the new stay is Avenger's at once,
/// because nothing but our own popup was in front (its header is read all the same). Once the
/// overlay has left the front, a new stay asks the header as always.
#[test]
fn a_popup_of_ours_in_front_does_not_take_the_overlay_down_with_it() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        local reads = #S.reads
        T.pressAt(header, 9)
        T.ok(S.clicks[#S.clicks].button == "right" and T.clickAt() == "586,106", T.clickAt())
        T.ok(T.lastSaid() == "Redo list, activated", T.said())
        local POP = { id = 77, title = "", class = "AXWindow/AXUnknown/", app = T.MAC_REAPER,
          bounds = { x = 560, y = 116, w = 200, h = 150 }, client = { x = 560, y = 116, w = 200, h = 150 } }
        T.popup(77, 560, 116, 200, 150)
        T.show(POP)
        T.ok(header.active, "held over its popup")
        T.ok(T.count("came to the front after 'Redo list' was pressed and a test sees a menu") == 1)
        -- An entry chosen with the list's own keys: the list goes, and the plug-in has the keyboard.
        T.closePopups()
        T.show(W)
        T.ok(header.active, "never out of the front")
        T.ok(T.count("[avenger] 'VST3i: VPS Avenger - Track 1' (1394x898): a new stay after a popup of ours — "
          .. "still VPS Avenger; its header is read again") == 1)
        T.ok(#S.reads == reads + 1 and not T.lastRead("vps-avenger header").answered)
        T.show(T.OTHER)
        T.ok(not header.active)
        T.show(W)
        T.ok(not header.active and not T.lastRead("vps-avenger header").answered, "asked afresh")
    "##);
}

/// In REAPER's FX chain on a Mac the panel starts right of the FX list (daw-hosts: the list's edge
/// + 9, and 52 down), and Avenger's corner 2 left of it and 2 below; the clicks follow.
#[test]
fn avenger_in_a_reaper_fx_chain_on_a_mac() {
    run_mac(r##"
        local S = T.S
        local CHAIN = { id = 32, title = 'FX: Track 1 "Track 1"', class = "AXWindow/AXStandardWindow/",
          app = T.MAC_REAPER, bounds = { x = 0, y = 38, w = 1700, h = 1000 }, client = { x = 0, y = 60, w = 1700, h = 978 } }
        S.listed = { CHAIN }
        S.controls = { T.FXLIST }
        T.at(CHAIN)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        -- The panel at (8 + 222 + 9, 60 + 52) = (239, 112), 1461 x 926 — its width the larger:
        -- 657.45 -> 658 across, 53.7 -> 54 down.
        T.ok(r and T.region(r.region) == "239,112,897,166", r and T.region(r.region))
        T.answer(r, T.header(237, 114, 1.6, 80))
        T.event()
        T.ok(header.active)
        -- Undo: 237 + 488 = 725, 114 + 22.4 -> 136.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "725,136", T.clickAt())
    "##);
}

// ---------------------------------------------------------------------------------------------
// The preset database (modules/vps-avenger-presets), an optional dependency: the maintainer's
// decisions of 2026-09-28 — the module may orient itself on it — and of 2026-10-26: what it gives
// is said plainly, and "from the database" follows only a name it put where the screen's read is
// not (a read taken for a catalog name by similarity, the neighbour where the read failed or
// disagrees, the last position's name where Preset info reads nothing). Against T.FIXTURE, a
// catalog of four small expansions, never the real data.
// ---------------------------------------------------------------------------------------------

/// Not installed: the module is stage 1's — ten stops and no Preset info, and a step says the name
/// it read, as it was read.
#[test]
fn without_the_preset_database_the_module_is_stage_1s() {
    run_mac(r##"
        local S = T.S
        local header = T.avengerAt80("AR Band Pass Slide")
        local labels = {}
        for i, c in ipairs(header.controls) do labels[i] = c.label end
        T.ok(table.concat(labels, "|") == "Preset|Previous preset|Next preset|Load preset|Save preset|"
          .. "Save preset as|Initialize preset|Undo|Redo list|Zoom|" .. T.RING_LABELS, table.concat(labels, "|"))
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Bond Pess Slide") == "AR Bond Pess Slide", T.said())
        T.ok(T.step(header, 2, nil, nil) == "the preset name cannot be read now", T.said())
        T.ok(T.count("database") == 0, "nothing of a database: " .. T.dump())
    "##);
}

/// Installed but not readable — its index missing, or of another format: Preset info says so, the
/// log says why, the index is not read again, and ◀ and ▶ say what they read, as without it.
/// Nothing is read at load. The reader answers nil and an error value, `invalid`, its message the
/// sentence the log has, and the same one at every call after.
#[test]
fn a_preset_database_that_cannot_be_read_leaves_the_steps_as_they_were() {
    run_mac(r##"
        local S = T.S
        T.presets({})
        local header = T.avengerAt80("AR Band Pass Slide")
        T.ok(#header.controls == 11 + T.RING_CONTROLS and header.controls[4].label == "Preset info", tostring(#header.controls))
        T.ok(#S.presetReads == 0, "nothing read at load")
        T.ok(T.info(header, "AR Band Pass Slide") == "the preset database cannot be read", T.said())
        T.ok(T.count("[avenger] the preset database cannot be read: data/index.json cannot be read: The "
          .. "system cannot find the file specified. (os error 2) — Previous and Next work without it") == 1, T.dump())
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Bond Pess Slide") == "AR Bond Pess Slide", T.said())
        T.ok(T.step(header, 3, "AR Bond Pess Slide", nil) == "the preset name cannot be read now", T.said())
        T.ok(#S.presetReads == 1, "asked once: " .. #S.presetReads)
    "##);
    run_mac(r##"
        local S = T.S
        T.presets({ ["data/index.json"] = '{"format":2,"expansions":[]}' })
        local header = T.avengerAt80("AR Band Pass Slide")
        T.ok(T.info(header, "AR Band Pass Slide") == "the preset database cannot be read", T.said())
        T.ok(T.count("data/index.json is format 2, and this reader reads format 1") == 1, T.dump())
        local idx, err = S.presets.index()
        T.ok(idx == nil and T.is(err, "invalid", "data/index.json is format 2, and this reader reads format 1"), T.what(err))
        local x, err2 = S.presets.expansion("Alpha")
        T.ok(x == nil and rawequal(err2, err), "the index's error, the same value: " .. T.what(err2))
    "##);
}

/// The reader's own answers, as the module asks them: the index in its order, an expansion's file
/// read at the first question and kept, a category by the blocks of the list (Alpha's Pads come
/// before its Leads), and Avenger's default names for what the file leaves empty — the macros
/// counted across the tabs.
#[test]
fn the_preset_databases_reader_interprets_its_format() {
    run_mac(r##"
        local S = T.S
        local P = T.presets()
        local idx = P.index()
        T.ok(idx.exported == "2026-09-26 18:57" and #idx.expansions == 4 and idx.expansions[2].name == "Beta")
        local x = P.expansion("Alpha")
        T.ok(x and #x.presets == 6, "Alpha's file")
        T.ok(P.expansion("Alpha") == x, "kept")
        local name, k, n = P.category(x, 5)
        T.ok(name == "Pads" and k == 1 and n == 1, tostring(name))
        name, k, n = P.category(x, 2)
        T.ok(name == "Arp" and k == 2 and n == 2, tostring(name))
        local osc = P.oscillators(x, 1)
        T.ok(#osc == 3 and osc[1].name == "Arp" and not osc[1].default and osc[2].name == "OSC 2" and osc[2].default)
        local tabs = P.macros(x, 2)
        T.ok(#tabs == 2 and tabs[1].buttons[2].name == "Gate" and tabs[2].knobs[1].name == "Macro 4"
          and tabs[2].knobs[1].default and tabs[2].knobs[2].name == "Wobble" and tabs[2].buttons[1].name == "MacroBtn 3")
        local none, why = P.expansion("Omega")
        T.ok(none == nil and T.is(why, "not_found", "the index has no expansion 'Omega'"), T.what(why))
        local reads = table.concat(S.presetReads, " ")
        T.ok(reads == "data/index.json data/expansions/alpha.json", reads)
    "##);
}

/// A name read is placed in the catalog as the project's tool places it: the same words (runs of
/// spaces as one, the acute accent as an apostrophe, any case) are said in the catalog's spelling
/// and unmarked; a name only similar enough (0.72) is the database's reading, marked — looked for
/// first in the expansion Avenger was last known in, then in the whole catalog, where it also has
/// to stand out from every other name by 0.05; a name that begins with a catalog name and goes on,
/// and "Init Preset", are not in it. A name in several expansions is placed in the one Avenger was
/// last known in, and otherwise names them. Preset info says where the preset is, its oscillators
/// and its macros, plainly (the maintainer's decision of 2026-10-26).
#[test]
fn preset_info_places_the_name_read_in_the_catalog_and_says_what_the_database_has() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        local band = "AR Band Pass Slide. Expansion Alpha, category Arp, 1 of 2. 3 oscillators: Arp, OSC 2, Lead. "
          .. "Macros: Tone, Macro 2, Drive; buttons: +12, MacroBtn 2"
        T.ok(T.info(header, "AR Band Pass Slide") == band, T.said())
        T.ok(T.info(header, "ar band pass slide") == band, T.said())
        T.ok(T.info(header, "AR Bond Pess Slide") == "AR Band Pass Slide, from the database"
          .. string.sub(band, #"AR Band Pass Slide" + 1), T.said())
        T.ok(T.count("[avenger] 'Preset info': read 'AR Bond Pess Slide' — Alpha 1 of 6 (by similarity 0.89)") == 1, T.dump())
        T.ok(T.info(header, "LD Palm's Lead 1") == "LD Palm's Lead 1. Expansion Alpha, category Leads, 1 of 1. "
          .. "2 oscillators: Lead, Sub. Macros: Glide, Macro 2, Macro 3; buttons: MacroBtn 1, MacroBtn 2", T.said())
        T.ok(T.info(header, "AR Deep Arp") == "AR Deep Arp. Expansion Alpha, category Arp, 2 of 2. 1 oscillator: One Osc. "
          .. "2 macro tabs. Tab 1: Cut, Res, Env; buttons: Hold, Gate. Tab 2: Macro 4, Wobble, Macro 6; buttons: "
          .. "MacroBtn 3, MacroBtn 4", T.said())
        T.ok(T.info(header, "PD  Soft Pad") == "PD Soft Pad. Expansion Beta, category Pads, 1 of 1. 1 oscillator: Soft. "
          .. "Macros: Air, Macro 2, Macro 3; buttons: MacroBtn 1, MacroBtn 2", T.said())
        T.ok(T.starts(T.info(header, "pl üsch"), "PL Üsch. Expansion Gamma, category Plucked, 1 of 1. "), T.said())
        -- Not in the catalog: somebody's own variant, and Avenger's blank preset.
        T.ok(T.info(header, "BA Deep Sub mine") == "BA Deep Sub mine, not in the database", T.said())
        T.ok(T.count("read 'BA Deep Sub mine' — a name of its own that begins with 'BA Deep Sub'") == 1, T.dump())
        T.ok(T.info(header, "Init Preset") == "Init Preset, not in the database", T.said())
        -- In three expansions, with none known: named, not placed.
        T.ok(T.info(header, "BA Deep Sub") == "BA Deep Sub, in 3 expansions: Alpha, Beta and Gamma", T.said())
        -- Known to be in Beta: placed there.
        T.info(header, "SQ Feel It")
        T.ok(T.starts(T.info(header, "BA Deep Sub"), "BA Deep Sub. Expansion Beta, category Bass, 1 of 1. "), T.said())
        -- Nothing read: the position the last read placed — its name marked, the database's guess
        -- for the preset shown, and what it has on that preset plain.
        T.ok(T.starts(T.info(header, nil), "the preset name cannot be read now; BA Deep Sub, from the "
          .. "database. Expansion Beta, category Bass, 1 of 1. "), T.said())
        -- One case apart from Alpha's "BA Reese": exact is Gamma's; any case is both.
        T.ok(T.starts(T.info(header, "Ba Reese"), "Ba Reese. Expansion Gamma, category Bass, 1 of 2. "), T.said())
        T.info(header, "Init Preset")
        T.ok(T.info(header, "BA REESE") == "BA Reese, in 2 expansions: Alpha and Gamma", T.said())
        T.ok(T.info(header, nil) == "the preset name cannot be read now", T.said())
        -- A name the catalog does not have, as like three of its names as like each: none of them.
        T.ok(T.info(header, "LD Lead 4") == "LD Lead 4, not in the database", T.said())
        T.ok(T.count("[avenger] 'Preset info': read 'LD Lead 4' — not in the catalog: 'LD Lead 1' (0.89) is no "
          .. "closer than another name (0.89)") == 1, T.dump())
        -- Found by similarity, and in three expansions: the name marked, the expansions plain.
        T.ok(T.info(header, "BA Dcep Sub") == "BA Deep Sub, from the database, in 3 expansions: Alpha, Beta and Gamma",
          T.said())
        -- A poor read like names in two expansions: over the whole catalog, the one it is clearly
        -- closer to; in the expansion Avenger was last known in, that one's, by the threshold alone.
        T.ok(T.starts(T.info(header, "PD Sarm Pad"), "PD Warm Pad, from the database. Expansion Alpha, category Pads, "), T.said())
        T.info(header, "SQ Feel It")
        T.ok(T.starts(T.info(header, "PD Sarm Pad"), "PD Soft Pad, from the database. Expansion Beta, category Pads, "), T.said())
        -- Each expansion's file read once, at its first question.
        local reads = table.concat(S.presetReads, " ")
        T.ok(reads == "data/index.json data/expansions/alpha.json data/expansions/beta.json data/expansions/gamma.json", reads)
    "##);
    run_mac(r##"
        local S = T.S
        -- Gamma's file missing: where the index puts the preset, and no more.
        local files = {}
        for k, v in pairs(T.FIXTURE) do if k ~= "data/expansions/gamma.json" then files[k] = v end end
        T.presets(files)
        local header = T.avengerAt80("PL Üsch")
        T.ok(T.info(header, "PL Üsch") == "PL Üsch. Expansion Gamma; its categories, oscillators and macros cannot be "
          .. "read from it", T.said())
        T.ok(T.count("[avenger] 'Preset info': Gamma's file cannot be read: data/expansions/gamma.json cannot be read") == 1, T.dump())
    "##);
    run_mac(r##"
        -- A name in seven expansions: the first five named, and how many more.
        local rows = {}
        for k = 1, 7 do
          rows[k] = string.format('{"name":"E%d","file":"data/expansions/e%d.json","presets":["PD Everywhere"]}', k, k)
        end
        T.presets({ ["data/index.json"] = '{"format":1,"exported":"2026-09-26 18:57","factoryStandard":[],'
          .. '"expansions":[' .. table.concat(rows, ",") .. ']}' })
        local header = T.avengerAt80("PD Everywhere")
        T.ok(T.info(header, "PD Everywhere") == "PD Everywhere, in 7 expansions: E1, E2, E3, E4, E5 and 2 more", T.said())
    "##);
}

/// The maintainer's decision of 2026-10-26, in place of 2026-09-28's mark on everything not read
/// off the screen: what the database has on a preset placed by its words is said with no "from
/// the database" anywhere in a Preset info — its expansion, category, oscillators and macros, one
/// macro tab or two, and the expansions a name is in. The mark stays after each name the database
/// put where the screen's read is not, once per such name: a read taken for a catalog name by
/// similarity, the last position's name where Preset info reads nothing, the neighbour where a
/// step reads nothing after its click, and the neighbour beside a read that disagrees with it.
#[test]
fn the_databases_values_are_said_plainly_and_only_its_guesses_are_marked() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        local function marks(said)
          local n = 0
          for _ in string.gmatch(said, "from the database") do n += 1 end
          return n
        end
        -- Read by their words: no mark, with one macro tab or two, and in several expansions.
        for _, name in ipairs({ "AR Band Pass Slide", "AR Deep Arp", "LD Palm's Lead 1", "PD Soft Pad", "PL Üsch" }) do
          local said = T.info(header, name)
          T.ok(marks(said) == 0 and string.find(said, ". Expansion ", 1, true) ~= nil
            and string.find(said, " oscillator", 1, true) ~= nil, said)
        end
        T.info(header, "Init Preset")
        local several = T.info(header, "BA Deep Sub")
        T.ok(several == "BA Deep Sub, in 3 expansions: Alpha, Beta and Gamma", several)
        -- A read the database took for a catalog name by similarity: that name marked, once.
        local fuzzy = T.info(header, "AR Bond Pess Slide")
        T.ok(marks(fuzzy) == 1 and T.starts(fuzzy, "AR Band Pass Slide, from the database. Expansion Alpha, "), fuzzy)
        T.info(header, "Init Preset")
        several = T.info(header, "BA Dcep Sub")
        T.ok(marks(several) == 1 and T.starts(several, "BA Deep Sub, from the database, in 3 expansions: "), several)
        -- Nothing read: the last position's name marked, once, and the rest plain.
        T.info(header, "AR Deep Arp")
        local kept = T.info(header, nil)
        T.ok(marks(kept) == 1 and T.starts(kept, "the preset name cannot be read now; AR Deep Arp, from the database. "
          .. "Expansion Alpha, category Arp, 2 of 2. 1 oscillator: One Osc. 2 macro tabs. Tab 1: "), kept)
        -- ◀ and ▶: the neighbour read as it stands, plain; the neighbour where nothing was read
        -- after the click, and beside a read that disagrees, marked.
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Deep Arp") == "AR Deep Arp", T.said())
        T.ok(T.step(header, 3, "AR Deep Arp", nil) == "BA Deep Sub, from the database", T.said())
        T.ok(T.step(header, 3, "PD Warm Pad", "PD Soft Pad") == "PD Soft Pad; LD Palm's Lead 1, from the database", T.said())
    "##);
}

/// ◀ and ▶ with the database: the name read just before the click places the preset, and the
/// database's neighbour in its list is what the read-back is compared with. The read-back is said
/// when it is the neighbour (marked when only similar); the neighbour, marked, when nothing could be
/// read — and the next step goes on from it; both when they disagree, the screen's first — and the
/// position is then where the screen says. A read that is another catalog name disagrees, however
/// similar it is to the neighbour (a numbered series). At an expansion's first or last preset there
/// is no neighbour: Avenger goes on into the user's own next expansion, which the catalog cannot
/// know. Nothing is said before the watch is over, one thing per step.
#[test]
fn previous_and_next_say_what_was_read_and_the_databases_neighbour_marked_when_it_differs() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        -- The same words as the neighbour: said, unmarked.
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Deep Arp") == "AR Deep Arp", T.said())
        T.ok(T.count("[avenger] 'Next preset': the database's neighbour: 'AR Deep Arp' (Alpha 2 of 6)") == 1, T.dump())
        -- Similar to the neighbour: the neighbour's spelling, marked.
        T.ok(T.step(header, 3, "AR Deep Arp", "BA Dcep Sub") == "BA Deep Sub, from the database", T.said())
        T.ok(T.count("[avenger] 'Next preset': read 'BA Dcep Sub', the database's neighbour by similarity 0.91") == 1, T.dump())
        -- Nothing read, before or after: the neighbour of the last position, marked.
        T.ok(T.step(header, 3, nil, nil) == "BA Reese, from the database", T.said())
        -- ... and the step after that goes on from it.
        T.ok(T.step(header, 3, nil, "PD Warm Pad") == "PD Warm Pad", T.said())
        -- Another name read (Avenger's own search open, say): the screen's name, then the database's.
        T.ok(T.step(header, 3, "PD Warm Pad", "PD Soft Pad") == "PD Soft Pad; LD Palm's Lead 1, from the database", T.said())
        -- The screen wins: the position is Beta's PD Soft Pad now.
        T.ok(T.step(header, 2, "PD Soft Pad", "SQ Feel It") == "SQ Feel It", T.said())
        T.ok(T.step(header, 2, "SQ Feel It", "BA Deep Sub") == "BA Deep Sub", T.said())
        -- Beta's first preset: no neighbour before it, so the step is as without the database.
        T.ok(T.step(header, 2, "BA Deep Sub", "My Own Preset") == "My Own Preset", T.said())
        T.ok(T.count("the database's neighbour: none, BA Deep Sub is at the start of its expansion") == 1, T.dump())
        -- Unchanged on screen: said so, and the neighbour the database expected.
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Band Pass Slide")
          == "AR Band Pass Slide, unchanged; AR Deep Arp, from the database", T.said())
        -- A name the catalog does not have: as read, and the neighbour.
        T.ok(T.step(header, 3, "AR Band Pass Slide", "Zzz Custom Thing") == "Zzz Custom Thing; AR Deep Arp, from the database", T.said())
        -- ... and with no position after it, the next step has no neighbour to add.
        T.ok(T.step(header, 3, nil, nil) == "the preset name cannot be read now", T.said())
        -- Another catalog name, however similar to the neighbour, is that name: in a numbered
        -- series, LD Lead 3 read where LD Lead 1 was expected.
        T.ok(T.step(header, 2, "LD Lead 2", "LD Lead 3") == "LD Lead 3; LD Lead 1, from the database", T.said())
        -- A poor read of the neighbour is the neighbour, marked.
        T.ok(T.step(header, 2, "LD Lead 3", "LD Laed 2") == "LD Lead 2, from the database", T.said())
    "##);
}

/// Where the screen and the database's neighbour disagree, the screen's words are said: a name read
/// unchanged is not the neighbour, however like it; nor is a poor read closer to another name than
/// to the neighbour, or somebody's own variant of the neighbour's name — and a read the catalog has
/// only by similarity is said as read then, with the neighbour after it, and places nothing. At an
/// expansion's last preset there is no neighbour, and a step from there that reads nothing leaves
/// no position behind; read unchanged, the position stays.
#[test]
fn a_step_says_the_screens_words_where_they_disagree_with_the_databases_neighbour() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("LD Laed 1")
        -- A swallowed click, the name read poorly the same way before and after: placed on LD Lead 1
        -- before the click, and the screen then says it did not move — LD Lead 2, as like the read
        -- as it is, is only what the database expected.
        T.ok(T.step(header, 3, "LD Laed 1", "LD Laed 1") == "LD Laed 1, unchanged; LD Lead 2, from the database", T.said())
        T.ok(T.count("[avenger] 'Next preset': read 'LD Laed 1', as before the click — Delta 1 of 3 (by similarity "
          .. "0.89, said as read); not the database's neighbour 'LD Lead 2'") == 1, T.dump())
        -- Even when the unchanged read is as like the neighbour as like the name it was placed on
        -- (in the known expansion the first of equals, LD Lead 1).
        T.ok(T.step(header, 3, "LD Lead 4", "LD Lead 4") == "LD Lead 4, unchanged; LD Lead 2, from the database", T.said())
        -- The name after the neighbour, read poorly: closer to LD Lead 3 than to the expected LD Lead
        -- 1 — as read, then the neighbour; and no position after it.
        T.ok(T.step(header, 2, "LD Lead 2", "LD Laed 3") == "LD Laed 3; LD Lead 1, from the database", T.said())
        T.ok(T.step(header, 2, nil, nil) == "the preset name cannot be read now", T.said())
        -- Somebody's own variant of the neighbour's name is not the neighbour.
        T.ok(T.step(header, 3, "AR Deep Arp", "BA Deep Sub mine") == "BA Deep Sub mine; BA Deep Sub, from the database", T.said())
        -- Alpha's last preset: no neighbour after it, and a click from there that reads nothing
        -- leaves no position — Avenger is in an expansion the catalog cannot name.
        T.ok(T.step(header, 3, "LD Palm's Lead 1", nil) == "the preset name cannot be read now", T.said())
        T.ok(T.count("the database's neighbour: none, LD Palm's Lead 1 is at the end of its expansion") == 1, T.dump())
        T.ok(T.step(header, 2, nil, nil) == "the preset name cannot be read now", T.said())
        -- Read unchanged there, it stays where it was: the step back has its neighbour.
        T.ok(T.step(header, 3, "LD Palm's Lead 1", "LD Palm's Lead 1") == "LD Palm's Lead 1, unchanged", T.said())
        T.ok(T.step(header, 2, nil, "PD Warm Pad") == "PD Warm Pad", T.said())
        T.ok(T.count("read 'PD Warm Pad', the database's neighbour exactly") == 1, T.dump())
    "##);
}

/// The position is forgotten whenever the preset may have changed where the module does not read
/// it: a MENU item chosen, the redo list opened, the overlay leaving the front. A step overtaken
/// by another press decides nothing: its neighbour is never said, and the position is the later
/// step's — and a press made before the step before it has seen its name change has no neighbour
/// when it reads the name that step started from. After Save preset as and Load preset the name
/// is the user's own until a step shows another, and is not looked for by similarity.
#[test]
fn the_databases_position_is_forgotten_when_the_preset_may_have_changed() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header, box, W = T.avengerAt80("AR Band Pass Slide")
        T.info(header, "AR Deep Arp")
        T.choose(header, 5, 490, 116, 192, 96)
        T.ok(T.count("[avenger] the database's position (Alpha 2 of 6) is forgotten: Load preset chosen") == 1, T.dump())
        T.ok(T.info(header, nil) == "the preset name cannot be read now", T.said())
        T.info(header, "AR Deep Arp")
        T.pressAt(header, 10)
        T.ok(T.lastSaid() == "Redo list, activated", T.said())
        T.ok(T.count("is forgotten: the redo list opened") == 1, T.dump())
        T.run(3000)
        T.info(header, "AR Deep Arp")
        T.show(T.OTHER)
        T.ok(not header.active)
        T.ok(T.count("is forgotten: the overlay left the front") == 1, T.dump())
        T.show(W)
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.ok(header.active)
        T.ok(T.info(header, nil) == "the preset name cannot be read now", T.said())
    "##);
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        -- ▶ twice before the first name is read back: the first step's neighbour is AR Deep Arp,
        -- the second's BA Deep Sub.
        T.nameIs("AR Band Pass Slide")
        T.pressAt(header, 3)
        T.run(150)
        T.nameIs("AR Deep Arp")
        T.pressAt(header, 3)
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, T.nameRead("BA Deep Sub")) end
        end
        T.ok(string.find(T.said(), "BA Deep Sub", 1, true) ~= nil, T.said())
        T.ok(string.find(T.said(), "AR Deep Arp, from the database", 1, true) == nil, "the overtaken step's neighbour: " .. T.said())
        T.ok(T.starts(T.info(header, nil), "the preset name cannot be read now; BA Deep Sub, from the database. Expansion Alpha, "), T.said())
    "##);
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        -- ▶ twice before the name has changed on screen, the second press reading the name from
        -- before the first click: a neighbour of that would be one preset behind, so there is none,
        -- and the step says what it reads.
        T.nameIs("AR Band Pass Slide")
        T.pressAt(header, 3)
        T.run(150)
        T.pressAt(header, 3)
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, T.nameRead("BA Deep Sub")) end
        end
        T.ok(T.count("[avenger] 'Next preset': the step before has not settled, and the name may still be the one "
          .. "from before its click — no neighbour") == 1, T.dump())
        T.ok(string.find(T.said(), "BA Deep Sub", 1, true) ~= nil, T.said())
        T.ok(string.find(T.said(), "from the database", 1, true) == nil, T.said())
        -- Settled, the next step has its neighbour again.
        T.ok(T.step(header, 3, "AR Deep Arp", "BA Deep Sub") == "BA Deep Sub", T.said())
        T.ok(T.count("read 'BA Deep Sub', the database's neighbour exactly") == 1, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        -- After Save preset as, Avenger shows the name the user typed: a catalog name like it is a
        -- coincidence, so it is looked up as the same words only — until a step shows another name.
        T.choose(header, 7, 490, 116, 192, 96)
        T.ok(T.info(header, "AR Bond Pess Slide") == "AR Bond Pess Slide, not in the database", T.said())
        T.ok(T.count("read 'AR Bond Pess Slide' — not in the catalog as it stands — after Load or Save as, not "
          .. "looked for by similarity") == 1, T.dump())
        T.ok(T.step(header, 3, "AR Bond Pess Slide", "AR Bond Pess Slide") == "AR Bond Pess Slide, unchanged", T.said())
        T.ok(T.step(header, 3, "AR Bond Pess Slide", "AR Deap Arp") == "AR Deep Arp, from the database", T.said())
        T.ok(T.starts(T.info(header, "AR Bond Pess Slide"), "AR Band Pass Slide, from the database. Expansion Alpha, "), T.said())
        -- Load preset alike.
        T.choose(header, 5, 490, 116, 192, 96)
        T.ok(T.count("is forgotten: Load preset chosen") == 1, T.dump())
        T.ok(T.info(header, "AR Bond Pess Slide") == "AR Bond Pess Slide, not in the database", T.said())
    "##);
}

/// A step waits for the preset's name before its click, and only its module waits: another program
/// brought to the front meanwhile takes the press away — nothing is clicked or said in it, and the
/// log says why.
#[test]
fn a_step_whose_window_left_the_front_during_its_name_read_clicks_nothing() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        local said, clicks = #S.speech, #S.clicks
        S.ocr = function(g)
          if T.region(g) ~= "152,95,440,119" then return "" end
          -- Another program comes to the front while the name is being read.
          S.front, S.chain = T.OTHER, { T.OTHER }
          T.turn()
          return "Init Preset"
        end
        T.pressAt(header, 3)
        T.ok(#S.clicks == clicks, "a click in the other program: " .. tostring(T.clickAt()))
        T.ok(#S.speech == said, "something said: " .. T.said(said + 1))
        T.ok(T.count("[avenger] 'Next preset': not going on after the name was read — the overlay is on another "
          .. "window now") == 1, T.dump())
    "##);
}

// ---------------------------------------------------------------------------------------------
// ◀ and ▶ on Init Preset (the first Mac test of the page, 2026-10-14: TODO.md, item 3).
// ---------------------------------------------------------------------------------------------

/// Avenger steps no preset from its blank preset, and the avenger_control project's tool clicked
/// and waited for a change that never came. Where the latest name read is Init Preset — here the
/// Preset read-out's, on arrival — ◀ and ▶ say they are unavailable as Tab reaches them (their
/// `text`), and a press reads the name, clicks nothing, watches nothing and says why; a press whose
/// read gets nothing goes by the latest name read. The press's own read of a real name — a preset
/// loaded some other way — makes the step work again, and from then on neither says it is
/// unavailable. A choice from MENU and the overlay leaving the front forget the claim, since the
/// preset may have changed with nothing read since: Tab onto ▶ claims nothing, and its press reads
/// again — Init Preset still, after a Load whose file dialog was cancelled.
#[test]
fn previous_and_next_are_unavailable_on_init_preset_and_a_press_clicks_nothing() {
    run_mac(r##"
        local S = T.S
        local refused = function(label)
          return label .. ": Avenger steps no preset from Init Preset — load a preset first"
        end
        local header, _, W = T.avengerAt80("Init Preset")
        -- A Tab from the control labelled `from`, and the label it lands on, announced.
        local function tabFrom(from)
          for i, c in ipairs(header.controls) do
            if c.label == from then header.focus = i end
          end
          T.tab()
          return header.controls[header.focus].label
        end
        T.ok(T.lastSaid() == "Preset, Init Preset", T.said())
        T.ok(tabFrom("Preset") == "Previous preset"
          and T.lastSaid() == "Previous preset, unavailable on Init Preset, button", T.said())
        T.ok(tabFrom("Previous preset") == "Next preset"
          and T.lastSaid() == "Next preset, unavailable on Init Preset, button", T.said())
        local clicks, reads = #S.clicks, #S.reads
        T.pressAt(header, 2)
        T.ok(T.lastSaid() == refused("Previous preset"), T.said())
        T.pressAt(header, 3)
        T.ok(T.lastSaid() == refused("Next preset"), T.said())
        T.run(600)
        T.ok(#S.clicks == clicks and #S.reads == reads, "nothing clicked, nothing watched")
        T.ok(T.count("[avenger] 'Next preset': the name reads Init Preset, which Avenger steps no preset from — "
          .. "nothing clicked") == 1, T.dump())
        -- A read that gets nothing: the latest name read stands.
        T.nameIs(nil)
        T.pressAt(header, 3)
        T.ok(#S.clicks == clicks and T.lastSaid() == refused("Next preset"), T.said())
        T.ok(T.count("[avenger] 'Next preset': the name read last was Init Preset") == 1, T.dump())
        -- A preset loaded some other way: the press's own read says so, and the step works again.
        T.nameIs("LD Lead 1")
        T.pressAt(header, 3)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "469,106", "▶ clicked: " .. T.clickAt())
        T.nameReads("LD Lead 2")
        T.ok(T.lastSaid() == "LD Lead 2", T.said())
        T.ok(tabFrom("Preset") == "Previous preset" and T.lastSaid() == "Previous preset, button", T.said())
        T.ok(tabFrom("Previous preset") == "Next preset" and T.lastSaid() == "Next preset, button", T.said())
        -- Back on Init Preset, read by a press; then Load chosen from MENU, its file dialog cancelled:
        -- nothing read since, so nothing claimed, and the next press reads it again.
        T.nameIs("Init Preset")
        T.pressAt(header, 3)
        T.ok(T.lastSaid() == refused("Next preset"), T.said())
        T.choose(header, 4, 490, 116, 192, 96)
        -- Chosen at its click, not followed (main.luau, ONE_CLICK).
        T.ok(T.count("[menu item] 'VPS Avenger': 'Load preset' — test ") == 1, T.dump())
        T.ok(tabFrom("Previous preset") == "Next preset" and T.lastSaid() == "Next preset, button", T.said())
        clicks = #S.clicks
        T.pressAt(header, 3)
        T.ok(#S.clicks == clicks and T.lastSaid() == refused("Next preset"), T.said())
        T.ok(tabFrom("Preset") == "Previous preset"
          and T.lastSaid() == "Previous preset, unavailable on Init Preset, button", T.said())
        -- Away and back: the claim does not outlive the stay it was read in.
        T.show(T.OTHER)
        T.ok(not header.active, T.dump())
        T.show(W)
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.ok(header.active, T.dump())
        T.ok(tabFrom("Preset") == "Previous preset" and T.lastSaid() == "Previous preset, button", T.said())
    "##);
}

/// Where the preset may have changed with nothing read since, the claim of Init Preset is
/// forgotten, as the database's position is: after Undo — UNDO clicked where a hotspot would click
/// it, and said as one says its click — and after the redo list. And the Preset read-out's answer
/// that comes once the overlay has left the front is no read of the stay it was asked in: it
/// claims nothing for the next one.
#[test]
fn undo_the_redo_list_and_a_late_read_out_answer_leave_no_claim_of_init_preset() {
    run_mac(r##"
        local S = T.S
        local header, _, W = T.avengerAt80("Init Preset")
        local function tabFrom(from)
          for i, c in ipairs(header.controls) do
            if c.label == from then header.focus = i end
          end
          T.tab()
          return header.controls[header.focus].label
        end
        -- What Tab onto ◀ says.
        local function previous()
          T.ok(tabFrom("Preset") == "Previous preset", "Tab from Preset")
          return T.lastSaid()
        end
        T.ok(previous() == "Previous preset, unavailable on Init Preset, button", T.said())
        local clicks = #S.clicks
        T.pressAt(header, 8)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "586,106" and S.clicks[#S.clicks].button == nil,
          "UNDO: " .. T.clickAt())
        T.ok(T.lastSaid() == "Undo, activated" and T.count("[avenger] 'Undo': clicking (586,106)") == 1, T.said())
        T.ok(previous() == "Previous preset, button", T.said())
        -- Init Preset read again by a press, then the redo list opened and closed.
        T.pressAt(header, 3)
        T.ok(previous() == "Previous preset, unavailable on Init Preset, button", T.said())
        T.pressAt(header, 9)
        T.ok(T.clickAt() == "586,106" and S.clicks[#S.clicks].button == "right", "UNDO, right: " .. T.clickAt())
        T.ok(T.lastSaid() == "Redo list, activated", T.said())
        T.popup(320, 586, 110, 150, 200)
        T.tick(1)
        T.closePopups()
        T.tick(2)
        T.ok(previous() == "Previous preset, button", T.said())
        -- Tab onto Preset asks its read; the overlay leaves the front before the answer, which
        -- reads Init Preset. Back, with the name unreadable, nothing is claimed.
        T.ok(tabFrom("Macro button 2") == "Preset", "Tab from the last control")
        T.show(T.OTHER)
        T.runDue()
        T.ok(not header.active, T.dump())
        T.nameIs(nil)
        T.show(W)
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.ok(header.active and T.lastSaid() == "Preset, cannot be read now", T.said())
        T.ok(previous() == "Previous preset, button", T.said())
    "##);
}

// ---------------------------------------------------------------------------------------------
// From the avenger_control project into stage 1 (the port's plan, slice 0): its reading of what
// a recogniser makes of the blank preset, and Ableton Live on a Mac as a daw-hosts entry.
// ---------------------------------------------------------------------------------------------

/// The project's `_looks_like_init_preset_ocr`, as `read/ocr.luau` ports it: its own list of
/// readings, and a reading with "preset" or "presst" in it that is 0.84 alike to "init preset" —
/// alike as Python's difflib measures it, whose ratios for these readings are pinned here. A
/// reading without either word is not taken however alike it is ("Init Pre5et"), as the project
/// has it, nor a name of the user's own with "Preset" in it.
#[test]
fn the_projects_readings_of_the_blank_preset_are_taken_for_it() {
    run_mac(r##"
        local S = T.S
        local R = T.source("modules/vps-avenger/src/read/ocr.luau")(T.host)
        for _, t in ipairs({ "Init Preset", "init preset", "INIT PRESET", "  Init   Preset ", "hit Presst", "hit Preset",
          "lnit Preset", "lnit Presst", "Init Presst", "Init", "Init Preset.", "Init Preset 2", "Inlt Preset", "Unit Preset" }) do
          T.ok(R.isInitPreset(t), "taken: " .. t)
          local name, init = R.presetName(t)
          T.ok(name == "Init Preset" and init == true, t)
        end
        for _, t in ipairs({ "", "BA Deep Sub", "Pad Preset", "LD Init Lead", "Initial", "Presets", "My Preset 1",
          "Init Pre5et", "Iinit Presest" }) do
          T.ok(not R.isInitPreset(t), "not taken: " .. t)
          T.ok(R.presetName(t) == t, "as it is: " .. t)
        end
        -- difflib's ratios, as Python 3 computes them for these.
        for _, c in ipairs({ { "hit presst", 0.762 }, { "hit preset", 0.857 }, { "lnit presst", 0.818 },
          { "init preset 2", 0.917 }, { "pad preset", 0.667 }, { "my preset 1", 0.636 }, { "init", 0.533 } }) do
          local r = R.ratio(c[1], "init preset")
          T.ok(math.abs(r - c[2]) < 0.001, c[1] .. ": " .. r)
        end
    "##);
}

/// In the ring: the Preset read-out says a reading of the blank preset as "Init Preset" — its
/// `[read]` line keeps what was read — and ◀ and ▶ read the name the same way, before the click
/// and after it: "hit Presst" before is Init Preset, which Avenger steps no preset from, so ▶
/// clicks nothing; "lnit Preset" read after a click is said as Init Preset. The log says each
/// reading taken so.
#[test]
fn the_ring_says_a_reading_of_the_blank_preset_as_init_preset() {
    run_mac(r##"
        local S = T.S
        local header = T.avengerAt80("hit Presst")
        T.ok(T.lastSaid() == "Preset, Init Preset", T.said())
        T.ok(T.count("[read] 'Preset' = \"hit Presst\", said as \"Init Preset\" (region 34,7 180x15") == 1, T.dump())
        local clicks = #S.clicks
        T.ok(T.step(header, 3, "hit Presst", nil) == "Next preset: Avenger steps no preset from Init Preset — load a "
          .. "preset first", T.said())
        T.ok(#S.clicks == clicks, "nothing clicked")
        T.ok(T.count("[avenger] the name read 'hit Presst' is taken as Avenger's blank preset, Init Preset") >= 1, T.dump())
        T.ok(T.step(header, 3, "BA Deep Sub", "lnit Preset") == "Init Preset", T.said())
        T.ok(T.count("[avenger] the name read 'lnit Preset' is taken as Avenger's blank preset, Init Preset") >= 1, T.dump())
    "##);
}

/// With the preset database, a reading of the blank preset is Avenger's blank preset, which is in
/// no expansion: Preset info says "Init Preset, not in the database", the log says why, and a step
/// from it clicks nothing and asks the database for no neighbour.
#[test]
fn the_preset_database_takes_a_reading_of_the_blank_preset_for_no_catalog_name() {
    run_mac(r##"
        local S = T.S
        T.presets()
        local header = T.avengerAt80("hit Presst")
        T.ok(T.info(header, "hit Presst") == "Init Preset, not in the database", T.said())
        T.ok(T.count("[avenger] 'Preset info': read 'Init Preset' — Avenger's blank preset") == 1, T.dump())
        local clicks = #S.clicks
        T.ok(T.step(header, 3, "Init Presst", "BA Deep Sub") == "Next preset: Avenger steps no preset from Init "
          .. "Preset — load a preset first", T.said())
        T.ok(#S.clicks == clicks and T.count("the database's neighbour") == 0, T.dump())
    "##);
}

/// Ableton Live on a Mac, by daw-hosts' new entry: a titled window of `com.ableton.live` — its
/// title says nothing that matters, and its shape is not known — with the plug-in 16 below the
/// window's top, from its left edge (the project's 18 and -2, less the difference VPS Avenger keeps
/// on its own side). Its content origin taken here as its frame, as Logic's is. Avenger is read
/// and clicked there as anywhere: its corner at the frame + (-2, 18), where the project's tables
/// start in Live.
#[test]
fn avenger_in_an_ableton_live_window_on_a_mac() {
    run_mac(r##"
        local S = T.S
        local LIVE = { pid = 7000, exe = "Live", name = "Live", bundleId = "com.ableton.live" }
        local w, h = T.size(80)
        local L = { id = 51, title = "VPS Avenger/1-MIDI", class = "AXWindow/AXStandardWindow/", app = LIVE,
          bounds = { x = 300, y = 200, w = w, h = h + 16 }, client = { x = 300, y = 200, w = w, h = h + 16 } }
        S.listed = { L }
        T.at(L)
        local header = T.avenger()
        local r = T.lastRead("vps-avenger header")
        -- The panel at the frame + (0, 16) = (300, 216), 1394 x 898: 627.4 -> 628 across, 51.2 -> 52 down.
        T.ok(r and T.region(r.region) == "300,216,928,268", r and T.region(r.region))
        T.answer(r, T.header(298, 218, 1.6, 80))
        T.event()
        T.ok(header.active, T.dump())
        T.ok(T.count("panel of 'VPS Avenger/1-MIDI' id=51 at 0,16 1394x898 from its content origin, identify=true") == 1, T.dump())
        T.ok(T.count("(+0.0,+0.0) and (+0.0,+0.0) from where the table puts them with Avenger's corner at the panel's -2,+2") == 1, T.dump())
        -- Undo: 298 + 488 = 786, 218 + 22.4 -> 240.
        T.pressAt(header, 8)
        T.ok(T.clickAt() == "786,240", T.clickAt())
    "##);
}
