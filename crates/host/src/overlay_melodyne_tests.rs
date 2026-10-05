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

-- A point of a picture of the editor: Melodyne's light page, and with S.disc the analysis disc as
-- calibration/Melodyne-clean.png shows it, centred 4 px above the middle of the client area — a
-- rim from 50.5 to 53.5 px out reading 120, the filled pie inside it 188, and the caption band
-- across it, 13 rows, halving what is under it.
function T.picture(x, y)
  local strip = T.strip(x - T.MEL.client.x, y - T.MEL.client.y)
  if strip then return strip end
  local function grey(v) return { r = v, g = v, b = v } end
  if not S.disc then return grey(250) end
  local dx = x - (T.MEL.client.x + T.MEL.client.w // 2)
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

-- host.screen.profile, from a picture only: the rows of a read-out box, from T.picture, or the note
-- area's columns — the frame T.frame made. The region must lie inside the picture.
rawset(T.host.screen, "profile", function(opts)
  local snap, r = opts.snapshot, opts.region
  assert(snap ~= nil and not snap.released, "profiled from its own picture")
  local p = snap.region
  assert(r and r[1] >= p[1] and r[2] >= p[2] and r[3] <= p[3] and r[4] <= p[4],
    "the region lies inside the picture")
  if opts.axes == "rows" then
    local mins, maxs = {}, {}
    for y = r[2], r[4] - 1 do
      local lo, hi = 255, 0
      for x = r[1], r[3] - 1 do
        local v = T.picture(x, y).r
        lo, hi = math.min(lo, v), math.max(hi, v)
      end
      mins[#mins + 1], maxs[#maxs + 1] = lo, hi
    end
    S.inkProfiles += 1
    return { x = r[1], y = r[2], w = r[3] - r[1], h = r[4] - r[2], rows = { min = mins, max = maxs } }
  end
  assert(opts.axes == "columns" and opts.dark == 190, "columns, dark below 190")
  S.profiles += 1
  S.profileRegion = r
  return S.frame
end)

-- host.screen.snapshotAsync as the harness takes it, at its time and answered by T.runDue, counted
-- per key; a newer request with a key takes the place of an older one not answered yet, whose
-- callback is then never called here (the host answers it nil, which the module drops alike).
local snapshotAsync = T.host.screen.snapshotAsync
rawset(T.host.screen, "snapshotAsync", function(opts, cb)
  for k in pairs(opts) do
    assert(k == "region" or k == "key" or k == "at", "snapshotAsync: an option " .. tostring(k))
  end
  assert(type(opts.at) == "number", "asked at a set time, so Melodyne's own clicks do not wait for it")
  S.snapKeys[opts.key] = (S.snapKeys[opts.key] or 0) + 1
  for i = #S.asyncs, 1, -1 do
    if S.asyncs[i].key == opts.key then table.remove(S.asyncs, i) end
  end
  snapshotAsync(opts, cb)
  S.asyncs[#S.asyncs].key = opts.key
end)

-- A frame of the note area, 910 columns: a light page (mean 200) with a bar line every 120
-- columns, at 50, 170, 290 and on (mean 150), about 19 dark rows a column, and a note's blob of
-- 40 dark rows in the ten columns from `at`.
function T.frame(at)
  local mean, dark = {}, {}
  for i = 1, 910 do
    mean[i] = (i % 120 == 50) and 150 or 200
    dark[i] = (i >= at and i < at + 10) and 40 or 19
  end
  S.frame = { columns = { mean = mean, dark = dark } }
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

-- Melodyne loaded with the disc up, its arrival sentence said, then a look and its pass name read:
-- "Busy: <pass>" has been said. The overlay comes back, and how much had been said before the look.
function T.analysing(pass)
  S.disc = true
  local o = T.melodyne()
  S.now += 400
  T.runDue()
  local arrived = #S.speech
  T.look()
  T.answer(T.lastRead("analysis pass"), T.reading(pass))
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

-- Three frames of the note area, the note's blob at 300, 330 and 360: the first is the baseline,
-- the second the first arrival, and the third a move of 30 columns to the right, which asks for
-- the time signature unless this input epoch has read it.
function T.moveRight()
  T.frame(300)
  T.measure()
  T.frame(330)
  T.measure()
  T.frame(360)
  return T.measure()
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
/// ends it.
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
        T.ok(S.profiles == 2, "an empty read of dashes is silence still")
        T.shows("value")
        T.watch()
        T.boxes("B4", "")
        T.measure()
        T.ok(S.profiles == 2, "a value ends it")
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
/// no frame is taken while the read is out. Its answer says the move, in beats.
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
        T.ok(S.profiles == 3, "three frames")
        T.frame(390)
        T.measure()
        T.ok(S.profiles == 3, "no frame while the time signature is out")
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
        T.frame(390)
        T.measure()
        T.ok(T.readsOf("time signature") == 1, "no second read in the same input epoch")
        T.ok(T.said(said + 1) == "right, one beat | right, one beat", T.said(said + 1))
        S.inputEpoch += 1
        T.frame(420)
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

/// A time-signature read that failed says the move without beats and leaves the epoch unread, so
/// the next move asks again; one that never answered — dropped, as when the module is disabled —
/// is not pending any more and is not taken as read either.
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
        T.frame(390)
        T.measure()
        T.ok(T.readsOf("time signature") == 2, "asked again after a failed read")
        T.drop(T.lastRead("time signature"))
        T.frame(420)
        T.measure()
        T.ok(T.readsOf("time signature") == 3, "asked again after a read that never answered")
        T.answer(T.lastRead("time signature"), T.reading("4/4"))
        T.ok(T.said(said + 1) == "right, one step | right, one beat", T.said(said + 1))
    "#);
}

/// A frame is compared only with one of the same stay. Away and back with no tick in between — so
/// no closed gate was seen on the way — and the read-outs silent again in the new stay: its first
/// frame is a baseline, not a move measured against a picture of the last stay. Away and back once
/// more, the last stay's silence is not this one's, and no picture is taken until a read of this
/// stay. And within a stay, a frame from before the gate closed is gone once it opens again.
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
        T.ok(#S.speech == said, "the first arrival of the stay is no step: " .. T.said(said + 1))

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
        T.watch()
        T.boxes("A4")
        T.measure()
        T.ok(T.count('[melodyne] note area: gate closed (the read-outs are not silent), read-outs "A4"|""') == 1, T.dump())
        T.silent()
        said = #S.speech
        T.frame(450)
        T.measure()
        T.ok(T.count("[melodyne] note area: baseline") == 4, "after the gate was closed, a baseline: " .. T.dump())
        T.ok(#S.speech == said, T.said(said + 1))
    "#);
}

/// The view moved — Melodyne scrolled — and the note area re-baselines: the note's last position
/// goes with the old picture. The first arrival after it is measured against nothing, where it
/// used to be measured against a place in the old view and said as a move of that size.
#[test]
fn after_the_view_moved_the_notes_last_position_is_forgotten() {
    run(r#"
        local S = T.S
        T.melodyne()
        T.silent()
        local said = #S.speech
        T.frame(300)
        T.measure()
        T.frame(330)
        T.measure()
        T.frameMany()
        T.measure()
        T.ok(T.count("the view moved, re-baselining") == 1, T.dump())
        T.frame(600)
        T.measure()
        T.frame(630)
        T.measure()
        T.ok(T.readsOf("time signature") == 0 and #S.speech == said,
          "no move measured against the old view: " .. T.said(said + 1))
    "#);
}

/// Every hundred frames the log says how many and what they cost the event loop.
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
        T.ok(T.count("[melodyne] note area: 100 frames, 0 ms on the event loop for them (0.0 each), "
          .. "0.0 ms from asking to the answer on average") == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The analysis (2026-10-05).
// ---------------------------------------------------------------------------------------------

/// Melodyne is analysing as the overlay arrives. The look made on arrival finds the disc in its own
/// picture, taken at a set time off the event loop, and reads the pass name from that picture with
/// a callback; the answer comes before the overlay's arrival sentence, so "Busy" waits, and is said
/// — queued, not interrupting — on the first look after the arrival. Once.
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
        T.ok(T.region(r.region) == "456,371,561,384", "the caption band and two columns of the rim: "
          .. T.region(r.region))
        T.answer(r, T.reading("Polyphonic Detection"))
        T.ok(#S.speech == before, "not before the overlay's own sentence: " .. T.said(before + 1))
        T.ok(T.count('[melodyne] analysis found: "Polyphonic Detection"') == 1, T.dump())
        S.now += 400
        T.runDue()
        local arrived = #S.speech
        T.ok(arrived == before + 1, "the overlay arrived: " .. T.said(before + 1))
        T.ok(T.look() == "Ran", "the look does not wait")
        T.ok(T.said(arrived + 1) == "Busy: Polyphonic Detection", T.said(arrived + 1))
        T.ok(S.speech[#S.speech].interrupt == false, "queued behind the arrival, not interrupting it")
        T.answer(T.lastRead("analysis pass"), T.reading("Polyphonic Detection"))
        T.look()
        T.answer(T.lastRead("analysis pass"), T.reading("Polyphonic Detection"))
        T.ok(T.said(arrived + 1) == "Busy: Polyphonic Detection", "said once: " .. T.said(arrived + 1))
        T.ok(T.count("[melodyne] analysis found") == 1, T.dump())
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
          T.answer(T.lastRead("analysis pass"), T.reading(""))
        end
        T.look()
        T.answer(T.lastRead("analysis pass"), T.reading("", "failed"))
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
        T.answer(T.lastRead("analysis pass"), T.reading("Polyphonic Detection"))
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
/// this stay — idle after the look made on arrival, busy before the name is read, then the pass —
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
        T.answer(T.lastRead("analysis pass"), T.reading("Polyphonic Detection"))
        said = #S.speech
        T.hotkey("Alt+A")
        T.ok(T.said(said + 1) == "Analysis, Polyphonic Detection", T.said(said + 1))
        T.ok(#S.reads == reads + 1, "the read-out read nothing itself: " .. #S.reads)
    "#);
}

/// Back onto "Analysis" during an analysis — Alt+A left the focus there, and coming back resumes on
/// it. The look made on arrival has its answer before the arrival sentence asks the read-out, which
/// says the pass; "Busy" would say it again, and is not said.
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
        T.answer(T.lastRead("analysis pass"), T.reading("Polyphonic Detection"))
        T.ok(S.snapKeys["analysis disc"] == pictures, "the look was made on arrival, not by the clock")
        local said = #S.speech
        S.now += 400
        T.runDue()
        T.ok(T.said(said + 1) == "Analysis, Polyphonic Detection", T.said(said + 1))
        T.look()
        T.answer(T.lastRead("analysis pass"), T.reading("Polyphonic Detection"))
        T.look()
        T.ok(T.said(said + 1) == "Analysis, Polyphonic Detection", "no Busy after it: " .. T.said(said + 1))
    "#);
}

/// The overlay comes back during an analysis with the focus on "Inspector", whose sentence reads
/// its box and is said when that read answers. The pass name is read first, and the next look comes
/// before the sentence: "Busy" waits for the sentence to be SAID, not only begun, and follows it.
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
        T.answer(T.lastRead("analysis pass"), T.reading("Polyphonic Detection"))
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
        T.ok(S.profiles == 4, "still measured with Fade's empty box: " .. S.profiles)
        T.shows("value")
        T.watch()
        T.boxes("")
        T.frame(420)
        T.measure()
        T.ok(S.profiles == 4, "a value in the box, dropped by the recogniser, is not silence: " .. S.profiles)
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
