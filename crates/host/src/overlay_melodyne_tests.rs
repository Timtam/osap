//! Melodyne's overlay module (`modules/melodyne`), loaded for real against the scripted host of
//! `overlay_menu_tests.rs` with the window prelude's real matcher, as `overlay_avenger_tests.rs`
//! loads VPS Avenger.
//!
//! Written for step B4 of the plan that gives every module a mailbox (b0-final.md; built
//! 2026-10-05): Melodyne's two polls read with a callback. Since every handler waits, a poll that waited for
//! its read kept Melodyne's own keys waiting behind it — the read-out watcher asks eight times a
//! second, and every one of its reads held them for as long as it took. Now the tick ends at once
//! and the answer is a handler of its own. What the user hears is meant to be the same, so
//! the scenarios hold the module to its sentences and to the checks an answer that comes later
//! needs: the watcher says a box that changed and not one that did not; one read is out at a time;
//! an answer from before the overlay left and came back is dropped and is no baseline; one that
//! comes while a menu is open is dropped; one asked before a tool switch's hold lifted is held
//! even when it comes after, and the first one asked after the switch is the new tool's baseline,
//! said nothing; one that failed changes nothing. And the note-area measurement says
//! a move once the time signature is read — at once within an input epoch that read it — and not
//! about a Melodyne that has left the front; a time-signature read that failed or never answered
//! is asked again rather than taken as read.
//!
//! Then for the maintainer's decisions of 2026-10-05: the analysis read-out is back — Melodyne's
//! progress disc found from pixels of a picture taken off the event loop, its pass name read with a
//! callback, "Busy: <pass>" after the overlay's own arrival sentence, "Analysis finished" once, and
//! the read-out on Alt+A — and the note area says nothing while the disc is up. Its frames are
//! pictures taken off the event loop too, compared only within a stay and only while the gate is
//! open, and the read-outs' silence belongs to a stay. The watcher reads the boxes from one picture
//! taken off the event loop — their borders, what each holds and the text — and whether they are
//! silent is that picture's to say: an empty read of a box with a value is not silence, and boxes
//! that are empty, hold a dash or are not drawn are.
//!
//! Then for the live session of 2026-10-05 (2026-10-06): the pass name is read in English — the
//! user's language where English is not read — from two crops inside the disc's rim, about the
//! centre its rim puts it at, and taken only once two looks in a row have read the same name in
//! both, in letters and spaces, accented ones too — never a misreading, and a pass that follows
//! another at its second look — and "Busy" waits for it. A move in the note area is measured from
//! the two halves of one frame, at half the peak of their change — told apart from the note's width
//! by the note's body between them, in its colour on its own rows — and called by its size: whole
//! beats when the time signature is read, else his own steps, kept as parts of the bar so that they
//! follow the zoom; a nudge is a fine step, and what the pictures cannot size is said by its
//! direction alone, or as "moved". A frame with half a move is compared again with the next, one
//! whose cursor moved is no edit — bar lines that moved with a scroll are no cursor — and a picture
//! of the note area in which nothing changed is not profiled.
//!
//! Then for the maintainer's decision of 2026-10-09: the note area's profile is reduced off the
//! event loop — `host.screen.profile` given a callback, under the frame's key, from the picture's
//! callback — and the frame is measured when it answers, its checks asked again; one frame is out
//! at a time, from its picture to its profile (`host.screen.pending`).
//!
//! The scenarios answer the module's reads by hand, with what Melodyne would show, and hand the
//! note-area measurement column profiles made up for it (`T.frame`) and pictures with or without
//! the disc and with the read-out boxes as Melodyne draws them (`T.picture`, `T.strip`): what they
//! check is the module's decisions, not Melodyne.

use std::path::PathBuf;

use mlua::{Function, Lua, Table};

use super::overlay_host_panel_tests::{HP, WINDOW_PRELUDE};
use super::overlay_menu_tests::{finish, harness, RUNTIME};

/// The helpers, added on top of the host-panel ones.
const MEL: &str = r##"
local S = T.S
S.profiles = 0
S.inputEpoch = 0
S.rightBox = false

-- Melodyne's editor window, standalone: its client area 1000 x 700 at (8, 31).
T.MEL = { id = 300, title = "Untitled - Melodyne", class = "GNWindowDoc",
  app = { pid = 6100, exe = "Melodyne.exe", name = "Melodyne" },
  bounds = { x = 0, y = 0, w = 1016, h = 739 }, client = { x = 8, y = 31, w = 1000, h = 700 } }

S.disc = false
S.snapKeys = {}
S.changeWaits = {}
S.reductions = {}
S.rowProfiles = 0
S.leftBox = true
S.shows = { left = "value", right = "value" }
S.inkProfiles = 0

-- What the read-out boxes show: "value", "dash" or "empty", the left box and the right one (the
-- left one's when only one is given).
function T.shows(left, right)
  S.shows = { left = left, right = right or left }
end

-- The read-out strip as Melodyne draws it, at a point of the client area (nil outside it): the
-- bare tool bar 191, and each box that is drawn — the left one with S.leftBox, the right one with
-- S.rightBox — a border of 157 round a fill of 194, holding what S.shows says: a value, black in
-- rows 64-72; a dash, one row of 94; or nothing. As calibration/Melodyne-clean.png and
-- Melodyne-2/3-clean.png show them.
function T.strip(cx, cy)
  if cy < 60 or cy >= 76 or cx < 212 or cx >= 380 then return nil end
  local function grey(v) return { r = v, g = v, b = v } end
  for _, b in ipairs({
    { drawn = S.leftBox, x1 = 218, x2 = 289, shows = S.shows.left, ink = 230, dash = 253 },
    { drawn = S.rightBox, x1 = 298, x2 = 369, shows = S.shows.right, ink = 310, dash = 333 },
  }) do
    if b.drawn and cx >= b.x1 and cx <= b.x2 and cy >= 61 and cy <= 73 then
      if cx == b.x1 or cx == b.x2 or cy == 61 then return grey(157) end
      if b.shows == "value" and cy >= 64 and cy <= 72 and cx >= b.ink and cx < b.ink + 16 then
        return grey(0)
      end
      if b.shows == "dash" and cy == 69 and cx >= b.dash and cx < b.dash + 3 then return grey(94) end
      return grey(194)
    end
  end
  return grey(191)
end

-- A point of the screen as a live read sees it: the strip, and every other point the harness's
-- S.pixel, which lights no tool.
function T.screenAt(x, y)
  return T.strip(x - T.MEL.client.x, y - T.MEL.client.y) or S.pixel
end

-- A point of a picture of the editor, as it was when `shows` was taken (now when not given):
-- Melodyne's light page; the note of the frame (T.frame), orange on its rows unless it is drawn
-- grey; and with the disc the analysis disc as calibration/Melodyne-clean.png shows it, centred 4
-- px above the middle of the client area and S.discDx px right of it — a rim from 50.5 to 53.5 px
-- out reading 120, the filled pie inside it 188, and the caption band across it, 13 rows, halving
-- what is under it.
function T.picture(x, y, shows)
  local strip = T.strip(x - T.MEL.client.x, y - T.MEL.client.y)
  if strip then return strip end
  local function grey(v) return { r = v, g = v, b = v } end
  local frame, disc = S.frame, S.disc
  if shows then frame, disc = shows.frame, shows.disc end
  local n = frame and frame.note
  if n and x >= n.x1 and x < n.x2 and y >= n.y1 and y < n.y2 then
    if n.grey then return grey(200) end
    return { r = 240, g = 170, b = 60 }
  end
  if not disc then return grey(250) end
  local dx = x - (T.MEL.client.x + T.MEL.client.w // 2 + (S.discDx or 0))
  local dy = y - (T.MEL.client.y + T.MEL.client.h // 2 - 4)
  local d = math.sqrt(dx * dx + dy * dy)
  if d >= 50.5 and d <= 53.5 then return grey(120) end
  if d > 53.5 then return grey(250) end
  if dy >= -6 and dy <= 6 then return grey(188 // 2 + 12) end
  return grey(188)
end

rawset(T.host.screen, "pixel", function(x, y)
  S.pixels += 1
  return T.screenAt(x, y)
end)

-- host.screen.pixels: live, one screen read for every point, all or nothing; from a snapshot, the
-- picture's points, each of which must lie inside it.
rawset(T.host.screen, "pixels", function(points, opts)
  assert(type(points) == "table", "host.screen.pixels takes a list of points")
  for k in pairs(opts or {}) do
    assert(k == "snapshot", "host.screen.pixels: opts takes snapshot, got " .. tostring(k))
  end
  local snap = opts and opts.snapshot
  assert(snap == nil or not snap.released, "host.screen.pixels: the snapshot was released")
  if snap == nil then S.pixels += 1 end
  local out = {}
  for i, p in ipairs(points) do
    local x, y = p[1], p[2]
    assert(x == math.floor(x) and y == math.floor(y), "host.screen.pixels: whole numbers")
    if snap then
      local r = snap.region
      if x < r[1] or y < r[2] or x >= r[3] or y >= r[4] then
        return nil, "the point is not inside the snapshot"
      end
      out[i] = T.picture(x, y)
    else
      local c = T.screenAt(x, y)
      if c == nil then return nil, "screen capture failed" end
      out[i] = c
    end
  end
  return out
end)

-- host.screen.profile, from a picture only: rows from T.picture — the read-out boxes' (counted in
-- S.inkProfiles), the note area's (S.rowProfiles), the darkest and lightest of each row and its
-- mean red, green and blue — or the note area's columns, the frame T.frame made, which are asked
-- with a callback and a key: reduced off the event loop, kept in S.reductions and answered by
-- T.runDue, as the host answers on a later turn. The region must lie inside the picture.
local function profile(opts)
  local snap, r = opts.snapshot, opts.region
  assert(snap ~= nil and not snap.released, "profiled from its own picture")
  local p = snap.region
  assert(r and r[1] >= p[1] and r[2] >= p[2] and r[3] <= p[3] and r[4] <= p[4],
    "the region lies inside the picture")
  if opts.axes == "rows" then
    local mins, maxs, rs, gs, bs = {}, {}, {}, {}, {}
    for y = r[2], r[4] - 1 do
      local lo, hi, sr, sg, sb = 255, 0, 0, 0, 0
      for x = r[1], r[3] - 1 do
        local c = T.picture(x, y, snap.shows)
        lo, hi = math.min(lo, c.r), math.max(hi, c.r)
        sr, sg, sb = sr + c.r, sg + c.g, sb + c.b
      end
      local n = r[3] - r[1]
      mins[#mins + 1], maxs[#maxs + 1] = lo, hi
      rs[#rs + 1], gs[#gs + 1], bs[#bs + 1] = sr / n, sg / n, sb / n
    end
    if r[2] >= T.MEL.client.y + 60 + 16 then S.rowProfiles += 1 else S.inkProfiles += 1 end
    return { x = r[1], y = r[2], w = r[3] - r[1], h = r[4] - r[2],
      rows = { min = mins, max = maxs, r = rs, g = gs, b = bs } }
  end
  assert(opts.axes == "columns" and opts.dark == 190, "columns, dark below 190")
  S.profiles += 1
  S.profileRegion = r
  return snap.shows and snap.shows.frame or S.frame
end
rawset(T.host.screen, "profile", function(opts, cb)
  if opts.axes == "columns" then
    assert(type(cb) == "function", "the note area's profile is reduced off the event loop, with a callback")
    assert(type(opts.key) == "string" and opts.key ~= "", "under a key")
  else
    assert(cb == nil, "a box's or a body's rows are profiled on the event loop")
  end
  -- As the host does: without a callback, `key` and then `at` raise.
  for _, k in ipairs(cb == nil and { "key", "at" } or {}) do
    if opts[k] ~= nil then
      error(("host.screen.profile: opts.%s is only taken with a callback (the second argument)"):format(k), 2)
    end
  end
  local p = profile(opts)
  if cb == nil then return p end
  S.reductions[#S.reductions + 1] = { key = opts.key, cb = cb, p = p }
end)

-- Whether a picture or a profile the module asked with `key` is out: asked and not answered.
rawset(T.host.screen, "pending", function(key)
  assert(type(key) == "string" and key ~= "", "host.screen.pending takes a non-empty key")
  for _, r in ipairs(S.asyncs) do
    if r.key == key then return true end
  end
  for _, r in ipairs(S.reductions) do
    if r.key == key then return true end
  end
  return false
end)

-- The profiles asked with a callback, answered after the pictures that are due — a profile asked
-- from a picture's callback included — as the image worker's answer comes on the host's next turn.
local runDue = T.runDue
T.runDue = function()
  runDue()
  while #S.reductions > 0 do
    local r = table.remove(S.reductions, 1)
    S.epoch += 1
    T.call("answer", r.cb, r.p)
  end
end

-- What a picture shows, as far as these scenarios draw it: the note area's frame and the disc.
local function shows() return { frame = S.frame, disc = S.disc } end

-- Whether two pictures are the same, pixel for pixel: the same disc, and frames whose columns hold
-- the same numbers.
local function samePicture(a, b)
  if a == nil or b == nil or a.disc ~= b.disc then return false end
  if a.frame == b.frame then return true end
  if a.frame == nil or b.frame == nil then return false end
  for _, k in ipairs({ "mean", "dark" }) do
    local x, y = a.frame.columns[k], b.frame.columns[k]
    if #x ~= #y then return false end
    for i = 1, #x do
      if x[i] ~= y[i] then return false end
    end
  end
  return true
end

-- host.screen.snapshotAsync as the harness takes it, at its time and answered by T.runDue, counted
-- per key; a newer request with a key takes the place of an older one not answered yet, whose
-- callback is then never called here (the host answers it nil, which the module drops alike). Each
-- picture keeps what it showed; a change wait (`change`, kept in S.changeWaits) is answered, as one
-- of a single round is, with the picture taken and whether it differs from `from`'s.
local snapshotAsync = T.host.screen.snapshotAsync
rawset(T.host.screen, "snapshotAsync", function(opts, cb)
  for k in pairs(opts) do
    assert(k == "region" or k == "key" or k == "at" or k == "change",
      "snapshotAsync: an option " .. tostring(k))
  end
  assert(type(opts.at) == "number", "asked at a set time, so Melodyne's own clicks do not wait for it")
  local change = opts.change
  if change ~= nil then
    for k in pairs(change) do
      assert(k == "from" or k == "timeout" or k == "tolerance" or k == "minPixels",
        "snapshotAsync: a change option " .. tostring(k))
    end
    assert(change.from == nil or not change.from.released, "a change wait's from was released")
    S.changeWaits[#S.changeWaits + 1] = change
  end
  S.snapKeys[opts.key] = (S.snapKeys[opts.key] or 0) + 1
  for i = #S.asyncs, 1, -1 do
    if S.asyncs[i].key == opts.key then table.remove(S.asyncs, i) end
  end
  snapshotAsync(opts, function(snap, why, info)
    if snap then snap.shows = shows() end
    if snap and change then
      info = table.clone(info)
      info.changed = not samePicture(change.from and change.from.shows, snap.shows)
      info.settled, info.rebased = info.changed, false
    end
    return cb(snap, why, info)
  end)
  S.asyncs[#S.asyncs].key = opts.key
end)

-- The language each read with a callback asked for, beside the rest of what T.submit keeps.
local submit = T.submit
T.submit = function(what, opts, cb)
  submit(what, opts, cb)
  S.reads[#S.reads].lang = opts and opts.lang
end

-- A frame of the note area, 910 columns: a light page (mean 200) with a bar line every `bar`
-- columns (120), at 50 and on — `shift` columns further when the view has moved — full height
-- (mean 150, 500 dark rows, as the calibration captures show them), about 19 dark rows a column,
-- and a note's blob of `ink` dark rows (40) in the `width` columns (10) from `at` — no note when
-- `at` is nil — or of `shape`, a list of dark rows column by column, drawn orange on screen rows
-- 300 to 311, or grey with `grey`; more blobs like it from the columns in `also`; and with
-- `cursor` the playback cursor, a full-height line, in that column.
function T.frame(at, opts)
  opts = opts or {}
  local bar, ink, shape = opts.bar or 120, opts.ink or 40, opts.shape
  local width = shape and #shape or opts.width or 10
  local mean, dark = {}, {}
  for i = 1, 910 do
    mean[i] = 200
    dark[i] = (at ~= nil and i >= at and i < at + width) and (shape and shape[i - at + 1] or ink) or 19
  end
  for _, x in ipairs(opts.also or {}) do
    for i = x, x + width - 1 do dark[i] = ink end
  end
  for i = 1, 910 do
    if (i - (opts.shift or 0)) % bar == 50 % bar then mean[i], dark[i] = 150, 500 end
  end
  if opts.cursor then dark[opts.cursor] = 500 end
  local x0 = T.MEL.client.x + 48 - 1
  S.frame = { columns = { mean = mean, dark = dark },
    note = at and { x1 = x0 + at, x2 = x0 + at + width, y1 = 300, y2 = 312, grey = opts.grey } or nil }
end

-- Whether a read of the module's with `key` is out: asked with a callback and not answered — nor
-- dropped (T.drop).
rawset(T.host.ocr, "pending", function(key)
  for _, r in ipairs(S.reads) do
    if r.key == key and not r.answered then return true end
  end
  return false
end)

-- The last read asked for with `key`, and how many were.
function T.lastRead(key)
  for i = #S.reads, 1, -1 do
    if S.reads[i].key == key then return S.reads[i] end
  end
  return nil
end
function T.readsOf(key)
  local n = 0
  for _, r in ipairs(S.reads) do
    if r.key == key then n += 1 end
  end
  return n
end

-- A reading of `text` ("" when nothing was read), or with `status` ("failed", "stale") and no text.
function T.reading(text, status)
  status = status or (text ~= "" and "text" or "none")
  local r = { status = status, text = status == "text" and text or "", lines = {}, words = {},
    skipped = false, newer = status == "stale" }
  if status == "failed" then r.error = "the text recogniser has not answered a region for 5 s" end
  return r
end

-- Answers read `r` with one reading per region, as the host delivers an answer: the epoch turns
-- over first — the epoch, not the input epoch. What became of the answer's handler comes back.
function T.answer(r, ...)
  assert(r and not r.answered, "no read waiting to be answered")
  r.answered = true
  S.epoch += 1
  local readings = { ... }
  if r.list then return T.call("answer", r.cb, readings, {}) end
  return T.call("answer", r.cb, readings[1])
end

-- The read-out watcher's last read, answered: the left box reads `note`, the right one `pitch`
-- (when the read asked for it); "" is a box read empty.
function T.boxes(note, pitch)
  local r = T.lastRead("selection")
  if #r.regions == 1 then return T.answer(r, T.reading(note)) end
  return T.answer(r, T.reading(note), T.reading(pitch or ""))
end

-- A read the module made is dropped, its callback never called — as when the module is disabled
-- before the answer: it is no longer pending.
function T.drop(r)
  assert(r and not r.answered, "no read waiting to be dropped")
  r.answered = true
end

-- Loads the module as the host loads it, with Melodyne's window in front, and hands back its
-- overlay, active: the first evaluation, asked for as it bound (`host.timer.after(0)`), has run.
-- A read that waits — the reading after a tool switch, "Position" — is answered at once, each
-- region with what S.ocr(region) says is written there (nothing by default); one with a callback
-- is kept for the scenario to answer (T.submit, T.answer).
function T.melodyne()
  local made = T.collect()
  S.listed = { T.MEL, T.OTHER }
  T.at(T.MEL)
  T.modules("modules/melodyne/")
  -- The input epoch turns when the scenario says (T.key); a delivery turns the epoch only.
  rawset(T.host, "inputEpoch", function() return S.inputEpoch end)
  -- A tool's hotkey (F1 to F6) selects its tab once the modifiers are up: they are.
  rawset(T.host.keys, "modifiersDown", function() return false end)
  T.answerWaits(function(what)
    local function one(r)
      S.recognized[#S.recognized + 1] = { r[1], r[2], r[3], r[4] }
      return T.reading(S.ocr and S.ocr(r) or "")
    end
    if type(what[1]) == "table" then
      local list = {}
      for i, e in ipairs(what) do list[i] = one(e.region or e) end
      return list, {}
    end
    return one(what.region or what)
  end)
  T.source("modules/melodyne/src/main.luau")(T.host)
  assert(#made == 1, "one overlay: " .. #made)
  T.runDue()
  return made[1]
end

-- The picture asked with `key` that is due, taken and answered on its own, as on the host's next
-- turn — nothing else that is due: what became of its callback, nil when none was asked.
function T.snapOf(key)
  for i, r in ipairs(S.asyncs) do
    if r.key == key and r.at <= S.now then
      table.remove(S.asyncs, i)
      S.epoch += 1
      local snap = T.snap(r.region)
      snap.time = r.at
      return T.call("answer", r.cb, snap, nil, { waited = r.at - r.asked, frames = 1 })
    end
  end
  return nil
end

-- Melodyne's polls, run as the host runs a host.timer.every: what became of the handler. The
-- watcher's picture is then taken and answered, and the note area's and the analysis look's with
-- everything else that is due, as on the host's next turns.
function T.watch()
  local ran = T.call("timer", S.polls[120])
  T.snapOf("selection")
  return ran
end
function T.measure()
  local ran = T.call("timer", S.polls[200])
  T.runDue()
  return ran
end
function T.look()
  local ran = T.call("timer", S.polls[500])
  T.runDue()
  return ran
end

-- The read-outs fall silent: the boxes show a dash, and the watcher's next read answers one in
-- every box it reads.
function T.silent()
  T.shows("dash")
  T.watch()
  local r = T.lastRead("selection")
  if #r.regions == 1 then return T.answer(r, T.reading("-")) end
  return T.answer(r, T.reading("-"), T.reading("-"))
end

-- A hotkey of the overlay's, pressed: the epoch and the input epoch turn over first.
function T.hotkey(spec)
  assert(S.hotkeyFns[spec], spec .. " is not registered")
  S.epoch += 1
  S.inputEpoch += 1
  return T.call("hotkey", S.hotkeyFns[spec])
end

-- The pass name's last read, answered: the first crop reads `text`, the second `other` (`text`
-- when not given), or both have `status` ("failed") and no text.
function T.pass(text, other, status)
  local r = T.lastRead("analysis pass")
  if status then return T.answer(r, T.reading("", status), T.reading("", status)) end
  return T.answer(r, T.reading(text), T.reading(other == nil and text or other))
end

-- Melodyne loaded with the disc up: the look made on arrival reads the pass name, the look made at
-- once after that first name reads it again and the name is taken, the arrival sentence is said,
-- then a look says "Busy: <pass>" and its read is answered. The overlay comes back, and how much
-- had been said before that look.
function T.analysing(pass)
  S.disc = true
  local o = T.melodyne()
  T.pass(pass)
  T.runDue()
  T.pass(pass)
  S.now += 400
  T.runDue()
  local arrived = #S.speech
  T.look()
  T.pass(pass)
  return o, arrived
end

-- A key of the overlay's own, as the captured key delivers it: the epoch and the input epoch turn
-- over first. What became of the key's handler comes back.
function T.key(spec)
  assert(S.holding[spec], spec .. " is not captured, so it would not reach the overlay")
  S.epoch += 1
  S.inputEpoch += 1
  return T.call("key", S.captured[spec])
end

function T.said(from)
  local out = {}
  for i = from or 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end

-- An assertion whose failure prints the whole log and everything said to the test's output: a Luau
-- error message is cut at about 500 bytes, which is a few log lines.
function T.ok(cond, what)
  if cond then return end
  print(T.dump())
  print("said: " .. T.said())
  error(tostring(what or "assertion failed"), 2)
end

-- Melodyne loaded, the right box drawn, and the watcher's first read answered with `note` and
-- `pitch`: the baseline, said nothing.
function T.watching(note, pitch)
  S.rightBox = true
  local o = T.melodyne()
  T.ok(o.active, "Melodyne's overlay is in front")
  T.watch()
  local said = #S.speech
  T.boxes(note, pitch)
  T.ok(#S.speech == said, "the baseline is said nothing: " .. T.said(said + 1))
  return o
end

-- A frame of the view moved: fourteen blobs, 60 columns apart from 100 — more changed runs than a
-- note's move makes.
function T.frameMany()
  T.frame(100)
  for k = 0, 13 do
    for i = 100 + 60 * k, 109 + 60 * k do S.frame.columns.dark[i] = 40 end
  end
end

-- Two frames of the note area, the note's blob at 300 and then at 330: the first is the baseline,
-- the second a move of 30 columns to the right — a beat of the 120-column bar in 4/4 — which asks
-- for the time signature unless this input epoch has read it.
function T.moveRight()
  T.frame(300)
  T.measure()
  T.frame(330)
  return T.measure()
end

-- The note's blob drawn at `at` (none when nil) in a frame made with `opts` (T.frame) and measured,
-- the time signature answered `sig` ("4/4") when it is asked: what was said ("" for nothing).
function T.to(at, opts, sig)
  local said = #S.speech
  local asked = T.readsOf("time signature")
  T.frame(at, opts)
  T.measure()
  if T.readsOf("time signature") > asked then
    T.answer(T.lastRead("time signature"), T.reading(sig or "4/4"))
  end
  return T.said(said + 1)
end
"##;

/// A fresh VM with the scripted host, the runtime, the prelude's matcher, the host-panel helpers
/// and the ones above; the host plays Windows, the one platform Melodyne's module supports.
fn run(scenario: &str) {
    let lua = Lua::new();
    let t: Table = harness(&lua);
    let wrapped: Function = lua
        .load(format!("return function(host) {RUNTIME}\nend"))
        .set_name("overlay-runtime/src/main.luau")
        .eval()
        .expect("the runtime compiles");
    let host: Table = t.get("host").unwrap();
    let o: Table = wrapped.call(host).expect("the runtime loads against the scripted host");
    t.set("O", o).unwrap();
    let prelude: Function = lua
        .load(format!("return function(host) {WINDOW_PRELUDE}\nend"))
        .set_name("window_prelude.luau")
        .eval()
        .expect("the window prelude compiles");
    t.set("windowPrelude", prelude).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
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
    lua.globals().set("T", t).unwrap();
    for (name, chunk) in [("host-panel helpers", HP), ("melodyne helpers", MEL)] {
        if let Err(e) = lua.load(chunk).set_name(name).exec() {
            panic!("{e}");
        }
    }
    if let Err(e) = lua.load(scenario).set_name("scenario").exec() {
        panic!("{e}");
    }
    finish(&lua);
}

// ---------------------------------------------------------------------------------------------
// The read-out watcher (M1).
// ---------------------------------------------------------------------------------------------

/// The watcher's tick asks with a callback and ends: nothing waits, the read is out under its key,
/// both boxes in one call, where Melodyne's window puts them. A key of Melodyne's own pressed
/// while it is out runs at once — it used to wait behind the tick.
#[test]
fn the_watcher_asks_with_a_callback_and_melodynes_keys_do_not_wait_behind_it() {
    run(r#"
        local S = T.S
        S.rightBox = true
        local o = T.melodyne()
        T.ok(o.active, "Melodyne's overlay is in front")
        local waited = #S.recognized
        T.ok(T.watch() == "Ran", "the tick does not wait")
        local r = T.lastRead("selection")
        T.ok(r and not r.answered, "a read with a callback is out")
        T.ok(#S.recognized == waited, "nothing was read that waits")
        T.ok(#r.regions == 2 and T.region(r.regions[1]) == "228,92,295,105"
          and T.region(r.regions[2]) == "308,92,375,105",
          "the two boxes, from the client area's corner: " .. T.region(r.regions[1]))
        T.ok(r.snapshot ~= nil and T.region(r.snapshot.region) == "220,91,380,107",
          "read from the strip's own picture")
        T.ok(T.host.ocr.pending("selection"), "the read is pending")
        T.ok(T.key("Tab") == "Ran", "a key of Melodyne's own runs while the read is out")
        T.ok(T.key("Tab") == "Ran", "and the next")
        T.ok(not r.answered, "the read is still out")
        T.ok(T.boxes("A4", "+12 ct") == "Ran", "its answer runs as a handler")
        T.ok(not T.host.ocr.pending("selection"), "and is not pending any more")
    "#);
}

/// Without the right box, the watcher reads the left one alone.
#[test]
fn without_the_right_box_the_watcher_reads_the_left_one_alone() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.watch()
        local r = T.lastRead("selection")
        T.ok(#r.regions == 1 and T.region(r.regions[1]) == "228,92,295,105",
          "the left box alone: " .. #r.regions)
        T.boxes("0.00 dB")
        T.watch()
        local said = #S.speech
        T.boxes("-3.00 dB")
        T.ok(T.said(said + 1) == "-3.00 dB", T.said(said + 1))
    "#);
}

/// The first answer of a stay is the baseline and says nothing; then a box that changed is said
/// and one that did not is not, both changed boxes in one sentence; a dash is not said.
#[test]
fn a_box_that_changed_is_said_and_one_that_did_not_is_not() {
    run(r#"
        local S = T.S
        T.watching("A4", "+12 ct")
        local said = #S.speech
        T.watch()
        T.boxes("A4", "+13 ct")
        T.watch()
        T.boxes("A4", "+13 ct")
        T.watch()
        T.boxes("A#4", "+0 ct")
        T.watch()
        T.boxes("-", "+0 ct")
        T.ok(T.said(said + 1) == "+13 ct | A#4, +0 ct", T.said(said + 1))
        T.ok(S.speech[#S.speech].interrupt == true, "said interrupting")
        T.ok(T.count('left="A4" right="+13 ct"') == 1, T.dump())
    "#);
}

/// One read out at a time: a tick while the last read is out asks nothing; once it has answered,
/// the next tick asks again.
#[test]
fn one_read_is_out_at_a_time() {
    run(r#"
        local S = T.S
        T.watching("A4", "+12 ct")
        T.watch()
        T.ok(T.readsOf("selection") == 2, "asked again after the answer")
        T.watch()
        T.watch()
        T.ok(T.readsOf("selection") == 2, "nothing asked while that read is out")
        T.boxes("A4", "+12 ct")
        T.watch()
        T.ok(T.readsOf("selection") == 3, "asked again after the answer")
    "#);
}

/// An answer asked before the overlay left and came back — with no tick in between, so the
/// forgetting on the way out never ran — is dropped: nothing said, and it is no baseline either, so
/// the first answer of the new stay is the baseline and says nothing, and the next change is said.
#[test]
fn an_answer_from_before_the_overlay_left_and_came_back_is_dropped_and_is_no_baseline() {
    run(r#"
        local S = T.S
        local o = T.watching("A4", "+12 ct")
        T.watch()
        local old = T.lastRead("selection")
        T.show(T.OTHER)
        T.ok(not o.active, "another application in front")
        T.show(T.MEL)
        T.ok(o.active, "Melodyne in front again, a new stay")
        local said = #S.speech
        T.answer(old, T.reading("B4"), T.reading("+5 ct"))
        T.ok(#S.speech == said, "the old answer says nothing: " .. T.said(said + 1))
        T.watch()
        T.boxes("C4", "+1 ct")
        T.ok(#S.speech == said, "the new stay's first answer is its baseline: " .. T.said(said + 1))
        T.watch()
        T.boxes("D4", "+1 ct")
        T.ok(T.said(said + 1) == "D4", T.said(said + 1))
    "#);
}

/// Away while the read is out, the tick forgets; the answer, which comes while away, says nothing
/// and is no news about the read-outs either — both boxes read empty there do not make them silent,
/// so the note area is not measured on the way back. Back, the watcher reads again, and its first
/// answer is the baseline.
#[test]
fn after_the_overlay_was_away_the_watcher_reads_again() {
    run(r#"
        local S = T.S
        local o = T.watching("A4", "+12 ct")
        T.watch()
        local old = T.lastRead("selection")
        T.show(T.OTHER)
        T.watch()
        T.ok(T.readsOf("selection") == 2, "nothing asked while away")
        local away = #S.speech
        T.answer(old, T.reading(""), T.reading(""))
        T.ok(#S.speech == away, "the old answer says nothing: " .. T.said(away + 1))
        T.show(T.MEL)
        T.ok(o.active)
        T.frame(300)
        T.measure()
        T.ok(S.profiles == 0, "the read-outs that spoke before still do: no note-area picture")
        local said = #S.speech
        T.watch()
        T.ok(T.readsOf("selection") == 3, "asked again once back")
        T.boxes("A4", "+12 ct")
        T.watch()
        T.boxes("A4", "+14 ct")
        T.ok(T.said(said + 1) == "+14 ct", T.said(said + 1))
    "#);
}

/// An answer that comes while a menu is open is dropped and remembered as nothing: once the menu
/// is closed, the same value read again is the change, and it is said.
#[test]
fn an_answer_while_a_menu_is_open_is_dropped() {
    run(r#"
        local S = T.S
        T.watching("A4", "+12 ct")
        T.watch()
        S.native = true
        local said = #S.speech
        T.boxes("File", "Edit")
        T.ok(#S.speech == said, "nothing said: " .. T.said(said + 1))
        T.watch()
        T.ok(T.readsOf("selection") == 2, "no read asked while the menu is open")
        S.native = false
        T.watch()
        T.boxes("B4", "+12 ct")
        T.ok(T.said(said + 1) == "B4", T.said(said + 1))
    "#);
}

/// A tool switch holds the read-outs for half a second. An answer to a read asked just before the
/// switch is held although it comes after the hold has lifted: the picture is of the moment it was
/// asked in. It is logged as held. The first answer to a read asked after the switch is the new
/// tool's values — here, as with reads slower than the hold, asked only after the hold — and becomes
/// the baseline unsaid, where it used to be said as a change; a change after it is said.
#[test]
fn an_answer_asked_before_the_hold_lifted_is_held_and_the_next_is_the_new_tools_baseline() {
    run(r#"
        local S = T.S
        T.watching("A4", "+12 ct")
        T.watch()
        local asked = T.lastRead("selection")
        S.epoch += 1
        S.inputEpoch += 1
        T.call("hotkey", S.hotkeyFns["F4"])
        S.now += 2000
        local said = #S.speech
        T.answer(asked, T.reading("0.00 dB"), T.reading(""))
        T.ok(#S.speech == said, "held: " .. T.said(said + 1))
        T.ok(T.count('left="0.00 dB" right=""  (held: the tool was just changed)') == 1, T.dump())
        T.watch()
        T.boxes("-3.00 dB", "")
        T.ok(#S.speech == said, "the new tool's first reading is its baseline: " .. T.said(said + 1))
        T.ok(T.count('left="-3.00 dB" right=""  (held: the first reading since the tool was changed)') == 1, T.dump())
        T.watch()
        T.boxes("-6.00 dB", "")
        T.ok(T.said(said + 1) == "-6.00 dB", T.said(said + 1))
    "#);
}

/// With reads faster than the hold, the first read asked after the tool switch comes during the
/// hold and is held anyway; the first one after the hold is compared with it, and a change is said.
#[test]
fn with_reads_faster_than_the_hold_a_change_after_it_is_said() {
    run(r#"
        local S = T.S
        T.watching("A4", "+12 ct")
        S.epoch += 1
        S.inputEpoch += 1
        T.call("hotkey", S.hotkeyFns["F4"])
        S.now += 100
        local said = #S.speech
        T.watch()
        T.boxes("0.00 dB", "")
        T.ok(#S.speech == said, "held: " .. T.said(said + 1))
        T.ok(T.count('left="0.00 dB" right=""  (held: the tool was just changed)') == 1, T.dump())
        S.now += 500
        T.watch()
        T.boxes("-3.00 dB", "")
        T.ok(T.said(said + 1) == "-3.00 dB", T.said(said + 1))
    "#);
}

/// A read that failed changes nothing: the read-outs that were speaking still are — the note area
/// takes no picture — and the next tick asks again. A read that answered both boxes empty while
/// they hold values is not silence either: the recogniser dropped out, and the picture shows the
/// values — it used to open the note area's gate. Dashes in both boxes are the read-outs falling
/// silent, and the note area starts measuring; an empty read of them is still silence, and a value
/// ends it. (The second picture is the first one again, so it is not profiled.)
#[test]
fn a_failed_read_changes_nothing_and_the_picture_says_what_an_empty_read_is() {
    run(r#"
        local S = T.S
        T.watching("A4", "+12 ct")
        T.frame(300)
        T.measure()
        T.ok(S.profiles == 0 and S.snapKeys["note area"] == nil,
          "the read-outs speak, so the note area takes no picture")
        T.watch()
        local said = #S.speech
        T.answer(T.lastRead("selection"), T.reading("", "failed"), T.reading("", "failed"))
        T.ok(#S.speech == said, "nothing said: " .. T.said(said + 1))
        T.measure()
        T.ok(S.profiles == 0, "a failed read is not silence")
        T.watch()
        T.ok(T.readsOf("selection") == 3, "the next tick asks again")
        T.boxes("", "")
        T.ok(#S.speech == said, "an empty read says nothing: " .. T.said(said + 1))
        T.measure()
        T.ok(S.profiles == 0 and S.snapKeys["note area"] == nil, "an empty read of values is not silence")
        T.shows("dash")
        T.watch()
        T.boxes("-", "-")
        T.ok(#S.speech == said, "a dash is not said: " .. T.said(said + 1))
        T.measure()
        T.ok(S.profiles == 1, "dashes are silence: the note area is measured")
        T.ok(T.count('[melodyne] note area: gate opened, read-outs "-"|"-"') == 1, T.dump())
        T.watch()
        T.boxes("", "")
        T.measure()
        T.ok(S.snapKeys["note area"] == 2, "an empty read of dashes is silence still")
        T.shows("value")
        T.watch()
        T.boxes("B4", "")
        T.measure()
        T.ok(S.snapKeys["note area"] == 2, "a value ends it")
        T.ok(T.count('[melodyne] note area: gate closed (the read-outs are not silent), read-outs "B4"|"-"') == 1,
          T.dump())
    "#);
}

/// At load the read-outs have said nothing yet, and that is not silence: no picture of the note
/// area is taken until a read of this stay has shown dashes — at load the first frames were once
/// measured before any read had answered.
#[test]
fn before_any_read_the_note_area_takes_no_picture() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.frame(300)
        T.measure()
        T.measure()
        T.ok(S.snapKeys["note area"] == nil and S.profiles == 0, "no picture before a read")
        T.watch()
        T.boxes("")
        T.measure()
        T.ok(S.snapKeys["note area"] == nil, "nor after an empty one")
        T.silent()
        T.measure()
        T.ok(S.snapKeys["note area"] == 1 and S.profiles == 1, "a dash opens the gate")
    "#);
}

// ---------------------------------------------------------------------------------------------
// The note area's move and the time signature (M4).
// ---------------------------------------------------------------------------------------------

/// A move asks for the time signature with a callback, and the tick ends: nothing is said yet, and
/// no frame is taken while the read is out. Its answer says the move, in beats — the first move of
/// the stay as well, measured from the two halves of its own frame.
#[test]
fn a_move_is_said_once_the_time_signature_has_answered() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local said = #S.speech
        T.ok(T.moveRight() == "Ran", "the tick does not wait")
        local r = T.lastRead("time signature")
        T.ok(r and not r.answered, "the time signature is asked with a callback")
        T.ok(T.region(r.region) == "630,43,676,65", "where Melodyne prints it: " .. T.region(r.region))
        T.ok(#S.speech == said, "nothing said before it answers: " .. T.said(said + 1))
        T.ok(S.profiles == 2, "two frames")
        T.frame(390)
        T.measure()
        T.ok(S.profiles == 2, "no frame while the time signature is out")
        T.ok(T.answer(r, T.reading("4/4")) == "Ran")
        T.ok(T.said(said + 1) == "right, one beat", T.said(said + 1))
        T.ok(S.speech[#S.speech].interrupt == true, "said interrupting")
        T.ok(T.count('[melodyne] note area said "right, one beat" (runs 2, arrived 10, frame age 0 ms, read-outs "-"|"")')
          == 1, T.dump())
    "#);
}

/// Within one input epoch the time signature is read once: the next move is said at once, with no
/// read. A new input epoch reads it again.
#[test]
fn the_time_signature_is_read_once_per_input_epoch() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local said = #S.speech
        T.moveRight()
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.frame(360)
        T.measure()
        T.ok(T.readsOf("time signature") == 1, "no second read in the same input epoch")
        T.ok(T.said(said + 1) == "right, one beat | right, one beat", T.said(said + 1))
        S.inputEpoch += 1
        T.frame(390)
        T.measure()
        T.ok(T.readsOf("time signature") == 2, "read again in a new input epoch")
        T.ok(#S.speech == said + 2, "and the move waits for it: " .. T.said(said + 1))
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.ok(#S.speech == said + 3 and T.said(said + 3) == "right, one beat", T.said(said + 1))
    "#);
}

/// The time signature answers after Melodyne has left the front: the move is not said.
#[test]
fn a_move_whose_melodyne_left_the_front_during_the_read_is_not_said() {
    run(r#"
        local S = T.S
        local o = T.melodyne()
        T.silent()
        local said = #S.speech
        T.moveRight()
        local r = T.lastRead("time signature")
        T.show(T.OTHER)
        T.ok(not o.active, "another application in front")
        local away = #S.speech
        T.answer(r, T.reading("4/4"))
        T.ok(#S.speech == away, "nothing said: " .. T.said(away + 1))
    "#);
}

/// A time-signature read that failed says the move without beats — one step, the first of the steps
/// he makes — and leaves the epoch unread, so the next move asks again; one that never answered —
/// dropped, as when the module is disabled — is not pending any more and is not taken as read
/// either.
#[test]
fn a_time_signature_read_that_failed_or_never_answered_is_asked_again() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local said = #S.speech
        T.moveRight()
        T.answer(T.lastRead("time signature"), T.reading("", "failed"))
        T.ok(T.said(said + 1) == "right, one step", T.said(said + 1))
        T.frame(360)
        T.measure()
        T.ok(T.readsOf("time signature") == 2, "asked again after a failed read")
        T.drop(T.lastRead("time signature"))
        T.frame(390)
        T.measure()
        T.ok(T.readsOf("time signature") == 3, "asked again after a read that never answered")
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.ok(T.said(said + 1) == "right, one step | right, one beat", T.said(said + 1))
    "#);
}

/// A frame is compared only with one of the same stay. Away and back with no tick in between — so
/// no closed gate was seen on the way — and the read-outs silent again in the new stay: its first
/// frame is a baseline, not a move measured against a picture of the last stay, and the next move
/// is measured from it. Away and back once more, the last stay's silence is not this one's, and no
/// picture is taken until a read of this stay. And within a stay, a frame from before the gate
/// closed is gone once it opens again.
#[test]
fn a_frame_is_compared_only_within_its_stay_and_never_across_a_closed_gate() {
    run(r#"
        local S = T.S
        local o = T.melodyne()
        T.silent()
        T.frame(300)
        T.measure()
        T.ok(T.count("[melodyne] note area: baseline") == 1, T.dump())
        T.show(T.OTHER)
        T.show(T.MEL)
        T.ok(o.active, "back, a new stay")
        T.silent()
        local said = #S.speech
        T.frame(330)
        T.measure()
        T.ok(T.count("[melodyne] note area: baseline") == 2, "a baseline, not a move: " .. T.dump())
        T.ok(T.count("[melodyne] note area: gate closed") == 0, "no closed gate was seen: " .. T.dump())
        T.frame(360)
        T.measure()
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.ok(T.said(said + 1) == "right, one beat",
          "measured from this stay's frame at 330, not the last stay's at 300: " .. T.said(said + 1))
        said = #S.speech

        T.show(T.OTHER)
        T.show(T.MEL)
        local pictures = S.snapKeys["note area"]
        T.measure()
        T.ok(S.snapKeys["note area"] == pictures, "the last stay's silence is not this one's: no picture")

        T.silent()
        T.frame(390)
        T.measure()
        T.frame(420)
        T.measure()
        T.ok(T.count("[melodyne] note area: baseline") == 3, T.dump())
        T.ok(T.said(said + 1) == "right, one beat", "the move within this stay: " .. T.said(said + 1))
        T.watch()
        T.boxes("A4")
        T.measure()
        T.ok(T.count('[melodyne] note area: gate closed (the read-outs are not silent), read-outs "A4"|""') == 1, T.dump())
        T.silent()
        said = #S.speech
        T.frame(450)
        T.measure()
        T.ok(T.count("[melodyne] note area: baseline") == 4, "after the gate was closed, a baseline: " .. T.dump())
        T.ok(#S.speech == said, "a baseline says nothing: " .. T.said(said + 1))
    "#);
}

/// The view moved — Melodyne scrolled — and the note area re-baselines: the note's last position
/// goes with the old picture. The next frame is a baseline, and the move after it is measured from
/// that, where it used to be measured against a place in the old view and said as a move of that
/// size.
#[test]
fn after_the_view_moved_the_notes_last_position_is_forgotten() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local said = #S.speech
        T.frame(300)
        T.measure()
        T.frameMany()
        T.measure()
        T.ok(T.count("the view moved, re-baselining") == 1, T.dump())
        T.frame(600)
        T.measure()
        T.ok(T.count("[melodyne] note area: baseline") == 2 and T.readsOf("time signature") == 0,
          "a baseline after the view moved: " .. T.dump())
        T.frame(630)
        T.measure()
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.ok(T.said(said + 1) == "right, one beat", "no move measured against the old view: " .. T.said(said + 1))
    "#);
}

/// Every hundred frames the log says how many, how many were the picture before, and what they cost
/// the event loop.
#[test]
fn every_hundred_frames_the_log_says_what_they_cost() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        T.frame(300)
        for _ = 1, 99 do T.measure() end
        T.ok(T.count("[melodyne] note area: 100 frames") == 0, T.dump())
        T.measure()
        T.ok(T.count("[melodyne] note area: 100 frames, 99 of them the picture before, 0 ms on the event "
          .. "loop for them (0.0 each), 0.0 ms from asking to the answer on average") == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The analysis (2026-10-05).
// ---------------------------------------------------------------------------------------------

/// Melodyne is analysing as the overlay arrives. The look made on arrival finds the disc in its own
/// picture, taken at a set time off the event loop, and reads the pass name from that picture with
/// a callback, in English, from two crops of the caption inside the rim. One look's name is not
/// taken on its own: the next picture is asked for at once, and its read takes it. Both answers
/// come before the overlay's arrival sentence, so "Busy" waits, and is said — queued, not
/// interrupting — on the first look after the arrival. Once.
#[test]
fn an_analysis_found_on_arrival_is_said_after_the_arrival_sentence_once() {
    run(r#"
        local S = T.S
        S.disc = true
        local o = T.melodyne()
        T.ok(o.active, "Melodyne's overlay is in front")
        local before = #S.speech
        T.ok(S.snapKeys["analysis disc"] == 1, "a look on arrival, one picture")
        local r = T.lastRead("analysis pass")
        T.ok(r and not r.answered, "the pass name is read with a callback")
        T.ok(#r.regions == 2 and T.region(r.regions[1]) == "458,371,558,384"
          and T.region(r.regions[2]) == "459,371,558,384",
          "the caption from 50 and 49 px left of the centre to 49 right of it, inside the rim: "
          .. T.region(r.regions[1]))
        T.ok(r.lang == "en" and r.snapshot ~= nil, "in English, from the look's own picture")
        T.pass("Polyphonic Detection")
        T.ok(T.count("[melodyne] analysis found") == 0, "one look is not enough: " .. T.dump())
        T.ok(S.snapKeys["analysis disc"] == 2, "the next picture is asked for at once")
        T.runDue()
        T.ok(T.readsOf("analysis pass") == 2, "and read")
        T.pass("Polyphonic Detection")
        T.ok(#S.speech == before, "not before the overlay's own sentence: " .. T.said(before + 1))
        T.ok(T.count('[melodyne] analysis found: "Polyphonic Detection" (2 looks read it, 0 reads not taken;'
          .. " the disc's centre +0 px across from the window's middle)") == 1, T.dump())
        S.now += 400
        T.runDue()
        local arrived = #S.speech
        T.ok(arrived == before + 1, "the overlay arrived: " .. T.said(before + 1))
        T.ok(T.look() == "Ran", "the look does not wait")
        T.ok(T.said(arrived + 1) == "Busy: Polyphonic Detection", T.said(arrived + 1))
        T.ok(S.speech[#S.speech].interrupt == false, "queued behind the arrival, not interrupting it")
        T.pass("Polyphonic Detection")
        T.look()
        T.pass("Polyphonic Detection")
        T.ok(T.said(arrived + 1) == "Busy: Polyphonic Detection", "said once: " .. T.said(arrived + 1))
        T.ok(T.count("[melodyne] analysis found") == 1, T.dump())
        T.ok(S.snapKeys["analysis disc"] == 4, "no more pictures asked for at once: " .. S.snapKeys["analysis disc"])
    "#);
}

/// A misreading is never said. A read whose two crops differ is not taken, nor one that is not
/// letters and spaces, nor a name read by one look alone — "Busy" and the read-out wait for a name
/// two looks in a row have read, a read not taken between them counting neither way: a misreading
/// both crops shared once is never the pass. The first read not taken is in the log, and the rest
/// are counted when the name is taken and when the analysis ends.
#[test]
fn a_misreading_of_the_pass_name_is_never_said() {
    run(r#"
        local S = T.S
        S.disc = true
        T.melodyne()
        T.pass("Polyphonie Detectionl", "Polyphonic Detection")
        T.ok(S.snapKeys["analysis disc"] == 1, "a read not taken asks for no picture at once")
        T.ok(T.count('[melodyne] analysis pass: not taken, read "Polyphonie Detectionl" and "Polyphonic Detection"')
          == 1, T.dump())
        S.now += 400
        T.runDue()
        local arrived = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(arrived + 1) == "Analysis, busy", "no name yet: " .. T.said(arrived + 1))
        T.look()
        T.pass("Pöbhffoiü@.petectionl")
        T.look()
        T.pass("Polyphonie Detection")
        T.ok(S.snapKeys["analysis disc"] == 4, "the first name asks for the next picture at once")
        T.runDue()
        T.pass("Polyphonic Detection")
        T.look()
        T.pass("Polyphonic Detection", "Polyphonic Detectiom")
        T.hotkey("Alt+A")
        T.ok(T.said(arrived + 1) == "Analysis, busy | Analysis, busy",
          "one look each is not enough: " .. T.said(arrived + 1))
        T.look()
        T.pass("Polyphonic Detection")
        T.ok(T.count("[melodyne] analysis pass: not taken") == 1, "the first read not taken, once: " .. T.dump())
        T.ok(T.count('[melodyne] analysis found: "Polyphonic Detection" (2 looks read it, 3 reads not taken;')
          == 1, T.dump())
        T.hotkey("Alt+A")
        T.ok(T.said(arrived + 1)
          == "Analysis, busy | Analysis, busy | Busy: Polyphonic Detection | Analysis, Polyphonic Detection",
          T.said(arrived + 1))
        T.ok(not string.find(T.said(), "Polyphonie", 1, true) and not string.find(T.said(), "petection", 1, true)
          and not string.find(T.said(), "Detectiom", 1, true), "no misreading said: " .. T.said())
        S.disc = false
        T.look()
        T.look()
        T.ok(T.count('[melodyne] analysis finished: "Polyphonic Detection" (2 looks read it, 1 another name, '
          .. '3 reads not taken)') == 1, T.dump())
    "#);
}

/// A pass that follows another is taken at its second look in a row, however long the first ran —
/// the read-out says it, and "Busy" has said the first, once — while a name read once between two
/// looks of another never is: neither the misreading read once after two looks of the right name,
/// nor one read once before them.
#[test]
fn a_pass_that_follows_another_is_taken_at_its_second_look_in_a_row() {
    run(r#"
        local S = T.S
        local o, arrived = T.analysing("Polyphonic Detection")
        T.ok(T.said(arrived + 1) == "Busy: Polyphonic Detection", T.said(arrived + 1))
        for _ = 1, 6 do
          T.look()
          T.pass("Polyphonic Detection")
        end
        T.look()
        T.pass("Tempo Detection")
        T.hotkey("Alt+A")
        T.ok(string.find(T.said(arrived + 1), "Analysis, Polyphonic Detection$") ~= nil,
          "one look of the next pass is not enough: " .. T.said(arrived + 1))
        T.look()
        T.pass("Tempo Detection")
        T.ok(T.count('[melodyne] analysis pass now "Tempo Detection", was "Polyphonic Detection" (2 looks in a row)')
          == 1, T.dump())
        local said = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, Tempo Detection", T.said(said + 1))
        T.ok(not string.find(T.said(arrived + 1), "Busy: Tempo", 1, true), "Busy once: " .. T.said(arrived + 1))
        T.look()
        T.pass("Tcmpo Detection")
        T.look()
        T.pass("Tempo Detection")
        said = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, Tempo Detection", "a misreading read once: " .. T.said(said + 1))
        T.ok(T.count("[melodyne] analysis pass now") == 1, T.dump())
    "#);
    run(r#"
        local S = T.S
        S.disc = true
        T.melodyne()
        T.pass("Polyphonic Detectior")
        T.runDue()
        T.pass("Polyphonic Detection")
        T.look()
        T.pass("Polyphonic Detection")
        T.ok(T.count('[melodyne] analysis found: "Polyphonic Detection" (2 looks read it, 0 reads not taken;')
          == 1, T.dump())
        T.look()
        T.pass("Polyphonic Detectior")
        local said = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, Polyphonic Detection", "read once before and once after: " .. T.said(said + 1))
    "#);
}

/// The disc goes away: "Analysis finished" once, queued, after a second look in a row without it —
/// and the log names the pass. A single look without it between two with it ends nothing.
#[test]
fn analysis_finished_is_said_once_when_the_disc_has_gone() {
    run(r#"
        local S = T.S
        local o, arrived = T.analysing("Polyphonic Detection")
        T.ok(T.said(arrived + 1) == "Busy: Polyphonic Detection", T.said(arrived + 1))
        local said = #S.speech
        S.disc = false
        T.look()
        S.disc = true
        T.look()
        S.disc = false
        T.look()
        T.ok(#S.speech == said, "one look without the disc ends nothing: " .. T.said(said + 1))
        T.look()
        T.ok(T.said(said + 1) == "Analysis finished", T.said(said + 1))
        T.ok(S.speech[#S.speech].interrupt == false, "queued")
        T.ok(T.count('[melodyne] analysis finished: "Polyphonic Detection"') == 1, T.dump())
        T.look()
        T.look()
        T.ok(T.said(said + 1) == "Analysis finished", "once: " .. T.said(said + 1))
    "#);
}

/// The recogniser drops out on the pass name — empty, failed — while the disc stays: nothing is
/// said, the analysis is not over, and the read-out still names the pass. An analysis seen in an
/// earlier stay is not finished in this one: away and back after it ended, nothing is said.
#[test]
fn a_dropout_of_the_recogniser_does_not_end_the_analysis() {
    run(r#"
        local S = T.S
        local o, arrived = T.analysing("Polyphonic Detection")
        local said = #S.speech
        for _ = 1, 3 do
          T.look()
          T.pass("")
        end
        T.look()
        T.pass(nil, nil, "failed")
        T.ok(#S.speech == said, "nothing said: " .. T.said(said + 1))
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, Polyphonic Detection", T.said(said + 1))

        T.show(T.OTHER)
        T.look()
        S.disc = false
        T.show(T.MEL)
        T.ok(o.active, "back, a new stay")
        said = #S.speech
        T.look()
        T.look()
        T.ok(not string.find(T.said(said + 1), "Analysis finished", 1, true),
          "an analysis of the last stay ends nothing in this one: " .. T.said(said + 1))
    "#);
}

/// While the disc is up the note area says nothing and compares nothing: a frame with the disc in
/// it is not even profiled — before any look has seen it — and once a look has, no picture is
/// taken at all. After the analysis the first frame is a baseline.
#[test]
fn the_note_area_says_nothing_while_melodyne_analyses() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local said = #S.speech
        S.disc = true
        T.frame(300)
        T.measure()
        T.frame(330)
        T.measure()
        T.frame(360)
        T.measure()
        T.ok(S.snapKeys["note area"] == 3 and S.profiles == 0, "pictures, none of them profiled")
        T.ok(T.readsOf("time signature") == 0 and #S.speech == said, T.said(said + 1))
        T.look()
        T.measure()
        T.ok(S.snapKeys["note area"] == 3, "no picture while the analysis is known")
        T.ok(T.count("[melodyne] note area: gate closed (Melodyne is analysing)") == 1, T.dump())
        T.pass("Polyphonic Detection")
        S.disc = false
        T.look()
        T.look()
        T.frame(390)
        T.measure()
        T.ok(S.profiles == 1 and T.count("[melodyne] note area: baseline") == 1, T.dump())
        T.frame(420)
        T.measure()
        T.ok(not string.find(T.said(said + 1), "step", 1, true) and not string.find(T.said(said + 1), "beat", 1, true),
          "no move said: " .. T.said(said + 1))
    "#);
}

/// "Analysis", a static text at the end of the Tab ring with Alt+A: says what the looks found in
/// this stay — idle after the look made on arrival, busy before the name is taken, then the pass —
/// reading nothing itself. Alt+A moves the focus there, and Space stays Melodyne's.
#[test]
fn the_analysis_read_out_says_what_the_looks_found() {
    run(r#"
        local S = T.S
        local o = T.melodyne()
        local said = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, idle", "the look made on arrival: " .. T.said(said + 1))
        T.ok(o.controls[o.focus].label == "Analysis", "the focus moved there")
        T.ok(o.focus == #o.controls, "at the end of the Tab ring")
        T.ok(not S.holding["Space"] and not S.holding["Return"], "Space and Return stay Melodyne's")
        T.ok(S.speech[#S.speech].interrupt == true, "said interrupting, as a key's answer")
        local reads = #S.reads
        S.disc = true
        T.look()
        said = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, busy", T.said(said + 1))
        T.pass("Polyphonic Detection")
        said = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, busy", "one look's name is not taken: " .. T.said(said + 1))
        T.runDue()
        T.pass("Polyphonic Detection")
        said = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, Polyphonic Detection", T.said(said + 1))
        T.ok(#S.reads == reads + 2, "the read-out read nothing itself: " .. #S.reads)
    "#);
}

/// Back onto "Analysis" during an analysis — Alt+A left the focus there, and coming back resumes on
/// it. The look made on arrival, and the one its first name asks for at once, have their answers
/// before the arrival sentence asks the read-out, which says the pass; "Busy" would say it again,
/// and is not said.
#[test]
fn coming_back_onto_the_analysis_read_out_says_the_pass_once() {
    run(r#"
        local S = T.S
        local o = T.analysing("Polyphonic Detection")
        T.hotkey("Alt+A")
        T.show(T.OTHER)
        T.show(T.MEL)
        T.ok(o.active and o.controls[o.focus].label == "Analysis", "back, on the read-out")
        local pictures = S.snapKeys["analysis disc"]
        T.runDue()
        T.pass("Polyphonic Detection")
        T.runDue()
        T.pass("Polyphonic Detection")
        T.ok(S.snapKeys["analysis disc"] == pictures + 1,
          "the look made on arrival and the one its first name asked for, none by the clock")
        local said = #S.speech
        S.now += 400
        T.runDue()
        T.ok(T.said(said + 1) == "Analysis, Polyphonic Detection", T.said(said + 1))
        T.look()
        T.pass("Polyphonic Detection")
        T.look()
        T.ok(T.said(said + 1) == "Analysis, Polyphonic Detection", "no Busy after it: " .. T.said(said + 1))
    "#);
}

/// The overlay comes back during an analysis with the focus on "Inspector", whose sentence reads
/// its box and is said when that read answers. The pass name is taken first, and the next look
/// comes before the sentence: "Busy" waits for the sentence to be SAID, not only begun, and follows
/// it.
#[test]
fn busy_waits_for_an_arrival_sentence_that_reads_its_control() {
    run(r#"
        local S = T.S
        local o = T.analysing("Polyphonic Detection")
        local inspector = nil
        for i, c in ipairs(o.controls) do
          if c.label == "Inspector" then inspector = i end
        end
        T.show(T.OTHER)
        o.focus = inspector
        T.show(T.MEL)
        T.ok(o.active and o.focus == inspector, "back, on Inspector")
        T.runDue()
        T.pass("Polyphonic Detection")
        T.runDue()
        T.pass("Polyphonic Detection")
        local said = #S.speech
        -- The sentence's read is kept out for a while, as a slow one is (the host answers the
        -- runtime's reads at the end of every turn here).
        local deliver = T.deliver
        T.deliver = function() end
        S.now += 400
        T.runDue()
        T.ok(#S.focusReads == 1, "the arrival sentence reads the box: " .. T.dump())
        T.look()
        T.ok(#S.speech == said, "nothing ahead of the sentence still reading: " .. T.said(said + 1))
        T.deliver = deliver
        T.deliver()
        T.ok(string.find(T.said(said + 1), "^Inspector") ~= nil, T.said(said + 1))
        T.look()
        T.ok(S.speech[#S.speech].text == "Busy: Polyphonic Detection" and #S.speech == said + 2,
          "after the sentence: " .. T.said(said + 1))
    "#);
}

/// Under a tool with no read-out the read-outs are silent from the picture, not from the recogniser:
/// a box that is not drawn — under Time, where the recogniser reads nothing — or one drawn with
/// nothing in it — Fade's — holds no value, so the note area is measured and a move is said. An
/// empty read changed nothing before, and after a note selected under Pitch the gate stayed shut.
/// A value in the box again, read or dropped by the recogniser, ends the silence.
#[test]
fn under_a_tool_with_no_read_out_the_note_area_speaks() {
    run(r#"
        local S = T.S
        T.watching("A4", "+12 ct")
        T.frame(300)
        T.measure()
        T.ok(S.profiles == 0, "the read-outs speak")
        S.leftBox, S.rightBox = false, false
        T.watch()
        T.ok(#T.lastRead("selection").regions == 1, "the right box is read only when it is drawn")
        T.boxes("")
        local said = #S.speech
        T.ok(T.moveRight() == "Ran", "measured")
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.ok(T.said(said + 1) == "right, one beat", T.said(said + 1))

        S.leftBox = true
        T.shows("empty")
        T.watch()
        T.boxes("")
        T.frame(390)
        T.measure()
        T.ok(S.profiles == 3, "still measured with Fade's empty box: " .. S.profiles)
        T.shows("value")
        T.watch()
        T.boxes("")
        T.frame(420)
        T.measure()
        T.ok(S.profiles == 3, "a value in the box, dropped by the recogniser, is not silence: " .. S.profiles)
    "#);
}

/// The watcher reads the boxes from one picture taken off the event loop — their borders, what each
/// holds and the text — and touches no live pixel on its tick, where which boxes are drawn was two
/// points of the screen every half second. What the picture showed is what the Tab ring asks.
#[test]
fn the_watcher_reads_the_boxes_from_one_picture_taken_off_the_event_loop() {
    run(r#"
        local S = T.S
        S.rightBox = true
        T.melodyne()
        local live = S.pixels
        for _ = 1, 10 do
          S.now += 120
          T.watch()
          T.boxes("A4", "+12 ct")
        end
        T.ok(S.pixels == live, "no live pixel on the watcher's ticks: " .. (S.pixels - live))
        T.ok(S.snapKeys["selection"] == 10 and S.inkProfiles == 20, "one picture a tick, both boxes measured")
        S.rightBox = false
        S.now += 120
        T.watch()
        T.ok(#T.lastRead("selection").regions == 1, "the right box gone from the picture is not read")
        T.boxes("0.00 dB")
        T.ok(S.pixels == live, "nor a live pixel to see it go")
    "#);
}

/// In a window wide enough that part of the disc lies past the note area's right edge — 958 px in —
/// the note area's picture holds the disc's place too, and a frame with the disc in it is still not
/// profiled. A picture of the note area alone missed the disc's points, and the frame was compared.
#[test]
fn in_a_wide_window_a_frame_with_the_disc_in_it_is_not_compared() {
    run(r#"
        local S = T.S
        T.MEL.client.w, T.MEL.bounds.w = 1900, 1916
        T.melodyne()
        T.silent()
        S.disc = true
        T.frame(300)
        T.measure()
        T.measure()
        T.ok(S.snapKeys["note area"] == 2 and S.profiles == 0, "pictures, none of them profiled: " .. S.profiles)
    "#);
}

// ---------------------------------------------------------------------------------------------
// The note area's steps, and the pictures it profiles (2026-10-06).
// ---------------------------------------------------------------------------------------------

/// At a bar of 80 px in 4/4 a beat is 20 px. A grid step there, drawn thin — five rows of ink, under
/// the eight the coarse threshold needs, as the notes of the live session were — is "one beat",
/// where it was "one fine step" or nothing; a nudge of 2 px is "one fine step"; and a step back the
/// other way is a beat again.
#[test]
fn at_a_bar_of_80_px_a_grid_step_is_one_beat_and_a_nudge_one_fine_step() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local thin = { bar = 80, ink = 24 }
        T.ok(T.to(300, thin) == "", "the baseline")
        T.ok(T.to(320, thin) == "right, one beat", "a grid step of 20 px: " .. T.said())
        T.ok(T.to(340, thin) == "right, one beat", T.said())
        T.ok(T.to(342, thin) == "right, one fine step", "a nudge of 2 px: " .. T.said())
        T.ok(T.to(322, thin) == "left, one beat", "back: " .. T.said())
        T.ok(T.readsOf("time signature") == 1, "the time signature read once")
    "#);
}

/// At a bar of 130 px in 4/4 a beat is 32.5 px: a move of 34 px is "one beat", twice, and a nudge
/// of 2 px after them is "one fine step".
#[test]
fn at_a_bar_of_130_px_a_move_of_34_px_is_one_beat() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local bars = { bar = 130 }
        T.to(300, bars)
        T.ok(T.to(334, bars) == "right, one beat", T.said())
        T.ok(T.to(368, bars) == "right, one beat", T.said())
        T.ok(T.to(370, bars) == "right, one fine step", T.said())
        T.ok(T.to(302, bars) == "left, 2 beats", "68 px back: " .. T.said())
    "#);
}

/// A note wider than its move: the two halves of the move are the note's width apart, and the move
/// is their width — 20 px of a 30 px note is a beat at a bar of 80 px, where the distance between
/// the halves, 30 px, was past the fine pass's 12 px and said nothing. What tells the two apart is
/// the note's body between the halves, in the note's colour on the note's own rows, read from the
/// frame's picture and the one before it. A nudge after it is a fine step, and a step back the
/// other way is a beat again; a note narrower than its move has the page between its halves.
#[test]
fn a_note_wider_than_its_move_is_measured_by_the_halves_width_not_their_distance() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local long = { bar = 80, width = 30 }
        T.to(300, long)
        T.ok(T.to(320, long) == "right, one beat", T.said())
        T.ok(S.rowProfiles == 3, "the note's rows in both pictures, and the columns between: " .. S.rowProfiles)
        T.ok(T.to(323, long) == "right, one fine step", T.said())
        T.ok(T.to(303, long) == "left, one beat", T.said())
        T.ok(T.to(343, long) == "right, 2 beats", "40 px of a 30 px note: " .. T.said())
    "#);
}

/// A move whose pictures cannot say whether the halves are the move or the note — a note drawn
/// without colour — is said by what both readings agree on, and by its direction alone when they do
/// not: never a guess.
#[test]
fn a_move_the_pictures_cannot_size_is_said_by_what_both_readings_agree_on() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local grey = { bar = 80, width = 30, grey = true }
        T.to(300, grey)
        T.ok(T.to(320, grey) == "right", "20 px or the note's 30: one beat or two, so neither: " .. T.said())
    "#);
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local grey = { bar = 80, width = 20, grey = true }
        T.to(300, grey)
        T.ok(T.to(320, grey) == "right, one beat", "the halves touch, 20 px either way: " .. T.said())
    "#);
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local short = { bar = 80, width = 10, grey = true }
        T.to(300, short)
        T.ok(T.to(320, short) == "right, one beat", "20 px or the note's 10: one beat either way: " .. T.said())
    "#);
}

/// Halves that overlap are more than one note moving — here two notes moved towards each other —
/// and their centroids say nothing about which way: the ink's flow decides, and where it comes to
/// less than a pixel, as here, the sentence is "moved", never a direction or a size it cannot know.
#[test]
fn a_change_that_is_no_rigid_move_is_said_as_moved() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        T.to(300, { bar = 80, also = { 330 } })
        T.ok(T.to(305, { bar = 80, also = { 325 } }) == "moved", T.said())
    "#);
}

/// A frame in which the cursor moved is the selection walking from note to note, or the playhead,
/// not an edit — the cursor follows a selection and not an edit — and its change is said nothing,
/// though a note's highlight moved with it. The next frame, the cursor where it was, is measured.
#[test]
fn a_frame_in_which_the_cursor_moved_is_no_edit() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        T.to(300, { bar = 80, cursor = 100 })
        T.ok(T.to(320, { bar = 80, cursor = 400 }) == "", "the selection moved: " .. T.said())
        T.ok(T.to(340, { bar = 80, cursor = 400 }) == "right, one beat", T.said())
    "#);
}

/// A frame with half a move in it — the note gone from its old place and not drawn yet in the new
/// one — says nothing, and the next frame is compared with the one before it: the whole move, once.
/// A change that is still half at the next frame is let go, and the move after it is measured from
/// there.
#[test]
fn half_a_move_is_measured_with_the_next_frame() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local bars = { bar = 80 }
        T.to(300, bars)
        T.ok(T.to(nil, bars) == "", "half a move says nothing: " .. T.said())
        T.ok(T.to(320, bars) == "right, one beat", "the whole move, from the frame before: " .. T.said())
        T.ok(T.to(nil, bars) == "" and T.to(nil, bars) == "", T.said())
        T.ok(T.to(340, bars) == "" and T.to(340, bars) == "", "still half: let go, " .. T.said())
        T.ok(T.to(360, bars) == "right, one beat", T.said())
    "#);
}

/// A picture of the note area is asked to be compared with the one its last profile was made from —
/// exactly: no tolerance, one pixel — and one in which nothing changed is let go unprofiled. A changed
/// one is profiled and kept in its place, the one before let go; when the gate closes the kept one
/// is let go too, and the next picture has nothing to be compared with.
#[test]
fn a_picture_in_which_nothing_changed_is_not_profiled() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        T.frame(300)
        T.measure()
        T.ok(#S.changeWaits == 0, "the first picture has nothing to be compared with")
        T.measure()
        T.measure()
        T.ok(S.snapKeys["note area"] == 3 and S.profiles == 1, "the same picture twice more, not profiled: " .. S.profiles)
        local w = S.changeWaits[1]
        T.ok(#S.changeWaits == 2 and w.from ~= nil and S.changeWaits[2].from == w.from and w.tolerance == 0
          and w.minPixels == 1 and w.timeout == 1, "compared with the picture profiled, exactly")
        T.frame(330)
        T.measure()
        T.ok(S.profiles == 2 and w.from.released, "a changed one is profiled, and the picture before let go")
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.measure()
        local kept = S.changeWaits[#S.changeWaits].from
        T.ok(S.profiles == 2 and kept ~= w.from and not kept.released,
          "compared with the one profiled now: " .. S.profiles)
        T.shows("value")
        T.watch()
        T.boxes("A4")
        T.measure()
        T.ok(kept.released, "the kept picture let go when the gate closes")
        local waits = #S.changeWaits
        T.silent()
        T.measure()
        T.ok(#S.changeWaits == waits and S.profiles == 3, "the next picture has nothing to be compared with")
    "#);
}

/// The note area's profile is reduced off the event loop: asked with a callback under the frame's
/// key from the picture's callback, and the frame is measured once it answers — until then the
/// next tick asks for no picture. A menu opened, or Melodyne left the front, between the picture
/// and its profile's answer: the frame is let go unmeasured.
#[test]
fn the_note_areas_profile_is_reduced_off_the_event_loop_one_frame_at_a_time() {
    run(r#"
        local S = T.S
        local o = T.melodyne()
        T.silent()
        T.frame(300)
        T.ok(T.call("timer", S.polls[200]) == "Ran", "the tick does not wait")
        T.ok(S.snapKeys["note area"] == 1 and #S.reductions == 0, "a picture asked, nothing reduced yet")
        -- The picture's answer alone, as on the host's next turn: its profile is asked, and out.
        T.snapOf("note area")
        T.ok(#S.reductions == 1 and S.reductions[1].key == "note area" and S.profiles == 1,
          "the profile is asked with a callback, under the frame's key")
        T.ok(T.count("[melodyne] note area: baseline") == 0, "the frame waits for its profile")
        T.ok(T.host.screen.pending("note area"), "and is out")
        T.call("timer", S.polls[200])
        T.ok(S.snapKeys["note area"] == 1, "no picture while the frame is out")
        T.runDue()
        T.ok(T.count("[melodyne] note area: baseline") == 1 and not T.host.screen.pending("note area"), T.dump())
        -- A menu opens between the picture and its profile's answer: the frame is let go
        -- unmeasured. Measured, its move would be said, and would ask for the time signature.
        T.frame(330)
        T.call("timer", S.polls[200])
        T.snapOf("note area")
        T.ok(#S.reductions == 1, "the second frame's profile is asked")
        local said = #S.speech
        S.native = true
        T.runDue()
        T.ok(#S.speech == said and T.readsOf("time signature") == 0,
          "a frame answered while a menu is open is not measured: " .. T.said(said + 1))
        -- The menu closed, the move is measured against the frame before it.
        S.native = false
        T.measure()
        T.ok(T.readsOf("time signature") == 1, "measured once the menu is closed")
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.ok(T.said(said + 1) == "right, one beat", T.said(said + 1))
        -- Melodyne leaves the front between the picture and its profile: let go, unmeasured, and
        -- not taken for a baseline either.
        T.frame(360)
        T.call("timer", S.polls[200])
        T.snapOf("note area")
        T.ok(#S.reductions == 1, "the fourth frame's profile is asked")
        said = #S.speech
        local baselines = T.count("[melodyne] note area: baseline")
        T.show(T.OTHER)
        T.ok(not o.active, "another application in front")
        T.runDue()
        T.ok(#S.speech == said and T.count("[melodyne] note area: baseline") == baselines,
          "a frame of a Melodyne that left is not measured: " .. T.said(said + 1) .. "\n" .. T.dump())
    "#);
}

/// While calibrating, every frame's measurement is in the log — the move, how its size was chosen,
/// both halves, the bar and the correlation's second opinion, then the unit it was said against —
/// and the runs, with the transport read beside them by a read with a callback.
#[test]
fn while_calibrating_the_log_says_how_each_move_was_measured() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.calibrate(true)
        T.silent()
        local bars = { bar = 80 }
        T.to(300, bars)
        T.ok(T.to(320, bars) == "right, one beat", T.said())
        T.ok(T.count("[melodyne] move: right 20.0 px, by the page between the halves; 10 arrived at x367-376,"
          .. " 10 left at x347-356; at half their peak 10 at x367-376 and 10 at x347-356; bar 80 px;"
          .. " correlation 20 px") == 1, T.dump())
        T.ok(T.count("[melodyne] move of 20.0 px, against a unit of 20.0 px, bar 80 px, time signature 4") == 1,
          T.dump())
        local r = T.lastRead("transport")
        T.ok(r ~= nil and not r.answered, "the transport is read with a callback")
        T.answer(r, T.reading("1.2.1"))
        T.ok(T.count('[melodyne] note area: 2 run(s) moved, strongest x') == 1
          and T.count('transport="1.2.1"') == 1, T.dump())
        T.to(nil, bars)
        T.ok(T.count("[melodyne] move: nothing (0 arrived, 10 left); the next frame is compared"
          .. " with the one before") == 1, T.dump())
    "#);
}

/// The pass name is read from crops about the disc's centre as its rim puts it, not about half the
/// window: a disc drawn 2 px right of the middle is read 2 px further right, and the log says how
/// far the centre was from the middle.
#[test]
fn the_pass_name_is_read_about_the_discs_measured_centre() {
    run(r#"
        local S = T.S
        S.disc, S.discDx = true, 2
        T.melodyne()
        local r = T.lastRead("analysis pass")
        T.ok(#r.regions == 2 and T.region(r.regions[1]) == "460,371,560,384"
          and T.region(r.regions[2]) == "461,371,560,384", "about the centre 2 px right: " .. T.region(r.regions[1]))
        T.pass("Polyphonic Detection")
        T.runDue()
        T.pass("Polyphonic Detection")
        T.ok(T.count("the disc's centre +2 px across from the window's middle)") == 1, T.dump())
    "#);
}

/// A name in Latin letters with accents, as a translated Melodyne may show, is taken — Luau's %a
/// knows no letter past ASCII — and one with a symbol in it is not.
#[test]
fn a_pass_name_with_accented_letters_is_taken_and_one_with_a_symbol_is_not() {
    run(r#"
        local S = T.S
        local o, arrived = T.analysing("Détection polyphonique")
        T.ok(T.said(arrived + 1) == "Busy: Détection polyphonique", T.said(arrived + 1))
    "#);
    run(r#"
        local S = T.S
        S.disc = true
        T.melodyne()
        T.pass("Polyphonic • Detection")
        T.look()
        T.pass("Polyphonic • Detection")
        T.ok(T.count("[melodyne] analysis found") == 0, "not letters: " .. T.dump())
    "#);
}

/// The pass is read in English. A read answered "failed" with no language, where English is not
/// among the recogniser's languages, turns the reads to the user's own language; one where English
/// is there — a read refused, say — leaves them in English, and so does one answered before the
/// language list is known, when `resolveLanguage` answers nil for every language.
#[test]
fn where_english_is_not_read_the_pass_is_read_in_the_users_language() {
    run(r#"
        local S = T.S
        S.disc = true
        local english, known = "en-US", { "de-DE", "en-US" }
        rawset(T.host.ocr, "languages", function() return table.clone(known) end)
        rawset(T.host.ocr, "resolveLanguage", function(lang)
          return #known > 0 and lang == "en" and english or nil
        end)
        T.melodyne()
        local function failed()
          local r = T.reading("", "failed")
          r.lang = ""
          return r
        end
        local r = T.lastRead("analysis pass")
        T.ok(r.lang == "en", "in English")
        T.answer(r, failed(), failed())
        T.look()
        T.ok(T.lastRead("analysis pass").lang == "en", "English is there: still in English")
        english, known = nil, {}
        T.answer(T.lastRead("analysis pass"), failed(), failed())
        T.look()
        T.ok(T.lastRead("analysis pass").lang == "en", "the list not known yet: still in English")
        known = { "de-DE" }
        T.answer(T.lastRead("analysis pass"), failed(), failed())
        T.ok(T.count("[melodyne] analysis pass: English is not read here; the user's language from now on")
          == 1, T.dump())
        T.look()
        T.ok(T.lastRead("analysis pass").lang == nil, "the user's language")
    "#);
}

/// Melodyne scrolls under a move — the bar lines, full height, move with the music — and the move
/// is measured against the view's own: a beat, though the note moved 45 px on the screen. A scroll
/// alone is no move. The bar lines' columns are no cursor; they silenced the edit.
#[test]
fn a_move_that_made_melodyne_scroll_is_measured_against_the_views_move() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local bars = { bar = 300 }
        T.to(300, bars)
        T.ok(T.to(375, bars) == "right, one beat", T.said())
        T.ok(T.to(420, { bar = 300, shift = -30 }) == "right, one beat",
          "a beat, the view 30 px left under it: " .. T.said())
        T.ok(T.to(390, { bar = 300, shift = -60 }) == "", "the view alone moved: " .. T.said())
    "#);
}

/// A note whose ink thins towards its ends is measured at half the change's peak: at three rows
/// its halves take in the taper, 32 columns for a move of 20, and the move was said as more than a
/// beat.
#[test]
fn a_note_that_tapers_is_measured_at_half_the_peak_of_its_change() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local shape = {}
        for j = 0, 59 do
          shape[#shape + 1] = 19 + math.floor(21 * math.min(j + 1, 60 - j, 16) / 16 + 0.5)
        end
        local tapered = { bar = 80, shape = shape }
        T.to(300, tapered)
        T.ok(T.to(320, tapered) == "right, one beat", T.said())
        T.ok(T.to(300, tapered) == "left, one beat", T.said())
    "#);
}

/// A picture like the one before a held frame lets the hold go, as a profile of it would: the next
/// half move is held again, and the move it is half of is said.
#[test]
fn a_picture_like_the_one_before_a_held_frame_lets_the_hold_go() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local bars = { bar = 80 }
        T.to(300, bars)
        T.ok(T.to(nil, bars) == "", "half a move: held")
        T.ok(T.to(300, bars) == "" and S.profiles == 2, "the picture before it, not profiled: " .. S.profiles)
        T.ok(T.to(nil, bars) == "", "half a move again: held")
        T.ok(T.to(320, bars) == "right, one beat", "the whole move: " .. T.said())
    "#);
}

/// The step he has been making is kept as a part of the bar, so it follows the zoom at once: with
/// no time signature, a step of 20 px learned at a bar of 80 is one step of 30 at 120 too, where in
/// pixels it was "2 steps". A grid of eighths in 4/4 is one step at every press, where it flipped
/// between a fine step and a beat; with no time signature, moves of 30 to 35 px at a bar of 100
/// are one step each, where a fraction of each move flipped between a quarter and a third. Whole
/// beats are beats, whatever was learned: a first move of two beats is "2 beats", and a beat after
/// it "one beat".
#[test]
fn the_step_follows_the_zoom_and_a_press_is_one_step_every_time() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        T.to(300, { bar = 80 }, "-")
        T.ok(T.to(320, { bar = 80 }, "-") == "right, one step", T.said())
        T.show(T.OTHER)
        T.show(T.MEL)
        T.silent()
        T.to(300, { bar = 120 }, "-")
        T.ok(T.to(330, { bar = 120 }, "-") == "right, one step", "zoomed in, 30 px: " .. T.said())
    "#);
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local bars = { bar = 80 }
        T.to(300, bars)
        local said = {}
        for _, at in ipairs({ 310, 320, 331, 340 }) do said[#said + 1] = T.to(at, bars) end
        T.ok(table.concat(said, " | ") == "right, one step | right, one step | right, one step | right, one step",
          table.concat(said, " | "))
    "#);
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local bars = { bar = 100 }
        T.to(300, bars, "-")
        local said = {}
        for _, at in ipairs({ 333, 367, 397, 428, 463, 493 }) do said[#said + 1] = T.to(at, bars, "-") end
        T.ok(table.concat(said, " | ") == string.rep("right, one step | ", 5) .. "right, one step",
          table.concat(said, " | "))
    "#);
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local bars = { bar = 80 }
        T.to(300, bars)
        T.ok(T.to(340, bars) == "right, 2 beats", T.said())
        T.ok(T.to(360, bars) == "right, one beat", T.said())
        T.ok(T.to(363, bars) == "right, one fine step", T.said())
    "#);
}
