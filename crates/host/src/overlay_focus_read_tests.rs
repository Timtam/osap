//! The overlay runtime's announcement reads — an OCR control's value region and any control's
//! `ocrLabel` — off the event loop, run for real against the scripted host of
//! `overlay_menu_tests.rs`.
//!
//! Written for the maintainer's decisions of 2026-10-01. The reads were `host.ocr.recognize`, on the
//! event loop, which on a Mac also carries the keyboard's event tap: an Intel MacBook Air spent 143
//! to 786 ms in each of sforzando's focus reads, with every key waiting behind it. Now they are one
//! `host.ocr.read` per announcement, under one key per overlay, and the announcement is ONE
//! sentence — name, what follows it, value — said when the answer comes, as it was said before,
//! only without the loop waiting for it. It is said only while it is still about what is in front
//! of the user: not superseded by a later announcement's read, the overlay still active, on the same
//! window and in the same stay, the focus where it was, no key of the overlay's own since and no
//! other announcement since, and no menu open over the overlay or opened since. A region with
//! nothing drawn in it, or nothing the recogniser read, says what an empty read said — the
//! control's `fallback`, "no text" without one; a read that failed says "cannot be read now"; a
//! read refused at the call is said at once without a value. Every answer is one `[read]` line,
//! saying what became of it.
//!
//! Activating an OCR button keeps the order it had while the read held the event loop: the
//! answer, the sentence, then the click — so a menu the click opens comes after the value, and an
//! OCREdit's caret goes into its field after its sentence too. A press's late announcement (a
//! toggle's watch) and the arrival's are not made over a key of the overlay's own that came after.
//!
//! `T.fields` builds the overlay — an OCR read-out with a hotkey, and a plain button — in front;
//! `T.tab()` moves the focus as the captured key does, and `T.answerList` delivers a read's answer
//! as the host does, on a later turn of the loop. No timer decides anything here either.

use super::overlay_menu_tests::run_with;

/// The helpers, added to the harness's `T`; also the start of `overlay_runtime_marks_tests.rs`'s.
pub(crate) const FR: &str = r##"
local S = T.S
S.clicks = {}
S.moves = {}
S.hotkeyFns = {}
S.dump = {}

rawset(T.host, "input", T.strict("host.input", {
  click = function(x, y)
    S.clicks[#S.clicks + 1] = { x, y }
    S.order[#S.order + 1] = "click"
  end,
  move = function(x, y)
    S.moves[#S.moves + 1] = { x, y }
    S.order[#S.order + 1] = "move"
  end,
}))
-- A hotkey's callback, kept so a scenario can press it; and no modifier held when it fires.
local register = T.host.hotkey.register
rawset(T.host.hotkey, "register", function(spec, fn)
  S.hotkeyFns[spec] = fn
  return register(spec, fn)
end)
rawset(T.host.keys, "modifiersDown", function() return false end)
-- What was said, in S.order too, so a scenario can tell whether the sentence or the click came first.
local output = T.host.speech.output
rawset(T.host.speech, "output", function(text, opts)
  output(text, opts)
  S.order[#S.order + 1] = "speech"
end)
rawset(T.host.window, "ownsPoint", function() return true end)
-- What a calibration shot asks besides: the origin's accessibility dump, and its pictures.
rawset(T.host.element, "rawDump", function() return S.dump end)
-- Every read, with the options it was asked with, in the order things happened.
local read = T.host.ocr.read
rawset(T.host.ocr, "read", function(what, opts, cb)
  if type(opts) == "function" then cb, opts = opts, nil end
  read(what, opts, cb)
  S.reads[#S.reads].opts = opts or {}
  S.order[#S.order + 1] = "read"
end)

-- The corners of region `r` as one string, for messages.
function T.region(r) return table.concat({ r[1], r[2], r[3], r[4] }, ",") end

-- Whether a log line is exactly `line`.
function T.logged(line)
  for _, l in ipairs(S.logs) do
    if l == line then return true end
  end
  return false
end

-- An overlay over S.origin (client 100,50 800x600), in front: the OCR read-out "Preset" with a
-- hotkey — region 34,7 to 214,22, so 134,57 to 314,72 on screen — a plain button "Init", and
-- whatever `extra(o)` adds after them. The focus starts on Preset, said by nobody: T.tab() from
-- there goes to Init, and once more back to Preset.
function T.fields(extra, opts)
  local o = T.O.new("Synth")
  o:addOCRButton({ label = "Preset", region = { 34, 7, 214, 22 }, hotkey = "Alt+P",
    fallback = opts and opts.fallback })
  o:addCustomButton({ label = "Init", onActivate = function() end })
  if extra then extra(o) end
  T.front(o)
  return o
end

-- The answer to read `i` (1-based, in the order asked), a list read: `by` maps each entry's name to
-- { status, text?, error?, approx?, lang? } ("none" when it names nothing; `approx`: its word placed
-- by estimate, as the second recogniser's are; `lang` "en-US" unless given, "" for a stale one),
-- every reading `newer` when `newer` is set — a later read of its key was asked after this one
-- started recognising — or when it is "stale". Exactly once, with the epoch turned over first, as
-- the host delivers one.
function T.answerList(i, by, newer)
  local r = S.reads[i]
  assert(r and r.list, "read " .. tostring(i) .. " is not a list read: " .. T.dump())
  assert(not r.answered, "read " .. i .. " was answered already")
  r.answered = true
  S.epoch += 1
  local list, byName = {}, {}
  for k, g in ipairs(r.regions) do
    local a = (by or {})[r.names[k]] or { status = "none" }
    local text = a.status == "text" and a.text or ""
    local box = { text = text, x = g[1], y = g[2], w = g[3] - g[1], h = g[4] - g[2], approx = a.approx }
    local reading = {
      name = r.names[k], x = box.x, y = box.y, w = box.w, h = box.h, status = a.status,
      newer = newer == true or a.status == "stale", text = text,
      lines = text ~= "" and { { text = text, x = box.x, y = box.y, w = box.w, h = box.h, words = { box } } } or {},
      words = text ~= "" and { box } or {}, lang = a.lang or (a.status == "stale" and "" or "en-US"), error = a.error,
    }
    list[k] = reading
    byName[r.names[k]] = reading
  end
  T.call("answer", r.cb, list, byName)
end

-- What was said from entry `from` on.
function T.said(from)
  local out = {}
  for i = from + 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end

-- The last thing said.
function T.last() return S.speech[#S.speech] end

-- Where entry `what` first comes in S.order from entry `from` on, or nil.
function T.at(what, from)
  for i = (from or 0) + 1, #S.order do
    if S.order[i] == what then return i end
  end
  return nil
end

-- Return on whatever is focused, as the captured key delivers it.
function T.ret()
  assert(S.holding["Return"], "Return is not captured, so it would not reach the overlay")
  S.epoch += 1
  T.call("key", S.captured["Return"])
end
"##;

fn run(scenario: &str) {
    run_with("windows", FR, scenario)
}

fn run_mac(scenario: &str) {
    run_with("macos", FR, scenario)
}

// ---------------------------------------------------------------------------------------------
// The read is off the loop, and the sentence comes with the answer.
// ---------------------------------------------------------------------------------------------

/// Tab onto the read-out: one `host.ocr.read` under the overlay's own key, in the user's language,
/// of the region as placed; nothing is said until the answer comes, and then ONE sentence — the
/// name, the type word, the key, the value — interrupting, as a key's announcement does, with one
/// `[read]` line that says it was spoken.
#[test]
fn the_value_is_read_off_the_loop_and_said_in_one_sentence_with_the_answer() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab()
        assert(o.focus == 2 and T.last().text == "Init, button", "a control with nothing to read is said at once")
        local said = #S.speech
        T.tab()
        assert(o.focus == 1)
        assert(#S.reads == 1, "one read: " .. T.dump())
        local r = S.reads[1]
        assert(#r.regions == 1 and r.names[1] == "value", "the value region, named")
        assert(T.region(r.regions[1]) == "134,57,314,72", T.region(r.regions[1]))
        assert(r.key:find("com.platform.overlay focus ", 1, true) == 1, r.key)
        assert(r.opts.lang == nil, "the user's language, as every read without lang")
        assert(#S.speech == said, "nothing before the answer: " .. T.said(said))
        S.now += 230
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        assert(#S.speech == said + 1, T.said(said))
        assert(T.last().text == "Preset, button, Alt+P, Init Preset", T.last().text)
        assert(T.last().interrupt == true, "after a key: it interrupts, as it did")
        assert(T.count('[read] \'Preset\' = "Init Preset" (region 34,7 180x15, 230 ms, 1 word, en-US); spoken') == 1, T.dump())
    "#);
}

/// The same on a Mac: the same sentence, the same one read.
#[test]
fn on_a_mac_the_same_read_and_the_same_sentence() {
    run_mac(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        assert(#S.reads == 1 and o.focus == 1)
        T.answerList(1, { value = { status = "text", text = "64" } })
        assert(T.last().text == "Preset, button, Alt+P, 64", T.last().text)
    "#);
}

/// Each overlay reads under a key of its own, and none of them is the pass-through's.
#[test]
fn each_overlay_announces_under_its_own_key() {
    run(r#"
        local S = T.S
        local a = T.fields()
        T.tab(); T.tab()
        a.active = false
        local b = T.fields()
        T.tab(); T.tab()
        assert(#S.reads == 2, T.dump())
        local ka, kb = S.reads[1].key, S.reads[2].key
        assert(ka ~= kb, "one key per overlay: " .. ka)
        assert(kb:find("com.platform.overlay focus ", 1, true) == 1, kb)
    "#);
}

// ---------------------------------------------------------------------------------------------
// Spoken only while it is still true.
// ---------------------------------------------------------------------------------------------

/// Two Tabs back onto the read-out before the first answer: the first read is answered "stale"
/// — superseded before it was recognised — and says nothing; the second is said. And one already
/// recognising when the next was asked comes back `newer`, and says nothing either.
#[test]
fn a_second_tab_before_the_answer_drops_the_first() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        T.tab(); T.tab()
        assert(#S.reads == 2 and S.reads[1].key == S.reads[2].key, T.dump())
        local said = #S.speech
        T.answerList(1, { value = { status = "stale" } })
        assert(#S.speech == said, T.said(said))
        assert(T.count('[read] \'Preset\' = "" (region 34,7 180x15, 0 ms, 0 words): not recognised; not spoken: '
          .. "a later announcement's read replaced it before it was recognised") == 1, T.dump())
        T.answerList(2, { value = { status = "text", text = "Init Preset" } })
        assert(#S.speech == said + 1 and T.last().text == "Preset, button, Alt+P, Init Preset", T.said(said))

        T.tab(); T.tab()
        T.tab(); T.tab()
        said = #S.speech
        T.answerList(3, { value = { status = "text", text = "Old" } }, true)
        assert(#S.speech == said, T.said(said))
        assert(T.count('= "Old" (region 34,7 180x15, 0 ms, 1 word, en-US); not spoken: a later announcement asked for '
          .. "another read") == 1, T.dump())
        T.answerList(4, { value = { status = "text", text = "New" } })
        assert(T.last().text == "Preset, button, Alt+P, New", T.said(said))
    "#);
}

/// Tab on to the next control before the answer: the answer is about a control the user has
/// left, and is not said over the one they are on.
#[test]
fn an_answer_after_the_focus_moved_on_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        T.tab()
        assert(o.focus == 2 and T.last().text == "Init, button")
        local said = #S.speech
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        assert(#S.speech == said, T.said(said))
        assert(T.count('= "Init Preset" (region 34,7 180x15, 0 ms, 1 word, en-US); not spoken: the focus has moved on') == 1, T.dump())
    "#);
}

/// The overlay went, before the answer: not said. Out and back on the same window: the answer is
/// from the stay before, and not said over the return. Over another window: not said.
#[test]
fn an_answer_after_the_overlay_left_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        o.active = false
        local said = #S.speech
        T.answerList(1, { value = { status = "text", text = "A" } })
        assert(#S.speech == said, T.said(said))
        assert(T.count('= "A" (region 34,7 180x15, 0 ms, 1 word, en-US); not spoken: the overlay is no longer active') == 1, T.dump())

        o = T.fields()
        o._lastActiveId = S.origin.id
        T.tab(); T.tab()
        o:_deactivate()
        o:_activate()
        assert(o.active and o.focus == 1, "back on the same window, on the same control")
        said = #S.speech
        T.answerList(2, { value = { status = "text", text = "B" } })
        assert(#S.speech == said, T.said(said))
        assert(T.count('= "B" (region 34,7 180x15, 0 ms, 1 word, en-US); not spoken: the overlay left the front and came '
          .. 'back since') == 1, T.dump())

        o = T.fields()
        T.tab(); T.tab()
        S.origin = { id = 8, class = S.origin.class, app = S.origin.app, client = S.origin.client,
          bounds = S.origin.bounds }
        said = #S.speech
        T.answerList(3, { value = { status = "text", text = "C" } })
        assert(#S.speech == said, T.said(said))
        assert(T.count('not spoken: the overlay is on another window now') == 1, T.dump())
    "#);
}

/// A key of the overlay's own that says nothing and leaves the focus where it is — a control's
/// hotkey that keeps the focus, whose action is the module's — still means the user has moved on:
/// the answer to a read asked before it is not said.
#[test]
fn a_later_key_drops_the_answer() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addCustomButton({ label = "Quiet", hotkey = "Alt+Q", hotkeyKeepsFocus = true, onActivate = function() end })
        end)
        T.tab(); T.tab(); T.tab()
        assert(o.focus == 1 and #S.reads == 1)
        local said = #S.speech
        assert(S.hotkeyFns["Alt+Q"], "the hotkey is held")
        S.hotkeyFns["Alt+Q"]()
        assert(o.focus == 1 and #S.speech == said, "the key moved nothing and said nothing")
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        assert(#S.speech == said, T.said(said))
        assert(T.count('= "Init Preset" (region 34,7 180x15, 0 ms, 1 word, en-US); not spoken: a later key reached the '
          .. 'overlay') == 1, T.dump())
    "#);
}

/// A menu counts open over the overlay by the time the answer comes — one the plug-in put up
/// itself while the read was slow: the sentence is not said over the menu.
#[test]
fn an_answer_while_a_menu_is_open_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        o._menu = { open = true }
        local said = #S.speech
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        o._menu = nil
        assert(#S.speech == said, T.said(said))
        assert(T.count('not spoken: a menu is open over the overlay') == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// What a reading that is not text says.
// ---------------------------------------------------------------------------------------------

/// Nothing drawn there, and nothing the recogniser read: what an empty read said — the control's
/// `fallback`, "no text" without one. A read that failed: "cannot be read now", as for a region
/// that cannot be placed. Each line says which.
#[test]
fn blank_none_and_failed_say_what_an_empty_read_said() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        T.answerList(1, { value = { status = "blank" } })
        assert(T.last().text == "Preset, button, Alt+P, no text", T.last().text)
        assert(T.count('= "" (region 34,7 180x15, 0 ms, 0 words, en-US): nothing drawn there; spoken') == 1, T.dump())
        T.tab(); T.tab()
        T.answerList(2, { value = { status = "none" } })
        assert(T.last().text == "Preset, button, Alt+P, no text", T.last().text)
        assert(T.count('= "" (region 34,7 180x15, 0 ms, 0 words, en-US): the recogniser read nothing; spoken') == 1, T.dump())
        T.tab(); T.tab()
        T.answerList(3, { value = { status = "failed", error = "screen capture failed" } })
        assert(T.last().text == "Preset, button, Alt+P, cannot be read now", T.last().text)
        assert(T.count('= "" (region 34,7 180x15, 0 ms, 0 words, en-US): the read failed (screen capture failed); spoken') == 1,
          T.dump())

        local f = T.fields(nil, { fallback = "no preset name" })
        T.tab(); T.tab()
        T.answerList(4, { value = { status = "blank" } })
        assert(T.last().text == "Preset, button, Alt+P, no preset name", T.last().text)
    "#);
}

/// A region with nothing of it left on screen — corners turned around by a region function —
/// is not asked about: host.ocr.read would refuse it, and host.ocr.recognize read it as nothing. It
/// is said at once as an empty read was, and the log says why.
#[test]
fn a_region_empty_on_screen_is_not_read_and_says_what_an_empty_read_said() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addOCRButton({ label = "Turned", region = function() return { 60, 20, 40, 30 } end, readOnly = true })
        end)
        T.tab(); T.tab()
        assert(o.focus == 3 and #S.reads == 0, T.dump())
        assert(T.last().text == "Turned, no text", T.last().text)
        assert(T.count("[read] 'Turned' not read: its region is empty on screen (160,70 to 140,80)") == 1, T.dump())
    "#);
}

/// `hoverToRead`: the pointer goes onto the control before the read is asked, so the picture is
/// taken with it there, as it was.
#[test]
fn hover_to_read_moves_before_the_read_is_asked() {
    run(r#"
        local S = T.S
        local o = T.fields()
        o.hoverToRead = true
        T.tab()
        local from = #S.order
        T.tab()
        assert(S.order[from + 1] == "move" and S.order[from + 2] == "read", table.concat(S.order, ","))
        assert(S.moves[1][1] == 224 and S.moves[1][2] == 64, "the middle of the region")
    "#);
}

/// A read the host refuses at the call is said at once, with the static name and "cannot be read
/// now" for the value, and the log says why.
#[test]
fn a_read_refused_at_the_call_is_said_at_once_without_a_value() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab()
        S.readRaises = "host.ocr.read: corners that cover more than 40 million pixels"
        T.tab()
        assert(#S.reads == 0)
        assert(T.last().text == "Preset, button, Alt+P, cannot be read now", T.last().text)
        assert(T.count("[read] 'Preset' could not be read: host.ocr.read: corners that cover more than 40 million "
          .. "pixels") == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The name read off the screen, activation, arrival.
// ---------------------------------------------------------------------------------------------

/// A hotspot's `ocrLabel` is read off the loop like a value: the name read, then the rest of the
/// sentence; a name read as nothing leaves the static label, as it did. A control with a name region
/// AND a value region — no kind takes both today, so the scenario gives the read-out one by hand —
/// has both read in one read, one picture, so the name and the value agree.
#[test]
fn a_name_is_read_off_the_loop_and_with_the_value_in_one_read() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          local mic = o:addOCRButton({ label = "Mic", region = { 300, 20, 360, 36 }, readOnly = true })
          mic.ocrLabel = { 300, 40, 360, 56 }
          o:addHotspotButton({ label = "Spot", at = { 10, 20 }, ocrLabel = { 400, 40, 460, 56 } })
        end)
        T.tab(); T.tab()
        assert(o.focus == 3 and #S.reads == 1, T.dump())
        local r = S.reads[1]
        assert(#r.regions == 2 and r.names[1] == "label" and r.names[2] == "value", T.dump())
        assert(T.region(r.regions[1]) == "400,90,460,106" and T.region(r.regions[2]) == "400,70,460,86",
          T.region(r.regions[1]) .. " / " .. T.region(r.regions[2]))
        T.answerList(1, { label = { status = "text", text = "Room Mic" }, value = { status = "text", text = "-3.0 dB" } })
        assert(T.last().text == "Room Mic, -3.0 dB", T.last().text)
        assert(T.logged('[read] \'Mic\' name = "Room Mic" (region 300,40 60x16, 0 ms, 1 word, en-US)'), T.dump())
        assert(T.count('[read] \'Room Mic\' = "-3.0 dB" (region 300,20 60x16, 0 ms, 1 word, en-US); spoken') == 1, T.dump())

        T.tab()
        assert(o.focus == 4 and #S.reads == 2)
        assert(#S.reads[2].regions == 1 and S.reads[2].names[1] == "label")
        T.answerList(2, { label = { status = "none" } })
        assert(T.last().text == "Spot, button", T.last().text)
        assert(T.count('[read] \'Spot\' name = "" (region 400,40 60x16, 0 ms, 0 words, en-US): the recogniser read nothing; '
          .. 'spoken') == 1, T.dump())
    "#);
}

/// Return on the read-out: the read is asked, and nothing else yet; when the answer comes, the
/// sentence — without the key, which was just pressed — and THEN the click, as it was while the
/// read held the event loop: the value is the one from before the click whatever the picture costs,
/// and the sentence does not cut off what the click opens.
#[test]
fn activating_says_the_value_with_the_answer_and_then_clicks() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        local said = #S.speech
        local from = #S.order
        T.press(o)
        assert(#S.reads == 2, T.dump())
        assert(#S.clicks == 0 and #S.speech == said, "nothing but the read before the answer: " .. table.concat(S.order, ","))
        T.answerList(2, { value = { status = "text", text = "Init Preset" } })
        assert(T.last().text == "Preset, button, Init Preset" and T.last().interrupt == true, T.said(said))
        local read, speech, click = T.at("read", from), T.at("speech", from), T.at("click", from)
        assert(read and speech and click and read < speech and speech < click, table.concat(S.order, ","))
        assert(#S.clicks == 1 and S.clicks[1][1] == 224 and S.clicks[1][2] == 64, "the middle of the region")
    "#);
}

/// A key of the overlay's own between the press and the answer — Tab on — and the press's click is
/// not made: the user has moved on, and a click landing after that is the mistake pressing again
/// cannot undo. The log says so.
#[test]
fn a_later_key_takes_the_activations_click_with_it() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        T.ret()
        assert(#S.reads == 2 and #S.clicks == 0)
        T.tab()
        assert(o.focus == 2 and T.last().text == "Init, button")
        T.answerList(2, { value = { status = "text", text = "Init Preset" } })
        assert(#S.clicks == 0, "no click after the key")
        assert(T.last().text == "Init, button", "nothing said over the control the user is on")
        assert(T.count("[read] 'Preset': not clicking — a later key reached the overlay") == 1, T.dump())
    "#);
}

/// sforzando's fields: Return on an OCR button that opens a menu, with a test that sees native
/// popups. The menu can come only after the click, and the click only after the sentence — so the
/// value is said, then the menu opens and is seen, the press counted for it (`opensMenu`). Before
/// the click came after the sentence, a test that saw the popup before the answer dropped the
/// sentence, and the press said nothing at all — on Windows too.
#[test]
fn an_ocr_button_that_opens_a_menu_says_its_value_before_the_menu() {
    run(r#"
        local S = T.S
        local o = T.O.new("Synth")
        o:addOCRButton({ label = "Instrument", region = { 34, 7, 214, 22 }, opensMenu = true })
        o:addCustomButton({ label = "Init", onActivate = function() end })
        o:_watchMenus({ T.O.menuTests.nativePopup })
        T.front(o)
        T.tick(2)
        local said = #S.speech
        local from = #S.order
        T.ret()
        assert(#S.reads == 1 and #S.clicks == 0, "the read asked, the click waiting for its answer: " .. T.dump())
        T.tick(1)
        assert(not o:menuOpen(), "no menu before the click")
        T.answerList(1, { value = { status = "text", text = "Piano" } })
        assert(T.said(said) == "Instrument, button, Piano", T.said(said))
        local speech, click = T.at("speech", from), T.at("click", from)
        assert(speech and click and speech < click, table.concat(S.order, ","))
        S.native = true
        T.tick(1)
        assert(o:menuOpen(), "the menu the click opened is seen")
        assert(T.count("after 'Instrument' was pressed") == 1, "the press counted, at its click: " .. T.dump())
        assert(T.said(said) == "Instrument, button, Piano", "said once: " .. T.said(said))
    "#);
}

/// A menu opens over the overlay while a focus read is under way, a choice is made in it, and it
/// closes before the answer comes: the value read is the one from before the choice, and it is not
/// said — a menu opened since.
#[test]
fn an_answer_after_a_menu_opened_and_closed_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.O.new("Synth")
        o:addOCRButton({ label = "Instrument", region = { 34, 7, 214, 22 }, opensMenu = true })
        o:addCustomButton({ label = "Init", onActivate = function() end })
        o:_watchMenus({ T.O.menuTests.nativePopup })
        T.front(o)
        T.tick(2)
        T.tab(); T.tab()
        assert(o.focus == 1 and #S.reads == 1, T.dump())
        S.native = true
        T.tick(1)
        assert(o:menuOpen())
        S.passed = { { key = "Down" }, { key = "Return" } }
        S.native = false
        T.tick(3)
        assert(not o:menuOpen(), "closed again")
        local said = #S.speech
        T.answerList(1, { value = { status = "text", text = "Piano" } })
        assert(#S.speech == said, "the value from before the choice: " .. T.said(said))
        assert(T.count("not spoken: a menu opened over the overlay since") == 1, T.dump())
    "#);
}

/// The control list is not the one the read was asked in — a module that rebuilt it, the same
/// index another control: the answer is not said.
#[test]
fn an_answer_for_a_control_no_longer_in_its_place_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        local said = #S.speech
        o.controls[1] = o.controls[2]
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        assert(#S.speech == said, T.said(said))
        assert(T.count("not spoken: the focus has moved on") == 1, T.dump())
    "#);
}

/// Another announcement made in the meantime with no key of the overlay's — the module's own code
/// pressing a toggle, whose watch announces it — and the focus read's answer, older than that
/// sentence, is not said after it.
#[test]
fn an_answer_after_another_announcement_is_not_spoken() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addHotspotToggle({ label = "Mute", at = { 10, 20 }, onColor = { 255, 255, 255 }, offColor = { 20, 20, 20 } })
        end)
        T.tab(); T.tab(); T.tab()
        assert(o.focus == 1 and #S.reads == 1)
        o:activate(3)
        for _ = 1, 10 do
          S.now += 120
          T.runDue()
        end
        assert(T.last().text == "Mute, toggle button, off", T.last().text)
        assert(#S.reads == 1, "the toggle reads nothing")
        local said = #S.speech
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        assert(#S.speech == said, T.said(said))
        assert(T.count("not spoken: a later announcement was made") == 1, T.dump())
    "#);
}

/// An OCREdit (Komplete Kontrol's "Save as"): on focus its sentence comes with the answer and THEN
/// the click that puts the caret into the field; a focus that moved on before the answer takes the
/// click along, and says so. Activated, it reads again and clicks after the sentence, once.
#[test]
fn an_ocredit_clicks_into_its_field_after_its_sentence() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addOCREdit({ label = "Save as", region = { 300, 20, 400, 36 } })
        end)
        T.tab(); T.tab()
        assert(o.focus == 3 and #S.reads == 1 and #S.clicks == 0, "the click waits for the answer: " .. T.dump())
        local from = #S.order
        T.answerList(1, { value = { status = "text", text = "My preset" } })
        assert(T.last().text == "Save as, edit, My preset", T.last().text)
        local speech, click = T.at("speech", from), T.at("click", from)
        assert(speech and click and speech < click, table.concat(S.order, ","))
        assert(#S.clicks == 1 and S.clicks[1][1] == 450 and S.clicks[1][2] == 78, "the middle of the field")

        T.tab(); T.tab(); T.tab()
        assert(o.focus == 3 and #S.reads == 3, T.dump())
        T.tab()
        assert(o.focus == 1 and #S.reads == 4)
        T.answerList(3, { value = { status = "stale" } })
        assert(#S.clicks == 1, "the focus moved on: no click into the field")
        assert(T.count("[read] 'Save as': the focus moved on before the answer; not clicking into the field") == 1, T.dump())

        o:activate(3)
        local reads = #S.reads
        T.answerList(reads, { value = { status = "text", text = "My preset" } })
        assert(T.last().text == "Save as, edit, My preset", T.last().text)
        assert(#S.clicks == 2 and S.clicks[2][1] == 450 and S.clicks[2][2] == 78, "one click, after the sentence")
    "#);
}

/// Arriving: the announcement is the timer's, queued behind what is being said, and its read is
/// asked then; the sentence comes with the answer, still queued.
#[test]
fn arriving_reads_and_says_the_sentence_queued() {
    run(r#"
        local S = T.S
        local o = T.fields()
        o._lastActiveId = S.origin.id
        o:_deactivate()
        S.after = {}
        o:_activate()
        -- Only the announcement's timer: the confirming recheck would look for contexts this
        -- unbound overlay does not have.
        local announce = S.after[#S.after]
        S.after = {}
        S.now += 350
        announce.fn()
        assert(#S.reads == 1, T.dump())
        local said = #S.speech
        T.answerList(1, { value = { status = "text", text = "Init Preset" } })
        assert(#S.speech == said + 1, T.said(said))
        assert(T.last().text == "Preset, button, Init Preset" and T.last().interrupt == false, T.said(said))
    "#);
}

/// A Tab in the first moments, before the arrival is announced: the Tab's sentence stands, and the
/// arrival's is not made over it (its read would take the place of the Tab's).
#[test]
fn a_key_before_the_arrival_is_announced_keeps_its_own_sentence() {
    run(r#"
        local S = T.S
        local o = T.fields()
        o._lastActiveId = S.origin.id
        o:_deactivate()
        S.after = {}
        o:_activate()
        local announce = S.after[#S.after]
        S.after = {}
        T.tab()
        assert(o.focus == 2 and T.last().text == "Init, button", T.last().text)
        local said = #S.speech
        S.now += 350
        announce.fn()
        assert(#S.reads == 0 and #S.speech == said, T.said(said))
        assert(T.count("[activate] 'Synth': a key reached it before its arrival was announced; not announcing the "
          .. "arrival") == 1, T.dump())
    "#);
}

/// A hotspot toggle with a name read off the screen: pressed, its state is watched until it changes
/// or the watch gives up, then announced with its hint — the name read off the loop, the state and
/// the hint in the same sentence, as they were.
#[test]
fn a_watched_toggle_says_its_read_name_with_its_state_and_hint() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addHotspotToggle({ label = "Mic 1", at = { 10, 20 }, ocrLabel = { 30, 40, 90, 56 },
            onColor = { 255, 255, 255 }, offColor = { 20, 20, 20 }, hint = { off = "the arrows pick presets" } })
        end)
        T.tab(); T.tab()
        assert(o.focus == 3 and #S.reads == 1)
        T.answerList(1, { label = { status = "text", text = "Kick" } })
        assert(T.last().text == "Kick, toggle button, off", T.last().text)
        T.ret()
        local said = #S.speech
        for _ = 1, 10 do
          S.now += 120
          T.runDue()
        end
        assert(T.count("[watch] Synth: gave up after") == 1, T.dump())
        assert(#S.reads == 2 and #S.speech == said, "the watch's announcement waits for its read: " .. T.said(said))
        T.answerList(2, { label = { status = "text", text = "Kick" } })
        assert(T.last().text == "Kick, toggle button, off, the arrows pick presets", T.said(said))
        assert(T.last().interrupt == true, "the press's announcement interrupts, as it did")
    "#);
}

/// Return on one toggle, Tab on to the next before the first one's watch has ended: the focused
/// toggle's sentence is the one said, and the watch's announcement of the toggle the user left is
/// not made — it would have taken the read's key, and the sentence, of the toggle they are on.
#[test]
fn a_toggles_watch_after_a_tab_does_not_take_the_focused_controls_place() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addHotspotToggle({ label = "Mic 1", at = { 10, 20 }, ocrLabel = { 30, 40, 90, 56 },
            onColor = { 255, 255, 255 }, offColor = { 20, 20, 20 } })
          o:addHotspotToggle({ label = "Mic 2", at = { 110, 20 }, ocrLabel = { 130, 40, 190, 56 },
            onColor = { 255, 255, 255 }, offColor = { 20, 20, 20 } })
        end)
        T.tab(); T.tab()
        assert(o.focus == 3 and #S.reads == 1)
        T.answerList(1, { label = { status = "text", text = "Kick" } })
        T.ret()
        T.tab()
        assert(o.focus == 4 and #S.reads == 2, T.dump())
        local said = #S.speech
        for _ = 1, 10 do
          S.now += 120
          T.runDue()
        end
        assert(T.count("[watch] Synth: 'Mic 1' not announced — a later key reached the overlay") == 1, T.dump())
        assert(#S.reads == 2 and #S.speech == said, "the watch asked no read and said nothing: " .. T.said(said))
        T.answerList(2, { label = { status = "text", text = "Snare" } })
        assert(o.focus == 4 and T.said(said) == "Snare, toggle button, off", T.said(said))
    "#);
}

/// What the `[read]` line tells apart: the words the recogniser placed, and the ones whose place
/// the host estimated (on Windows the second recogniser's answer, how a lone digit is read); and the
/// language the read was made in. A reading of two lines is said as one phrase.
#[test]
fn the_read_line_counts_estimated_words_apart_and_names_the_language() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.tab(); T.tab()
        T.answerList(1, { value = { status = "text", text = "64", approx = true, lang = "de-DE" } })
        assert(T.last().text == "Preset, button, Alt+P, 64", T.last().text)
        assert(T.count('[read] \'Preset\' = "64" (region 34,7 180x15, 0 ms, 0 words, 1 placed by estimate, de-DE); '
          .. 'spoken') == 1, T.dump())
        T.tab(); T.tab()
        T.answerList(2, { value = { status = "text", text = "Init\nPreset" } })
        assert(T.last().text == "Preset, button, Alt+P, Init Preset", T.last().text)
    "#);
}

/// A calibration shot reads nothing: no host.ocr.read, nothing said.
#[test]
fn a_calibration_shot_reads_nothing() {
    run(r#"
        local S = T.S
        local o = T.fields()
        local said = #S.speech
        assert(o:calibrationShot() == true)
        assert(#S.reads == 0 and #S.speech == said, T.dump())
    "#);
}
