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
//! The scenarios answer the module's reads by hand, with what Melodyne would show, and hand the
//! note-area measurement column profiles made up for it (`T.frame`): what they check is the
//! module's decisions, not Melodyne.

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

-- A point of the screen as host.screen.pixel reads it: with S.rightBox, the right read-out box is
-- drawn — its left border (298, 67 in the client area) dark against the gap beside it (294, 67) —
-- and every other point is the harness's S.pixel, which lights no tool and draws no box.
rawset(T.host.screen, "pixel", function(x, y)
  S.pixels += 1
  local cx, cy = x - T.MEL.client.x, y - T.MEL.client.y
  if S.rightBox and cy == 67 then
    if cx == 298 then return { r = 150, g = 150, b = 150 } end
    if cx == 294 then return { r = 200, g = 200, b = 200 } end
  end
  return S.pixel
end)

-- The note area's column profile, as host.screen.profile hands it back: the frame T.frame made.
rawset(T.host.screen, "profile", function(opts)
  S.profiles += 1
  S.profileRegion = opts.region
  return S.frame
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

-- Melodyne's two polls, run as the host runs a host.timer.every: what became of the handler.
function T.watch() return T.call("timer", S.polls[120]) end
function T.measure() return T.call("timer", S.polls[200]) end

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
/// takes no picture — and the next tick asks again. A read that answered both boxes empty is the
/// read-outs falling silent, and the note area starts measuring.
#[test]
fn a_failed_read_changes_nothing_and_an_empty_one_is_silence() {
    run(r#"
        local S = T.S
        T.watching("A4", "+12 ct")
        T.frame(300)
        T.measure()
        T.ok(S.profiles == 0, "the read-outs speak, so the note area is not measured")
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
        T.ok(S.profiles == 1, "an empty read is silence: the note area is measured")
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
    "#);
}

/// Within one input epoch the time signature is read once: the next move is said at once, with no
/// read. A new input epoch reads it again.
#[test]
fn the_time_signature_is_read_once_per_input_epoch() {
    run(r#"
        local S = T.S
        T.melodyne()
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
