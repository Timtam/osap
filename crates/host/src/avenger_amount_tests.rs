//! VPS Avenger's knobs turned from the page (the plan's slice 7, `modules/vps-avenger/src/amount.luau`
//! and the single-knob read of `src/read/knob.luau`), on the overlay runtime's hand-over of the keys
//! (`trip:handOver{ kind = "keys" }`), loaded for real against the scripted host of
//! `overlay_avenger_tests.rs`, whose `host.input` now records a press held, its moves and its
//! release, and whose disable lets go of a held button as the host does.
//!
//! What the plan says is pinned here. A macro knob, an oscillator's knob or the filter's band is
//! pressed where the tool presses it — the right button for Pan and Random Pitch — and HELD, after
//! the keyboard is handed over and before the tool's sentence, said behind the screen reader: PageUp,
//! PageDown, Return and Escape captured, so none of them reaches Avenger or the DAW. Each PageUp or
//! PageDown is one move of one table point times the overlay's factor, clamped at 106 points from the
//! press (the filter at 368 and 509), a key at the limit still moving there and playing Slider.wav
//! behind the `sounds` setting. Return lets go at once, then the knob is READ off a picture once
//! Avenger has drawn what the keys did — a change wait from the picture before the press — and posted,
//! or said not read where that read comes to nothing or the trip is given up meanwhile; Escape lets go
//! and reads nothing, the knob's value posted as not read where it moved; the filter reads nothing at
//! all, and posts the strip as read before the press. An oscillator's control is refused on the Drums and the Trig tab with nothing
//! pressed. Every end lets go of the button exactly once — after a disable, the host's release is the
//! only one. The reader reads one knob off its own small picture with one list of a few hundred
//! points, and the page key's read reads Fine Tune's ring once for both of its values. A calibrating
//! run writes the knob's pictures before and after.

use super::overlay_avenger_tests::{run, run_mac};

/// Avenger at 80 % in REAPER's floating window on a Mac, painted as `spec` says, its words `words`,
/// the page open in front; and what the scenarios here share.
const OPEN: &str = r##"
local S = T.S
function T.pageOn(spec, words)
  local header, box, W = T.avengerAt80("Init Preset")
  T.pageReady()
  T.paintAvenger(98, 84, 1.6, spec)
  T.words(words or {})
  T.openPage()
  T.event()
  return header, box, W
end
function T.settle(n) T.tick(n or 3) end
-- Every snapshot asked for from here on, with its options and whether it had a callback.
function T.recordSnapshots()
  local asked = {}
  local snapshot = rawget(T.host.screen, "snapshot")
  rawset(T.host.screen, "snapshot", function(opts, cb)
    asked[#asked + 1] = { region = opts and opts.region, change = opts and opts.change, cb = cb ~= nil }
    return snapshot(opts, cb)
  end)
  return asked
end
-- A key typed into Avenger during the hand-over: taken by its capture.
function T.key(k, n)
  for _ = 1, n or 1 do T.ok(T.typed(k) == "taken", k .. " taken by the hand-over's capture") end
end
function T.sliders()
  local n = 0
  for _, s in ipairs(S.sounds) do if s == "sounds/Slider.wav" then n += 1 end end
  return n
end
-- The live snapshot handles' regions, as one string.
function T.liveRegions()
  local out = {}
  for s in pairs(S.live) do out[#out + 1] = T.region(s.region) end
  table.sort(out)
  return table.concat(out, " ")
end
T.MACRO = { osc = 1, selected = "osc", index = 1, macroShown = 1, amounts = { 40, 0, 0 } }
T.MACRO_WORDS = { ["macro tabs"] = "MACRO 1", ["macro macro1"] = "Cutoff" }
function T.macroSpec(a1)
  local s = table.clone(T.MACRO)
  s.amounts = { a1, 0, 0 }
  return s
end
T.OSC = { osc = 1, selected = "osc", index = 1, volume = 150, transpose = 0, fine = 0, macroShown = 1,
  amounts = { 40, 0, 0 } }
T.OSC_WORDS = { ["osc 1"] = "Warm Saw", ["macro tabs"] = "MACRO 1" }
"##;

fn scenario(s: &str) -> String {
    format!("{OPEN}\n{s}")
}

// ---------------------------------------------------------------------------------------------
// A macro knob.
// ---------------------------------------------------------------------------------------------

/// Macro 1's Amount: the knob's box pictured first and read once; the keys handed over — PageUp,
/// PageDown, Return and Escape captured, no flag written, the scope pinned to Avenger's window —
/// then ONE press at (736, 480) through the factor 1.6, held, and the tool's sentence #6 behind the
/// screen reader. Three PageUps are three moves of one point each (1.6 pixels, whole pixels placed as
/// the runtime places them); nothing is posted or played meanwhile. Return lets go at once, where the
/// keys left the pointer, before any read; then one change wait from the picture before the press,
/// on the knob's box, and one list of points: the page back with 70 % on the knob, the others as read
/// before, the focus on its Amount button. A PageUp after it is no move.
#[test]
fn macro_amount_presses_the_knob_hands_over_four_keys_and_moves_one_point_times_the_factor_per_key() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
        T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == 40, "40 % at the key")
        local said, lists, flags, sounds, posts = #S.speech, #S.pointLists, #S.menuOpen, #S.sounds, #T.page().posts
        local before = S.picture
        local asked = T.recordSnapshots()
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        -- (706, 450)-(766, 510) at 1.6 from (98, 84): 1228,804 to 1324,900.
        T.ok(#asked == 1 and not asked[1].cb and T.region(asked[1].region) == "1228,804,1324,900",
          "the knob's box pictured first: " .. #asked)
        T.ok(T.held() == 1 and T.liveRegions() == "1228,804,1324,900", "held through the hand-over: " .. T.liveRegions())
        T.ok(#S.pointLists == lists + 1 and S.pointLists[#S.pointLists] <= 400, "one list of points")
        for _, k in ipairs({ "PageUp", "PageDown", "Return", "Escape" }) do T.ok(S.holding[k], k .. " captured") end
        for i = flags + 1, #S.menuOpen do T.ok(S.menuOpen[i] ~= true, "no flag written") end
        T.ok(S.scopedTo == W.id, "the scope pinned to Avenger's window")
        -- (736, 480) at 1.6: 98 + 1177.6 -> 1276, 84 + 768 -> 852.
        local downs = T.inputs("down")
        T.ok(#downs == 1 and T.xy(downs[1]) == "1276,852" and downs[1].button == "left", "one press: " .. T.xy(downs[1]))
        T.ok(#S.speech == said + 1 and T.lastSaid()
          == "Please press Page Up or Down to set the amount of the Cutoff Slider. To Confirm, press Return."
          and S.speech[#S.speech].interrupt == false, T.said(said + 1))
        T.ok(T.count("[avenger] [amount] 'macro 1': before the press — Cutoff 40 percent; ") == 1, T.dump())
        T.ok(T.count("[avenger] [amount] 'macro 1': pressed (1276,852) with the left button; PageUp moves up, "
          .. "PageDown down, 1 point × 1.60 each, 374..586") == 1, T.dump())
        -- 479, 478, 477 at 1.6: 850.9 -> 850, 849.3 -> 849, 847.7 -> 847.
        T.key("PageUp", 3)
        local moves = T.inputs("move")
        T.ok(#moves == 3 and T.xy(moves[1]) == "1276,850" and T.xy(moves[2]) == "1276,849" and T.xy(moves[3]) == "1276,847",
          "three moves: " .. T.xy(moves[1]) .. " " .. T.xy(moves[2]) .. " " .. T.xy(moves[3]))
        T.ok(#S.sounds == sounds and #T.page().posts == posts and #T.inputs("up") == 0, "nothing played, posted or let go")
        -- Tab and Shift+Tab do nothing while the knob is held (Act.SWALLOWED): taken, the first saying
        -- how the hand-over ends, once and interrupting.
        T.ok(S.menuTaken["Tab"] and S.menuTaken["Shift+Tab"], "Tab and Shift+Tab swallowed")
        local sw = #S.speech
        T.key("Tab")
        T.ok(#S.speech == sw + 1 and T.lastSaid() == "Return confirms, Escape cancels."
          and S.speech[#S.speech].interrupt == true, T.said(sw + 1))
        T.key("Shift+Tab")
        T.key("Tab")
        T.ok(#S.speech == sw + 1 and #T.inputs("move") == 3 and #T.inputs("up") == 0, "said once; nothing moved or let go")
        T.repaint(T.macroSpec(70))
        local n = #S.input
        T.key("Return")
        T.ok(#S.input == n + 1 and S.input[n + 1][1] == "up" and T.xy(S.input[n + 1]) == "1276,847"
          and S.input[n + 1].button == "left", "let go at once, where the keys left it: " .. T.xy(S.input[n + 1]))
        T.ok(#S.pointLists == lists + 1, "nothing read before the release")
        T.ok(T.count("'macro 1': Return — the left button released at (1276,847) after 3 moves (net -3, 0 at the limit), "
          .. "the last ") == 1, T.dump())
        T.settle()
        local waits = {}
        for _, a in ipairs(asked) do if a.change then waits[#waits + 1] = a end end
        T.ok(#asked == 2 and #waits == 1, "one change wait: " .. #asked)
        local cw = waits[1].change
        T.ok(T.region(waits[1].region) == "1228,804,1324,900" and cw.settle == 50 and cw.timeout == 2000
          and cw.tolerance == 16 and cw.minPixels == 4, "the box")
        T.ok(cw.from ~= nil and cw.from.picture == before and T.region(cw.from.region) == "1228,804,1324,900",
          "from the picture before the press")
        T.ok(#S.pointLists == lists + 2 and S.pointLists[#S.pointLists] <= 400, "one list of points after")
        T.ok(T.count("[avenger] [amount] 'macro 1': read Cutoff 70 percent (Cutoff 40 percent before) — changed true") == 1,
          T.dump())
        local st = T.lastPost("state").state.macro_panel
        T.ok(st.amount_values[1] == 70 and st.amount_values[2] == 0 and st.amount_values[3] == 0,
          tostring(st.amount_values[1]))
        T.ok(T.lastPost("error").text == "" and T.lastPost().kind == "focus" and T.lastPost().id == "macro-1-slot-1-amount",
          tostring(T.lastPost().id))
        T.ok(S.front == T.PAGE_WIN and #S.heldButtons == 0 and T.ups() == 1 and T.held() == 0, "the page back, nothing held")
        T.ok(not S.menuTaken["Tab"] and not S.menuTaken["Shift+Tab"], "Tab and Shift+Tab given back")
        T.ok(T.typed("PageUp") ~= "taken" and #T.inputs("move") == 3, "a PageUp after it moves nothing")
    "##));
}

/// Avenger's window leaving the front between the read before the press and the hand-over (while
/// the press point is checked): handOver answers `not_in_front`, having given the trip up itself —
/// nothing is pressed, the picture is let go, and the page hears the give-up's words, not "the
/// keyboard was not handed over" on a trip already over.
#[test]
fn a_hand_over_refused_for_the_front_presses_nothing_and_leaves_the_words_to_the_give_up() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
        S.owns = function()
          S.listed, S.front, S.chain = { W, T.OTHER }, T.OTHER, { T.OTHER }
          return true
        end
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        T.settle(2)
        T.ok(#T.inputs("down") == 0 and T.held() == 0 and not S.holding["Return"], "nothing pressed, nothing held")
        T.ok(T.count("the keyboard was not handed over") == 0 and T.count("done after the trip ended") == 0, T.dump())
        T.ok(T.lastPost("error").text == "A macro's amount: another window came to the front; nothing more was done",
          tostring(T.lastPost("error").text))
    "##));
}

/// Escape lets go and reads nothing: with no move, at the press point, the knob's value kept as read
/// before; after two PageDowns, where they left it, no change wait and no points read after the
/// press, the value posted as not read; the focus on the Amount button either way.
#[test]
fn escape_releases_without_reading_and_says_the_value_not_read() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
        local asked = T.recordSnapshots()
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        T.key("Escape")
        T.settle()
        local ups = T.inputs("up")
        T.ok(#ups == 1 and T.xy(ups[1]) == "1276,852", "let go at the press: " .. T.xy(ups[1]))
        T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == 40, "kept as read before")
        T.ok(T.lastPost("error").text == "" and T.lastPost().id == "macro-1-slot-1-amount", tostring(T.lastPost().id))
        T.ok(#asked == 1, "nothing read after: " .. #asked)
        T.event()
        local lists, from = #S.pointLists, #asked
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        T.ok(#S.pointLists == lists + 1, "the read before")
        -- 481, 482 at 1.6: 854.1 -> 854, 855.7 -> 855.
        T.key("PageDown", 2)
        T.repaint(T.macroSpec(30))
        T.key("Escape")
        T.settle()
        ups = T.inputs("up")
        T.ok(#ups == 2 and T.xy(ups[2]) == "1276,855", "let go where the keys left it: " .. T.xy(ups[2]))
        T.ok(#asked == from + 1 and #S.pointLists == lists + 1, "no change wait, no points after the press")
        T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == nil, "not read")
        T.ok(T.lastPost("error").text == "" and T.lastPost().id == "macro-1-slot-1-amount", tostring(T.lastPost().id))
        T.ok(T.count("'macro 1': Escape — the left button released at (1276,855) after 2 moves (net +2, 0 at the limit)") == 1,
          T.dump())
        T.ok(T.held() == 0 and T.ups() == 2, "nothing held")
    "##));
}

/// The travel is clamped at 106 points from the press, either way: the 106th PageUp is at 374, the
/// 107th moves there again and plays Slider.wav, and so does every one after it; a PageDown from
/// there moves at once. 107 PageDowns from the press stop at 586. With the sounds off, the move is
/// made and nothing is played. Every key is one move.
#[test]
fn the_travel_is_clamped_at_106_points_with_slider_wav_at_the_limit_and_one_move_per_key() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        local keys = 0
        local function up(n) T.key("PageUp", n); keys += n end
        local function down(n) T.key("PageDown", n); keys += n end
        local function last() local m = T.inputs("move"); return T.xy(m[#m]) end
        up(106)
        -- 374 at 1.6: 84 + 598.4 + 0.5 -> 682.
        T.ok(last() == "1276,682" and T.sliders() == 0, "106 points up, no sound: " .. last())
        up(1)
        T.ok(last() == "1276,682" and T.sliders() == 1, "the 107th: the limit again, Slider.wav")
        up(1)
        T.ok(last() == "1276,682" and T.sliders() == 2, "and again")
        T.ok(T.count("'macro 1': PageUp at the limit (374) — Slider.wav; the next ones are counted") == 1, T.dump())
        down(1)
        -- 375: 84 + 600 + 0.5 -> 684.
        T.ok(last() == "1276,684" and T.sliders() == 2, "back at once: " .. last())
        T.ok(#T.inputs("move") == keys, "one move a key: " .. #T.inputs("move") .. " of " .. keys)
        T.key("Return")
        T.settle(20)
        T.ok(T.count("after 109 moves (net -105, 2 at the limit)") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        down(107)
        -- 586: 84 + 937.6 + 0.5 -> 1022.
        T.ok(last() == "1276,1022" and T.sliders() == 3, "107 points down: the limit, Slider.wav: " .. last())
        T.setting("sounds", false)
        down(1)
        T.ok(last() == "1276,1022" and T.sliders() == 3, "the sounds off: the move, no sound")
        T.ok(#T.inputs("move") == keys, "one move a key")
        T.key("Escape")
        T.settle()
        T.ok(T.held() == 0 and T.ups() == 2, "let go twice in all")
    "##));
}

/// Return with nothing moved reads the knob at once — a plain picture, no change wait — and says
/// nothing of it being unchanged; the press and the release both at the press point.
#[test]
fn return_with_no_move_reads_at_once_and_claims_nothing() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
        local asked = T.recordSnapshots()
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        T.key("Return")
        T.settle()
        T.ok(#asked == 2 and asked[2].cb and asked[2].change == nil and T.region(asked[2].region) == "1228,804,1324,900",
          "a plain picture after: " .. #asked)
        T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == 40 and T.lastPost("error").text == ""
          and T.lastPost().id == "macro-1-slot-1-amount", tostring(T.lastPost("error").text))
        local d, u = T.inputs("down"), T.inputs("up")
        T.ok(#d == 1 and #u == 1 and T.xy(d[1]) == "1276,852" and T.xy(u[1]) == "1276,852", T.xy(u[1]))
        T.ok(T.held() == 0, "nothing held")
    "##));
}

/// Moves Avenger does not show: the change wait runs to its bound, two seconds, and the knob is read
/// all the same — said "unchanged" in the error region, the value read posted.
#[test]
fn a_move_avenger_does_not_show_is_said_unchanged_at_the_bound() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        T.key("PageUp", 2)
        T.key("Return")
        T.tick(10)
        T.ok(T.lastPost("error") == nil or T.lastPost("error").text ~= "A macro's amount: unchanged, Cutoff 40 percent",
          "not before the bound")
        T.run(2000)
        T.settle()
        T.ok(T.lastPost("error").text == "A macro's amount: unchanged, Cutoff 40 percent" and T.lastPost("error").focus == true,
          tostring(T.lastPost("error").text))
        T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == 40, "the value read")
        T.ok(T.count("[avenger] [amount] 'macro 1': read Cutoff 40 percent (Cutoff 40 percent before) — changed false") == 1,
          T.dump())
        T.ok(T.held() == 0, "nothing held")
    "##));
}

/// The read after Return that comes to nothing — the change wait answers no picture, or one in which
/// the knob has no pointer: the knob's words in the error region, "cannot be read now", its value
/// posted as not read, every picture let go.
#[test]
fn a_read_after_return_that_reads_nothing_says_the_knob_cannot_be_read_now() {
    for how in ["no picture", "no pointer"] {
        let s = r##"
            local how = "@HOW@"
            local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
            if how == "no picture" then
              -- The change wait answered with no picture, on the next turn, as a capture that failed.
              local snapshot = rawget(T.host.screen, "snapshot")
              rawset(T.host.screen, "snapshot", function(opts, cb)
                if cb ~= nil and opts and opts.change ~= nil then
                  S.screenOut[#S.screenOut + 1] = { cb = cb, value = nil, why = T.E("failed", "the capture failed") }
                  return
                end
                return snapshot(opts, cb)
              end)
            end
            T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
            T.key("PageUp", 2)
            if how == "no pointer" then T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1 }) end
            T.key("Return")
            T.settle()
            T.ok(T.lastPost("error").text == "A macro's amount: Cutoff's knob cannot be read now",
              how .. ": " .. tostring(T.lastPost("error").text))
            T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == nil, how .. ": posted as not read")
            T.ok(T.count(how == "no picture" and "'macro 1': no picture after Return — the capture failed"
              or "'macro 1': read nothing (Cutoff 40 percent before) — changed true") == 1, T.dump())
            T.ok(T.held() == 0 and T.ups() == 1, how .. ": nothing held")
        "##
        .replace("@HOW@", how);
        run_mac(&scenario(&s));
    }
}

/// On macro tab 2, with no name read for the knob: the tool's "Macro <(tab − 1) × 3 + i>" in the
/// sentence and in the value's words — Macro 4 for the first knob — and the focus on tab 2's Amount.
#[test]
fn on_macro_tab_2_an_unnamed_knob_is_macro_4_and_the_focus_is_its_tab_s_amount() {
    run_mac(&scenario(r##"
        local spec = table.clone(T.MACRO)
        spec.macroShown = 2
        local _, _, W = T.pageOn(spec, { ["macro tabs"] = "MACRO 1 MACRO 2" })
        T.ok(T.lastPost("state").state.macro_panel.selected_index == 2, "tab 2 shown")
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        T.ok(T.lastSaid() == "Please press Page Up or Down to set the amount of the Macro 4 Slider. To Confirm, press Return.",
          T.said())
        T.key("PageUp")
        local after = table.clone(spec)
        after.amounts = { 50, 0, 0 }
        T.repaint(after)
        T.key("Return")
        T.settle()
        T.ok(T.count("'macro 1': read Macro 4 50 percent (Macro 4 40 percent before)") == 1, T.dump())
        T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == 50 and T.lastPost().id == "macro-2-slot-1-amount",
          tostring(T.lastPost().id))
    "##));
}

// ---------------------------------------------------------------------------------------------
// The oscillator's knobs and the filter.
// ---------------------------------------------------------------------------------------------

/// Volume: the strip pictured and read first, then the Level knob's box; the press at (346, 68)
/// with the left button; sentence #7 with the OSC tab's name; two PageDowns down, then the travel's
/// two limits, 106 points either way of the press, each with Slider.wav; after Return one list of
/// points off the Level box, and the page back with the volume read — the strip as read before the
/// press, the oscillator's other knobs as last read — the focus on its button.
#[test]
fn osc_volume_reads_the_strip_first_and_the_level_knob_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.OSC, T.OSC_WORDS)
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.volume_values[1] == 150 and o.transpose_values[1] == 0, "read at the key")
        local asked = T.recordSnapshots()
        local lists = #S.pointLists
        T.tripFrom({ action = "osc_control", args = { control = "volume" }, focused = true }, W)
        -- The strip (230, 18)-(640, 145), then Level's box (328, 50)-(364, 86), at 1.6 from (98, 84).
        T.ok(#asked == 2 and T.region(asked[1].region) == "466,113,1122,316" and T.region(asked[2].region) == "623,164,680,222",
          "the strip, then the knob: " .. #asked)
        local d = T.inputs("down")
        T.ok(#d == 1 and T.xy(d[1]) == "652,193" and d[1].button == "left", T.xy(d[1]))
        T.ok(T.lastSaid() == "Please press Page Up or Down to set the volume of Warm Saw. To Confirm, press Return.", T.said())
        T.key("PageDown", 2)
        local m = T.inputs("move")
        -- 69, 70 at 1.6: 194.9 -> 194, 196.5 -> 196.
        T.ok(T.xy(m[1]) == "652,194" and T.xy(m[2]) == "652,196", T.xy(m[1]) .. " " .. T.xy(m[2]))
        -- The travel, 106 points either way of the press at 68, as a macro knob's: from 70, 108
        -- PageUps to -38 (84 - 60.8 + 0.5 -> 23), the 109th at the limit; 212 PageDowns to 174
        -- (84 + 278.4 + 0.5 -> 362), the 213th at the limit.
        local function last() local mv = T.inputs("move"); return T.xy(mv[#mv]) end
        T.key("PageUp", 108)
        T.ok(last() == "652,23" and T.sliders() == 0, "106 points up, no sound: " .. last())
        T.key("PageUp")
        T.ok(last() == "652,23" and T.sliders() == 1, "the limit again, Slider.wav: " .. last())
        T.key("PageDown", 212)
        T.ok(last() == "652,362" and T.sliders() == 1, "106 points down, no sound: " .. last())
        T.key("PageDown")
        T.ok(last() == "652,362" and T.sliders() == 2, "the limit again, Slider.wav: " .. last())
        T.ok(#T.inputs("move") == 2 + 108 + 1 + 212 + 1, "one move a key")
        T.repaint({ osc = 1, selected = "osc", index = 1, volume = 100, transpose = 0, fine = 0, macroShown = 1,
          amounts = { 40, 0, 0 } })
        lists = #S.pointLists
        T.key("Return")
        T.settle()
        T.ok(#S.pointLists == lists + 1, "one list of points after: " .. (#S.pointLists - lists))
        o = T.lastPost("state").state.osc_panel
        T.ok(o.volume_values[1] == 100 and o.pan_values[1] == 0 and o.transpose_values[1] == 0 and o.fine_tune_values[1] == 0
          and o.count == 1 and o.selected_index == 1, tostring(o.volume_values[1]))
        T.ok(T.lastPost("error").text == "" and T.lastPost().id == "osc-volume", tostring(T.lastPost().id))
        T.ok(T.held() == 0 and T.ups() == 1, "nothing held")
    "##));
}

/// Pan and Random Pitch press and let go with the right button, as the tool does, and the host's
/// record is asked for the right one; their sentences say the control's key with a space. The read
/// after Return posts both of its knob's values: Pan's press Level's volume and pan, Random Pitch's
/// — Fine Tune's knob, read once — Fine Tune and Random Pitch, the latter as the project reads it:
/// the Fine Tune pointer turned off the top is counted beside the upright one (knob.luau, fineKnob),
/// but a pointer no larger than a bare one has no bright pixels past it, so it reads 0; Fine Tune's
/// own press says "fine tune".
#[test]
fn pan_and_random_pitch_press_and_release_the_right_button() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.OSC, T.OSC_WORDS)
        local function turn(control, button, words, focus, paint)
          T.event()
          local lists = #S.pointLists
          T.tripFrom({ action = "osc_control", args = { control = control }, focused = true }, W)
          local d = T.inputs("down")
          T.ok(d[#d].button == button, control .. ": pressed with the " .. tostring(d[#d].button))
          T.ok(T.lastSaid() == "Please press Page Up or Down to set the " .. words .. " of Warm Saw. To Confirm, press Return.",
            T.lastSaid())
          T.ok(S.heldButtons[1] == button, control .. " held: " .. tostring(S.heldButtons[1]))
          T.key("PageUp")
          T.repaint(paint)
          T.key("Return")
          T.settle()
          local u = T.inputs("up")
          T.ok(u[#u].button == button and #S.heldButtons == 0, control .. ": let go of the " .. tostring(u[#u].button))
          T.ok(#S.pointLists == lists + 3, control .. ": the strip's dots and one ring before, one ring after: "
            .. (#S.pointLists - lists))
          T.ok(T.lastPost().id == focus, control .. ": " .. tostring(T.lastPost().id))
          return T.lastPost("state").state.osc_panel
        end
        -- Each repainted with a value of its own, so a value the read after Return did not post
        -- shows: Level's both, the third knob's both, and each of the others.
        local function osc(t)
          local s = table.clone(T.OSC)
          for k, v in pairs(t) do s[k] = v end
          return s
        end
        local o = turn("pan", "right", "pan", "osc-pan", osc({ volume = 100 }))
        T.ok(o.pan_values[1] == 0 and o.volume_values[1] == 100, "both of Level's values: " .. tostring(o.volume_values[1]))
        o = turn("random_pitch", "right", "random pitch", "osc-random-pitch", osc({ volume = 100, fine = 25 }))
        T.ok(o.fine_tune_values[1] == 25 and o.random_pitch_values[1] == 0 and o.volume_values[1] == 100,
          "both of the third knob's values: " .. tostring(o.fine_tune_values[1]) .. ", " .. tostring(o.random_pitch_values[1]))
        o = turn("fine_tune", "left", "fine tune", "osc-fine-tune", osc({ volume = 100, fine = -50 }))
        T.ok(o.fine_tune_values[1] == -50 and o.transpose_values[1] == 0, "fine tune read: " .. tostring(o.fine_tune_values[1]))
        o = turn("transpose", "left", "transpose", "osc-transpose", osc({ volume = 100, fine = -50, transpose = 24 }))
        T.ok(o.transpose_values[1] == 24 and o.fine_tune_values[1] == -50, "transpose read: " .. tostring(o.transpose_values[1]))
        T.ok(T.held() == 0 and T.ups() == 4, "let go of four times")
    "##));
}

/// An oscillator's control or the filter's band with the Drums or the Trig tab selected: nothing
/// captured, pressed or said, the error said beside the tool's focus.
#[test]
fn an_oscillator_control_on_the_drums_or_trig_tab_presses_nothing() {
    for (tab, action, args, label) in [
        ("drums", "osc_control", "{ control = \"volume\" }", "An oscillator's control"),
        ("trig", "osc_control", "{ control = \"pan\" }", "An oscillator's control"),
        ("drums", "osc_filter_control", "{ control = \"low_pass\" }", "The 2-band filter"),
    ] {
        let s = r##"
            local tab = "@TAB@"
            local _, _, W = T.pageOn({ osc = 2, selected = tab, macroShown = 1 }, T.OSC_WORDS)
            local said = #S.speech
            T.tripFrom({ action = "@ACTION@", args = @ARGS@, focused = true }, W)
            T.settle()
            T.ok(not S.holding["PageUp"] and #T.inputs("down") == 0 and #S.speech == said, "nothing captured, pressed or said")
            local posts = T.page().posts
            local Tab = tab == "drums" and "Drums" or "Trig"
            T.ok(posts[#posts].kind == "error" and posts[#posts].text
              == "@LABEL@: the " .. Tab .. " tab is selected; it has no oscillator controls"
              and posts[#posts - 1].kind == "focus" and posts[#posts - 1].id == "osc-tab-detail-group",
              tostring(posts[#posts].text))
            T.ok(T.held() == 0, "nothing held")
        "##
        .replace("@TAB@", tab)
        .replace("@ACTION@", action)
        .replace("@ARGS@", args)
        .replace("@LABEL@", label);
        run_mac(&scenario(&s));
    }
}

/// The filter's handles move along x, within 368 to 509: Low-Pass from 508 — one PageDown to 509,
/// the next at the limit with Slider.wav, a PageUp back — and High-Pass from 369 to 368. Return lets
/// go and reads nothing: no change wait, no points, the page back on the band's button with the strip
/// as read before the press.
#[test]
fn the_filter_moves_along_x_within_368_to_509_and_reads_nothing() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.OSC, T.OSC_WORDS)
        T.ok(T.lastPost("state").state.osc_panel.mute_states[1] == false, "not muted at the key")
        -- Muted since: the strip read before the press is what Return posts.
        local spec = table.clone(T.OSC)
        spec.mute = { [1] = true }
        T.repaint(spec)
        local asked = T.recordSnapshots()
        local lists = #S.pointLists
        T.tripFrom({ action = "osc_filter_control", args = { control = "low_pass" }, focused = true }, W)
        -- (508, 62) at 1.6: 911.3 -> 911, 183.7 -> 183.
        local d = T.inputs("down")
        T.ok(#d == 1 and T.xy(d[1]) == "911,183" and d[1].button == "left", T.xy(d[1]))
        T.ok(T.lastSaid() == "Please press Page Up to move Low-Pass left, or Page Down to move Low-Pass right for Warm Saw. "
          .. "To Confirm, press Return.", T.said())
        T.ok(#asked == 1, "only the strip before: " .. #asked)
        local function last() local m = T.inputs("move"); return T.xy(m[#m]) end
        T.key("PageDown")
        -- 509: 912.9 -> 912.
        T.ok(last() == "912,183" and T.sliders() == 0, last())
        T.key("PageDown")
        T.ok(last() == "912,183" and T.sliders() == 1, "the limit, Slider.wav")
        T.key("PageUp")
        T.ok(last() == "911,183", last())
        T.key("Return")
        T.settle()
        -- The one list of points is the strip's, read before the press: its dots' colours.
        T.ok(#T.inputs("up") == 1 and #asked == 1 and #S.pointLists == lists + 1, "let go, nothing read")
        T.ok(T.lastPost("error").text == "" and T.lastPost().id == "osc-low-pass", tostring(T.lastPost().id))
        T.ok(T.lastPost("state").state.osc_panel.mute_states[1] == true, "the strip as read before the press, posted")
        T.event()
        T.tripFrom({ action = "osc_filter_control", args = { control = "high_pass" }, focused = true }, W)
        -- (369, 62): 688.9 -> 688; 368: 687.3 -> 687.
        d = T.inputs("down")
        T.ok(T.xy(d[2]) == "688,183", T.xy(d[2]))
        T.key("PageUp")
        T.ok(last() == "687,183" and T.sliders() == 1, last())
        T.key("PageUp")
        T.ok(last() == "687,183" and T.sliders() == 2, "the limit, Slider.wav")
        T.key("Return")
        T.settle()
        T.ok(T.lastPost().id == "osc-high-pass" and T.held() == 0 and T.ups() == 2, tostring(T.lastPost().id))
    "##));
}

// ---------------------------------------------------------------------------------------------
// Every end lets go once.
// ---------------------------------------------------------------------------------------------

/// Every end of the trip but its own keys lets go of the button exactly once, in `onEnd`: the page
/// key, another application's window, the page's close box, the plug-in's window gone, and a move
/// that raised. No move after the release, the knob posted as not read in the state the give-up
/// posts, the give-up's own words, no picture held, and a key typed after it handed to nobody.
#[test]
fn every_ending_releases_the_button_exactly_once() {
    for end in ["key", "another application", "page closed", "gone", "failed"] {
        let s = r##"
            local ending = "@END@"
            local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
            T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
            T.key("PageUp", 2)
            if ending == "key" then
              S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = T.PAGE_WIN, { T.PAGE_WIN } end
              T.pageKey()
            elseif ending == "another application" then
              S.listed = { W, T.OTHER }
              T.show(T.OTHER)
            elseif ending == "page closed" then
              T.pageClosed("user")
            elseif ending == "gone" then
              T.show({ id = W.id, title = "VST3i: ReaEQ - Track 1", class = W.class, app = W.app, bounds = W.bounds,
                client = W.client })
            else
              S.moveRaises = "the move broke"
              T.typed("PageUp")
            end
            T.settle(4)
            T.ok(T.ups() == 1 and #S.heldButtons == 0, ending .. ": let go once: " .. T.ups())
            local seen = false
            for _, e in ipairs(S.input) do
              if e[1] == "up" then seen = true elseif seen then T.ok(e[1] ~= "move", ending .. ": a move after the release") end
            end
            T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == nil, ending .. ": posted as not read")
            local words = ({
              key = "A macro's amount: stopped by the page key; what VPS Avenger did then was not read",
              ["another application"] = "A macro's amount: another window came to the front; nothing more was done",
              ["page closed"] = "A macro's amount: the page was closed; nothing more was done",
              gone = "The plug-in window was closed. Please open it again, then press Ctrl+Shift+Win+F1 again.",
              failed = "A macro's amount: something went wrong in this module; nothing more was done",
            })[ending]
            T.ok(T.lastPost("error").text == words, ending .. ": " .. tostring(T.lastPost("error").text))
            T.ok(T.held() == 0, ending .. ": no picture held: " .. T.held())
            T.ok(T.typed("PageUp") ~= "taken" and #S.heldButtons == 0, ending .. ": a key after it handed to nobody")
            T.ok(T.count("the hand-over ends") == 1, ending .. ": " .. T.dump())
        "##
        .replace("@END@", end);
        run_mac(&scenario(&s));
    }
}

/// A give-up while the knob is read after Return — the page key, another application's window or
/// the page's close box during the change wait — posts the knob as not read, not as it was before
/// the press, and so it stays once the wait has answered. The page key with nothing moved keeps the
/// value read before: nothing was turned.
#[test]
fn a_give_up_while_the_knob_is_read_after_return_posts_it_as_not_read() {
    for end in ["key", "another application", "page closed", "key, nothing moved"] {
        let s = r##"
            local ending = "@END@"
            local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
            T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
            if ending ~= "key, nothing moved" then
              T.key("PageUp", 3)
              -- Avenger draws nothing of it: the change wait looks on, to its bound.
              T.key("Return")
              T.tick()
              T.ok(#S.changeWaits == 1, ending .. ": the change wait out: " .. #S.changeWaits)
            end
            if ending == "another application" then
              S.listed = { W, T.OTHER }
              T.show(T.OTHER)
            elseif ending == "page closed" then
              T.pageClosed("user")
            else
              S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = T.PAGE_WIN, { T.PAGE_WIN } end
              T.pageKey()
            end
            T.settle()
            local was = ending == "key, nothing moved" and 40 or nil
            local function value() return T.lastPost("state").state.macro_panel.amount_values[1] end
            T.ok(value() == was, ending .. ": " .. tostring(value()))
            local words = ({
              key = "A macro's amount: stopped by the page key; what VPS Avenger did then was not read",
              ["key, nothing moved"] = "A macro's amount: stopped by the page key; what VPS Avenger did then was not read",
              ["another application"] = "A macro's amount: another window came to the front; nothing more was done",
              ["page closed"] = "A macro's amount: the page was closed; nothing more was done",
            })[ending]
            T.ok(T.lastPost("error").text == words, ending .. ": " .. tostring(T.lastPost("error").text))
            T.run(2000)
            T.settle()
            T.ok(value() == was, ending .. ": after the wait: " .. tostring(value()))
            T.ok(T.held() == 0 and T.ups() == 1, ending .. ": nothing held, let go once")
        "##
        .replace("@END@", end);
        run_mac(&scenario(&s));
    }
}

/// A disable while the button is held: the host lets go of it at the disable, and the end that
/// follows the enable — at the first tick, or at a PageUp the stale capture takes — finds it off the
/// host's record and sends nothing more: one release in all, `onEnd` told once, the knob posted as
/// not read, no picture held.
#[test]
fn a_disable_mid_hold_is_released_by_the_host_and_not_again_after_the_enable() {
    for first in ["tick", "key"] {
        let s = r##"
            local first = "@FIRST@"
            local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
            T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
            T.key("PageUp", 2)
            T.disable()
            T.ok(#S.hostReleases == 1 and S.hostReleases[1] == "left" and #S.heldButtons == 0, "the host let go")
            T.enable()
            if first == "tick" then
              T.tick()
            else
              T.ok(T.typed("PageUp") == "taken", "the stale capture takes it")
            end
            T.settle(4)
            T.ok(#T.inputs("up") == 0 and #S.hostReleases == 1 and T.ups() == 1, first .. ": one release, the host's")
            T.ok(#T.inputs("move") == 2, first .. ": no move after the enable")
            T.ok(T.count("'macro 1': the page was closed — the host let go of the left button already; not released again") == 1,
              T.dump())
            T.ok(T.count("the hand-over ends — page closed") == 1, first .. ": told once: " .. T.dump())
            T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == nil, first .. ": posted as not read")
            T.ok(T.held() == 0, first .. ": no picture held: " .. T.held())
        "##
        .replace("@FIRST@", first);
        run_mac(&scenario(&s));
    }
}

/// A hand-over refused (a Mac without the Accessibility grant) presses nothing and says nothing,
/// and lets the knob's picture go; a press point something else covers is refused before the keys
/// are taken.
#[test]
fn a_refused_hand_over_or_press_point_presses_nothing() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
        local said = #S.speech
        S.captureRaises = "the event tap could not be created (Accessibility)"
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        T.settle()
        S.captureRaises = nil
        T.ok(#T.inputs("down") == 0 and #S.speech == said, "nothing pressed or said")
        T.ok(T.lastPost("error").text == "A macro's amount: the keyboard cannot be handed to VPS Avenger now",
          tostring(T.lastPost("error").text))
        T.ok(T.held() == 0, "the picture let go")
        T.event()
        S.owns = false
        T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
        T.settle()
        T.ok(not S.holding["PageUp"] and #T.inputs("down") == 0, "nothing captured or pressed")
        T.ok(T.lastPost("error").text == "something else is covering A macro's amount, or its window is gone", tostring(T.lastPost("error").text))
        T.ok(T.held() == 0, "the picture let go")
        S.owns = true
        -- A knob the page cannot have meant: refused at once, with no trip.
        T.event()
        local n = #S.pageOrder
        T.pageMsg({ action = "macro_amount", args = { index = 4 }, focused = true })
        T.ok(T.lastPost().text == "A macro's amount: no macro knob was named" and not string.find(T.pageSince(n), "focus 31"),
          tostring(T.lastPost().text))
        T.pageMsg({ action = "osc_control", args = { control = "cutoff" }, focused = true })
        T.ok(T.lastPost().text == "An oscillator's control: cutoff is no control of an oscillator's", tostring(T.lastPost().text))
        T.pageMsg({ action = "osc_filter_control", args = { control = "band" }, focused = true })
        T.ok(T.lastPost().text == "The 2-band filter: band is no band of the 2-band filter", tostring(T.lastPost().text))
    "##));
}

// ---------------------------------------------------------------------------------------------
// The knob reader, one knob at a time.
// ---------------------------------------------------------------------------------------------

/// Knob.read on a picture of one knob's box alone (Knob.BOX), at 50, 100 and 200 % on a Mac and at
/// 100 % on a Windows display at 150 % (factor 3): each reads what was painted, its bright pixels
/// found by one grid of the square round its ring and only the colours of the cells that hold one
/// asked — ONE list of points — and its marks inside the box. A knob painted with no pointer asks no
/// colours, and is not read.
#[test]
fn the_knob_reader_reads_one_knob_off_its_own_box_at_50_100_and_200_percent() {
    run_mac(&scenario(r##"
        T.avengerAt80("Init Preset")
        local K = S.included["modules/vps-avenger/src/read/knob.luau"][1]
        local P = S.included["modules/vps-avenger/src/read/picture.luau"][1]
        local cases = {
          { name = "Mac 50 %", k = 1, zoom = 50 },
          { name = "Mac 100 %", k = 2, zoom = 100 },
          { name = "Mac 200 %", k = 4, zoom = 200 },
          { name = "Windows 150 %, 100 %", k = 3, zoom = 100 },
        }
        local origin = { client = { x = 100, y = 100, w = 4000, h = 3000 }, class = "JUCE_1a05739a091" }
        for _, c in ipairs(cases) do
          local pl = P.placer(origin, c.k, c.zoom)
          local function read(spec, which)
            T.paintAvenger(100, 100, c.k, spec)
            local _, box = K.place(which)
            local r = pl:rect(box)
            local snap = T.snap(r)
            snap.picture = S.picture
            local lists, marks = #S.pointLists, {}
            -- In a handler, where the grids of its square wait.
            local fields, words = T.handled(K.read, pl, snap, which, marks)
            snap:release()
            for _, m in ipairs(marks) do
              T.ok(m.x >= r[1] and m.x < r[3] and m.y >= r[2] and m.y < r[4], c.name .. " " .. which .. ": a mark outside")
            end
            return fields, words, #S.pointLists - lists, S.pointLists[#S.pointLists]
          end
          -- The colours asked are those of the cells that hold a bright pixel: the list's length,
          -- the bright ones among them.
          local function bright(words, n)
            local b, asked = string.match(words, "(%d+) bright of the ring's %d+ pixels %((%d+) asked")
            return b ~= nil and tonumber(asked) == n and tonumber(b) >= 8 and tonumber(b) <= n
          end
          for _, a in ipairs({ 0, 50, 100 }) do
            local f, w, lists, n = read({ amounts = { a, 0, 0 } }, "macro1")
            T.ok(f.amount == a and lists == 1 and bright(w, n),
              string.format("%s macro %d: %s, %d list(s), %s", c.name, a, tostring(f.amount), lists, w))
          end
          for _, v in ipairs({ 0, 150, 200 }) do
            local f, w, lists, n = read({ volume = v }, "volume")
            T.ok(f.volume == v and f.pan == 0 and lists == 1 and bright(w, n),
              string.format("%s volume %d: %s, %d list(s), %s", c.name, v, tostring(f.volume), lists, w))
          end
          for _, st in ipairs({ -48, 24, 48 }) do
            local f, _, lists = read({ transpose = st }, "transpose")
            T.ok(f.transpose == st and lists == 1, string.format("%s transpose %d: %s", c.name, st, tostring(f.transpose)))
          end
          for _, ct in ipairs({ -50, 0, 100 }) do
            local f, _, lists = read({ fine = ct }, "fine_tune")
            T.ok(f.fineTune == ct and f.randomPitch ~= nil and lists == 1,
              string.format("%s fine %d: %s", c.name, ct, tostring(f.fineTune)))
          end
          local f, w, lists = read({}, "macro2")
          T.ok(f.amount == nil and lists == 0 and string.find(w, "too few", 1, true), c.name .. ": no pointer, not read: " .. w)
          f, w, lists = read({}, "random_pitch")
          T.ok(f.fineTune == nil and f.randomPitch == nil and lists == 0 and string.find(w, "too few", 1, true),
            c.name .. ": " .. w)
        end
        T.ok(T.held() == 0, "every picture released")
    "##));
}

/// The knob reader on rings with the counts measured on the second Mac test's `Sliders/…` pictures
/// at 100 % (s = 2, one pixel per point), every pixel painted where it lies from the knob's middle,
/// as the project measures it from the pixel's corner: a bare upright pointer is the 50 bright
/// pixels of `Pan/5 Avenger Pan C.png`, Volume 100 % and Pan 0; Pan 50, 75 and 100 % have 73, 87
/// and 100 with 8 beside the pointer, 25 % has 60 with 2 beside it and reads 0, as the project's
/// does; Random 50 and 100 % have 73 and 100 with 9 beside the upright, left, and Alternate 100 % the
/// same right; a Fine Tune pointer at -50 cents of 53 bright pixels, turned off the top, reads -50
/// and Random Pitch 0. The reader as it was — the project's pixels taken as two per point, a few
/// hundred points of the ring, from each pixel's middle — read these Pans as 100 and these Randoms as
/// 100.
#[test]
fn the_knob_reader_reads_the_projects_counts_pixel_for_pixel() {
    run_mac(&scenario(r##"
        T.avengerAt80("Init Preset")
        local K = S.included["modules/vps-avenger/src/read/knob.luau"][1]
        local P = S.included["modules/vps-avenger/src/read/picture.luau"][1]
        local origin = { client = { x = 100, y = 100, w = 4000, h = 3000 }, class = "JUCE_1a05739a091" }
        local pl = P.placer(origin, 2, 100)
        -- The knob's middle on screen, a whole pixel here: (100 + 2 x, 100 + 2 y).
        local function read(which, pixels)
          local p, box = K.place(which)
          local ex, ey = 100 + 2 * p[1], 100 + 2 * p[2]
          local shapes = {}
          for _, q in ipairs(pixels) do
            local x, y = ex + q[1], ey + q[2]
            shapes[#shapes + 1] = { kind = "rect", x1 = x, y1 = y, x2 = x + 1, y2 = y + 1, c = { 230, 230, 230 } }
          end
          S.picture = { bg = { 24, 24, 24 }, shapes = shapes }
          local snap = T.snap(pl:rect(box))
          snap.picture = S.picture
          local fields, words = T.handled(K.read, pl, snap, which, {})
          snap:release()
          return fields, words
        end
        -- A pointer straight up, 2 wide — dx -1 and 0 — from 4 to 28 above the middle: 50 pixels; with
        -- `side` pixels 8 to the left (or right) of it, from 10 up, and `more` beside it at dx 1 and
        -- then -2, which count as bright but not beside it.
        local function upright(side, more, right)
          local out = {}
          for dy = -28, -4 do
            out[#out + 1] = { -1, dy }
            out[#out + 1] = { 0, dy }
          end
          for i = 1, side do out[#out + 1] = { right and 8 or -8, -9 - i } end
          for i = 1, more do
            if i <= 25 then out[#out + 1] = { 1, -3 - i } else out[#out + 1] = { -2, -3 - (i - 25) } end
          end
          return out
        end
        T.ok(#upright(8, 15) == 73 and #upright(8, 29) == 87 and #upright(8, 42) == 100 and #upright(2, 8) == 60,
          "the counts painted")
        local f, w = read("pan", upright(0, 0))
        T.ok(f.volume == 100 and f.pan == 0, "bare: " .. w)
        for _, c in ipairs({ { 8, 15, false, -50 }, { 8, 29, false, -75 }, { 8, 42, true, 100 }, { 2, 8, false, 0 } }) do
          f, w = read("pan", upright(c[1], c[2], c[3]))
          T.ok(f.pan == c[4] and f.volume == 100, string.format("pan %d: %s", c[4], w))
        end
        for _, c in ipairs({ { 9, 14, false, -50 }, { 9, 41, false, -100 }, { 9, 41, true, 100 } }) do
          f, w = read("random_pitch", upright(c[1], c[2], c[3]))
          T.ok(f.randomPitch == c[4] and f.fineTune == 0, string.format("random %d: %s", c[4], w))
        end
        -- Fine Tune at -50 cents: -24 of its 48 semitones, 70° round from 135°, at 205°: a line 2 wide
        -- from 4 out, 53 pixels.
        local line, seen = {}, {}
        local a = math.rad(205)
        for r = 4, 60 do
          for _, d in ipairs({ 0, 1 }) do
            local x, y = math.floor(math.cos(a) * r + 0.5) + d, math.floor(math.sin(a) * r + 0.5)
            local key = x .. "," .. y
            if #line < 53 and not seen[key] then
              seen[key] = true
              line[#line + 1] = { x, y }
            end
          end
        end
        f, w = read("fine_tune", line)
        T.ok(#line == 53 and f.fineTune == -50 and f.randomPitch == 0, "fine -50: " .. w)
        T.ok(T.held() == 0, "every picture released")
    "##));
}

/// The page key's read with an oscillator selected reads three lists of points — the strip's dots'
/// colours, the three macro knobs' off their one grid, and Level, Transpose and the third knob off
/// another, the third ONCE for Fine Tune and Random Pitch; its values and its words as before.
#[test]
fn the_open_read_reads_fine_tunes_ring_once() {
    run_mac(&scenario(r##"
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, volume = 150, transpose = 24, fine = 0,
          macroShown = 1, amounts = { 40, 0, 0 } })
        T.words(T.OSC_WORDS)
        local lists = #S.pointLists
        T.openPage()
        T.ok(#S.pointLists == lists + 3, "three lists: " .. (#S.pointLists - lists))
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.volume_values[1] == 150 and o.pan_values[1] == 0 and o.transpose_values[1] == 24 and o.fine_tune_values[1] == 0
          and o.random_pitch_values[1] == 0, "the knobs read")
        local line
        for _, l in ipairs(S.logs) do if string.find(l, "[avenger] [read] oscillator: Level ", 1, true) then line = l end end
        T.ok(line ~= nil and string.find(line, "; Transpose ", 1, true) and string.find(line, "; Fine Tune ", 1, true)
          and string.find(line, " cents — 0; Random Pitch ", 1, true), tostring(line))
    "##));
}

/// The read after Return at the largest oscillator knob — at 200 % on a Windows display at 150 %,
/// factor 6, the third knob's ring of about 28,700 pixels — spends its turns of Luau's loops and
/// calls on the pointer's bright pixels — its one list of points the 636 colours of the cells that hold one: fewer
/// turns than the ring's pixels, so a loop over them in Luau would pass the bound.
#[test]
fn the_amounts_read_loops_over_a_few_hundred_points_in_luau() {
    run(&r##"
        local S = T.S
        -- A screen that holds all of Avenger, so that the strip's selection is read.
        S.screenSize = { w = 6000, h = 4000 }
        local j = T.onWindows(200, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 6, 200))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        local spec = { osc = 1, selected = "osc", index = 1, volume = 50, transpose = 0, fine = 0 }
        T.paintAvenger(248, 60, 6, spec)
        T.words({ ["osc 1"] = "Warm Saw" })
        T.openPage()
        T.event()
        local K = S.included["modules/vps-avenger/src/read/knob.luau"][1]
        local read, spent = K.read, {}
        K.read = function(...)
          local t0 = T.interrupts()
          local out = table.pack(read(...))
          spent[#spent + 1] = T.interrupts() - t0
          return table.unpack(out, 1, out.n)
        end
        S.focusAtOnce = false
        S.focusLands = function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageMsg({ action = "osc_control", args = { control = "fine_tune" }, focused = true })
        T.land()
        T.runDue()
        T.ok(#T.inputs("down") == 1, "pressed")
        T.ok(T.typed("PageUp") == "taken", "PageUp")
        T.repaint({ osc = 1, selected = "osc", index = 1, volume = 50, transpose = 0, fine = 25 })
        T.ok(T.typed("Return") == "taken", "Return")
        T.tick(3)
        K.read = read
        T.ok(#spent == 2 and S.pointLists[#S.pointLists] == 636, "two reads, 636 points: " .. #spent .. " "
          .. tostring(S.pointLists[#S.pointLists]))
        -- Measured 2026-10-24, every bright pixel scored against the rays near it: @MEASURED@ turns each.
        for _, n in ipairs(spent) do
          T.ok(n < @BOUND@, string.format("a knob's read spent %d turns: %s", n, table.concat(spent, ", ")))
        end
        T.ok(T.lastPost("state").state.osc_panel.fine_tune_values[1] == 25, "read")
    "##
    .replace("@MEASURED@", "23,659 and 25,052")
    .replace("@BOUND@", "27000"));
}

/// On Windows at 150 % display scaling, Avenger's own child window at (248, 60), factor 2.4: the
/// press and every move through the factor, and the limit at 374.
#[test]
fn on_windows_at_150_percent_the_moves_go_through_the_factor() {
    run(r##"
        local S = T.S
        local j = T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 2.4, { osc = 2, selected = "osc", index = 1, macroShown = 1, amounts = { 40, 0, 0 } })
        T.words({ ["macro tabs"] = "MACRO 1" })
        T.openPage()
        T.event()
        S.focusAtOnce = false
        S.focusLands = function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageMsg({ action = "macro_amount", args = { index = 1 }, focused = true })
        T.land()
        T.runDue()
        -- (736, 480) at 2.4 from (248, 60): 2014.9 -> 2014, 1212.5 -> 1212.
        local d = T.inputs("down")
        T.ok(#d == 1 and T.xy(d[1]) == "2014,1212", T.xy(d[1]))
        T.ok(T.lastSaid() == "Please press Page Up or Down to set the amount of the Macro 1 Slider. To Confirm, press Return.",
          T.said())
        T.ok(T.typed("PageUp") == "taken" and T.typed("PageUp") == "taken", "two PageUps")
        local m = T.inputs("move")
        -- 479, 478: 1210.1 -> 1210, 1207.7 -> 1207.
        T.ok(T.xy(m[1]) == "2014,1210" and T.xy(m[2]) == "2014,1207", T.xy(m[1]) .. " " .. T.xy(m[2]))
        for _ = 1, 110 do T.typed("PageUp") end
        m = T.inputs("move")
        -- 374: 60 + 897.6 + 0.5 -> 958.
        T.ok(T.xy(m[#m]) == "2014,958", T.xy(m[#m]))
        T.ok(T.typed("Escape") == "taken", "Escape")
        T.tick(3)
        T.ok(T.ups() == 1 and T.held() == 0, "let go once")
    "##);
}

/// A calibrating run writes the knob's pictures before the press and after Return — its box, with
/// the reader's marks — and the filter's band before and after, once per knob and zoom in a
/// session; every picture let go.
#[test]
fn a_calibrating_run_saves_the_knobs_pictures_once_per_knob_and_zoom() {
    run_mac(&scenario(r##"
        T.calibrate(true)
        local _, _, W = T.pageOn(T.MACRO, T.MACRO_WORDS)
        local function shots()
          local out = {}
          for _, m in ipairs(S.marked) do
            local name = string.match(m.path, "(amount%-[%w%-_]+)%.png$")
            if name then out[#out + 1] = name end
          end
          return table.concat(out, ",")
        end
        for _ = 1, 2 do
          T.event()
          T.tripFrom({ action = "macro_amount", args = { index = 1 }, focused = true }, W)
          T.key("PageUp")
          T.repaint(T.macroSpec(50))
          T.key("Return")
          T.settle()
        end
        T.ok(shots() == "amount-macro-1-before-80pct,amount-macro-1-after-80pct", shots())
        for _, m in ipairs(S.marked) do
          if string.find(m.path, "amount-macro-1-", 1, true) then
            T.ok(T.region(m.region) == "1228,804,1324,900" and #m.marks >= 1, "the box with the reader's marks")
            for _, k in ipairs(m.marks) do
              T.ok(k[1] >= 1228 and k[1] < 1324 and k[2] >= 804 and k[2] < 900, "a mark inside the box")
            end
          end
        end
        T.event()
        T.tripFrom({ action = "osc_filter_control", args = { control = "low_pass" }, focused = true }, W)
        T.key("PageUp")
        T.key("Return")
        T.settle()
        T.ok(shots() == "amount-macro-1-before-80pct,amount-macro-1-after-80pct,amount-low_pass-before-80pct,"
          .. "amount-low_pass-after-80pct", shots())
        T.ok(T.count("[avenger] [calibrate] the before picture of 'macro 1' -> ") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A knob's ring the picture holds only part of is not read: Avenger at 80 % on a screen 860 tall
/// cuts the macro knobs' rings, from y 807 to 897, at its bottom edge — the second Mac test's window
/// reached below its screen — and what is left of a ring is no count of the project's, where the
/// reader took an amount off it. The page says the macros are below the screen. On a screen that
/// holds them, the three read.
#[test]
fn a_ring_the_screens_edge_cuts_is_not_read() {
    run_mac(&scenario(r##"
        local _, _, W = T.avengerAt80("LD Lead 1")
        T.pageReady()
        S.screenSize = { w = 2560, h = 860 }
        T.paintAvenger(98, 84, 1.6, T.macroSpec(50))
        T.words(T.MACRO_WORDS)
        T.openPage()
        T.event()
        local m = T.lastPost("state").state.macro_panel
        local a = m and m.amount_values or {}
        T.ok(a[1] == nil and a[2] == nil and a[3] == nil, "none read: " .. T.jsonEncode(a))
        T.ok(T.count("knob 1 not read: the ring is not wholly in the picture") == 1, T.dump())
        T.ok(string.find(T.lastPost("error").text, "macros are below the screen", 1, true) ~= nil,
          T.lastPost("error").text)
        -- On a screen that holds them: read, the three off one grid.
        S.screenSize = { w = 2560, h = 1440 }
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.openPage()
        T.event()
        a = T.lastPost("state").state.macro_panel.amount_values
        T.ok(a[1] == 50 and a[2] == 0 and a[3] == 0, "read: " .. T.jsonEncode(a))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}
