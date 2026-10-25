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

/// MENU's Load preset from the ring, in a popup painted from (242, 20) — left of 245, the old edge
/// of the area it is looked for in, where the second Mac test found it cut — and left drawn after
/// the click, as Avenger's popup stays while its file dialog opens: found whole, and its row clicked
/// ONCE (`retries = 0`), never again into the dialog, frozen or drawn again; that test clicked it
/// four times. Save preset as alike. Initialize preset, which opens no dialog, is followed: a popup
/// frozen as at its click is the click pending (overlay runtime 0.13), not clicked again; drawn
/// again — the pointer over its row — and still open, it is clicked again.
#[test]
fn menus_file_items_are_clicked_once_in_a_popup_found_from_240() {
    run_mac(&scenario(r##"
        local header = T.avengerAt80("LD Lead 1")
        local base = { osc = 1, selected = "osc", index = 1 }
        T.paintAvenger(98, 84, 1.6, base)
        -- The clicks after the item's: while the popup stays frozen, and once it is drawn again.
        local function choose(label)
          T.pressAt(header, T.indexOf(header, label))
          T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 242, 20, 322, 70 } })
          T.tick(3)
          -- 242 is 485.2 on screen, 485: 241.9 from Avenger's corner.
          T.ok(T.count("[avenger] '" .. label .. "': MENU's popup at (241.9,20.0) 80.0x50.0 from Avenger's corner") == 1,
            T.dump())
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
        local frozen, again = choose("Load preset")
        T.ok(frozen == 0 and again == 0 and T.clickAt() == "549,135", "Load once, at its row: " .. T.clickAt())
        T.ok(T.count("'Load preset' — test 'drawn' sees the menu at 485,116 128x80") == 1
          and T.count("choosing item (64,19) at (549,135)") == 1, T.dump())
        frozen, again = choose("Save preset as")
        T.ok(frozen == 0 and again == 0, "Save as once: " .. frozen .. " " .. again)
        frozen, again = choose("Initialize preset")
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
        local function load()
          T.pressAt(header, T.indexOf(header, "Load preset"))
          T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 242, 20, 322, 70 } })
          T.tick(3)
          T.ok(T.clickAt() == "549,135", "Load's row: " .. T.clickAt())
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

/// MENU's Load from the ring, its popup painted below MENU: chosen at Load's row in the middle of
/// the area seen; and the redo list from the page, which holds the trip for as long as it is seen
/// and lets it go on once it has gone — then the name.
#[test]
fn menu_from_the_ring_and_the_redo_list_from_the_page_in_popups_painted_inside_avenger() {
    run_mac(&scenario(r##"
        local header, _, W = T.avengerAt80("LD Lead 1")
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1 })
        T.pressAt(header, 4)
        T.ok(T.clickAt() == "517,106" and T.lastSaid() == "Load preset, activated", T.clickAt())
        T.paintAvenger(98, 84, 1.6, { osc = 1, selected = "osc", index = 1, popup = { 250, 20, 330, 70 } })
        T.tick(3)
        T.ok(T.count("[avenger] 'Load preset': MENU's popup at (250.0,20.0) 80.0x50.0 from Avenger's corner") == 1, T.dump())
        T.ok(T.clickAt() == "562,135", "Load's row: " .. T.clickAt())
        T.repaint({ osc = 1, selected = "osc", index = 1 })
        T.tick(3)
        -- Chosen at its click, not followed: the item opens a file dialog (main.luau, ONE_CLICK).
        T.ok(T.count("choosing item (64,19) at (562,135)") == 1, T.dump())
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
        -- No dots: no count, nothing clicked.
        T.event()
        T.paintAvenger(98, 84, 1.6, {})
        clicks = #S.clicks
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

/// An oscillator's knob reset: a double-click — both clicks with the right button for Pan — and
/// the knob read after it; never while Drums is selected, which has no oscillator controls.
#[test]
fn a_knob_is_reset_with_a_double_click_and_never_on_the_drums_tab() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1, volume = 150, transpose = 0, fine = 0 })
        T.tripFrom({ action = "reset_osc_control", args = { control = "volume" }, focused = true }, W)
        T.ok(T.clickAt() == "652,193" and #S.changeWaits == 0, "the first click: " .. T.clickAt())
        local n = #S.clicks
        T.tick(1)
        T.ok(#S.clicks == n + 1 and T.clickAt() == "652,193" and #S.changeWaits == 1, "the second, then the wait")
        T.repaint({ osc = 2, selected = "osc", index = 1, volume = 100, transpose = 0, fine = 0 })
        T.settle()
        T.ok(T.lastPost("state").state.osc_panel.volume_values[1] == 100 and T.lastPost().id == "osc-reset-volume",
          tostring(T.lastPost("state").state.osc_panel.volume_values[1]))
        T.event()
        T.tripFrom({ action = "reset_osc_control", args = { control = "pan" }, focused = true }, W)
        T.tick(1)
        T.ok(S.clicks[#S.clicks].button == "right" and S.clicks[#S.clicks - 1].button == "right", "Pan: the right button")
        T.tick(16)
        T.ok(T.lastPost("error").text == "Resetting an oscillator's control: unchanged, Pan 0", T.lastPost("error").text)
        T.event()
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "drums" })
        local clicks = #S.clicks
        T.tripFrom({ action = "reset_osc_control", args = { control = "volume" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text
          == "Resetting an oscillator's control: the Drums tab is selected; it has no oscillator controls", T.lastPost("error").text)
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
        T.ok(T.lastSaid() == "Volume 100 percent", T.said())
        -- Reset pan: the right button.
        clicks = #S.clicks
        T.pressAt(header, T.indexOf(header, "Reset pan"))
        T.tick(1)
        T.ok(#S.clicks == clicks + 2 and S.clicks[#S.clicks].button == "right" and S.clicks[#S.clicks - 1].button == "right",
          "Pan with the right button")
        T.tick(16)
        T.ok(T.lastSaid() == "Pan 0, unchanged", T.said())
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
        -- The overlay's other keys work on.
        T.tab()
        T.ok(header.focus == level + 1 and T.lastSaid() == "Reset level, button", "Tab moves on: " .. T.said())
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
/// taken, where the darkest of equals was the first; with one drawn selected, that one. A picture
/// taken while Avenger redraws, as the second Mac test's: Trig the darkest by 7, over the others
/// 21 apart — none.
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
        -- The slots' names read with it, which the page shows.
        local labels = o.drumkit_part_labels
        T.ok(labels and (labels[1] or labels["1"]) == "CR Wide", "the names read: " .. T.jsonEncode(labels or {}))
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
/// stand — marked so — and the count the page's Confirm OSC Count says, kept for the preset read
/// then until a name read differs, marked as confirmed; where the dots give one, theirs is taken and
/// the page is told so. Confirm OSC Count clicks nothing, and is refused before a name was read.
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
        local NO_DOTS = { osc = 4, selected = "osc", index = 1, dots = {} }
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
        T.repaint({ osc = 4, selected = "osc", index = 1, dots = {}, mute = { [3] = true } })
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
