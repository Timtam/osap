//! VPS Avenger's page (`modules/vps-avenger/src/page.luau`, `ui/`): the avenger_control project's
//! panel shown with `host.gui.webview` beside Avenger, as the maintainer decided on 2026-10-12 —
//! its first stage, which reads the preset's name and the zoom and clicks nothing. The module is
//! loaded for real against the scripted host of `overlay_avenger_tests.rs`, with its scripted page
//! (`T.pageHost`): a page object that records what it is asked, as `page_example_tests.rs` has one.
//!
//! What the stage does is pinned here, with no window: the page is described once at load — and
//! again only when the Mac page mode changes, which is fixed per page object — and built when
//! Avenger is identified, not on the key; the key on an identified Avenger reads Avenger off one
//! picture and shows the page beside Avenger once the reads are in, the state, the error region
//! cleared and the focus posted first, with Open.wav; the key in the page brings Avenger's window
//! forward first — also one its DAW hid, which the host no longer lists — and hides the page after,
//! with Close.wav; anywhere else the key says the tool's sentence, without a DAW in it, and a press
//! whose reads show no page says why; `ready` gets one state message, which says how old its reads
//! are when the page reloaded itself since they were posted to it, and not when the press that made
//! them built the page; every action of the page is answered "not available in this module yet" —
//! Initialize's No clicks nothing and says nothing — and none is acted on while the page does not
//! have the keyboard; and the page key's setting refuses a key the platform cannot hold.
//!
//! And the reads: one picture, cut at the primary screen's edge, within what one snapshot may hold
//! (the scripted snapshot keeps the host's budget); one press at a time by the keys of its questions,
//! so that a disable that dropped them leaves the key working; and a bound on the Luau work of the
//! open and of each read, which a loop over any box's pixels passes.
//!
//! And the protocol, with no JS run: every action name `ui/panel.js` sends and every `data-action`
//! of `ui/index.html` is on the page's own list, `ACTIONS`, and every name on it has a handler in
//! the module or is on its explicit list of actions not built yet.

use super::overlay_avenger_tests::{run, run_mac};

/// The page key on a Mac: the tool's chord, Control-Shift-Command-F1 (`Ctrl` is Command and `Win`
/// is Control in a key spec there).
const MAC_KEY: &str = "Ctrl+Shift+Win+F1";

/// Described once at load, as a tool page that activates; built as Avenger is identified, before
/// any key; and described again — the old one closed — when the Mac page mode changes, and only
/// then. The five settings of the plan's §2.9, with their defaults — the expansions list the
/// tool's own default, "Factory Standard".
#[test]
fn the_page_is_described_once_and_again_when_the_mac_page_mode_changes() {
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.ok(#S.pages == 1, "described once at load: " .. #S.pages)
        local o = T.page().opts
        T.ok(o.file == "ui/index.html" and o.title == "Avenger Control" and o.window == nil and o.activates == true,
          "a tool page from the module's own folder, which activates")
        T.ok(T.page().prepares == 1 and #T.page().shows == 0, "built as Avenger was identified, and not shown")
        T.ok(S.settings.roster == "Factory Standard" and S.settings.pageKey == "@KEY@" and S.settings.sounds == true
          and S.settings.pageLeavesDawActive == false and S.settings.drumSqCadence == 250, "the defaults")
        T.ok(table.concat(S.settingDefs.roster.opts.oneOf, "|") == "Factory Standard|Complete",
          "the rosters, the tool's default first")
        T.ok(S.settingDefs.drumSqCadence.opts.min == 100 and S.settingDefs.drumSqCadence.opts.max == 1000, "its bounds")
        T.ok(S.pageKeys["@KEY@"] ~= nil, "the key")
        -- The Mac page mode on: the page closed and described again, without activation.
        T.setting("pageLeavesDawActive", true)
        T.ok(#S.pages == 2 and S.pages[1].closed and T.page().opts.activates == false, tostring(#S.pages))
        T.ok(T.count("[avenger] [page] the page is described again: activates = false") == 1, T.dump())
        T.setting("pageLeavesDawActive", true)
        T.ok(#S.pages == 2, "the dialog's OK with nothing changed describes nothing")
        -- The new page is built by the key, which says so, and shown once it has loaded.
        T.openPage()
        T.ok(T.lastSaid() == "Building the page." and #T.page().shows == 1, T.said())
        T.pageReady()
        T.ok(#T.page().shows == 2 and S.front == T.PAGE_WIN, "shown once it has loaded")
        T.ok(S.sounds[#S.sounds] == "sounds/Open.wav", tostring(S.sounds[#S.sounds]))
    "##.replace("@KEY@", MAC_KEY).as_str());
}

/// The key on an identified Avenger reads Avenger off one picture — the preset's name in the text
/// read of it, where the ring reads it — and once the reads are in posts the state — on a picture
/// with nothing of Avenger's drawn in it, the name and the zoom read, the oscillators and macros
/// not, which the page shows as not read — then clears the error region and puts the focus on the
/// preset's name, and only then shows the page beside Avenger's window, with the keyboard, and
/// plays Open.wav. Pressed in the page, it asks Avenger's window to the front FIRST and hides the
/// page after, with Close.wav. The second show goes back where the user was. A window the host no
/// longer lists is asked for all the same — a DAW that hides its plug-in windows while the page is
/// in front shows them again when it comes forward — and taken as closed only when it refuses; a
/// window that would not come keeps the page, saying why in its error region; the sounds follow
/// their setting.
#[test]
fn the_key_shows_the_page_beside_avenger_and_in_the_page_goes_back_to_it() {
    run_mac(r##"
        local S = T.S
        local header, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.ok(#T.page().posts == 0, "nothing posted before the key")
        T.nameIs("LD Lead 1")
        local reads = #S.recognized
        local from = #S.pageOrder
        T.openPage()
        local name = nil
        for i = reads + 1, #S.recognized do
          if S.recognized[i].name == "preset" then name = S.recognized[i] end
        end
        T.ok(name and T.region(name) == "152,95,440,119" and name.snapshot ~= nil,
          "the name read where the ring reads it, of the picture")
        T.ok(T.pageSince(from) == "post state,post error,post focus,show,sound sounds/Open.wav", T.pageSince(from))
        local st = T.lastPost("state").state
        T.ok(st.preset == "LD Lead 1" and st.zoom == 80, tostring(st.preset) .. " " .. tostring(st.zoom))
        T.ok(st.expansions == nil and st.read_age_seconds == nil, "no database, and read just now")
        T.ok(st.osc_panel.count == nil and st.osc_panel.selected_type == nil and st.macro_panel.count == nil
          and st.macro_panel.selected_index == nil and st.expansion_view == nil, "the rest not read")
        T.ok(T.lastPost("error").text == "" and T.lastPost("focus").id == "preset-name", "cleared, and the name")
        local shown = T.page().shows[1]
        T.ok(shown.focus == true and shown.avoid.x == W.bounds.x and shown.avoid.y == W.bounds.y
          and shown.avoid.w == W.bounds.w and shown.avoid.h == W.bounds.h, "beside Avenger, with the keyboard")
        T.ok(S.front == T.PAGE_WIN, "in front")
        T.ok(T.count("[avenger] [page] shown beside 'VST3i: VPS Avenger - Track 1', ") == 1, T.dump())
        T.event()
        T.ok(not header.active, "the header lets go in front of the page")
        -- In the page: Avenger's window first, then the page hidden.
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        from = #S.pageOrder
        T.pageKey()
        T.ok(T.pageSince(from) == "focus 31,hide,sound sounds/Close.wav", T.pageSince(from))
        T.ok(S.front == W, "Avenger in front")
        -- Back on Avenger, identified again in its new stay, the key shows the page where the user was.
        T.event()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.ok(header.active, "the header again")
        T.setting("sounds", false)
        from = #S.pageOrder
        T.openPage()
        T.ok(T.pageSince(from) == "post state,post error,post restoreFocus,show", T.pageSince(from))
        -- Avenger's window gone, and the focus refused: the page stays, and says the tool's sentence.
        S.listed = {}
        S.focusAnswer = false
        from = #S.pageOrder
        T.pageKey()
        T.ok(T.pageSince(from) == "focus 31,post error", T.pageSince(from))
        T.ok(T.lastPost("error").text == "The plug-in window was closed. Please open it again, then press @KEY@ again.",
          T.lastPost("error").text)
        -- Declined: the page stays.
        S.listed = { W }
        S.focusAnswer = false
        from = #S.pageOrder
        T.pageKey()
        T.ok(T.pageSince(from) == "focus 31,post error", T.pageSince(from))
        T.ok(T.lastPost("error").text == "VPS Avenger's window would not come to the front.", T.lastPost("error").text)
        -- The close box: hidden by the host, the front given back; Close.wav when the sounds are on.
        T.setting("sounds", true)
        T.pageClosed("user")
        T.ok(S.sounds[#S.sounds] == "sounds/Close.wav", tostring(S.sounds[#S.sounds]))
        -- Hidden by its DAW while the page is in front — not listed — and shown again as the DAW comes
        -- forward: the focus accepted, the page hidden, and no word of a closed window.
        S.front, S.chain = T.PAGE_WIN, { T.PAGE_WIN }
        S.listed = {}
        S.focusAnswer = true
        local function errors()
          local n = 0
          for _, p in ipairs(T.page().posts) do if p.kind == "error" then n += 1 end end
          return n
        end
        local before = errors()
        from = #S.pageOrder
        T.pageKey()
        T.ok(T.pageSince(from) == "focus 31,hide,sound sounds/Close.wav", T.pageSince(from))
        T.ok(errors() == before, "nothing said of a closed window")
        T.ok(T.count("[avenger] [page] 'VST3i: VPS Avenger - Track 1' (id=31) is not listed while the page is in front; "
          .. "its focus was accepted") == 1, T.dump())
    "##.replace("@KEY@", MAC_KEY).as_str());
}

/// Anywhere but on an identified Avenger the key opens nothing and says the tool's sentence —
/// without the DAWs it named, which are daw-hosts' to know — before Avenger is identified, and
/// with another application in front.
#[test]
fn the_key_away_from_an_identified_avenger_says_why_and_opens_nothing() {
    run_mac(r##"
        local S = T.S
        local W = T.float(80)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.ok(not header.active, "not identified yet")
        T.pageKey()
        local said = "It seems that the VPS Avenger plug-in window is not at the front. Please open the plug-in "
          .. "window, then press @KEY@ again."
        T.ok(T.lastSaid() == said, T.said())
        T.ok(#T.page().shows == 0 and #T.page().posts == 0 and T.page().prepares == 0, "nothing built or shown")
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.ok(header.active and T.page().prepares == 1, "identified, and the page built")
        S.listed = { W, T.OTHER }
        T.show(T.OTHER)
        T.ok(not header.active)
        local n = #S.speech
        T.pageKey()
        T.ok(#S.speech == n + 1 and T.lastSaid() == said and #T.page().shows == 0, T.said())
    "##.replace("@KEY@", MAC_KEY).as_str());
}

/// `ready` — the page loaded, or reloaded itself — gets one state message: before the key, with
/// nothing read; after it, the reads already made, with how many seconds ago they were made once
/// the page has reloaded itself since they were posted to it — and so does the host's onReady of
/// the reload, since the page's own `ready` may come first. The expansions come from the preset
/// database's roster, marked — "Factory Standard" unless the setting says otherwise — and the
/// roster setting's change is posted at once.
#[test]
fn ready_gets_one_state_message_from_the_reads_already_made() {
    run_mac(r##"
        local S = T.S
        T.presets()
        T.avengerAt80("Init Preset")
        local n = #T.page().posts
        T.pageMsg({ action = "ready" })
        T.ok(#T.page().posts == n + 1 and T.lastPost().kind == "state", "one state")
        local st = T.lastPost().state
        T.ok(st.preset == nil and st.zoom == nil and st.read_age_seconds == nil, "nothing read before the key")
        T.ok(table.concat(st.expansions, "|") == "Alpha" and st.expansions_from_database == true,
          "the database's roster, the tool's default, marked")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        S.now += 5000
        -- The page reloads itself: the host's onReady posts the reads with their age, and so does
        -- the answer to the page's own `ready`.
        n = #T.page().posts
        T.pageReady()
        T.ok(#T.page().posts == n + 1 and T.lastPost().state.read_age_seconds == 5, "the reload's state, with the age")
        n = #T.page().posts
        T.pageMsg({ action = "ready" })
        T.ok(#T.page().posts == n + 1, "one state")
        st = T.lastPost().state
        T.ok(st.preset == "LD Lead 1" and st.zoom == 80 and st.read_age_seconds == 5, tostring(st.read_age_seconds))
        T.setting("roster", "Complete")
        st = T.lastPost().state
        T.ok(table.concat(st.expansions, "|") == "Alpha|Beta|Gamma|Delta",
          "the roster changed: " .. table.concat(st.expansions, "|"))
    "##);
}

/// While the page has the keyboard, every action it sends is answered in its error region with
/// the action's own words — "not available in this module yet" — and so is a name the page does
/// not have, which the log calls so; Initialize's No closes the page's dialog and clicks nothing,
/// and says nothing. A message while the page does not have the keyboard is not acted on, a problem
/// the page reports is logged wherever the user is, and the page's own word for its keyboard is
/// logged where it disagrees with the host's.
#[test]
fn actions_are_answered_not_available_yet_and_only_while_the_page_has_the_keyboard() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.openPage()
        T.ok(S.front == T.PAGE_WIN, "in front")
        local clicks = #S.clicks
        T.pageMsg({ action = "next_preset", args = {}, focused = true })
        T.ok(T.lastPost().kind == "error" and T.lastPost().text == "Next preset is not available in this module yet",
          tostring(T.lastPost().text))
        T.pageMsg({ action = "osc_control", args = { control = "volume" }, focused = true })
        T.ok(T.lastPost().text == "An oscillator's control is not available in this module yet", T.lastPost().text)
        T.pageMsg({ action = "bogus", args = {} })
        T.ok(T.lastPost().text == "bogus is not available in this module yet", T.lastPost().text)
        T.ok(T.count("[avenger] [page] 'bogus' is no action of the page's") == 1, T.dump())
        local n = #T.page().posts
        T.pageMsg({ action = "cancel_initialize_preset", args = {}, focused = true })
        T.ok(#T.page().posts == n, "No says nothing")
        T.ok(T.count("[avenger] [page] Initialize Preset: No — the page's dialog closed; nothing clicked in Avenger") == 1, T.dump())
        T.ok(#S.clicks == clicks, "nothing clicked at all")
        -- The user went back to Avenger some other way: the page is not in front.
        S.front, S.chain = W, { W }
        n = #T.page().posts
        T.pageMsg({ action = "next_preset", args = {}, focused = false })
        T.ok(#T.page().posts == n, "not acted on")
        T.ok(T.count("[avenger] [page] 'next_preset' not acted on: the page does not have the keyboard") == 1, T.dump())
        T.pageMsg({ kind = "problem", text = "boom (panel.js:3)" })
        T.ok(T.count("[avenger] [page] the page reports: boom (panel.js:3)") == 1, T.dump())
        T.pageMsg({ action = "next_preset", args = {}, focused = true })
        T.pageMsg({ action = "next_preset", args = {}, focused = true })
        T.ok(T.count("[avenger] [page] the page says it has the keyboard, and the host that its window is not in front") == 1,
          T.dump())
        T.ok(#T.page().posts == n, "still nothing acted on")
    "##);
}

/// The page key is a setting, checked with host.keys.check: a key the platform cannot hold is
/// refused and the key in effect stays — the setting says it again, and so does the speech — and
/// a good one takes its place at once. A stored key the platform cannot hold is not registered at
/// load: the default is, and the setting is set back to it. On Windows the default is the tool's
/// Windows chord.
#[test]
fn the_page_key_setting_refuses_a_key_the_platform_cannot_hold() {
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        S.badKeys["Win+Alt+Q"] = { "voiceover" }
        T.setting("pageKey", "Win+Alt+Q")
        T.ok(S.settings.pageKey == "@KEY@" and S.pageKeys["@KEY@"] ~= nil and S.pageKeys["Win+Alt+Q"] == nil,
          "refused, and the key in effect stays")
        T.ok(T.lastSaid() == "Win+Alt+Q cannot be the page key; it stays @KEY@.", T.said())
        T.setting("pageKey", "Ctrl+Shift+F2")
        T.ok(S.pageKeys["Ctrl+Shift+F2"] ~= nil and S.pageKeys["@KEY@"] == nil, "the new key, and the old one let go")
        T.ok(T.lastSaid() == "Page key changed to Ctrl+Shift+F2.", T.said())
        T.openPage("Ctrl+Shift+F2")
        T.ok(#T.page().shows == 1, "the new key shows the page")
    "##.replace("@KEY@", MAC_KEY).as_str());
    run_mac(r##"
        local S = T.S
        S.settings.pageKey = "Win+Alt+Q"
        S.badKeys["Win+Alt+Q"] = { "voiceover" }
        T.avengerAt80("Init Preset")
        T.ok(S.pageKeys["@KEY@"] ~= nil and S.settings.pageKey == "@KEY@", "the default, and the setting with it")
        T.ok(T.count("[avenger] [page] the page key 'Win+Alt+Q' cannot be used (voiceover): @KEY@ instead") == 1, T.dump())
    "##.replace("@KEY@", MAC_KEY).as_str());
    run(r##"
        local S = T.S
        T.onWindows(80, 1)
        T.avenger()
        T.ok(S.settings.pageKey == "Ctrl+Shift+Alt+F1" and S.pageKeys["Ctrl+Shift+Alt+F1"] ~= nil, "the Windows chord")
    "##);
}

// ---------------------------------------------------------------------------------------------
// The open-time reads: one picture, every question of it at once, one text read of it.
// ---------------------------------------------------------------------------------------------

/// The words of a picture of Avenger with three oscillators — the second one's name Avenger's own
/// caption, read as "0SC 2" — two macro tabs, the second shown, and the browser in 8 per Row.
const WORDS: &str = r#"{
  preset = "LD Lead 1", ["expansion view"] = "8 per Row",
  ["osc 1"] = "Bass", ["osc 2"] = "0SC 2", ["osc 3"] = "Lead",
  ["macro tabs"] = "MACRO 1 MACRO 2",
  ["macro macro1"] = "Cutoff", ["macro macro2"] = "Macro 5", ["macro macro3"] = "Drive",
  ["macro btn1"] = "Hold", ["macro btn2"] = "MacroBtn 4",
  ["central tabs"] = "EDITOR ARP DRUM SQ STEP SQ",
}"#;

/// The key on Avenger at 80 % in REAPER's floating window: ONE snapshot of the plug-in's panel,
/// waited for in the key's handler; every grid and profile of it asked at once, with callbacks, and
/// the page shown only once they and one text read of the same picture are in. The state carries
/// what the picture shows under the tool's keys: the count of the dots, the names cleaned the
/// project's way ("0SC 2" is OSC 2), the tab whose bottom strip is darkest, Mute and Solo by their
/// red, Drums' and Trig's too; the selected oscillator's knobs from their pointers; the macro tabs
/// counted from their captions, the one shown by its strip, its names, its buttons and its knobs'
/// amounts; the browser's view and FAV. The Drums tab's own switches and DRUM SQ are not read while
/// an oscillator is selected, as the project reads them only with Drums. Every number is in the
/// log, and the time from the key to the show.
#[test]
fn the_reads_of_one_picture_give_the_tools_state_keys() {
    run_mac(&r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 2, mute = { [1] = true }, solo = { [3] = true },
          drumsMute = true, trigSolo = true, macroShown = 2, buttons = { true, false }, amounts = { 40, 0, 100 },
          volume = 100, transpose = 24, fine = 0, fav = true })
        T.words(@WORDS@)
        local snaps, from = S.snaps, #S.pageOrder
        T.pageKey()
        T.ok(S.snaps == snaps + 1, "one picture, in the key's handler")
        T.ok(#S.screenOut > 0 and S.screenOut[#S.screenOut].cb ~= nil and T.pageSince(from) == "",
          "every question asked with a callback, and nothing shown yet")
        local asked = S.cellsAsked + S.profilesAsked
        T.tick()
        T.ok(T.pageSince(from) == "post state,post error,post focus,show,sound sounds/Open.wav", T.pageSince(from))
        T.ok(S.snaps == snaps + 1 and S.cellsAsked + S.profilesAsked == asked, "nothing asked again")
        local texts = 0
        for _, r in ipairs(S.recognized) do if r.snapshot then texts += 1 end end
        T.ok(texts == 12, "one text read of the picture, a region per name: " .. texts)
        local st = T.lastPost("state").state
        T.ok(st.preset == "LD Lead 1" and st.zoom == 80 and st.expansion_view == "8 per Row"
          and st.expansion_open == false and st.selected_expansion == "" and st.expansion_favorite_view == true,
          "the header and the browser")
        local o = st.osc_panel
        T.ok(o.count == 3 and table.concat(o.names, "|") == "Bass|OSC 2|Lead" and o.names_from_database == nil,
          "the oscillators: " .. tostring(o.count) .. " " .. table.concat(o.names or {}, "|"))
        T.ok(o.selected_type == "osc" and o.selected_index == 2 and o.selected_label == "OSC 2", "the second selected")
        T.ok(o.mute_states[1] == true and o.mute_states[2] == false and o.mute_states[3] == false
          and o.solo_states[1] == false and o.solo_states[2] == false and o.solo_states[3] == true, "Mute and Solo")
        T.ok(o.drums_mute == true and o.drums_solo == false and o.trig_mute == false and o.trig_solo == true, "Drums and Trig")
        T.ok(o.volume_values[2] == 100 and o.pan_values[2] == 0 and o.transpose_values[2] == 24
          and o.fine_tune_values[2] == 0 and o.random_pitch_values[2] == 0 and o.volume_values[1] == nil,
          string.format("the knobs: %s %s %s %s %s", tostring(o.volume_values[2]), tostring(o.pan_values[2]),
            tostring(o.transpose_values[2]), tostring(o.fine_tune_values[2]), tostring(o.random_pitch_values[2])))
        T.ok(o.drum_pattern == nil and o.drum_lock == nil and o.drum_mode == nil and o.central_panel_tab == nil
          and o.drum_sq_pattern_selected == nil, "nothing of the Drums tab while an oscillator is selected")
        local m = st.macro_panel
        T.ok(m.count == 2 and m.selected_index == 2, "two macro tabs, the second shown")
        T.ok(table.concat(m.tabs[2], "|") == "Cutoff|Macro 5|Drive|Hold|MacroBtn 4" and m.tabs[1] == nil
          and m.selected_labels == m.tabs[2], "its names")
        T.ok(m.button_states[1] == true and m.button_states[2] == false, "its buttons")
        T.ok(m.amount_values[1] == 40 and m.amount_values[2] == 0 and m.amount_values[3] == 100,
          string.format("its knobs: %s %s %s", tostring(m.amount_values[1]), tostring(m.amount_values[2]),
            tostring(m.amount_values[3])))
        for _, line in ipairs({ "[avenger] [read] strip: 3 dots", "[avenger] [read] strip: the tabs' names 1 'Bass'; 2 '0SC 2' -> 'OSC 2'",
          "[avenger] [read] strip: the tabs' bottom strips, darkest selected:", "OSC 1 Mute 100.0 % red",
          "[avenger] [read] macros: button 1 highlight ", "ON; button 2 highlight 0.0 %", "[avenger] [read] oscillator: Level ",
          "[avenger] [read] expansion: FAV ", "[avenger] [read] words: preset 'LD Lead 1'",
          "[avenger] [read] 19 questions of one picture; the picture ", "[avenger] [page] shown beside" }) do
          T.ok(T.count(line) == 1, "the log: " .. line)
        end
    "##.replace("@WORDS@", WORDS));
}

/// The project's helpers for what a recogniser makes of Avenger's text: an OSC tab's caption read
/// as "0SC 2", "OSC l" or "QSC 3" is Avenger's own, "OSC <n>", for the tab it is in, and a name of
/// the sound designer's own is kept; the tabs' count from the strip read whole; the macro tabs'
/// count from their captions; the browser's view from its caption; and a name read against the
/// database's, taken in its spelling when it is the same words, replaced and marked when it is
/// near enough, and kept when it is not.
#[test]
fn the_projects_text_helpers_clean_count_and_check_what_is_read() {
    run_mac(r##"
        T.avengerAt80("Init Preset")
        local function file(rel) return T.S.included["modules/vps-avenger/src/" .. rel][1] end
        local R, Strip, E = file("read/ocr.luau"), file("read/strip.luau"), file("read/expansion.luau")
        for _, c in ipairs({ { "0SC 2", 2, "OSC 2" }, { "OSC l", 1, "OSC 1" }, { "QSC 3", 3, "OSC 3" },
          { "OSC", 4, "OSC 4" }, { "• OSC 5", 5, "OSC 5" }, { "OSC Lead", 2, "OSC Lead" },
          { "Oscar Bass", 1, "Oscar Bass" }, { "OSC 3", 2, "OSC 3" }, { "  Bass   Line ", 1, "Bass Line" } }) do
          T.ok(R.cleanOscName(c[1], c[2]) == c[3], c[1] .. " -> " .. R.cleanOscName(c[1], c[2]))
        end
        T.ok(Strip.countFromText("OSC 1 OSC 2 0SC 3 Drums Trig") == 3 and Strip.countFromText("osc1 OSG 4") == 4
          and Strip.countFromText("XOSC 2 Drums") == nil and Strip.countFromText("") == nil, "the strip's count")
        T.ok(R.macroTabCount("MACRO 1  MACRO 2 Macro 3") == 3 and R.macroTabCount("macro i") == 1
          and R.macroTabCount("MACRO 7") == 1 and R.macroTabCount("") == nil, "the macro tabs' count")
        T.ok(E.parseView("8 per Row") == "8 per Row" and E.parseView("list") == "List" and E.parseView("4perRow") == "4 per Row"
          and E.parseView("") == nil and E.parseView("per 8") == nil, "the view")
        local a, sa = R.snap("Laed", "Lead")
        local b, sb = R.snap("lead", "Lead")
        local c, sc = R.snap("Bass", "Lead")
        local d, sd = R.snap("Bass", nil)
        T.ok(a == "Lead" and sa == true and b == "Lead" and sb == false and c == "Bass" and sc == false
          and d == "Bass" and sd == false, "the database's vocabulary")
    "##);
}

/// On Windows, Avenger's own child window on a display scaled to 150 %: the factor carries the
/// display's scaling (2.4 at 80 %), and the project's pixels per pixel of the picture are 2 over
/// 1.5 — so the same picture of Avenger, drawn half again as large, reads as it does on a Mac: the
/// shares are of the region, and a box holds about as many of the project's pixels at either size.
#[test]
fn on_windows_at_150_percent_display_scaling_the_same_picture_reads_the_same() {
    run(&r##"
        local S = T.S
        T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(248, 60, 2.4, { osc = 3, selected = "osc", index = 2, mute = { [1] = true }, solo = { [3] = true },
          drumsMute = true, trigSolo = true, macroShown = 2, buttons = { true, false }, amounts = { 40, 0, 100 },
          volume = 100, transpose = 24, fine = 0, fav = true })
        T.words(@WORDS@)
        T.openPage()
        local st = T.lastPost("state").state
        local o, m = st.osc_panel, st.macro_panel
        T.ok(st.zoom == 80 and o.count == 3 and table.concat(o.names, "|") == "Bass|OSC 2|Lead" and o.selected_index == 2,
          "the strip")
        T.ok(o.mute_states[1] == true and o.solo_states[3] == true and o.mute_states[2] == false and o.drums_mute == true
          and o.trig_solo == true, "Mute and Solo")
        T.ok(o.volume_values[2] == 100 and o.transpose_values[2] == 24 and o.fine_tune_values[2] == 0,
          string.format("the knobs: %s %s %s", tostring(o.volume_values[2]), tostring(o.transpose_values[2]),
            tostring(o.fine_tune_values[2])))
        T.ok(m.count == 2 and m.selected_index == 2 and m.button_states[1] == true and m.button_states[2] == false
          and m.amount_values[1] == 40 and m.amount_values[2] == 0 and m.amount_values[3] == 100, "the macros")
        T.ok(st.expansion_favorite_view == true, "FAV")
    "##.replace("@WORDS@", WORDS));
}

/// With the Drums tab selected: its pattern switch, its lock and its mode (the project's rule —
/// DRUMKIT's caption the brighter is SEQUENCE), the central tab, and DRUM SQ on it: PATTERN CHAIN
/// by its brightness, the patterns that are on by their red, the selected one by its outline. One
/// macro tab, counted from a caption with no number, is the one shown. And with the preset database
/// installed: the preset placed in the catalog gives the expansion, marked, and the names its tabs
/// have there are the vocabulary the names read are checked against — "Laed" is the catalog's
/// "Lead", "Tane" its "Tone", each marked as the database's — while a name the same as the
/// catalog's, or Avenger's own caption, is not marked.
#[test]
fn the_drums_tab_drum_sq_and_the_database_vocabulary_are_read_and_marked() {
    run_mac(r##"
        local S = T.S
        T.presets()
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "drums", pattern = true, lock = true, mode = "sq",
          central = "drum_sq", sq = { chain = true, follow = false, on = { b = true }, selected = "a" },
          buttons = { false, true }, amounts = { 0, 50, 100 } })
        T.words({ preset = "AR Band Pass Slide", ["expansion view"] = "List",
          ["osc 1"] = "Arp", ["osc 2"] = "OSC 2", ["osc 3"] = "Laed", ["macro tabs"] = "Macro",
          ["macro macro1"] = "Tane", ["macro macro2"] = "Macro 2", ["macro macro3"] = "Drive",
          ["macro btn1"] = "+12", ["macro btn2"] = "MacroBtn 2" })
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.preset == "AR Band Pass Slide" and st.expansion == "Alpha" and st.expansion_from_database == true,
          "the expansion, the database's: " .. tostring(st.expansion))
        T.ok(st.expansion_view == "List" and st.expansion_open == false, "the view")
        local o = st.osc_panel
        T.ok(table.concat(o.names, "|") == "Arp|OSC 2|Lead" and o.names_from_database[1] == false
          and o.names_from_database[2] == false and o.names_from_database[3] == true, table.concat(o.names, "|"))
        T.ok(o.selected_type == "drums" and o.selected_index == nil and o.selected_label == "Drums", "Drums selected")
        T.ok(o.drum_pattern == true and o.drum_lock == true and o.drum_mode == "sq",
          string.format("the Drums tab: %s %s %s", tostring(o.drum_pattern), tostring(o.drum_lock), tostring(o.drum_mode)))
        T.ok(o.central_panel_tab == "drum_sq", "the central tab: " .. tostring(o.central_panel_tab))
        T.ok(o.drum_sq_pattern_chain == true and o.drum_sq_auto_follow == false and o.drum_sq_pattern_selected == "a"
          and o.drum_sq_pattern_states[1] == true and o.drum_sq_pattern_states[2] == false
          and o.drum_sq_pattern_states[3] == false,
          string.format("DRUM SQ: %s %s %s", tostring(o.drum_sq_pattern_chain), tostring(o.drum_sq_auto_follow),
            tostring(o.drum_sq_pattern_selected)))
        T.ok(o.volume_values == nil, "no oscillator's knobs with Drums selected")
        local m = st.macro_panel
        T.ok(m.count == 1 and m.selected_index == 1, "one macro tab, shown")
        T.ok(table.concat(m.tabs[1], "|") == "Tone|Macro 2|Drive|+12|MacroBtn 2" and m.labels_from_database[1] == true
          and m.labels_from_database[3] == false, table.concat(m.tabs[1], "|"))
        T.ok(m.button_states[1] == false and m.button_states[2] == true, "the buttons")
        T.ok(m.amount_values[1] == 0 and m.amount_values[2] == 50 and m.amount_values[3] == 100,
          string.format("the knobs: %s %s %s", tostring(m.amount_values[1]), tostring(m.amount_values[2]),
            tostring(m.amount_values[3])))
        T.ok(T.count("3 'Laed' -> 'Lead', the database's (0.75 alike)") == 1, T.dump())
        T.ok(T.count("[avenger] [read] drum sq: PATTERN CHAIN mean ") == 1, T.dump())
    "##);
}

/// What a read could not find is not in the state, or a hole in its list, and the page says each
/// one as "not read" (its words for them are checked to be in ui/panel.js): on a picture with
/// nothing of Avenger's in it and no words, nothing is read but what a picture shows of any
/// switch — off; and on one with three dots, one name read and no knob drawn, the other names and
/// the knobs are holes.
#[test]
fn what_a_read_could_not_find_is_not_read() {
    for words in [
        "Preset name, not read", "OSC tabs, not read", "name not read", "Selected tab, not read",
        "Macro tabs, not read", "Macro tab, not read", "Expansion View, not read", "Size, not read",
        "Volume, not read", "Pan, not read", "Transpose, not read", "Fine Tune, not read",
        "Amount, not read", "PATTERN CHAIN, not read", "AUTO FOLLOW, not read", "LENGTH, not read",
        "FAV, not read", "FAV view, not read", "Central tab, not read",
    ] {
        assert!(PANEL_JS.contains(words), "panel.js has no '{words}'");
    }
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.words({})
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.preset == nil and st.zoom == 80 and st.expansion_view == nil and st.expansion_open == nil
          and st.selected_expansion == nil, "the header and the browser not read, but the zoom")
        local o, m = st.osc_panel, st.macro_panel
        T.ok(o.count == nil and o.names == nil and o.mute_states == nil and o.selected_type == nil
          and o.volume_values == nil, "the strip not read")
        T.ok(o.drums_mute == false and o.trig_solo == false, "a switch on a picture shows off")
        T.ok(m.count == nil and m.selected_index == nil and m.tabs == nil and m.amount_values[1] == nil,
          "the macros not read")
        T.ok(T.count("[avenger] [read] expansion: the view reads nothing — the area read: nothing read") == 1, T.dump())
        T.ok(T.count("[avenger] [read] strip: the tabs read whole — no count") == 1, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1 })
        T.words({ ["osc 1"] = "Bass" })
        T.openPage()
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.count == 3 and o.names[1] == "Bass" and o.names[2] == nil and o.names[3] == nil, "holes for the names")
        T.ok(o.selected_type == "osc" and o.selected_index == 1 and o.volume_values[1] == nil
          and o.transpose_values[1] == nil, "no pointer drawn: the knobs not read")
        T.ok(T.count("[avenger] [read] oscillator: Level 0 bright of ") == 1, T.dump())
    "##);
}

/// The browser's caption reading no view, its area is read as text — a second read of the same
/// picture, the only one — and with the preset database the presets it names give the open
/// expansion; its categories and presets are the database's lists, marked, and the preset placed
/// in it is the one loaded. Without the database, two words of an open expansion's say an
/// expansion is open, which one not read. A caption that reads a view reads no area.
#[test]
fn an_open_expansion_is_read_from_the_browsers_area() {
    run_mac(r##"
        local S = T.S
        T.presets()
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.words({ preset = "LD Lead 2", ["expansion area"] = "Leads LD Lead 1 LD Lead 2 LD Lead 3" })
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.expansion_open == true and st.selected_expansion == "Delta" and st.expansion_view == nil, "Delta open")
        T.ok(table.concat(st.expansion_categories, "|") == "Leads"
          and table.concat(st.expansion_presets, "|") == "LD Lead 1|LD Lead 2|LD Lead 3"
          and table.concat(st.expansion_preset_categories, "|") == "Leads|Leads|Leads"
          and st.expansion_lists_from_database == true, "its lists, the database's")
        T.ok(st.loaded_expansion_preset_index == 1 and st.expansion == "Delta", "the preset loaded")
        local asked = false
        for _, r in ipairs(S.recognized) do if r.name == "expansion area" and r.snapshot then asked = true end end
        T.ok(asked and T.count("the area read: Delta, by its presets named (18)") == 1, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.words({ ["expansion area"] = "Bass Leads Pads" })
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.expansion_open == true and st.selected_expansion == "" and st.expansion_categories == nil, "open, not named")
    "##);
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.words({ ["expansion view"] = "4 per Row", ["expansion area"] = "Bass Leads Pads" })
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.expansion_view == "4 per Row" and st.expansion_open == false, "the view")
        for _, r in ipairs(S.recognized) do T.ok(r.name ~= "expansion area", "no area read") end
    "##);
}

/// One press at a time: a second press while the first one's reads are out does nothing; and a
/// press whose reads come back after the user moved on — the overlay left the front — shows no
/// page.
#[test]
fn a_press_while_the_reads_are_out_does_nothing_and_a_page_is_not_shown_after_the_user_moved_on() {
    run_mac(r##"
        local S = T.S
        local W = select(3, T.avengerAt80("Init Preset"))
        T.pageReady()
        local snaps = S.snaps
        T.pageKey()
        T.pageKey()
        T.ok(S.snaps == snaps + 1, "one picture")
        T.ok(T.count("[avenger] [page] the key again while the last press's reads are out: nothing done") == 1, T.dump())
        T.tick()
        T.ok(#T.page().shows == 1, "shown once")
        -- In the page and back: then the key, and the user leaves before the reads are in.
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
        T.event()
        T.pageKey()
        S.listed = { W, T.OTHER }
        T.show(T.OTHER)
        T.tick()
        T.ok(#T.page().shows == 1, "not shown over another window")
        T.ok(T.count("[avenger] [page] not shown: the user moved on while Avenger was read — ") == 1, T.dump())
    "##);
}

/// In a calibrating run, the open writes one marked picture per part read and zoom, of the picture
/// the reads were answered from — strip, macros, knob, centre and drumsq, each `-<zoom>pct.png` —
/// with a crosshair on every point read, written with callbacks; a second open at the same zoom
/// writes none, and without calibration none is written.
#[test]
fn a_calibrating_run_writes_a_marked_picture_per_part_and_zoom() {
    run_mac(r##"
        local S = T.S
        local W = select(3, T.avengerAt80("Init Preset"))
        T.pageReady()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "drums", central = "drum_sq", mode = "drum",
          sq = { chain = true, selected = "b" }, amounts = { 10, 20, 30 } })
        -- Back to Avenger from the page, identified again in its new stay.
        local function back()
          S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
          T.pageKey()
          T.event()
          T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 1.6, 80))
          T.event()
        end
        local function pictures()
          local out = {}
          for _, m in ipairs(S.marked) do
            if not string.find(m.path, "calibration/header", 1, true) then out[#out + 1] = m end
          end
          return out
        end
        T.openPage()
        T.ok(#pictures() == 0, "nothing without calibration")
        back()
        T.calibrate(true)
        T.openPage()
        T.tick()
        local names = {}
        for _, m in ipairs(pictures()) do
          T.ok(m.snapshot ~= nil and #m.marks > 0, m.path)
          names[#names + 1] = string.match(m.path, "calibration/(.+)$")
        end
        T.ok(table.concat(names, " ") == "strip-80pct.png macros-80pct.png knob-80pct.png centre-80pct.png drumsq-80pct.png",
          table.concat(names, " "))
        T.ok(T.count("[avenger] [calibrate] strip picture, (") == 1 and T.count("[avenger] [calibrate] drumsq picture") == 1,
          T.dump())
        back()
        T.openPage()
        T.tick()
        T.ok(#pictures() == 5, "the same zoom again: none")
    "##);
}

/// The turns of Luau's loops and calls each read of the open may spend at 200 % (T.interrupts),
/// by the reader's file and function: the most either scenario below spent when measured on
/// 2026-10-12, plus a margin of READ_MARGIN — fewer turns than the pixels of the smallest box any
/// read looks at there (AUTO FOLLOW's, 44 by 32 pixels; the lock and the pattern switch, 40 by 40;
/// a Mute, 52 by 32; FAV, 100 by 40), so that a loop in Luau over any box's pixels, at least one
/// turn a pixel, is more than its read may spend. A read not listed may spend READ_MARGIN.
const READ_MARGIN: u64 = 1000;
const READ_TURNS: &[(&str, u64)] = &[
    ("strip count", 1065),
    ("strip decide", 9258),
    ("centre decide", 721),
    ("drumsq decide", 18957),
    ("macros selected", 611),
    ("macros buttons", 995),
    ("expansion favorite", 2133),
    ("knob macro", 7590),
    ("knob osc", 10416),
];

/// What the scenarios below share: the reads' functions counted (`spent`, by "file function"), and
/// the check of what each spent against READ_TURNS.
const SPENT: &str = r#"
local spent = {}
local function count(file, names)
  local M = T.S.included["modules/vps-avenger/src/read/" .. file .. ".luau"][1]
  for _, n in ipairs(names) do
    local f = M[n]
    M[n] = function(...)
      local t0 = T.interrupts()
      local out = table.pack(f(...))
      spent[file .. " " .. n] = (spent[file .. " " .. n] or 0) + T.interrupts() - t0
      return table.unpack(out, 1, out.n)
    end
  end
end
local function counted()
  count("strip", { "count", "decide", "namesFromWords" })
  count("centre", { "decide" })
  count("drumsq", { "decide" })
  count("macros", { "selected", "buttons", "labels", "count" })
  count("expansion", { "favorite", "fromArea", "parseView" })
  count("knob", { "macro", "osc", "oscMiddles" })
end
local function within(bounds)
  local words = {}
  for name, n in pairs(spent) do
    words[#words + 1] = name .. " " .. n
    local may = (bounds[name] or 0) + @MARGIN@
    T.ok(n <= may, string.format("%s spent %d turns; it may spend %d", name, n, may))
  end
  table.sort(words)
  return table.concat(words, ", ")
end
"#;

/// READ_TURNS as a Luau table.
fn read_turns() -> String {
    let fields = READ_TURNS.iter().map(|(k, v)| format!("[\"{k}\"] = {v}")).collect::<Vec<_>>().join(", ");
    format!("{{ {fields} }}")
}

/// No loop over pixels on the event loop: at 200 % — the largest Avenger, whose macro knob alone
/// spans 280 by 280 pixels (78,400 of them) — the whole open, from the key to the show, turns
/// Luau's loops and calls about 35,000 times with an oscillator selected and 48,000 with the Drums
/// tab and DRUM SQ, whose switches only that reads: each under a bound with a margin for small
/// changes that a loop over the strip's dots band (about 32,700 pixels at 200 %) or the macro
/// buttons (about 21,000) passes; and each read spends at most what READ_TURNS lets it, which a
/// loop over its smallest box passes. The points it reads are a few hundred at a time, and the
/// grids the host's own limit. On Windows, on a display at 100 % scaling and a screen that holds
/// all of Avenger, so that every part is read — and the reads come out as at 80 %.
#[test]
fn the_open_reads_loop_over_no_pixels_in_luau() {
    let scenario = |paint: &str, check: &str, limit: &str| {
        r##"
        local S = T.S
        S.screenSize = { w = 3840, h = 2400 }
        T.onWindows(200, 1)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 4, 200))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        @SPENT@
        counted()
        T.paintAvenger(248, 60, 4, @PAINT@)
        T.words({ ["macro tabs"] = "MACRO 1 MACRO 2 MACRO 3" })
        -- The key's handler — the picture and the questions — and then the answers' callbacks up to
        -- the state: the reads' own work, without the runtime's tick that runs in the same turn.
        local before = T.interrupts()
        T.pageKey()
        local turns = T.interrupts() - before
        S.turnsAtAnswers, S.turnsAtState = false, false
        T.tick()
        turns += S.turnsAtState - S.turnsAtAnswers
        local st = T.lastPost("state").state
        local o, m = st.osc_panel, st.macro_panel
        @CHECK@
        T.ok(m.count == 3 and m.selected_index == 3 and m.amount_values[1] == 30 and m.amount_values[2] == 60
          and m.amount_values[3] == 90 and m.button_states[1] == true and m.button_states[2] == true, "the macros at 200 %")
        T.ok(T.lastPost("error").text == "" and T.count("cut at the edge") == 0, "nothing cut")
        local most, all = 0, 0
        for _, n in ipairs(S.pointLists) do
          most = math.max(most, n)
          all += n
        end
        T.ok(most <= 400 and all <= 3000, string.format("points: at most %d at a time, %d in all", most, all))
        local each = within(@READS@)
        T.ok(turns < @LIMIT@, string.format("%d turns of Luau's loops and calls for the open, over %d grids of %d cells, "
          .. "%d profiles and %d points; the reads: %s", turns, S.cellsAsked, S.cellsTotal, S.profilesAsked, all, each))
        "##
        .replace("@SPENT@", &SPENT.replace("@MARGIN@", &READ_MARGIN.to_string()))
        .replace("@READS@", &read_turns())
        .replace("@PAINT@", paint)
        .replace("@CHECK@", check)
        .replace("@LIMIT@", limit)
    };
    // Measured 2026-10-12: 34,873 turns, of 12 grids of 4400 cells, 7 profiles and 1552 points.
    run(&scenario(
        r#"{ osc = 8, selected = "osc", index = 5, mute = { [8] = true }, solo = { [1] = true }, trigSolo = true,
          macroShown = 3, buttons = { true, true }, amounts = { 30, 60, 90 }, volume = 50, transpose = -24, fine = 0 }"#,
        r#"T.ok(o.count == 8 and o.selected_index == 5 and o.mute_states[8] == true and o.solo_states[1] == true
          and o.mute_states[1] == false and o.trig_solo == true, "the strip at 200 %")
        T.ok(o.volume_values[5] == 50 and o.transpose_values[5] == -24 and o.fine_tune_values[5] == 0,
          string.format("the knobs at 200 %%: %s %s %s", tostring(o.volume_values[5]), tostring(o.transpose_values[5]),
            tostring(o.fine_tune_values[5])))"#,
        "40000",
    ));
    // Measured 2026-10-12: 47,999 turns, of 12 grids of 4400 cells, 7 profiles and 1028 points.
    run(&scenario(
        r#"{ osc = 8, selected = "drums", pattern = true, lock = true, mode = "sq", central = "drum_sq",
          sq = { chain = true, follow = false, on = { b = true }, selected = "a" }, macroShown = 3,
          buttons = { true, true }, amounts = { 30, 60, 90 } }"#,
        r#"T.ok(o.selected_type == "drums" and o.drum_pattern == true and o.drum_lock == true and o.drum_mode == "sq"
          and o.central_panel_tab == "drum_sq" and o.drum_sq_pattern_chain == true and o.drum_sq_pattern_selected == "a"
          and o.drum_sq_pattern_states[1] == true, "the Drums tab and DRUM SQ at 200 %")"#,
        "53000",
    ));
}

/// Avenger at 200 % on a Mac, larger than the 2560 x 1440 screen its corner is on: the picture is
/// the part the screen shows — one snapshot of the whole panel, 3485 x 2244 points, would pass what
/// one may hold there (6,710,886 points) — and what lies past the edge is not read: the macros,
/// whose tab captions are no text of the picture, and Trig's Solo, cut by the edge. The questions
/// are asked of what the picture holds of their regions, so the rest reads as at 80 %; the log says
/// where the picture was cut, and the page's error region that part of Avenger was not read.
#[test]
fn on_a_mac_avenger_larger_than_the_screen_is_read_as_far_as_the_screen_shows_it() {
    run_mac(r##"
        local S = T.S
        local W = T.float(200)
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(98, 84, 4, 200))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.paintAvenger(98, 84, 4, { osc = 8, selected = "osc", index = 5, mute = { [8] = true }, solo = { [1] = true },
          trigSolo = true, macroShown = 3, buttons = { true, true }, amounts = { 30, 60, 90 }, volume = 50,
          transpose = -24, fine = 0 })
        T.words({ ["macro tabs"] = "MACRO 1 MACRO 2 MACRO 3" })
        T.openPage()
        T.ok(#T.page().shows == 1, "shown")
        T.ok(T.count("[avenger] [read] the picture: (100,82)-(2560,1440), cut at the edge of the 2560x1440 screen — the "
          .. "panel ends at 3585,2326; what lies past the edge is not read") == 1, T.dump())
        for _, r in ipairs(S.recognized) do
          if r.snapshot then
            local g = r.snapshot.region
            T.ok(g[1] == 100 and g[2] == 82 and g[3] == 2560 and g[4] == 1440, "the text read of the cut picture")
          end
        end
        local st = T.lastPost("state").state
        local o, m = st.osc_panel, st.macro_panel
        T.ok(o.count == 8 and o.selected_index == 5 and o.mute_states[8] == true and o.solo_states[1] == true
          and o.mute_states[1] == false, "the strip, on the screen")
        T.ok(o.trig_solo == nil, "Trig's Solo, cut by the edge, not read")
        T.ok(o.volume_values[5] == 50 and o.transpose_values[5] == -24 and o.fine_tune_values[5] == 0, "the knobs")
        T.ok(m.count == nil and m.selected_index == nil and m.amount_values[1] == nil and m.amount_values[3] == nil
          and m.button_states[1] == nil, "the macros, past the edge, not read")
        T.ok(T.lastPost("error").text == "At 200 percent, part of VPS Avenger lies past the edge of the screen: what lies "
          .. "there was not read.", T.lastPost("error").text)
    "##);
}

/// Where the key's reads show no page, the user hears why: on a Mac, Avenger at 200 % with its
/// corner on another display than the one the host names, whose panel no snapshot may hold — the
/// snapshot raises for the module's budget — and a capture that fails. The next press reads again.
/// The user who moved on hears nothing (the test of one press at a time).
#[test]
fn a_press_whose_reads_show_no_page_says_why() {
    run_mac(r##"
        local S = T.S
        local W = T.float(200)
        W.bounds.x, W.client.x = 2700, 2700
        S.listed = { W }
        T.at(W)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(2698, 84, 4, 200))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        T.pageKey()
        T.tick()
        T.ok(T.lastSaid() == "The page did not open: VPS Avenger could not be read now." and #T.page().shows == 0, T.said())
        T.ok(T.count("[avenger] [page] not shown: no picture of (2700,82)-(6185,2326): ") == 1
          and T.count("the limit is 128 MiB per module VM") == 1, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        S.snapFails = "screen capture failed"
        local n = #S.speech
        T.pageKey()
        T.ok(#S.speech == n + 1 and T.lastSaid() == "The page did not open: VPS Avenger could not be read now.", T.said())
        T.ok(T.count("[avenger] [page] not shown: no picture of (100,82)-(1494,980): screen capture failed") == 1, T.dump())
        S.snapFails = nil
        T.openPage()
        T.ok(#T.page().shows == 1, "the next press reads again")
    "##);
}

/// A page the key builds — pressed before it was ever ready, so its state is posted before it has
/// loaded and waits for the load — is not told that its reads are old: the page's own `ready`,
/// which comes as it loads, is answered with the reads the press made and no age, and the host's
/// onReady of that first load posts nothing more.
#[test]
fn a_page_the_key_builds_is_not_told_its_fresh_reads_are_old() {
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.nameIs("LD Lead 1")
        T.openPage()
        T.ok(T.lastSaid() == "Building the page." and #T.page().shows == 1, T.said())
        local st = T.lastPost("state").state
        T.ok(st.preset == "LD Lead 1" and st.read_age_seconds == nil, "the fresh state, waiting for the load")
        S.now += 1500
        local n = #T.page().posts
        T.pageMsg({ action = "ready" })
        st = T.lastPost().state
        T.ok(#T.page().posts == n + 1 and st.preset == "LD Lead 1" and st.read_age_seconds == nil, "ready: no age")
        T.pageReady()
        T.ok(#T.page().shows == 2 and #T.page().posts == n + 1, "shown, and nothing posted again")
    "##);
}

/// The module disabled and enabled again while a press's questions are out: they are dropped and
/// their callbacks never come — and the next press reads Avenger again, rather than finding the last
/// press's reads still out for good.
#[test]
fn a_press_after_a_disable_dropped_the_last_ones_questions_reads_again() {
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        local snaps = S.snaps
        T.pageKey()
        T.ok(S.snaps == snaps + 1 and #S.screenOut > 0, "the questions are out")
        T.dropScreen()
        T.pageKey()
        T.ok(S.snaps == snaps + 2, "read again")
        T.ok(T.count("the key again while the last press's reads are out") == 0, T.dump())
        T.tick()
        T.ok(#T.page().shows == 1, "shown")
    "##);
}

// ---------------------------------------------------------------------------------------------
// The protocol: the page's list of actions against what it sends, and against the module.
// ---------------------------------------------------------------------------------------------

const PANEL_JS: &str = include_str!("../../../modules/vps-avenger/ui/panel.js");
const INDEX_HTML: &str = include_str!("../../../modules/vps-avenger/ui/index.html");

/// Quoted names in `ui/panel.js` that are not actions: the arguments its actions carry — menu
/// items, controls, central tabs, drum slot columns — written the same way. A name added to the
/// script is either an action on ACTIONS or one of these.
const NOT_ACTIONS: &[&str] = &[
    "copy_first_half", "copy_sample_to", "copy_second_half", "copy_slot_to", "drum_sq",
    "edit_sample", "export_to_midi", "fine_tune", "high_pass", "high_shelf", "import_midi",
    "lock_all", "low_pass", "low_shelf", "master_fx", "mod_env", "paste_to_first_half",
    "paste_to_second_half", "random_pitch", "remove_sample", "reset_pan", "reset_pitch",
    "reset_shuffle", "reset_vel_amount", "reset_volume", "save_drumkit_as", "save_sequence_as",
    "send_rack", "shaper_1", "step_sq", "swap_sample_with", "swap_slot_with", "unlock_all",
    "vel_amount",
];

/// The names on `const ACTIONS = [ … ];` in the script.
fn page_actions() -> Vec<String> {
    let start = PANEL_JS.find("const ACTIONS = [").expect("panel.js has its ACTIONS");
    let rest = &PANEL_JS[start + "const ACTIONS = [".len()..];
    let end = rest.find("];").expect("ACTIONS ends");
    rest[..end]
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Every single-quoted string in `text` shaped like an action's name: lower-case words joined by
/// underscores, at least two of them. With the offset it starts at.
fn quoted_names(text: &str) -> Vec<(usize, String)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    for i in 0..b.len() {
        if b[i] != b'\'' {
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && (b[j].is_ascii_lowercase() || b[j].is_ascii_digit() || b[j] == b'_') {
            j += 1;
        }
        if j >= b.len() || b[j] != b'\'' || j == i + 1 {
            continue;
        }
        let name = &text[i + 1..j];
        let shaped = name.as_bytes()[0].is_ascii_lowercase()
            && name.contains('_')
            && name.split('_').all(|w| !w.is_empty());
        if shaped {
            out.push((i, name.to_string()));
        }
    }
    out
}

/// Every `data-action="…"` of the markup.
fn data_actions() -> Vec<String> {
    INDEX_HTML
        .match_indices("data-action=\"")
        .map(|(i, m)| {
            let rest = &INDEX_HTML[i + m.len()..];
            rest[..rest.find('"').expect("the attribute ends")].to_string()
        })
        .collect()
}

/// The page's list against what it sends: every name the script quotes as an action and every
/// `data-action` of the markup is on ACTIONS, every name on ACTIONS is sent from somewhere — a
/// quote outside the list, or the markup — and the list holds the tool's 79 shared actions, its
/// two Windows-only ones left out. Then the module: every name on ACTIONS has the words of its
/// answer, and either a handler or a place on the explicit list of actions not built yet, never
/// both; and nothing the module lists is a name the page does not send. Sent while the page has
/// the keyboard, each one not built is answered with its words, and each one with a handler is not.
#[test]
fn every_action_the_page_sends_is_on_its_list_and_answered_by_the_module() {
    let actions = page_actions();
    assert_eq!(actions.len(), 79, "the tool's 79 shared actions: {actions:?}");
    let mut sorted = actions.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), actions.len(), "ACTIONS names an action twice");
    for gone in ["toggle_windows_reaper_rect_x", "align_open_expansion"] {
        assert!(!actions.iter().any(|a| a == gone), "{gone} is the Windows-only block's, left out");
    }
    let list_start = PANEL_JS.find("const ACTIONS = [").unwrap();
    let list_end = list_start + PANEL_JS[list_start..].find("];").unwrap();
    let quoted = quoted_names(PANEL_JS);
    for (at, name) in &quoted {
        if (list_start..list_end).contains(at) {
            continue;
        }
        assert!(
            actions.contains(name) || NOT_ACTIONS.contains(&name.as_str()),
            "panel.js quotes '{name}', which is neither on ACTIONS nor an argument this test knows"
        );
    }
    let markup = data_actions();
    assert!(!markup.is_empty(), "the markup has its data-action buttons");
    for name in &markup {
        assert!(actions.contains(name), "index.html sends '{name}', which is not on ACTIONS");
    }
    for name in &actions {
        let sent = markup.contains(name)
            || quoted.iter().any(|(at, q)| q == name && !(list_start..list_end).contains(at));
        assert!(sent, "'{name}' is on ACTIONS and sent from nowhere");
    }
    for name in NOT_ACTIONS {
        assert!(!actions.iter().any(|a| a == name), "'{name}' is an action and an argument");
    }

    let list = actions.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ");
    run_mac(
        &r##"
        local S = T.S
        local ACTIONS = { @ACTIONS@ }
        T.avengerAt80("Init Preset")
        local P = S.included["modules/vps-avenger/src/page.luau"][1]
        local onList, notBuilt = {}, {}
        for _, a in ipairs(ACTIONS) do onList[a] = true end
        for _, a in ipairs(P.NOT_BUILT) do
          T.ok(notBuilt[a] == nil, "not built twice: " .. a)
          notBuilt[a] = true
          T.ok(onList[a], "the module lists '" .. a .. "' as not built, and the page does not send it")
        end
        for a in pairs(P.HANDLERS) do T.ok(onList[a], "a handler for '" .. a .. "', which the page does not send") end
        for a in pairs(P.LABELS) do T.ok(onList[a], "words for '" .. a .. "', which the page does not send") end
        for _, a in ipairs(ACTIONS) do
          T.ok(type(P.LABELS[a]) == "string" and P.LABELS[a] ~= "", "no words for '" .. a .. "'")
          T.ok((P.HANDLERS[a] ~= nil) ~= (notBuilt[a] == true), "'" .. a .. "' has a handler or is not built yet, one of them")
        end
        T.pageReady()
        T.openPage()
        for _, a in ipairs(ACTIONS) do
          local n = #T.page().posts
          T.pageMsg({ action = a, args = {}, focused = true })
          local said = P.LABELS[a] .. " is not available in this module yet"
          if notBuilt[a] then
            T.ok(#T.page().posts == n + 1 and T.lastPost().kind == "error" and T.lastPost().text == said, a)
          else
            T.ok(not (T.lastPost() and T.lastPost().text == said and #T.page().posts > n), a .. " has a handler")
          end
        end
    "##
        .replace("@ACTIONS@", &list),
    );
}
