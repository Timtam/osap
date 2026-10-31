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
//! others shows so, one that does not shows that) and the slot selected by its background — one row
//! that stands out, or "not read". No slot's name is read: the twelve slots are Avenger's fixed roles
//! (the maintainer's decision of 2026-10-31), and the drumkit's and a slot's Back and Next send one
//! click, said as sent, and read the switches after it. A slot's menu is right-clicked, its item
//! chosen in Avenger's painted popup, and Copy Sample to's target chosen in the submenu that item opens,
//! followed by the drawn test and counted from the submenu's own top, which opens upward low in
//! Avenger as JUCE opens one (a Mac finding, unmeasured on Avenger's Windows toolkit, KalliGui); no
//! submenu, or one too short for the target, is said, and the menu left open closed with
//! Escape; Save Drumkit as waits for its file dialog. Browse hands the keyboard over and decides by
//! the change wait on the slot's row right of its name after Return; a knob is pressed, held and moved
//! through the keys; its reset says what the change wait saw. The central tab is read after its click
//! with any tab selected. The page key reads the table only on the Drums tab. The ring's group shows
//! on the Drums tab, and stays while a later read decides no tab. No picture is held at the end of
//! any of it.

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

/// A read after Next that does not read the Drums tab selected — the second Mac test's log had it
/// after Next drumkit, taken then for a read while Avenger redraws and very probably the level meter
/// (read/strip.luau, THE SELECTED TAB) — is no answer (src/act.luau, `holds`): looked on from, and the
/// click said as sent once a read holds; one that never holds is said as sent all the same at the
/// bound, the switches not read now — never an error, and never "the Drums tab is not selected" —
/// the state left as read before the click (mt-verified, finding 5).
#[test]
fn a_read_taken_while_avenger_redraws_is_looked_on_from() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.repaint(T.drums({ selected = "osc", index = 1 }))
        T.tick(2)
        T.ok(T.count("what it shows does not hold") >= 1 and T.count("'The next drumkit': changed") == 0, T.dump())
        T.repaint(T.drums({ pattern = true }))
        T.settle()
        T.ok(T.count("[avenger] [act] 'The next drumkit': changed — Next drumkit sent") == 1
          and T.lastPost("error").text == "", T.lastPost("error").text)
        T.ok(T.lastPost("state").state.osc_panel.selected_type == "drums"
          and T.lastPost("state").state.osc_panel.drum_pattern == true, "the Drums tab, read after")
        T.event()
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.repaint(T.drums({ selected = "osc", index = 1 }))
        T.tick(16)
        T.ok(T.lastPost("error").text == "The next drumkit: sent; the drumkit's switches were not read now",
          T.lastPost("error").text)
        T.ok(T.count("[avenger] [act] 'The next drumkit': unheld — sent; the drumkit's switches were not read now") == 1,
          T.dump())
        T.ok(T.lastPost("state").state.osc_panel.selected_type == "drums", "the read before kept")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The drumkit's Next at (623, 224.5) — (1095, 443) — and Back at (616, 224.5) — (1084, 443): ONE
/// click each, said as sent, with no text read before or after it and no name compared — the slots'
/// names are Avenger's fixed roles, and comparing them said "nothing VPS Avenger shows changed" while
/// the pattern switch had (the developer's Mac test of 2026-10-31, mt-verified finding 5). The change
/// wait leaves the name column out, whose rows flash while a sequence plays; the switches read after
/// it are posted. Nothing changed at the bound is said as sent too. In SEQUENCE mode without the lock
/// the sequence is what was stepped; with the lock, the drumkit.
#[test]
fn back_and_next_send_one_click_and_say_it_sent() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        local asked = T.recordSnapshots()
        local reads, clicks = #S.recognized, #S.clicks
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "1095,443", "Next, once: " .. T.clickAt())
        T.repaint(T.drums({ slots = { mute = { [3] = true } } }))
        T.settle()
        T.ok(#S.recognized == reads, "no text read: " .. (#S.recognized - reads))
        T.ok(#S.clicks == clicks + 1, "one click in all")
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_mute_states[3] == true and o.drumkit_part_labels == nil, "the switches read after it, posted")
        T.ok(T.count("[avenger] [act] 'The next drumkit': changed — Next drumkit sent") == 1
          and T.lastPost("error").text == "", T.lastPost("error").text)
        -- The name column, (271, …)-(318, …) — 531.6 to 606.8 on screen — out of the change wait's watch.
        local waits = 0
        for _, a in ipairs(asked) do
          if a.change then
            waits += 1
            for _, w in ipairs(a.change.watch or {}) do
              T.ok(w[1] >= 607 or w[3] <= 531, "a watched box clear of the names: " .. T.region(w))
            end
          end
        end
        T.ok(waits == 1, "one change wait: " .. waits)
        -- Back, nothing changed: said as sent at the bound.
        T.event()
        T.tripFrom({ action = "previous_drumkit", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1084,443", "Back: " .. T.clickAt())
        T.tick(16)
        T.ok(T.lastPost("error").text == "" and T.count("[avenger] [act] 'The previous drumkit': unchanged — Previous "
          .. "drumkit sent") == 1, T.dump())
        -- SEQUENCE mode, no lock: the sequence stepped.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.drums({ mode = "sq" }))
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.tick(16)
        T.ok(T.count("— Next sequence sent") == 1, T.dump())
        -- SEQUENCE with the lock: the drumkit stepped.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.drums({ mode = "sq", lock = true }))
        T.tripFrom({ action = "next_drumkit", args = {}, focused = true }, W)
        T.tick(16)
        T.ok(T.count("'The next drumkit': unchanged — Next drumkit sent") == 1, T.dump())
        T.ok(#S.recognized == reads, "still no text read")
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
/// (727, 368), the tool's (393, 176.5) within a point — chosen in it; both gone, the target's row
/// redrawn said. A submenu that never comes is said, and the popup left open closed with Escape
/// while it still counts open.
#[test]
fn copy_sample_to_opens_its_submenu_and_the_target_is_chosen_in_it() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
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
        T.repaint(T.drums())
        T.settle(6)
        T.ok(T.lastPost().id == "drumkit-menu-2", tostring(T.lastPost().id))
        -- No slot's name read: what the change wait on the target's row saw is said.
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Copy Sample to chosen; VPS Avenger redrew Percussion "
          .. "2's row") == 1, T.dump())
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
          .. "left open (no_submenu) — pressing Escape to close it") == 1, T.dump())
        T.repaint(T.drums())
        T.settle(4)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Slot 10's Swap Slot with, its item at (323, 300.5) — (615, 565) — in the lower half of Avenger:
/// the submenu opens UPWARD, its bottom at the item's, as JUCE opens one (on a Mac; unmeasured on
/// KalliGui), and the target is counted from the
/// submenu's own top — slot 1 on its first row and slot 12 on its last, where the tool's point for
/// slot 1, the item's own row, lies on slot 12's. A submenu too short to hold the target is none to
/// choose in: "no submenu it can be chosen in was seen", never "did not open", and the popup left
/// open closed with Escape.
#[test]
fn a_submenu_opened_upward_is_counted_from_its_own_top() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
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
        T.repaint(T.drums())
        T.settle(6)
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Swap Slot with chosen; VPS Avenger redrew both rows")
          == 1, T.dump())
        c = swap(12, { 382, 168, 460, 306 })
        T.ok(on(c, 12), "slot 12 on the last row: " .. T.clickAt())
        T.repaint(T.drums())
        T.settle(6)
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Swap Slot with chosen; VPS Avenger redrew both rows")
          == 2, T.dump())
        -- Four rows: too short for slot 12.
        local sent = #S.sent
        T.ok(swap(12, { 382, 168, 460, 214 }) == nil, "nothing chosen in it")
        T.ok(T.count("which its menuItem function says is not its menu (it is ") == 1, T.dump())
        T.run(8400)
        T.ok(T.lastPost("error").text == "A drum slot's menu: no submenu it can be chosen in was seen; nothing was chosen in it",
          T.lastPost("error").text)
        T.ok(#S.sent == sent + 1 and S.sent[#S.sent] == "Escape" and T.count("[avenger] 'A drum slot's menu': its popup was "
          .. "left open (unfit_submenu) — pressing Escape to close it") == 1, T.dump())
        T.repaint(T.drums())
        T.settle(4)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Swap Slot with and Remove Sample: what the change wait on their rows saw, said — no slot's name is
/// read. Copy Slot to: the target's switches read after it, a Mute lit there a change. Save Drumkit
/// as: its file dialog, a window of the DAW's, waited for and Avenger in front again — "chosen"; with
/// none within the bound, "no file dialog was seen".
#[test]
fn swap_and_remove_are_read_back_and_save_waits_for_its_dialog() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
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
          T.repaint(T.drums())
          T.settle(6)
        end)
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Swap Slot with chosen; VPS Avenger redrew both rows")
          == 1, T.dump())
        -- Remove Sample on slot 2: (323, 64.5 + 126) -> (615, 389).
        choose({ index = 2, item = "remove_sample" }, function()
          T.ok(T.clickAt() == "615,389", "Remove Sample's row: " .. T.clickAt())
          T.repaint(T.drums())
          T.settle(6)
        end)
        T.ok(T.lastPost("state").state.osc_panel.drumkit_part_labels == nil
          and T.count("changed — Remove Sample chosen; VPS Avenger redrew Open Hi-Hat's row") == 1, T.dump())
        -- Copy Slot to slot 5: its name as it was, its Mute lit after — the slot's settings copied,
        -- which its LEDs show.
        choose({ index = 2, item = "copy_slot_to", target_index = 5 }, function()
          local clicks = #S.clicks
          T.paintAvenger(98, 84, 1.6, T.drums({ popup = { 282, 66, 380, 200 }, submenu = { 382, 155, 460, 300 } }))
          T.ok(T.untilClicks(clicks + 1), "the submenu's item: " .. T.dump())
          T.repaint(T.drums({ slots = { mute = { [5] = true } } }))
          T.settle(6)
        end)
        T.ok(T.count("[avenger] [act] 'A drum slot's menu': changed — Copy Slot to chosen; VPS Avenger redrew Percussion 2's "
          .. "row") == 1, T.dump())
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

/// Browse on slot 3: the strip read, the slot's row right of its name pictured and held — its
/// sample's box, its switches and its knobs, (318.5, 72.25)-(526, 87.75), (608, 200)-(940, 224) at
/// 1.6 — the keyboard handed over with Return alone, Browse clicked at (345, 80) — (650, 212) — and the
/// tool's sentence #13 behind the screen reader. Return through: one change wait on the row from the
/// held picture, the table read, what the wait saw said — no name read (the slots' names are fixed
/// roles: the maintainer's decision of 2026-10-31) — the page back on Browse. The page key during
/// it: the give-up.
#[test]
fn browse_hands_the_keyboard_over_and_reads_the_slots_row_after_return() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        local asked = T.recordSnapshots()
        local reads = #S.recognized
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "browse" }, focused = true }, W)
        T.ok(T.clickAt() == "650,212", "Browse: " .. T.clickAt())
        T.ok(S.holding["Return"] and not S.holding["Escape"] and T.open() == true, "Return alone, let through")
        T.ok(T.lastSaid() == "Use the Avenger browser. To return to Avenger Control, press Return."
          and S.speech[#S.speech].interrupt == false, tostring(T.lastSaid()))
        T.ok(T.held() == 1 and T.liveRegions() == "608,200,940,224", "the row's picture held: " .. T.liveRegions())
        T.repaint(T.drums({ slots = { mute = { [3] = true } } }))
        T.ok(T.typed("Return") == "passed", "Return through to Avenger")
        T.tick()
        T.runDue()
        T.settle()
        local waits = {}
        for _, a in ipairs(asked) do if a.change then waits[#waits + 1] = a end end
        T.ok(#waits == 1 and T.region(waits[1].region) == "608,200,940,224" and waits[1].change.settle == 50,
          "one change wait on the row: " .. #waits)
        T.ok(#S.recognized == reads, "no text read: " .. (#S.recognized - reads))
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_mute_states[3] == true and T.lastPost().id == "drumkit-browse-3" and S.front == T.PAGE_WIN
          and T.lastPost("error").text == "", tostring(T.lastPost().id))
        T.ok(T.count("[avenger] [browse] 'slot 3': after Return — the slot's row changed") == 1, T.dump())
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held: " .. T.held())
        -- The page key while the user browses.
        T.event()
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "browse" }, focused = true }, W)
        T.ok(S.holding["Return"], "handed over")
        T.pageKey()
        T.settle(2)
        T.ok(T.starts(T.lastPost("error").text, "A drum slot's Browse, Back, Next or Edit Sample: stopped by the page key"),
          tostring(T.lastPost("error").text))
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

/// Browse decides by the change wait on the slot's row right of its name, from the picture held
/// before the click: a sample loaded redraws the row — "VPS Avenger redrew its row" — and nothing
/// redrawn is "unchanged" at the bound. The name column is out of the watch: the selection repainted
/// over the name's box alone — slot 3 selected by the click — is no change. Until 0.16 Browse compared
/// the slot's name read before and after, a fixed role that no sample changes.
#[test]
fn browse_decides_by_the_change_wait_on_the_slots_row() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums({ slots = { selected = 1 } }))
        S.honourMinPixels = true
        local function browse()
          T.event()
          T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "browse" }, focused = true }, W)
          T.ok(S.holding["Return"], "handed over")
        end
        local function back()
          T.ok(T.typed("Return") == "passed", "Return through to Avenger")
          T.tick()
          T.runDue()
          T.tick(16)
        end
        -- The selection repainted over the name's box alone: no change at the row.
        browse()
        T.repaint(T.drums({ slots = { selected = 3 } }))
        back()
        T.ok(T.lastPost("error").text == "A drum slot's Browse, Back, Next or Edit Sample: unchanged: VPS Avenger drew no "
          .. "change at Ride's row", T.lastPost("error").text)
        -- The row redrawn: its Loop lit.
        browse()
        T.repaint(T.drums({ slots = { selected = 3, loop = { [3] = true } } }))
        back()
        T.ok(T.lastPost("error").text == "" and T.lastPost().id == "drumkit-browse-3"
          and T.lastPost("state").state.osc_panel.drumkit_loop_states[3] == true, T.lastPost("error").text)
        T.ok(T.count("[act]") >= 0 and T.count("[avenger] [browse] 'slot 3': after Return — the slot's row changed") == 1, T.dump())
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held: " .. T.held())
    "##));
}

/// A slot's Back at (389, 80) and Next at (401, 80) — (720, 212), (740, 212): ONE click each, said as
/// sent by the slot's role, with no text read — the slot's name is a fixed role, and comparing it said
/// "unchanged, Bassdrum reads BD" of every step (mt-verified, finding 5) — the switches read after it;
/// Edit Sample at (370, 80) — (690, 212) — and the selection and the central tab read after it.
#[test]
fn a_slots_back_next_and_edit_sample_are_read_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums({ slots = { selected = 1 } }))
        local reads, clicks = #S.recognized, #S.clicks
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "back" }, focused = true }, W)
        T.ok(T.clickAt() == "720,212", "Back: " .. T.clickAt())
        T.repaint(T.drums({ slots = { selected = 1, loop = { [3] = true } } }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drumkit_loop_states[3] == true and T.lastPost().id == "drumkit-back-3"
          and T.count("changed — Ride: previous sample sent") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "next" }, focused = true }, W)
        T.ok(T.clickAt() == "740,212", "Next: " .. T.clickAt())
        T.tick(16)
        T.ok(T.lastPost().id == "drumkit-next-3" and T.lastPost("error").text == ""
          and T.count("unchanged — Ride: next sample sent") == 1, T.dump())
        T.ok(#S.clicks == clicks + 2 and #S.recognized == reads, "one click each, no text read")
        T.event()
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "edit_sample" }, focused = true }, W)
        T.ok(T.clickAt() == "690,212", "Edit Sample: " .. T.clickAt())
        T.repaint(T.drums({ slots = { selected = 3 }, central = "editor" }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_part_selected == "3" and o.central_panel_tab == "editor" and T.lastPost().id == "drumkit-edit-sample-3",
          tostring(o.drumkit_part_selected) .. " " .. tostring(T.lastPost().id))
        T.ok(T.count("changed — Ride selected; EDITOR shown") == 1, T.dump())
        local clicks2 = #S.clicks
        T.event()
        T.pageMsg({ action = "drumkit_column", args = { index = 3, column = "routing" }, focused = true })
        T.ok(#S.clicks == clicks2 and T.lastPost("error").text
          == "A drum slot's Browse, Back, Next or Edit Sample: routing is no column of a drum slot's", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A slot's Volume: pressed at (458, 80) — (831, 212) — with the left button and held, the tool's
/// sentence #9 with the slot's role in the kit, whatever words a read would find there (no slot's name
/// is read); one PageUp is one move of one point times the factor; Return lets go once and reads
/// nothing, the page back on the Volume button. Pan and Pitch at their points, the left button too.
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
        T.ok(T.lastSaid() == "Please press Page Up or Down to set volume for Ride. To Confirm, press Return."
          and S.speech[#S.speech].interrupt == false, tostring(T.lastSaid()))
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

/// A slot's Browse with Avenger's window leaving the front between the act's check and the
/// hand-over (while the slot's row is pictured to be held): handOver answers `not_in_front`, having
/// given the trip up itself — Browse is not clicked, the picture is let go, and nothing more is ended
/// on a trip already over.
#[test]
fn a_browse_refused_for_the_front_clicks_nothing_and_ends_nothing_more() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        local snapshot = rawget(T.host.screen, "snapshot")
        rawset(T.host.screen, "snapshot", function(opts, cb)
          local r = opts and opts.region
          if type(r) == "table" and T.region(r) == "608,200,940,224" and not (opts and opts.change) then
            local s, why = snapshot(opts, cb)
            S.listed, S.front, S.chain = { W, T.OTHER }, T.OTHER, { T.OTHER }
            return s, why
          end
          return snapshot(opts, cb)
        end)
        local clicks = #S.clicks
        T.tripFrom({ action = "drumkit_column", args = { index = 3, column = "browse" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and not S.holding["Return"], "Browse not clicked, nothing handed over")
        T.ok(T.held() == 0, "the picture let go: " .. T.held())
        T.ok(T.count("the keyboard was not handed over") == 0 and T.count("done after the trip ended") == 0, T.dump())
        T.ok(string.find(T.lastPost("error").text, "another window came to the front; nothing more was done", 1, true) ~= nil,
          tostring(T.lastPost("error").text))
    "##));
}

/// A double-click the host does not make (O.doubleClick's answers, D41): one the host refuses at
/// the call clicks nothing, watches for no change and ends the trip with the page told; one whose
/// second half the host did not send — the callback's `false, err` — ends it the same way once the
/// first click was made, and nothing is watched either. The harness's `S.clickAnswers` takes the
/// call's answer first and the callback's second.
#[test]
fn a_reset_whose_click_is_not_made_watches_for_nothing_and_says_so() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums())
        local covered = T.E("covered", "something else is drawn over the point")
        local said = "Resetting a drum slot's control was not clicked: something else is drawn over the point"
        local n, asked = #S.clicks, S.changeWaitsAsked
        S.clickAnswers = { covered }
        T.tripFrom({ action = "reset_drumkit_control", args = { index = 3, control = "volume" }, focused = true }, W)
        T.tick(2)
        T.ok(#S.clicks == n and S.changeWaitsAsked == asked, "nothing clicked, nothing watched: " .. #S.clicks)
        T.ok(T.lastPost("error").text == said, T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
        T.event()
        S.clickAnswers = { true, covered }
        T.tripFrom({ action = "reset_drumkit_control", args = { index = 3, control = "volume" }, focused = true }, W)
        T.ok(#S.clicks == n + 1 and T.clickAt() == "831,212", "the first click: " .. T.clickAt())
        T.tick(2)
        T.ok(#S.clicks == n + 1 and S.changeWaitsAsked == asked, "the second not made, nothing watched")
        T.ok(T.count("[avenger] 'Resetting a drum slot's control': the double-click was not made whole — "
          .. "something else is drawn over the point") == 1, T.dump())
        T.ok(T.lastPost("error").text == said, T.lastPost("error").text)
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

/// The page key on the Drums tab reads the table off its one picture: every slot's buttons and the
/// slot selected — no slot's name. On an oscillator tab no slot is read and none of the table's keys is
/// posted; and where the dots gave no count — the strip's count from its text — the table is read once
/// the strip is decided.
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
        -- No slot's name read, whatever words lie there: the twelve are fixed roles (read/drums.luau, THE
        -- NAMES; the maintainer's decision of 2026-10-31).
        T.ok(T.slotReads(reads) == 0, "no slot's name in the text read: " .. T.slotReads(reads))
        T.ok(o.drumkit_part_selected == "4" and o.drumkit_part_labels == nil
          and o.drumkit_mute_states[2] == true and o.drumkit_mute_states[1] == false and o.drumkit_solo_states[5] == true
          and o.drumkit_lock_states[1] == true and o.drumkit_loop_states[12] == true, "the table at the key")
        T.ok(T.count("[avenger] [read] drums: slot 1 Mute ") == 1 and T.count("the slots' names") == 0, T.dump())
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
        -- OSC 1's dot and one that fits no count's step: the count from the strip's text, and the table
        -- read after the strip is decided.
        back()
        local GREEN = { 100, 220, 180 }
        T.paintAvenger(98, 84, 1.6, { selected = "drums", mode = "drum", slots = { selected = 2 },
          dots = { { 246, 3, GREEN }, { 266, 3, GREEN } } })
        T.words(T.slotWords("KIT", { strip = "OSC 1 OSC 2" }))
        reads = #S.recognized
        T.openPage()
        T.event()
        o = T.lastPost("state").state.osc_panel
        T.ok(T.slotReads(reads) == 0 and o.selected_type == "drums" and o.drumkit_part_selected == "2" and o.count == 2
          and o.drumkit_part_labels == nil, "no names asked: " .. T.slotReads(reads))
        T.ok(T.count("[avenger] [read] strip: the tabs read whole name OSC 1, 2 — 2 oscillators") == 1, T.dump())
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

// ---------------------------------------------------------------------------------------------
// The developer's Mac test of 2026-10-31: the slots' fixed names, and the Drums tab's controls kept
// while the selection is not read (mt-verified, findings 4C and 6; the maintainer's decisions B4 and
// B5 of that day).
// ---------------------------------------------------------------------------------------------

/// The twelve slots are Avenger's fixed roles — the column draws CRSH … BD whatever the kit, in all
/// 144 reads of that test — and no slot's name is read: not at the page key, not after the Drums
/// tab's click, not around a slot's click. A slot is said by its role ("Open Hi-Hat selected"), and
/// the page shows the twelve roles, never "name not read" beside them.
#[test]
fn drum_reads_ask_no_slot_names_and_the_page_shows_the_fixed_roles() {
    let js = include_str!("../../../modules/vps-avenger/ui/panel.js");
    assert!(!js.contains("drumkitNamesRead") && !js.contains("drumkit_part_labels"), "panel.js reads no slot's name");
    assert!(js.contains("const drumkitCurrentLabels = DRUMKIT_ROLES;"), "the roles are the slots' labels");
    assert!(js.contains("const selectedSampleName = selectedSlot != null ? DRUMKIT_ROLES[selectedSlot - 1] : 'selected';"));
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1 }, T.slotWords("CR"))
        local reads = #S.recognized
        T.tripFrom({ action = "select_drums_tab", args = {}, focused = true }, W)
        T.repaint(T.drums({ slots = { selected = 1 } }))
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.selected_type == "drums" and T.slotReads(reads) == 0,
          "the Drums tab read, no slot's name: " .. T.slotReads(reads))
        T.event()
        T.tripFrom({ action = "select_drumkit_part", args = { index = 2 }, focused = true }, W)
        T.repaint(T.drums({ slots = { selected = 2 } }))
        T.settle()
        T.ok(T.count("[avenger] [act] 'Choosing a drum slot': changed — Open Hi-Hat selected") == 1, T.dump())
        T.ok(T.lastPost("state").state.osc_panel.drumkit_part_selected == "2"
          and T.lastPost("state").state.osc_panel.drumkit_part_labels == nil, "the slot selected, no name posted")
        T.ok(T.slotReads(reads) == 0, "still no slot's name read")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The ring's Drums group stays while a read of the strip decides no tab — no tab stands out — and
/// says "selection not read now" in place of its values; a press reads it again (one decided by it
/// is what the control acts on), and nothing is clicked on an undecided read. A read that decides
/// another tab hides it. Until 0.16 one undecided read before Slot mute hid the whole group
/// (the developer's Mac test of 2026-10-31: Tab from Slot mute landed on Level).
#[test]
fn an_undecided_read_keeps_the_rings_drums_group_and_reads_again_at_the_next_press() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        T.words({})
        T.paintAvenger(98, 84, 1.6, T.drums({ slots = { selected = 3 } }))
        local osc = T.indexOf(header, "Oscillator")
        header.focus = osc - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, Drums, slider", T.said())
        -- No tab stands out now.
        local FLAT = T.drums({ selected = false, strips = { 52, 52, drums = 52, trig = 52 }, slots = { selected = 3 } })
        T.paintAvenger(98, 84, 1.6, FLAT)
        local clicks, snaps = #S.clicks, S.snaps
        local mute = T.indexOf(header, "Slot mute")
        T.pressAt(header, mute)
        T.ok(#S.clicks == clicks and S.snaps > snaps and T.lastSaid() == "Slot mute: selection not read now", T.said())
        T.tab()
        T.ok(T.lastSaid() == "Slot solo, selection not read now, button", "the group kept: " .. T.said())
        -- Drums read selected again: the press clicks the selected slot's Mute.
        T.paintAvenger(98, 84, 1.6, T.drums({ slots = { selected = 3 } }))
        T.pressAt(header, mute)
        T.ok(#S.clicks == clicks + 1, "Mute clicked: " .. T.clickAt())
        T.repaint(T.drums({ slots = { selected = 3, mute = { [3] = true } } }))
        T.tick(3)
        T.ok(T.lastSaid() == "Ride Mute ON", T.said())
        -- OSC 1 decided: the group gone.
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1 })
        T.stepAt(header, osc, -1)
        T.repaint({ osc = 2, selected = "osc", index = 2 })
        T.tick(4)
        header.focus = T.indexOf(header, "Add oscillator")
        T.tab()
        T.ok(T.starts(T.lastSaid(), "Level"), "the group hidden: " .. T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The page's Drums section is kept while a read decides no tab, its values not read — every value
/// of the section's left out, the section said "selection not read now" (`drums_kept`) — and a drum
/// action's read before its click that decides none clicks nothing. A read that decides a tab again
/// takes the mark away.
#[test]
fn an_undecided_read_before_a_drum_click_keeps_the_pages_drums_section() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.drums({ slots = { selected = 3, mute = { [3] = true } } }))
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.selected_type == "drums" and o.drums_kept == nil and o.drumkit_mute_states[3] == true, "Drums read")
        T.paintAvenger(98, 84, 1.6, T.drums({ selected = false, strips = { 52, 52, drums = 52, trig = 52 } }))
        local clicks = #S.clicks
        T.tripFrom({ action = "toggle_drumkit_button", args = { index = 3, button = "mute" }, focused = true }, W)
        T.settle(2)
        o = T.lastPost("state").state.osc_panel
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "A drum slot's Mute, Solo, Lock or Loop: the selected tab cannot be read now",
          T.lastPost("error").text)
        T.ok(o.selected_type == nil and o.drums_kept == true and o.drumkit_mute_states == nil, T.jsonEncode(o))
        local js = T.repoText("modules/vps-avenger/ui/panel.js")
        T.ok(string.find(js, "else if (drumsKept) label = 'Drums, selection not read now';", 1, true) ~= nil, "panel.js")
        -- Decided again.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.drums({ slots = { selected = 3 } }))
        T.tripFrom({ action = "toggle_drumkit_button", args = { index = 3, button = "mute" }, focused = true }, W)
        T.ok(#S.clicks == clicks + 1, "clicked")
        T.repaint(T.drums({ slots = { selected = 3, mute = { [3] = true } } }))
        T.settle()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.selected_type == "drums" and o.drums_kept == nil and o.drumkit_mute_states[3] == true, T.jsonEncode(o))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}
