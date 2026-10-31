//! VPS Avenger's popups on a Mac, and the oscillator strip and the macros by clicks (the plan's
//! slice 5, `modules/vps-avenger/src/{act,ring,popups}.luau` and `src/actions/{osc,macros}.luau`),
//! loaded for real against the scripted host of `overlay_avenger_tests.rs`, whose painted picture of
//! Avenger now changes after a click (`T.repaint`) and answers the module's change waits.
//!
//! The popups first (TODO.md, the first Mac test of the page, item 5): in Logic no window of the
//! process comes for the zoom list, and the runtime's drawn test sees it painted inside Avenger —
//! the page's Size, the ring's MENU, the redo list that holds a trip until it has gone.
//!
//! Then every action of the slice from the page, as a trip: the part read off a picture before the
//! click, the click at the table's point through the overlay's frame and factor, the change waited
//! for from that picture, and the part read again; "unchanged" said at the wait's bound; a hover
//! redraw looked past; the right button for a tab's menu and for Pan's reset, its item at the
//! tool's own x; a refusal that clicks nothing, at a limit with the tool's focus; a popup left open
//! closed with Escape; a trip given up before the click and mid-action; and no picture held after
//! an action. And the ring's controls, which run the same specs, read again after a trip of the
//! page and after ▶, one action at a time.

use super::overlay_avenger_tests::{run, run_mac};

/// What every scenario here begins with: Avenger at 80 % in REAPER's floating window on a Mac,
/// painted as `spec` says (overlay_avenger_tests.rs, T.paintAvenger), its words `words`, the page
/// open in front.
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
-- A trip's change wait and what follows it: `n` ticks, the trip's back step among them.
function T.settle(n) T.tick(n or 3) end
-- The ring's control labelled `label`, its index.
function T.indexOf(o, label)
  for i, c in ipairs(o.controls) do if c.label == label then return i end end
  error("no control " .. label)
end
"##;

fn scenario(s: &str) -> String {
    format!("{OPEN}\n{s}")
}

// ---------------------------------------------------------------------------------------------
// Avenger's popups, painted inside it.
// ---------------------------------------------------------------------------------------------

/// The page's Size with the zoom list painted inside Avenger (no window comes): the drawn test,
/// armed on the zoom field's press, sees it alike twice, and the entry for 100 % is chosen by the
/// list's own height, as for a window; the list gone, the zoom field is read again and the page
/// comes back with it.
#[test]
fn a_size_from_the_page_is_chosen_in_a_zoom_list_painted_inside_avenger() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1 })
        T.tripFrom({ action = "set_zoom", args = { percent = 100 }, focused = true }, W)
        T.ok(T.clickAt() == "132,106", "the zoom field: " .. T.clickAt())
        T.ok(T.count("[menu] 'VPS Avenger': drawn: armed on 'Zoom' in 117,113-213,503") == 1, T.dump())
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 14, 18.25, 70, 262 } })
        T.tick(3)
        T.ok(T.count("[menu] 'VPS Avenger': drawn: a popup 90x389 at 120,113") == 1, T.dump())
        T.ok(T.clickAt() == "165,308", "100 %'s entry: " .. T.clickAt())
        T.repaint({ osc = 1, selected = "osc", index = 1 })
        T.tick(3)
        T.ok(T.count("[menu item] 'VPS Avenger': 'Zoom' chosen — clicked 1 time(s) at (165,308); the menu has closed") == 1,
          T.dump())
        T.run(300)
        local r = T.lastRead("vps-avenger header")
        T.ok(r and not r.answered, "the zoom field read again")
        T.answer(r, T.header(98, 84, 2, 100))
        T.run(300)
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.zoom == 100 and T.lastPost("error").text == "",
          tostring(T.lastPost("state").state.zoom) .. " " .. tostring(T.lastPost("error").text))
    "##));
}

/// MENU's Load preset and Save preset as from the ring, like the original (the maintainer's decision
/// of 2026-10-31, D1): each item clicked ONCE at the project's point, 180 ms after MENU — whatever
/// the popup does after it, frozen while the file dialog opens or drawn again with the pointer over
/// its row, it is never clicked again (the second Mac test clicked Load preset four times), and no
/// menu test follows it. Initialize preset, which opens no dialog, is still chosen in the popup seen
/// — found whole from (242, 20), left of 245, the old edge of the area it is looked for in — and
/// followed: a popup frozen as at its click is the click pending (overlay runtime 0.13), not clicked
/// again; drawn again — the pointer over its row — and still open, it is clicked again.
#[test]
fn menus_file_items_are_clicked_once_at_the_projects_points_and_initialize_is_followed() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        local base = { osc = 1, selected = "osc", index = 1 }
        T.paintAvenger(98, 84, 1.6, base)
        -- The clicks after the item's: while the popup stays frozen, and once it is drawn again.
        local function choose(label)
          T.pressAt(header, T.indexOf(header, label))
          T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 242, 20, 322, 70 } })
          T.tick(3)
          local clicks = #S.clicks
          T.tick(6)
          local frozen = #S.clicks - clicks
          local c = S.clicks[#S.clicks]
          T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1,
            popup = { 242, 20, 322, 70, pointer = { (c[1] - 98) / 1.6, (c[2] - 84) / 1.6 } } })
          T.tick(4)
          local again = #S.clicks - clicks - frozen
          T.repaint(base)
          T.tick(4)
          return frozen, again
        end
        -- Load at (257, 32): 98 + 411.2 -> 509, 84 + 51.2 -> 135.
        local frozen, again = choose("Load preset")
        T.ok(frozen == 0 and again == 0 and T.clickAt() == "509,135", "Load once, at its point: " .. T.clickAt())
        T.ok(T.count("drawn: armed on 'Load preset'") == 0, "no menu test: " .. T.dump())
        -- Save as at (257, 55): 84 + 88 -> 172.
        frozen, again = choose("Save preset as")
        T.ok(frozen == 0 and again == 0 and T.clickAt() == "509,172", "Save as once: " .. T.clickAt())
        frozen, again = choose("Initialize preset")
        -- 242 is 485.2 on screen, 485: 241.9 from Avenger's corner.
        T.ok(T.count("[avenger] 'Initialize preset': MENU's popup at (241.9,20.0) 80.0x50.0 from Avenger's corner") == 1,
          T.dump())
        T.ok(frozen == 0 and again == 1, "Initialize pending while frozen, clicked again once drawn again: " .. frozen
          .. " " .. again .. " " .. T.dump())
    "##));
}

/// A16 (the second Mac test's finding 18): the ring's Load preset says the preset loaded. Its item
/// clicked, the runtime's O:afterDialog waits for the dialog — REAPER's open panel coming forward
/// over Avenger — and once Avenger is in front again, the name is watched as the page's Load watches
/// it, read until it differs from the name before and has settled, and said in the page's words,
/// "Load preset: <name>". A name that never settles is said as not read. With no dialog within the
/// runtime's bound, the name all the same, and that none was seen.
#[test]
fn the_rings_load_preset_says_the_preset_loaded_after_its_dialog() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("LD Lead 1")
        local base = { osc = 1, selected = "osc", index = 1 }
        T.paintAvenger(98, 84, 1.6, base)
        local dialog = { id = 951, title = "Open", class = "AXWindow/AXDialog/", app = T.MAC_REAPER,
          bounds = { x = 300, y = 200, w = 700, h = 450 }, client = { x = 300, y = 228, w = 700, h = 422 } }
        -- Load at the project's point, 180 ms after MENU (D1): 98 + 411.2 -> 509, 84 + 51.2 -> 135.
        local function load()
          T.pressAt(header, T.indexOf(header, "Load preset"))
          T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 242, 20, 322, 70 } })
          T.tick(3)
          T.ok(T.clickAt() == "509,135", "Load's point: " .. T.clickAt())
          T.repaint(base)
        end
        -- Back in Avenger, its header read again.
        local function back()
          S.listed = { W }
          T.show(W)
          local r = T.lastRead("vps-avenger header")
          if r and not r.answered then T.answer(r, T.header(98, 84, 1.6, 80)) end
          T.event()
          T.run(400)
          T.ok(header.active, "back in front: " .. T.dump())
        end
        load()
        T.ok(T.count("[menu] 'VPS Avenger': waits for the dialog that comes next") == 1, T.dump())
        S.listed = { W, dialog }
        T.show(dialog)
        T.tick(2)
        T.ok(T.count("[menu] 'VPS Avenger': a dialog came forward — 'Open'") == 1, T.dump())
        T.run(9000)
        local said = #S.speech
        T.nameIs("Resurrection")
        back()
        T.nameReads("Resurrection")
        T.ok(T.lastSaid() == "Load preset: Resurrection", T.said(said + 1))
        T.ok(T.count("[avenger] 'Load preset': the name after the dialog — Load preset: Resurrection") == 1, T.dump())
        -- A name that never settles: not read.
        load()
        S.listed = { W, dialog }
        T.show(dialog)
        T.tick(2)
        back()
        local i = 0
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then
            i += 1
            T.answer(r, T.nameRead("Loading " .. i))
          end
        end
        T.ok(T.lastSaid() == "Load preset: the preset name cannot be read now", T.said())
        -- No dialog within the bound: the name all the same.
        load()
        T.nameIs("Resurrection")
        T.run(8200)
        T.nameReads("Resurrection")
        T.ok(T.lastSaid() == "Load preset: Resurrection — no file dialog was seen", T.said())
        T.ok(T.count("[menu] 'VPS Avenger': no dialog came within 8000 ms") == 1, T.dump())
    "##));
}

/// A Load the user cancelled in its file dialog is said as that (mt-verified, finding 8: "Load
/// preset: VC Vowel Choir 7" was said for a dialog left with Escape): the name read after the dialog
/// is the one read before the press, and has not changed by the watch's bound — "Load preset: <name>,
/// unchanged"; Save as alike. From the page, the note "unchanged, <name>".
#[test]
fn load_preset_cancelled_says_the_name_unchanged() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("Resurrection")
        local base = { osc = 1, selected = "osc", index = 1 }
        T.paintAvenger(98, 84, 1.6, base)
        local dialog = { id = 951, title = "Open", class = "AXWindow/AXDialog/", app = T.MAC_REAPER,
          bounds = { x = 300, y = 200, w = 700, h = 450 }, client = { x = 300, y = 228, w = 700, h = 422 } }
        for _, label in ipairs({ "Load preset", "Save preset as" }) do
          T.pressAt(header, T.indexOf(header, label))
          T.tick(3)
          S.listed = { W, dialog }
          T.show(dialog)
          T.tick(2)
          -- Escape in the dialog: Avenger shows the preset it had.
          S.listed = { W }
          T.show(W)
          local r = T.lastRead("vps-avenger header")
          if r and not r.answered then T.answer(r, T.header(98, 84, 1.6, 80)) end
          T.event()
          T.run(400)
          local said = #S.speech
          T.nameReads("Resurrection")
          T.ok(T.lastSaid() == label .. ": Resurrection, unchanged", T.said(said + 1))
          T.ok(T.count("[avenger] '" .. label .. "': the name after the dialog — " .. label .. ": Resurrection, unchanged")
            == 1, T.dump())
        end
        -- From the page: the note.
        T.pageReady()
        T.nameIs("Resurrection")
        T.openPage()
        T.event()
        T.tripFrom({ action = "pick_load_preset", args = {}, focused = true }, W)
        T.tick(3)
        S.listed = { W, dialog }
        T.show(dialog)
        T.runDue()
        S.listed = { W }
        T.show(W)
        T.runDue()
        T.nameReads("Resurrection")
        local err = T.lastPost("error").text
        T.ok(T.lastPost("state").state.preset == "Resurrection" and err == "Load Preset: unchanged, Resurrection",
          err .. " " .. T.dump())
        T.ok(T.count("[avenger] [page] 'Load Preset': answered — note 'unchanged, Resurrection'") == 1, T.dump())
    "##));
}

/// MENU's Load from the ring, its popup painted below MENU: clicked at the project's point 180 ms
/// after MENU, whatever is painted (the maintainer's decision of 2026-10-31, D1), and never again;
/// and the redo list from the page, which holds the trip for as long as it is seen and lets it go on
/// once it has gone — then the name.
#[test]
fn menu_from_the_ring_and_the_redo_list_from_the_page_in_popups_painted_inside_avenger() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("LD Lead 1")
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1 })
        T.pressAt(header, 4)
        T.ok(T.clickAt() == "517,106" and T.lastSaid() == "Load preset, activated", T.clickAt())
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 250, 20, 330, 70 } })
        local clicks = #S.clicks
        T.tick(3)
        -- The project's (257, 32): 98 + 411.2 -> 509, 84 + 51.2 -> 135; once, no menu test.
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "509,135", "Load's point: " .. T.clickAt())
        T.repaint({ osc = 1, selected = "osc", index = 1 })
        T.tick(3)
        T.ok(#S.clicks == clicks + 1 and T.count("drawn: armed on 'Load preset'") == 0, T.dump())
        -- The redo list from the page.
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        T.tripFrom({ action = "redo_menu", args = {}, focused = true }, W)
        T.ok(S.clicks[#S.clicks].button == "right" and T.clickAt() == "586,106", "UNDO, right: " .. T.clickAt())
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 306, 20, 420, 120 } })
        T.tick(3)
        T.ok(T.count("trip 'Redo': its menu is seen") == 1, T.dump())
        T.run(3000)
        T.ok(S.front ~= T.PAGE_WIN, "the list is the user's for as long as it is up")
        T.repaint({ osc = 1, selected = "osc", index = 1 })
        T.tick(3)
        T.ok(T.count("trip 'Redo': its menu has closed") == 1, T.dump())
        T.runDue()
        T.nameReads("LD Lead 2")
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.preset == "LD Lead 2", "the name after the list")
    "##));
}

// ---------------------------------------------------------------------------------------------
// The oscillator strip from the page.
// ---------------------------------------------------------------------------------------------

/// An oscillator tab: the strip read off a picture first — the count from the dots — then the
/// tab clicked where the count puts it, the change waited for from that picture, and the selection
/// read again: the page back with OSC 2 selected. A tab past the count, or a strip whose dots read
/// nothing, clicks nothing and says why; a tab that does not change is "unchanged" at the wait's
/// bound, two seconds on.
#[test]
fn an_oscillator_tab_is_clicked_where_the_count_puts_it_and_read_back() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 3, selected = "osc", index = 1 })
        local clicks, snaps, mark = #S.clicks, S.snaps, #S.pageOrder
        T.tripFrom({ action = "select_osc_tab", args = { index = 2 }, focused = true }, W)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "721,140" and S.clicks[#S.clicks].button == nil,
          "OSC 2 of 3: " .. T.clickAt())
        T.ok(S.snaps == snaps + 1 and #S.changeWaits == 1, "one picture before the click, and a change wait")
        T.ok(not string.find(T.pageSince(mark), "post", 1, true), "nothing posted before the read after: "
          .. T.pageSince(mark))
        T.repaint({ osc = 3, selected = "osc", index = 2 })
        T.settle()
        local st = T.lastPost("state").state
        T.ok(S.front == T.PAGE_WIN and st.osc_panel.selected_type == "osc" and st.osc_panel.selected_index == 2
          and st.osc_panel.count == 3 and T.lastPost("error").text == "", tostring(st.osc_panel.selected_index))
        T.ok(T.lastPost().kind == "restoreFocus", "the focus back where the user was")
        T.ok(T.count("[avenger] [act] 'Choosing an oscillator tab': changed — OSC 2 selected") == 1, T.dump())
        -- Past the count: nothing clicked.
        T.event()
        clicks = #S.clicks
        T.tripFrom({ action = "select_osc_tab", args = { index = 4 }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text
          == "Choosing an oscillator tab: OSC 4 is not there; VPS Avenger shows 3 oscillators", T.lastPost("error").text)
        -- The same tab again: unchanged at the bound.
        T.event()
        T.tripFrom({ action = "select_osc_tab", args = { index = 2 }, focused = true }, W)
        T.ok(#S.clicks == clicks + 1, "clicked")
        T.tick(10)
        T.ok(T.lastPost("error").text ~= "Choosing an oscillator tab: unchanged, OSC 2 is selected", "not before the bound")
        T.tick(6)
        T.ok(T.lastPost("error").text == "Choosing an oscillator tab: unchanged, OSC 2 is selected"
          and T.lastPost("error").focus == true, T.lastPost("error").text)
        -- No dots, OSC 1's among them: the strip covered — by a menu of Avenger's, as no dot at OSC
        -- 1's place says (read/strip.luau, A COVERED STRIP) — nothing clicked.
        T.event()
        T.paintAvenger(98, 84, 1.6, {})
        clicks = #S.clicks
        T.tripFrom({ action = "select_osc_tab", args = { index = 1 }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Choosing an oscillator tab: the oscillator strip is "
          .. "covered now, perhaps by a menu of VPS Avenger's; nothing was clicked", T.lastPost("error").text)
        -- OSC 1's dot and one that fits no count's step: not counted, and nothing clicked.
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 3, dots = { { 246, 3, { 100, 220, 180 } }, { 266, 3, { 100, 220, 180 } } } })
        T.tripFrom({ action = "select_osc_tab", args = { index = 1 }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text
          == "Choosing an oscillator tab: the oscillator tabs cannot be counted now", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Add OSC: clicked where the tool clicks it, then the count and the names read again — the page
/// lands on the new tab; at eight, nothing is clicked.
#[test]
fn add_osc_reads_the_new_count_and_none_is_added_at_eight() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 3, selected = "osc", index = 1 }, { ["osc 1"] = "Bass" })
        T.tripFrom({ action = "add_osc", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "968,140", "Add OSC: " .. T.clickAt())
        T.words({ ["osc 1"] = "Bass", ["osc 4"] = "OSC 4" })
        T.repaint({ osc = 4, selected = "osc", index = 4 })
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.count == 4 and o.names[1] == "Bass" and o.names[4] == "OSC 4", tostring(o.count))
        T.ok(T.lastPost().kind == "focus" and T.lastPost().id == "osc-tab-4", tostring(T.lastPost().id))
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 8, selected = "osc", index = 1 })
        local clicks = #S.clicks
        T.tripFrom({ action = "add_osc", args = {}, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Add OSC: VPS Avenger has 8 oscillators, the most it takes",
          T.lastPost("error").text)
        -- At the limit the focus goes where the tool puts it, and the sentence is said beside it.
        local posts = T.page().posts
        T.ok(posts[#posts - 1].kind == "focus" and posts[#posts - 1].id == "osc-tab-8" and posts[#posts].focus ~= true,
          tostring(posts[#posts - 1].id))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Mute and Solo: the LED clicked, its share of red read after — never flipped from memory: a
/// click that changes nothing is "unchanged" with the state read; one that lights it reads ON. A
/// hover drawn first (a picture that changed in nothing the read tells) is looked past within the
/// same bound. Drums' and Trig's at the tool's points.
#[test]
fn mute_and_solo_read_their_leds_after_the_click_and_look_past_a_hover() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1 })
        T.tripFrom({ action = "toggle_osc_mute", args = { index = 1 }, focused = true }, W)
        -- OSC 1 of 2 at 244 + 0.5 * 146 = 317, Mute 7 left: 98 + 1.6 * 310 -> 594; y 26 -> 126.
        T.ok(T.clickAt() == "594,126", "Mute: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1 })
        T.tick(2)
        T.ok(T.count("the picture changed and what this action changes did not") == 1, T.dump())
        T.repaint({ osc = 2, selected = "osc", index = 1, mute = { [1] = true } })
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.mute_states[1] == true and T.lastPost("error").text == "", tostring(o.mute_states[1]))
        T.event()
        T.tripFrom({ action = "toggle_osc_solo", args = { index = 2 }, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "An oscillator's Solo: unchanged, OSC 2 Solo OFF", T.lastPost("error").text)
        T.event()
        T.tripFrom({ action = "toggle_drums_mute", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1008,126", "Drums' Mute at (569, 26): " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1, mute = { [1] = true }, drumsMute = true })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drums_mute == true, "Drums' Mute ON")
        -- And OFF again: a switch read OFF is posted OFF, not as not read.
        T.event()
        T.tripFrom({ action = "toggle_drums_mute", args = {}, focused = true }, W)
        T.repaint({ osc = 2, selected = "osc", index = 1, mute = { [1] = true } })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drums_mute == false, "Drums' Mute OFF: "
          .. tostring(T.lastPost("state").state.osc_panel.drums_mute))
        T.event()
        T.tripFrom({ action = "toggle_drums_solo", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1031,126", "Drums' Solo at (583, 26): " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1, mute = { [1] = true }, drumsSolo = true })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.drums_solo == true and T.lastPost("error").text == "", "Drums' Solo ON")
        T.event()
        T.tripFrom({ action = "toggle_trig_solo", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1095,126", "Trig's Solo at (623, 26): " .. T.clickAt())
        T.tick(16)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// An OSC tab's menu: right-clicked at the tab's menu point, its popup painted below the click,
/// Copy chosen at its row and at the tool's own x — noted, so the page offers Paste — with a small
/// window of the DAW's (a tooltip) that newWindow sees first and that is not the menu; Paste,
/// whose names are read before the click and after it: "nothing … changed" where they and all the
/// rest read the same, and chosen where the tab's name changed; and Delete of the third of three,
/// whose read after it lands the page on the menu of the tab that is now the last. Delete with one
/// oscillator is refused without a click. The Drums tab's Initialize the same way, at its tab.
#[test]
fn a_tab_menu_is_opened_with_the_right_button_and_its_item_chosen_in_the_painted_popup() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 3, selected = "osc", index = 1 }, { ["osc 1"] = "Bass", ["osc 2"] = "Lead" })
        T.tripFrom({ action = "osc_menu_item", args = { index = 2, item = "Copy" }, focused = true }, W)
        T.ok(T.clickAt() == "679,140" and S.clicks[#S.clicks].button == "right", "the menu point, right: " .. T.clickAt())
        T.popup(400, 690, 112, 60, 16)
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, popup = { 363, 36, 440, 90 } })
        T.tick(3)
        T.ok(T.count("[menu item] 'VPS Avenger': 'An oscillator's menu' — test 'newWindow' sees a window at 690,112 "
          .. "60x16, which its menuItem function says is not its menu (its rows do not reach the item's row at 46.0); "
          .. "waiting for the next answer") == 1, T.dump())
        T.ok(T.clickAt() == "679,158" and S.clicks[#S.clicks].button == nil, "Copy's row: " .. T.clickAt())
        T.closePopups()
        T.repaint({ osc = 3, selected = "osc", index = 1 })
        T.settle(4)
        T.ok(T.lastPost("state").state.osc_panel.copied == "Lead" and T.lastPost("error").text == "",
          tostring(T.lastPost("state").state.osc_panel.copied))
        -- Paste on OSC 1, nothing changing: said so at the bound.
        T.event()
        T.tripFrom({ action = "osc_menu_item", args = { index = 1, item = "Paste" }, focused = true }, W)
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, popup = { 265, 36, 340, 90 } })
        T.tick(3)
        T.ok(T.clickAt() == "522,175", "Paste's row: " .. T.clickAt())
        T.repaint({ osc = 3, selected = "osc", index = 1 })
        T.tick(20)
        T.ok(T.lastPost("error").text == "An oscillator's menu: Paste chosen; nothing VPS Avenger shows changed",
          T.lastPost("error").text)
        -- Paste again, the tab's name read changed after it.
        T.event()
        T.tripFrom({ action = "osc_menu_item", args = { index = 1, item = "Paste" }, focused = true }, W)
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, popup = { 265, 36, 340, 90 } })
        T.tick(3)
        T.words({ ["osc 1"] = "Lead", ["osc 2"] = "Lead" })
        T.repaint({ osc = 3, selected = "osc", index = 1 })
        T.settle(4)
        T.ok(T.lastPost("error").text == "" and T.lastPost("state").state.osc_panel.names[1] == "Lead",
          tostring(T.lastPost("error").text))
        -- Delete of the third.
        T.event()
        T.tripFrom({ action = "osc_menu_item", args = { index = 3, item = "Delete" }, focused = true }, W)
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, popup = { 461, 36, 540, 90 } })
        T.tick(3)
        T.repaint({ osc = 2, selected = "osc", index = 1 })
        T.settle(4)
        local st = T.lastPost("state").state
        T.ok(st.osc_panel.count == 2 and T.lastPost().kind == "focus" and T.lastPost().id == "osc-menu-2",
          tostring(st.osc_panel.count) .. " " .. tostring(T.lastPost().id))
        T.ok(st.osc_panel.copied == "Lead", "the copy is a fact of the session")
        -- One oscillator: no Delete.
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1 })
        local clicks = #S.clicks
        T.tripFrom({ action = "osc_menu_item", args = { index = 1, item = "Delete" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text
          == "An oscillator's menu: VPS Avenger deletes no oscillator when it has one", T.lastPost("error").text)
        -- Drums' Initialize, nothing shown changing: said so at the bound.
        T.event()
        T.tripFrom({ action = "drums_menu_item", args = { item = "Initialize" }, focused = true }, W)
        T.ok(T.clickAt() == "1020,140" and S.clicks[#S.clicks].button == "right", "Drums, right: " .. T.clickAt())
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 560, 36, 620, 80 } })
        T.tick(3)
        T.ok(T.clickAt() == "1020,194", "Initialize's row: " .. T.clickAt())
        T.repaint({ osc = 1, selected = "osc", index = 1 })
        T.tick(20)
        T.ok(T.lastPost("error").text == "The Drums' menu: Initialize chosen; nothing VPS Avenger shows changed",
          T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// An oscillator's knob reset: ONE double-click of the host's (host.input.click's `clicks = 2`, overlay
/// runtime 0.15) — both clicks with the right button for Pan, the second marked as click 2 — and the
/// knob read after it, said against the tool's default; never while Drums is selected, which has no
/// oscillator controls.
#[test]
fn a_knob_is_reset_with_a_double_click_and_never_on_the_drums_tab() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1, volume = 150, transpose = 0, fine = 0 })
        T.tripFrom({ action = "reset_osc_control", args = { control = "volume" }, focused = true }, W)
        T.ok(T.clickAt() == "652,193" and S.clicks[#S.clicks].double == 1 and #S.changeWaits == 0,
          "the first click: " .. T.clickAt())
        local n = #S.clicks
        T.tick(1)
        T.ok(#S.clicks == n + 1 and T.clickAt() == "652,193" and S.clicks[#S.clicks].double == 2 and #S.changeWaits == 1,
          "the second, marked click 2, then the wait")
        T.repaint({ osc = 2, selected = "osc", index = 1, volume = 100, transpose = 0, fine = 0 })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.volume_values[1] == 100 and T.lastPost().id == "osc-reset-volume",
          tostring(T.lastPost("state").state.osc_panel.volume_values[1]))
        T.ok(T.count("[avenger] [act] 'Resetting an oscillator's control': before 150 percent, after 100 percent, its "
          .. "default 100 percent (the tool's) — reset") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "reset_osc_control", args = { control = "pan" }, focused = true }, W)
        T.tick(1)
        T.ok(S.clicks[#S.clicks].button == "right" and S.clicks[#S.clicks - 1].button == "right"
          and S.clicks[#S.clicks].double == 2 and S.clicks[#S.clicks - 1].double == 1, "Pan: the right button, twice")
        T.tick(16)
        T.ok(T.lastPost("error").text == "Resetting an oscillator's control: Pan 0, already at its default",
          T.lastPost("error").text)
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "drums" })
        local clicks = #S.clicks
        T.tripFrom({ action = "reset_osc_control", args = { control = "volume" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text
          == "Resetting an oscillator's control: the Drums tab is selected; it has no oscillator controls", T.lastPost("error").text)
    "##));
}

/// The third Mac test's §7 (the avenger_control developer's report of 2026-10-25): a reset is said
/// against the control's default — the tool's, Volume 100 % and a macro's amount 0 — from the knob
/// read before and after the double-click: from 160 to 100 reset; 160 staying 160 failed; 160 to 130
/// changed but not to its default, not confirmed; 100 staying 100 already at its default; the knob
/// not read after, not confirmed. One log line each, with the three values. A macro knob from 40 to 0
/// reset, and from 0 already at its default. The default is never posted from the click: what is
/// posted is what was read.
#[test]
fn a_reset_is_said_against_the_tools_default() {
    run_mac(&scenario(r##"
        local LABEL = "Resetting an oscillator's control"
        local _, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1, volume = 160, transpose = 0, fine = 0 })
        local function reset(before, after, words)
          T.event()
          T.repaint({ osc = 1, selected = "osc", index = 1, volume = before, transpose = 0, fine = 0 })
          T.tripFrom({ action = "reset_osc_control", args = { control = "volume" }, focused = true }, W)
          T.tick(1)
          if after ~= before then
            T.repaint({ osc = 1, selected = "osc", index = 1, volume = after, transpose = 0, fine = 0 })
          end
          T.run(2600)
          local text = T.lastPost("error").text
          if words == nil then
            T.ok(text == "" and T.lastPost().id == "osc-reset-volume", "reset: " .. text)
          else
            T.ok(text == LABEL .. ": " .. words, text)
          end
        end
        reset(160, 100, nil)
        T.ok(T.count("before 160 percent, after 100 percent, its default 100 percent (the tool's) — reset") == 1, T.dump())
        reset(160, 160, "Volume 160 percent, unchanged; the reset failed")
        T.ok(T.count("before 160 percent, after 160 percent, its default 100 percent (the tool's) — failed: unchanged") == 1,
          T.dump())
        T.ok(T.lastPost("state").state.osc_panel.volume_values[1] == 160, "the value read posted, not the default")
        reset(160, 130, "Volume 130 percent, not its default 100 percent; the reset is not confirmed")
        T.ok(T.count("— changed, not to its default") == 1, T.dump())
        reset(100, 100, "Volume 100 percent, already at its default")
        T.ok(T.count("before 100 percent, after 100 percent, its default 100 percent (the tool's) — already at its default")
          == 1, T.dump())
        -- The knob not read after the click: its pointer gone from the picture.
        T.event()
        T.repaint({ osc = 1, selected = "osc", index = 1, volume = 160, transpose = 0, fine = 0 })
        T.tripFrom({ action = "reset_osc_control", args = { control = "volume" }, focused = true }, W)
        T.tick(1)
        T.repaint({ osc = 1, selected = "osc", index = 1, transpose = 0, fine = 0 })
        T.run(2600)
        T.ok(T.lastPost("error").text == LABEL .. ": Volume cannot be read now; whether it was reset is not confirmed",
          T.lastPost("error").text)
        T.ok(T.count("after not read, its default 100 percent (the tool's) — not read") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local words = { ["macro tabs"] = "MACRO 1", ["macro macro1"] = "Cutoff" }
        local _, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false },
          amounts = { 40, 0, 0 } }, words)
        T.tripFrom({ action = "reset_macro_amount", args = { index = 1 }, focused = true }, W)
        T.tick(1)
        T.ok(S.clicks[#S.clicks].double == 2 and S.clicks[#S.clicks - 1].double == 1, "one double-click of the host's")
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false }, amounts = { 0, 0, 0 } })
        T.settle()
        T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == 0 and T.lastPost().id == "macro-1-slot-1-reset-amount",
          tostring(T.lastPost("state").state.macro_panel.amount_values[1]))
        T.ok(T.count("before 40 percent, after 0 percent, its default 0 percent (the tool's) — reset") == 1, T.dump())
        -- From its default: said so.
        T.event()
        T.tripFrom({ action = "reset_macro_amount", args = { index = 1 }, focused = true }, W)
        T.run(2600)
        T.ok(T.lastPost("error").text == "Resetting a macro's amount: Cutoff 0 percent, already at its default",
          T.lastPost("error").text)
    "##));
}

/// A trip given up in the middle of an action — another application in front while the change is
/// waited for — reads nothing more and says so; and no picture is left held.
#[test]
fn a_trip_given_up_mid_action_reads_nothing_more() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1 })
        T.tripFrom({ action = "select_osc_tab", args = { index = 2 }, focused = true }, W)
        S.listed = { W, T.OTHER }
        T.show(T.OTHER)
        T.repaint({ osc = 2, selected = "osc", index = 2 })
        T.settle()
        T.ok(#S.changeWaits == 0 and T.lastPost("error").text
          == "Choosing an oscillator tab: another window came to the front; nothing more was done", T.lastPost("error").text)
        T.ok(T.count("[avenger] [act] 'Choosing an oscillator tab': changed") == 0, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A trip given up before its click — another application in front while the picture before the
/// click is taken — clicks nothing in a place the user has left, and releases that picture.
#[test]
fn a_trip_given_up_while_the_picture_before_the_click_is_taken_clicks_nothing() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1 })
        local clicks = #S.clicks
        S.onSnapshotWait = function()
          S.onSnapshotWait = nil
          S.listed = { W, T.OTHER }
          S.front, S.chain = T.OTHER, { T.OTHER }
        end
        T.tripFrom({ action = "select_osc_tab", args = { index = 2 }, focused = true }, W)
        T.event()
        T.settle()
        T.ok(#S.clicks == clicks and #S.changeWaits == 0, "nothing clicked: " .. (#S.clicks - clicks))
        T.ok(T.lastPost("error").text
          == "Choosing an oscillator tab: another window came to the front; nothing more was done", T.lastPost("error").text)
        T.ok(T.held() == 0, "the picture before the click released: " .. T.held())
    "##));
}

/// A right click's popup beside a lasting change of the same click — a panel drawn left of the
/// Trig tab, which stays: the item placed at the tool's own x inside what was seen, not in the
/// middle of the span, which lies left of the popup here; clicked once, and the popup counted gone
/// although the panel stays.
#[test]
fn a_right_clicks_item_lies_at_its_own_x_and_its_popup_goes_beside_a_lasting_change() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1 })
        T.tripFrom({ action = "trig_menu_item", args = { item = "Initialize" }, focused = true }, W)
        T.ok(T.clickAt() == "1084,140" and S.clicks[#S.clicks].button == "right", "Trig, right: " .. T.clickAt())
        local clicks = #S.clicks
        local after = { osc = 2, selected = "trig", lasting = { 496, 36, 560, 182 } }
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "trig", lasting = { 496, 36, 560, 182 }, popup = { 616, 36, 700, 70 } })
        T.tick(3)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "1084,194", "Initialize at the tool's x: " .. T.clickAt())
        T.ok(T.count("[avenger] 'The Trig's menu': the popup at (496.") == 1 and T.count("at the item's own x") == 1, T.dump())
        T.repaint(after)
        T.settle(6)
        T.ok(#S.clicks == clicks + 1, "clicked once: " .. (#S.clicks - clicks))
        T.ok(T.count("[menu item] 'VPS Avenger': 'The Trig's menu' chosen — clicked 1 time(s) at (1084,194); the menu has closed")
          == 1, T.dump())
        T.ok(T.lastPost("state").state.osc_panel.selected_type == "trig" and T.lastPost("error").text == "",
          tostring(T.lastPost("error").text))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The macros from the page.
// ---------------------------------------------------------------------------------------------

/// The macro tabs: counted from their captions before a click — tab 2 of two clicked, read as
/// shown after; a tab past the count, a fifth tab and a delete of the only one refused without a
/// click, the last two with the tool's focus; "+" where the count puts it; Delete Last Tab from tab
/// 1's menu, at the tool's x — and, where the popup did not take the item, closed with Escape
/// rather than a click on the tab, which a click outside a popup opened at a point would reach.
#[test]
fn macro_tabs_are_counted_before_a_click_and_read_after() {
    run_mac(&scenario(r##"
        local words = { ["macro tabs"] = "MACRO 1 MACRO 2" }
        local _, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 1 }, words)
        T.tripFrom({ action = "select_macro_tab", args = { index = 2 }, focused = true }, W)
        T.ok(T.clickAt() == "1324,782", "tab 2: " .. T.clickAt())
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 2 })
        T.settle()
        T.ok(T.lastPost("state").state.macro_panel.selected_index == 2 and T.lastPost().id == "macro-tab-2",
          tostring(T.lastPost("state").state.macro_panel.selected_index))
        T.event()
        local clicks = #S.clicks
        T.tripFrom({ action = "select_macro_tab", args = { index = 3 }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Choosing a macro tab: macro tab 3 is not there; VPS Avenger shows 2",
          T.lastPost("error").text)
        T.event()
        T.tripFrom({ action = "add_macro_tab", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1367,782", "+ of two tabs at 793: " .. T.clickAt())
        T.words({ ["macro tabs"] = "MACRO 1 MACRO 2 MACRO 3" })
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 3 })
        T.settle()
        T.ok(T.lastPost("state").state.macro_panel.count == 3 and T.lastPost().id == "macro-tab-3", tostring(T.lastPost().id))
        T.event()
        T.tripFrom({ action = "delete_macro_tab", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1256,782" and S.clicks[#S.clicks].button == "right", "tab 1, right: " .. T.clickAt())
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, macroShown = 3, popup = { 724, 437, 800, 495 } })
        T.tick(3)
        T.ok(T.clickAt() == "1256,858", "Delete's row: " .. T.clickAt())
        T.words({ ["macro tabs"] = "MACRO 1 MACRO 2" })
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 2 })
        T.settle(4)
        T.ok(T.lastPost("state").state.macro_panel.count == 2 and T.lastPost().id == "macro-delete-last", tostring(T.lastPost().id))
        T.event()
        T.words({ ["macro tabs"] = "MACRO 1" })
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1 })
        clicks = #S.clicks
        T.tripFrom({ action = "delete_macro_tab", args = {}, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Delete Last Tab: VPS Avenger keeps its only macro tab",
          T.lastPost("error").text)
        local posts = T.page().posts
        T.ok(posts[#posts - 1].kind == "focus" and posts[#posts - 1].id == "macro-tab-1", tostring(posts[#posts - 1].id))
        T.event()
        T.words({ ["macro tabs"] = "MACRO 1 MACRO 2 MACRO 3 MACRO 4" })
        T.tripFrom({ action = "add_macro_tab", args = {}, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Add Macro Tab: VPS Avenger has 4 macro tabs, the most it takes",
          T.lastPost("error").text)
        posts = T.page().posts
        T.ok(posts[#posts - 1].kind == "focus" and posts[#posts - 1].id == "macro-tab-4", tostring(posts[#posts - 1].id))
        -- A popup that does not take Delete, drawn as at the click from then on: the click is pending
        -- (overlay runtime 0.13), made once and given up at the press's bound, then the popup closed
        -- with Escape, and the tab is not clicked.
        T.event()
        T.words({ ["macro tabs"] = "MACRO 1 MACRO 2" })
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 2 })
        T.tripFrom({ action = "delete_macro_tab", args = {}, focused = true }, W)
        clicks = #S.clicks
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, macroShown = 2, popup = { 724, 437, 800, 495 } })
        T.tick(80)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "1256,858", "Delete clicked once: " .. (#S.clicks - clicks))
        T.ok(S.sent[#S.sent] == "Escape" and T.count("[avenger] 'Delete Last Tab': its popup was left open (refused) — "
          .. "pressing Escape to close it") == 1, T.dump())
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 2 })
        T.settle(4)
        T.ok(#S.clicks == clicks + 1, "the tab not clicked to close it")
        T.ok(T.lastPost("error").text == "Delete Last Tab: the menu did not take the choice", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A macro button: clicked, its highlight read after — a click that changes nothing is said
/// "unchanged" with the state read, never turned over from memory; a reset mode, which Avenger shows
/// only in its menu, said only as chosen; a button menu's item clicked once, and the buttons read
/// again once they have changed — not at once, before Avenger has acted on the click; and a knob's
/// amount reset with a double-click, read after it.
#[test]
fn macro_buttons_reset_modes_button_menus_and_amounts() {
    run_mac(&scenario(r##"
        local words = { ["macro tabs"] = "MACRO 1", ["macro btn1"] = "Hold", ["macro macro1"] = "Cutoff" }
        local _, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false },
          amounts = { 40, 0, 0 } }, words)
        T.tripFrom({ action = "toggle_macro_button", args = { index = 1 }, focused = true }, W)
        T.ok(T.clickAt() == "1316,921", "button 1: " .. T.clickAt())
        T.tick(16)
        local m = T.lastPost("state").state.macro_panel
        T.ok(T.lastPost("error").text == "A macro button: unchanged, Hold OFF" and m.button_states[1] == false,
          T.lastPost("error").text)
        -- A reset mode: the field's menu, the mode's row, and nothing read after it.
        T.event()
        T.tripFrom({ action = "macro_reset_mode", args = { index = 1, mode = "snap" }, focused = true }, W)
        T.ok(T.clickAt() == "1276,802" and S.clicks[#S.clicks].button == "right", "macro 1's field, right: " .. T.clickAt())
        local waits = S.changeWaitsAsked
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false },
          amounts = { 40, 0, 0 }, popup = { 736, 451, 800, 478 } })
        T.tick(3)
        T.ok(T.clickAt() == "1284,836", "Snap At Center's row: " .. T.clickAt())
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false }, amounts = { 40, 0, 0 } })
        T.settle(4)
        T.ok(S.changeWaitsAsked == waits and T.lastPost("error").text == "" and T.lastPost().id == "macro-1-slot-1-menu",
          tostring(T.lastPost().id))
        -- A button's menu item: once; then the buttons' change waited for — Special Switch on button 2
        -- turns button 1 off — and both read after it.
        T.event()
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { true, false }, amounts = { 40, 0, 0 } })
        T.tripFrom({ action = "macro_button_menu_item", args = { index = 2, item = "special" }, focused = true }, W)
        T.ok(T.clickAt() == "1399,921" and S.clicks[#S.clicks].button == "right", "button 2, right: " .. T.clickAt())
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { true, false },
          amounts = { 40, 0, 0 }, popup = { 790, 530, 860, 558 } })
        T.tick(3)
        local clicks = #S.clicks
        T.ok(T.clickAt() == "1410,948", "Special Switch's row: " .. T.clickAt())
        T.tick(4)
        T.ok(#S.clicks == clicks and #S.changeWaits == 1 and T.count("[avenger] [act] 'A macro button's menu': changed") == 0,
          "clicked once, the buttons' change still waited for")
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false }, amounts = { 40, 0, 0 } })
        T.settle(4)
        T.ok(#S.clicks == clicks, "clicked once")
        T.ok(T.lastPost().id == "macro-1-slot-5-menu" and T.lastPost("state").state.macro_panel.button_states[1] == false,
          tostring(T.lastPost().id) .. " " .. tostring(T.lastPost("state").state.macro_panel.button_states[1]))
        -- The amount, reset.
        T.event()
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false }, amounts = { 40, 0, 0 } })
        T.tripFrom({ action = "reset_macro_amount", args = { index = 1 }, focused = true }, W)
        T.ok(T.clickAt() == "1276,852", "knob 1: " .. T.clickAt())
        T.tick(1)
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false }, amounts = { 0, 0, 0 } })
        T.settle()
        T.ok(T.lastPost("state").state.macro_panel.amount_values[1] == 0 and T.lastPost().id == "macro-1-slot-1-reset-amount",
          tostring(T.lastPost("state").state.macro_panel.amount_values[1]))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// On Windows, Avenger's own child window on a display at 150 %: Drums' Mute clicked at the tool's
/// point through the factor 2.4, and read after.
#[test]
fn on_windows_at_150_percent_a_led_is_clicked_through_the_factor_and_read() {
    run(r##"
        local S = T.S
        local j = T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 2.4, { osc = 2, selected = "osc", index = 1 })
        T.words({})
        T.openPage()
        T.event()
        S.focusAtOnce = false
        S.focusLands = function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageMsg({ action = "toggle_drums_mute", args = {}, focused = true })
        T.land()
        T.runDue()
        -- (569, 26) at 2.4 from (248, 60): 248 + 1365.6 -> 1614, 60 + 62.4 -> 122.
        T.ok(T.clickAt() == "1614,122", "Drums' Mute: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1, drumsMute = true })
        T.tick(3)
        T.ok(T.lastPost("state").state.osc_panel.drums_mute == true, "read ON")
    "##);
}

// ---------------------------------------------------------------------------------------------
// The ring.
// ---------------------------------------------------------------------------------------------

/// Some of the ring's controls after Zoom: the Oscillator stepper reads the strip as it is first
/// announced, steps to the next tab with the same click as the page's, and says the tab read after
/// it; Mute says the selected tab's switch and clicks it; Reset level after Level, the tool's
/// separate reset button, double-clicks the knob and says the value read; Reset pan with the right
/// button; Macro button 1 says its name and state and clicks it. A part is read once in a stay.
/// (Level itself turns the knob: the next test.)
#[test]
fn the_rings_strip_and_macro_controls_say_what_was_read_and_click_as_the_page_does() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, volume = 150, transpose = 0, fine = 0,
          macroShown = 1, buttons = { false, false } })
        T.words({ ["macro tabs"] = "MACRO 1", ["macro btn1"] = "Hold" })
        local i = T.indexOf(header, "Oscillator")
        local snaps = S.snaps
        T.stepAt(header, i, 1)
        T.ok(S.snaps == snaps + 2 and T.clickAt() == "721,140", "the strip read, then the picture before the click: "
          .. T.clickAt())
        T.repaint({ osc = 3, selected = "osc", index = 2, volume = 150, transpose = 0, fine = 0, macroShown = 1,
          buttons = { false, false } })
        T.tick(4)
        T.ok(string.find(T.lastSaid() or "", "OSC 2", 1, true) ~= nil, T.said())
        -- Mute of OSC 2.
        i = T.indexOf(header, "Mute")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, OSC 2, off, button", T.said())
        T.pressAt(header, i)
        T.repaint({ osc = 3, selected = "osc", index = 2, mute = { [2] = true }, volume = 150, transpose = 0, fine = 0,
          macroShown = 1, buttons = { false, false } })
        T.tick(3)
        T.ok(T.lastSaid() == "OSC 2 Mute ON", T.said())
        -- Reset level, after the knob: a double-click on the knob, at (346, 68) at 1.6 from (98, 84).
        i = T.indexOf(header, "Reset level")
        T.ok(header.controls[i - 1].label == "Level", "after the knob")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Reset level, button", T.said())
        local clicks = #S.clicks
        T.pressAt(header, i)
        T.tick(1)
        T.ok(#S.clicks == clicks + 2 and T.clickAt() == "652,193" and S.clicks[#S.clicks].button == nil
          and S.clicks[#S.clicks - 1].button == nil, "a double-click on the knob, the left button: " .. T.clickAt())
        T.repaint({ osc = 3, selected = "osc", index = 2, mute = { [2] = true }, volume = 100, transpose = 0, fine = 0,
          macroShown = 1, buttons = { false, false } })
        T.tick(3)
        T.ok(T.lastSaid() == "Volume 100 percent, reset", T.said())
        -- Reset pan: the right button.
        clicks = #S.clicks
        T.pressAt(header, T.indexOf(header, "Reset pan"))
        T.tick(1)
        T.ok(#S.clicks == clicks + 2 and S.clicks[#S.clicks].button == "right" and S.clicks[#S.clicks - 1].button == "right",
          "Pan with the right button")
        T.tick(16)
        T.ok(T.lastSaid() == "Pan 0, already at its default", T.said())
        -- Macro button 1.
        i = T.indexOf(header, "Macro button 1")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Macro button 1, Hold, off, button", T.said())
        T.pressAt(header, i)
        T.repaint({ osc = 3, selected = "osc", index = 2, mute = { [2] = true }, volume = 100, transpose = 0, fine = 0,
          macroShown = 1, buttons = { true, false } })
        T.tick(3)
        T.ok(T.lastSaid() == "Hold ON", T.said())
    "##));
}

/// A12, the maintainer's decision of 2026-10-24: the ring's knobs turn as the tool's knob buttons
/// do, through the page's own gesture (src/amount.luau). Return on Level reads the strip and the
/// knob, holds PageUp, PageDown, Return and Escape with the runtime's O:holdKeys while the overlay
/// stays in front, presses the knob and holds it, and says the tool's sentence behind the screen
/// reader, logged as everything the ring says. Each PageDown is one point times the factor; Tab
/// still moves the focus, and a stepper's press says the ring is still working on the last press, as
/// do ◀, ▶, MENU's items, Zoom, Undo and the redo list, which click nothing meanwhile;
/// Return — the hold's on whichever control the focus is — lets go where the keys left the pointer,
/// and the knob is read after a change wait and said; then the keys are the overlay's again. Pan
/// presses and lets go with the right button, and Return with nothing moved reads it at once.
/// Macro 1, its name read, says the tool's macro sentence; Escape with nothing moved says the
/// amount unchanged and reads nothing after, and after moves that it was let go and not read, which
/// the knob then says. Leaving the front lets go once and says nothing; back in front a press
/// works again. On the Drums tab nothing is pressed, and keys the host cannot capture press
/// nothing. A key of the hold where the overlay has moved on ends the gesture, let go and silent.
#[test]
fn the_rings_knobs_turn_with_page_up_and_page_down_and_return_confirms() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("LD Lead 1")
        local function paint(extra)
          local spec = { osc = 1, selected = "osc", index = 1, volume = 150, transpose = 0, fine = 0, macroShown = 1,
            amounts = { 40, 0, 0 } }
          for k, v in pairs(extra or {}) do spec[k] = v end
          T.paintAvenger(98, 84, 1.6, spec)
        end
        paint()
        T.words({ ["macro tabs"] = "MACRO 1", ["macro macro1"] = "Cutoff" })
        local level = T.indexOf(header, "Level")
        local said = #S.speech
        T.pressAt(header, level)
        -- The press at (346, 68) at 1.6 from (98, 84), with the left button.
        local d = T.inputs("down")
        T.ok(#d == 1 and T.xy(d[1]) == "652,193" and d[1].button == "left", T.xy(d[1]))
        for _, k in ipairs({ "PageUp", "PageDown", "Return", "Escape" }) do T.ok(S.holding[k], k .. " held") end
        T.ok(T.count("[keys] 'VPS Avenger' holds PageUp, PageDown, Return, Escape for its module") == 1, T.dump())
        T.ok(#S.speech == said + 1 and T.lastSaid()
          == "Please press Page Up or Down to set the volume of OSC 1. To Confirm, press Return."
          and S.speech[#S.speech].interrupt == false, T.said(said + 1))
        T.ok(T.count("[avenger] [ring] said: Please press Page Up or Down to set the volume of OSC 1.") == 1, T.dump())
        -- 69, 70 at 1.6: 194.9 -> 194, 196.5 -> 196.
        T.ok(T.typed("PageDown") == "taken" and T.typed("PageDown") == "taken", "the hold's keys")
        local m = T.inputs("move")
        T.ok(#m == 2 and T.xy(m[1]) == "652,194" and T.xy(m[2]) == "652,196", T.xy(m[1]) .. " " .. T.xy(m[2]))
        -- Tab and Shift+Tab do nothing while the knob is held (Act.SWALLOWED): swallowed, also by the
        -- hook, the first saying how the hold ends, once and interrupting.
        T.ok(T.count("[keys] 'VPS Avenger' holds PageUp, PageDown, Return, Escape for its module until its onKey ends "
          .. "it; Tab, Shift+Tab swallowed meanwhile") == 1, T.dump())
        local at = header.focus
        T.tab()
        T.ok(header.focus == at and T.lastSaid() == "Return confirms, Escape cancels."
          and S.speech[#S.speech].interrupt == true, "Tab swallowed: " .. T.said())
        local swallowed = #S.speech
        T.ok(T.typed("Shift+Tab") == "taken" and T.typed("Tab") == "taken" and header.focus == at
          and #S.speech == swallowed and #T.inputs("move") == 2 and #T.inputs("up") == 0, "said once, nothing moved")
        -- The overlay's other keys work on.
        local clicks = #S.clicks
        T.stepAt(header, T.indexOf(header, "Oscillator"), 1)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Oscillator: still working on the last press", T.said())
        -- The header's own controls that click Avenger click nothing either, pressed with Space (Return
        -- is the hold's): a click with the knob's button held would drag the knob to its point.
        for _, label in ipairs({ "Next preset", "Previous preset", "Load preset", "Initialize preset", "Undo", "Redo list" }) do
          header.focus = T.indexOf(header, label)
          header:_syncNativeKeys()
          T.ok(S.holding["Space"] ~= nil, "Space activates " .. label)
          S.epoch += 1
          T.call("key", S.captured["Space"])
          T.ok(#S.clicks == clicks and T.lastSaid() == label .. ": still working on the last press", label .. ": " .. T.said())
        end
        T.stepAt(header, T.indexOf(header, "Zoom"), 1)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Zoom: still working on the last press", T.said())
        T.ok(#T.inputs("down") == 1 and #T.inputs("up") == 0, "only the knob pressed, and still held")
        -- Return, the hold's on the stepper too: let go at once where the keys left it, then read.
        paint({ volume = 100 })
        local n = #S.input
        T.ok(T.typed("Return") == "taken", "Return is the hold's")
        T.ok(#S.input == n + 1 and S.input[n + 1][1] == "up" and T.xy(S.input[n + 1]) == "652,196"
          and S.input[n + 1].button == "left", "let go at once: " .. T.xy(S.input[n + 1]))
        T.tick(3)
        T.ok(T.lastSaid() == "Volume 100 percent", T.said())
        T.ok(T.count("[avenger] [amount] 'volume': read Volume 100 percent (Volume 150 percent before) — changed true") == 1,
          T.dump())
        T.ok(not S.holding["PageUp"] and not S.holding["Escape"] and T.typed("PageUp") == "application", "the keys back")
        T.ok(S.holding["Tab"] and not S.menuTaken["Tab"] and not S.menuTaken["Shift+Tab"], "Tab the ring's again")
        T.ok(T.count("[keys] 'VPS Avenger': its hold of PageUp, PageDown, Return, Escape ends (ended)") == 1, T.dump())
        -- Pan: the right button, read at once with nothing moved.
        local pan = T.indexOf(header, "Pan")
        T.pressAt(header, pan)
        d = T.inputs("down")
        T.ok(#d == 2 and d[2].button == "right" and S.heldButtons[1] == "right", "Pan with the right button")
        T.ok(T.lastSaid() == "Please press Page Up or Down to set the pan of OSC 1. To Confirm, press Return.", T.said())
        T.ok(T.typed("Return") == "taken", "Return")
        T.tick(3)
        local u = T.inputs("up")
        T.ok(#u == 2 and u[2].button == "right" and T.xy(u[2]) == T.xy(d[2]) and #S.heldButtons == 0, "let go of the right")
        T.ok(T.lastSaid() == "Pan 0", T.said())
        -- Macro 1, its name read as it is announced.
        local macro = T.indexOf(header, "Macro 1")
        header.focus = macro - 1
        T.tab()
        T.ok(T.lastSaid() == "Macro 1, Cutoff, 40 percent, button", T.said())
        local snaps = S.snaps
        T.pressAt(header, macro)
        T.ok(T.lastSaid() == "Please press Page Up or Down to set the amount of the Cutoff Slider. To Confirm, press Return.",
          T.said())
        T.ok(T.typed("Escape") == "taken", "Escape")
        T.tick(3)
        T.ok(T.lastSaid() == "Cutoff 40 percent, unchanged" and S.snaps == snaps + 1, "nothing read after: " .. T.said())
        T.pressAt(header, macro)
        T.typed("PageUp")
        T.typed("PageUp")
        paint({ amounts = { 60, 0, 0 } })
        T.ok(T.typed("Escape") == "taken", "Escape")
        T.tick(3)
        T.ok(T.lastSaid() == "Cutoff let go, not read", T.said())
        header.focus = macro - 1
        T.tab()
        T.ok(T.lastSaid() == "Macro 1, Cutoff, amount not read, button", T.said())
        T.ok(T.ups() == 4 and T.held() == 0, "let go four times, no picture held: " .. T.ups())
        -- Leaving the front while the knob is held: let go once, nothing said.
        T.pressAt(header, level)
        T.typed("PageUp")
        said = #S.speech
        S.listed = { W, T.OTHER }
        T.show(T.OTHER)
        T.tick(3)
        T.ok(T.ups() == 5 and #S.heldButtons == 0 and #S.speech == said, "let go once, nothing said: " .. T.said(said + 1))
        T.ok(T.count("[keys] 'VPS Avenger': its hold of PageUp, PageDown, Return, Escape ends (left)") == 1, T.dump())
        T.ok(not S.holding["PageUp"], "the keys back")
        -- Back in front: a press works again; on the Drums tab nothing is pressed.
        S.listed = { W }
        T.show(W)
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.run(400)
        T.ok(header.active, "back in front: " .. T.dump())
        paint({ selected = "drums" })
        local downs = #T.inputs("down")
        T.pressAt(header, level)
        T.ok(#T.inputs("down") == downs and not S.holding["PageUp"], "nothing pressed or held")
        T.ok(T.lastSaid() == "Level: the Drums tab is selected; it has no oscillator controls", T.said())
        -- Keys the host cannot capture: nothing pressed.
        paint()
        S.captureRaises = { PageUp = "the event tap could not be created (Accessibility)" }
        T.pressAt(header, level)
        S.captureRaises = nil
        T.ok(#T.inputs("down") == downs, "nothing pressed")
        T.ok(T.lastSaid() == "Level: the keyboard cannot be handed to VPS Avenger now", T.said())
        -- A key of the hold where the overlay is no longer where the press began — its stay moved on
        -- under the hold, which no recheck of the runtime's leaves standing: the gesture ends at that
        -- key, let go once and nothing said, and the keys are the overlay's again.
        T.pressAt(header, level)
        local ups, moves = T.ups(), #T.inputs("move")
        said = #S.speech
        header._stay += 1
        T.ok(T.typed("PageDown") == "taken", "the hold's key")
        header._stay -= 1
        T.ok(T.ups() == ups + 1 and #S.heldButtons == 0 and #S.speech == said, "let go once, nothing said: "
          .. T.said(said + 1))
        T.ok(#T.inputs("move") == moves and not S.holding["PageUp"], "no move, the keys back")
        T.ok(T.held() == 0 and #S.heldButtons == 0, "nothing held: " .. T.held())
    "##));
}

/// The selected tab is the darkest bottom strip only where it stands out — by 5 and by at least the
/// spread of the others (read/strip.luau): with no tab drawn selected, every strip alike, none is
/// taken, where the darkest of equals was the first; with one drawn selected, that one. A band whose
/// every row differs from tab to tab, as the second Mac test's (2026-10-24): Trig the darkest by 7,
/// over the others 21 apart — none.
#[test]
fn the_selected_tab_is_one_that_stands_out_or_none() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 3 })
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.count == 3 and o.selected_type == nil and o.selected_index == nil, "none: " .. tostring(o.selected_type))
        T.ok(T.count("— none: OSC 1 by 0.0, the others within 0.0") == 1, T.dump())
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 2 })
        T.openPage()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.selected_type == "osc" and o.selected_index == 2, "OSC 2: " .. tostring(o.selected_index))
        -- Mid-redraw.
        T.event()
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 3, strips = { 69, 80, 90, drums = 85, trig = 62 } })
        T.openPage()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.selected_type == nil and T.count("— none: Trig by 7.0, the others within 21.0") == 1,
          tostring(o.selected_type) .. " " .. T.dump())
    "##));
}

/// Under a Solo, Avenger lights every other tab's Mute as a Mute pressed: with OSC 1's Solo on and
/// OSC 2's and OSC 3's Mute lit, the ring's Mute on OSC 2 says it silenced by OSC 1's Solo, never on;
/// its press, which changes nothing that can be read, says the same, unchanged; the page is posted
/// the tabs whose Solo is on (`solo_by`), by which it words the other Mutes. With the Solo off, a
/// Mute is on or off again. A tab whose own Solo is on has its Mute its own: OSC 2 soloed beside
/// OSC 1, its Mute lit, says it on; Drums' Solo is among the Solos posted.
#[test]
fn a_mute_under_another_tabs_solo_is_said_as_silenced_by_it() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("LD Lead 1")
        local SOLO = { osc = 3, selected = "osc", index = 2, solo = { [1] = true }, mute = { [2] = true, [3] = true } }
        T.paintAvenger(98, 84, 1.6, SOLO)
        T.words({})
        local i = T.indexOf(header, "Mute")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, OSC 2, silenced by OSC 1's Solo, button", T.said())
        T.pressAt(header, i)
        T.tick(16)
        T.ok(T.lastSaid() == "OSC 2 Mute, silenced by OSC 1's Solo, unchanged", T.said())
        -- The page's key: the tabs whose Solo is on.
        T.event()
        T.pageReady()
        T.openPage()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.solo_by and table.concat(o.solo_by, "|") == "OSC 1" and o.mute_states[2] == true, "the page: "
          .. table.concat(o.solo_by or {}, "|"))
        -- The Solo off: OSC 2's Mute is its own again.
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 2, mute = { [2] = true } })
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.run(400)
        -- The press reads the strip again, before and after its click.
        T.pressAt(header, i)
        T.tick(16)
        T.ok(T.lastSaid() == "OSC 2 Mute ON, unchanged", T.said())
        T.openPage()
        T.ok(T.lastPost("state").state.osc_panel.solo_by == nil, "no Solo on")
        -- OSC 2's own Solo on beside OSC 1's and Drums': its Mute its own.
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 2, solo = { [1] = true, [2] = true },
          mute = { [2] = true }, drumsSolo = true })
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.run(400)
        T.pressAt(header, i)
        T.tick(16)
        T.ok(T.lastSaid() == "OSC 2 Mute ON, unchanged", "its own Solo on: " .. T.said())
        T.openPage()
        local by = T.lastPost("state").state.osc_panel.solo_by
        T.ok(table.concat(by or {}, "|") == "OSC 1|OSC 2|Drums", "the Solos: " .. table.concat(by or {}, "|"))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The avenger_control developer's report of 2026-10-26: the ring's Oscillator stepper said only
/// "OSC 1", "OSC 2", where the page names the tabs. A tab is now said by its name: one read off the
/// screen first, else the preset database's for the preset read — as it is, with no "from the
/// database" (the maintainer, 2026-10-26) — else "OSC <i>". AR Band Pass Slide's three tabs are
/// Arp, OSC 2 (none of the sound designer's) and Lead: the stepper says Arp, and two steps on Lead,
/// with no text read on a step; Mute and Solo say Lead, and so does Mute's answer; under Arp's
/// Solo, Mute is silenced by Arp's Solo. Add oscillator reads the names: a name read wins, and the
/// fourth tab, not read, is OSC 4 — four tabs are not the database's three, whose names are then
/// not taken. The words come from the stepper's own function, Osc.tabWords, which takes a name read
/// over the database's.
#[test]
fn the_rings_oscillator_stepper_mute_and_solo_say_a_tab_by_its_name() {
    run_mac(&scenario(r##"
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1 })
        T.words({})
        local i = T.indexOf(header, "Oscillator")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, Arp, slider", T.said())
        -- Two steps: the strip read before and after each click, its text never.
        local reads = #S.recognized
        T.stepAt(header, i, 1)
        T.repaint({ osc = 3, selected = "osc", index = 2 })
        T.tick(4)
        T.ok(string.find(T.lastSaid() or "", "OSC 2", 1, true) ~= nil, T.said())
        T.stepAt(header, i, 1)
        T.repaint({ osc = 3, selected = "osc", index = 3 })
        T.tick(4)
        T.ok(string.find(T.lastSaid() or "", "Lead", 1, true) ~= nil and string.find(T.lastSaid(), "OSC", 1, true) == nil,
          T.said())
        local texts = {}
        for k = reads + 1, #S.recognized do texts[#texts + 1] = tostring(S.recognized[k].name) end
        T.ok(#texts == 0, "no text read on a step: " .. table.concat(texts, ", "))
        -- Mute and Solo of Lead.
        local m, s = T.indexOf(header, "Mute"), T.indexOf(header, "Solo")
        header.focus = m - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, Lead, off, button", T.said())
        T.pressAt(header, m)
        T.repaint({ osc = 3, selected = "osc", index = 3, mute = { [3] = true } })
        T.tick(3)
        T.ok(T.lastSaid() == "Lead Mute ON", T.said())
        header.focus = s - 1
        T.tab()
        T.ok(T.lastSaid() == "Solo, Lead, off, button", T.said())
        -- Under Arp's Solo.
        T.pressAt(header, m)
        T.repaint({ osc = 3, selected = "osc", index = 3, mute = { [3] = true }, solo = { [1] = true } })
        T.tick(16)
        T.ok(T.lastSaid() == "Lead Mute, silenced by Arp's Solo, unchanged", T.said())
        header.focus = m - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, Lead, silenced by Arp's Solo, button", T.said())
        -- Add oscillator: the names read, the new fourth tab selected and not read.
        T.words({ ["osc 1"] = "Arp", ["osc 2"] = "Wobble", ["osc 3"] = "Lead" })
        reads = #S.recognized
        T.pressAt(header, T.indexOf(header, "Add oscillator"))
        T.repaint({ osc = 4, selected = "osc", index = 4 })
        T.tick(3)
        T.ok(T.lastSaid() == "4 oscillators", T.said())
        local named = {}
        for k = reads + 1, #S.recognized do named[tostring(S.recognized[k].name)] = true end
        T.ok(named["osc 2"] and named["osc 4"], "the names read after Add oscillator, as a step reads none")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, OSC 4, slider", T.said())
        T.stepAt(header, i, -1)
        T.repaint({ osc = 4, selected = "osc", index = 3 })
        T.tick(4)
        T.ok(string.find(T.lastSaid() or "", "Lead", 1, true) ~= nil, T.said())
        header.focus = m - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, Lead, off, button", T.said())
        -- The stepper's words: a name read over a read's words for the tab (a name kept, or the
        -- database's), those over Avenger's own.
        local Osc = S.included["modules/vps-avenger/src/actions/osc.luau"][1]
        local both = { selectedType = "osc", selectedIndex = 2, oscNames = { [2] = "Wobble" },
          oscWords = { "Arp", "Bass", "Lead" } }
        T.ok(Osc.tabWords(both) == "Wobble", tostring(Osc.tabWords(both)))
        both.oscNames = { [1] = "Arp" }
        T.ok(Osc.tabWords(both) == "Bass" and Osc.oscName(4, both) == "OSC 4", tostring(Osc.tabWords(both)))
        T.ok(string.find(T.said(), "database", 1, true) == nil, "no mark: " .. T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Where the database gives no name, a tab is Avenger's "OSC <i>": for a preset it places by
/// similarity alone (a poor read of AR Band Pass Slide), for one it does not have, and where the
/// strip shows another count than the database's three — a tab added or deleted since.
#[test]
fn the_rings_oscillator_stepper_says_osc_i_where_the_database_names_no_tab() {
    for (name, osc) in [("AR Band Pass Slida", 3), ("My Own Lead", 3), ("AR Band Pass Slide", 2)] {
        let s = r##"
            T.presets()
            local header = T.avengerAt80("@NAME@")
            T.paintAvenger(98, 84, 1.6, { osc = @OSC@, selected = "osc", index = 1 })
            T.words({})
            header.focus = T.indexOf(header, "Oscillator") - 1
            T.tab()
            T.ok(T.lastSaid() == "Oscillator, OSC 1, slider", "@NAME@, @OSC@ tabs: " .. T.said())
            header.focus = T.indexOf(header, "Mute") - 1
            T.tab()
            T.ok(T.lastSaid() == "Mute, OSC 1, off, button", "@NAME@, @OSC@ tabs: " .. T.said())
            T.ok(T.held() == 0, "every picture released: " .. T.held())
        "##
        .replace("@NAME@", name)
        .replace("@OSC@", &osc.to_string());
        run_mac(&scenario(&s));
    }
}

/// The page's own answers say a tab as the page shows it (the review of 2026-10-26), and since 0.16
/// the database's name first where it has the preset (the maintainer's decision of 2026-10-31, D3):
/// the key's read of AR Band Pass Slide's first tab as Pluck is the check of the database's Arp, and
/// disagrees — so the page shows "Arp, from the database", choosing it, selected already, is
/// "unchanged, Arp, from the database is selected", and its Mute "Arp, from the database Mute ON".
/// After ▶ to a preset the database does not have, the names the key took are the preset before's,
/// and the tab is OSC 1 again; ◀ back to AR Band Pass Slide loads it from its file again, and the
/// tab is the database's Arp, unmarked: no read of the key's is about this load. A Copy of a tab whose name was not read is the
/// database's name, as the stepper says it; and a Paste over it is an edit of the preset's tabs,
/// after which the ring takes none of the database's names for them: back in Avenger the stepper
/// says OSC 1.
#[test]
fn the_pages_answers_say_a_tab_as_shown_and_an_edit_takes_the_databases_names_away() {
    run_mac(&scenario(r##"
        T.presets()
        local _, _, W = T.avengerAt80("AR Band Pass Slide")
        T.pageReady()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1 })
        T.words({ preset = "AR Band Pass Slide", ["osc 1"] = "Pluck", ["osc 2"] = "OSC 2", ["osc 3"] = "Lead" })
        T.openPage()
        T.event()
        T.ok(T.lastPost("state").state.osc_panel.names[1] == "Arp, from the database", "checked at the key: "
          .. tostring(T.lastPost("state").state.osc_panel.names[1]))
        T.ok(T.count("1 'Pluck', not the database's 'Arp' (0.25 alike) — the database's, marked") == 1, T.dump())
        T.tripFrom({ action = "select_osc_tab", args = { index = 1 }, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "Choosing an oscillator tab: unchanged, Arp, from the database is selected",
          T.lastPost("error").text)
        T.event()
        T.tripFrom({ action = "toggle_osc_mute", args = { index = 1 }, focused = true }, W)
        T.repaint({ osc = 3, selected = "osc", index = 1, mute = { [1] = true } })
        T.settle()
        T.ok(T.count("[avenger] [act] 'An oscillator's Mute': changed — Arp, from the database Mute ON") == 1, T.dump())
        -- ▶ to My Own Lead: the page's state keeps the names, which are the preset before's.
        T.event()
        T.tripFrom({ action = "next_preset", args = {}, focused = true }, W)
        T.nameReads("My Own Lead", 600)
        T.ok(T.lastPost("state").state.preset == "My Own Lead", tostring(T.lastPost("state").state.preset))
        T.event()
        T.tripFrom({ action = "select_osc_tab", args = { index = 1 }, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "Choosing an oscillator tab: unchanged, OSC 1 is selected", T.lastPost("error").text)
        -- ◀ back to AR Band Pass Slide loads it again from its file: the database's Arp, not Pluck.
        T.event()
        T.tripFrom({ action = "prev_preset", args = {}, focused = true }, W)
        T.nameReads("AR Band Pass Slide", 600)
        T.event()
        T.tripFrom({ action = "select_osc_tab", args = { index = 1 }, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "Choosing an oscillator tab: unchanged, Arp is selected", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        T.presets()
        local header, _, W = T.avengerAt80("AR Band Pass Slide")
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1 })
        T.words({ preset = "AR Band Pass Slide" })
        local i = T.indexOf(header, "Oscillator")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, Arp, slider", T.said())
        T.pageReady()
        T.openPage()
        T.event()
        T.tripFrom({ action = "osc_menu_item", args = { index = 1, item = "Copy" }, focused = true }, W)
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, popup = { 265, 36, 340, 90 } })
        T.tick(3)
        T.closePopups()
        T.repaint({ osc = 3, selected = "osc", index = 1 })
        T.settle(4)
        T.ok(T.lastPost("state").state.osc_panel.copied == "Arp" and T.lastPost("error").text == "",
          tostring(T.lastPost("state").state.osc_panel.copied))
        T.event()
        T.tripFrom({ action = "osc_menu_item", args = { index = 1, item = "Paste" }, focused = true }, W)
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, popup = { 265, 36, 340, 90 } })
        T.tick(3)
        T.ok(T.clickAt() == "522,175", "Paste's row: " .. T.clickAt())
        T.repaint({ osc = 3, selected = "osc", index = 1 })
        T.tick(20)
        T.ok(T.lastPost("error").text == "An oscillator's menu: Paste chosen; nothing VPS Avenger shows changed",
          T.lastPost("error").text)
        T.event()
        -- Back in Avenger with the page key: the arrival on the stepper says OSC 1, not Arp.
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.run(400)
        T.ok(header.active and S.front == W, "the header again")
        T.ok(T.lastSaid() == "Oscillator, OSC 1, slider", "the arrival: " .. T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// After ▶ no name is known until the watch has settled on one (main.luau, lastName): the stepper
/// read before then — Tab onto it at once — says OSC 1, not the database's Arp of the preset
/// before, and once the watch has read AR Deep Arp the strip is read again for its one tab, One
/// Osc. After MENU's Load the preset is most likely the user's own file, whatever its name: AR Band
/// Pass Slide loaded so is said OSC 1, the database's names not taken.
#[test]
fn the_rings_tab_names_wait_for_the_name_a_step_settles_on_and_take_none_after_load() {
    run_mac(&scenario(r##"
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1 })
        local i = T.indexOf(header, "Oscillator")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, Arp, slider", T.said())
        T.nameIs("AR Band Pass Slide")
        T.pressAt(header, 3)
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, OSC 1, slider", "before the watch read a name: " .. T.said())
        T.repaint({ osc = 1, selected = "osc", index = 1 })
        for _ = 1, 25 do
          T.run(150)
          local r = T.lastRead("vps-avenger name")
          if r and not r.answered then T.answer(r, T.nameRead("AR Deep Arp")) end
        end
        T.ok(T.lastSaid() == "AR Deep Arp", T.said())
        local snaps = S.snaps
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, One Osc, slider" and S.snaps == snaps + 1, "read again: " .. T.said())
        -- MENU's Load, with no dialog seen: the name read after it all the same.
        local base = { osc = 3, selected = "osc", index = 1 }
        T.pressAt(header, T.indexOf(header, "Load preset"))
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, popup = { 242, 20, 322, 70 } })
        T.tick(3)
        T.ok(T.clickAt() == "509,135", "Load's point: " .. T.clickAt())
        T.repaint(base)
        T.nameIs("AR Band Pass Slide")
        T.run(8200)
        T.nameReads("AR Band Pass Slide")
        T.ok(T.lastSaid() == "Load preset: AR Band Pass Slide — no file dialog was seen", T.said())
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, OSC 1, slider", "after Load: " .. T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// What the ring read is forgotten where Avenger may have changed with nothing of the ring's read
/// since: after a trip of the page — the overlay away meanwhile, not told it left — after ▶, which
/// may load another preset, and after UNDO, which may take it back. Mute says the switch read
/// again, off a new picture, each time.
#[test]
fn the_rings_reads_are_forgotten_after_a_trip_of_the_page_and_after_next_preset() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("LD Lead 1")
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1 })
        T.words({})
        local i = T.indexOf(header, "Mute")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, OSC 1, off, button", T.said())
        -- The page, and a trip of it that lights OSC 1's Mute.
        T.pageReady()
        T.openPage()
        T.event()
        T.tripFrom({ action = "toggle_osc_mute", args = { index = 1 }, focused = true }, W)
        T.repaint({ osc = 2, selected = "osc", index = 1, mute = { [1] = true } })
        T.settle()
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.osc_panel.mute_states[1] == true, "the trip read it ON")
        T.event()
        -- Back in Avenger with the page key: the arrival on Mute, and Tab onto it again, say the switch
        -- read once more, off one new picture.
        local snaps = S.snaps
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.run(400)
        T.ok(header.active and S.front == W, "the header again")
        T.ok(T.lastSaid() == "Mute, OSC 1, on, button", "the arrival: " .. T.said())
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, OSC 1, on, button" and S.snaps == snaps + 1, "read again: " .. T.said())
        -- ▶, and the next preset has three oscillators, the second selected.
        T.repaint({ osc = 3, selected = "osc", index = 2 })
        T.ok(T.step(header, 3, "LD Lead 1", "LD Lead 2") == "LD Lead 2", T.said())
        snaps = S.snaps
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, OSC 2, off, button" and S.snaps == snaps + 1, "read again after ▶: " .. T.said())
        -- UNDO, which may take the preset back: the third tab selected now.
        T.repaint({ osc = 3, selected = "osc", index = 3 })
        T.pressAt(header, T.indexOf(header, "Undo"))
        T.ok(T.lastSaid() == "Undo, activated", T.said())
        snaps = S.snaps
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, OSC 3, off, button" and S.snaps == snaps + 1, "read again after Undo: " .. T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The ring's macro tabs: the Macro tab stepper says the tab shown of how many, read with the
/// captions' text, and steps to the next with the page's click; Delete last macro tab opens macro
/// tab 1's menu with the right button and chooses Delete, then says the count read. And one action
/// at a time: a press while Macro button 1's change is still waited for sends nothing.
#[test]
fn the_rings_macro_tabs_and_one_action_at_a_time() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        local function paint(shown, extra)
          local spec = { osc = 1, selected = "osc", index = 1, macroShown = shown, buttons = { false, false } }
          for k, v in pairs(extra or {}) do spec[k] = v end
          T.paintAvenger(98, 84, 1.6, spec)
        end
        paint(1)
        T.words({ ["macro tabs"] = "MACRO 1 MACRO 2", ["macro btn1"] = "Hold" })
        local i = T.indexOf(header, "Macro tab")
        header.focus = i - 1
        T.tab()
        T.ok(string.find(T.lastSaid() or "", "1 of 2", 1, true) ~= nil, T.said())
        T.stepAt(header, i, 1)
        T.ok(T.clickAt() == "1324,782", "tab 2: " .. T.clickAt())
        paint(2)
        T.tick(4)
        T.ok(string.find(T.lastSaid() or "", "2 of 2", 1, true) ~= nil, T.said())
        -- Delete last macro tab, in tab 1's menu.
        T.pressAt(header, T.indexOf(header, "Delete last macro tab"))
        T.ok(T.clickAt() == "1256,782" and S.clicks[#S.clicks].button == "right", "tab 1, right: " .. T.clickAt())
        paint(2, { popup = { 724, 437, 800, 495 } })
        T.tick(3)
        T.ok(T.clickAt() == "1256,858" and S.clicks[#S.clicks].button == nil, "Delete's row: " .. T.clickAt())
        T.words({ ["macro tabs"] = "MACRO 1", ["macro btn1"] = "Hold" })
        paint(1)
        T.tick(6)
        T.ok(T.lastSaid() == "1 macro tab", T.said())
        -- Macro button 1, and Add macro tab pressed while its change is waited for.
        T.pressAt(header, T.indexOf(header, "Macro button 1"))
        T.ok(T.clickAt() == "1316,921", "button 1: " .. T.clickAt())
        local clicks = #S.clicks
        T.pressAt(header, T.indexOf(header, "Add macro tab"))
        T.ok(#S.clicks == clicks and T.count("[avenger] [ring] 'Add macro tab': an action of the ring is still out — this "
          .. "press is not sent") == 1, T.dump())
        -- Said once a press, without cutting in, and in the log.
        local last = S.speech[#S.speech]
        T.ok(last.text == "Add macro tab: still working on the last press" and last.interrupt == false, T.said())
        T.ok(T.count("[avenger] [ring] said: Add macro tab: still working on the last press") == 1, T.dump())
        paint(1, { buttons = { true, false } })
        T.tick(3)
        T.ok(T.lastSaid() == "Hold ON" and #S.clicks == clicks, T.said())
        T.ok(S.speech[#S.speech].interrupt == true and T.count("[avenger] [ring] said: Hold ON") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The Drums and the Trig tab: selected at their tabs and read after — the Drums tab's own switches
/// and the central tab with it — and the Trig tab's Initialize chosen in its menu at its row;
/// Trig's Mute at the tool's point.
#[test]
fn the_drums_and_trig_tabs_are_selected_and_their_menus_and_switches_used() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1 })
        T.words({ ["slot 1"] = "CR Wide" })
        T.tripFrom({ action = "select_drums_tab", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1020,140", "Drums at (576, 35): " .. T.clickAt())
        T.repaint({ osc = 2, selected = "drums", pattern = true, central = "editor" })
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.selected_type == "drums" and o.drum_pattern == true and o.central_panel_tab == "editor",
          tostring(o.selected_type) .. " " .. tostring(o.central_panel_tab))
        -- No slot's name read with it: the page shows the twelve fixed roles (read/drums.luau, THE
        -- NAMES; the maintainer's decision of 2026-10-31).
        local slotRead = false
        for _, r in ipairs(S.recognized) do if r.name == "slot 1" then slotRead = true end end
        T.ok(o.drumkit_part_labels == nil and not slotRead, "no slot's name read")
        T.words({})
        T.event()
        T.tripFrom({ action = "select_trig_tab", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1084,140", "Trig at (616, 35): " .. T.clickAt())
        T.repaint({ osc = 2, selected = "trig" })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.selected_type == "trig", "Trig selected")
        T.event()
        T.tripFrom({ action = "trig_menu_item", args = { item = "Initialize" }, focused = true }, W)
        T.ok(T.clickAt() == "1084,140" and S.clicks[#S.clicks].button == "right", "Trig, right: " .. T.clickAt())
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "trig", popup = { 600, 36, 660, 80 } })
        T.tick(3)
        T.ok(S.clicks[#S.clicks].button == nil and S.clicks[#S.clicks][2] == 194, "Initialize's row: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "trig", trigMute = true })
        T.settle(4)
        T.ok(T.lastPost("error").text == "" and T.lastPost("state").state.osc_panel.trig_mute == true,
          tostring(T.lastPost("error").text))
        T.event()
        T.tripFrom({ action = "toggle_trig_mute", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "1072,126", "Trig's Mute at (609, 26): " .. T.clickAt())
    "##));
}

/// The turns of Luau's loops and calls an action's reads may spend at 200 % on Windows, the largest
/// Avenger: the part read before the click and the one after it (Open.part, the strip, the knobs and
/// the central tab with eight oscillators), and a reading of the drawn menu test over the zoom
/// list's rectangle — 240 by 976 pixels, of which its comparison takes every fourth row — each
/// fewer than the pixels of what it reads, so a loop over them in Luau would pass the bound.
#[test]
fn an_actions_reads_and_the_drawn_test_loop_over_no_pixels_in_luau() {
    run(r##"
        local S = T.S
        S.screenSize = { w = 3840, h = 2400 }
        T.onWindows(200, 1)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 4, 200))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 4, { osc = 8, selected = "osc", index = 5, volume = 50, transpose = 0, fine = 0 })
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
        S.focusLands = function() S.front, S.chain = T.WFX, { S.controls[2], T.WFX } end
        T.pageMsg({ action = "select_osc_tab", args = { index = 6 }, focused = true })
        T.land()
        T.runDue()
        T.repaint({ osc = 8, selected = "osc", index = 6, volume = 50, transpose = 0, fine = 0 })
        T.tick(3)
        Open.part = part
        T.ok(#spent == 2 and T.lastPost("state").state.osc_panel.selected_index == 6, "two reads: " .. #spent)
        -- Measured 2026-10-17: 20,072 turns each, of a part 1640 by 1000 pixels at 200 %; 2026-10-24,
        -- with every pixel of the strip's dots and the knobs' pointers counted, 67,597.
        for _, n in ipairs(spent) do
          T.ok(n < 75000, string.format("a part read spent %d turns: %s", n, table.concat(spent, ", ")))
        end
        -- The drawn test's readings of the zoom list's rectangle, at 200 %: the turns of a tick that
        -- answers one, the item still waiting.
        T.event()
        T.tripFrom({ action = "set_zoom", args = { percent = 100 }, focused = true }, T.WFX)
        T.tick(1)
        local t0 = T.interrupts()
        T.tick(1)
        local tick = T.interrupts() - t0
        T.ok(T.count("[menu] 'VPS Avenger': drawn: armed on 'Zoom' in ") == 1, T.dump())
        -- Measured 2026-10-17: 4,667 turns.
        T.ok(tick < 8000, string.format("a tick with a drawn reading spent %d turns", tick))
    "##);
}

// ---------------------------------------------------------------------------------------------
// The oscillators counted (the second Mac test of the page, 2026-10-24).
// ---------------------------------------------------------------------------------------------

/// The dots' colours as Avenger draws them, measured on the second Mac test's pictures
/// (`OSC Tabs/Mutesolo/…`): a playing oscillator's green, a silent one's pink — muted, or silenced
/// by another tab's Solo — and a soloed one's orange; and the dots themselves, 4 points across from
/// the tab's left, `step` points apart.
const DOTS: &str = r##"
T.PLAYING, T.SILENT, T.SOLO = { 179, 253, 224 }, { 248, 138, 175 }, { 246, 192, 151 }
function T.dotsAt(step, n, colours)
  local out = {}
  for i = 1, n do out[i] = { 244 + (i - 1) * step, 4, colours and colours[i] or T.PLAYING } end
  return out
end
"##;

/// Eight oscillators at 80 % — the dots 38.3 points apart, as on the pictures — with OSC 3 muted,
/// its dot pink: eight counted by their spacing, where the project's green alone left a gap (the
/// second Mac test of the page, 2026-10-24: one Mute and no oscillator could be read). OSC 3's Solo
/// is clicked at the eight tabs' place, (346, 26) — not the seven's (353.5, 26). Seven pink and one
/// orange — OSC 5 soloed — are eight as well; two dots 147 points apart are two.
#[test]
fn the_dots_count_the_oscillators_in_every_colour_by_their_spacing() {
    run_mac(&format!("{OPEN}\n{DOTS}\n{}", r##"
        local _, _, W = T.pageOn({ osc = 8, selected = "osc", index = 1, mute = { [3] = true },
          dots = T.dotsAt(38.3, 8, { [3] = T.SILENT }) })
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.count == 8 and o.count_from == nil and o.mute_states[3] == true, "eight, read: " .. tostring(o.count))
        T.ok(T.count("8 oscillators by their spacing") == 1 and T.count("tab 3 silent") == 1, T.dump())
        -- (346, 26) at 1.6 from (98, 84): 651.6 -> 652, 125.6 -> 126.
        T.tripFrom({ action = "toggle_osc_solo", args = { index = 3 }, focused = true }, W)
        T.ok(T.clickAt() == "652,126", "OSC 3's Solo at the eight tabs' place: " .. T.clickAt())
        T.settle(16)
        -- Seven silent and OSC 5's orange.
        local silent = {}
        for i = 1, 8 do silent[i] = i == 5 and T.SOLO or T.SILENT end
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 8, selected = "osc", index = 1, dots = T.dotsAt(38.3, 8, silent) })
        T.tripFrom({ action = "select_osc_tab", args = { index = 8 }, focused = true }, W)
        T.ok(T.count("tab 5 solo") == 1 and T.count("tab 8 silent") >= 1, T.dump())
        T.repaint({ osc = 8, selected = "osc", index = 8, dots = T.dotsAt(38.3, 8, silent) })
        T.settle()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.count == 8 and o.selected_index == 8, "eight under a Solo: " .. tostring(o.count))
        -- Two, 147 apart.
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 2, dots = T.dotsAt(147, 2) })
        T.tripFrom({ action = "select_osc_tab", args = { index = 1 }, focused = true }, W)
        T.repaint({ osc = 2, selected = "osc", index = 1, dots = T.dotsAt(147, 2) })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.count == 2 and T.count("2 oscillators by their spacing") >= 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// What the dots do not settle is not read, and nothing is clicked by it: eight dots 41 points apart
/// — the seven tabs' step — put the eighth past the seventh tab; seven dots 41 apart with the seventh
/// tab's name read "OSC 8" are a count short.
#[test]
fn dots_that_fit_no_count_and_a_count_its_names_contradict_are_not_taken() {
    run_mac(&format!("{OPEN}\n{DOTS}\n{}", r##"
        local _, _, W = T.pageOn({ osc = 7, selected = "osc", index = 1, dots = T.dotsAt(41, 8) })
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.count == nil, "no count: " .. tostring(o.count))
        T.ok(T.count("the dot at x 951.0 lands at tab 7.97 of 7, 41 apart") == 1, T.dump())
        local clicks = #S.clicks
        T.tripFrom({ action = "toggle_osc_mute", args = { index = 3 }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "An oscillator's Mute: the oscillator tabs cannot be counted now",
          T.lastPost("error").text)
        -- Seven, the seventh tab's name OSC 8.
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 7, selected = "osc", index = 1, dots = T.dotsAt(41, 7) })
        T.words({ ["osc 6"] = "SC 6 OS", ["osc 7"] = "OSC 8" })
        T.openPage()
        T.event()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.count == nil and o.names == nil and o.mute_states == nil, "a count short, not taken")
        T.ok(T.count("a tab's name reads OSC 8 — the dots' count of 7 is short, and not taken") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// More of what the dots do not settle, and what they settle despite it: two dots 200 points apart —
/// a gap within a quarter of no count's step, 146 and 293 the nearest — give no count; a part the
/// band's right edge cuts is no dot, and the three dots 97 apart beside it are three. Where the dots
/// give none, the preset database's count is taken only for a name it places by its words: a name it
/// places by similarity alone is placed, and counts nothing.
#[test]
fn a_gap_no_step_fits_a_part_the_band_cuts_and_a_name_placed_by_similarity_count_nothing() {
    run_mac(&format!("{OPEN}\n{DOTS}\n{}", r##"
        T.presets()
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1, dots = T.dotsAt(200, 2) })
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.count == nil and T.count("their middle gap of 200.0 fits no count's step (the nearest 146, 2 tabs')") == 1,
          tostring(o.count) .. " " .. T.dump())
        local function again(spec, words)
          S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
          T.pageKey()
          T.event()
          T.paintAvenger(98, 84, 1.6, spec)
          T.words(words or {})
          T.openPage()
          T.event()
          return T.lastPost("state").state
        end
        local dots = T.dotsAt(97, 3)
        dots[4] = { 546, 4, T.PLAYING }
        o = again({ osc = 3, selected = "osc", index = 1, dots = dots }).osc_panel
        T.ok(o.count == 3 and o.count_from == nil and T.count("— cut by the band's edge") == 1,
          "three: " .. tostring(o.count) .. " " .. T.dump())
        local st = again({ osc = 3, selected = "osc", index = 1, dots = {} }, { preset = "AR Band Pass Slida" })
        T.ok(st.expansion == "Alpha" and st.osc_panel.count == nil, "placed by similarity, not counted: "
          .. tostring(st.expansion) .. " " .. tostring(st.osc_panel.count))
        T.ok(T.count("by the preset database") == 0, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A part below the screen is said as that, with the way back (read/open.luau, Open.offScreen) —
/// the second Mac test's window reached below its 1080-point screen, and those parts read "not
/// read": Avenger at 80 % on a screen 700 tall, the macros from y 764 down. The ring's Macro 1 says
/// it, Macro button 1's press says it and clicks nothing, and so does the page's action; the page's
/// key names the parts in its error region. The strip, on the screen, reads as ever.
#[test]
fn a_part_below_the_screen_is_said_as_that_with_the_way_back() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("LD Lead 1")
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1, macroShown = 1, buttons = { false, false } })
        T.words({})
        S.screenSize = { w = 2560, h = 700 }
        local BELOW = "the macros are below the screen; move the plug-in's window up"
        local i = T.indexOf(header, "Macro 1")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Macro 1, " .. BELOW .. ", button", T.said())
        local clicks = #S.clicks
        T.pressAt(header, T.indexOf(header, "Macro button 1"))
        T.tick(2)
        T.ok(#S.clicks == clicks and T.lastSaid() == "Macro button 1: " .. BELOW, T.said())
        i = T.indexOf(header, "Mute")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Mute, OSC 1, off, button", "the strip on the screen: " .. T.said())
        -- The page: its key, and an action on the macros.
        T.pageReady()
        T.openPage()
        T.ok(T.lastPost("error").text == "The central panel and the macros are below the screen; move the plug-in's window "
          .. "up. What lies past the edge was not read.", T.lastPost("error").text)
        T.event()
        T.tripFrom({ action = "toggle_macro_button", args = { index = 1 }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and string.find(T.lastPost("error").text, ": " .. BELOW, 1, true) ~= nil,
          T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The ring's reads place the name last read in the database only while the preset cannot have
/// changed since: with the dots not drawn, the strip's count is the database's for the name read;
/// after Undo — the preset may be another, nothing read since — the read made again places no name,
/// and counts nothing from the database (the second Mac test placed "Init Preset" after a load).
#[test]
fn the_rings_reads_place_no_name_after_the_preset_may_have_changed() {
    run_mac(&format!("{OPEN}\n{DOTS}\n{}", r##"
        T.presets()
        local header = T.avengerAt80("AR Band Pass Slide")
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, dots = {} })
        T.words({})
        local i = T.indexOf(header, "Oscillator")
        header.focus = i - 1
        T.tab()
        T.ok(T.count("[avenger] [read] strip: 3 oscillators, by the preset database") == 1, T.dump())
        T.pressAt(header, T.indexOf(header, "Undo"))
        T.tick(3)
        local snaps = S.snaps
        header.focus = i - 1
        T.tab()
        T.ok(S.snaps > snaps, "read again")
        T.ok(T.count("[avenger] [read] strip: 3 oscillators, by the preset database") == 1, "no name placed: " .. T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Where the dots give no count: the preset database's, for a preset it places by its words as they
/// stand, which the page says as the count alone — and the count the page's Confirm OSC Count says,
/// kept for the preset read then until a name read differs, marked as confirmed; where the dots give
/// one, theirs is taken and the page is told so. Confirm OSC Count clicks nothing, and is refused
/// before a name was read.
#[test]
fn a_count_from_the_database_or_confirmed_fills_in_where_the_dots_give_none() {
    run_mac(&format!("{OPEN}\n{DOTS}\n{}", r##"
        T.presets()
        local NO_DOTS = { osc = 3, selected = "osc", index = 1, dots = {} }
        local _, _, W = T.pageOn(NO_DOTS, { preset = "AR Band Pass Slide" })
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.count == 3 and o.count_from == "database", "the database's three: " .. tostring(o.count))
        T.ok(T.count("[avenger] [read] strip: 3 oscillators, by the preset database") == 1, T.dump())
        -- A preset the database does not have, and Confirm OSC Count.
        local function again(words, spec)
          S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
          T.pageKey()
          T.event()
          T.paintAvenger(98, 84, 1.6, spec or NO_DOTS)
          T.words(words)
          T.openPage()
          T.event()
          return T.lastPost("state").state.osc_panel
        end
        o = again({ preset = "My Own Lead" })
        T.ok(o.count == nil, "not counted")
        local clicks = #S.clicks
        T.event()
        T.pageMsg({ action = "confirm_osc_count", args = { count = 4 }, focused = true })
        o = T.lastPost("state").state.osc_panel
        T.ok(#S.clicks == clicks and o.count == 4 and o.count_from == "confirmed", "confirmed: " .. tostring(o.count))
        o = again({ preset = "My Own Lead" })
        T.ok(o.count == 4 and o.count_from == "confirmed", "kept for its preset: " .. tostring(o.count))
        -- The dots read three: theirs.
        o = again({ preset = "My Own Lead" }, { osc = 3, selected = "osc", index = 1 })
        T.ok(o.count == 3 and o.count_from == nil
          and T.count("the dots read 3 oscillators, 4 as confirmed for this preset — the dots' count is taken") == 1, T.dump())
        T.event()
        T.pageMsg({ action = "confirm_osc_count", args = { count = 4 }, focused = true })
        T.ok(T.lastPost("state").state.osc_panel.count == 3
          and T.lastPost("error").text == "Confirm OSC Count: the dots read 3 oscillators, and that count is taken",
          tostring(T.lastPost("error").text))
        -- Another name read: forgotten.
        o = again({ preset = "Another Lead" })
        T.ok(o.count == nil, "forgotten with the name")
        -- No name read: refused.
        o = again({})
        T.event()
        T.pageMsg({ action = "confirm_osc_count", args = { count = 2 }, focused = true })
        T.ok(T.lastPost().text == "Confirm OSC Count: the preset's name has not been read, and the count is kept by it",
          tostring(T.lastPost().text))
        T.pageMsg({ action = "confirm_osc_count", args = { count = 9 }, focused = true })
        T.ok(T.lastPost().text == "Confirm OSC Count: no count from 1 to 8 was sent", tostring(T.lastPost().text))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// On Windows Avenger is a child window of the DAW's, the overlay's origin: the page keeps its
/// values by the DAW's window in front, and so does the count confirmed — a trip's read and the next
/// key find it there, and OSC 3's Mute is clicked at the four tabs' place. Read by the child's own
/// window, the count confirmed was found nowhere.
#[test]
fn on_windows_a_confirmed_count_is_found_by_the_window_the_page_keeps() {
    run(&format!("{OPEN}\n{}", r##"
        local j = T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        -- OSC 1's dot, and one that fits no count's step: the dots give no count, and the strip is not
        -- covered (read/strip.luau, A COVERED STRIP: a strip with no dot at OSC 1's place is).
        local GREEN = { 100, 220, 180 }
        local NO_DOTS = { osc = 4, selected = "osc", index = 1, dots = { { 246, 3, GREEN }, { 266, 3, GREEN } } }
        T.paintAvenger(248, 60, 2.4, NO_DOTS)
        T.words({ preset = "My Own Lead" })
        T.openPage()
        T.event()
        T.ok(T.lastPost("state").state.osc_panel.count == nil, "not counted")
        T.ok(j.id ~= T.WFX.id, "the origin is the child")
        T.pageMsg({ action = "confirm_osc_count", args = { count = 4 }, focused = true })
        T.ok(T.lastPost("state").state.osc_panel.count == 4, "confirmed")
        -- A trip's read: the count confirmed, OSC 3's Mute at the four tabs' place.
        local ST = T.source("modules/vps-avenger/src/tables/strip.luau")(T.host)
        local x = math.floor(248 + 2.4 * (ST.tabX(3, 4) + ST.MUTE_X_OFFSET) + 0.5)
        local y = math.floor(60 + 2.4 * ST.MUTE_Y + 0.5)
        S.focusAtOnce = false
        S.focusLands = function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageMsg({ action = "toggle_osc_mute", args = { index = 3 }, focused = true })
        T.land()
        T.runDue()
        T.ok(T.clickAt() == x .. "," .. y, "OSC 3's Mute at the four tabs' place: " .. T.clickAt() .. " " .. T.dump())
        T.ok(T.count("strip: 4 oscillators, as confirmed for this preset") >= 1, T.dump())
        T.repaint({ osc = 4, selected = "osc", index = 1, dots = NO_DOTS.dots, mute = { [3] = true } })
        T.tick(3)
        T.ok(T.lastPost("state").state.osc_panel.mute_states[3] == true, "read ON")
        -- The next key: the count kept.
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageKey()
        T.event()
        T.paintAvenger(248, 60, 2.4, NO_DOTS)
        T.openPage()
        T.event()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.count == 4 and o.count_from == "confirmed", "kept for its preset: " .. tostring(o.count))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// VPS Avenger 0.16: the macros' names and count, the database first (the maintainer's decision of
// 2026-10-31, D3), and the reads nobody needs dropped (mt-verified, findings 12 and 13).
// ---------------------------------------------------------------------------------------------

/// A macro button's toggle reads no text (mt-verified, finding 13: six regions and 350 ms a toggle
/// in the developer's Mac test, for a name the action had): the button's highlight read off the
/// picture, and its name the one last read in this window.
#[test]
fn a_macro_button_toggle_reads_no_text() {
    run_mac(&scenario(r##"
        local words = { ["macro tabs"] = "MACRO 1", ["macro btn1"] = "Hold" }
        local _, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { false, false } }, words)
        local reads = #S.recognized
        T.tripFrom({ action = "toggle_macro_button", args = { index = 1 }, focused = true }, W)
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1, buttons = { true, false } })
        T.settle()
        for i = reads + 1, #S.recognized do
          T.ok(string.sub(S.recognized[i].name or "", 1, 5) ~= "macro", "no macro region read: " .. tostring(S.recognized[i].name))
        end
        T.ok(T.lastPost("error").text == "" and T.count("[avenger] [act] 'A macro button': changed — Hold ON") == 1, T.dump())
    "##));
}

/// A macro field's dot is no part of its name (mt-verified, finding 12): a first word "•", "·", "o",
/// "O", "0" or "°" whose box ends left of 7 units from the field's left — the pink dot, at 3.75 to
/// 6.25 units in the developer's Mac test's macros picture at 80 % — is dropped by its place, the
/// region uncut; Windows' 'o Macr02' is Avenger's default "Macro 2". A zero further in is a name's
/// ("0 dB"), and a reading whose boxes PaddleOCR only guessed loses only a lone bullet word.
#[test]
fn a_macro_fields_dot_is_no_part_of_its_name() {
    run_mac(&scenario(r##"
        local function reading(list)
          local words, text = {}, {}
          for _, w in ipairs(list) do
            words[#words + 1] = { text = w[1], x = w[2], y = w[3], w = w[4], h = 16, approx = w[5] }
            text[#text + 1] = w[1]
          end
          return { status = "text", text = table.concat(text, " "), words = words, lines = {} }
        end
        -- The fields at 80 % from Avenger's corner (98, 84): macro1 from x 1236, macro2 1317, macro3
        -- 1399, btn1 1276, btn2 1358; the dot's box ending by x1 + 11.2.
        local words = {
          ["macro tabs"] = "MACRO 1",
          ["macro macro1"] = reading({ { "•", 1242, 795, 4 }, { "Add", 1260, 795, 20 }, { "Lead", 1284, 795, 24 } }),
          ["macro macro2"] = reading({ { "o", 1323, 795, 5 }, { "Macr02", 1340, 795, 36 } }),
          ["macro macro3"] = reading({ { "0", 1429, 795, 6 }, { "dB", 1440, 795, 12 } }),
          ["macro btn1"] = reading({ { "•", 1282, 894, 4 }, { "Soften", 1286, 894, 30 }, { "To", 1320, 894, 10 },
            { "Pad", 1333, 894, 16 } }),
          ["macro btn2"] = reading({ { "o", 1358, 894, 20, true }, { "Gate", 1378, 894, 40, true } }),
        }
        T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 1 }, words)
        local m = T.lastPost("state").state.macro_panel
        T.ok(table.concat(m.tabs[1], "|") == "Add Lead|Macro 2|0 dB|Soften To Pad|Gate", table.concat(m.tabs[1], "|"))
        T.ok(T.count("macro1 '• Add Lead' ('•' before it dropped)") == 1
          and T.count("macro2 'o Macr02' ('o' before it dropped) -> 'Macro 2', Avenger's default") == 1, T.dump())
    "##));
}

/// Avenger's own default macro names are the vocabulary where the database has none — Init Preset
/// here — as the project names them (its _current_macro_label: "Macro 1" … "MacroBtn 2" on tab 1),
/// compared with what was read with Cyrillic and Greek look-alike letters folded (mt-verified,
/// finding 12): Vision's "Масrо 3" (Cyrillic М, а and о) is "Macro 3", said in Latin letters.
#[test]
fn avengers_default_macro_names_are_taken_through_look_alike_letters() {
    run_mac(&scenario(r##"
        local words = { ["macro tabs"] = "MACRO 1", ["macro macro1"] = "Macro 1", ["macro macro2"] = "Macтo 2",
          ["macro macro3"] = "Масrо 3", ["macro btn1"] = "MacroBtn 1", ["macro btn2"] = "MаcrоBtn 2" }
        T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 1 }, words)
        local m = T.lastPost("state").state.macro_panel
        T.ok(table.concat(m.tabs[1], "|") == "Macro 1|Macro 2|Macro 3|MacroBtn 1|MacroBtn 2", table.concat(m.tabs[1], "|"))
        local R = S.included["modules/vps-avenger/src/read/ocr.luau"][1]
        T.ok(R.foldConfusables("Масrо 3") == "Macro 3" and R.foldConfusables("Bass") == "Bass"
          and R.foldConfusables("ΑΒΓ") == "ABΓ", R.foldConfusables("Масrо 3"))
        T.ok(R.defaultMacroName(2, 1) == "Macro 4" and R.defaultMacroName(2, 5) == "MacroBtn 4", "tab 2's defaults")
    "##));
}

/// The macro tabs' count is the preset database's where it has the preset (mt-verified, finding 12;
/// avw2-verified, R5): the second Windows session read the caption "UACRO 1" all through, and with
/// AR Deep Arp's two tabs in the database the page shows two, tab 2 shown, its names the database's
/// — the read of 'o Macro 4' its check, its dot dropped, agreeing. Without the database the caption
/// counts with one letter off (A2): "UACRO 1" is one tab.
#[test]
fn the_macro_tab_count_comes_from_the_database_when_the_caption_reads_uacro() {
    run_mac(&scenario(r##"
        T.presets()
        local function reading(list)
          local words, text = {}, {}
          for _, w in ipairs(list) do
            words[#words + 1] = { text = w[1], x = w[2], y = 795, w = w[3], h = 16 }
            text[#text + 1] = w[1]
          end
          return { status = "text", text = table.concat(text, " "), words = words, lines = {} }
        end
        T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 2 }, { preset = "AR Deep Arp",
          ["macro tabs"] = "UACRO 1", ["macro macro1"] = reading({ { "o", 1242, 5 }, { "Macro", 1260, 30 }, { "4", 1294, 6 } }),
          ["macro macro2"] = "Wobble" })
        local m = T.lastPost("state").state.macro_panel
        T.ok(m.count == 2 and m.selected_index == 2, "two tabs, the database's: " .. tostring(m.count))
        T.ok(table.concat(m.tabs[2], "|") == "Macro 4|Wobble|Macro 6|MacroBtn 3|MacroBtn 4", table.concat(m.tabs[2], "|"))
        T.ok(T.count("tabs 'UACRO 1' — 1 tab; 2 tabs, the database's, where the caption reads 1 — the database's taken")
          == 1, T.dump())
        local R = S.included["modules/vps-avenger/src/read/ocr.luau"][1]
        for _, c in ipairs({ { "UACRO 1", 1 }, { "MACRO 1 +", 1 }, { "UACRO 1 MACRO 2", 2 }, { "MACR0 1", 1 }, { "MACRO", 1 },
          { "Partial", nil }, { "", nil }, { "MACRO 1  MACRO 2 Macro 3", 3 }, { "macro i", 1 } }) do
          T.ok(R.macroTabCount(c[1]) == c[2], c[1] .. " -> " .. tostring(R.macroTabCount(c[1])))
        end
    "##));
    run_mac(&scenario(r##"
        T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 1 }, { ["macro tabs"] = "UACRO 1" })
        local m = T.lastPost("state").state.macro_panel
        T.ok(m.count == 1 and m.selected_index == 1, "one tab, read: " .. tostring(m.count))
    "##));
}

/// THE DATABASE FIRST (the maintainer's decision of 2026-10-31, D3): where the database has the
/// preset, the OSC tabs' and the macros' names are its own, the screen read for them at the page key
/// only, as a check — a read not near the database's name marks that name, "from the database" —
/// and the ring's and the actions' reads read no name: the ring's Macro 1 says the page key's
/// checked name, the Oscillator stepper the database's tab. An Add Macro Tab counts the tabs off
/// their captions, and once it has changed the count the database's names are not taken: the next
/// page key reads them.
#[test]
fn the_databases_names_come_first_and_the_page_key_checks_them() {
    run_mac(&scenario(r##"
        T.presets()
        local header, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1, macroShown = 1, amounts = { 0, 0, 0 } },
          { preset = "AR Deep Arp", ["osc 1"] = "One Osc", ["macro tabs"] = "MACRO 1 MACRO 2", ["macro macro1"] = "Cutoff",
            ["macro macro2"] = "Res" })
        local st = T.lastPost("state").state
        T.ok(st.osc_panel.names[1] == "One Osc" and st.macro_panel.tabs[1][1] == "Cut, from the database"
          and st.macro_panel.tabs[1][2] == "Res" and st.macro_panel.tabs[1][3] == "Env", T.jsonEncode(st.macro_panel))
        T.ok(T.count("macro1 'Cutoff', not the database's 'Cut' (0.67 alike) — the database's, marked") == 1, T.dump())
        -- Back in Avenger: the ring reads no field, and says the checked names.
        T.event()
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.run(400)
        T.ok(header.active, "the header again")
        local reads = #S.recognized
        header.focus = T.indexOf(header, "Macro 1") - 1
        T.tab()
        T.ok(T.lastSaid() == "Macro 1, Cut, from the database, 0 percent, button", T.said())
        header.focus = T.indexOf(header, "Oscillator") - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, One Osc, slider", T.said())
        for i = reads + 1, #S.recognized do
          local n = S.recognized[i].name or ""
          T.ok(string.sub(n, 1, 5) ~= "macro" and string.sub(n, 1, 3) ~= "osc", "no name read: " .. n)
        end
        -- Add Macro Tab from the page: the captions read before and after, the count changed, and
        -- the database's names not taken from then on.
        T.pageReady()
        T.openPage()
        T.event()
        T.tripFrom({ action = "add_macro_tab", args = {}, focused = true }, W)
        T.words({ preset = "AR Deep Arp", ["macro tabs"] = "MACRO 1 MACRO 2 MACRO 3", ["macro macro1"] = "Cutoff" })
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 3, amounts = { 0, 0, 0 } })
        T.settle()
        T.ok(T.lastPost("state").state.macro_panel.count == 3, tostring(T.lastPost("state").state.macro_panel.count))
        -- Back in Avenger, and the page key again: the names read off the screen.
        T.event()
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.run(400)
        T.ok(header.active and S.front == W, "the header again")
        T.repaint({ osc = 1, selected = "osc", index = 1, macroShown = 1, amounts = { 0, 0, 0 } })
        T.openPage()
        T.tick()
        st = T.lastPost("state").state
        T.ok(st.macro_panel.count == 3 and st.macro_panel.tabs[1][1] == "Cutoff", T.jsonEncode(st.macro_panel))
    "##));
}

// ---------------------------------------------------------------------------------------------
// The strip under the level meter and under a menu (the developer's Mac test of 2026-10-31,
// mt-verified findings 4 and 11; the second Windows session of that day, avw2-verified A8).
// ---------------------------------------------------------------------------------------------

/// The strip's log line of the selection: the floor and the mean of every tab, by its label, and
/// whether the means — the rule the floor replaced — would have decided one.
const FLOORS: &str = r##"
function T.floors()
  local line = nil
  for _, l in ipairs(S.logs) do
    if string.find(l, "the tabs' bottom strips' floor (their means)", 1, true) then line = l end
  end
  assert(line, "no selection line")
  local list = string.match(line, "darkest selected: (.-) — ")
  local out, means = {}, {}
  for part in string.gmatch(list, "[^,]+") do
    local label, f, m = string.match(part, "^%s*(.-) ([%d%.]+) %(([%d%.]+)%)$")
    out[label] = { floor = tonumber(f), mean = tonumber(m) }
    means[#means + 1] = tonumber(m)
  end
  table.sort(means)
  local lead, spread = means[2] - means[1], means[#means] - means[2]
  return out, lead >= 5 and lead >= spread, line
end
"##;

/// The level meter: a light bar one row tall under a sounding tab, in the band the selection is
/// read in (read/strip.luau, THE SELECTED TAB), lifted a tab's mean by up to 28 in the developer's Mac
/// test of 2026-10-31 and left the selection undecided — or, by the project's darkest-wins, wrong.
/// The selection is read by the band's FLOOR, each column's darkest pixel, which the meter does not
/// move. With the shades measured there (the selected tab 34, the others 52): (a) Drums selected and
/// lit, its mean 62; (b) OSC 1 and Drums lit, that log's 17228; (c) the second Windows session's 579 —
/// one oscillator, selected (44) and lit, Drums and Trig at 58; (d) no tab dark, two lit: none. In (a)
/// to (c) the means, logged beside the floors, would have refused.
#[test]
fn the_level_meter_under_a_tab_moves_no_selection() {
    run_mac(&format!("{OPEN}\n{FLOORS}\n{}", r##"
        local _, _, W = T.pageOn({ osc = 2, strips = { 52, 52, drums = 34, trig = 52 }, meters = { drums = 26 } })
        local function again(spec)
          S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
          T.pageKey()
          T.event()
          T.paintAvenger(98, 84, 1.6, spec)
          T.openPage()
          T.event()
          return T.lastPost("state").state.osc_panel
        end
        -- (a)
        local o = T.lastPost("state").state.osc_panel
        local f, byMean, line = T.floors()
        T.ok(o.selected_type == "drums" and f.Drums.floor == 34 and f.Drums.mean > 55 and not byMean, line)
        T.ok(T.count("— Drums, by 18.0, the others within 0.0") == 1, T.dump())
        -- (b)
        o = again({ osc = 2, strips = { 52, 52, drums = 34, trig = 52 }, meters = { [1] = 26, drums = 26 } })
        f, byMean, line = T.floors()
        T.ok(o.selected_type == "drums" and f["OSC 1"].mean > 58 and f["OSC 1"].floor == 52 and not byMean, line)
        -- (c)
        o = again({ osc = 1, strips = { 44, drums = 58, trig = 58 }, meters = { [1] = 60 } })
        f, byMean, line = T.floors()
        T.ok(o.selected_type == "osc" and o.selected_index == 1 and f["OSC 1"].floor == 44 and not byMean, line)
        -- (d): the last read that decided one had OSC 1, so no Drums section is kept.
        o = again({ osc = 2, strips = { 52, 52, drums = 52, trig = 52 }, meters = { [1] = 26, drums = 26 } })
        f, byMean, line = T.floors()
        T.ok(o.selected_type == nil and o.drums_kept == nil and T.count("— none: ") == 1, line)
    "##));
}

/// A strip under Avenger's MENU popup, as the developer's Mac test of 2026-10-31 had it at every page
/// key with the popup left open (mt-verified, finding 4A): OSC 1's dot under the popup, the strip's
/// text holding the menu's items. Covered — read/strip.luau, A COVERED STRIP: no selection, no Mute
/// or Solo, no Drums switch, no name; the strip's text not read at all, the tab never named
/// "Load preset.. OSC 1 Save"; the page told (`strip_covered`) and its Drums section kept from the
/// last read that decided Drums (`drums_kept`), every value of it not read.
#[test]
fn a_strip_under_avengers_menu_is_not_read_and_not_kept() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "drums", pattern = true },
          { strip = "Load preset.. OSC 1 + DRUMS TRIG Save" })
        T.ok(T.lastPost("state").state.osc_panel.selected_type == "drums", "Drums read first")
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "drums", pattern = true, popup = { 240, 26, 297.5, 75 } })
        local reads = #S.recognized
        T.openPage()
        T.event()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.strip_covered == true and o.selected_type == nil and o.drum_pattern == nil and o.mute_states == nil
          and o.names == nil and o.drums_kept == true, T.jsonEncode(o))
        T.ok(T.count("[avenger] [read] strip: covered — no dot at OSC 1's place; nothing of the strip is read") == 1, T.dump())
        for i = reads + 1, #S.recognized do
          local n = S.recognized[i].name or ""
          T.ok(n ~= "strip" and string.sub(n, 1, 4) ~= "osc ", "nothing of the strip read as text: " .. n)
        end
        T.ok(T.count("the tabs read whole") == 0, T.dump())
        -- What the page shows of it.
        local js = T.repoText("modules/vps-avenger/ui/panel.js")
        T.ok(string.find(js, "strip_covered: s.osc_panel.strip_covered === true", 1, true) ~= nil
          and string.find(js, "if (oscPanel.selected_type === 'drums' || drumsKept) {", 1, true) ~= nil, "panel.js")
        -- The menu's items' words in the text of a strip whose dots were read: covered all the same.
        local G = { 100, 220, 180 }
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "drums", dots = { { 246, 3, G }, { 266, 3, G } } })
        T.openPage()
        T.event()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.strip_covered == true and o.selected_type == nil and o.names == nil
          and T.count("strip: covered — its text holds MENU's item 'load preset'") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The ring on a covered strip: its Oscillator stepper says the strip covered, and every PRESS reads
/// it again — a new picture each — where until 0.16 one covered read answered four presses with "the
/// oscillator tabs cannot be counted now" and no picture (mt-verified, findings 4A and 4C). The
/// announcement says what that read found. Uncovered, the next press steps; a click whose read before
/// it is covered clicks nothing.
#[test]
fn the_rings_strip_under_a_menu_is_read_again_at_each_press() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        T.words({})
        local COVERED = { osc = 2, selected = "osc", index = 1, popup = { 240, 26, 297.5, 75 } }
        T.paintAvenger(98, 84, 1.6, COVERED)
        local osc = T.indexOf(header, "Oscillator")
        header.focus = osc - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, the oscillator strip is covered now, perhaps by a menu of VPS Avenger's, slider",
          T.said())
        local snaps, clicks = S.snaps, #S.clicks
        T.stepAt(header, osc, 1)
        T.ok(T.lastSaid() == "Oscillator: the oscillator strip is covered now, perhaps by a menu of VPS Avenger's"
          and S.snaps == snaps + 1, "a new picture at the press: " .. (S.snaps - snaps) .. " " .. T.said())
        T.stepAt(header, osc, 1)
        T.ok(S.snaps == snaps + 2 and #S.clicks == clicks, "and at the next")
        -- Mute pressed on it: nothing clicked.
        T.pressAt(header, T.indexOf(header, "Mute"))
        T.ok(#S.clicks == clicks and string.find(T.lastSaid() or "", "covered now", 1, true) ~= nil, T.said())
        -- The menu gone: the next press reads it and steps.
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1 })
        T.stepAt(header, osc, 1)
        T.ok(#S.clicks == clicks + 1, "OSC 2 clicked: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 2 })
        T.tick(3)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// OSC 2's and OSC 3's dots seen and OSC 1's missing: the spacing alone would count three, and the
/// strip is covered all the same — OSC 1 is always there, its dot drawn in one of three colours in
/// every read of both logs that found dots (read/strip.luau, A COVERED STRIP). No tab selected, and a
/// click on it refused. A menu that counts open over the overlay covers it too (O:menuOpen).
#[test]
fn a_missing_first_dot_with_the_others_seen_is_covered() {
    run_mac(&scenario(r##"
        local G = { 100, 220, 180 }
        local w = (536 - 244) / 3
        local SPEC = { osc = 3, selected = "osc", index = 2, dots = { { 244 + w + 2, 3, G }, { 244 + 2 * w + 2, 3, G } } }
        local _, _, W = T.pageOn(SPEC)
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.strip_covered == true and o.selected_type == nil and o.count == nil, T.jsonEncode(o))
        T.ok(T.count("3 oscillators by their spacing") == 1 and T.count("strip: covered — no dot at OSC 1's place") == 1,
          T.dump())
        local clicks = #S.clicks
        T.tripFrom({ action = "toggle_osc_mute", args = { index = 2 }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "An oscillator's Mute: the oscillator strip is covered now, "
          .. "perhaps by a menu of VPS Avenger's; nothing was clicked", T.lastPost("error").text)
        -- A menu counting open over the overlay (O:menuOpen) covers it, whatever its dots; dots not read
        -- at all are no cover: then the strip is not counted.
        local Strip = S.included["modules/vps-avenger/src/read/strip.luau"][1]
        local Open = S.included["modules/vps-avenger/src/read/open.luau"][1]
        local P = S.included["modules/vps-avenger/src/read/picture.luau"][1]
        local pl = P.placer({ client = { x = 98, y = 84, w = 1394, h = 898 } }, 1.6, 80)
        local first = { mid = 98 + 1.6 * 247.25 }
        T.ok(Strip.coverOf(pl, { first }, true, true) == "a menu counts open over the overlay"
          and Strip.coverOf(pl, { first }, true, false) == nil and Strip.coverOf(pl, {}, false, false) == nil
          and Strip.coverOf(pl, {}, true, false) == "no dot at OSC 1's place", "the cover's rule")
        T.ok(Open.menuOpen({ menuOpen = function() return true end }) == true and Open.menuOpen(nil) == false
          and Open.menuOpen({ menuOpen = function() error("gone") end }) == false, "O:menuOpen asked, guarded")
    "##));
}

/// The OSC tabs' names are read from 32 to 38.3 units (tables/strip.luau, NAME_TOP and NAME_BOTTOM):
/// clear of the tab's coloured rule above them (30.6 to 31.9) and of the level meter below them
/// (38.55 to 39.15), every glyph row in (33.75 to 37.95, the descender of "Arp") — a pixel taken by
/// its middle, as the picture draws it — at the factors 1, 1.6, 1.589 (the second Windows session's),
/// 2 and 4, with Avenger's corner placed up to half a pixel either way (mt-verified, finding 11). The
/// project's 28 to 42 held both; 38.75, as first proposed, takes the meter's row at 1 and at 1.6.
#[test]
fn the_osc_name_regions_leave_out_the_tabs_rule_and_the_meter() {
    run_mac(&scenario(r##"
        local ST = T.source("modules/vps-avenger/src/tables/strip.luau")(T.host)
        T.ok(ST.NAME_TOP == 32 and ST.NAME_BOTTOM == 38.3, "the bounds")
        local function check(top, bottom)
          local bad = {}
          for _, k in ipairs({ 1, 1.6, 1.589, 2, 4 }) do
            for _, d in ipairs({ -0.5, -0.25, 0, 0.25, 0.5 }) do
              local cy = 120 + d
              local y1, y2 = math.floor(cy + k * top + 0.5), math.floor(cy + k * bottom + 0.5)
              local function rows(a, b)
                local out = {}
                for j = 100, 400 do
                  if j + 0.5 >= cy + k * a and j + 0.5 < cy + k * b then out[#out + 1] = j end
                end
                return out
              end
              for _, j in ipairs(rows(33.75, 37.95)) do
                if j < y1 or j >= y2 then bad[#bad + 1] = string.format("%g/%g: glyph row %d out", k, d, j) end
              end
              for _, span in ipairs({ { 30.6, 31.9, "rule" }, { 38.55, 39.15, "meter" } }) do
                for _, j in ipairs(rows(span[1], span[2])) do
                  if j >= y1 and j < y2 then bad[#bad + 1] = string.format("%g/%g: %s row %d in", k, d, span[3], j) end
                end
              end
            end
          end
          return bad
        end
        local bad = check(ST.NAME_TOP, ST.NAME_BOTTOM)
        T.ok(#bad == 0, table.concat(bad, "; "))
        T.ok(#check(28, 42) > 0 and #check(32, 38.75) > 0, "the old and the first proposed bounds fail it")
    "##));
}

/// A Mute or a Solo changes no tab's selection, and its read after the click may decide none — the
/// level meter lights with the un-muted oscillator's sound: that read's selection is not posted
/// (src/act.luau, `leaves`), so the ring's tab is not wiped and the next Mute still clicks and is said
/// by its tab (avw2-verified, X-F7 and A8).
#[test]
fn a_mute_whose_read_after_decides_no_tab_leaves_the_selected_tab_alone() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        T.words({})
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1 })
        local ST = T.source("modules/vps-avenger/src/tables/strip.luau")(T.host)
        local at = math.floor(98 + 1.6 * (ST.tabX(1, 2) + ST.MUTE_X_OFFSET) + 0.5) .. "," .. math.floor(84 + 1.6 * ST.MUTE_Y + 0.5)
        local mute = T.indexOf(header, "Mute")
        header.focus = mute - 1
        T.tab()
        T.pressAt(header, mute)
        T.ok(T.clickAt() == at, "OSC 1's Mute: " .. T.clickAt())
        -- After it, no tab stands out: every strip alike.
        T.repaint({ osc = 2, strips = { 52, 52, drums = 52, trig = 52 }, mute = { [1] = true } })
        T.tick(3)
        T.ok(T.lastSaid() == "OSC 1 Mute ON", T.said())
        -- The ring's tab kept: Mute announced by it, from what was read.
        header.focus = mute - 1
        T.tab()
        T.ok(string.find(T.lastSaid() or "", "OSC 1, on", 1, true) ~= nil, T.said())
        local clicks = #S.clicks
        T.repaint({ osc = 2, selected = "osc", index = 1, mute = { [1] = true } })
        T.pressAt(header, mute)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == at, "clicked again: " .. T.clickAt())
        T.repaint({ osc = 2, selected = "osc", index = 1 })
        T.tick(3)
        T.ok(T.lastSaid() == "OSC 1 Mute OFF", T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A choice of tab whose read after the click decides no tab is looked on from within the bound, as a
/// drum action's is: the tab read once one decides, and at the bound the read before kept and said as
/// not read — never a change with the selection posted as nil, which hid the page's Drums section
/// (mt-verified, finding 4C). Another tab reading selected than the one clicked is said so.
#[test]
fn a_tab_click_whose_read_after_decides_nothing_is_looked_on_from() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1 })
        T.tripFrom({ action = "select_drums_tab", args = {}, focused = true }, W)
        T.repaint({ osc = 2, strips = { 52, 52, drums = 52, trig = 52 } })
        T.tick(2)
        T.ok(T.count("what it shows does not hold") >= 1, T.dump())
        T.repaint({ osc = 2, selected = "drums" })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.selected_type == "drums"
          and T.count("[avenger] [act] 'The Drums tab': changed — Drums selected") == 1, T.dump())
        -- Never decided by the bound.
        T.event()
        T.tripFrom({ action = "select_trig_tab", args = {}, focused = true }, W)
        T.repaint({ osc = 2, strips = { 52, 52, drums = 52, trig = 52 } })
        T.tick(16)
        T.ok(T.lastPost("error").text == "The Trig tab: the selected tab cannot be read now"
          and T.lastPost("state").state.osc_panel.selected_type == "drums", T.lastPost("error").text)
        -- OSC 2 reads selected after Trig's click.
        T.event()
        T.tripFrom({ action = "select_trig_tab", args = {}, focused = true }, W)
        T.repaint({ osc = 2, selected = "osc", index = 2 })
        T.settle()
        T.ok(T.lastPost("error").text == "The Trig tab: Trig was clicked; OSC 2 reads selected", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}
