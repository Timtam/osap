//! VPS Avenger's page (`modules/vps-avenger/src/page.luau`, `ui/`): the avenger_control project's
//! panel shown with `host.gui.webview` beside Avenger, as the maintainer decided on 2026-10-12, on
//! the overlay runtime's page support (`O:addPage`, `O:inPlugin`). The module is loaded for real
//! against the scripted host of `overlay_avenger_tests.rs`, with the runtime and its scripted page
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
//! (the scripted snapshot keeps the host's budget); every question waited for in the key's handler,
//! so one press at a time and nothing left out for a disable to drop; and a bound on the Luau work
//! of the open and of each read, which a loop over any box's pixels passes.
//!
//! And the header's actions (slice 4, `src/actions.luau`), each a trip from a scripted page message
//! as the ring's scenarios play Avenger: ◀ and ▶ with the name watched and "unchanged" said — and
//! on Init Preset, which Avenger steps no preset from, nothing sent and why said — Load
//! with Escape in its file dialog, Save as, Initialize's Yes — its warning box read before YES is
//! clicked where it was read, then everything read again — and No, which clicks nothing; UNDO, the
//! redo list ended by its menu's close, any size from the zoom list; a trip that may have changed
//! the preset and read no name after it, which posts the preset as not read; and a second message
//! while a trip is out, dropped.
//!
//! And the protocol, with no JS run: every action name `ui/panel.js` sends and every `data-action`
//! of `ui/index.html` is on the page's own list, `ACTIONS`, and every name on it has a handler in
//! the module or is on its explicit list of actions not built yet; and Back and Next carry
//! aria-disabled, set from the state, and stay in the Tab order.

use super::overlay_avenger_tests::{run, run_mac};

/// The page key on a Mac: the tool's chord, Control-Shift-Command-F1 (`Ctrl` is Command and `Win`
/// is Control in a key spec there).
const MAC_KEY: &str = "Ctrl+Shift+Win+F1";

/// Described once at load, as a tool page that activates; built as Avenger is identified, before
/// any key; and described again — the old one closed — when the Mac page mode changes, and only
/// then. The six settings — the plan's §2.9 and the Expansion View stored — with their defaults:
/// the expansions list the tool's own default, "Factory Standard", of the tool's four, and the view
/// "8 per Row", the tool's AVGExpansionView's.
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
          and S.settings.pageLeavesDawActive == false and S.settings.drumSqCadence == 250
          and S.settings.expansionView == "8 per Row", "the defaults")
        T.ok(table.concat(S.settingDefs.roster.opts.oneOf, "|") == "Factory Standard|Complete|Nyhdroe|Joyman",
          "the rosters, the tool's default first")
        T.ok(table.concat(S.settingDefs.expansionView.opts.oneOf, "|") == "List|1 per Row|2 per Row|3 per Row|4 per Row|"
          .. "5 per Row|6 per Row|7 per Row|8 per Row", "the views")
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
        -- Loaded: `open` asked afresh, and the page shown on the turn its reads answer.
        T.pageReady()
        T.tick()
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
/// in front shows them again when it comes forward — and taken as closed only when it refuses, or
/// when the focus answers `gone`; a window that would not come keeps the page, saying why in its
/// error region; the sounds follow their setting.
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
          and st.macro_panel.selected_index == nil, "the rest not read")
        -- Nothing read in the browser's header: no expansion open, the view the one stored.
        T.ok(st.expansion_open == false and st.expansion_view == "8 per Row" and st.expansion_view_from == "stored",
          "the view stored")
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
        -- Back on Avenger — its stay held across the page, so it is not identified again — the key
        -- shows the page where the user was.
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
        -- The host's own `gone` for a window still listed — it went between the listing and the focus:
        -- closed, by its code; and `hidden`, a window that is there, would not come.
        S.focusErr = T.E("gone", "no such window")
        from = #S.pageOrder
        T.pageKey()
        T.ok(T.pageSince(from) == "focus 31,post error", T.pageSince(from))
        T.ok(T.lastPost("error").text == "The plug-in window was closed. Please open it again, then press @KEY@ again.",
          T.lastPost("error").text)
        S.focusErr = T.E("hidden", "the window is hidden")
        T.pageKey()
        T.ok(T.lastPost("error").text == "VPS Avenger's window would not come to the front.", T.lastPost("error").text)
        S.focusErr = nil
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
        T.runDue()
        T.ok(header.active and T.page().prepares == 1, "identified, and the page built on the next turn")
        S.listed = { W, T.OTHER }
        T.show(T.OTHER)
        T.ok(not header.active)
        local n = #S.speech
        T.pageKey()
        T.ok(#S.speech == n + 1 and T.lastSaid() == said and #T.page().shows == 0, T.said())
    "##.replace("@KEY@", MAC_KEY).as_str());
}

/// `ready` — the page loaded, or reloaded itself — gets one state message, then the settings
/// (avenger_settings_tests.rs): before the key, with nothing read; after it, the reads already made,
/// with how many seconds ago they were made once the page has reloaded itself since they were posted
/// to it — and so does the host's onReady of the reload, since the page's own `ready` may come
/// first. The expansions come from the preset
/// database's roster, unmarked — "Factory Standard" unless the setting says otherwise — and the
/// roster setting's change is posted at once.
#[test]
fn ready_gets_one_state_message_from_the_reads_already_made() {
    run_mac(r##"
        local S = T.S
        T.presets()
        T.avengerAt80("Init Preset")
        local n = #T.page().posts
        T.pageMsg({ action = "ready" })
        T.ok(#T.page().posts == n + 2 and T.page().posts[n + 1].kind == "state" and T.lastPost().kind == "settings",
          "one state, then the settings")
        local st = T.lastPost("state").state
        T.ok(st.preset == nil and st.zoom == nil and st.read_age_seconds == nil, "nothing read before the key")
        T.ok(table.concat(st.expansions, "|") == "Alpha" and st.expansions_from_database == nil,
          "the database's roster, the tool's default, unmarked")
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
        T.ok(#T.page().posts == n + 2, "one state, then the settings")
        st = T.lastPost("state").state
        T.ok(st.preset == "LD Lead 1" and st.zoom == 80 and st.read_age_seconds == 5, tostring(st.read_age_seconds))
        T.setting("roster", "Complete")
        st = T.lastPost("state").state
        T.ok(table.concat(st.expansions, "|") == "Alpha|Beta|Gamma|Delta",
          "the roster changed: " .. table.concat(st.expansions, "|"))
    "##);
}

/// While the page has the keyboard, a name the page does not have is answered in its error region
/// with the action's own words — "not available in this module yet" — and the log calls it so (since
/// the browser, every action of the page's has a handler); Initialize's No closes the page's dialog
/// and clicks nothing, and says nothing. A message while the page does not have the keyboard is not acted on, a problem
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
/// waited for in the key's handler; every grid and profile of it asked there at once with a
/// callback, and beside them the text read of what needs none of their answers (A19, the runtime's
/// page answered later: `openLater`); nothing posted or shown in the key's handler. On the turn the
/// answers come, the rest of the text read and the knobs' two grids in the last answer's handler,
/// then the state, the error region, the focus and the show. The state carries
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
        local snaps, from, asked = S.snaps, #S.pageOrder, S.cellsAsked + S.profilesAsked
        T.pageKey()
        T.ok(S.snaps == snaps + 1, "one picture, in the key's handler")
        -- The picture's 28 (read/open.luau, the asks: among them DRUM SQ's patterns' rows, one
        -- profile each, FAV's two grids of the star column, and the browser's picture test's two),
        -- every one with a callback, and the first text read beside them.
        T.ok(#S.screenOut == 28 and S.cellsAsked + S.profilesAsked == asked + 28 and #S.pageReads == 1,
          "every question asked at once: " .. #S.screenOut .. ", " .. (S.cellsAsked + S.profilesAsked - asked))
        T.ok(T.pageSince(from) == "", "nothing posted or shown in the key's handler: " .. T.pageSince(from))
        local early = {}
        for _, e in ipairs(S.pageReads[1].what) do early[#early + 1] = e.name end
        T.ok(table.concat(early, ",") == "preset,expansion view,expansion name,macro tabs,macro macro1,macro macro2,"
          .. "macro macro3,macro btn1,macro btn2,central tabs", "the regions that need no answer: " .. table.concat(early, ","))
        T.tick()
        -- Then a grid for the three macro knobs' rings and one for the oscillator's three
        -- (read/knob.luau), in the last answer's handler.
        T.ok(S.cellsAsked + S.profilesAsked == asked + 30, "the knobs' grids after: " .. (S.cellsAsked + S.profilesAsked - asked))
        T.ok(T.pageSince(from) == "post state,post error,post focus,show,sound sounds/Open.wav", T.pageSince(from))
        asked = S.cellsAsked + S.profilesAsked
        T.tick()
        T.ok(S.snaps == snaps + 1 and S.cellsAsked + S.profilesAsked == asked, "nothing asked again")
        local texts = 0
        for _, r in ipairs(S.recognized) do if r.snapshot then texts += 1 end end
        -- The browser's header's name among them (read/expansion.luau, E.entries): ten in the first
        -- read, the three OSC tabs' names in the second.
        T.ok(texts == 13, "two text reads of the picture, a region per name: " .. texts)
        local st = T.lastPost("state").state
        -- FAV is read only while an expansion reads open (read/expansion.luau, E.favorite).
        T.ok(st.preset == "LD Lead 1" and st.zoom == 80 and st.expansion_view == "8 per Row"
          and st.expansion_open == false and st.selected_expansion == "" and st.expansion_favorite_view == nil,
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
          "[avenger] [read] expansion: FAV not read — no expansion reads open", "[avenger] [read] words: preset 'LD Lead 1'",
          "[avenger] [read] 28 questions of one picture; the picture ", "[avenger] [page] shown beside" }) do
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
        T.ok(st.expansion_view == "8 per Row" and st.expansion_favorite_view == nil, "the view, and no FAV with the grid shown")
    "##.replace("@WORDS@", WORDS));
}

/// With the Drums tab selected: its pattern switch, its lock and its mode (the project's rule —
/// DRUMKIT's caption the brighter is SEQUENCE), the central tab, and DRUM SQ on it: PATTERN CHAIN
/// by its brightness, the patterns that are on by their red, the selected one by its outline. One
/// macro tab, counted from a caption with no number, is the one shown. And with the preset database
/// installed: the preset placed in the catalog gives the expansion, and the names its tabs have
/// there are the vocabulary the names read are checked against — "Laed" is the catalog's "Lead",
/// "Tane" its "Tone" — posted in the catalog's spelling and unmarked, as everything the database
/// gives since the maintainer's decision of 2026-10-26; the log says which the database replaced.
#[test]
fn the_drums_tab_drum_sq_and_the_database_vocabulary_are_read() {
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
        T.ok(st.preset == "AR Band Pass Slide" and st.expansion == "Alpha" and st.expansion_from_database == nil,
          "the expansion, the database's: " .. tostring(st.expansion))
        T.ok(st.expansion_view == "List" and st.expansion_open == false, "the view")
        local o = st.osc_panel
        T.ok(table.concat(o.names, "|") == "Arp|OSC 2|Lead" and o.names_from_database == nil, table.concat(o.names, "|"))
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
        T.ok(table.concat(m.tabs[1], "|") == "Tone|Macro 2|Drive|+12|MacroBtn 2" and m.labels_from_database == nil,
          table.concat(m.tabs[1], "|"))
        T.ok(m.button_states[1] == false and m.button_states[2] == true, "the buttons")
        T.ok(m.amount_values[1] == 0 and m.amount_values[2] == 50 and m.amount_values[3] == 100,
          string.format("the knobs: %s %s %s", tostring(m.amount_values[1]), tostring(m.amount_values[2]),
            tostring(m.amount_values[3])))
        T.ok(T.count("3 'Laed' -> 'Lead', the database's (0.75 alike)") == 1, T.dump())
        T.ok(T.count("[avenger] [read] drum sq: PATTERN CHAIN mean ") == 1, T.dump())
    "##);
}

/// The maintainer's decision of 2026-10-26, in place of 2026-09-28's mark: what the preset database
/// gives is shown on the page as it is. The state carries no key that marks a value as the
/// database's — the expansion the preset is in, the names its spelling replaced in the OSC tabs and
/// the macro slots, an open expansion's lists, the roster — and the script says "from the database"
/// nowhere: the headings, the expansions' list, the open expansion's lists and the macro slots say
/// their words alone, and a count the dots did not give is the count alone where it is the
/// database's, "as confirmed" where the user confirmed it.
#[test]
fn the_pages_state_and_script_mark_nothing_as_the_databases() {
    run_mac(r##"
        local S = T.S
        T.presets()
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.paintAvenger(98, 84, 1.6, { osc = 3, selected = "osc", index = 1, buttons = { false, true },
          amounts = { 0, 50, 100 } })
        T.words({ preset = "AR Band Pass Slide", ["expansion name"] = "Alpha",
          ["osc 1"] = "Arp", ["osc 2"] = "OSC 2", ["osc 3"] = "Laed", ["macro tabs"] = "Macro",
          ["macro macro1"] = "Tane", ["macro macro2"] = "Macro 2", ["macro macro3"] = "Drive",
          ["macro btn1"] = "+12", ["macro btn2"] = "MacroBtn 2" })
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.expansion == "Alpha" and table.concat(st.osc_panel.names, "|") == "Arp|OSC 2|Lead"
          and st.macro_panel.tabs[1][1] == "Tone" and st.expansion_open == true and st.selected_expansion == "Alpha"
          and #st.expansion_categories > 0 and #st.expansions > 0, "the database's values, posted: " .. T.jsonEncode(st))
        local json = T.jsonEncode(st)
        T.ok(string.find(json, "from_database", 1, true) == nil, json)
        T.ok(T.count("3 'Laed' -> 'Lead', the database's (0.75 alike)") == 1, "the log still says it: " .. T.dump())
    "##);
    let js = PANEL_JS.replace("\r\n", "\n");
    for gone in ["from the database", "from_database", "FromDatabase"] {
        assert!(!js.contains(gone), "panel.js still marks a value as the database's: {gone}");
    }
    assert!(
        js.contains("label: `${count} OSC tab${count === 1 ? '' : 's'}${oscPanel.count_from === 'confirmed' ? ', as confirmed' : ''}`"),
        "the count column: the database's alone, a confirmed one so"
    );
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
        // Under a Solo (src/read/strip.luau, Strip.silencedBy).
        "silenced by ${by[0]}'s Solo", "silenced by the Solos of ",
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
        T.ok(st.preset == nil and st.zoom == 80, "the header not read, but the zoom")
        -- Nothing read in the browser's header is the grid shown (read/browser.luau, Br.state): the
        -- view posted is the one stored, and said as that.
        T.ok(st.expansion_open == false and st.selected_expansion == "" and st.expansion_view == "8 per Row"
          and st.expansion_view_from == "stored", "the browser: the view stored")
        local o, m = st.osc_panel, st.macro_panel
        T.ok(o.count == nil and o.names == nil and o.mute_states == nil and o.selected_type == nil
          and o.volume_values == nil, "the strip not read")
        T.ok(o.drums_mute == false and o.trig_solo == false, "a switch on a picture shows off")
        T.ok(m.count == nil and m.selected_index == nil and m.tabs == nil and m.amount_values[1] == nil,
          "the macros not read")
        T.ok(T.count("[avenger] [read] expansion: the header reads no name (blank); title band 0.0 % lit, list bar 0.0 % "
          .. "lit — the grid: no expansion open, the view 8 per Row stored") == 1, T.dump())
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

/// The browser told by its header and the picture test (read/browser.luau, Br.state): the header's
/// name read as an expansion's, that one open, with the database's lists, the preset placed in it
/// the one loaded. Nothing drawn in the header and no title in its band: no expansion open, the view
/// the one stored, "8 per Row" by default (this Avenger draws no view's label: `Expansions/Avenger
/// 100 closed.png`). A text in the header that is no expansion's name over a picture of the grid
/// settles nothing. A caption that reads a view is the grid in that view. The browser's area, its
/// categories and its preset rows are never read as text (the maintainer's decision of 2026-10-25).
#[test]
fn an_open_expansion_is_read_from_its_header_and_the_grid_from_the_header_and_its_picture() {
    let none_below = r##"
        for _, r in ipairs(S.recognized) do
          T.ok(r.name ~= "browser area" and r.name ~= "categories" and r.name ~= "presets", "nothing below the header read")
        end
    "##;
    run_mac(&format!(r##"
        local S = T.S
        T.presets()
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.words({{ preset = "LD Lead 2", ["expansion name"] = "Delta" }})
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.expansion_open == true and st.selected_expansion == "Delta" and st.expansion_view == nil
          and st.selected_expansion_from_database == nil, "Delta open, by its header")
        T.ok(table.concat(st.expansion_categories, "|") == "Leads" and st.expansion_lists_from_database == nil,
          "its lists, the database's, unmarked")
        T.ok(st.loaded_expansion_preset_index == 1 and st.expansion == "Delta", "the preset loaded")
        T.ok(T.count("expansion: the header reads 'Delta' — Delta open (exact)") == 1, T.dump())
        {none_below}
    "##));
    run_mac(&format!(r##"
        local S = T.S
        T.presets()
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.words({{ preset = "LD Lead 2" }})
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.expansion_open == false and st.expansion_view == "8 per Row" and st.expansion_view_from == "stored"
          and st.selected_expansion == "", "closed, the view stored: " .. tostring(st.expansion_view_from))
        {none_below}
    "##));
    run_mac(&format!(r##"
        local S = T.S
        T.presets()
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.words({{ preset = "LD Lead 2", ["expansion name"] = "Qqx" }})
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.expansion_open == nil and st.selected_expansion == nil, "not read: " .. tostring(st.expansion_open))
        T.ok(T.count("expansion: the header reads 'Qqx', no expansion's name; title band 0.0 % lit, list bar 0.0 % lit — the grid "
          .. "— not taken as closed") == 1, T.dump())
        {none_below}
    "##));
    run_mac(&format!(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        T.words({{ ["expansion view"] = "4 per Row" }})
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.expansion_view == "4 per Row" and st.expansion_view_from == nil and st.expansion_open == false, "the view")
        {none_below}
    "##));
}

/// A press while another's reads are out reads afresh, and only the latest press's answer shows the
/// page (the runtime's `openLater`); the page then having the keyboard, the key goes back without
/// reading anything; and a press whose reads end after the user moved on — another window came to
/// the front while Avenger's words were read — shows no page.
#[test]
fn one_press_at_a_time_and_a_page_is_not_shown_after_the_user_moved_on() {
    run_mac(r##"
        local S = T.S
        local W = select(3, T.avengerAt80("Init Preset"))
        T.pageReady()
        local snaps = S.snaps
        T.pageKey()
        T.ok(S.snaps == snaps + 1 and #T.page().shows == 0 and #S.pageReads == 1, "read once, its answers out")
        T.pageKey()
        T.ok(S.snaps == snaps + 2 and #S.pageReads == 2, "the second press reads afresh")
        T.tick()
        T.ok(#T.page().shows == 1 and S.front == T.PAGE_WIN
          and T.count("and the key was pressed again since — not shown") == 1, "shown once, by the latest: " .. T.dump())
        T.event()
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.ok(S.snaps == snaps + 2 and T.page().hides == 1 and S.front == W, "the press in the page goes back, reading nothing")
        T.event()
        -- The user leaves while the key's words are read.
        S.listed = { W, T.OTHER }
        local words = S.ocr
        S.ocr = function(r, name)
          if name == "preset" then
            S.front, S.chain = T.OTHER, { T.OTHER }
            S.epoch += 1
          end
          return words(r, name)
        end
        T.pageKey()
        T.tick()
        T.ok(#T.page().shows == 1, "not shown over another window")
        T.ok(T.count("[avenger] [page] not shown: the user moved on while Avenger was read — ") == 1, T.dump())
    "##);
}

/// In a calibrating run, the open writes one marked picture per part read and zoom, of the picture
/// the reads were answered from — strip, macros, knob, centre, drumsq, drums, sample, sq-controls
/// and browser, each `-<zoom>pct.png` — with a crosshair on every point read, written with
/// callbacks; a second open at the same zoom writes none, and without calibration none is written.
/// Where the browser is settled — here by its header, which reads no name — no
/// `browser-unsettled` is written; where nothing settles it, one is, once per zoom.
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
        T.ok(table.concat(names, " ") == "strip-80pct.png macros-80pct.png knob-80pct.png centre-80pct.png drumsq-80pct.png "
          .. "drums-80pct.png sample-80pct.png sq-controls-80pct.png browser-80pct.png", table.concat(names, " "))
        T.ok(T.count("[avenger] [calibrate] strip picture, (") == 1 and T.count("[avenger] [calibrate] drumsq picture") == 1,
          T.dump())
        back()
        T.openPage()
        T.tick()
        T.ok(#pictures() == 9, "the same zoom again: none")
        -- The header reading no expansion's name and the area nothing: unsettled, written once.
        back()
        T.words({ ["expansion name"] = "~" })
        T.openPage()
        T.tick()
        back()
        T.openPage()
        T.tick()
        T.ok(#pictures() == 10 and string.find(pictures()[10].path, "calibration/browser-unsettled-80pct.png", 1, true),
          tostring(pictures()[10] and pictures()[10].path))
    "##);
}

/// The turns of Luau's loops and calls each read of the open may spend at 200 % (T.interrupts),
/// by the reader's file and function: the most either scenario below spent when measured on
/// 2026-10-12 (the drumkit table's on 2026-10-17: its 48 boxes, of 52 by 32 pixels each, 79,872 in
/// all; the strip's count, the knobs' and DRUM SQ's on 2026-10-24: the dots' band 34,384 pixels, the
/// three oscillator knobs' rings 38,265 and the macro knobs' 108,573, each three off one grid since
/// the second review of that day), plus a margin of READ_MARGIN — fewer turns than the pixels of the smallest box any
/// read looks at there (AUTO FOLLOW's, 44 by 32 pixels; the lock and the pattern switch, 40 by 40;
/// a Mute, 52 by 32; FAV, 100 by 40), so that a loop in Luau over any box's pixels, at least one
/// turn a pixel, is more than its read may spend. A read not listed may spend READ_MARGIN.
const READ_MARGIN: u64 = 1000;
const READ_TURNS: &[(&str, u64)] = &[
    ("strip count", 23423),
    ("strip decide", 9258),
    ("centre decide", 721),
    ("drumsq decide", 7270),
    ("drums decide", 22065),
    ("macros selected", 611),
    ("macros buttons", 995),
    ("expansion favorite", 2133),
    ("knob macros", 24834),
    ("knob osc", 36324),
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
  count("drums", { "decide" })
  count("macros", { "selected", "buttons", "labels", "count" })
  count("expansion", { "favorite", "look", "parseView" })
  count("knob", { "macro", "macros", "osc", "oscMiddles" })
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

/// No loop over pixels on the event loop: at 200 % — the largest Avenger, whose macro knob's ring
/// alone is 36,191 pixels — the whole open, from the key to the show, turns Luau's loops and calls
/// about 95,000 times with an oscillator selected and with the Drums tab, DRUM SQ and the drumkit
/// table: each under a bound with a margin for small changes that a loop over the strip's dots band
/// (about 34,400 pixels at 200 %) or the macro buttons (about 21,000) passes; and each read spends
/// at most what READ_TURNS lets it, which a loop over its smallest box passes. Most of it is the
/// dots' and the pointers' own pixels, every one of which the project's readers count (read/strip.luau,
/// read/knob.luau): the colours of the cells that hold one, at most a few thousand points. The grids
/// are the host's own limit. On Windows, on a display at 100 % scaling and a screen that holds all
/// of Avenger, so that every part is read — and the reads come out as at 80 %.
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
        -- The reads' own work, from the picture to the values (read/open.luau), from the key to the
        -- last answer's handler that hands them over (the page's open answers later): without the
        -- runtime's own part of the key.
        local Open = S.included["modules/vps-avenger/src/read/open.luau"][1]
        local read, turns = Open.read, 0
        Open.read = function(o, opts, done)
          local t0 = T.interrupts()
          return read(o, opts, function(...)
            turns += T.interrupts() - t0
            return done(...)
          end)
        end
        T.pageKey()
        T.tick()
        Open.read = read
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
        T.ok(most <= 2000 and all <= 5000, string.format("points: at most %d at a time, %d in all", most, all))
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
    // Measured 2026-10-12: 34,873 turns, of 12 grids of 4400 cells, 7 profiles and 1552 points; on
    // 2026-10-17, with the drumkit table's grids asked too, 33,731 of 16 grids of 9236 cells, 8 profiles
    // and 1356 points; on 2026-10-24, with every pixel of the dots and the knobs' pointers counted,
    // 94,614, the points 1750 at most and 4023 in all.
    run(&scenario(
        r#"{ osc = 8, selected = "osc", index = 5, mute = { [8] = true }, solo = { [1] = true }, trigSolo = true,
          macroShown = 3, buttons = { true, true }, amounts = { 30, 60, 90 }, volume = 50, transpose = -24, fine = 0 }"#,
        r#"T.ok(o.count == 8 and o.selected_index == 5 and o.mute_states[8] == true and o.solo_states[1] == true
          and o.mute_states[1] == false and o.trig_solo == true, "the strip at 200 %")
        T.ok(o.volume_values[5] == 50 and o.transpose_values[5] == -24 and o.fine_tune_values[5] == 0,
          string.format("the knobs at 200 %%: %s %s %s", tostring(o.volume_values[5]), tostring(o.transpose_values[5]),
            tostring(o.fine_tune_values[5])))"#,
        "110000",
    ));
    // Measured 2026-10-12: 47,999 turns, of 12 grids of 4400 cells, 7 profiles and 1028 points; on
    // 2026-10-17, with the drumkit table read — its 48 boxes, the selection and the names — 71,569 of 16
    // grids of 9236 cells, 8 profiles and 1028 points; on 2026-10-24, with every pixel of the dots and
    // the knobs' pointers counted and DRUM SQ's selection by its rows, 95,610.
    run(&scenario(
        r#"{ osc = 8, selected = "drums", pattern = true, lock = true, mode = "sq", central = "drum_sq",
          sq = { chain = true, follow = false, on = { b = true }, selected = "a" }, macroShown = 3,
          buttons = { true, true }, amounts = { 30, 60, 90 } }"#,
        r#"T.ok(o.selected_type == "drums" and o.drum_pattern == true and o.drum_lock == true and o.drum_mode == "sq"
          and o.central_panel_tab == "drum_sq" and o.drum_sq_pattern_chain == true and o.drum_sq_pattern_selected == "a"
          and o.drum_sq_pattern_states[1] == true, "the Drums tab and DRUM SQ at 200 %")"#,
        "111000",
    ));
}

/// Avenger at 200 % on a Mac, larger than the 2560 x 1440 screen its corner is on: the picture is
/// the part the screen shows — one snapshot of the whole panel, 3485 x 2244 points, would pass what
/// one may hold there (6,710,886 points) — and what lies past the edge is not read: the macros,
/// whose tab captions are no text of the picture, and Trig's Solo, cut by the edge. The questions
/// are asked of what the picture holds of their regions, so the rest reads as at 80 %; the log says
/// where the picture was cut, and the page's error region which parts leave the screen, which way,
/// and how to bring them back (read/open.luau, Open.offScreen).
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
        T.ok(T.lastPost("error").text == "The oscillator strip is past the screen's right edge; the central panel and the "
          .. "macros are below the screen and past its right edge; move the plug-in's window up and left. What lies past "
          .. "the edge was not read.", T.lastPost("error").text)
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
/// onReady of that first load shows the page with Avenger read afresh (the runtime asks `open`
/// again there), with no age either.
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
        st = T.lastPost("state").state
        T.ok(#T.page().posts == n + 2 and st.preset == "LD Lead 1" and st.read_age_seconds == nil, "ready: no age")
        T.pageReady()
        T.tick()
        st = T.lastPost("state").state
        T.ok(#T.page().shows == 2 and #T.page().posts > n + 2 and st.read_age_seconds == nil,
          "shown once loaded, read afresh, with no age")
    "##);
}

/// A press's questions and its first text read are out after the key's handler, answered on a later
/// turn (the runtime's `openLater`). A disable drops them: that press shows nothing, and holds
/// nothing back — the next press reads Avenger again and shows the page.
#[test]
fn a_press_a_disable_dropped_shows_nothing_and_the_next_press_reads_again() {
    run_mac(r##"
        local S = T.S
        T.avengerAt80("Init Preset")
        T.pageReady()
        local snaps = S.snaps
        T.pageKey()
        T.ok(S.snaps == snaps + 1 and #S.screenOut > 0 and #S.pageReads == 1, "out after the handler")
        T.dropScreen()
        T.tick()
        T.ok(#T.page().shows == 0 and #S.pageReads == 0, "nothing shown of the press dropped")
        T.pageKey()
        T.tick()
        T.ok(S.snaps == snaps + 2 and #T.page().shows == 1, "read again, and shown")
    "##);
}

// ---------------------------------------------------------------------------------------------
// The header's actions (slice 4): each a trip from a page message, as the ring's scenarios play
// Avenger at 80 % in REAPER's floating window on a Mac.
// ---------------------------------------------------------------------------------------------

/// ▶ from the page: Avenger's window asked forward (its id, on a Mac), ▶ clicked where the ring
/// clicks it, nothing posted until the name is read — and the name watched until it changes; then
/// the state with the preset heading from it, the error region cleared, the focus back where the
/// user was, and the page back. ◀ at a name that does not change is said "unchanged", in the error
/// region the page comes back on.
#[test]
fn previous_and_next_from_the_page_click_watch_the_name_and_say_unchanged() {
    run_mac(r##"
        local S = T.S
        local header, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        T.ok(header.away, "away while the page is in front")
        local clicks, from = #S.clicks, #S.pageOrder
        T.tripFrom({ action = "next_preset", args = {}, focused = true }, W)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "469,106", "▶ where the ring clicks it: " .. T.clickAt())
        T.ok(T.pageSince(from) == "focus 31", "nothing posted before the name is read: " .. T.pageSince(from))
        T.nameReads("LD Lead 2", 600)
        T.ok(T.pageSince(from) == "focus 31,post state,post error,post restoreFocus,show", T.pageSince(from))
        T.ok(T.lastPost("state").state.preset == "LD Lead 2" and T.lastPost("error").text == "", "the heading, read")
        T.ok(S.front == T.PAGE_WIN and #S.clicks == clicks + 1, "the page back, and one click")
        T.event()
        T.nameIs("LD Lead 2")
        T.tripFrom({ action = "prev_preset", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "450,106", "◀: " .. T.clickAt())
        T.nameReads("LD Lead 2")
        T.ok(T.lastPost("error").text == "Previous preset: unchanged, LD Lead 2" and T.lastPost("error").focus == true,
          tostring(T.lastPost("error").text))
        T.ok(T.lastPost("state").state.preset == "LD Lead 2" and S.front == T.PAGE_WIN, "the page back on it")
    "##);
}

/// ▶ from the page reads Avenger's loading display first (the tester's log: "cYanmaGentaYellowb"
/// after "AR Catch Me", posted to the page as the preset), each read handed to two of the watch's
/// looks, as on a Mac: the name counts once two readings agree or the catalog has it, so the state
/// posted is the name, and the loading display is never posted.
#[test]
fn previous_and_next_from_the_page_post_a_settled_name() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        T.tripFrom({ action = "next_preset", args = {}, focused = true }, W)
        local answers = { "cYanmaGentaYellowb", "LD Lead 2" }
        T.slowNameReads(function(i) return answers[math.min(i, #answers)] end)
        T.ok(T.lastPost("state").state.preset == "LD Lead 2", tostring(T.lastPost("state").state.preset))
        for _, p in ipairs(T.page().posts) do
          T.ok(not (p.kind == "state" and p.state and p.state.preset == "cYanmaGentaYellowb"),
            "the loading display posted as the preset")
        end
        T.ok(T.count("— LD Lead 1 -> LD Lead 2 (2 readings alike)") == 1, T.dump())
    "##);
}

/// ▶ from the page with Avenger's loading display read once by the watch's bound, and nothing
/// after it: the name never settled, so the page gets the error, not the text — no state with it
/// as the preset.
#[test]
fn a_name_read_once_by_the_watchs_bound_is_not_posted_from_the_page() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        T.tripFrom({ action = "next_preset", args = {}, focused = true }, W)
        T.slowNameReads(function(i) return i == 1 and "cYanmaGentaYellowb" or nil end)
        T.ok(T.lastPost("error").text == "Next preset: the preset name cannot be read now",
          tostring(T.lastPost("error").text))
        for _, p in ipairs(T.page().posts) do
          T.ok(not (p.kind == "state" and p.state and p.state.preset == "cYanmaGentaYellowb"),
            "the loading display posted as the preset")
        end
        T.ok(T.count("[avenger] 'Next preset': 'cYanmaGentaYellowb' was read once and is no catalog name "
          .. "— not taken") == 1, T.dump())
    "##);
}

/// One trip at a time: ▶ pressed twice in the page before Avenger's window has come — the page
/// still has the keyboard — starts one trip; the second message is dropped, and the runtime logs
/// it. One click, one name.
#[test]
fn a_second_message_while_a_trip_is_out_is_dropped() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        local clicks = #S.clicks
        S.focusAtOnce = false
        S.focusLands = function() S.front, S.chain = W, { W } end
        T.pageMsg({ action = "next_preset", args = {}, focused = true })
        T.pageMsg({ action = "next_preset", args = {}, focused = true })
        T.ok(T.count("trip 'Next preset' not started — busy") == 1, T.dump())
        T.land()
        T.runDue()
        T.nameReads("LD Lead 2", 600)
        T.ok(#S.clicks == clicks + 1 and S.front == T.PAGE_WIN, "one click, and the page back")
    "##);
}

/// Load from the page: MENU clicked, Load chosen in Avenger's popup once it is seen (the ring's
/// rows), then the file dialog that comes — a window of the DAW's, the user's for as long as they
/// like — and Escape in it: Avenger in front again ends the wait, the name is watched, and the page
/// comes back with it, unchanged and not called an error. Save as the same, its dialog saving under
/// a new name, which the heading then shows.
#[test]
fn load_with_escape_in_its_dialog_and_save_as_read_the_name_after_the_dialog() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        local function dialog(title)
          return { id = 950, title = title, class = "AXWindow/AXDialog/", app = T.MAC_REAPER,
            bounds = { x = 300, y = 200, w = 700, h = 450 }, client = { x = 300, y = 228, w = 700, h = 422 } }
        end
        T.tripFrom({ action = "pick_load_preset", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "517,106", "MENU: " .. T.clickAt())
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,135", "Load's row: " .. T.clickAt())
        T.closePopups()
        T.tick(2)
        local open = dialog("Open")
        S.listed = { W, open }
        T.show(open)
        T.runDue()
        T.ok(T.count("a dialog came forward — 'Open'") == 1, T.dump())
        T.run(6000)
        T.ok(S.front == open, "nothing bounds the user's time in the dialog")
        -- Escape: Avenger in front again, the same name.
        S.listed = { W }
        T.show(W)
        T.runDue()
        T.nameReads("LD Lead 1")
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.preset == "LD Lead 1"
          and T.lastPost("error").text == "", "the name read, and the page back with nothing called wrong")
        T.event()
        T.tripFrom({ action = "pick_save_preset_as", args = {}, focused = true }, W)
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,172", "Save as's row: " .. T.clickAt())
        T.closePopups()
        T.tick(2)
        local save = dialog("Save")
        S.listed = { W, save }
        T.show(save)
        T.runDue()
        S.listed = { W }
        T.show(W)
        T.runDue()
        T.nameReads("My Lead")
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.preset == "My Lead", "the new name")
    "##);
}

/// Load and Save as from the page click their item ONCE (actions.luau, menuChoice's `retries`), as
/// the ring's do: Avenger's popup stays drawn, frozen, while its file dialog opens, which the menu's
/// test takes for a click it missed — the second Mac test's Load was clicked four times, the clicks
/// after the first landing in the dialog. The popup left up for eight turns: no click again.
#[test]
fn the_pages_load_and_save_as_click_their_item_once_under_a_popup_left_up() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        local open = { id = 950, title = "Open", class = "AXWindow/AXDialog/", app = T.MAC_REAPER,
          bounds = { x = 300, y = 200, w = 700, h = 450 }, client = { x = 300, y = 228, w = 700, h = 422 } }
        for _, c in ipairs({ { "pick_load_preset", "586,135" }, { "pick_save_preset_as", "586,172" } }) do
          T.tripFrom({ action = c[1], args = {}, focused = true }, W)
          T.popup(300, 490, 116, 192, 96)
          T.tick(1)
          T.ok(T.clickAt() == c[2], c[1] .. "'s row: " .. T.clickAt())
          local clicks = #S.clicks
          T.tick(8)
          T.ok(#S.clicks == clicks, c[1] .. ": " .. (#S.clicks - clicks) .. " click(s) more under the popup")
          T.closePopups()
          T.tick(2)
          S.listed = { W, open }
          T.show(open)
          T.runDue()
          S.listed = { W }
          T.show(W)
          T.runDue()
          T.nameReads("LD Lead 1")
          T.ok(S.front == T.PAGE_WIN, c[1] .. ": the page back")
          T.event()
        end
    "##);
}

/// Initialize: the page's No closes its dialog and clicks nothing in Avenger — no window asked
/// forward, nothing posted. Its Yes chooses Initialize in MENU, then READS Avenger's warning box —
/// nothing is clicked while the box is not read — and clicks YES where it was read; the box read
/// until two readings without it, then everything read again off one picture, and the page comes
/// back with it. The box overlay of the ring is never armed: the page answered the box.
#[test]
fn initialize_from_the_page_reads_the_box_before_yes_and_no_clicks_nothing() {
    run_mac(r##"
        local S = T.S
        local _, box, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        local clicks, posts, focuses = #S.clicks, #T.page().posts, #S.focusCalls
        T.pageMsg({ action = "cancel_initialize_preset", args = {}, focused = true })
        T.ok(#S.clicks == clicks and #T.page().posts == posts and #S.focusCalls == focuses, "No: nothing at all")
        T.tripFrom({ action = "pick_initialize_preset", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "517,106", "MENU: " .. T.clickAt())
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        T.ok(T.clickAt() == "586,193", "Initialize's row: " .. T.clickAt())
        T.closePopups()
        T.tick(2)
        local r = T.lastRead("vps-avenger page box")
        T.ok(r and T.region(r.region) == "546,404,1058,622", "the box's band: " .. (r and T.region(r.region) or "no read"))
        clicks = #S.clicks
        T.answer(r, T.noBox())
        T.ok(#S.clicks == clicks and T.lastRead("vps-avenger page box") ~= r, "not drawn yet: read again, nothing clicked")
        T.answer(T.lastRead("vps-avenger page box"), T.boxReading("Are you sure?"))
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "718,581", "YES where it was read: " .. T.clickAt())
        T.words({ preset = "Init Preset" })
        local snaps = S.snaps
        T.answer(T.lastRead("vps-avenger page box"), T.boxReading("Are you sure?"))
        T.answer(T.lastRead("vps-avenger page box"), T.noBox())
        T.ok(S.snaps == snaps, "one reading without the box is not yet its going")
        T.answer(T.lastRead("vps-avenger page box"), T.noBox())
        T.ok(S.snaps == snaps + 1 and #S.clicks == clicks + 1, "gone: Avenger read again, one picture, no other click")
        T.runDue()
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.preset == "Init Preset"
          and T.lastPost("error").text == "", "the page back with what was read")
        T.ok(T.count("[avenger] [page] 'Initialize preset': Avenger's warning box is up: \"Are you sure?\"") == 1, T.dump())
        T.ok(not box.active and T.count("Avenger's warning box is looked for") == 0, "the ring's box never armed")
    "##);
}

/// A page action whose trip the runtime does not start, said in the page by the code of inPlugin's
/// error value: a page that came forward without the page key holds no place (`not_found`), and the
/// page says to press the key in Avenger; Avenger's window gone (`gone`, nothing of its application
/// listed and the focus refused) is the tool's closed sentence; a focus the host answers `hidden` is
/// a window that would not come. Nothing is clicked.
#[test]
fn a_trip_not_started_says_why_in_the_page_by_its_code() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.openPage()
        T.event()
        local clicks = #S.clicks
        local function undo() T.pageMsg({ action = "undo", args = {}, focused = true }) end
        S.listed, S.focusAnswer = {}, false
        undo()
        T.ok(T.lastPost("error").text == "The plug-in window was closed. Please open it again, then press @KEY@ again.",
          T.lastPost("error").text)
        S.listed, S.focusErr = { W }, T.E("hidden", "the window is hidden")
        undo()
        T.ok(T.lastPost("error").text == "Undo: VPS Avenger's window would not come to the front", T.lastPost("error").text)
        S.focusErr, S.focusAnswer = nil, true
        -- Closed — the way back taken, Avenger in front again — and then shown by the module itself:
        -- no place held.
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageClosed("user")
        T.event()
        T.page().page:show({ focus = true })
        T.event()
        undo()
        T.ok(T.lastPost("error").text == "Undo: the page was not opened from VPS Avenger; press the page key there",
          T.lastPost("error").text .. "\n" .. T.dump())
        T.ok(#S.clicks == clicks, "nothing clicked")
    "##.replace("@KEY@", MAC_KEY).as_str());
}

/// While the warning box holds the slot — the ring's Initialize waiting on its box — a trip's
/// overlay is the box (the runtime's `trip.overlay`), which has no menu tests: no menu choice is
/// asked of it, where O:chooseMenuItem and trip:afterMenu raise, and the trip ends with the page
/// told the menu cannot be seen here. Every action that chooses in a menu — MENU's items, the redo
/// list, the size list, and a menu of src/act.luau's — clicks nothing and raises nothing.
#[test]
fn a_menu_choice_with_the_warning_box_up_is_not_asked_and_the_page_is_told() {
    run_mac(r##"
        local S = T.S
        local header, box = T.avengerAt80("Init Preset")
        T.choose(header, 7, 490, 116, 192, 96)
        T.poll(500)
        T.answer(T.lastRead("vps-avenger dialog"), T.boxReading("Are you sure?"))
        T.event()
        T.ok(box.active and not header.active, "the box holds the slot")
        local A = S.included["modules/vps-avenger/src/actions.luau"][1]
        local Ac = S.included["modules/vps-avenger/src/act.luau"][1]
        T.ok(Ac.menuHere(header) and not Ac.menuHere(box), "the header alone has the menu tests")
        -- A trip of the page's whose overlay is the box: inPlugin runs the act at once, as a handler.
        local function onTheBox(run)
          local ended
          local trip = { live = true, inFront = function() return true end, done = function(_, r) ended = r end }
          local o = { inPlugin = function(_, spec) T.handled(spec.act, box, trip); return true end }
          local clicks = #S.clicks
          run(o, trip)
          T.ok(#S.clicks == clicks, "nothing clicked")
          return ended and (ended.error or ended.sentence)
        end
        for _, action in ipairs({ "pick_load_preset", "pick_save_preset_as", "pick_initialize_preset", "redo_menu", "set_zoom" }) do
          local said = onTheBox(function(o) A.HANDLERS[action](o, { percent = 100 }) end)
          T.ok(said == "the menu cannot be seen here; nothing was chosen", action .. ": " .. tostring(said))
        end
        -- src/act.luau's menu, after its read before the click.
        local said = onTheBox(function(_, trip)
          local got
          T.handled(Ac.run, box, { label = "a menu", part = "strip",
            decide = function() return { at = { 300, 100 }, menu = { item = { 310, 120 } } } end,
            answer = function() return {} end }, function(r) got = r end, trip)
          trip.done(trip, got)
        end)
        T.ok(said == "the menu cannot be seen here; nothing was chosen", "act.luau: " .. tostring(said))
        T.ok(T.count("[avenger] [act] 'a menu': nothing chosen — overlay 'VPS Avenger warning' has no menu tests") == 1, T.dump())
    "##);
}

/// UNDO from the page, then the name — no change is no error; and the redo list: UNDO right-clicked
/// as the ring does, the trip holding for as long as the list is up — the user's choice in it — and
/// going on once the menu tests see it closed: then the name, which the heading shows.
#[test]
fn undo_and_the_redo_list_from_the_page_read_the_name_after() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        T.tripFrom({ action = "undo", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "586,106" and S.clicks[#S.clicks].button == nil, "UNDO: " .. T.clickAt())
        T.nameReads("LD Lead 1")
        T.ok(S.front == T.PAGE_WIN and T.lastPost("error").text == "", "no change, and nothing called wrong")
        T.event()
        T.tripFrom({ action = "redo_menu", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "586,106" and S.clicks[#S.clicks].button == "right", "UNDO, right: " .. T.clickAt())
        T.popup(320, 586, 110, 150, 200)
        T.tick(1)
        T.run(3000)
        T.ok(S.front ~= T.PAGE_WIN and T.lastRead("vps-avenger name") == nil or T.lastRead("vps-avenger name").answered,
          "the list is the user's for as long as it is up")
        T.closePopups()
        T.tick(2)
        T.runDue()
        T.nameReads("LD Lead 2")
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.preset == "LD Lead 2", "the name after the list")
    "##);
}

/// ◀ and ▶ in the page on Init Preset, which VPS Avenger steps no preset from (TODO.md, the first
/// Mac test of the page, item 3): the state read at the key says the steps are unavailable, with
/// the reason the page puts in their help, and a press sends nothing to Avenger — no trip, no
/// click — and says why in the error region, the page in front and the focus left where it is. The
/// page's next refresh with a preset read makes them available again, and ▶ goes into Avenger as
/// before. A trip whose own read in Avenger finds Init Preset — Avenger initialized some other way
/// meanwhile — clicks nothing and brings the page back with that name and why; UNDO from the page,
/// a preset read after it, makes them available again.
#[test]
fn previous_and_next_from_the_page_send_nothing_on_init_preset() {
    run_mac(r##"
        local S = T.S
        local WHY = "VPS Avenger steps no preset from Init Preset; load a preset first"
        local header, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.openPage()
        local st = T.lastPost("state").state
        T.ok(st.preset == "Init Preset" and st.preset_steps_unavailable == WHY, tostring(st.preset_steps_unavailable))
        T.event()
        local clicks, focuses = #S.clicks, #S.focusCalls
        for _, a in ipairs({ { "prev_preset", "Previous preset" }, { "next_preset", "Next preset" } }) do
          local n = #T.page().posts
          T.pageMsg({ action = a[1], args = {}, focused = true })
          T.ok(#T.page().posts == n + 1 and T.lastPost().kind == "error" and T.lastPost().text == a[2] .. ": " .. WHY
            and T.lastPost().focus == nil, tostring(T.lastPost().text))
        end
        T.runDue()
        T.ok(#S.clicks == clicks and #S.focusCalls == focuses and S.front == T.PAGE_WIN, "nothing sent to Avenger")
        T.ok(T.count("[avenger] [page] 'Next preset': the page's preset is Init Preset — nothing sent to VPS Avenger")
          == 1, T.dump())
        -- The page's next refresh: back to Avenger, a preset loaded there, and the key again.
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.ok(header.active, "the header again")
        T.nameIs("LD Lead 1")
        T.openPage()
        st = T.lastPost("state").state
        T.ok(st.preset == "LD Lead 1" and st.preset_steps_unavailable == nil, tostring(st.preset_steps_unavailable))
        T.event()
        T.tripFrom({ action = "next_preset", args = {}, focused = true }, W)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "469,106", "▶: " .. T.clickAt())
        T.nameReads("LD Lead 2")
        T.ok(T.lastPost("state").state.preset == "LD Lead 2" and T.lastPost("error").text == "", "stepped")
        -- Avenger initialized meanwhile, not through the page: the trip's own read before the click.
        T.event()
        T.nameIs("Init Preset")
        clicks = #S.clicks
        T.tripFrom({ action = "prev_preset", args = {}, focused = true }, W)
        T.runDue()
        T.ok(#S.clicks == clicks, "nothing clicked: " .. T.clickAt())
        st = T.lastPost("state").state
        T.ok(st.preset == "Init Preset" and st.preset_steps_unavailable == WHY and S.front == T.PAGE_WIN,
          tostring(st.preset) .. " " .. tostring(st.preset_steps_unavailable))
        T.ok(T.lastPost("error").text == "Previous preset: " .. WHY and T.lastPost("error").focus == true,
          tostring(T.lastPost("error").text))
        T.ok(T.count("[avenger] [page] 'Previous preset': the name read in VPS Avenger is Init Preset — nothing "
          .. "clicked") == 1, T.dump())
        -- UNDO from the page: a preset read after it, and the steps are available again.
        T.event()
        T.tripFrom({ action = "undo", args = {}, focused = true }, W)
        T.nameReads("LD Lead 2")
        st = T.lastPost("state").state
        T.ok(st.preset == "LD Lead 2" and st.preset_steps_unavailable == nil, tostring(st.preset))
    "##);
}

/// The ring and the page go by the same latest read. The page key's read of a preset — loaded some
/// other way while the ring claimed Init Preset — makes the ring's ◀ and ▶ available again, said
/// as the page key brings Avenger back; the page's Initialize, read again after it, makes them
/// unavailable. A trip that may have changed the preset and reads no name after it — UNDO whose
/// reads give nothing, the redo list given up when another window came to the front — posts the
/// preset as not read, so the page offers ◀ and ▶ again, and forgets the ring's claim as well: the
/// next trip's read decides, and one that reads nothing claims nothing.
#[test]
fn the_ring_and_the_page_go_by_the_same_latest_read_of_init_preset() {
    run_mac(r##"
        local S = T.S
        local WHY = "VPS Avenger steps no preset from Init Preset; load a preset first"
        local header, _, W = T.avengerAt80("Init Preset")
        -- The focus on ▶, which says it is unavailable.
        header.focus = 2
        T.tab()
        T.ok(T.lastSaid() == "Next preset, unavailable on Init Preset, button", T.said())
        -- Back to Avenger from the page, with the page key: what the arrival says.
        local function back()
          S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
          T.pageKey()
          T.event()
          T.run(400)
          T.ok(header.active and S.front == W, "the header again")
          return T.lastSaid()
        end
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.ok(T.lastPost("state").state.preset_steps_unavailable == nil, "a preset read at the key")
        T.event()
        T.ok(back() == "Next preset, button", T.said())
        -- Initialize from the page: Init Preset read again after it.
        T.openPage()
        T.event()
        T.tripFrom({ action = "pick_initialize_preset", args = {}, focused = true }, W)
        T.popup(300, 490, 116, 192, 96)
        T.tick(1)
        T.closePopups()
        T.tick(2)
        T.answer(T.lastRead("vps-avenger page box"), T.boxReading("Are you sure?"))
        T.words({ preset = "Init Preset" })
        T.answer(T.lastRead("vps-avenger page box"), T.noBox())
        T.answer(T.lastRead("vps-avenger page box"), T.noBox())
        T.runDue()
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.preset_steps_unavailable == WHY, "Init Preset")
        T.event()
        T.ok(back() == "Next preset, unavailable on Init Preset, button", T.said())
        -- UNDO from the page, and nothing read after it.
        T.nameIs("Init Preset")
        T.openPage()
        T.event()
        T.tripFrom({ action = "undo", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "586,106", "UNDO: " .. T.clickAt())
        T.nameReads(nil)
        local st = T.lastPost("state").state
        T.ok(st.preset == nil and st.preset_steps_unavailable == nil and S.front == T.PAGE_WIN,
          tostring(st.preset) .. " " .. tostring(st.preset_steps_unavailable))
        T.ok(T.lastPost("error").text == "Undo: the preset name cannot be read now", tostring(T.lastPost("error").text))
        T.event()
        -- ▶ is offered again, and the ring's claim is gone with it: a trip whose own read gets
        -- nothing claims nothing, and clicks.
        T.nameIs(nil)
        local clicks = #S.clicks
        T.tripFrom({ action = "next_preset", args = {}, focused = true }, W)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "469,106", "▶: " .. T.clickAt())
        T.nameReads("LD Lead 2")
        T.ok(T.lastPost("state").state.preset == "LD Lead 2", tostring(T.lastPost("state").state.preset))
        T.event()
        -- Init Preset again, read at the key; the redo list given up while it is up.
        T.ok(back() == "Next preset, button", T.said())
        T.nameIs("Init Preset")
        T.openPage()
        T.ok(T.lastPost("state").state.preset_steps_unavailable == WHY, "Init Preset at the key")
        T.event()
        T.tripFrom({ action = "redo_menu", args = {}, focused = true }, W)
        T.ok(S.clicks[#S.clicks].button == "right", "UNDO, right: " .. T.clickAt())
        T.popup(320, 586, 110, 150, 200)
        T.tick(1)
        T.show(T.OTHER)
        T.runDue()
        st = T.lastPost("state").state
        T.ok(st.preset == nil and st.preset_steps_unavailable == nil,
          tostring(st.preset) .. " " .. tostring(st.preset_steps_unavailable))
        T.ok(T.lastPost("error").text == "Redo: another window came to the front; nothing more was done",
          tostring(T.lastPost("error").text))
    "##);
}

/// Size from the page: the zoom list opened and the entry for the size asked chosen — 100 %, four
/// entries below 80 %, where the ring's stepper takes only the one beside — then the zoom field
/// read until it shows the new zoom, which the page's state then has.
#[test]
fn a_size_from_the_page_is_chosen_at_any_entry_and_read_back() {
    run_mac(r##"
        local S = T.S
        local _, _, W = T.avengerAt80("Init Preset")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        T.event()
        T.tripFrom({ action = "set_zoom", args = { percent = 100 }, focused = true }, W)
        T.ok(T.clickAt() == "132,106", "the zoom field: " .. T.clickAt())
        T.popup(310, 114, 114, 96, 390)
        T.tick(1)
        T.ok(T.clickAt() == "162,309", "100 %'s entry: " .. T.clickAt())
        T.closePopups()
        T.tick(2)
        T.run(300)
        local r = T.lastRead("vps-avenger header")
        T.ok(r and not r.answered, "the zoom field read again")
        T.answer(r, T.header(98, 84, 2, 100))
        T.run(300)
        T.ok(S.front == T.PAGE_WIN and T.lastPost("state").state.zoom == 100 and T.lastPost("error").text == "",
          tostring(T.lastPost("state").state.zoom) .. " " .. tostring(T.lastPost("error").text))
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

/// Back and Next in the page's markup and script (TODO.md, the first Mac test of the page, item 3):
/// plain buttons that carry aria-disabled — "false" until a state says otherwise — with their help,
/// and nothing that takes them out of the Tab order or hides them; the script sets aria-disabled and
/// their help (the title, which is also their description) from the state's
/// `preset_steps_unavailable` at every state, never `disabled`; it reads aria-disabled nowhere, so a
/// press is still sent to the module, which answers it; and the focus goes back to an
/// aria-disabled button as to any other.
#[test]
fn back_and_next_carry_aria_disabled_and_stay_in_the_tab_order() {
    // The files as checked out: a Windows checkout may end their lines with CR LF.
    let html = INDEX_HTML.replace("\r\n", "\n");
    let js = PANEL_JS.replace("\r\n", "\n");
    for id in ["prev-preset-button", "next-preset-button"] {
        let at = html.find(&format!("id=\"{id}\"")).unwrap_or_else(|| panic!("index.html has {id}"));
        let start = html[..at].rfind('<').unwrap();
        let end = at + html[at..].find('>').unwrap();
        let tag = &html[start..=end];
        assert!(tag.starts_with("<button type=\"button\""), "{id} is a button: {tag}");
        assert!(tag.contains("aria-disabled=\"false\"") && tag.contains("title=\""), "{id}: {tag}");
        for gone in [" disabled", "tabindex", " hidden", "aria-hidden"] {
            assert!(!tag.contains(gone), "{id} has '{gone}': {tag}");
        }
        assert!(js.contains(&format!("'{id}'")), "panel.js does not update {id}");
    }
    let start = js.find("function updatePresetSteps(reason) {").expect("panel.js has updatePresetSteps");
    let body = &js[start..start + js[start..].find("\n  }\n").expect("it ends")];
    assert!(body.contains("button.setAttribute('aria-disabled', reason ? 'true' : 'false');"), "{body}");
    assert!(body.contains("button.title = reason ? `${help}, unavailable: ${reason}.` : help;"), "{body}");
    for gone in [".disabled", "tabIndex", "tabindex", "hidden", "removeAttribute"] {
        assert!(!body.contains(gone), "updatePresetSteps has '{gone}': {body}");
    }
    let apply = js.find("function applyState(s) {").expect("panel.js has applyState");
    let apply_end = apply + js[apply..].find("\n  }\n").expect("applyState ends");
    let call = js.find("updatePresetSteps(readText(s.preset_steps_unavailable));").expect("the state's reason");
    assert!(call > apply && call < apply_end, "updatePresetSteps is called in applyState, at every state");
    assert!(
        !js.contains("getAttribute('aria-disabled')") && !js.contains("ariaDisabled"),
        "the script reads aria-disabled somewhere, and could drop a press"
    );
    let restore = js.find("function restoreFocus() {").expect("panel.js has restoreFocus");
    let restore_body = &js[restore..restore + js[restore..].find("\n  }\n").unwrap()];
    assert!(!restore_body.contains("aria-disabled"), "restoreFocus passes over an aria-disabled button");
}

/// The page claims nothing of DRUM SQ at a click (the plan's slice 8, its other half, the page edits
/// E1 to E4): a pattern SELECTED, PASTE offered and LENGTH shown come from the state alone — each is
/// set in applyState and nowhere else, so no click of a select button, of the pattern menu's COPY or
/// of the length list sets it, and the select buttons carry no click listener of their own beside
/// the one that sends their action. And the twelve slots' lists, which the module keys by the slot's
/// number where JSON could not carry them as a list (state.luau), are read through one reader that
/// takes either.
#[test]
fn the_page_claims_nothing_of_drum_sq_at_a_click_and_reads_the_slot_lists_either_way() {
    let js = PANEL_JS.replace("\r\n", "\n");
    let apply = js.find("function applyState(s) {").expect("panel.js has applyState");
    let apply_end = apply + js[apply..].find("\n  }\n").expect("applyState ends");
    for (set, declared) in [
        ("centralDrumSqPattern = ", "let centralDrumSqPattern = null;"),
        ("centralDrumSqPatternCopied = ", "let centralDrumSqPatternCopied = false;"),
        ("setCentralDrumSqLengthSteps(", "function setCentralDrumSqLengthSteps(value) {"),
    ] {
        let decl = js.find(declared).unwrap_or_else(|| panic!("panel.js has '{declared}'"));
        let sets: Vec<usize> = js
            .match_indices(set)
            .map(|(i, _)| i)
            .filter(|i| !(decl..decl + declared.len()).contains(i))
            .collect();
        assert!(!sets.is_empty(), "panel.js sets '{set}' from the state");
        for at in sets {
            let line = &js[at..at + js[at..].find('\n').unwrap_or(js.len() - at)];
            assert!(at > apply && at < apply_end, "'{set}' outside applyState: {line}");
        }
    }
    assert!(js.contains("centralDrumSqPatternCopied = oscPanel.drum_sq_pattern_copied === true;"), "PASTE from the state");
    let select = js.find("function appendPatternSelectButton(key) {").expect("panel.js has the select buttons");
    let select_body = &js[select..select + js[select..].find("\n    }\n").expect("it ends")];
    assert!(!select_body.contains("addEventListener"), "a select button sets something at its click: {select_body}");

    let reader = js.find("const readSlots = (v, read) => {").expect("panel.js has readSlots");
    let reader_body = &js[reader..reader + js[reader..].find("\n  };\n").expect("it ends")];
    for part in ["if (Array.isArray(v)) return v.map(read);", "parseInt(key, 10)", "i >= 1 && i <= 12"] {
        assert!(reader_body.contains(part), "readSlots has no '{part}': {reader_body}");
    }
    for key in [
        "drumkit_part_labels", "drumkit_mute_states", "drumkit_solo_states", "drumkit_lock_states",
        "drumkit_loop_states", "sample_editor_amp_states", "sample_editor_pitch_states", "sample_editor_eq_states",
    ] {
        assert!(
            js.contains(&format!("{key}: readSlots(s.osc_panel.{key}, ")),
            "panel.js reads {key} other than through readSlots"
        );
    }
}

/// The browser's page edits (the plan's slice 9, §7, and the third Mac test of the page,
/// 2026-10-25): Read Avenger's List is gone — the lists are the preset database's, never read off
/// the grid (the maintainer's decision) — from the markup, the script and its list of actions; a
/// roster's expansion without data is marked in its button's aria-label, never in its text, so the
/// search still filters by the name; the list's label and the open expansion's heading say the
/// list and the name alone — no "from the database" since the maintainer's decision of 2026-10-26,
/// also where only its place in the database's list named the expansion open — and whether an
/// expansion is known is told by the name the module posted, not by the heading. Confirm Expansion
/// View's help says it confirms the grid with no expansion open.
#[test]
fn the_browsers_page_edits_read_the_new_keys_and_mark_in_aria_labels_only() {
    let html = INDEX_HTML.replace("\r\n", "\n");
    let js = PANEL_JS.replace("\r\n", "\n");
    for gone in ["read-expansion-list-button", "read_expansion_list", "Read Avenger's List"] {
        assert!(!html.contains(gone) && !js.contains(gone), "{gone} is still on the page");
    }
    for gone in [
        "expansions_from_avenger", "expansions_marks", "currentExpansionsFromAvenger", "as VPS Avenger lists them",
        "selected_expansion_from_database", "currentSelectedFromDatabase", "currentExpansionsFromDatabase",
    ] {
        assert!(!js.contains(gone), "panel.js still has {gone}");
    }
    let search = js.find("function renderExpansionSearch() {").expect("panel.js has renderExpansionSearch");
    let body = &js[search..search + js[search..].find("\n  }\n").unwrap()];
    assert!(
        body.contains("btn.setAttribute('aria-label', `${item.name}, no presets in the database`);")
            && body.contains("btn.textContent = item.name;")
            && body.contains("`Expansions, ${shown.length} result${shown.length === 1 ? '' : 's'}`\n    );"),
        "the mark in the aria-label, the text the name, the list's label its count alone: {body}"
    );
    assert!(!body.contains("textContent = `${item.name}, "), "the mark in the button's text");
    assert!(
        js.contains("name.textContent = selected || (currentExpansionOpen ? unknownOpenExpansionText : '');"),
        "the heading is the name posted"
    );
    assert!(js.contains("const hasKnownExpansion = currentSelectedExpansion !== '';"), "known by the name posted");
    let at = html.find("id=\"settings-expansion-view-trigger\"").expect("Confirm Expansion View");
    let tag = &html[at..at + html[at..].find('>').unwrap()];
    assert!(
        tag.contains("title=\"Tell the page which Expansion View VPS Avenger's grid is in, with no expansion open — nothing in VPS Avenger changes.\""),
        "{tag}"
    );
}

/// Every action the tool's page sends (inventory A of the port's plan, §11.1: its panel/pages.py),
/// 81 names, its Windows-only Settings block's two among them.
const TOOL_PAGE_ACTIONS: &[&str] = &[
    "prev_preset", "next_preset", "start_rename", "pick_load_preset", "pick_save_preset_as",
    "pick_initialize_preset", "cancel_initialize_preset", "undo", "redo_menu", "set_zoom",
    "set_expansion_view", "open_expansion", "close_expansion", "browse_expansion_presets",
    "select_all_expansion_presets", "select_expansion_category", "go_to_loaded_preset_category",
    "select_expansion_preset", "load_expansion_preset", "mark_expansion_preset_favorite",
    "toggle_expansion_favorite_view", "align_open_expansion", "toggle_windows_reaper_rect_x",
    "select_osc_tab", "add_osc", "select_drums_tab", "select_trig_tab", "toggle_osc_mute",
    "toggle_drums_mute", "toggle_trig_mute", "toggle_osc_solo", "toggle_drums_solo",
    "toggle_trig_solo", "osc_menu_item", "drums_menu_item", "trig_menu_item", "rename_osc",
    "select_macro_tab", "add_macro_tab", "delete_macro_tab", "rename_macro", "macro_amount",
    "reset_macro_amount", "toggle_macro_button", "macro_reset_mode", "macro_button_menu_item",
    "osc_control", "reset_osc_control", "osc_filter_control", "toggle_drum_pattern",
    "select_drum_mode", "previous_drumkit", "next_drumkit", "select_drumkit_part",
    "drumkit_column", "toggle_drumkit_button", "drumkit_control", "reset_drumkit_control",
    "drumkit_menu_item", "select_central_tab", "sample_browser_focus", "select_sample_editor_tab",
    "sample_editor_field", "sample_editor_play_mode", "sample_editor_panel_button",
    "toggle_sample_editor_enable", "sample_editor_grid", "sample_editor_eq_type",
    "sample_editor_loop_stretch", "sample_editor_control", "reset_sample_editor_control",
    "central_drum_sq_sequence_menu_item", "central_drum_sq_pattern_player",
    "central_drum_sq_auto_follow", "select_central_drum_sq_pattern",
    "toggle_central_drum_sq_pattern", "central_drum_sq_pattern_menu_item",
    "central_drum_sq_control", "reset_central_drum_sq_control", "central_drum_sq_length",
    "central_drum_sq_speed",
];

/// The tool's page actions the port drops: REAPER's plug-in window edge, which the tool set by hand,
/// is daw-hosts' to derive (modules/daw-hosts: DAW knowledge only there).
const DROPPED: &[&str] = &["toggle_windows_reaper_rect_x"];

/// The port's own: the Settings section's two (slice 10), and the tool's Confirm OSC Count and
/// Confirm Expansion View, from its status menu, in the Settings section (the second Mac test of the
/// page, 2026-10-24). Read Avenger's List, the port's own of slice 9, is gone with the third.
const PORT_OWN: &[&str] = &["set_setting", "reset_settings", "confirm_osc_count", "confirm_expansion_view"];

/// The page's list against what it sends: every name the script quotes as an action and every
/// `data-action` of the markup is on ACTIONS, every name on ACTIONS is sent from somewhere — a
/// quote outside the list, or the markup — and the list is the tool's 81 page actions less the one
/// the port drops, with the port's own four: the port is closed. Then the module: every name on
/// ACTIONS has the words of its answer, and either a handler or a place on the explicit list of
/// actions not built yet, never both — and that list is empty; and nothing the module lists is a
/// name the page does not send. Sent while the page has the keyboard, each one not built would be
/// answered with its words, and each one with a handler is not.
#[test]
fn every_action_the_page_sends_is_on_its_list_and_answered_by_the_module() {
    let actions = page_actions();
    assert_eq!(TOOL_PAGE_ACTIONS.len(), 81, "the tool's page sends 81 actions");
    assert_eq!(actions.len(), 84, "the tool's 81, less the one dropped, and the port's four: {actions:?}");
    let mut sorted = actions.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), actions.len(), "ACTIONS names an action twice");
    let mut wanted: Vec<String> = TOOL_PAGE_ACTIONS
        .iter()
        .filter(|a| !DROPPED.contains(a))
        .chain(PORT_OWN.iter())
        .map(|a| a.to_string())
        .collect();
    wanted.sort();
    assert_eq!(sorted, wanted, "ACTIONS is the tool's page actions, less the one dropped, and the port's own");
    for gone in DROPPED {
        assert!(!actions.iter().any(|a| a == gone), "{gone} is dropped");
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
        T.ok(#P.NOT_BUILT == 0, "the port is closed: " .. table.concat(P.NOT_BUILT, ", "))
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
        -- reset_settings sent with no arguments reset every setting: the defaults, which the test's were.
        for k, d in pairs(S.settingDefs) do T.ok(S.settings[k] == d.default, k .. " at its default") end
    "##
        .replace("@ACTIONS@", &list),
    );
}

/// The page's Settings section (the plan's slice 10): the tool's Windows-only block (its
/// "Windows-only fallback controls") made general — a "Settings" disclosure over a group, after the
/// macros and before the error region — with Correct open Expansion (its action and the tool's help),
/// the expansions list's disclosure, the page key's field and its two buttons, the two switches as
/// aria-pressed buttons, DRUM SQ's pace with its bounds, and Reset Saved Settings, in that order, and
/// no live region of its own; Reset behind the page's own alertdialog, whose Reset sends and whose
/// Cancel does not. The script applies a settings message to the controls and sends nothing from
/// there — no perform, no post, no event — writes the two fields only where they do not have the
/// keyboard, and sets the switches' aria-pressed there only, never at the click.
#[test]
fn the_settings_section_is_the_windows_block_made_general() {
    let html = INDEX_HTML.replace("\r\n", "\n");
    let js = PANEL_JS.replace("\r\n", "\n");
    let tag_at = |id: &str| -> String {
        let at = html.find(&format!("id=\"{id}\"")).unwrap_or_else(|| panic!("index.html has #{id}"));
        let start = html[..at].rfind('<').unwrap();
        html[start..at + html[at..].find('>').unwrap() + 1].to_string()
    };
    let macros = html.find("id=\"macro-panel-group\"").expect("the macros");
    let trigger = html.find("id=\"settings-trigger\"").expect("the Settings trigger");
    let errors = html.find("<!-- =================== Errors").expect("the error region");
    assert!(macros < trigger && trigger < errors, "Settings between the macros and the error region");
    let t = tag_at("settings-trigger");
    assert!(t.contains("aria-expanded=\"false\"") && t.contains("aria-controls=\"settings-menu-items\""), "{t}");
    let g = tag_at("settings-menu-items");
    assert!(g.contains("role=\"group\"") && g.contains("aria-label=\"Settings\"") && g.contains(" hidden"), "{g}");
    let items = [
        "settings-correct-open-expansion",
        "settings-roster-trigger",
        "settings-page-key-input",
        "settings-page-key-change",
        "settings-page-key-default",
        "settings-sounds",
        "settings-leave-daw",
        "settings-drum-sq-cadence",
        "settings-reset-trigger",
    ];
    let mut last = trigger;
    for id in items {
        let at = html.find(&format!("id=\"{id}\"")).unwrap_or_else(|| panic!("index.html has #{id}"));
        assert!(at > last && at < errors, "#{id} in its place");
        last = at;
    }
    let c = tag_at("settings-correct-open-expansion");
    assert!(
        c.contains("data-action=\"align_open_expansion\"")
            && c.contains("title=\"Correct the panel to the expansion currently open in VPS Avenger.\""),
        "{c}"
    );
    for id in ["settings-sounds", "settings-leave-daw"] {
        assert!(tag_at(id).contains("aria-pressed="), "#{id} is a switch");
    }
    let f = tag_at("settings-drum-sq-cadence");
    assert!(f.contains("type=\"number\"") && f.contains("min=\"100\"") && f.contains("max=\"1000\""), "{f}");
    let section = &html[trigger..errors];
    assert!(!section.contains("aria-live"), "no live region in the section");
    let d = tag_at("confirm-reset-dialog");
    assert!(d.contains("role=\"alertdialog\"") && d.contains("aria-modal=\"true\"") && d.contains(" hidden"), "{d}");
    assert!(tag_at("confirm-reset-yes").contains("data-action=\"reset_settings\""), "Reset sends");
    assert!(!tag_at("confirm-reset-no").contains("data-action"), "Cancel sends nothing");
    assert!(js.contains("else if (message.kind === 'settings') applySettings(message);"), "settings messages applied");
    let apply = js.find("function applySettings(m) {").expect("panel.js has applySettings");
    let body = &js[apply..apply + js[apply..].find("\n  }\n").expect("applySettings ends")];
    for sends in ["perform(", "post(", "dispatchEvent", ".click("] {
        assert!(!body.contains(sends), "applySettings sends nothing: {sends}");
    }
    // The two fields: left alone while typed in, but the one that just sent shows the answer — Return
    // sends from the field and leaves the keyboard there — and typing again keeps what is typed.
    assert!(body.contains("const sentFrom = settingsSentFrom;\n    settingsSentFrom = null;"), "one answer writes it");
    for field in ["keyInput", "cadenceInput"] {
        assert!(
            body.contains(&format!("if (document.activeElement !== {field} || sentFrom === {field})")),
            "{field} written while typed in, unless it just sent"
        );
        assert_eq!(js.matches(&format!("settingsSentFrom = {field};")).count(), 1, "{field} marks what it sent");
        assert!(js.contains(&format!("if (settingsSentFrom === {field}) settingsSentFrom = null;")), "typing in {field} again");
    }
    // The messages themselves: Reset to Default resets the page key alone — Reset Saved Settings
    // sends by its data-action, with no key — and each switch sends the other of what it shows.
    assert!(js.contains("perform('reset_settings', { key: 'pageKey' });"), "Reset to Default: the page key");
    assert_eq!(js.matches("perform('reset_settings'").count(), 1, "reset_settings sent by Reset to Default only");
    assert!(
        js.contains("[[soundsToggle, 'sounds'], [leaveDawToggle, 'pageLeavesDawActive']].forEach(([toggle, setting]) => {"),
        "the switches' settings"
    );
    assert!(
        js.contains("perform('set_setting', { key: setting, value: toggle.getAttribute('aria-pressed') !== 'true' });"),
        "a switch sends the other of what it shows"
    );
    for (key, value) in [("roster", "option"), ("drumSqCadence", "text !== '' && Number.isFinite(n) ? n : text")] {
        assert!(js.contains(&format!("perform('set_setting', {{ key: '{key}', value: {value} }});")), "{key} sent");
    }
    assert!(js.contains("perform('set_setting', { key: 'pageKey', value: keyInput ? keyInput.value.trim() : '' });"), "the key sent");
    for toggle in ["soundsToggle", "leaveDawToggle"] {
        let set = format!("{toggle}.setAttribute('aria-pressed'");
        assert_eq!(js.matches(&set).count(), 1, "{toggle}'s aria-pressed set once");
        assert!(body.contains(&set), "{toggle}'s aria-pressed set in applySettings");
    }
}
