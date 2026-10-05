//! The overlay runtime's Tab pass-through, run for real against the scripted host of
//! `overlay_menu_tests.rs`: an unnamed stop in the plug-in's own ring is read off the screen.
//!
//! A Mac tester's log of Kontakt 7 standalone (2026-09-27) had the pass-through's ring mostly
//! made of stops with no name — `[passthrough] Kontakt 7: Tab -> stop 15 of 29 '', …` — so the
//! screen reader announced a role and little else. The maintainer's decisions of that day, which
//! these scenarios hold the runtime to: `host.element.focusStep` says where the element it
//! landed on is (`bounds`); a stop whose name has no visible character is read in that rectangle
//! with `host.ocr.recognize`, one key per overlay; the first two rows are spoken queued, not cutting
//! off the screen reader's own announcement (`interrupt = false`), and only while the answer is
//! still about the stop in front of the user — not superseded, the overlay active and not out of
//! the front since, the pass-through focused and inside the plug-in, no later key on it; every
//! outcome is one `[passthrough]` line, with the stop's control type; a named stop is left to the
//! screen reader; `readUnnamed = false` turns it off; and no timer decides any of it.
//!
//! The overlay is Kontakt 7 standalone's shape: one control of its own, then the pass-through.
//! `T.passThrough` builds it and Tabs onto the pass-through stop; `T.tab()` then steps into the
//! plug-in, `T.shiftTab()` back, and `T.answer` delivers a read's answer as the host does, on a
//! later turn of the loop.

use super::overlay_menu_tests::run_with;

/// The pass-through's helpers, added to the harness's `T`.
const PT: &str = r##"
local S = T.S

-- The plug-in window is S.origin: bounds 92,19 816x639. Every stop below lies inside it.
T.UNNAMED = { x = 300, y = 200, w = 120, h = 24 }

-- An overlay over Kontakt 7 standalone: its own "Kontakt file menu", then the pass-through
-- (`opts`, default { label = "Kontakt controls" }), over the plug-in ring `ring`. It comes to the
-- front on its first control and Tab takes it onto the pass-through stop, which says so.
function T.passThrough(ring, opts)
  S.ring, S.ringAt = ring, 0
  local o = T.O.new("Kontakt 7")
  o:addCustomButton({ label = "Kontakt file menu", hotkey = "Alt+F", onActivate = function() end })
  o:addPassThrough(opts or { label = "Kontakt controls" })
  T.front(o)
  T.tab()
  assert(o.focus == 2, "Tab reaches the pass-through stop")
  local last = S.speech[#S.speech]
  assert(last and last.text == "Kontakt controls, Tab to step through them", "the stop announces itself")
  return o
end

-- Shift+Tab, as the captured key delivers it.
function T.shiftTab()
  assert(S.holding["Shift+Tab"], "Shift+Tab is not captured, so it would not reach the overlay")
  S.epoch += 1
  T.call("key", S.captured["Shift+Tab"])
end

-- The host answers read `i` (1-based, in the order asked) with `status` and, for "text", the
-- rows `rows` top to bottom: exactly once, `newer` set when a later read with its key was asked.
function T.answer(i, status, rows, err)
  local r = S.reads[i]
  assert(r, "there is no read " .. tostring(i))
  assert(not r.answered, "read " .. i .. " was answered already")
  r.answered = true
  local newer = false
  for j = i + 1, #S.reads do
    if S.reads[j].key == r.key then newer = true end
  end
  local x1, y1, x2, y2 = r.region[1], r.region[2], r.region[3], r.region[4]
  local lines, words = {}, {}
  if status == "text" then
    for k, t in ipairs(rows) do
      local w = { text = t, x = x1, y = y1 + (k - 1) * 8, w = x2 - x1, h = 8 }
      words[#words + 1] = w
      lines[k] = { text = t, x = w.x, y = w.y, w = w.w, h = w.h, words = { w } }
    end
  end
  T.call("answer", r.cb, {
    x = x1, y = y1, w = x2 - x1, h = y2 - y1, status = status, newer = newer,
    text = status == "text" and table.concat(rows, "\n") or "", lines = lines, words = words,
    lang = status == "stale" and "" or "en-US", error = err,
  })
end

-- What was said from entry `from` on.
function T.said(from)
  local out = {}
  for i = from + 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end

-- The lines that report what became of an unnamed stop: one per stop.
function T.outcomes() return T.count("has no name") end
"##;

fn run(scenario: &str) {
    run_with("windows", PT, scenario)
}

fn run_mac(scenario: &str) {
    run_with("macos", PT, scenario)
}

// ---------------------------------------------------------------------------------------------
// An unnamed stop is read, and spoken after the screen reader.
// ---------------------------------------------------------------------------------------------

/// The case the feature is for: Tab lands on a stop with no name, its rectangle is read with one
/// `host.ocr.recognize`, nothing is said until the answer comes, and then its text is said QUEUED —
/// behind the screen reader's announcement of the focus, not over it — with one line saying so.
#[test]
fn an_unnamed_stop_is_read_in_its_rectangle_and_its_text_is_queued() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "FILE", bounds = { x = 120, y = 60, w = 40, h = 20 } },
          { name = "", bounds = T.UNNAMED }, { name = "LIBRARY" } })
        local said = #S.speech
        T.tab()
        assert(S.focusSteps == 1 and #S.reads == 0, "FILE has a name: the screen reader's alone")
        T.tab()
        assert(S.focusSteps == 2 and S.ringAt == 2)
        assert(#S.reads == 1, "one read for the unnamed stop: " .. T.dump())
        local r = S.reads[1]
        assert(r.region[1] == 300 and r.region[2] == 200 and r.region[3] == 420 and r.region[4] == 224,
          "its own rectangle, corners exclusive: " .. table.concat(r.region, ","))
        assert(type(r.key) == "string" and r.key ~= "", "a key, so a later step's read supersedes it")
        assert(#S.speech == said, "nothing said before the answer: " .. T.said(said))
        S.now += 40
        T.answer(1, "text", { "Factory Library" })
        assert(#S.speech == said + 1, T.said(said))
        local last = S.speech[#S.speech]
        assert(last.text == "Factory Library", last.text)
        assert(last.interrupt == false, "queued behind the screen reader, not cutting it off")
        assert(T.count('[passthrough] Kontakt 7: stop 2 of 3 (type 50000) has no name; read at 300,200 '
          .. '120x24 in 40 ms: "Factory Library", spoken') == 1, T.dump())
        assert(T.outcomes() == 1, T.dump())
        -- The step's own line is there as before.
        assert(T.count("[passthrough] Kontakt 7: Tab -> stop 2 of 3 '', 2 into the lap") == 1, T.dump())
    "#);
}

/// A stop with a name is announced by the screen reader, whole, and the overlay neither reads
/// nor says anything about it.
#[test]
fn a_named_stop_is_not_read() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "FILE", bounds = { x = 120, y = 60, w = 40, h = 20 } },
          { name = "Search", bounds = { x = 400, y = 60, w = 200, h = 20 } },
          { name = " Brand ", bounds = { x = 400, y = 90, w = 200, h = 20 } } })
        local said = #S.speech
        T.tab(); T.tab(); T.tab()
        assert(S.focusSteps == 3, "three steps")
        assert(#S.reads == 0, "no read for a named stop: " .. T.dump())
        assert(#S.speech == said, "nothing said over the screen reader: " .. T.said(said))
        assert(T.outcomes() == 0, T.dump())
    "#);
}

/// A name of white space only — spaces, a tab, a no-break space — gives the screen reader nothing
/// to say either, so it is read like an empty one.
#[test]
fn a_name_of_white_space_only_is_read() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "   ", bounds = T.UNNAMED },
          { name = "\t\n", bounds = { x = 300, y = 230, w = 120, h = 24 } },
          { name = "\194\160", bounds = { x = 300, y = 260, w = 120, h = 24 } },
          { name = "x", bounds = { x = 300, y = 290, w = 120, h = 24 } } })
        for i = 1, 3 do
          T.tab()
          assert(#S.reads == i, "stop " .. i .. " is read: " .. T.dump())
          T.answer(i, "text", { "Row " .. i })
        end
        T.tab()
        assert(#S.reads == 3, "a one-letter name is a name")
        assert(T.said(0):find("Row 1 | Row 2 | Row 3", 1, true), T.said(0))
        assert(S.reads[3].region[2] == 260, "each stop read in its own rectangle")
    "#);
}

// ---------------------------------------------------------------------------------------------
// Only while the answer is still about the stop in front of the user.
// ---------------------------------------------------------------------------------------------

/// Two fast Tabs: the first stop's text must not be read out while the second is focused. Its
/// read was superseded (`newer`), and the second's text is the one said.
#[test]
fn a_second_tab_before_the_answer_drops_the_first() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED },
          { name = "", bounds = { x = 300, y = 230, w = 120, h = 24 } }, { name = "SHOP" } })
        local said = #S.speech
        T.tab(); T.tab()
        assert(#S.reads == 2 and S.reads[1].key == S.reads[2].key, "one key for the overlay's reads")
        T.answer(1, "text", { "Factory Library" })
        assert(#S.speech == said, "the first stop's text is not said: " .. T.said(said))
        assert(T.count('stop 1 of 3 (type 50000) has no name; read at 300,200 120x24 in 0 ms: "Factory Library", '
          .. 'not spoken: a later step asked for another read') == 1, T.dump())
        T.answer(2, "text", { "Kontakt 7 Factory Selection" })
        assert(T.said(said) == "Kontakt 7 Factory Selection", T.said(said))
        assert(T.outcomes() == 2, T.dump())
    "#);
}

/// A read the host never recognised because a later one replaced it (`stale`) says nothing.
#[test]
fn a_stale_read_says_nothing() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED },
          { name = "", bounds = { x = 300, y = 230, w = 120, h = 24 } } })
        local said = #S.speech
        T.tab(); T.tab()
        T.answer(1, "stale")
        assert(#S.speech == said, T.said(said))
        assert(T.count("stop 1 of 2 (type 50000) has no name; read at 300,200 120x24 in 0 ms: not recognised, "
          .. "a later step's read replaced it") == 1, T.dump())
    "#);
}

/// The second Tab lands on a NAMED stop, so no read supersedes the first one's: the answer comes
/// back without `newer`, and the step count is what says it is out of date.
#[test]
fn a_later_key_onto_a_named_stop_drops_the_answer() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED }, { name = "VIEW" }, { name = "" } })
        local said = #S.speech
        T.tab(); T.tab()
        assert(#S.reads == 1, "VIEW is not read")
        T.answer(1, "text", { "Factory Library" })
        assert(#S.speech == said, T.said(said))
        assert(T.count('"Factory Library", not spoken: a later key moved on') == 1, T.dump())
    "#);
}

/// The overlay left the front before the answer came: nothing is said over whatever is in
/// front now.
#[test]
fn an_answer_after_the_overlay_went_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED }, { name = "SHOP" } })
        local said = #S.speech
        T.tab()
        o.active = false
        T.answer(1, "text", { "Factory Library" })
        assert(#S.speech == said, T.said(said))
        assert(T.count('"Factory Library", not spoken: the overlay is no longer active') == 1, T.dump())
    "#);
}

/// Out of the front and back on the SAME window before the answer came — Alt+Tab out and back,
/// another overlay holding the slot for a moment. The focus resumes on the pass-through and the
/// lap is not cleared until the stop is announced again, a moment later; every other check
/// still holds. The answer is from the stay before, and is not said over the return.
#[test]
fn an_answer_after_leaving_the_front_and_coming_back_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED }, { name = "SHOP" } })
        -- As _activate leaves it for this window, so the return resumes rather than starting over.
        o._lastActiveId = S.origin.id
        T.tab()
        assert(#S.reads == 1)
        o:_deactivate()
        o:_activate()
        assert(o.active and o.focus == 2 and o.controls[2]._net == 1,
          "back on the same window: the focus is on the stop again, and the lap not cleared yet")
        local said = #S.speech
        T.answer(1, "text", { "Factory Library" })
        assert(#S.speech == said, T.said(said))
        assert(T.count('"Factory Library", not spoken: the overlay left the front and came back since') == 1,
          T.dump())
    "#);
}

/// The overlay is still active, but over another window of the plug-in by the time the answer
/// comes — a second Kontakt standalone: the text was read in the first one.
#[test]
fn an_answer_after_the_overlay_moved_to_another_window_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED }, { name = "SHOP" } })
        T.tab()
        S.origin = { id = 8, class = S.origin.class, app = S.origin.app, client = S.origin.client,
          bounds = S.origin.bounds }
        local said = #S.speech
        T.answer(1, "text", { "Factory Library" })
        assert(#S.speech == said, T.said(said))
        assert(T.count('"Factory Library", not spoken: the overlay is on another window now') == 1, T.dump())
    "#);
}

/// Every stop visited, the next Tab leaves the plug-in and goes on to the overlay's own first
/// control: the pass-through is no longer focused when the last stop's answer comes.
#[test]
fn an_answer_after_the_focus_left_the_pass_through_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED } })
        T.tab()
        assert(#S.reads == 1)
        T.tab()
        assert(o.focus == 1, "the lap is done: Tab carried on to the overlay's first control")
        local said = #S.speech
        T.answer(1, "text", { "Factory Library" })
        assert(#S.speech == said, T.said(said))
        assert(T.count('"Factory Library", not spoken: the focus has left the pass-through') == 1, T.dump())
    "#);
}

/// Shift+Tab on the element Tab went in on backs out onto the pass-through stop, which is still
/// focused — but the keyboard is no longer in the plug-in, and the stop's text is not said.
#[test]
fn an_answer_after_backing_out_with_shift_tab_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED }, { name = "SHOP" }, { name = "" } })
        T.tab()
        T.shiftTab()
        assert(o.focus == 2 and S.focusSteps == 1, "backed out onto the stop without a step")
        local said = #S.speech
        T.answer(1, "text", { "Factory Library" })
        assert(#S.speech == said, T.said(said))
        assert(T.count('"Factory Library", not spoken: the keyboard has left the plugin\'s controls') == 1,
          T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// What is said, what is not read, and the switch.
// ---------------------------------------------------------------------------------------------

/// The first two rows, top to bottom, trimmed and joined with a comma; a third is dropped, and
/// the line says how many there were.
#[test]
fn two_rows_are_spoken_and_a_third_is_dropped() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = { x = 300, y = 200, w = 200, h = 120 } } })
        T.tab()
        T.answer(1, "text", { " Factory Library ", "Kontakt 7", "18349 Presets" })
        local last = S.speech[#S.speech]
        assert(last.text == "Factory Library, Kontakt 7" and last.interrupt == false, last.text)
        assert(T.count('"Factory Library, Kontakt 7" (2 of 3 rows), spoken') == 1, T.dump())
    "#);
}

/// Rows with nothing but white space are skipped before the two are taken, and are not counted
/// among the rows the line reports.
#[test]
fn blank_rows_are_skipped_before_two_are_taken() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = { x = 300, y = 200, w = 200, h = 120 } },
          { name = "", bounds = { x = 300, y = 330, w = 200, h = 120 } } })
        T.tab()
        T.answer(1, "text", { "  ", "Factory Library", "", "Kontakt 7" })
        assert(S.speech[#S.speech].text == "Factory Library, Kontakt 7", S.speech[#S.speech].text)
        assert(T.count('"Factory Library, Kontakt 7", spoken') == 1, T.dump())
        T.tab()
        T.answer(2, "text", { "Factory Library", " ", "Kontakt 7", "18349 Presets" })
        assert(T.count('"Factory Library, Kontakt 7" (2 of 3 rows), spoken') == 1, T.dump())
    "#);
}

/// A name made only of characters that show nothing — a zero-width space, a byte-order mark, a
/// narrow no-break space, a control character — gives the screen reader nothing to say, so the
/// stop is read. A visible letter anywhere in it, or a letter from outside ASCII, is a name.
#[test]
fn a_name_of_invisible_characters_only_is_read() {
    run(r#"
        local S = T.S
        local function at(k) return { x = 300, y = 200 + 30 * k, w = 120, h = 24 } end
        local o = T.passThrough({
          { name = "\226\128\139", bounds = at(0) },              -- U+200B zero-width space
          { name = "\239\187\191 \226\128\175", bounds = at(1) }, -- U+FEFF, a space, U+202F
          { name = "\1\226\128\141\t", bounds = at(2) },         -- a control character, U+200D, a tab
          { name = "\226\128\139Brand", bounds = at(3) },         -- a zero-width space before a word
          { name = "Gr\195\182\195\159e", bounds = at(4) },       -- "Größe"
          { name = "\233\159\179", bounds = at(5) },              -- one CJK character
        })
        for i = 1, 3 do
          T.tab()
          assert(#S.reads == i, "stop " .. i .. " is read: " .. T.dump())
        end
        T.tab(); T.tab(); T.tab()
        assert(#S.reads == 3, "the last three have names: " .. T.dump())
        assert(T.outcomes() == 0, "no answer yet, so no outcome line")
    "#);
}

/// `O:addPassThrough()` with no opts at all: labelled "Plugin controls", and reading on.
#[test]
fn a_pass_through_with_no_opts_reads_unnamed_stops() {
    run(r#"
        local S = T.S
        S.ring, S.ringAt = { { name = "", bounds = T.UNNAMED } }, 0
        local o = T.O.new("Kontakt 7")
        local c = o:addPassThrough()
        assert(c.label == "Plugin controls" and c.readUnnamed == true, T.dump())
        T.front(o)
        T.tab()
        assert(#S.reads == 1, T.dump())
        T.answer(1, "text", { "Factory Library" })
        assert(S.speech[#S.speech].text == "Factory Library")
    "#);
}

/// Each overlay reads under a key of its own, even two of one module with the same label: one
/// overlay's step must not supersede another's read. The key is the runtime's, named so a
/// module's own keys do not meet it.
#[test]
fn each_overlay_reads_under_its_own_key() {
    run(r#"
        local S = T.S
        local a = T.passThrough({ { name = "", bounds = T.UNNAMED } })
        T.tab()
        a:_deactivate()
        local b = T.passThrough({ { name = "", bounds = T.UNNAMED } })
        assert(a.label == b.label)
        T.tab()
        assert(#S.reads == 2, T.dump())
        local ka, kb = S.reads[1].key, S.reads[2].key
        assert(ka ~= kb, "one key per overlay: " .. ka)
        assert(ka:find("com.platform.overlay passthrough ", 1, true) == 1
          and kb:find("com.platform.overlay passthrough ", 1, true) == 1, ka .. " / " .. kb)
    "#);
}

/// Nothing drawn, nothing read, a read that failed: nothing is said, and the line says which.
#[test]
fn blank_none_and_failed_say_nothing_and_are_logged() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED },
          { name = "", bounds = { x = 300, y = 230, w = 120, h = 24 } },
          { name = "", bounds = { x = 300, y = 260, w = 120, h = 24 } }, { name = "SHOP" } })
        local said = #S.speech
        T.tab(); T.answer(1, "blank")
        T.tab(); T.answer(2, "none")
        T.tab(); T.answer(3, "failed", nil, "screen capture failed: the display is asleep")
        assert(#S.speech == said, T.said(said))
        assert(T.count("stop 1 of 4 (type 50000) has no name; read at 300,200 120x24 in 0 ms: nothing drawn "
          .. "there") == 1, T.dump())
        assert(T.count("stop 2 of 4 (type 50000) has no name; read at 300,230 120x24 in 0 ms: the recogniser "
          .. "read nothing") == 1, T.dump())
        assert(T.count("stop 3 of 4 (type 50000) has no name; read at 300,260 120x24 in 0 ms: the read failed "
          .. "(screen capture failed: the display is asleep)") == 1, T.dump())
        assert(T.outcomes() == 3, T.dump())
    "#);
}

/// A stop the platform gave no rectangle for, one too small to hold a letter, one wholly outside
/// the plug-in's window and one the window's edge leaves too little of are not read, each with
/// its line — the last giving the part inside the window as well, so a 120-wide rectangle is not
/// called under 2. Each line names the stop's control type, whatever it is.
#[test]
fn a_stop_without_a_usable_rectangle_is_not_read_and_is_logged() {
    run(r#"
        local S = T.S
        -- The window's right edge is 92 + 816 = 908.
        local o = T.passThrough({ { name = "", ctype = 50033 },
          { name = "", bounds = { x = 300, y = 200, w = 1, h = 24 } },
          { name = "", bounds = { x = 2000, y = 200, w = 120, h = 24 } },
          { name = "", bounds = { x = 907, y = 200, w = 120, h = 24 } }, { name = "SHOP" } })
        local said = #S.speech
        T.tab(); T.tab(); T.tab(); T.tab()
        assert(#S.reads == 0, T.dump())
        assert(#S.speech == said, T.said(said))
        assert(T.count("stop 1 of 5 (type 50033) has no name and no rectangle; nothing to read") == 1, T.dump())
        assert(T.count("stop 2 of 5 (type 50000) has no name; its rectangle 300,200 1x24 is under 2 across or "
          .. "down, nothing to read") == 1, T.dump())
        assert(T.count("stop 3 of 5 (type 50000) has no name; its rectangle 2000,200 120x24 is outside the "
          .. "plugin's window, nothing to read") == 1, T.dump())
        assert(T.count("stop 4 of 5 (type 50000) has no name; its rectangle 907,200 120x24 is under 2 across "
          .. "or down inside the plugin's window (907,200 1x24), nothing to read") == 1, T.dump())
        assert(T.outcomes() == 4, T.dump())
    "#);
}

/// `readUnnamed = false`: the module has switched it off for this control, and an unnamed stop is
/// left to the screen reader like a named one.
#[test]
fn read_unnamed_false_turns_the_reading_off() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED }, { name = "SHOP" } },
          { label = "Kontakt controls", readUnnamed = false })
        local said = #S.speech
        T.tab()
        assert(S.focusSteps == 1 and #S.reads == 0, T.dump())
        assert(#S.speech == said and T.outcomes() == 0, T.dump())
    "#);
}

/// The old spelling adds the same control, reading included.
#[test]
fn enable_uia_pass_through_reads_unnamed_stops_too() {
    run(r#"
        local S = T.S
        S.ring, S.ringAt = { { name = "", bounds = T.UNNAMED } }, 0
        local o = T.O.new("Kontakt 7")
        o:enableUiaPassThrough({ label = "Kontakt controls" })
        T.front(o)
        assert(o.focus == 1)
        T.tab()
        assert(#S.reads == 1, T.dump())
    "#);
}

/// A read the host refuses at the call is caught: the Tab has already moved the plug-in's focus,
/// so it stays consumed and the lap counted, and the line says why nothing was read.
#[test]
fn a_read_that_raises_is_logged_and_the_step_stands() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = T.UNNAMED }, { name = "SHOP" } })
        S.readRaises = "host.ocr.recognize: 17 reads waiting"
        local said = #S.speech
        T.tab()
        assert(o.focus == 2 and o.controls[2]._net == 1, "the step stands")
        assert(#S.speech == said, T.said(said))
        assert(T.count("stop 1 of 2 (type 50000) has no name; its rectangle 300,200 120x24 could not be read: "
          .. "host.ocr.recognize: 17 reads waiting") == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The rectangle as each platform gives it.
// ---------------------------------------------------------------------------------------------

/// On a Mac the rectangle is in points and may carry a fraction: the region is widened to whole
/// numbers that cover it — the near edges down, the far edges up.
#[test]
fn on_a_mac_a_fractional_rectangle_is_widened_to_whole_points() {
    run_mac(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = { x = 300.5, y = 200.25, w = 120.25, h = 23.5 } } })
        T.tab()
        local r = S.reads[1]
        assert(r and r.region[1] == 300 and r.region[2] == 200 and r.region[3] == 421 and r.region[4] == 224,
          r and table.concat(r.region, ",") or T.dump())
        T.answer(1, "text", { "Factory Library" })
        assert(S.speech[#S.speech].text == "Factory Library")
    "#);
}

/// An element that reaches past the plug-in's window — a list longer than its view — is read
/// only where the window is: what lies beyond is another window's.
#[test]
fn a_rectangle_past_the_window_is_cut_to_it() {
    run(r#"
        local S = T.S
        local o = T.passThrough({ { name = "", bounds = { x = 800, y = 600, w = 400, h = 300 } } })
        T.tab()
        local r = S.reads[1]
        assert(r and r.region[1] == 800 and r.region[2] == 600 and r.region[3] == 908 and r.region[4] == 658,
          r and table.concat(r.region, ",") or T.dump())
    "#);
}

/// A display left of or above the primary one has negative coordinates, on both systems: the
/// edges are rounded the same way (down is further left there), and the cut to a window on that
/// display works the same.
#[test]
fn a_rectangle_left_of_the_primary_display_is_read_there() {
    run_mac(r#"
        local S = T.S
        S.origin.bounds = { x = -1920, y = -40, w = 1000, h = 700 }
        local o = T.passThrough({ { name = "", bounds = { x = -1500.5, y = -30.25, w = 120.25, h = 23.5 } },
          { name = "", bounds = { x = -2000, y = -100, w = 200, h = 200 } } })
        T.tab()
        local r = S.reads[1]
        assert(r and r.region[1] == -1501 and r.region[2] == -31 and r.region[3] == -1380 and r.region[4] == -6,
          r and table.concat(r.region, ",") or T.dump())
        T.tab()
        r = S.reads[2]
        assert(r and r.region[1] == -1920 and r.region[2] == -40 and r.region[3] == -1800 and r.region[4] == 100,
          r and table.concat(r.region, ",") or T.dump())
    "#);
}
