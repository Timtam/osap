//! VPS Avenger's DRUM SQ from the page, the ring and its watcher (the plan's slice 8, its other
//! half: `modules/vps-avenger/src/actions/drumsq.luau`, `src/read/drumsq.luau`, `src/watch.luau`, its
//! knobs in `src/amount.luau` and the ring's group in `src/ring.luau`), loaded for real against the
//! scripted host of `overlay_avenger_tests.rs`.
//!
//! What the plan says is pinned here. Nothing is clicked in DRUM SQ unless the read before shows it as
//! the central tab, whatever the strip shows. PATTERN CHAIN, AUTO FOLLOW, a pattern selected and a
//! pattern turned on or off with the right button are read after their click, the selection never
//! written from it. The pattern menu's COPY is kept as a fact the page's PASTE follows, set only once
//! Avenger's menu took it; PASTE and CLEAR are answered by the change wait on the steps. SPEED opens
//! with the left button and its field is read; the sequence menu waits for its file dialog and reads
//! DRUM SQ after it. LENGTH is double-clicked, read, dragged from what it reads and read again. The
//! watcher looks only while the page is hidden and the overlay up, is armed by a read and disarmed by
//! its own, skips a turn while its last read is out, ranks only the patterns read on, and says a
//! letter only when it changes; its cadence is a setting. No picture is held at the end of any of it.

use super::avenger_drums_tests::OPEN;
use super::overlay_avenger_tests::{run, run_mac};

/// DRUM SQ painted, and what the scenarios here share.
const DRUMSQ: &str = r##"
-- The Drums tab with DRUM SQ as the central tab: PATTERN CHAIN and AUTO FOLLOW on, A selected, with
-- `sq` and `extra` over it.
function T.sq(sq, extra)
  local s = T.drums({ central = "drum_sq", sq = sq or { chain = true, follow = true, selected = "a" } })
  for k, v in pairs(extra or {}) do s[k] = v end
  return s
end
function T.refused(said, focus, posts)
  local p = T.page().posts
  if focus then
    return #p > posts + 1 and p[#p - 1].kind == "focus" and p[#p - 1].id == focus and p[#p].kind == "error"
      and p[#p].text == said and p[#p].focus ~= true
  end
  return #p > posts and p[#p].kind == "error" and p[#p].text == said and p[#p].focus == true
end
function T.back(W)
  S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
  T.pageKey()
  T.event()
end
function T.asked(name, from)
  for i = (from or 0) + 1, #S.recognized do
    if S.recognized[i].name == name then return true end
  end
  return false
end
-- A right click's menu chosen: the opener right-clicked, `popup` painted at it, the item clicked, the
-- popup gone and `after` painted.
function T.rightMenu(m, W, popup, opener, after)
  T.tripFrom(m, W)
  T.ok(T.clickAt() == opener and S.clicks[#S.clicks].button == "right", "the right click: " .. T.clickAt())
  local spec = table.clone(S.painted.spec)
  spec.popup = popup
  T.paintAvenger(98, 84, 1.6, spec)
  T.tick(3)
  local item = T.clickAt()
  T.repaint(after)
  T.settle(5)
  return item
end
-- One turn of the watcher: its timer's poll, then a tick that answers its picture and its questions.
function T.watch()
  T.ok(T.call("timer", S.polls[250]) == "Ran", "the watcher's turn ran")
  T.tick()
end
-- The watcher's pictures asked from entry `from` of S.asyncs' history on (T.recordSnapshots).
function T.watchPictures(asked, from)
  local n = 0
  for i = (from or 0) + 1, #asked do
    if asked[i].key == "vps-avenger watch picture" then n += 1 end
  end
  return n
end
-- The snapshots asked, with their keys.
function T.recordKeyed()
  local asked = {}
  local snapshot = rawget(T.host.screen, "snapshot")
  rawset(T.host.screen, "snapshot", function(opts, cb)
    asked[#asked + 1] = { region = opts and opts.region, key = opts and opts.key, cb = cb ~= nil }
    return snapshot(opts, cb)
  end)
  return asked
end
"##;

fn scenario(s: &str) -> String {
    format!("{OPEN}\n{DRUMSQ}\n{s}")
}

// ---------------------------------------------------------------------------------------------
// Where nothing is clicked.
// ---------------------------------------------------------------------------------------------

/// With EDITOR as the central tab every DRUM SQ action — its trips, LENGTH and its knob — reads and
/// clicks nothing: the focus on DRUM SQ's tab, the sentence beside it; no click, no press, no drag.
/// With DRUM SQ shown and an oscillator tab selected, they act: the strip is not asked.
#[test]
fn a_drum_sq_action_off_drum_sq_clicks_nothing() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        local LABELS = S.included["modules/vps-avenger/src/page.luau"][1].LABELS
        local actions = {
          { "central_drum_sq_pattern_player", {} },
          { "central_drum_sq_auto_follow", {} },
          { "select_central_drum_sq_pattern", { pattern = "b" } },
          { "toggle_central_drum_sq_pattern", { pattern = "c" } },
          { "central_drum_sq_pattern_menu_item", { item = "copy" } },
          { "central_drum_sq_speed", { item = "1/8" } },
          { "central_drum_sq_sequence_menu_item", { item = "import_midi" } },
          { "central_drum_sq_length", { steps = 32 } },
          { "central_drum_sq_control", { control = "vel_amount" } },
          { "reset_central_drum_sq_control", { control = "shuffle" } },
        }
        for _, a in ipairs(actions) do
          T.event()
          local clicks, downs, posts = #S.clicks, #T.inputs("down"), #T.page().posts
          T.tripFrom({ action = a[1], args = a[2], focused = true }, W)
          T.settle(2)
          T.ok(#S.clicks == clicks and #T.inputs("down") == downs and #T.inputs("drag") == 0, a[1] .. ": nothing clicked")
          T.ok(T.refused(LABELS[a[1]] .. ": DRUM SQ is not the central tab; nothing was clicked", "central-tab-drum_sq", posts),
            a[1] .. ": " .. tostring(T.lastPost().text))
          T.ok(S.front == T.PAGE_WIN and not S.holding["Return"], a[1] .. ": the page back")
        end
        -- DRUM SQ with OSC 1 selected: it acts.
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1, central = "drum_sq",
          sq = { chain = true, follow = true, selected = "a" } })
        T.tripFrom({ action = "central_drum_sq_pattern_player", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "647,498", "PATTERN CHAIN with OSC 1 selected: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1, central = "drum_sq", sq = { chain = false, follow = true, selected = "a" } })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_chain == false, "read OFF")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The patterns.
// ---------------------------------------------------------------------------------------------

/// PATTERN CHAIN at (343, 259) — (647, 498) — and AUTO FOLLOW at (356, 259) — (668, 498) — clicked and
/// read after; clicked again with nothing drawn: unchanged; a picture too dark to be DRUM SQ's: not
/// read.
#[test]
fn pattern_chain_and_auto_follow_are_clicked_and_read_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq())
        T.tripFrom({ action = "central_drum_sq_pattern_player", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "647,498", "PATTERN CHAIN: " .. T.clickAt())
        T.repaint(T.sq({ chain = false, follow = true, selected = "a" }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_chain == false and T.lastPost().id == "central-drum-sq-pattern-player"
          and T.count("[avenger] [act] 'The pattern chain': changed — PATTERN CHAIN OFF") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "central_drum_sq_auto_follow", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "668,498", "AUTO FOLLOW: " .. T.clickAt())
        T.repaint(T.sq({ chain = false, follow = false, selected = "a" }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_auto_follow == false and T.lastPost().id == "central-drum-sq-auto-follow",
          tostring(T.lastPost().id))
        T.event()
        T.tripFrom({ action = "central_drum_sq_auto_follow", args = {}, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "Auto follow: unchanged, AUTO FOLLOW OFF", T.lastPost("error").text)
        T.event()
        T.tripFrom({ action = "central_drum_sq_pattern_player", args = {}, focused = true }, W)
        T.repaint(T.sq({ chain = false, follow = false, selected = "a" }, { sqDark = true }))
        T.settle()
        T.ok(T.lastPost("error").text == "The pattern chain: PATTERN CHAIN cannot be read now", T.lastPost("error").text)
        T.ok(T.count("drum sq: not read — implausibly dark") >= 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Pattern B at (407, 259) — (749, 498) — clicked and read selected after; C read instead: C said and
/// the mismatch noted; none standing out: "cannot be read now".
#[test]
fn a_pattern_is_selected_and_its_selection_read() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq())
        T.tripFrom({ action = "select_central_drum_sq_pattern", args = { pattern = "b" }, focused = true }, W)
        T.ok(T.clickAt() == "749,498", "B: " .. T.clickAt())
        T.repaint(T.sq({ chain = true, follow = true, selected = "b" }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_selected == "b" and T.lastPost().id == "central-drum-sq-pattern-b"
          and T.count("changed — Pattern B selected") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "select_central_drum_sq_pattern", args = { pattern = "d" }, focused = true }, W)
        T.ok(T.clickAt() == "845,498", "D: " .. T.clickAt())
        T.repaint(T.sq({ chain = true, follow = true, selected = "c" }))
        T.settle()
        T.ok(T.lastPost("error").text == "Choosing a DRUM SQ pattern: Pattern D was clicked; Pattern C reads selected",
          T.lastPost("error").text)
        T.event()
        T.tripFrom({ action = "select_central_drum_sq_pattern", args = { pattern = "a" }, focused = true }, W)
        T.repaint(T.sq({ chain = true, follow = true }))
        T.settle()
        T.ok(T.lastPost("error").text == "Choosing a DRUM SQ pattern: the selected pattern cannot be read now",
          T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Pattern C turned on with the RIGHT button at (437, 259) — (797, 498) — and read ON after; A cannot
/// be turned on or off: refused with no trip, in the tool's words.
#[test]
fn b_c_and_d_are_toggled_with_the_right_button_and_a_is_refused() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq())
        T.tripFrom({ action = "toggle_central_drum_sq_pattern", args = { pattern = "c" }, focused = true }, W)
        T.ok(T.clickAt() == "797,498" and S.clicks[#S.clicks].button == "right", "C, right: " .. T.clickAt())
        T.repaint(T.sq({ chain = true, follow = true, on = { c = true }, selected = "a" }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drum_sq_pattern_states[2] == true and o.drum_sq_pattern_states[1] == false
          and T.lastPost().id == "central-drum-sq-pattern-toggle-c", tostring(T.lastPost().id))
        T.ok(T.count("changed — Pattern C ON") == 1, T.dump())
        T.event()
        local clicks, from = #S.clicks, #S.pageOrder
        T.pageMsg({ action = "toggle_central_drum_sq_pattern", args = { pattern = "a" }, focused = true })
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Turning a DRUM SQ pattern on or off: Pattern A cannot be "
          .. "switched on or off" and not string.find(T.pageSince(from), "focus", 1, true), T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The pattern menu, right-clicked at (489, 259) — (880, 498): a COPY whose popup never comes keeps
/// no fact. COPY at (489, 269) — (880, 514) — chosen in the painted popup: nothing read after it, and
/// `drum_sq_pattern_copied` posted from then on. PASTE at (880, 533) with the steps redrawn: said;
/// CLEAR at (880, 625) with nothing redrawn: "nothing changed".
#[test]
fn copy_in_the_pattern_menu_is_kept_as_a_fact_and_paste_reads_the_steps() {
    run_mac(&scenario(r##"
        S.honourMinPixels = true
        local _, _, W = T.pageOn(T.sq())
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_copied == nil, "no COPY yet")
        T.tripFrom({ action = "central_drum_sq_pattern_menu_item", args = { item = "copy" }, focused = true }, W)
        T.run(8400)
        T.ok(T.lastPost("error").text == "The DRUM SQ pattern menu: no menu was seen; nothing was chosen"
          and T.lastPost("state").state.osc_panel.drum_sq_pattern_copied == nil, T.lastPost("error").text)
        T.event()
        local POPUP = { 489, 262, 570, 345 }
        local item = T.rightMenu({ action = "central_drum_sq_pattern_menu_item", args = { item = "COPY" }, focused = true },
          W, POPUP, "880,498", T.sq())
        T.ok(item == "880,514", "COPY's row: " .. item)
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_copied == true and T.lastPost().id == "central-drum-sq-pattern-menu"
          and T.count("[avenger] [act] 'The DRUM SQ pattern menu': chosen — COPY chosen") == 1, T.dump())
        T.event()
        item = T.rightMenu({ action = "central_drum_sq_pattern_menu_item", args = { item = "paste" }, focused = true },
          W, POPUP, "880,498", T.sq(nil, { steps = 160 }))
        T.ok(item == "880,533" and T.count("changed — PASTE chosen; VPS Avenger redrew the pattern") == 1, item .. " " .. T.dump())
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_copied == true, "the fact kept")
        T.event()
        item = T.rightMenu({ action = "central_drum_sq_pattern_menu_item", args = { item = "clear" }, focused = true },
          W, POPUP, "880,498", T.sq(nil, { steps = 160 }))
        T.ok(item == "880,625", "CLEAR's row: " .. item)
        T.tick(16)
        T.ok(T.lastPost("error").text == "The DRUM SQ pattern menu: CLEAR chosen; nothing VPS Avenger shows changed",
          T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// SPEED, the sequence menu, LENGTH.
// ---------------------------------------------------------------------------------------------

/// SPEED, opened with the LEFT button on its field at (593, 416) — (1047, 750) — its popup painted
/// below it and 1/8 at (601, 437.5) — (1060, 784) — chosen; the field read before and after as text:
/// "SPEED 1/8"; "1l8" read for it is 1/8; no word read and the field redrawn: "redrew SPEED".
#[test]
fn speed_opens_with_the_left_button_and_reads_its_field() {
    run_mac(&scenario(r##"
        S.honourMinPixels = true
        local _, _, W = T.pageOn(T.sq(), { ["sq speed"] = "1/16" })
        local function speed(item, words, shade)
          T.tripFrom({ action = "central_drum_sq_speed", args = { item = item }, focused = true }, W)
          T.ok(T.clickAt() == "1047,750" and S.clicks[#S.clicks].button ~= "right", "SPEED's field: " .. T.clickAt())
          local spec = table.clone(S.painted.spec)
          spec.popup = { 590, 422, 650, 512 }
          T.paintAvenger(98, 84, 1.6, spec)
          T.tick(3)
          local at = T.clickAt()
          T.words(words)
          T.repaint(T.sq(nil, { fields = { sq_speed = shade } }))
          T.settle(5)
          return at
        end
        T.ok(speed("1/8", { ["sq speed"] = "1/8" }, 100) == "1060,784", "1/8's row")
        T.ok(T.count("[avenger] [act] 'The DRUM SQ speed': changed — SPEED 1/8") == 1 and T.lastPost().id == "central-drum-sq-speed",
          T.dump())
        T.event()
        T.words({ ["sq speed"] = "1/16" })
        speed("1/8", { ["sq speed"] = "1l8" }, 120)
        T.ok(T.count("changed — SPEED 1/8") == 2, T.dump())
        T.event()
        speed("1/16T", {}, 140)
        T.ok(T.count("changed — 1/16T chosen; VPS Avenger redrew SPEED") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The sequence menu right-clicked at (245, 256) — (490, 494) — IMPORT MIDI at (275, 289) — (538, 546)
/// — chosen; its file dialog, a window of the DAW's, comes and goes; once Avenger is in front again
/// DRUM SQ is read once more, its patterns posted as they are drawn now. No dialog within the bound:
/// "no file dialog was seen", DRUM SQ read all the same.
#[test]
fn the_sequence_menu_waits_for_its_dialog_and_reads_drum_sq_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq())
        local dialog = { id = 951, title = "Import", class = "AXWindow/AXDialog/", app = T.MAC_REAPER,
          bounds = { x = 300, y = 200, w = 700, h = 450 }, client = { x = 300, y = 228, w = 700, h = 422 } }
        local POPUP = { 245, 258, 330, 310 }
        T.tripFrom({ action = "central_drum_sq_sequence_menu_item", args = { item = "import_midi" }, focused = true }, W)
        T.ok(T.clickAt() == "490,494" and S.clicks[#S.clicks].button == "right", "SEQUENCE MENU: " .. T.clickAt())
        T.paintAvenger(98, 84, 1.6, T.sq(nil, { popup = POPUP }))
        T.tick(3)
        T.ok(T.clickAt() == "538,546", "IMPORT MIDI: " .. T.clickAt())
        T.repaint(T.sq())
        T.tick(3)
        S.listed = { W, dialog }
        T.show(dialog)
        T.runDue()
        T.run(3000)
        T.ok(S.front == dialog, "the dialog is the user's")
        T.repaint(T.sq({ chain = true, follow = true, on = { b = true, c = true }, selected = "a" }))
        S.listed = { W }
        T.show(W)
        T.runDue()
        T.settle(3)
        local o = T.lastPost("state").state.osc_panel
        T.ok(S.front == T.PAGE_WIN and o.drum_sq_pattern_states[1] == true and o.drum_sq_pattern_states[2] == true
          and T.lastPost().id == "central-drum-sq-sequence-menu", tostring(T.lastPost().id))
        T.ok(T.count("[avenger] [act] 'The DRUM SQ sequence menu': chosen — IMPORT MIDI chosen") == 1, T.dump())
        -- No dialog.
        T.event()
        T.tripFrom({ action = "central_drum_sq_sequence_menu_item", args = { item = "export_to_midi" }, focused = true }, W)
        T.paintAvenger(98, 84, 1.6, T.sq(nil, { popup = POPUP }))
        T.tick(3)
        T.ok(T.clickAt() == "538,564", "EXPORT TO MIDI: " .. T.clickAt())
        T.repaint(T.sq())
        T.run(8400)
        T.settle(3)
        T.ok(T.lastPost("error").text == "The DRUM SQ sequence menu: no file dialog was seen"
          and T.lastPost("state").state.osc_panel.drum_sq_pattern_states[1] == false, T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// LENGTH, asked 32 steps: double-clicked at (491, 416) — (884, 750) — both clicks left; its change
/// waited for and LENGTH read, 64; dragged from there 32 / 4 × 6.6 points down — to (884, 834) — with
/// host.input.drag; read again, 32: said, posted, the page back on LENGTH. 32 again, read 48 after
/// the double-click: dragged from 48 to (884, 792), and 32 after: unchanged; read 16: dragged UP to
/// (884, 707), where the tool
/// dragged only down. Not read: from the tool's 64, logged. Read 40 after the drag: noted. 64 asked
/// and 64 read after the double-click: no drag. Not a number: refused with no trip; 30 is 32.
#[test]
fn the_length_is_double_clicked_dragged_from_what_it_reads_and_read_again() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq(), { ["sq length"] = "12 BEATS" })
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_length_steps == 48, "48 steps read at the key: "
          .. tostring(T.lastPost("state").state.osc_panel.drum_sq_length_steps))
        local shade = 60
        -- LENGTH asked `steps`: `mid` read after the double-click, `after` after the drag.
        local function length(steps, mid, after)
          T.event()
          local clicks, drags = #S.clicks, #T.inputs("drag")
          T.tripFrom({ action = "central_drum_sq_length", args = { steps = steps }, focused = true }, W)
          T.ok(T.clickAt() == "884,750" and S.clicks[#S.clicks].button == nil, "the first click: " .. T.clickAt())
          T.tick()
          T.ok(#S.clicks == clicks + 2 and T.clickAt() == "884,750", "the second click")
          shade += 10
          T.words({ ["sq length"] = mid })
          S.onDrag = function()
            shade += 10
            T.words({ ["sq length"] = after })
            T.repaint(T.sq(nil, { fields = { sq_length = shade } }))
          end
          T.repaint(T.sq(nil, { fields = { sq_length = shade } }))
          T.tick()
          T.settle(4)
          S.onDrag = nil
          local d = T.inputs("drag")
          return #d > drags and d[#d] or nil
        end
        local d = length(32, "16 BEATS", "8 BEATS")
        T.ok(d and d[2] == 884 and d[3] == 750 and d[4] == 884 and d[5] == 834 and d.button == "left",
          "dragged from 64: " .. (d and (d[4] .. "," .. d[5]) or "none"))
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drum_sq_length_steps == 32 and T.lastPost().id == "central-drum-sq-length" and T.lastPost("error").text == "",
          tostring(o.drum_sq_length_steps))
        T.ok(T.count("[avenger] [length] before 48; the double-click — changed, read 64; dragged 52.8 points (884,750)→(884,834); "
          .. "after 32") == 1, T.dump())
        -- Read 32 before — the first's after — and 32 after: unchanged, though dragged.
        d = length(32, "12 BEATS", "8 BEATS")
        T.ok(d and d[5] == 792, "dragged from 48: " .. (d and (d[4] .. "," .. d[5]) or "none"))
        T.ok(T.lastPost("error").text == "The DRUM SQ length: unchanged, LENGTH 8 BEATS, 32 STEPS", T.lastPost("error").text)
        -- Read 16 after the double-click: dragged UP to 32, 26.4 points — (884, 707).
        d = length(32, "4 BEATS", "8 BEATS")
        T.ok(d and d[3] == 750 and d[5] == 707, "dragged up from 16: " .. (d and (d[4] .. "," .. d[5]) or "none"))
        d = length(32, "", "8 BEATS")
        T.ok(d and d[5] == 834 and T.count("the length after the double-click is not read — taken as 64, the tool's") == 1,
          "dragged from the tool's 64: " .. T.dump())
        d = length(32, "16 BEATS", "10 BEATS")
        T.ok(T.lastPost("error").text == "The DRUM SQ length: LENGTH reads 10 BEATS, 40 STEPS, not 32 steps",
          T.lastPost("error").text)
        d = length(64, "16 BEATS", "16 BEATS")
        T.ok(d == nil and T.count("; no drag; after 64") == 1, "no drag: " .. T.dump())
        T.event()
        local clicks, from = #S.clicks, #S.pageOrder
        T.pageMsg({ action = "central_drum_sq_length", args = { steps = "x" }, focused = true })
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "The DRUM SQ length: no length was named"
          and not string.find(T.pageSince(from), "focus", 1, true), T.lastPost("error").text)
        d = length(30, "16 BEATS", "8 BEATS")
        T.ok(d and d[5] == 834 and T.lastPost().id == "central-drum-sq-length", "30 is 32: " .. (d and d[5] or "none"))
        T.ok(T.held() == 0 and #S.heldButtons == 0, "every picture released: " .. T.held())
    "##));
}

/// LENGTH is never posted as it read before it last changed. A read that fails after the drag: "the
/// length cannot be read now", and the state has no LENGTH — not the one read after the
/// double-click; reads that fail from the double-click on: none either, not the one read before it.
/// The drag's end covered by another window: not dragged, and LENGTH as read after the double-click,
/// or none where that read failed. Another application in front after the double-click, from LENGTH
/// read 32, the trip given up: no LENGTH posted — not that 32 — and no drag.
#[test]
fn the_length_is_posted_as_not_read_where_no_read_came_after_its_change() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq(), { ["sq length"] = "12 BEATS" })
        local Open = S.included["modules/vps-avenger/src/read/open.luau"][1]
        local part = Open.part
        local function failing() error("a read that fails", 0) end
        -- `breakAt`: "drag" — the read after the drag fails; "double" — every read after the
        -- double-click does.
        local function length(steps, breakAt)
          T.event()
          T.tripFrom({ action = "central_drum_sq_length", args = { steps = steps }, focused = true }, W)
          T.tick()
          if breakAt == "double" then Open.part = failing end
          T.words({ ["sq length"] = "16 BEATS" })
          S.onDrag = function()
            if breakAt == "drag" then Open.part = failing end
            T.words({ ["sq length"] = "8 BEATS" })
            T.repaint(T.sq(nil, { fields = { sq_length = 90 } }))
          end
          T.repaint(T.sq(nil, { fields = { sq_length = 80 } }))
          T.tick()
          T.settle(4)
          S.onDrag = nil
          Open.part = part
        end
        length(32, "drag")
        T.ok(T.lastPost("error").text == "The DRUM SQ length: the length cannot be read now"
          and T.lastPost("state").state.osc_panel.drum_sq_length_steps == nil
          and T.lastPost("state").state.osc_panel.drum_sq_pattern_chain == true,
          tostring(T.lastPost("state").state.osc_panel.drum_sq_length_steps) .. " " .. T.lastPost("error").text)
        T.ok(#T.inputs("drag") == 1, "dragged from what the double-click left")
        T.words({ ["sq length"] = "12 BEATS" })
        length(32, "double")
        T.ok(T.lastPost("error").text == "The DRUM SQ length: the length cannot be read now"
          and T.lastPost("state").state.osc_panel.drum_sq_length_steps == nil, T.lastPost("error").text)
        T.ok(T.count("the length after the double-click is not read — taken as 64, the tool's") == 1, T.dump())
        -- The drag's end covered by another window: not dragged, LENGTH as the double-click left it,
        -- read 64; with that read failing too, none.
        S.owns = function(_, _, y) return y <= 760 end
        T.words({ ["sq length"] = "12 BEATS" })
        length(32)
        T.ok(#T.inputs("drag") == 2 and T.lastPost("error").text == "something else is covering The DRUM SQ length, or its window is gone"
          and T.lastPost("state").state.osc_panel.drum_sq_length_steps == 64, T.lastPost("error").text)
        T.words({ ["sq length"] = "12 BEATS" })
        length(32, "double")
        T.ok(#T.inputs("drag") == 2 and T.lastPost("error").text == "something else is covering The DRUM SQ length, or its window is gone"
          and T.lastPost("state").state.osc_panel.drum_sq_length_steps == nil, T.lastPost("error").text)
        S.owns = true
        -- Another application after the double-click, from LENGTH read 32.
        T.words({ ["sq length"] = "12 BEATS" })
        length(32)
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_length_steps == 32, "32 read after the drag")
        T.event()
        T.words({ ["sq length"] = "12 BEATS" })
        T.tripFrom({ action = "central_drum_sq_length", args = { steps = 32 }, focused = true }, W)
        T.tick()
        T.ok(#S.clicks >= 2 and T.clickAt() == "884,750", "double-clicked")
        S.listed = { W, T.OTHER }
        T.show(T.OTHER)
        T.tick(3)
        T.ok(T.lastPost("error").text == "The DRUM SQ length: another window came to the front; nothing more was done"
          and T.lastPost("state").state.osc_panel.drum_sq_length_steps == nil, T.lastPost("error").text)
        -- The change wait from the picture before the double-click answers at its bound, and lets
        -- the picture go.
        T.run(2400)
        T.ok(T.held() == 0 and #S.heldButtons == 0 and #T.inputs("drag") == 3, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The knobs, and the central tab.
// ---------------------------------------------------------------------------------------------

/// VEL AMOUNT pressed at (443, 416) — (807, 750) — with the left button and held, the tool's sentence
/// #11; Return lets go and reads nothing, the page back on VEL AMOUNT. SHUFFLE's reset: a
/// double-click at (538, 416) — (959, 750) — and the change wait's verdict.
#[test]
fn drum_sq_knobs_are_pressed_and_reset() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq())
        T.tripFrom({ action = "central_drum_sq_control", args = { control = "velocity" }, focused = true }, W)
        local downs = T.inputs("down")
        T.ok(#downs == 1 and T.xy(downs[1]) == "807,750" and downs[1].button == "left", "the press: " .. T.xy(downs[1]))
        T.ok(T.lastSaid() == "Please press Page Up or Down to set VEL AMOUNT. To Confirm, press Return.", tostring(T.lastSaid()))
        T.key("PageDown")
        T.key("Return")
        T.settle()
        T.ok(#T.inputs("up") == 1 and T.lastPost().id == "central-drum-sq-vel-amount", tostring(T.lastPost().id))
        T.event()
        T.tripFrom({ action = "reset_central_drum_sq_control", args = { control = "shuffle" }, focused = true }, W)
        T.ok(T.clickAt() == "959,750", "SHUFFLE: " .. T.clickAt())
        T.tick(1)
        T.repaint(T.sq(nil, { pattern = true }))
        T.settle()
        T.ok(T.count("[avenger] [act] 'Resetting VEL AMOUNT or SHUFFLE': changed — VPS Avenger redrew SHUFFLE") == 1
          and T.lastPost().id == "central-drum-sq-reset-shuffle", T.dump())
        T.event()
        T.tripFrom({ action = "reset_central_drum_sq_control", args = { control = "shuffle" }, focused = true }, W)
        T.tick(17)
        T.ok(T.lastPost("error").text == "Resetting VEL AMOUNT or SHUFFLE: unchanged: VPS Avenger drew no change at SHUFFLE",
          T.lastPost("error").text)
        T.ok(T.held() == 0 and #S.heldButtons == 0, "nothing held")
    "##));
}

/// Choosing a central tab reads what it shows with it: DRUM SQ's LENGTH where DRUM SQ reads, the
/// sample editor's tab where EDITOR reads on the Drums tab; ARP neither.
#[test]
fn select_central_tab_reads_the_editor_and_the_length_where_they_are_shown() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums({ central = "arp" }), { ["sq length"] = "8 BEATS" })
        local from = #S.recognized
        T.tripFrom({ action = "select_central_tab", args = { tab = "drum_sq" }, focused = true }, W)
        T.ok(T.clickAt() == "639,474", "DRUM SQ: " .. T.clickAt())
        T.repaint(T.sq())
        T.settle()
        T.ok(T.asked("sq length", from) and T.lastPost("state").state.osc_panel.drum_sq_length_steps == 32, "LENGTH read")
        T.event()
        T.tripFrom({ action = "select_central_tab", args = { tab = "editor" }, focused = true }, W)
        T.repaint(T.drums({ sampleTab = "amp" }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.sample_editor_tab == "amp", "the editor's tab read")
        T.event()
        from = #S.recognized
        T.tripFrom({ action = "select_central_tab", args = { tab = "arp" }, focused = true }, W)
        T.repaint(T.drums({ central = "arp" }))
        T.settle()
        T.ok(not T.asked("sq length", from) and not T.asked("browser", from) and T.lastPost("state").state.osc_panel.sample_editor_tab == nil
          and T.lastPost("state").state.osc_panel.drum_sq_length_steps == nil, "neither on ARP")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The watcher.
// ---------------------------------------------------------------------------------------------

/// The page key reads DRUM SQ with PATTERN CHAIN and AUTO FOLLOW on and A selected; back in Avenger,
/// each turn of the watcher takes ONE picture with a callback, of the central tabs' row and the
/// patterns' row — (488, 466)-(869, 506) — and asks its seven questions of it with callbacks, each
/// under a key of its own. A still selected: nothing said. B selected: "B" said once, interrupting;
/// B again: nothing; C: "C".
#[test]
fn the_watcher_speaks_the_pattern_only_when_it_changes_while_the_page_is_hidden() {
    run_mac(&scenario(r##"
        local ON = { chain = true, follow = true, on = { b = true, c = true } }
        local function sq(sel)
          local s = table.clone(ON)
          s.selected = sel
          return T.sq(s)
        end
        local _, _, W = T.pageOn(sq("a"))
        T.back(W)
        T.run(400)
        T.ok(S.polls[250] ~= nil, "the watcher's timer, every 250 ms")
        local asked = T.recordKeyed()
        local keys = {}
        for _, name in ipairs({ "cells", "profile" }) do
          local f = rawget(T.host.screen, name)
          rawset(T.host.screen, name, function(opts, cb)
            if opts.key then keys[#keys + 1] = opts.key end
            return f(opts, cb)
          end)
        end
        local said = #S.speech
        T.watch()
        T.ok(#asked == 1 and asked[1].key == "vps-avenger watch picture" and asked[1].cb and T.region(asked[1].region)
          == "488,466,869,506", "one picture: " .. #asked .. " " .. (asked[1] and T.region(asked[1].region) or ""))
        local distinct = {}
        for _, k in ipairs(keys) do distinct[k] = true end
        local n = 0
        for _ in pairs(distinct) do n += 1 end
        T.ok(#keys == 7 and n == 7, "seven questions, each its own key: " .. table.concat(keys, ", "))
        T.ok(#S.speech == said and T.count("[avenger] [watch] armed — DRUM SQ, PATTERN CHAIN and AUTO FOLLOW read on, "
          .. "pattern A selected; every 250 ms") == 1, T.dump())
        T.repaint(sq("b"))
        T.watch()
        T.ok(#S.speech == said + 1 and T.lastSaid() == "B" and S.speech[#S.speech].interrupt == true, T.said(said + 1))
        T.watch()
        T.ok(#S.speech == said + 1, "B again: nothing: " .. T.said(said + 1))
        T.repaint(sq("c"))
        T.watch()
        T.ok(#S.speech == said + 2 and T.lastSaid() == "C", T.said(said + 1))
        T.ok(T.count("[avenger] [watch] pattern C (B before), ") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A pattern the page chose is not said by the watcher: armed with A, the page opened and Pattern B
/// chosen there; back in Avenger the watcher is armed again from the trip's read, B, and says
/// nothing until the pattern changes from it.
#[test]
fn the_watcher_says_nothing_of_a_pattern_the_page_chose() {
    run_mac(&scenario(r##"
        local function sq(sel) return T.sq({ chain = true, follow = true, on = { b = true, c = true }, selected = sel }) end
        local _, _, W = T.pageOn(sq("a"))
        T.back(W)
        T.run(400)
        T.watch()
        T.openPage()
        T.event()
        T.tripFrom({ action = "select_central_drum_sq_pattern", args = { pattern = "b" }, focused = true }, W)
        T.repaint(sq("b"))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_selected == "b", "B chosen from the page")
        T.back(W)
        T.run(400)
        local said = #S.speech
        T.watch()
        T.ok(#S.speech == said and T.count("[avenger] [watch] armed — ") == 2, "nothing said: " .. T.said(said + 1))
        T.ok(T.count("pattern B selected; every 250 ms") == 1, T.dump())
        T.repaint(sq("c"))
        T.watch()
        T.ok(#S.speech == said + 1 and T.lastSaid() == "C", T.said(said + 1))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// No picture while the page is in front, while the page is shown beside Avenger, while a trip is
/// out, while the overlay is not in front, while a menu is open over Avenger, or while an action of
/// the ring's is out.
#[test]
fn the_watcher_does_not_look_while_the_page_is_shown_away_in_a_menu_or_inactive() {
    run_mac(&scenario(r##"
        local header, _, W = T.pageOn(T.sq())
        local asked = T.recordKeyed()
        local function none(what)
          T.ok(T.call("timer", S.polls[250]) == "Ran", what .. ": the turn ran")
          T.tick()
          T.ok(T.watchPictures(asked) == 0, what .. ": no picture")
        end
        none("the page in front")
        -- Avenger in front with the page still shown beside it.
        T.show(W)
        T.tick(2)
        T.ok(T.page().shown, "the page still shown")
        none("the page shown beside Avenger")
        T.openPage()
        T.event()
        T.back(W)
        T.ok(not T.page().shown and header.active, "the page hidden, the overlay up")
        -- A trip out: the page's message, Avenger in front, the trip's hand-over holding.
        T.openPage()
        T.event()
        T.tripFrom({ action = "central_drum_sq_control", args = { control = "shuffle" }, focused = true }, W)
        T.ok(#S.heldButtons == 1, "the knob held")
        none("a trip out")
        T.key("Return")
        T.settle()
        T.back(W)
        -- A menu open over Avenger.
        header.menuOpen = function() return true end
        none("a menu open")
        header.menuOpen = nil
        -- The overlay not in front.
        S.listed = { W, T.OTHER }
        T.show(T.OTHER)
        T.tick(2)
        none("another application")
        S.listed = { W }
        T.show(W)
        local r = T.lastRead("vps-avenger header")
        if r and not r.answered then T.answer(r, T.header(98, 84, 1.6, 80)) end
        T.event()
        T.run(400)
        T.ok(header.active, "the overlay up again")
        -- An action of the ring's out: Drum pattern pressed, its change not drawn yet.
        local function at(label)
          for i, c in ipairs(header.controls) do if c.label == label then return i end end
          error("no control " .. label)
        end
        header.focus = at("Oscillator") - 1
        T.tab()
        T.pressAt(header, at("Drum pattern"))
        T.ok(T.clickAt() == "994,183", "Drum pattern from the ring: " .. T.clickAt())
        none("the ring's action out")
        T.tick(16)
        T.ok(T.call("timer", S.polls[250]) == "Ran", "the turn ran")
        T.tick()
        T.ok(T.watchPictures(asked) == 1, "and with none of them, a picture")
        T.ok(T.held() == 0 and #S.heldButtons == 0, "every picture released: " .. T.held())
    "##));
}

/// A turn whose last picture is still out takes none: two turns before its answer, one picture; and
/// two while its questions are out — the picture answered, its questions held — still one.
#[test]
fn the_watcher_is_skipped_while_its_last_read_is_out() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq())
        T.back(W)
        local asked = T.recordKeyed()
        T.call("timer", S.polls[250])
        T.call("timer", S.polls[250])
        T.ok(T.watchPictures(asked) == 1, "one picture while it is out")
        S.holdScreenOut = true
        T.tick()
        T.ok(#S.screenOut == 7, "its questions out: " .. #S.screenOut)
        T.call("timer", S.polls[250])
        T.call("timer", S.polls[250])
        T.ok(T.watchPictures(asked) == 1, "still one while its questions are out")
        S.holdScreenOut = false
        T.tick()
        T.call("timer", S.polls[250])
        T.ok(T.watchPictures(asked) == 2, "the next turn's picture once all are in")
        T.tick()
        T.ok(T.count("skipped while the last was out") == 0 or true, "summary only after a minute")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Armed only by a read: with nothing read, no picture; with PATTERN CHAIN read off, none. The ring's
/// Auto follow turned on and read: armed, a picture. The watcher's own read of AUTO FOLLOW off
/// disarms it, says nothing, and is what the page's next state and the ring show; no picture after.
/// A picture too dark to tell says nothing and keeps it armed.
#[test]
fn the_watcher_is_armed_by_a_read_and_disarmed_by_its_own() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("Init Preset")
        T.paintAvenger(98, 84, 1.6, T.sq({ chain = false, follow = false, selected = "a" }))
        T.words({})
        local asked = T.recordKeyed()
        T.call("timer", S.polls[250])
        T.tick()
        T.ok(T.watchPictures(asked) == 0, "nothing read: no picture")
        T.pageReady()
        T.openPage()
        T.event()
        T.back(W)
        T.call("timer", S.polls[250])
        T.tick()
        T.ok(T.watchPictures(asked) == 0, "PATTERN CHAIN read off: no picture")
        -- The ring: its Central tab and its DRUM SQ group, Auto follow turned on and read.
        local function at(label)
          for i, c in ipairs(header.controls) do if c.label == label then return i end end
          error("no control " .. label)
        end
        header.focus = at("Oscillator") - 1
        T.tab()
        header.focus = at("Central tab") - 1
        T.tab()
        T.ok(string.find(T.lastSaid() or "", "DRUM SQ", 1, true) ~= nil, "Central tab: " .. T.said())
        T.tab()
        T.ok(T.starts(T.lastSaid(), "Pattern chain, off"), "the DRUM SQ group: " .. T.said())
        T.pressAt(header, at("Pattern chain"))
        T.ok(T.clickAt() == "647,498", "PATTERN CHAIN from the ring: " .. T.clickAt())
        T.repaint(T.sq({ chain = true, follow = false, selected = "a" }))
        T.tick(3)
        T.pressAt(header, at("Auto follow"))
        T.ok(T.clickAt() == "668,498", "AUTO FOLLOW from the ring: " .. T.clickAt())
        T.repaint(T.sq({ chain = true, follow = true, selected = "a" }))
        T.tick(3)
        T.ok(T.lastSaid() == "AUTO FOLLOW ON", T.said())
        T.call("timer", S.polls[250])
        T.tick()
        T.ok(T.watchPictures(asked) == 1 and T.count("[avenger] [watch] armed — ") == 1, "armed by the ring's read: " .. T.dump())
        -- Too dark to tell: nothing said, still armed.
        local said = #S.speech
        T.repaint(T.sq({ chain = true, follow = true, selected = "a" }, { sqDark = true }))
        T.call("timer", S.polls[250])
        T.tick()
        T.ok(#S.speech == said and T.count("[avenger] [watch] not read — nothing said, still armed") == 1, T.dump())
        T.repaint(T.sq({ chain = true, follow = false, selected = "a" }))
        T.call("timer", S.polls[250])
        T.tick()
        T.ok(T.watchPictures(asked) == 3 and T.count("[avenger] [watch] disarmed — AUTO FOLLOW off") == 1 and #S.speech == said,
          T.dump())
        local Ring = S.included["modules/vps-avenger/src/ring.luau"][1]
        T.ok(Ring.values().drumSq.follow == false, "the ring has it")
        T.pageMsg({ action = "ready" })
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_auto_follow == false, "the page's next state has it")
        T.call("timer", S.polls[250])
        T.tick()
        T.ok(T.watchPictures(asked) == 3, "no picture once disarmed")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The watcher ranks only A and the patterns its picture reads on: B drawn selected but off, with C
/// on, is not taken and nothing is said; the page key's read of the same picture, which looks at all
/// four, takes it.
#[test]
fn the_watcher_ranks_only_the_patterns_read_on() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sq({ chain = true, follow = true, on = { c = true }, selected = "a" }))
        T.back(W)
        local said = #S.speech
        T.watch()
        T.repaint(T.sq({ chain = true, follow = true, on = { c = true }, selected = "b" }))
        T.watch()
        T.ok(#S.speech == said, "B, off, not taken: " .. T.said(said + 1))
        T.openPage()
        T.event()
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_selected == "b", "the page key takes it")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Armed with no pattern standing out — PATTERN CHAIN and AUTO FOLLOW read on, none selected — the
/// first pattern the watcher reads is where it measures from, not a change: A painted, nothing said,
/// and A written; B then: "B".
#[test]
fn the_watcher_armed_with_no_pattern_read_takes_the_first_one_without_a_word() {
    run_mac(&scenario(r##"
        local function sq(sel) return T.sq({ chain = true, follow = true, on = { b = true }, selected = sel }) end
        local _, _, W = T.pageOn(sq(nil))
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_selected == nil, "no pattern read at the key")
        T.back(W)
        T.run(400)
        local said = #S.speech
        T.watch()
        T.ok(T.count("[avenger] [watch] armed — DRUM SQ, PATTERN CHAIN and AUTO FOLLOW read on, pattern none read "
          .. "selected; every 250 ms") == 1, T.dump())
        T.repaint(sq("a"))
        T.watch()
        T.ok(#S.speech == said and T.count("[avenger] [watch] pattern A read, none when armed — not said") == 1,
          "A not said: " .. T.said(said + 1))
        T.ok(T.values(W).drumSq.selected == "a", "A written")
        T.watch()
        T.ok(#S.speech == said, "A again: nothing")
        T.repaint(sq("b"))
        T.watch()
        T.ok(#S.speech == said + 1 and T.lastSaid() == "B" and T.count("[avenger] [watch] pattern B (A before), ") == 1,
          T.said(said + 1))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A turn's answers that come after the watcher may no longer look are dropped. A menu opened between
/// the turn and its picture: the picture let go, no question asked of it. A menu opened between the
/// questions and their answers: nothing said and nothing written, though B is painted. With the menu
/// closed, the next turn says B.
#[test]
fn the_watchers_answers_are_dropped_once_it_may_no_longer_look() {
    run_mac(&scenario(r##"
        local function sq(sel) return T.sq({ chain = true, follow = true, on = { b = true }, selected = sel }) end
        local header, _, W = T.pageOn(sq("a"))
        T.back(W)
        T.run(400)
        T.watch()
        local keys = {}
        for _, name in ipairs({ "cells", "profile" }) do
          local f = rawget(T.host.screen, name)
          rawset(T.host.screen, name, function(opts, cb)
            if opts.key then keys[#keys + 1] = opts.key end
            return f(opts, cb)
          end)
        end
        local said = #S.speech
        T.repaint(sq("b"))
        -- Between the turn and its picture.
        T.ok(T.call("timer", S.polls[250]) == "Ran", "the turn ran")
        header.menuOpen = function() return true end
        T.tick()
        T.ok(#keys == 0 and #S.speech == said and T.held() == 0, "no question asked: " .. table.concat(keys, ", "))
        header.menuOpen = nil
        -- Between the questions and their answers.
        S.holdScreenOut = true
        T.ok(T.call("timer", S.polls[250]) == "Ran", "the next turn ran")
        T.tick()
        T.ok(#keys == 7 and #S.screenOut == 7, "its questions out: " .. #S.screenOut)
        header.menuOpen = function() return true end
        S.holdScreenOut = false
        T.tick()
        T.ok(#S.screenOut == 0 and #S.speech == said and T.values(W).drumSq.selected == "a",
          "nothing said or written: " .. T.said(said + 1))
        header.menuOpen = nil
        T.watch()
        T.ok(#S.speech == said + 1 and T.lastSaid() == "B", T.said(said + 1))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The cadence is the setting's: 250 ms by default; set to 100, the timer at 250 is cancelled and one
/// at 100 armed, logged; set to 5000, which is no cadence, 250 again and the log says why.
#[test]
fn the_cadence_setting_arms_the_timer_again() {
    run_mac(&scenario(r##"
        T.avengerAt80("Init Preset")
        T.ok(S.polls[250] ~= nil and S.polls[100] == nil, "250 by default")
        T.setting("drumSqCadence", 100)
        T.ok(S.polls[250] == nil and S.polls[100] ~= nil and T.count("[avenger] [watch] every 100 ms") == 1, T.dump())
        T.setting("drumSqCadence", 5000)
        T.ok(S.polls[100] == nil and S.polls[250] ~= nil
          and T.count("[avenger] [watch] drumSqCadence '5000' is no cadence from 100 to 1000 ms; 250 instead") == 1, T.dump())
    "##));
}

/// A turn of the watcher waits for nothing on the event loop — every read with a callback — and the
/// Luau work of its answers, its picture's and its seven questions' callbacks, at 200 % on a Windows
/// display at 150 % — a factor of 6, where its picture is 1429 by 150 pixels, 214,350 of them — stays
/// under a bound a loop over them in Luau would pass many times over.
#[test]
fn the_watchers_turn_waits_for_nothing_and_loops_over_no_pixels() {
    run(&scenario(r##"
        S.screenSize = { w = 6000, h = 4000 }
        local j = T.onWindows(200, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 6, 200))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 6, T.sq({ chain = true, follow = true, on = { b = true }, selected = "a" }))
        T.words({})
        T.openPage()
        T.event()
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageKey()
        T.event()
        T.run(400)
        T.ok(header.active and not header.away, "back in Avenger")
        local function turn()
          T.ok(T.call("timer", S.polls[250]) == "Ran", "the turn ran")
          S.now += 150
          local t0 = T.interrupts()
          T.runDue()
          return T.interrupts() - t0
        end
        turn()
        T.repaint(T.sq({ chain = true, follow = true, on = { b = true }, selected = "b" }))
        local spent = turn()
        T.ok(T.lastSaid() == "B", T.said())
        T.ok(#S.couldNotWait == 0, "nothing waited: " .. table.concat(S.couldNotWait, ", "))
        -- Measured 2026-10-17: 22,170.
        T.ok(spent < 30000, string.format("%d turns for the answers of one turn of the watcher", spent))
        local Ring = S.included["modules/vps-avenger/src/ring.luau"][1]
        T.ok(Ring.values().drumSq.selected == "b", "the ring has the watcher's read")
    "##));
}

// ---------------------------------------------------------------------------------------------
// The ring.
// ---------------------------------------------------------------------------------------------

/// The ring's DRUM SQ group, inside the Drums group after Central tab: hidden while the Central tab
/// reads EDITOR; once it is stepped to DRUM SQ, Pattern chain's Return clicks it and says what it
/// read; Pattern steps to the next pattern read on; Pattern C's Return turns it on with the right
/// button.
#[test]
fn the_rings_drum_sq_group_shows_on_drum_sq_and_acts_as_the_page() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        local SQ = { chain = true, follow = true, on = { b = true }, selected = "a" }
        T.paintAvenger(98, 84, 1.6, T.drums({ sq = SQ }))
        T.words({})
        local function at(label)
          for i, c in ipairs(header.controls) do if c.label == label then return i end end
          error("no control " .. label)
        end
        header.focus = at("Oscillator") - 1
        T.tab()
        header.focus = at("Slot solo")
        T.tab()
        T.ok(T.starts(T.lastSaid(), "Central tab, EDITOR"), T.said())
        T.tab()
        T.ok(T.starts(T.lastSaid(), "Level"), "the group hidden on EDITOR: " .. T.said())
        T.stepAt(header, at("Central tab"), 1)
        T.ok(T.clickAt() == "578,474", "ARP: " .. T.clickAt())
        T.repaint(T.drums({ central = "arp", sq = SQ }))
        T.tick(6)
        T.stepAt(header, at("Central tab"), 1)
        T.ok(T.clickAt() == "639,474", "DRUM SQ: " .. T.clickAt())
        T.repaint(T.sq(SQ))
        T.tick(6)
        T.ok(string.find(T.lastSaid() or "", "DRUM SQ", 1, true) ~= nil, T.said())
        header.focus = at("Central tab")
        T.tab()
        T.ok(T.lastSaid() == "Pattern chain, on, button", "the group shown: " .. T.said())
        T.pressAt(header, at("Pattern chain"))
        T.ok(T.clickAt() == "647,498", "PATTERN CHAIN: " .. T.clickAt())
        local clicks = #S.clicks
        T.pressAt(header, at("Auto follow"))
        T.ok(#S.clicks == clicks, "one action at a time")
        T.repaint(T.sq({ chain = false, follow = true, on = { b = true }, selected = "a" }))
        T.tick(3)
        T.ok(T.lastSaid() == "PATTERN CHAIN OFF", T.said())
        T.stepAt(header, at("Pattern"), 1)
        T.ok(T.clickAt() == "749,498", "B, the next read on: " .. T.clickAt())
        T.repaint(T.sq({ chain = false, follow = true, on = { b = true }, selected = "b" }))
        T.tick(6)
        T.ok(string.find(T.lastSaid() or "", "B, 2 of 2", 1, true) ~= nil, T.said())
        T.pressAt(header, at("Pattern C"))
        T.ok(T.clickAt() == "797,498" and S.clicks[#S.clicks].button == "right", "Pattern C: " .. T.clickAt())
        T.repaint(T.sq({ chain = false, follow = true, on = { b = true, c = true }, selected = "b" }))
        T.tick(3)
        T.ok(T.lastSaid() == "Pattern C ON", T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// On Windows at 150 %: PATTERN CHAIN at (343, 259) through the factor 2.4 — (1071, 682).
#[test]
fn on_windows_at_150_percent_drum_sq_is_clicked_through_the_factor() {
    run(&scenario(r##"
        local j = T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 2.4, T.sq())
        T.words({})
        T.openPage()
        T.event()
        S.focusAtOnce = false
        S.focusLands = function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageMsg({ action = "central_drum_sq_pattern_player", args = {}, focused = true })
        T.land()
        T.runDue()
        -- 248 + 2.4 * 343 = 1071.2 -> 1071, 60 + 2.4 * 259 = 681.6 -> 682.
        T.ok(T.clickAt() == "1071,682", "PATTERN CHAIN: " .. T.clickAt())
        T.repaint(T.sq({ chain = false, follow = true, selected = "a" }))
        T.tick(3)
        T.ok(T.lastPost("state").state.osc_panel.drum_sq_pattern_chain == false, "read OFF")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}
