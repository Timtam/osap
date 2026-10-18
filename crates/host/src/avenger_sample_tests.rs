//! VPS Avenger's sample editor from the page (the plan's slice 8, its other half:
//! `modules/vps-avenger/src/actions/sample.luau`, `src/read/sample.luau`, its hand-overs in
//! `src/handover.luau` and its knobs in `src/amount.luau`), loaded for real against the scripted host
//! of `overlay_avenger_tests.rs`, whose painted Avenger now has the editor's tabs (`sampleTab`), the
//! tab's switch (`sampleOn`) and its fields' boxes (`fields`).
//!
//! What the plan says is pinned here. Nothing is clicked in the sample editor unless the read before
//! shows the Drums tab, EDITOR as the central tab, the editor in its place — not the sample browser —
//! and the control's own tab: Gain and Range share a point, Play Mode lies beside Amp's Strength. The
//! tab and the switch are read after their click, the switch never flipped from memory; the menus
//! open with the LEFT button on their field, their popup painted below it, and a popup left open is
//! closed by clicking its field again, never with Escape; their field is read before and after as
//! text, Loop-Stretch's slot's Loop with it. The envelope's Back and Next read its name; its Menu and
//! Preset, a typed field and a browser row hand the keyboard to Avenger and read what it shows after.
//! The knobs are pressed and held through the keys, their reset answered by the change wait. No
//! picture is held at the end of any of it.

use super::avenger_drums_tests::OPEN;
use super::overlay_avenger_tests::{run, run_mac};

/// The sample editor painted, and what the scenarios here share.
const SAMPLE: &str = r##"
-- The Drums tab, EDITOR and the editor's `tab` shown, slot 3 selected, with `extra`'s fields over it.
function T.sample(tab, extra)
  local s = T.drums({ sampleTab = tab or "sample", slots = { selected = 3 } })
  for k, v in pairs(extra or {}) do s[k] = v end
  return s
end
-- A trip refused before anything was clicked: the focus `focus` posted first and `said` beside it in
-- the error region, or with no focus the error region taking it.
function T.refused(said, focus, posts)
  local p = T.page().posts
  if focus then
    return #p > posts + 1 and p[#p - 1].kind == "focus" and p[#p - 1].id == focus and p[#p].kind == "error"
      and p[#p].text == said and p[#p].focus ~= true
  end
  return #p > posts and p[#p].kind == "error" and p[#p].text == said and p[#p].focus == true
end
-- Back to Avenger from the page with the page key, the overlay up in its stay again.
function T.back(W)
  S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
  T.pageKey()
  T.event()
end
-- Whether a text read asked for region `name` from entry `from` of S.recognized on.
function T.asked(name, from)
  for i = (from or 0) + 1, #S.recognized do
    if S.recognized[i].name == name then return true end
  end
  return false
end
-- A menu opened with the left button and chosen: the opener clicked, `popup` painted under it, the
-- item clicked, the popup gone and `after` painted, `words` read from then on.
function T.leftMenu(m, W, popup, opener, after, words)
  T.tripFrom(m, W)
  local c = S.clicks[#S.clicks]
  T.ok(T.clickAt() == opener and c.button ~= "right", "the opener, not with the right button: " .. T.clickAt())
  local spec = table.clone(S.painted.spec)
  spec.popup = popup
  T.paintAvenger(98, 84, 1.6, spec)
  T.tick(3)
  local item = T.clickAt()
  if words then T.words(words) end
  T.repaint(after)
  T.settle(5)
  return item
end
"##;

fn scenario(s: &str) -> String {
    format!("{OPEN}\n{SAMPLE}\n{s}")
}

// ---------------------------------------------------------------------------------------------
// What the page key reads.
// ---------------------------------------------------------------------------------------------

/// The page key on the Drums tab with EDITOR: the editor's tab by its dark bottom strip, the view,
/// and Amp's switch for the slot read as selected — the other tabs' lists left out, no browser read.
/// No tab read and the browser's rows in the words: the sample browser, its rows asked. On an
/// oscillator tab none of the editor's keys, and nothing of it asked.
#[test]
fn the_page_key_reads_the_sample_editor_on_the_drums_tab_with_editor() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("amp", { sampleOn = true }))
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.sample_editor_tab == "amp" and o.sample_editor_view == "editor" and o.sample_editor_amp_states[3] == true
          and o.sample_editor_amp_states[1] == nil and o.sample_editor_pitch_states == nil and o.sample_editor_eq_states == nil,
          tostring(o.sample_editor_tab) .. " " .. tostring(o.sample_editor_view))
        T.ok(not T.asked("browser"), "no browser read where a tab reads")
        T.ok(T.count("[avenger] [read] editor: the tabs' bottom strips ") == 1
          and T.count("[avenger] [read] editor: Amp's switch ") == 1, T.dump())
        T.ok(T.count("— amp") == 1, T.dump())
        -- The browser.
        T.back(W)
        T.paintAvenger(98, 84, 1.6, T.drums({ slots = { selected = 3 } }))
        T.words({ browser = "Crash Open Hi-Hat Ride" })
        local from = #S.recognized
        T.openPage()
        T.event()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.sample_editor_view == "browser" and o.sample_editor_tab == nil and T.asked("browser", from),
          tostring(o.sample_editor_view))
        T.ok(T.count("[avenger] [read] editor: no tab reads shown; the browser's rows 'Crash Open Hi-Hat Ride' — the "
          .. "sample browser") == 1, T.dump())
        -- An oscillator tab.
        T.back(W)
        T.paintAvenger(98, 84, 1.6, { osc = 2, selected = "osc", index = 1, central = "editor", sampleTab = "amp" })
        from = #S.recognized
        T.openPage()
        T.event()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.sample_editor_tab == nil and o.sample_editor_view == nil and o.sample_editor_amp_states == nil
          and not T.asked("browser", from), "nothing of the editor on OSC 1")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Slot 12 selected with Amp ON, and only slots 1, 2 and 12 named in the words: JSON cannot carry
/// lists so sparse, so the state keys them by the slot's number as text — Amp's switch at "12", the
/// three names at theirs — while every slot's Mute, all read, stays a list; the page opens with it,
/// and the state is written as the host writes it. A message page:post refuses is logged and the
/// page is shown all the same.
#[test]
fn slot_twelve_and_a_few_names_read_are_posted_keyed_by_the_slot() {
    run_mac(&scenario(r##"
        local words = { ["slot 1"] = "CR Wide", ["slot 2"] = "HH Open", ["slot 12"] = "BD 909" }
        local _, _, W = T.pageOn(T.drums({ sampleTab = "amp", sampleOn = true, slots = { selected = 12 } }), words)
        T.ok(T.page().shown and #T.page().shows == 1, "the page shown")
        local st = T.lastPost("state").state
        local o = st.osc_panel
        T.ok(o.drumkit_part_selected == "12" and o.sample_editor_amp_states["12"] == true
          and o.sample_editor_amp_states[12] == nil, "Amp's switch keyed by the slot: " .. T.jsonEncode(o.sample_editor_amp_states))
        T.ok(o.drumkit_part_labels["12"] == "BD 909" and o.drumkit_part_labels["1"] == "CR Wide"
          and o.drumkit_part_labels["3"] == nil and o.drumkit_part_labels[1] == nil, T.jsonEncode(o.drumkit_part_labels))
        T.ok(o.drumkit_mute_states[12] == false and o.drumkit_mute_states[1] == false, "every Mute read: a list")
        local text = T.jsonEncode(st)
        T.ok(string.find(text, '"sample_editor_amp_states":{"12":true}', 1, true) ~= nil
          and string.find(text, '"drumkit_part_labels":{"1":"CR Wide","12":"BD 909","2":"HH Open"}', 1, true) ~= nil, text)
        -- Slot 3 with every name read, Amp ON: lists again.
        T.back(W)
        T.paintAvenger(98, 84, 1.6, T.drums({ sampleTab = "amp", sampleOn = true, slots = { selected = 3 } }))
        T.words(T.slotWords("KIT"))
        T.openPage()
        T.event()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.sample_editor_amp_states[3] == true and o.drumkit_part_labels[12] == "KIT 12", "lists where JSON has them")
        -- A message page:post refuses.
        T.back(W)
        S.postsMayFail = true
        S.postRaises = "a value JSON cannot hold"
        T.openPage()
        T.event()
        T.ok(T.page().shown and #T.page().shows == 3, "the page shown all the same: " .. #T.page().shows)
        T.ok(T.count("[avenger] [page] a 'state' message could not be posted: page:post: a value JSON cannot hold") == 1,
          T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The editor's switch is the slot's it was read for. Amp read ON with slot 3 selected; slot 5 then
/// chosen, the table read again but not the editor: nothing of Amp's switch is posted — never slot
/// 3's ON shown as slot 5's. ARP and then EDITOR chosen as the central tab, whose read sees the
/// editor but not the table: the switch read then is the slot's read selected last, 5's.
#[test]
fn the_editors_switch_is_posted_only_for_the_slot_it_was_read_for() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("amp", { sampleOn = true }))
        T.ok(T.lastPost("state").state.osc_panel.sample_editor_amp_states[3] == true, "slot 3's Amp ON")
        T.tripFrom({ action = "select_drumkit_part", args = { index = 5 }, focused = true }, W)
        T.repaint(T.drums({ sampleTab = "amp", slots = { selected = 5 } }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_part_selected == "5" and o.sample_editor_amp_states == nil,
          "nothing of the switch for slot 5: " .. T.jsonEncode(o.sample_editor_amp_states or {}))
        T.event()
        T.tripFrom({ action = "select_central_tab", args = { tab = "arp" }, focused = true }, W)
        T.repaint(T.drums({ central = "arp", slots = { selected = 5 } }))
        T.settle()
        T.event()
        T.tripFrom({ action = "select_central_tab", args = { tab = "editor" }, focused = true }, W)
        T.repaint(T.drums({ sampleTab = "amp", sampleOn = true, slots = { selected = 5 } }))
        T.settle()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.sample_editor_tab == "amp" and o.sample_editor_amp_states[5] == true and o.sample_editor_amp_states[3] == nil,
          "the central tab's read, for slot 5: " .. T.jsonEncode(o.sample_editor_amp_states or {}))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// Where nothing is clicked.
// ---------------------------------------------------------------------------------------------

/// Every sample editor action — its trips, its hand-overs and its knob — reads first and clicks
/// nothing: with an oscillator tab selected (the focus on the Drums tab), with DRUM SQ as the
/// central tab (on EDITOR), with the sample browser shown (on its first row), and where the editor
/// cannot be read. No click, no press, nothing handed over. A browser row mirrored: refused in the
/// editor and with no slot read as selected.
#[test]
fn a_sample_editor_action_away_from_the_editor_clicks_nothing() {
    run_mac(&scenario(r##"
        local actions = {
          { "select_sample_editor_tab", { tab = "amp" } },
          { "toggle_sample_editor_enable", { tab = "amp" } },
          { "sample_editor_play_mode", { item = "polyphone" } },
          { "sample_editor_grid", { item = "1/8" } },
          { "sample_editor_eq_type", { band = 1, item = "peak" } },
          { "sample_editor_loop_stretch", { item = "turntable" } },
          { "sample_editor_panel_button", { button = "next" } },
          { "sample_editor_panel_button", { button = "menu" } },
          { "sample_editor_field", { field = "start" } },
          { "sample_editor_control", { control = "volume" } },
          { "reset_sample_editor_control", { control = "volume" } },
        }
        local _, _, W = T.pageOn({ osc = 2, selected = "osc", index = 1, central = "editor", sampleTab = "sample" })
        local LABELS = S.included["modules/vps-avenger/src/page.luau"][1].LABELS
        local function all(said, focus, words)
          for _, a in ipairs(actions) do
            T.event()
            if words then T.words(words) end
            local clicks, downs, posts = #S.clicks, #T.inputs("down"), #T.page().posts
            T.tripFrom({ action = a[1], args = a[2], focused = true }, W)
            T.settle(2)
            T.ok(#S.clicks == clicks and #T.inputs("down") == downs and #T.inputs("drag") == 0,
              a[1] .. ": nothing clicked or pressed")
            T.ok(T.refused(LABELS[a[1]] .. ": " .. said, focus, posts), a[1] .. ": " .. tostring(T.lastPost().text)
              .. " " .. tostring(T.lastPost().id))
            T.ok(S.front == T.PAGE_WIN and not S.holding["Return"], a[1] .. ": the page back, nothing handed over")
          end
        end
        all("the Drums tab is not selected; nothing was clicked", "drums-tab")
        T.paintAvenger(98, 84, 1.6, T.drums({ central = "drum_sq", sq = { chain = true, selected = "a" } }))
        all("EDITOR is not the central tab; nothing was clicked", "central-tab-editor")
        T.paintAvenger(98, 84, 1.6, T.drums({ slots = { selected = 3 } }))
        all("the sample browser is shown, not the sample editor; nothing was clicked", "sample-browser-item-1",
          { browser = "Crash Open Hi-Hat Ride" })
        all("the sample editor cannot be read now", nil, {})
        -- A browser row: refused in the editor, and with no slot read as selected.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.sample("sample"))
        local clicks, posts = #S.clicks, #T.page().posts
        T.tripFrom({ action = "sample_browser_focus", args = { index = 3, name = "Ride" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.refused(LABELS.sample_browser_focus .. ": the sample editor is shown, not the sample "
          .. "browser; nothing was clicked", "sample-editor-tab-sample", posts), tostring(T.lastPost().text))
        T.event()
        T.paintAvenger(98, 84, 1.6, T.drums({ slots = { lighter = { 3, 9 } } }))
        T.words({ browser = "Crash Open Hi-Hat Ride" })
        posts = #T.page().posts
        T.tripFrom({ action = "sample_browser_focus", args = { index = 3, name = "Ride" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.refused(LABELS.sample_browser_focus .. ": the selected slot cannot be read now; "
          .. "nothing was clicked", nil, posts) and not S.holding["Return"], tostring(T.lastPost().text))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The tabs and the switches.
// ---------------------------------------------------------------------------------------------

/// Amp at (300, 260) — (578, 500) — clicked on the Sample tab and read selected after, its switch
/// read with it for slot 3; Pitch clicked and EQ read instead: EQ said, the mismatch noted; EQ again
/// with nothing drawn: unchanged at the bound; a click after which no tab reads: "cannot be read now".
#[test]
fn an_editor_tab_is_clicked_and_its_switch_read_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("sample"))
        T.tripFrom({ action = "select_sample_editor_tab", args = { tab = "amp" }, focused = true }, W)
        T.ok(T.clickAt() == "578,500" and S.clicks[#S.clicks].button == nil, "Amp: " .. T.clickAt())
        T.repaint(T.sample("amp", { sampleOn = true }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.sample_editor_tab == "amp" and o.sample_editor_amp_states[3] == true and T.lastPost().id == "sample-editor-tab-amp"
          and T.lastPost("error").text == "", tostring(o.sample_editor_tab))
        T.ok(T.count("[avenger] [act] 'Choosing a sample editor tab': changed — Amp selected") == 1, T.dump())
        T.event()
        T.tripFrom({ action = "select_sample_editor_tab", args = { tab = "pitch" }, focused = true }, W)
        T.ok(T.clickAt() == "636,500", "Pitch: " .. T.clickAt())
        T.repaint(T.sample("eq"))
        T.settle()
        T.ok(T.lastPost("error").text == "Choosing a sample editor tab: Pitch was clicked; EQ reads selected"
          and T.lastPost("state").state.osc_panel.sample_editor_tab == "eq", T.lastPost("error").text)
        T.ok(T.lastPost("state").state.osc_panel.sample_editor_eq_states[3] == false, "EQ's switch read off")
        T.event()
        T.tripFrom({ action = "select_sample_editor_tab", args = { tab = "eq" }, focused = true }, W)
        T.ok(T.clickAt() == "1072,500", "EQ: " .. T.clickAt())
        T.tick(16)
        T.ok(T.lastPost("error").text == "Choosing a sample editor tab: unchanged, EQ is selected", T.lastPost("error").text)
        T.event()
        T.tripFrom({ action = "select_sample_editor_tab", args = { tab = "sample" }, focused = true }, W)
        T.repaint(T.drums({ slots = { selected = 3 } }))
        T.settle()
        T.ok(T.lastPost("error").text == "Choosing a sample editor tab: the sample editor's tab cannot be read now",
          T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Amp's switch at (267, 371) — (525, 678) — clicked on the Amp tab and read ON after; EQ's at
/// (258, 370) — (511, 676) — on EQ. Pitch's asked while Amp is shown: refused, nothing clicked, the
/// focus on the Pitch tab — the two share a point. Sample has no switch: refused with no trip.
#[test]
fn the_switch_is_clicked_only_on_its_own_tab() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("amp"))
        T.tripFrom({ action = "toggle_sample_editor_enable", args = { tab = "amp" }, focused = true }, W)
        T.ok(T.clickAt() == "525,678", "Amp's switch: " .. T.clickAt())
        T.repaint(T.sample("amp", { sampleOn = true }))
        T.settle()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.sample_editor_amp_states[3] == true and T.lastPost().id == "sample-editor-amp-enable", tostring(T.lastPost().id))
        T.ok(T.count("[avenger] [act] 'Turning the envelope or the EQ on or off': changed — Amp ON") == 1, T.dump())
        -- Clicked again, nothing drawn: unchanged, never flipped.
        T.event()
        T.tripFrom({ action = "toggle_sample_editor_enable", args = { tab = "amp" }, focused = true }, W)
        T.tick(16)
        T.ok(T.lastPost("error").text == "Turning the envelope or the EQ on or off: unchanged, Amp ON", T.lastPost("error").text)
        -- Pitch's while Amp is shown.
        T.event()
        local clicks, posts = #S.clicks, #T.page().posts
        T.tripFrom({ action = "toggle_sample_editor_enable", args = { tab = "pitch" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.refused("Turning the envelope or the EQ on or off: the Pitch tab is not shown; nothing "
          .. "was clicked", "sample-editor-tab-pitch", posts), tostring(T.lastPost().text))
        -- EQ's.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.sample("eq"))
        T.tripFrom({ action = "toggle_sample_editor_enable", args = { tab = "eq" }, focused = true }, W)
        T.ok(T.clickAt() == "511,676", "EQ's switch: " .. T.clickAt())
        T.repaint(T.sample("eq", { sampleOn = true }))
        T.settle()
        o = T.lastPost("state").state.osc_panel
        T.ok(o.sample_editor_eq_states[3] == true and T.lastPost().id == "sample-editor-eq-enable", tostring(T.lastPost().id))
        T.event()
        clicks = #S.clicks
        local from = #S.pageOrder
        T.pageMsg({ action = "toggle_sample_editor_enable", args = { tab = "sample" }, focused = true })
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Turning the envelope or the EQ on or off: sample has no "
          .. "on/off switch in the sample editor" and not string.find(T.pageSince(from), "focus", 1, true), T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The menus.
// ---------------------------------------------------------------------------------------------

/// The editor's four menus, each opened with the LEFT button on its field, its popup painted below
/// the field — inside the drawn test's region below its opener — and its item clicked at the tool's
/// row, the field read before and after as text: Play Mode Polyphone at (965, 679) and (984, 713),
/// said; Grid 1/8 at (612, 679) and (629, 714) with the field reading 1/8 already and redrawn by a
/// hover: unchanged; an EQ band's Low-Shelf at (784, 676) and (805, 710) read as Peak after: the
/// mismatch noted; band 2 at (1077, 676) and (1098, 710) with no word read and the field redrawn:
/// "redrew"; and with nothing redrawn: "nothing changed". Loop-Stretch Turntable at (517, 721) and
/// (613, 754): the field and the slot's Loop read after, never written from the item — Stretch with
/// the Loop read off after it is said with it off, and Turntable again with only the Loop changed is
/// a change.
#[test]
fn the_editors_menus_open_with_the_left_button_and_read_their_field() {
    run_mac(&scenario(r##"
        S.honourMinPixels = true
        local _, _, W = T.pageOn(T.sample("sample"), { ["play mode"] = "Monophone" })
        local item = T.leftMenu({ action = "sample_editor_play_mode", args = { item = "polyphone" }, focused = true }, W,
          { 540, 378, 610, 402 }, "965,679", T.sample("sample", { fields = { play_mode = 110 } }), { ["play mode"] = "Polyphone" })
        T.ok(item == "984,713", "Polyphone's row: " .. item)
        T.ok(T.count("[menu] 'VPS Avenger': drawn: armed on 'Play Mode' in 869,687-1189,935 ") == 1, T.dump())
        T.ok(T.count("[avenger] [act] 'Play Mode': changed — Play Mode Polyphone") == 1
          and T.lastPost().id == "sample-editor-sample-play-mode" and T.lastPost("error").text == "", T.dump())
        T.ok(T.count("[avenger] [read] editor: the fields play_mode 'Polyphone' -> polyphone") >= 1, T.dump())
        -- Grid on Amp, reading 1/8 before and after, its field redrawn — a hover, no change of what
        -- it reads: looked on from that picture, and unchanged at the bound.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.sample("amp"))
        T.words({ grid = "1/8" })
        item = T.leftMenu({ action = "sample_editor_grid", args = { item = "1/8" }, focused = true }, W,
          { 321, 378, 380, 450 }, "612,679", T.sample("amp", { fields = { grid = 90 } }))
        T.ok(item == "629,714", "1/8's row: " .. item)
        T.tick(16)
        T.ok(T.lastPost("error").text == "Grid: unchanged, Grid reads 1/8", T.lastPost("error").text)
        -- EQ band 1, read as Peak after.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.sample("eq"))
        T.words({ ["eq1 type"] = "Peak" })
        item = T.leftMenu({ action = "sample_editor_eq_type", args = { band = 1, item = "low_shelf" }, focused = true }, W,
          { 429, 376, 500, 432 }, "784,676", T.sample("eq", { fields = { eq1_type = 100 } }), { ["eq1 type"] = "Peak" })
        T.ok(item == "805,710", "band 1's Low-Shelf: " .. item)
        T.tick(16)
        T.ok(T.lastPost("error").text == "An EQ band's type: Low-Shelf chosen; Band 1 Type reads Peak", T.lastPost("error").text)
        -- Band 2, no word read, the field redrawn.
        T.event()
        T.words({})
        item = T.leftMenu({ action = "sample_editor_eq_type", args = { band = 2, item = "low_shelf" }, focused = true }, W,
          { 612, 376, 690, 432 }, "1077,676", T.sample("eq", { fields = { eq2_type = 120 } }))
        T.ok(item == "1098,710", "band 2's Low-Shelf: " .. item)
        T.ok(T.count("[avenger] [act] 'An EQ band's type': changed — Low-Shelf chosen; VPS Avenger redrew Band 2 Type") == 1,
          T.dump())
        -- And with nothing redrawn.
        T.event()
        item = T.leftMenu({ action = "sample_editor_eq_type", args = { band = 2, item = "peak" }, focused = true }, W,
          { 612, 376, 690, 432 }, "1077,676", T.sample("eq", { fields = { eq2_type = 120 } }))
        T.tick(16)
        T.ok(T.lastPost("error").text == "An EQ band's type: Peak chosen; nothing VPS Avenger shows changed",
          T.lastPost("error").text)
        -- Loop-Stretch: the field and slot 3's Loop read after.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.sample("sample"))
        T.words({ ["loop stretch"] = "Off" })
        item = T.leftMenu({ action = "sample_editor_loop_stretch", args = { item = "turntable" }, focused = true }, W,
          { 300, 404, 380, 438 }, "517,721", T.sample("sample", { slots = { selected = 3, loop = { [3] = true } },
            fields = { loop_stretch = 100 } }), { ["loop stretch"] = "Turntable" })
        T.ok(item == "613,754", "Turntable's row: " .. item)
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.drumkit_loop_states[3] == true and T.lastPost().id == "sample-editor-loop-stretch", tostring(T.lastPost().id))
        T.ok(T.count("[avenger] [act] 'Loop-Stretch': changed — Loop-Stretch Turntable; Ride Loop ON") == 1, T.dump())
        -- Stretch, and Avenger's Loop off after it: said as read, never as the item would have it.
        T.event()
        item = T.leftMenu({ action = "sample_editor_loop_stretch", args = { item = "stretch" }, focused = true }, W,
          { 300, 404, 380, 438 }, "517,721", T.sample("sample", { fields = { loop_stretch = 130 } }),
          { ["loop stretch"] = "Stretch" })
        T.ok(T.count("[avenger] [act] 'Loop-Stretch': changed — Loop-Stretch Stretch; Ride Loop OFF") == 1
          and T.lastPost("state").state.osc_panel.drumkit_loop_states[3] == false, T.dump())
        -- Turntable over Turntable, the field the same and the Loop turned on: the Loop's change is the
        -- change.
        T.event()
        T.words({ ["loop stretch"] = "Turntable" })
        T.paintAvenger(98, 84, 1.6, T.sample("sample", { fields = { loop_stretch = 140 } }))
        T.leftMenu({ action = "sample_editor_loop_stretch", args = { item = "turntable" }, focused = true }, W,
          { 300, 404, 380, 438 }, "517,721", T.sample("sample", { slots = { selected = 3, loop = { [3] = true } },
            fields = { loop_stretch = 140 } }))
        T.ok(T.count("[avenger] [act] 'Loop-Stretch': changed — Loop-Stretch Turntable; Ride Loop ON") == 2
          and T.lastPost("error").text == "", T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The editor's controls on another tab than theirs are refused, nothing clicked, the focus on the
/// tab to choose: Play Mode on Amp (it lies beside Amp's Strength), Grid on Sample and on EQ, an EQ
/// band's type on Pitch, Gain on Pitch and Range on Sample (the two share a point), and the
/// envelope's Speed on Sample.
#[test]
fn the_editors_controls_are_refused_off_their_tab() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("amp"))
        local function refused(tab, m, said, focus)
          T.event()
          T.paintAvenger(98, 84, 1.6, T.sample(tab))
          local clicks, downs, posts = #S.clicks, #T.inputs("down"), #T.page().posts
          T.tripFrom(m, W)
          T.settle(2)
          T.ok(#S.clicks == clicks and #T.inputs("down") == downs and not S.holding["Return"], m.action .. ": nothing clicked")
          T.ok(T.refused(said, focus, posts), m.action .. ": " .. tostring(T.lastPost().text))
        end
        refused("amp", { action = "sample_editor_play_mode", args = { item = "monophone" }, focused = true },
          "Play Mode: the Sample tab is not shown; nothing was clicked", "sample-editor-tab-sample")
        refused("sample", { action = "sample_editor_grid", args = { item = "1/16" }, focused = true },
          "Grid: the Amp or the Pitch tab is not shown; nothing was clicked", "sample-editor-tab-amp")
        refused("eq", { action = "sample_editor_grid", args = { item = "1/16" }, focused = true },
          "Grid: the Amp or the Pitch tab is not shown; nothing was clicked", "sample-editor-tab-amp")
        refused("pitch", { action = "sample_editor_eq_type", args = { band = 1, item = "peak" }, focused = true },
          "An EQ band's type: the EQ tab is not shown; nothing was clicked", "sample-editor-tab-eq")
        refused("pitch", { action = "sample_editor_field", args = { field = "gain" }, focused = true },
          "A sample editor field: the Sample tab is not shown; nothing was clicked", "sample-editor-tab-sample")
        refused("sample", { action = "sample_editor_field", args = { field = "range" }, focused = true },
          "A sample editor field: the Pitch tab is not shown; nothing was clicked", "sample-editor-tab-pitch")
        refused("sample", { action = "sample_editor_field", args = { field = "speed" }, focused = true },
          "A sample editor field: the Amp or the Pitch tab is not shown; nothing was clicked", "sample-editor-tab-amp")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A left opener's popup that never takes its item: once the runtime gives up ("untaken") while the
/// popup still counts open, it is closed by clicking its field again, as the zoom field's is — no
/// Escape goes out.
#[test]
fn a_left_opened_popup_left_open_is_closed_by_clicking_its_opener() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("amp"))
        T.tripFrom({ action = "sample_editor_grid", args = { item = "1/8" }, focused = true }, W)
        T.ok(T.clickAt() == "612,679", "Grid's field: " .. T.clickAt())
        T.paintAvenger(98, 84, 1.6, T.sample("amp", { popup = { 321, 378, 380, 450 } }))
        local sent = #S.sent
        T.run(8400)
        T.ok(T.count("its popup was left open (refused) — clicking its opener again to close it") == 1, T.dump())
        T.ok(T.clickAt() == "612,679" and S.clicks[#S.clicks].button == nil and #S.sent == sent, "the field clicked again, no Escape")
        T.repaint(T.sample("amp"))
        T.settle(4)
        T.ok(T.lastPost("error").text == "Grid: the menu did not take the choice", T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The hand-overs.
// ---------------------------------------------------------------------------------------------

/// Start on the Sample tab: its field double-clicked at (303, 372) — (583, 679) — Avenger's editor
/// seen open over the field's box, the keyboard handed over with Return and Escape let through and
/// nothing said, the field seen closed after Return, and its value READ back: the page back on
/// Start. Escape on End: the value read, noted. Band 1's Frequency on EQ at (576, 676): the page's
/// element, not the tool's; Speed on Pitch: Pitch's own. An editor that never opens: said, nothing
/// handed over.
#[test]
fn a_sample_field_is_typed_into_avenger_and_read_back() {
    run_mac(&scenario(r##"
        local SPEC = T.sample("sample")
        local _, _, W = T.pageOn(SPEC)
        local function typed(field, box, spec, key, value)
          local clicks, said = #S.clicks, #S.speech
          T.tripFrom({ action = "sample_editor_field", args = { field = field }, focused = true }, W)
          T.tick()
          local first, second = T.clickAt(clicks + 1), T.clickAt(clicks + 2)
          local s = table.clone(spec)
          s.editor = box
          T.repaint(s)
          T.tick()
          T.ok(S.holding["Return"] and S.holding["Escape"] and #S.speech == said, field .. ": handed over, nothing said")
          T.words({ ["the field renamed"] = value })
          T.repaint(spec)
          T.ok(T.typed(key) == "passed", key .. " through to Avenger")
          T.settle(4)
          return first, second
        end
        local a, b = typed("start", { 285, 367, 321, 377 }, SPEC, "Return", "12.5 %")
        T.ok(a == "583,679" and b == "583,679", "two clicks on Start: " .. a .. " " .. b)
        T.ok(T.count("[avenger] [rename] 'sample start': Return — read '12.5 %'") == 1 and T.lastPost().id == "sample-editor-sample-start"
          and T.lastPost("error").text == "" and S.front == T.PAGE_WIN, T.dump())
        T.event()
        typed("end", { 339, 367, 375, 377 }, SPEC, "Escape", "3.0 s")
        T.ok(T.lastPost("error").text == "A sample editor field: Escape: End reads 3.0 s", T.lastPost("error").text)
        -- Band 1's Frequency on EQ.
        T.event()
        local EQ = T.sample("eq")
        T.paintAvenger(98, 84, 1.6, EQ)
        a = typed("eq1_freq", { 281, 365, 317, 375 }, EQ, "Return", "1200 Hz")
        T.ok(a == "576,676" and T.lastPost().id == "sample-editor-eq-band-1-freq", a .. " " .. tostring(T.lastPost().id))
        -- Speed on Pitch, at (392, 372) — (725, 679): the focus on Pitch's own Speed.
        T.event()
        local PITCH = T.sample("pitch")
        T.paintAvenger(98, 84, 1.6, PITCH)
        a = typed("speed", { 374, 367, 410, 377 }, PITCH, "Return", "1/8")
        T.ok(a == "725,679" and T.lastPost().id == "sample-editor-pitch-speed", a .. " " .. tostring(T.lastPost().id))
        T.paintAvenger(98, 84, 1.6, EQ)
        -- No editor opens.
        T.event()
        T.tripFrom({ action = "sample_editor_field", args = { field = "eq1_gain" }, focused = true }, W)
        T.tick()
        T.run(2400)
        T.settle(2)
        T.ok(T.lastPost("error").text == "A sample editor field: VPS Avenger's editor did not open; nothing was renamed"
          and not S.holding["Return"], T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The envelope's Next at (585, 261) — (1034, 502) — on Amp: its preset's name read before and
/// after, "Pluck" to "Pluck 2", said; the same name again: unchanged at the bound; no name read and
/// the envelope redrawn: "redrew"; Back on the Sample tab: refused.
#[test]
fn the_envelopes_back_and_next_read_its_name() {
    run_mac(&scenario(r##"
        S.honourMinPixels = true
        local _, _, W = T.pageOn(T.sample("amp"), { ["envelope name"] = "Pluck" })
        T.tripFrom({ action = "sample_editor_panel_button", args = { button = "next" }, focused = true }, W)
        T.ok(T.clickAt() == "1034,502", "Next: " .. T.clickAt())
        T.words({ ["envelope name"] = "Pluck 2" })
        T.repaint(T.sample("amp", { fields = { envelope_name = 100 } }))
        T.settle()
        T.ok(T.count("[avenger] [act] 'An envelope's Menu, Preset, Back or Next': changed — Amp Envelope reads Pluck 2") == 1
          and T.lastPost().id == "sample-editor-amp-next", T.dump())
        T.event()
        T.tripFrom({ action = "sample_editor_panel_button", args = { button = "back" }, focused = true }, W)
        T.ok(T.clickAt() == "1024,502", "Back: " .. T.clickAt())
        T.tick(16)
        T.ok(T.lastPost("error").text == "An envelope's Menu, Preset, Back or Next: unchanged, Amp Envelope reads Pluck 2",
          T.lastPost("error").text)
        T.event()
        T.words({})
        T.tripFrom({ action = "sample_editor_panel_button", args = { button = "next" }, focused = true }, W)
        T.repaint(T.sample("amp", { fields = { envelope_name = 130 } }))
        T.settle()
        T.ok(T.count("changed — VPS Avenger redrew the Amp Envelope") == 1, T.dump())
        T.event()
        T.paintAvenger(98, 84, 1.6, T.sample("sample"))
        local clicks, posts = #S.clicks, #T.page().posts
        T.tripFrom({ action = "sample_editor_panel_button", args = { button = "back" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.refused("An envelope's Menu, Preset, Back or Next: the Amp or the Pitch tab is not "
          .. "shown; nothing was clicked", "sample-editor-tab-amp", posts), tostring(T.lastPost().text))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The envelope's Menu at (501, 261) — (900, 502) — on Pitch: the envelope pictured and held, the
/// keyboard handed over with Return alone BEFORE the click, nothing said; Return through: the change
/// wait from the held picture, the name read after and said. The page key during it: the give-up,
/// nothing held, the name forgotten. A hand-over refused clicks nothing; so do Menu on the Sample tab
/// and Preset on EQ.
#[test]
fn the_envelopes_menu_and_preset_hand_the_keyboard_to_avenger() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("pitch"), { ["envelope name"] = "Ramp" })
        local said, clicks = #S.speech, #S.clicks
        T.tripFrom({ action = "sample_editor_panel_button", args = { button = "menu" }, focused = true }, W)
        T.ok(T.clickAt() == "900,502" and #S.clicks == clicks + 1, "Menu: " .. T.clickAt())
        T.ok(S.holding["Return"] and not S.holding["Escape"] and #S.speech == said, "Return alone, nothing said")
        T.ok(T.held() == 1, "the envelope's picture held: " .. T.held())
        T.words({ ["envelope name"] = "Ramp Up" })
        T.repaint(T.sample("pitch", { fields = { envelope_name = 90 } }))
        T.ok(T.typed("Return") == "passed", "Return through to Avenger")
        T.tick()
        T.runDue()
        T.settle()
        T.ok(T.lastPost().id == "sample-editor-pitch-menu" and T.lastPost("error").text == "" and S.front == T.PAGE_WIN,
          tostring(T.lastPost().id))
        T.ok(T.count("[avenger] [browse] 'envelope menu': after Return — 'Ramp Up', changed (before: 'Ramp')") == 1, T.dump())
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held")
        -- Preset, and the page key during it.
        T.event()
        T.tripFrom({ action = "sample_editor_panel_button", args = { button = "preset" }, focused = true }, W)
        T.ok(T.clickAt() == "960,502" and S.holding["Return"], "Preset: " .. T.clickAt())
        T.pageKey()
        T.settle(2)
        T.ok(T.starts(T.lastPost("error").text, "An envelope's Menu, Preset, Back or Next: stopped by the page key"),
          tostring(T.lastPost("error").text))
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held")
        T.ok(T.values(W).editorFields == nil, "what the user did in Avenger's list not read: the name forgotten")
        -- A hand-over refused.
        T.event()
        S.captureRaises = "the Accessibility permission is not granted"
        clicks = #S.clicks
        T.tripFrom({ action = "sample_editor_panel_button", args = { button = "menu" }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "An envelope's Menu, Preset, Back or Next: the keyboard "
          .. "cannot be handed to VPS Avenger now", T.lastPost("error").text)
        S.captureRaises = nil
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held after the refusal")
        -- Menu on the Sample tab, Preset on EQ: refused, nothing clicked, nothing handed over.
        for _, c in ipairs({ { "sample", "menu" }, { "eq", "preset" } }) do
          T.event()
          T.paintAvenger(98, 84, 1.6, T.sample(c[1]))
          local n, posts = #S.clicks, #T.page().posts
          T.tripFrom({ action = "sample_editor_panel_button", args = { button = c[2] }, focused = true }, W)
          T.settle(2)
          T.ok(#S.clicks == n and not S.holding["Return"] and T.refused("An envelope's Menu, Preset, Back or Next: the Amp "
            .. "or the Pitch tab is not shown; nothing was clicked", "sample-editor-tab-amp", posts),
            c[2] .. " on " .. c[1] .. ": " .. tostring(T.lastPost().text))
        end
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A row of the sample browser: the browser read as shown and slot 3 as selected, the slot's name
/// pictured and held, the keyboard handed over with Return alone, the row clicked at (294, 297) —
/// (568, 559) — and the tool's sentence #13 behind the screen reader; slot 3's name not read
/// meanwhile. Return through: the slot's new name read and said, and the view as read then, the page
/// back on the row. The same name: unchanged. A fourteenth row: refused with no trip.
#[test]
fn a_sample_browser_row_is_clicked_with_the_keyboard_handed_over() {
    run_mac(&scenario(r##"
        local words = T.slotWords("KIT", { ["slot 3"] = "RD Ride 1", browser = "Crash Open Hi-Hat Ride" })
        local _, _, W = T.pageOn(T.drums({ slots = { selected = 3 } }), words)
        T.ok(T.lastPost("state").state.osc_panel.sample_editor_view == "browser", "the browser read at the key")
        local function row(before, after)
          T.event()
          T.tripFrom({ action = "sample_browser_focus", args = { index = 3, name = "Ride" }, focused = true }, W)
          T.ok(T.clickAt() == "568,559" and S.holding["Return"] and not S.holding["Escape"], "row 3: " .. T.clickAt())
          T.ok(T.lastSaid() == "Use the Avenger browser. To return to Avenger Control, press Return."
            and S.speech[#S.speech].interrupt == false, tostring(T.lastSaid()))
          T.ok(T.values(W).kitNames[3] == nil, "slot 3's name not read meanwhile")
          local r = T.lastRead("vps-avenger browse slot")
          T.answer(r, { status = "text", text = before, words = {}, lines = {} })
          T.words(T.slotWords("KIT", { ["slot 3"] = after, browser = "Crash Open Hi-Hat Ride" }))
          T.repaint(T.drums({ slots = { selected = 3 } }))
          T.ok(T.typed("Return") == "passed", "Return through")
          T.tick()
          T.runDue()
          T.settle()
        end
        row("RD Ride 1", "RD Jazz")
        T.ok(T.lastPost().id == "sample-browser-item-3" and T.lastPost("error").text == ""
          and T.lastPost("state").state.osc_panel.drumkit_part_labels[3] == "RD Jazz", tostring(T.lastPost().id))
        T.ok(T.lastPost("state").state.osc_panel.sample_editor_view == "browser", "the view read after Return")
        T.ok(T.count("[avenger] [browse] 'sample browser row 3': after Return — 'RD Jazz', changed by the names (before: "
          .. "'RD Ride 1')") == 1, T.dump())
        row("RD Jazz", "RD Jazz")
        T.ok(T.lastPost("error").text == "The sample browser: unchanged, Ride reads RD Jazz", T.lastPost("error").text)
        T.event()
        local clicks, from = #S.clicks, #S.pageOrder
        T.pageMsg({ action = "sample_browser_focus", args = { index = 14, name = "x" }, focused = true })
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "The sample browser: no row of the sample browser was named"
          and not string.find(T.pageSince(from), "focus", 1, true), T.lastPost("error").text)
        T.ok(T.held() == 0 and not S.holding["Return"], "nothing held: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// The knobs.
// ---------------------------------------------------------------------------------------------

/// Volume of the sample editor: pressed at (307, 407) — (589, 735) — with the left button and held,
/// the tool's sentence #10 with the slot read as selected by its name as read at the key; one PageUp
/// is one move of one point times the factor; Return lets go once and reads nothing, the page back on
/// Volume. With DRUM SQ as the central tab nothing is pressed.
#[test]
fn a_sample_knob_is_pressed_held_and_released_through_the_keys() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("sample"), T.slotWords("KIT", { ["slot 3"] = "SD Tight" }))
        local asked = T.recordSnapshots()
        T.tripFrom({ action = "sample_editor_control", args = { control = "volume" }, focused = true }, W)
        local downs = T.inputs("down")
        T.ok(#downs == 1 and T.xy(downs[1]) == "589,735" and downs[1].button == "left", "the press: " .. T.xy(downs[1]))
        T.ok(T.lastSaid() == "Please press Page Up or Down to set Volume for SD Tight. To Confirm, press Return."
          and S.speech[#S.speech].interrupt == false, tostring(T.lastSaid()))
        T.key("PageUp")
        local moves = T.inputs("move")
        T.ok(#moves == 1 and T.xy(moves[1]) == "589,734", "one move: " .. T.xy(moves[1]))
        local n = #asked
        T.key("Return")
        T.settle()
        T.ok(#T.inputs("up") == 1 and #asked == n and T.lastPost().id == "sample-editor-volume", tostring(T.lastPost().id))
        -- Pan and Rate at their points.
        T.event()
        T.tripFrom({ action = "sample_editor_control", args = { control = "Low Pass" }, focused = true }, W)
        downs = T.inputs("down")
        T.ok(#downs == 2 and T.xy(downs[2]) == "933,735", "Lowpass: " .. T.xy(downs[2]))
        T.key("Escape")
        T.settle()
        T.ok(T.lastPost().id == "sample-editor-lowpass", tostring(T.lastPost().id))
        -- DRUM SQ as the central tab.
        T.event()
        T.paintAvenger(98, 84, 1.6, T.drums({ central = "drum_sq", sq = { chain = true, selected = "a" } }))
        T.tripFrom({ action = "sample_editor_control", args = { control = "volume" }, focused = true }, W)
        T.settle(2)
        T.ok(#T.inputs("down") == 2 and T.lastPost("error").text == "A sample control: EDITOR is not the central tab; nothing "
          .. "was clicked", T.lastPost("error").text)
        T.ok(T.held() == 0 and #S.heldButtons == 0, "nothing held")
    "##));
}

/// Reset Pan: a double-click at (393, 407) — (727, 735) — both clicks with the left button; the change
/// wait's verdict on the knob's box said, no value claimed.
#[test]
fn a_sample_knobs_reset_says_what_the_change_wait_saw() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn(T.sample("sample"))
        T.tripFrom({ action = "reset_sample_editor_control", args = { control = "pan" }, focused = true }, W)
        T.ok(T.clickAt() == "727,735" and S.clicks[#S.clicks].button == nil, "the first click: " .. T.clickAt())
        local n = #S.clicks
        T.tick(1)
        T.ok(#S.clicks == n + 1 and T.clickAt() == "727,735", "the second")
        T.repaint(T.sample("sample", { pattern = true }))
        T.settle()
        T.ok(T.count("[avenger] [act] 'Resetting a sample control': changed — VPS Avenger redrew the Pan knob") == 1
          and T.lastPost().id == "sample-editor-reset-pan", T.dump())
        T.event()
        T.tripFrom({ action = "reset_sample_editor_control", args = { control = "pan" }, focused = true }, W)
        T.tick(17)
        T.ok(T.lastPost("error").text == "Resetting a sample control: unchanged: VPS Avenger drew no change at the Pan knob",
          T.lastPost("error").text)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// Windows, and the cost.
// ---------------------------------------------------------------------------------------------

/// On Windows, Avenger's own child window at (248, 60) on a display at 150 %: Amp's switch at
/// (267, 371) and Grid's field at (321, 372), its item 1/8 at (332, 393.5), through the factor 2.4.
#[test]
fn on_windows_at_150_percent_the_editor_is_clicked_through_the_factor() {
    run(&scenario(r##"
        local j = T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 2.4, T.sample("amp"))
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
        trip({ action = "toggle_sample_editor_enable", args = { tab = "amp" }, focused = true })
        -- 248 + 2.4 * 267 = 888.8 -> 889, 60 + 2.4 * 371 = 950.4 -> 950.
        T.ok(T.clickAt() == "889,950", "Amp's switch: " .. T.clickAt())
        T.repaint(T.sample("amp", { sampleOn = true }))
        T.tick(3)
        T.ok(T.lastPost("state").state.osc_panel.sample_editor_amp_states[3] == true, "read ON")
        T.event()
        trip({ action = "sample_editor_grid", args = { item = "1/8" }, focused = true })
        -- 248 + 2.4 * 321 = 1018.4 -> 1018, 60 + 2.4 * 372 = 952.8 -> 953.
        T.ok(T.clickAt() == "1018,953" and S.clicks[#S.clicks].button == "left", "Grid's field: " .. T.clickAt())
        T.paintAvenger(248, 60, 2.4, T.sample("amp", { popup = { 321, 378, 380, 450 } }))
        T.tick(3)
        -- 248 + 2.4 * 332 = 1044.8 -> 1045, 60 + 2.4 * 393.5 = 1004.4 -> 1004.
        T.ok(T.clickAt() == "1045,1004", "1/8: " .. T.clickAt())
        T.repaint(T.sample("amp"))
        T.settle(5)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The turns of Luau's loops and calls the editor's part read spends at 200 % on a Windows display at
/// 150 % — a factor of 6: the strip, the central tab, the drumkit table and the editor's tabs and
/// switch, before and after Amp's switch — each under a bound fewer than the pixels of the table's
/// boxes, so a loop over them in Luau would pass it.
#[test]
fn the_editor_reads_loop_over_no_pixels_in_luau() {
    run(&scenario(r##"
        S.screenSize = { w = 6000, h = 4000 }
        local j = T.onWindows(200, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 6, 200))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 6, T.sample("amp"))
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
        T.pageMsg({ action = "toggle_sample_editor_enable", args = { tab = "amp" }, focused = true })
        T.land()
        T.runDue()
        T.repaint(T.sample("amp", { sampleOn = true }))
        T.tick(3)
        Open.part = part
        T.ok(#spent == 2 and T.lastPost("state").state.osc_panel.sample_editor_amp_states[3] == true, "two reads: " .. #spent)
        -- Measured 2026-10-17: 38,194 each; the table's 48 boxes alone are 179,712 pixels.
        for _, n in ipairs(spent) do
          T.ok(n < 45000, string.format("a part read spent %d turns: %s", n, table.concat(spent, ", ")))
        end
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}
