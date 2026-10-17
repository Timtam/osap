//! VPS Avenger's Drums tab from the page and the ring (the plan's slice 8, its drums:
//! `modules/vps-avenger/src/actions/drums.luau`, `src/read/drums.luau`, a drum slot's Browse in
//! `src/handover.luau` and its knobs in `src/amount.luau`), loaded for real against the scripted host
//! of `overlay_avenger_tests.rs`, whose painted picture of Avenger now has the drumkit table
//! (`slots`) and a submenu over a popup (`submenu`).
//!
//! What the plan says is pinned here. Nothing is clicked off the Drums tab. The pattern switch, the
//! modes and the lock, a slot's name, its Mute, Solo, Lock and Loop, the drumkit's Back and Next and a
//! slot's Back, Next and Edit Sample are clicked where the tool clicks them, through the overlay's
//! frame and factor, and what they change is READ after: every slot's buttons (a Solo that mutes the
//! others shows so, one that does not shows that), the slot selected by its background — one row that
//! stands out, or "not read" — and the slots' names as text. A slot's menu is right-clicked, its item
//! chosen in Avenger's painted popup, and Copy Sample to's target chosen in the submenu that item opens,
//! followed by the drawn test and counted from the submenu's own top, which JUCE opens upward low in
//! Avenger; no submenu, or one too short for the target, is said, and the menu left open closed with
//! Escape; Save Drumkit as waits for its file dialog. A name a text read missed is no change. Browse
//! hands the keyboard over and decides by the slot's name read before and after Return; a knob is
//! pressed, held and moved through the keys; its reset says what the change wait saw.
//! The central tab is read after its click with any tab selected. The page key reads the table only on
//! the Drums tab, the names only where the strip's dots gave the count. The ring's group shows only on
//! the Drums tab. No picture is held at the end of any of it.

use super::overlay_avenger_tests::{run, run_mac};

/// Avenger at 80 % in REAPER's floating window on a Mac, painted as `spec` says, its words `words`,
/// the page open in front; and what the scenarios here share.
pub(super) const OPEN: &str = r##"
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
function T.indexOf(o, label)
  for i, c in ipairs(o.controls) do if c.label == label then return i end end
  error("no control " .. label)
end
-- The Drums tab selected, DRUMKIT chosen and EDITOR shown, with `extra`'s fields over it.
function T.drums(extra)
  local s = { osc = 2, selected = "drums", mode = "drum", central = "editor" }
  for k, v in pairs(extra or {}) do s[k] = v end
  return s
end
-- The twelve slots' names, slot i "<prefix> i" unless `over` says.
function T.slotWords(prefix, over)
  local w = {}
  for i = 1, 12 do w["slot " .. i] = (prefix or "KIT") .. " " .. i end
  for k, v in pairs(over or {}) do w[k] = v end
  return w
end
-- A list of `n` trues.
function T.all(n)
  local t = {}
  for i = 1, n do t[i] = true end
  return t
end
-- The posts from entry `from` on of `kind`.
function T.postsSince(from, kind)
  local out = {}
  local posts = T.page().posts
  for i = from + 1, #posts do
    if kind == nil or posts[i].kind == kind then out[#out + 1] = posts[i] end
  end
  return out
end
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
-- The live snapshot handles' regions, as one string.
function T.liveRegions()
  local out = {}
  for s in pairs(S.live) do out[#out + 1] = T.region(s.region) end
  table.sort(out)
  return table.concat(out, " ")
end
-- A key typed into Avenger during a hand-over of the keys: taken by its capture.
function T.key(k, n)
  for _ = 1, n or 1 do T.ok(T.typed(k) == "taken", k .. " taken by the hand-over's capture") end
end
-- The names of the regions of the text reads from `from` on that are a slot's.
function T.slotReads(from)
  local n = 0
  for i = (from or 0) + 1, #S.recognized do
    if string.match(S.recognized[i].name or "", "^slot %d+$") then n += 1 end
  end
  return n
end
-- Ticks, one at a time, until `n` clicks have been made (at most `max` ticks, default 10).
function T.untilClicks(n, max)
  for _ = 1, max or 10 do
    if #S.clicks >= n then return true end
    T.tick(1)
  end
  return #S.clicks >= n
end
-- What state.luau holds for window `W` now.
function T.values(W)
  return S.included["modules/vps-avenger/src/state.luau"][1].values(W.id)
end
"##;

fn scenario(s: &str) -> String {
    format!("{OPEN}\n{s}")
}

// ---------------------------------------------------------------------------------------------
// The Drums tab's switches.
// ---------------------------------------------------------------------------------------------

/// The pattern switch at (560, 62) — (994, 183) at 80 % — clicked and read ON after; clicked again and
/// nothing changed: "unchanged" at the bound. DRUMKIT, SEQUENCE and LOCK, each clicked at its point
/// and read after; a mode the page cannot have meant refused with no trip.
#[test]
fn the_pattern_and_the_modes_are_clicked_on_the_drums_tab_and_read_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        T.tripFrom({ action = "toggle_drum_pattern", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "994,183" and S.clicks[#S.clicks].button == nil, "the pattern switch: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drum_pattern == true and T.lastPost("error").text == "", tostring(o.drum_pattern))
        T.ok(T.count("[avenger] [act] 'The drum pattern': changed — Pattern ON") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "toggle_drum_pattern", args = {}, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "The drum pattern: unchanged, Pattern ON", T.lastPost("error").text)
        -- (613, 135): 98 + 980.8 -> 1079, 84 + 216 -> 300.
        T.event()
        T.tripFrom({ action = "select_drum_mode", args = { mode = "sq" }, focused = true }, W)
        T.ok(T.clickAt() == "1079,300", "SEQUENCE: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true, mode = "sq" }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drum_mode == "sq" and T.count("changed — SEQUENCE selected") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "select_drum_mode", args = { mode = "drum" }, focused = true }, W)
        T.ok(T.clickAt() == "1016,300", "DRUMKIT: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drum_mode == "drum", "DRUMKIT selected")
        T.event()
        T.tripFrom({ action = "select_drum_mode", args = { mode = "lock" }, focused = true }, W)
        T.ok(T.clickAt() == "1045,300", "LOCK: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true, lock = true }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drum_lock == true and T.count("changed — LOCKED") == 1, T.dump())
        T.event()
        local clicks, from = #S.clicks, #S.pageOrder
        T.pageMsg({ action = "select_drum_mode", args = { mode = "x" }, focused = true })
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "The drum mode: x is no drum mode", T.lastPost("error").text)
        T.ok(not string.find(T.pageSince(from), "focus", 1, true), "no trip: " .. T.pageSince(from))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// With an oscillator tab selected, every drum action reads the strip and clicks nothing — the tool
/// clicked the drum points over the oscillator's knobs: no click, no press, the page's focus on the
/// Drums tab and the sentence said beside it.
#[test]
fn a_drum_action_off_the_drums_tab_clicks_nothing() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1, volume = 100, transpose = 0, fine = 0 })
        local LABELS = S.included["modules/vps-avenger/src/page.luau"][1].LABELS
        local actions = {
          { "toggle_drum_pattern", {} },
          { "select_drum_mode", { mode = "drum" } },
          { "previous_drumkit", {} },
          { "next_drumkit", {} },
          { "select_drumkit_part", { index = 3 } },
          { "toggle_drumkit_button", { index = 3, button = "mute" } },
          { "drumkit_menu_item", { index = 2, item = "lock_all" } },
          { "drumkit_column", { index = 3, column = "back" } },
          { "drumkit_column", { index = 3, column = "browse" } },
          { "reset_drumkit_control", { index = 3, control = "volume" } },
          { "drumkit_control", { index = 3, control = "volume" } },
        }
        for _, a in ipairs(actions) do
          T.event()
          local clicks, downs, posts = #S.clicks, #T.inputs("down"), #T.page().posts
          T.tripFrom({ action = a[1], args = a[2], focused = true }, W)
          T.settle(2)
          T.ok(#S.clicks == clicks and #T.inputs("down") == downs, a[1] .. ": nothing clicked or pressed")
          local said = LABELS[a[1]] .. ": the Drums tab is not selected; nothing was clicked"
          local p = T.page().posts
          T.ok(#p > posts + 1 and p[#p - 1].kind == "focus" and p[#p - 1].id == "drums-tab" and p[#p].kind == "error"
            and p[#p].text == said and p[#p].focus ~= true, a[1] .. ": " .. tostring(p[#p].text))
          T.ok(S.front == T.PAGE_WIN and not S.holding["Return"], a[1] .. ": the page back, nothing handed over")
        end
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The drumkit table.
// ---------------------------------------------------------------------------------------------

/// The drumkit's Next at (623, 225) — (1095, 444) — with the twelve slots' names read before and after
/// the click, in the one text read of each: the names that changed posted and in the log, up to three
/// said; nothing changed is said at the bound, in SEQUENCE mode as the slots reading as before and
/// the sequence not read; with no names read, the change wait's verdict.
#[test]
fn back_and_next_read_the_slots_names_before_and_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums(), T.slotWords("KIT"))
        local reads = #S.recognized
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1095,444", "Next: " .. T.clickAt())
        T.ok(T.slotReads(reads) == 12, "the names read before the click: " .. T.slotReads(reads))
        T.words(T.slotWords("KIT", { ["slot 1"] = "CR Wide", ["slot 3"] = "RD Jazz", ["slot 10"] = "SD Tight",
          ["slot 12"] = "BD 909", ["slot 8"] = "SD Rim" }))
        T.repaint(T.drums())
        T.settle()
        T.ok(T.slotReads(reads) == 24, "and after it")
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_part_labels[1] == "CR Wide" and o.drumkit_part_labels[12] == "BD 909"
          and o.drumkit_part_labels[2] == "KIT 2", tostring(o.drumkit_part_labels[1]))
        T.ok(T.count("[avenger] [act] 'The next drumkit': changed — Crash reads CR Wide, Ride reads RD Jazz, "
          .. "Snare 2 reads SD Rim, and 2 more") == 1, T.dump())
        T.ok(T.lastPost("error").text == "", T.lastPost("error").text)
        -- The same names again: nothing changed, at the bound.
        T.event()
        T.tripFrom({ action = "previous_drumkit", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1085,444", "Back: " .. T.clickAt())
        T.repaint(T.drums())
        T.tick(16)
        T.ok(T.lastPost("error").text == "The previous drumkit: nothing VPS Avenger shows changed", T.lastPost("error").text)
        -- One name missed by the read after, nothing repainted: a slot read once is no change, and the
        -- change wait's verdict stands — unchanged at the bound.
        T.event()
        local now = table.clone(S.words)
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        local missed = table.clone(now)
        missed["slot 5"] = ""
        T.words(missed)
        T.tick(16)
        T.ok(T.lastPost("error").text == "The next drumkit: nothing VPS Avenger shows changed", T.lastPost("error").text)
        T.ok(T.count("redrew the drumkit") == 0 and T.count("Percussion 2 reads") == 0, T.dump())
        -- The same with the picture changed: the change wait's verdict, "changed", and no name said
        -- for it — never "unchanged" by taking the missed name as it was.
        T.event()
        T.words(now)
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.words(missed)
        T.repaint(T.drums())
        T.settle()
        T.ok(T.count("[avenger] [act] 'The next drumkit': changed — VPS Avenger redrew the drumkit") == 1
          and T.count("Percussion 2 reads") == 0, T.dump())
        T.words(now)
        -- SEQUENCE mode, no lock: the sequence is not read.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.drums({ mode = "sq" }))
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "The next drumkit: the slots read as before; the sequence is not read",
          T.lastPost("error").text)
        -- SEQUENCE with the lock: Back and Next step the drumkit, and nothing changed is said plainly.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.drums({ mode = "sq", lock = true }))
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "The next drumkit: nothing VPS Avenger shows changed", T.lastPost("error").text)
        -- No names read: the change wait decides.
        T.event()
        T.words({})
        T.paintAvenger(98, 84, 1.6, T.drums())
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.repaint(T.drums({ pattern = true }))
        T.settle()
        T.ok(T.count("[avenger] [act] 'The next drumkit': changed — VPS Avenger redrew the drumkit") == 2, T.dump())
        T.event()
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "The next drumkit: nothing VPS Avenger shows changed", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A slot's name at (282, 80) for slot 3 — (549, 212) — clicked, and the slot read as selected by its
/// background: one row lighter than the others. Two rows alike are no selection: "cannot be read
/// now", the key left out. Another slot read as selected than the one clicked is said as that.
#[test]
fn a_slot_is_selected_and_its_selection_read() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums({ slots = { selected = 1 } }))
        T.ok(T.lastPost("state").state.osc_panel.drumkit_part_selected == "1", "slot 1 read at the key")
        T.tripFrom({ action = "select_drumkit_part", args = { index = 3 }, focused = true }, W)
        T.ok(T.clickAt() == "549,212", "slot 3: " .. T.clickAt())
        T.repaint(T.drums({ slots = { selected = 3 } }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drumkit_part_selected == "3" and T.lastPost("error").text == "",
          tostring(T.lastPost("state").state.osc_panel.drumkit_part_selected))
        T.ok(T.count("[avenger] [act] 'Choosing a drum slot': changed — Ride selected") == 1, T.dump())
        T.ok(T.count("— selected: slot 3, by ") >= 1, T.dump())
        -- Two rows alike.
        T.event()
        T.tripFrom({ action = "select_drumkit_part", args = { index = 4 }, focused = true }, W)
        T.repaint(T.drums({ slots = { selected = 4, lighter = { 9 } } }))
        T.settle()
        T.ok(T.lastPost("error").text == "Choosing a drum slot: the selected slot cannot be read now"
          and T.lastPost("state").state.osc_panel.drumkit_part_selected == nil, T.lastPost("error").text)
        T.ok(T.count("— selected: two alike: slot ") >= 1, T.dump())
        -- No row standing out: no slot is taken as selected.
        T.event()
        T.tripFrom({ action = "select_drumkit_part", args = { index = 2 }, focused = true }, W)
        T.repaint(T.drums({ pattern = true }))
        T.settle()
        T.ok(T.lastPost("error").text == "Choosing a drum slot: the selected slot cannot be read now"
          and T.count("— selected: too close: slot ") >= 1, T.lastPost("error").text)
        -- Another slot reads as selected.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.drums({ slots = { selected = 3 } }))
        T.tripFrom({ action = "select_drumkit_part", args = { index = 5 }, focused = true }, W)
        T.repaint(T.drums({ slots = { selected = 6 } }))
        T.settle()
        T.ok(T.lastPost("error").text == "Choosing a drum slot: slot 5 was clicked; Closed Hi-Hat 1 reads selected",
          T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A slot's Mute, Solo, Lock and Loop clicked at the tool's points on its row and read after — every
/// slot's four with it, by the tool's own test for a lit button. Solo of slot 3 with the others
/// painted muted after posts them muted; with none painted muted, none — never the tool's assumption
/// that a Solo mutes every other slot. Loop by the same test. A button the page cannot have meant is
/// refused with no trip.
#[test]
fn a_slots_buttons_are_read_after_their_click_every_slot_with_them() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        T.tripFrom({ action = "toggle_drumkit_button", args = { index = 3, button = "mute" }, focused = true }, W)
        -- (429, 80): 98 + 686.4 -> 784, 84 + 128 -> 212.
        T.ok(T.clickAt() == "784,212", "Mute of slot 3: " .. T.clickAt())
        T.repaint(T.drums({ slots = { mute = { [3] = true } } }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_mute_states[3] == true and o.drumkit_mute_states[1] == false and T.lastPost().id == "drumkit-mute-3",
          tostring(o.drumkit_mute_states[3]) .. " " .. tostring(T.lastPost().id))
        -- Solo of slot 3, and Avenger mutes none of the others: none is posted muted.
        T.event()
        T.tripFrom({ action = "toggle_drumkit_button", args = { index = 3, button = "solo" }, focused = true }, W)
        T.ok(T.clickAt() == "804,212", "Solo: " .. T.clickAt())
        T.repaint(T.drums({ slots = { solo = { [3] = true } } }))
        T.settle()
        o = T.lastPost("state").state.osc_panel
        local muted = 0
        for i = 1, 12 do if o.drumkit_mute_states[i] then muted += 1 end end
        T.ok(muted == 0 and o.drumkit_solo_states[3] == true, "none posted muted: " .. muted)
        -- Solo of slot 5, and Avenger mutes the others: every slot's Mute as painted.
        T.event()
        T.tripFrom({ action = "toggle_drumkit_button", args = { index = 5, button = "solo" }, focused = true }, W)
        local others = T.all(12)
        others[5] = nil
        T.repaint(T.drums({ slots = { solo = { [3] = true, [5] = true }, mute = others } }))
        T.settle()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_solo_states[5] == true and o.drumkit_mute_states[1] == true and o.drumkit_mute_states[12] == true
          and o.drumkit_mute_states[5] == false, "every slot's Mute as painted")
        T.repaint(T.drums({ slots = { solo = { [3] = true } } }))
        -- Lock and Loop.
        T.event()
        T.tripFrom({ action = "toggle_drumkit_button", args = { index = 3, button = "lock" }, focused = true }, W)
        T.ok(T.clickAt() == "765,212", "Lock: " .. T.clickAt())
        T.repaint(T.drums({ slots = { solo = { [3] = true }, lock = { [3] = true } } }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drumkit_lock_states[3] == true, "Lock ON")
        T.event()
        T.tripFrom({ action = "toggle_drumkit_button", args = { index = 3, button = "loop" }, focused = true }, W)
        T.ok(T.clickAt() == "618,212", "Loop: " .. T.clickAt())
        T.repaint(T.drums({ slots = { solo = { [3] = true }, lock = { [3] = true }, loop = { [3] = true } } }))
        T.settle()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_loop_states[3] == true and o.drumkit_loop_states[4] == false, "Loop read by the same test")
        T.ok(T.count("Ride Loop ON") >= 1, T.dump())
        -- Loop clicked and nothing drawn: unchanged, never flipped.
        T.event()
        T.tripFrom({ action = "toggle_drumkit_button", args = { index = 3, button = "loop" }, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "A drum slot's Mute, Solo, Lock or Loop: unchanged, Ride Loop ON",
          T.lastPost("error").text)
        T.event()
        local clicks = #S.clicks
        T.pageMsg({ action = "toggle_drumkit_button", args = { index = 3, button = "x" }, focused = true })
        T.ok(#S.clicks == clicks and T.lastPost("error").text
          == "A drum slot's Mute, Solo, Lock or Loop: x is no button of a drum slot's", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// A slot's menu and its submenu.
// ---------------------------------------------------------------------------------------------

/// Slot 2's menu: right-clicked at (282, 64.5) — (549, 187) — its popup painted below the click, Lock
/// All chosen at its row, (323, 70.5) — (615, 197) — and once the popup has gone, every slot's Lock
/// read: twelve of twelve, the page back on the slot's Menu button.
#[test]
fn lock_all_is_chosen_in_the_painted_slot_menu_and_every_lock_read() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        T.tripFrom({ action = "drumkit_menu_item", args = { index = 2, item = "Lock All" }, focused = true }, W)
        T.ok(T.clickAt() == "549,187" and S.clicks[#S.clicks].button == "right", "slot 2, right: " .. T.clickAt())
        T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 66, 380, 200 } }))
        T.tick(3)
        T.ok(T.clickAt() == "615,197" and S.clicks[#S.clicks].button == nil, "Lock All's row: " .. T.clickAt())
        T.repaint(T.drums({ slots = { lock = T.all(12) } }))
        T.settle(5)
        local o = T.lastPost("state").state.osc_panel
        local locked = 0
        for i = 1, 12 do if o.drumkit_lock_states[i] then locked += 1 end end
        T.ok(locked == 12 and T.lastPost().id == "drumkit-menu-2", locked .. " " .. tostring(T.lastPost().id))
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — 12 of 12 slots locked") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Slot 2's Copy Sample to slot 5: the item at (615, 293) placed in the painted popup and, before its
/// click, the drawn test's two pictures for the submenu, its region from the popup's right edge on;
/// the submenu painted right of the popup and seen at 285, the target 5.75 + 4 × 11.5 below its top —
/// (727, 368), the tool's (393, 176.5) within a point — chosen in it; both gone, the names read: slot
/// 5 reads slot 2's. A submenu that never comes is said, and the popup left open closed with Escape
/// while it still counts open.
#[test]
fn copy_sample_to_opens_its_submenu_and_the_target_is_chosen_in_it() {
    run_mac(&scenario(r##"
        local words = T.slotWords("KIT", { ["slot 2"] = "SN Crack", ["slot 5"] = "PC Shaker" })
        local _, _, W = T.pageOn(T.drums(), words)
        T.tripFrom({ action = "drumkit_menu_item", args = { index = 2, item = "copy_sample_to", target_index = 5 },
          focused = true }, W)
        T.ok(T.clickAt() == "549,187" and S.clicks[#S.clicks].button == "right", "slot 2, right: " .. T.clickAt())
        local clicks = #S.clicks
        T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 66, 380, 200 } }))
        T.tick(5)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "615,293", "Copy Sample to's row, once: " .. T.clickAt())
        -- The popup seen at 549,190 157x214: the submenu looked for from its right edge, 706, on.
        T.ok(T.count("[menu] 'VPS Avenger': drawn: follows the submenu of 'A drum slot's menu': looked for in "
          .. "706,84-962,644, its menu at 549,190 157x214") == 1, T.dump())
        T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 66, 380, 200 }, submenu = { 382, 125, 460, 270 } }))
        T.ok(T.untilClicks(clicks + 2) and T.clickAt() == "727,368", "slot 5 in the submenu: " .. T.clickAt())
        T.ok(T.count("[avenger] 'A drum slot's menu': the submenu at (381.9,125.6) 78.1x143.1 from Avenger's corner in "
          .. "50 % units, its first row 0.9 from its item's row; the target 51.8 below its top") == 1, T.dump())
        T.ok(T.count("— test 'drawn' sees a new popup at ") == 1, T.dump())
        T.words(T.slotWords("KIT", { ["slot 2"] = "SN Crack", ["slot 5"] = "SN Crack" }))
        T.repaint(T.drums())
        T.settle(6)
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_part_labels[5] == "SN Crack" and T.lastPost().id == "drumkit-menu-2", tostring(o.drumkit_part_labels[5]))
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Copy Sample to chosen: Percussion 2 reads SN Crack") == 1,
          T.dump())
        T.ok(#S.clicks == clicks + 2, "clicked twice in all: " .. (#S.clicks - clicks))
        -- No submenu comes.
        T.event()
        T.tripFrom({ action = "drumkit_menu_item", args = { index = 2, item = "swap_sample_with", target_index = 5 },
          focused = true }, W)
        T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 66, 380, 200 } }))
        T.tick(5)
        T.ok(T.clickAt() == "615,312", "Swap Sample with's row: " .. T.clickAt())
        local sent = #S.sent
        T.run(8400)
        T.ok(T.lastPost("error").text == "A drum slot's menu: its submenu did not open; nothing was chosen in it",
          T.lastPost("error").text)
        T.ok(#S.sent == sent + 1 and S.sent[#S.sent] == "Escape" and T.count("[avenger] 'A drum slot's menu': its popup was "
          .. "left open (no submenu) — pressing Escape to close it") == 1, T.dump())
        T.repaint(T.drums())
        T.settle(4)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Slot 10's Swap Slot with, its item at (323, 300.5) — (615, 565) — in the lower half of Avenger:
/// JUCE opens the submenu UPWARD, its bottom at the item's, and the target is counted from the
/// submenu's own top — slot 1 on its first row and slot 12 on its last, where the tool's point for
/// slot 1, the item's own row, lies on slot 12's. A submenu too short to hold the target is none to
/// choose in: "no submenu it can be chosen in was seen", never "did not open", and the popup left
/// open closed with Escape.
#[test]
fn a_submenu_opened_upward_is_counted_from_its_own_top() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums(), T.slotWords("KIT"))
        -- The target's click, with the submenu painted at `sub` once the item is clicked.
        local function swap(target, sub)
          T.event()
          T.tripFrom({ action = "drumkit_menu_item", args = { index = 10, item = "swap_slot_with", target_index = target },
            focused = true }, W)
          T.ok(T.clickAt() == "549,386" and S.clicks[#S.clicks].button == "right", "slot 10, right: " .. T.clickAt())
          local clicks = #S.clicks
          T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 190, 380, 324 } }))
          T.tick(5)
          T.ok(#S.clicks == clicks + 1 and T.clickAt() == "615,565", "Swap Slot with's row: " .. T.clickAt())
          T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 190, 380, 324 }, submenu = sub }))
          return T.untilClicks(clicks + 2) and S.clicks[#S.clicks] or nil
        end
        -- Whether click `c` lies on the submenu's row `n`: the harness paints its rows from 4 below its
        -- top, 168, 3 tall and 11.5 apart — row n's middle at 173.5 + (n - 1) x 11.5 — and a row is
        -- half a step either way of it.
        local function on(c, n) return c ~= nil and math.abs((c[2] - 84) / 1.6 - (173.5 + (n - 1) * 11.5)) <= 5.75 end
        local c = swap(1, { 382, 168, 460, 306 })
        T.ok(on(c, 1) and c[1] == 727 and not on(c, 12), "slot 1 on the first row: " .. T.clickAt())
        T.ok(T.count("[avenger] 'A drum slot's menu': the submenu at (381.9,169.4) 78.1x135.6 from Avenger's corner in 50 % "
          .. "units, its first row -125.4 from its item's row; the target 5.8 below its top") == 1, "opened upward: " .. T.dump())
        T.words(T.slotWords("KIT", { ["slot 1"] = "KIT 10", ["slot 10"] = "KIT 1" }))
        T.repaint(T.drums())
        T.settle(6)
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Snare 1 reads KIT 1, Crash reads KIT 10") == 1, T.dump())
        c = swap(12, { 382, 168, 460, 306 })
        T.ok(on(c, 12), "slot 12 on the last row: " .. T.clickAt())
        T.words(T.slotWords("KIT", { ["slot 1"] = "KIT 10", ["slot 10"] = "KIT 12", ["slot 12"] = "KIT 1" }))
        T.repaint(T.drums())
        T.settle(6)
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Snare 1 reads KIT 12, Bassdrum reads KIT 1") == 1, T.dump())
        -- Four rows: too short for slot 12.
        local sent = #S.sent
        T.ok(swap(12, { 382, 168, 460, 214 }) == nil, "nothing chosen in it")
        T.ok(T.count("which its menuItem function says is not its menu (it is ") == 1, T.dump())
        T.run(8400)
        T.ok(T.lastPost("error").text == "A drum slot's menu: no submenu it can be chosen in was seen; nothing was chosen in it",
          T.lastPost("error").text)
        T.ok(#S.sent == sent + 1 and S.sent[#S.sent] == "Escape" and T.count("[avenger] 'A drum slot's menu': its popup was "
          .. "left open (unfit submenu) — pressing Escape to close it") == 1, T.dump())
        T.repaint(T.drums())
        T.settle(4)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Swap Slot with: both rows' names read after it. Remove Sample: the slot's name read after it.
/// Save Drumkit as: its file dialog, a window of the DAW's, waited for and Avenger in front again —
/// "chosen"; with none within the bound, "no file dialog was seen".
#[test]
fn swap_and_remove_are_read_back_and_save_waits_for_its_dialog() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums(), T.slotWords("KIT", { ["slot 2"] = "SN Crack", ["slot 5"] = "PC Shaker" }))
        local function choose(args, after)
          T.event()
          T.tripFrom({ action = "drumkit_menu_item", args = args, focused = true }, W)
          T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 66, 380, 200 } }))
          T.tick(5)
          if after then after() end
        end
        choose({ index = 2, item = "swap_slot_with", target_index = 5 }, function()
          local clicks = #S.clicks
          T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 66, 380, 200 }, submenu = { 382, 145, 460, 290 } }))
          T.ok(T.untilClicks(clicks + 1), "the submenu's item: " .. T.dump())
          T.words(T.slotWords("KIT", { ["slot 2"] = "PC Shaker", ["slot 5"] = "SN Crack" }))
          T.repaint(T.drums())
          T.settle(6)
        end)
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Open Hi-Hat reads PC Shaker, Percussion 2 reads SN Crack")
          == 1, T.dump())
        -- Remove Sample on slot 2: (323, 64.5 + 126) -> (615, 389).
        choose({ index = 2, item = "remove_sample" }, function()
          T.ok(T.clickAt() == "615,389", "Remove Sample's row: " .. T.clickAt())
          T.words(T.slotWords("KIT", { ["slot 2"] = "Empty", ["slot 5"] = "SN Crack" }))
          T.repaint(T.drums())
          T.settle(6)
        end)
        T.ok(T.lastPost("state").state.osc_panel.drumkit_part_labels[2] == "Empty"
          and T.count("changed — Open Hi-Hat reads Empty") == 1, T.dump())
        -- Copy Slot to slot 5: its name as it was, its Mute lit after — the slot's settings copied,
        -- which its LEDs show.
        choose({ index = 2, item = "copy_slot_to", target_index = 5 }, function()
          local clicks = #S.clicks
          T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 66, 380, 200 }, submenu = { 382, 155, 460, 300 } }))
          T.ok(T.untilClicks(clicks + 1), "the submenu's item: " .. T.dump())
          T.repaint(T.drums({ slots = { mute = { [5] = true } } }))
          T.settle(6)
        end)
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Copy Slot to chosen: Percussion 2 reads SN Crack") == 1,
          T.dump())
        -- Save Drumkit as: (323, 64.5 + 43) -> (615, 256), then its dialog.
        local dialog = { id = 951, title = "Save", class = "AXWindow/AXDialog/", app = T.MAC_REAPER,
          bounds = { x = 300, y = 200, w = 700, h = 450 }, client = { x = 300, y = 228, w = 700, h = 422 } }
        choose({ index = 2, item = "save_drumkit_as" }, function()
          T.ok(T.clickAt() == "615,256", "Save Drumkit as's row: " .. T.clickAt())
          T.repaint(T.drums())
          T.tick(3)
          S.listed = { W, dialog }
          T.show(dialog)
          T.runDue()
          T.run(3000)
          T.ok(S.front == dialog, "the dialog is the user's")
          S.listed = { W }
          T.show(W)
          T.runDue()
          T.settle(3)
        end)
        T.ok(S.front == T.PAGE_WIN and T.lastPost("error").text == "" and T.lastPost().id == "drumkit-menu-2",
          tostring(T.lastPost("error").text))
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': chosen — Save Drumkit as chosen") == 1, T.dump())
        choose({ index = 2, item = "save_drumkit_as" }, function()
          T.repaint(T.drums())
          T.run(8400)
        end)
        T.ok(T.lastPost("error").text == "A drum slot's menu: no file dialog was seen", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A slot menu's arguments the page cannot have meant are refused at once, with no trip: no target
/// for a submenu's item, the target the slot itself, an item it does not have, a fourteenth slot.
#[test]
fn a_slot_menus_arguments_are_refused_without_a_trip() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        local cases = {
          { { index = 2, item = "copy_sample_to" }, "no target slot was named" },
          { { index = 2, item = "swap_slot_with", target_index = 2 }, "a slot is not copied or swapped with itself" },
          { { index = 2, item = "cut" }, "cut is no item of a drum slot's menu" },
          { { index = 13, item = "lock_all" }, "no drum slot was named" },
        }
        for _, c in ipairs(cases) do
          local clicks, from = #S.clicks, #S.pageOrder
          T.pageMsg({ action = "drumkit_menu_item", args = c[1], focused = true })
          T.ok(#S.clicks == clicks and T.lastPost("error").text == "A drum slot's menu: " .. c[2], T.lastPost("error").text)
          T.ok(not string.find(T.pageSince(from), "focus", 1, true) and S.front == T.PAGE_WIN, "no trip: " .. T.pageSince(from))
        end
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// A slot's columns and knobs.
// ---------------------------------------------------------------------------------------------

/// Browse on slot 3: the strip read, the slot's name pictured and held, the keyboard handed over
/// with Return alone, Browse clicked at (345, 80) — (650, 212) — and the tool's sentence #13 behind
/// the screen reader; meanwhile the slot's name is not read in the state. Return through: a change
/// wait on the name from the held picture, the table read with its text, the name posted, the page
/// back on Browse. The page key during it: the give-up, the name posted as not read.
#[test]
fn browse_hands_the_keyboard_over_and_reads_the_slots_name_after_return() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums(), T.slotWords("KIT", { ["slot 3"] = "RD Ride 1" }))
        T.ok(T.values(W).kitNames[3] == "RD Ride 1", "read at the key")
        local asked = T.recordSnapshots()
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "browse" }, focused = true }, W)
        T.ok(T.clickAt() == "650,212", "Browse: " .. T.clickAt())
        T.ok(S.holding["Return"] and not S.holding["Escape"] and T.open() == true, "Return alone, let through")
        T.ok(T.lastSaid() == "Use the Avenger browser. To return to Avenger Control, press Return."
          and S.speech[#S.speech].interrupt == false, tostring(T.lastSaid()))
        -- The name (248, 73)-(318, 87) at 1.6: (495, 201)-(607, 223).
        T.ok(T.held() == 1 and T.liveRegions() == "495,201,607,223", "the name's picture held: " .. T.liveRegions())
        T.ok(T.values(W).kitNames[3] == nil and T.values(W).kitNames[4] == "KIT 4", "slot 3's name not read meanwhile")
        T.words(T.slotWords("KIT", { ["slot 3"] = "RD Ride 2" }))
        T.repaint(T.drums())
        T.ok(T.typed("Return") == "passed", "Return through to Avenger")
        T.tick()
        T.runDue()
        T.settle()
        local waits = {}
        for _, a in ipairs(asked) do if a.change then waits[#waits + 1] = a end end
        T.ok(#waits == 1 and T.region(waits[1].region) == "495,201,607,223" and waits[1].change.settle == 50,
          "one change wait on the name: " .. #waits)
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_part_labels[3] == "RD Ride 2" and T.lastPost().id == "drumkit-browse-3" and S.front == T.PAGE_WIN,
          tostring(o.drumkit_part_labels[3]) .. " " .. tostring(T.lastPost().id))
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held: " .. T.held())
        -- The page key while the user browses.
        T.event()
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "browse" }, focused = true }, W)
        T.ok(S.holding["Return"], "handed over")
        T.pageKey()
        T.settle(2)
        T.ok(T.starts(T.lastPost("error").text, "A drum slot's Browse, Back, Next or Edit Sample: stopped by the page key"),
          tostring(T.lastPost("error").text))
        T.ok(T.lastPost("state").state.osc_panel.drumkit_part_labels[3] == nil, "slot 3's name posted as not read")
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held")
        -- A hand-over refused: nothing clicked, said, nothing held.
        T.event()
        S.captureRaises = "the Accessibility permission is not granted"
        local clicks = #S.clicks
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "browse" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "A drum slot's Browse, Back, Next or Edit Sample: the "
          .. "keyboard cannot be handed to VPS Avenger now", T.lastPost("error").text)
        S.captureRaises = nil
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held after the refusal")
    "##));
}

/// Browse decides by the slot's name: read off the held picture with a callback while the user
/// browses, and after Return. The same name with the selection repainted over the name's box —
/// slot 3 selected by the click — is "unchanged"; another name with nothing repainted is the name
/// said. Only where the name before was not read do the pixels decide: nothing repainted,
/// "unchanged".
#[test]
fn browse_decides_by_the_slots_name_read_before_and_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums({ slots = { selected = 1 } }), T.slotWords("KIT", { ["slot 3"] = "RD Ride 1" }))
        local function browse(before)
          T.event()
          local reads = #S.reads
          T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "browse" }, focused = true }, W)
          local r = T.lastRead("vps-avenger browse slot")
          T.ok(#S.reads == reads + 1 and r and r.snapshot ~= nil and T.region(r.region) == "495,201,607,223"
            and r.names[1] == "slot 3", "the name read off the held picture, with a callback")
          if before then T.answer(r, { status = "text", text = before, words = {}, lines = {} }) end
        end
        local function back()
          T.ok(T.typed("Return") == "passed", "Return through to Avenger")
          T.tick()
          T.runDue()
          T.tick(16)
        end
        -- The same name, the selection repainted.
        browse("RD Ride 1")
        T.repaint(T.drums({ slots = { selected = 3 } }))
        back()
        T.ok(T.lastPost("error").text == "A drum slot's Browse, Back, Next or Edit Sample: unchanged, Ride reads RD Ride 1",
          T.lastPost("error").text)
        T.ok(T.count("[avenger] [browse] 'slot 3': after Return — 'RD Ride 1', unchanged by the names (before: 'RD Ride 1')")
          == 1, T.dump())
        -- Another name, nothing repainted.
        browse("RD Ride 1")
        T.words(T.slotWords("KIT", { ["slot 3"] = "RD Ride 2" }))
        back()
        T.ok(T.lastPost("error").text == "" and T.lastPost().id == "drumkit-browse-3"
          and T.lastPost("state").state.osc_panel.drumkit_part_labels[3] == "RD Ride 2", T.lastPost("error").text)
        T.ok(T.count("after Return — 'RD Ride 2', changed by the names (before: 'RD Ride 1')") == 1, T.dump())
        -- The name before not read: the pixels, which show nothing new.
        browse(nil)
        back()
        T.ok(T.lastPost("error").text == "A drum slot's Browse, Back, Next or Edit Sample: unchanged, Ride reads RD Ride 2",
          T.lastPost("error").text)
        T.ok(T.count("after Return — 'RD Ride 2', unchanged by the pixels (before: not answered yet)") == 1, T.dump())
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held: " .. T.held())
    "##));
}

/// A slot's Back at (389, 80) and Next at (401, 80) — (720, 212), (740, 212) — and its name read
/// before and after; Edit Sample at (370, 80) — (690, 212) — and the selection and the central tab
/// read after it.
#[test]
fn a_slots_back_next_and_edit_sample_are_read_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums({ slots = { selected = 1 } }), T.slotWords("KIT", { ["slot 3"] = "RD A" }))
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "back" }, focused = true }, W)
        T.ok(T.clickAt() == "720,212", "Back: " .. T.clickAt())
        T.words(T.slotWords("KIT", { ["slot 3"] = "RD B" }))
        T.repaint(T.drums({ slots = { selected = 1 } }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drumkit_part_labels[3] == "RD B" and T.lastPost().id == "drumkit-back-3"
          and T.count("changed — Ride reads RD B") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "next" }, focused = true }, W)
        T.ok(T.clickAt() == "740,212", "Next: " .. T.clickAt())
        T.words(T.slotWords("KIT", { ["slot 3"] = "RD C" }))
        T.repaint(T.drums({ slots = { selected = 1 } }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drumkit_part_labels[3] == "RD C" and T.lastPost().id == "drumkit-next-3",
          tostring(T.lastPost().id))
        T.event()
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "edit_sample" }, focused = true }, W)
        T.ok(T.clickAt() == "690,212", "Edit Sample: " .. T.clickAt())
        T.repaint(T.drums({ slots = { selected = 3 }, central = "editor" }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_part_selected == "3" and o.central_panel_tab == "editor" and T.lastPost().id == "drumkit-edit-sample-3",
          tostring(o.drumkit_part_selected) .. " " .. tostring(T.lastPost().id))
        T.ok(T.count("changed — RD C selected; EDITOR shown") == 1, T.dump())
        local clicks = #S.clicks
        T.event()
        T.pageMsg({ action = "drumkit_column", args = { index = 3, column = "routing" }, focused = true })
        T.ok(#S.clicks == clicks and T.lastPost("error").text
          == "A drum slot's Browse, Back, Next or Edit Sample: routing is no column of a drum slot's", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A slot's Volume: pressed at (458, 80) — (831, 212) — with the left button and held, the tool's
/// sentence #9 with the slot's name as read at the key; one PageUp is one move of one point times the
/// factor; Return lets go once and reads nothing, the page back on the Volume button. With no name
/// read, the factory name. Pan and Pitch at their points, the left button too.
#[test]
fn a_drum_knob_is_pressed_held_and_released_through_the_keys() {
    for named in [true, false] {
        let s = r##"
        local named = @NAMED@
        local _, _, W = T.pageOn(T.drums(), named and T.slotWords("KIT", { ["slot 3"] = "SD Tight" }) or {})
        local asked = T.recordSnapshots()
        T.tripFrom({ action = "drumkit_control", args = { index = 3, control = "volume" }, focused = true }, W)
        local downs = T.inputs("down")
        T.ok(#downs == 1 and T.xy(downs[1]) == "831,212" and downs[1].button == "left", "the press: " .. T.xy(downs[1]))
        T.ok(T.lastSaid() == "Please press Page Up or Down to set volume for " .. (named and "SD Tight" or "Ride")
          .. ". To Confirm, press Return." and S.speech[#S.speech].interrupt == false, tostring(T.lastSaid()))
        -- The tool's travel, 106 points either way of the knob's row at 80.
        T.ok(T.count("pressed (831,212) with the left button; PageUp moves up, PageDown down, 1 point × 1.60 each, -26..186")
          == 1, T.dump())
        -- 79 at 1.6: 84 + 126.4 -> 210.
        T.key("PageUp")
        local moves = T.inputs("move")
        T.ok(#moves == 1 and T.xy(moves[1]) == "831,210", "one move: " .. T.xy(moves[1]))
        local n = #asked
        T.key("Return")
        T.settle()
        T.ok(#T.inputs("up") == 1 and #asked == n, "let go once, nothing pictured after: " .. (#asked - n))
        T.ok(S.front == T.PAGE_WIN and T.lastPost().id == "drumkit-volume-3" and T.lastPost("error").text == "",
          tostring(T.lastPost().id))
        -- Pan and Pitch at their points, with the left button.
        T.event()
        T.tripFrom({ action = "drumkit_control", args = { index = 3, control = "pan" }, focused = true }, W)
        downs = T.inputs("down")
        T.ok(#downs == 2 and T.xy(downs[2]) == "879,212" and downs[2].button == "left", "Pan: " .. T.xy(downs[2]))
        T.key("Escape")
        T.settle()
        T.ok(T.lastPost().id == "drumkit-pan-3", tostring(T.lastPost().id))
        T.event()
        T.tripFrom({ action = "drumkit_control", args = { index = 3, control = "pitch" }, focused = true }, W)
        downs = T.inputs("down")
        T.ok(#downs == 3 and T.xy(downs[3]) == "927,212" and downs[3].button == "left", "Pitch: " .. T.xy(downs[3]))
        T.key("Return")
        T.settle()
        T.ok(T.lastPost().id == "drumkit-pitch-3" and T.ups() == 3, tostring(T.lastPost().id))
        T.ok(T.held() == 0 and #S.heldButtons == 0, "nothing held")
        "##;
        run_mac(&scenario(&s.replace("@NAMED@", if named { "true" } else { "false" })));
    }
}

/// A slot's Volume reset: a double-click at (831, 212), both clicks with the left button; what the
/// change wait saw on the knob's box is said — redrawn, or no change at its bound — and no value is
/// claimed.
#[test]
fn a_drum_knobs_reset_says_what_the_change_wait_saw() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        T.tripFrom({ action = "reset_drumkit_control", args = { index = 3, control = "volume" }, focused = true }, W)
        T.ok(T.clickAt() == "831,212" and S.clicks[#S.clicks].button == nil, "the first click: " .. T.clickAt())
        local n = #S.clicks
        T.tick(1)
        T.ok(#S.clicks == n + 1 and T.clickAt() == "831,212" and S.clicks[#S.clicks].button == nil, "the second")
        T.repaint(T.drums({ pattern = true }))
        T.settle()
        T.ok(T.count("[avenger] [act] 'Resetting a drum slot's control': changed — VPS Avenger redrew Ride's Volume knob") == 1,
          T.dump())
        T.ok(T.lastPost().id == "drumkit-reset-volume-3" and T.lastPost("error").text == "", tostring(T.lastPost().id))
        T.event()
        T.tripFrom({ action = "reset_drumkit_control", args = { index = 3, control = "volume" }, focused = true }, W)
        T.tick(17)
        T.ok(T.lastPost("error").text == "Resetting a drum slot's control: unchanged: VPS Avenger drew no change at Ride's "
          .. "Volume knob", T.lastPost("error").text)
        T.ok(T.count("the picture changed and what this action changes did not") == 0, "no looking on")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The central tabs.
// ---------------------------------------------------------------------------------------------

/// A central tab: DRUM SQ at (338, 244) — (639, 474) — clicked on the Drums tab and read after, DRUM
/// SQ's states with it; EDITOR with an oscillator tab selected, read all the same; ARP said as
/// selected; the same tab again unchanged; one that cannot be read said so; a tab the page cannot
/// reach from here refused with no trip.
#[test]
fn a_central_tab_is_clicked_and_read_with_any_tab_selected() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        T.tripFrom({ action = "select_central_tab", args = { tab = "drum_sq" }, focused = true }, W)
        T.ok(T.clickAt() == "639,474", "DRUM SQ: " .. T.clickAt())
        T.repaint(T.drums({ central = "drum_sq", sq = { chain = true, selected = "a" } }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.central_panel_tab == "drum_sq" and o.drum_sq_pattern_chain == true and T.lastPost().id == "central-tab-drum_sq",
          tostring(o.central_panel_tab))
        -- An oscillator tab selected: read all the same.
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1, central = "drum_sq" })
        T.tripFrom({ action = "select_central_tab", args = { tab = "editor" }, focused = true }, W)
        T.ok(T.clickAt() == "517,474", "EDITOR: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1, central = "editor" })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.central_panel_tab == "editor", "EDITOR read with OSC 1 selected")
        T.event()
        T.tripFrom({ action = "select_central_tab", args = { tab = "arp" }, focused = true }, W)
        T.ok(T.clickAt() == "578,474", "ARP: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1, central = "arp" })
        T.settle()
        T.ok(T.count("[avenger] [act] 'Choosing a central tab': changed — ARP selected") == 1 and T.lastPost("error").text == "",
          T.dump())
        T.event()
        T.tripFrom({ action = "select_central_tab", args = { tab = "arp" }, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "Choosing a central tab: unchanged, ARP is selected", T.lastPost("error").text)
        -- No tab darker than the others.
        T.event()
        T.tripFrom({ action = "select_central_tab", args = { tab = "step_sq" }, focused = true }, W)
        T.ok(T.clickAt() == "700,474", "STEP SQ: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1 })
        T.settle()
        T.ok(T.lastPost("error").text == "Choosing a central tab: the central tab cannot be read now", T.lastPost("error").text)
        T.event()
        local clicks, from = #S.clicks, #S.pageOrder
        T.pageMsg({ action = "select_central_tab", args = { tab = "fx" }, focused = true })
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Choosing a central tab: FX is not reachable from this page yet",
          T.lastPost("error").text)
        T.ok(not string.find(T.pageSince(from), "focus", 1, true), "no trip")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The page key, the ring, Windows, and the cost.
// ---------------------------------------------------------------------------------------------

/// The page key on the Drums tab reads the table off its one picture: every slot's buttons, the slot
/// selected, and the twelve names in its one text read. On an oscillator tab no slot is read and none
/// of the table's keys is posted; and where the dots gave no count — the strip's count from its text
/// — the names are not asked, and the log says why.
#[test]
fn the_page_key_reads_the_drumkit_table_only_on_the_drums_tab() {
    run_mac(&scenario(r##"
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.paintAvenger(98, 84, 1.6, T.drums({ slots = { selected = 4, mute = { [2] = true }, solo = { [5] = true },
          lock = { [1] = true }, loop = { [12] = true } } }))
        T.words(T.slotWords("KIT", { ["slot 4"] = "HH Closed" }))
        local reads = #S.recognized
        T.openPage()
        T.event()
        local o = T.lastPost("state").state.osc_panel
        T.ok(T.slotReads(reads) == 12, "the names in the one text read: " .. T.slotReads(reads))
        T.ok(o.drumkit_part_selected == "4" and o.drumkit_part_labels[4] == "HH Closed" and o.drumkit_part_labels[1] == "KIT 1"
          and o.drumkit_mute_states[2] == true and o.drumkit_mute_states[1] == false and o.drumkit_solo_states[5] == true
          and o.drumkit_lock_states[1] == true and o.drumkit_loop_states[12] == true, "the table at the key")
        T.ok(T.count("[avenger] [read] drums: slot 1 Mute ") == 1 and T.count("[avenger] [read] drums: the slots' names 1 'KIT 1'") == 1,
          T.dump())
        -- An oscillator tab.
        local back = function()
          S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
          T.pageKey()
          T.event()
        end
        back()
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1, slots = { selected = 4, mute = { [2] = true } } })
        reads = #S.recognized
        T.openPage()
        T.event()
        o = T.lastPost("state").state.osc_panel
        T.ok(T.slotReads(reads) == 0, "no slot read on an oscillator tab")
        T.ok(o.drumkit_part_selected == nil and o.drumkit_part_labels == nil and o.drumkit_mute_states == nil
          and o.drumkit_loop_states == nil, "none of the table's keys")
        -- No dots: the count from the strip's text.
        back()
        T.paintAvenger(98, 84, 1.6, { selected = "drums", mode = "drum", slots = { selected = 2 } })
        T.words(T.slotWords("KIT", { strip = "OSC 1 OSC 2" }))
        reads = #S.recognized
        T.openPage()
        T.event()
        o = T.lastPost("state").state.osc_panel
        T.ok(T.slotReads(reads) == 0 and o.selected_type == "drums" and o.drumkit_part_selected == "2"
          and o.drumkit_part_labels == nil, "no names asked: " .. T.slotReads(reads))
        T.ok(T.count("drums: the slots' names are not read — the strip's count came from its text") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The ring's Drums group: hidden while the strip reads an oscillator tab; shown once a read of it in
/// this stay reads the Drums tab. Drum pattern's Return clicks the switch and says it read after;
/// Drum mode steps to SEQUENCE; Drum slot steps from the slot read as selected to the next; Slot mute
/// clicks the selected slot's Mute; Central tab steps from EDITOR to ARP. One action at a time.
#[test]
fn the_rings_drum_group_shows_on_the_drums_tab_and_acts_as_the_page() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1 })
        T.words({})
        local add = T.indexOf(header, "Add oscillator")
        header.focus = add - 2
        T.tab()
        T.tab()
        T.tab()
        T.ok(T.starts(T.lastSaid(), "Level"), "the group hidden on OSC 1: " .. T.said())
        -- The Drums tab, read by the Oscillator stepper's step.
        T.stepAt(header, T.indexOf(header, "Oscillator"), 1)
        T.stepAt(header, T.indexOf(header, "Oscillator"), 1)
        T.repaint(T.drums({ slots = { selected = 3 } }))
        T.tick(4)
        header.focus = add
        T.tab()
        T.ok(T.lastSaid() == "Drum pattern, off, button", "the group shown: " .. T.said())
        T.pressAt(header, T.indexOf(header, "Drum pattern"))
        T.ok(T.clickAt() == "994,183", "the pattern switch: " .. T.clickAt())
        local clicks = #S.clicks
        T.pressAt(header, T.indexOf(header, "Drum lock"))
        T.ok(#S.clicks == clicks, "one action at a time")
        T.repaint(T.drums({ pattern = true, slots = { selected = 3 } }))
        T.tick(3)
        T.ok(T.lastSaid() == "Pattern ON", T.said())
        T.stepAt(header, T.indexOf(header, "Drum mode"), 1)
        T.ok(T.clickAt() == "1079,300", "SEQUENCE: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true, mode = "sq", slots = { selected = 3 } }))
        T.tick(6)
        T.ok(string.find(T.lastSaid() or "", "SEQUENCE", 1, true) ~= nil, T.said())
        -- (282, 95.5): 98 + 451.2 -> 549, 84 + 152.8 -> 237.
        T.stepAt(header, T.indexOf(header, "Drum slot"), 1)
        T.ok(T.clickAt() == "549,237", "slot 4: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true, mode = "sq", slots = { selected = 4 } }))
        T.tick(6)
        T.ok(string.find(T.lastSaid() or "", "Closed Hi-Hat 2, 4 of 12", 1, true) ~= nil, T.said())
        T.pressAt(header, T.indexOf(header, "Slot mute"))
        T.ok(T.clickAt() == "784,237", "slot 4's Mute: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true, mode = "sq", slots = { selected = 4, mute = { [4] = true } } }))
        T.tick(3)
        T.ok(T.lastSaid() == "Closed Hi-Hat 2 Mute ON", T.said())
        T.stepAt(header, T.indexOf(header, "Central tab"), 1)
        T.ok(T.clickAt() == "578,474", "ARP: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true, mode = "sq", central = "arp", slots = { selected = 4, mute = { [4] = true } } }))
        T.tick(6)
        T.ok(string.find(T.lastSaid() or "", "ARP", 1, true) ~= nil, T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// On Windows, Avenger's own child window at (248, 60) on a display at 150 %: the pattern switch at
/// (560, 62) and slot 3's Mute at (429, 80) through the factor 2.4.
#[test]
fn on_windows_at_150_percent_a_slot_is_clicked_through_the_factor() {
    run(&scenario(r##"
        local j = T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 2.4, T.drums())
        T.words({})
        T.openPage()
        T.event()
        local function trip(m)
          S.focusAtOnce = false
          S.focusLands = function() S.front, S.chain = T.WFX, { j, T.WFX } end
          T.pageMsg(m)
          T.land()
          T.runDue()
        end
        trip({ action = "toggle_drum_pattern", args = {}, focused = true })
        -- 248 + 2.4 * 560 = 1592, 60 + 2.4 * 62 = 208.8 -> 209.
        T.ok(T.clickAt() == "1592,209", "the pattern switch: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true }))
        T.tick(3)
        T.ok(T.lastPost("state").state.osc_panel.drum_pattern == true, "read ON")
        T.event()
        trip({ action = "toggle_drumkit_button", args = { index = 3, button = "mute" }, focused = true })
        -- 248 + 2.4 * 429 = 1277.6 -> 1278, 60 + 2.4 * 80 = 252.
        T.ok(T.clickAt() == "1278,252", "slot 3's Mute: " .. T.clickAt())
        T.repaint(T.drums({ pattern = true, slots = { mute = { [3] = true } } }))
        T.tick(3)
        T.ok(T.lastPost("state").state.osc_panel.drumkit_mute_states[3] == true, "read ON")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The turns of Luau's loops and calls the drums part's read spends at 200 % on a Windows display at
/// 150 % — a factor of 6, the largest Avenger there is: the table's 48 boxes, its selection and the
/// strip, before and after a slot's Mute — each fewer than the pixels of the table's boxes, so a loop
/// over them in Luau would pass the bound.
#[test]
fn the_drum_reads_loop_over_no_pixels_in_luau() {
    run(&scenario(r##"
        S.screenSize = { w = 6000, h = 4000 }
        local j = T.onWindows(200, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 6, 200))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 6, T.drums({ slots = { selected = 3, mute = { [1] = true } } }))
        T.words({})
        T.openPage()
        T.event()
        local Open = S.included["modules/vps-avenger/src/read/open.luau"][1]
        local part, spent = Open.part, {}
        Open.part = function(...)
          local t0 = T.interrupts()
          local out = table.pack(part(...))
          spent[#spent + 1] = T.interrupts() - t0
          return table.unpack(out, 1, out.n)
        end
        S.focusAtOnce = false
        S.focusLands = function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageMsg({ action = "toggle_drumkit_button", args = { index = 3, button = "mute" }, focused = true })
        T.land()
        T.runDue()
        T.repaint(T.drums({ slots = { selected = 3, mute = { [1] = true, [3] = true } } }))
        T.tick(3)
        Open.part = part
        T.ok(#spent == 2 and T.lastPost("state").state.osc_panel.drumkit_mute_states[3] == true, "two reads: " .. #spent)
        -- The table's 48 boxes at a factor of 6 are 78 by 48 pixels each, 179,712 in all.
        for _, n in ipairs(spent) do
          T.ok(n < 60000, string.format("a part read spent %d turns: %s", n, table.concat(spent, ", ")))
        end
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}
