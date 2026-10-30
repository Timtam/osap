//! The second Mac test of VPS Avenger's page (2026-10-24), its findings in the overlay runtime and
//! daw-hosts, run for real against the scripted host of `overlay_host_panel_tests.rs` with the
//! window prelude's real matcher, and daw-hosts loaded from its file.
//!
//! Logic's "Load Preset..." panel is not a plug-in window (finding 5): every embedded overlay had
//! asked its identity question in it. And the focus key's own click into the plug-in's panel, after
//! which Logic reports no focus at all, begins a stay there (finding 9, the maintainer's decision of
//! 2026-10-24); any other empty focus chain still begins none.
//!
//! And from the second Mac tester's log (2026-10-30): Ableton Live's file panels are no plug-in
//! windows either.

use super::overlay_host_panel_tests::{run_with_helpers, SPECIFICITY_ARBITER};
use super::overlay_page_tests::PG;
use super::page_example_tests::PAGE_WEBVIEW;

/// What these scenarios add to the host-panel helpers: Logic's file panel, a recheck that keeps
/// what it was told, and a focus round handed the round's `info`, as the host hands it.
const MT2: &str = r##"
local S = T.S
-- Logic's "Load Preset..." panel: a titled AXDialog of Logic's, the system's open panel.
T.LOAD_PANEL = { id = 31, title = "Load Preset...", class = "AXWindow/AXDialog/open-panel", app = T.MAC_LOGIC,
  bounds = { x = 353, y = 111, w = 1214, h = 726 }, client = { x = 353, y = 111, w = 1214, h = 726 } }
-- A button of Logic's header: the DAW's own chrome, above the plug-in's panel (which begins at 202,118).
T.HEADER = { id = 41, class = "AXButton//", bounds = { x = 220, y = 70, w = 20, h = 20 } }
S.rechecked = {}
-- The onFocus callbacks apart from the activation triggers, so that a round hands its `info` to
-- them alone, as the host's does; T.event still calls both.
S.focusCbs = {}
rawset(T.host.window, "onFocus", function(cb)
  S.triggers[#S.triggers + 1] = cb
  S.focusCbs[#S.focusCbs + 1] = cb
end)
rawset(T.host.window, "recheck", function(opts)
  S.rechecks += 1
  S.rechecked[#S.rechecked + 1] = opts or false
end)
-- A focus round, as the host delivers one: the epoch turns over, and every onFocus callback is
-- handed the round's `info`.
function T.round(info)
  S.epoch += 1
  for _, cb in ipairs(S.focusCbs) do T.call("focus", cb, info or {}) end
end
"##;

fn run_mac(scenario: &str) {
    run_with_helpers("macos", &[("second Mac test helpers", MT2)], scenario)
}

/// `run_mac` with the page tests' helpers (`overlay_page_tests.rs`): the scripted page, the
/// specificity arbiter and the synth's overlay with its page, ending on its hooks paired.
fn run_page_on(os: &str, scenario: &str) {
    let scenario = format!("{scenario}
T.pairsAll()
");
    run_with_helpers(
        os,
        &[
            ("second Mac test helpers", MT2),
            ("the scripted page", PAGE_WEBVIEW),
            ("specificity arbiter", SPECIFICITY_ARBITER),
            ("page helpers", PG),
        ],
        &scenario,
    )
}

/// Finding 5: Logic's file panel is no plug-in window of daw-hosts' Logic entry — its identifier
/// ends in `-panel`, a plug-in window's is empty — so an overlay bound with `daw.all` asks nothing
/// there, while the plug-in window is still one. The focus key, with the panel open over it, finds
/// the plug-in window.
#[test]
fn logics_file_panel_is_not_a_plugin_window() {
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        local asked = 0
        T.at(T.LOAD_PANEL)
        local o = T.bind({ hosts = T.daw.all, control = { windows = "x" },
          identify = function() asked += 1; return true end }, "Mine")
        assert(not o.active and asked == 0, "nothing asked in the file panel: " .. T.dump())
        assert(not T.host.window.test(T.daw.matcher(T.daw.logic[1]), T.LOAD_PANEL), "the panel is not matched")
        assert(T.host.window.test(T.daw.matcher(T.daw.logic[1]), T.LOGIC), "the plug-in window still is")
        T.show(T.LOGIC)
        assert(o.active and asked == 1, T.dump())
        -- The focus key, the panel listed first.
        T.daw.activate()
        S.listed = { T.LOAD_PANEL, T.LOGIC }
        T.at(T.LOGIC)
        S.hotkeyFns["Cmd+Shift+F6"]()
        assert(S.focusCalls[#S.focusCalls] == 21, "the plug-in window, not the panel: " .. T.dump())
    "##);
}

/// The second Mac tester's log (2026-10-30): Ableton Live's Save panel, a titled
/// `AXWindow/AXDialog/save-panel` of Live's, was a plug-in window of daw-hosts' Live entry, and every
/// embedded overlay walked it: 36 s in one turn of the event loop, and the guard stopped a module.
/// Live's entry leaves the system's open and save panels out as Logic's does, by their identifier.
/// Its other titled windows, the Set's among them, it still takes until a probe from a Live plug-in
/// window says what tells them apart (TODO.md).
#[test]
fn lives_file_panels_are_not_plugin_windows() {
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        local LIVE = { pid = 7000, exe = "Live", name = "Live", bundleId = "com.ableton.live" }
        local SAVE = { id = 61, title = "Save", class = "AXWindow/AXDialog/save-panel", app = LIVE,
          bounds = { x = 380, y = 200, w = 1156, h = 448 }, client = { x = 380, y = 232, w = 1156, h = 416 } }
        local OPEN = { id = 62, title = "Open", class = "AXWindow/AXDialog/open-panel", app = LIVE,
          bounds = { x = 380, y = 200, w = 1156, h = 448 }, client = { x = 380, y = 232, w = 1156, h = 416 } }
        local SET = { id = 51, title = "Untitled", class = "AXWindow/AXStandardWindow/", app = LIVE,
          bounds = { x = 0, y = 25, w = 1440, h = 875 }, client = { x = 0, y = 25, w = 1440, h = 875 } }
        local live = T.daw.matcher(T.daw.ableton[4])
        S.listed = { SET, SAVE, OPEN }
        T.at(SAVE)
        local asked = 0
        local o = T.bind({ hosts = T.daw.all, control = { windows = "x" },
          identify = function() asked += 1; return true end }, "Mine")
        assert(not o.active and asked == 0, "nothing asked in Live's save panel: " .. T.dump())
        assert(not T.host.window.test(live, SAVE), "the save panel is not matched")
        assert(not T.host.window.test(live, OPEN), "nor the open panel")
        T.show(OPEN)
        assert(not o.active and asked == 0, "nothing asked in Live's open panel: " .. T.dump())
        assert(T.count("panel of 'Save'") == 0 and T.count("panel of 'Open'") == 0, T.dump())
        -- The Set's window is still taken, as before.
        assert(T.host.window.test(live, SET), "the Set's window still is")
        T.show(SET)
        assert(o.active and asked == 1, T.dump())
    "##);
}

/// Finding 9 and the decision of 2026-10-24: the keyboard on Logic's header, the focus key clicks
/// into the plug-in's panel, and Logic then reports an empty chain. daw-hosts tells every overlay of
/// its click and says the title; in the round that click asked for, the empty chain begins a stay
/// in that panel, `identify` is asked once, and the line says so. The stay then goes on over the
/// empty chain as any stay does.
#[test]
fn the_focus_keys_own_click_begins_a_stay_on_an_empty_chain() {
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        T.daw.activate()
        local press = S.hotkeyFns["Cmd+Shift+F6"]
        S.listed = { T.LOGIC }
        T.at(T.LOGIC, { T.HEADER, T.LOGIC })
        local asked = 0
        local o = T.bind({ hosts = T.daw.logic, control = { windows = "x" },
          identify = function() asked += 1; return true end }, "Avenger")
        assert(not o.active and asked == 0, "the keyboard on Logic's header: " .. T.dump())
        press()
        assert(#S.clicks == 1 and S.clicks[1][1] == 205 and S.clicks[1][2] == 121, T.dump())
        -- Logic reports nothing after the click.
        S.chain = {}
        T.run(300)
        assert(S.speech[#S.speech].text == "Inst 1", "the title: " .. S.speech[#S.speech].text)
        local told = S.rechecked[#S.rechecked]
        assert(told and told.entered and told.entered.window == 21 and told.entered.x == 205
          and told.entered.y == 121, "the click is told of: " .. T.dump())
        assert(T.count("[daw-hosts] nothing is read after the click into 'Inst 1'; asking every overlay to look "
          .. "again, told of the click at 205,121") == 1, T.dump())
        -- The round it asked for, as the host hands it.
        T.round({ entered = { window = 21, x = 205, y = 121, module = "com.platform.daw-hosts", at = 1 } })
        assert(o.active and asked == 1, "the click begins a stay: " .. T.dump())
        assert(T.count("[overlay] 'Avenger': com.platform.daw-hosts's click into 'Inst 1' at 205,121 begins a "
          .. "stay — Logic Pro reports no focus after it") == 1, T.dump())
        -- Later rounds read nothing either: the stay goes on, nothing asked again.
        T.round()
        T.event()
        assert(o.active and asked == 1, T.dump())
        assert(T.count("the stay goes on there") == 0, "the stay is the click's, not said again: " .. T.dump())
    "##);
}

/// Any other empty chain begins no stay: one in a round without a click, one whose click was into
/// another window, one whose point lies outside the plug-in's panel (Logic's header), and one read
/// after the click's round — by a poll's evaluation, outside any round, once the stay the click
/// began has ended on Logic's header: the click is that round's alone. The focus key with nothing
/// read BEFORE its press still clicks nothing and says it could not tell; and after its click, with
/// another window in front by then, it tells no overlay of the click.
#[test]
fn no_other_empty_chain_begins_a_stay() {
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        T.daw.activate()
        S.listed = { T.LOGIC }
        T.at(T.LOGIC, {})
        local asked = 0
        local o = T.bind({ hosts = T.daw.logic, control = { windows = "x" },
          identify = function() asked += 1; return true end }, "Avenger")
        T.round({ entered = { window = 21, x = 205, y = 121, module = "m" } })
        assert(o.active and asked == 1, "the click begins a stay: " .. T.dump())
        -- A poll's evaluation with the keyboard on Logic's header ends it; the next reads nothing.
        S.chain = { T.HEADER, T.LOGIC }
        T.turn()
        o:_recheck()
        assert(not o.active, "the stay ends on Logic's header: " .. T.dump())
        S.chain = {}
        T.turn()
        o:_recheck()
        assert(not o.active and asked == 1, "the click begins no stay outside its round: " .. T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        T.daw.activate()
        S.listed = { T.LOGIC, T.LOAD_PANEL }
        T.at(T.LOGIC, { T.HEADER, T.LOGIC })
        S.hotkeyFns["Cmd+Shift+F6"]()
        assert(#S.clicks == 1, T.dump())
        -- Nothing read after the click, and the window in front is another by then.
        S.front, S.chain = T.LOAD_PANEL, {}
        T.run(300)
        for _, told in ipairs(S.rechecked) do assert(not (told and told.entered), "a click told of: " .. T.dump()) end
        assert(S.speech[#S.speech].text == "could not tell whether the keyboard is in Inst 1", T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        T.modules("modules/daw-hosts/")
        T.daw.activate()
        S.listed = { T.LOGIC }
        T.at(T.LOGIC, {})
        local asked = 0
        local o = T.bind({ hosts = T.daw.logic, control = { windows = "x" },
          identify = function() asked += 1; return true end }, "Avenger")
        T.round()
        assert(not o.active, "no click: " .. T.dump())
        T.round({ entered = { window = 99, x = 205, y = 121, module = "m" } })
        assert(not o.active, "another window's: " .. T.dump())
        T.round({ entered = { window = 21, x = 230, y = 80, module = "m" } })
        assert(not o.active, "on Logic's header, outside the panel: " .. T.dump())
        -- A key, a poll, a tick after a round with a click inside: none of them carries it.
        T.event()
        T.tick(2)
        assert(not o.active and asked == 0, T.dump())
        -- The focus key with the chain empty before it: no click, as before.
        S.hotkeyFns["Cmd+Shift+F6"]()
        assert(#S.clicks == 0 and S.speech[#S.speech].text == "could not tell whether the keyboard is in Inst 1", T.dump())
        assert(#S.rechecked == 0, "no recheck told of anything: " .. T.dump())
    "##);
}

/// M6 and R5 of the second Mac test: the page key with the overlay not in front, Logic's plug-in
/// window in front and nothing read of where the keyboard is (no stay, so no overlay). The page
/// opens from the window: `identify` is asked on the DAW's panel, the overlay comes up on it through
/// the key's hold, the page is shown beside the window, and when it comes forward the group is away
/// as from the plug-in. The way back ends the key's hold.
#[test]
fn the_page_key_opens_the_page_from_a_plugin_window_whose_identify_says_yes() {
    run_page_on("macos", r##"
        local S = T.S
        T.show(T.W, {})
        local ov = T.synth()
        T.ok(not ov.active and S.identifies == 0, "an empty chain begins no stay: " .. T.dump())
        S.pg.ready = true
        T.key()
        T.ok(S.identifies == 1, "identify asked on the panel: " .. S.identifies)
        T.ok(#S.pg.shows == 1, "the page shown: " .. T.dump())
        T.ok(T.count("[page] 'Synth': the page key with the overlay not in front, in its plug-in's window 'Inst 1': "
          .. "the overlay comes up on the DAW's panel there, and the page opens") == 1, T.dump())
        T.pageFront()
        T.ok(ov.away, "the group is away: " .. T.dump())
        -- Back in the plug-in: the key's hold is out, the ordinary match decides.
        T.key()
        T.land()
        T.run(200)
        local held = 0
        for _, c in ipairs(ov.contexts) do if c.keyed then held += 1 end end
        T.ok(held == 0 and not ov.away, "the key's hold is out: " .. held)
    "##);
}

/// With the keyboard on Logic's own header the page opens from the window too: where the keyboard is
/// is not asked. A plug-in window whose `identify` says no, another application's window and a
/// binding without `identify` are refused as before, and the hold the key tried is taken out again.
#[test]
fn the_page_key_from_a_window_asks_identify_and_refuses_another_plugin() {
    run_page_on("macos", r##"
        local S = T.S
        T.show(T.W, T.CHROME)
        local ov = T.synth()
        T.ok(not ov.active, T.dump())
        S.pg.ready = true
        T.key()
        T.ok(#S.pg.shows == 1, "opened over Logic's header: " .. T.dump())
    "##);
    run_page_on("macos", r##"
        local S = T.S
        T.show(T.W, {})
        local ov = T.synth({ binding = { identify = function() S.identifies += 1; return false end } })
        S.pg.ready = true
        T.key()
        T.ok(#S.pg.shows == 0 and S.identifies == 1, "another plug-in: " .. T.dump())
        T.ok(T.last() == "Synth is not in front; its page opens from there.", T.last())
        T.ok(not ov.active, "no hold left: " .. T.dump())
        for _, c in ipairs(ov.contexts) do T.ok(not c.keyed, "a held context is left") end
        T.show(T.OTHER)
        T.key()
        T.ok(#S.pg.shows == 0 and S.identifies == 1, "another application's window: " .. T.dump())
    "##);
    run_page_on("macos", r##"
        local S = T.S
        T.show(T.W, {})
        T.synth({ binding = { identify = false } })
        S.pg.ready = true
        T.key()
        T.ok(#S.pg.shows == 0, "no identify, no page from the window: " .. T.dump())
    "##);
}

/// On Windows no DAW entry gives a panel: the page key with the keyboard on REAPER's FX list is
/// refused, as it always was.
#[test]
fn the_page_key_from_a_window_needs_the_daws_panel() {
    run_page_on("windows", r##"
        local S = T.S
        T.show(T.W, T.CHROME)
        T.synth()
        S.pg.ready = true
        T.key()
        T.ok(#S.pg.shows == 0 and S.identifies == 0, T.dump())
    "##);
}

/// What the window-of-the-press scenarios share: Logic's main window, Avenger's own window in the
/// window list, the file panel's, and an overlay whose MENU is chosen in through `chooseMenuItem` — a
/// menu drawn inside the plug-in, seen by a test of the module's own (S.inside) — with newWindow
/// beside it, which sees the panel the item opens.
const DIALOG: &str = r##"
local S = T.S
T.modules("modules/daw-hosts/")
T.MAIN = { id = 20, title = "Untitled - Tracks", class = "AXWindow/AXStandardWindow/", app = T.MAC_LOGIC,
  bounds = { x = 0, y = 25, w = 1440, h = 875 }, client = { x = 0, y = 25, w = 1440, h = 875 } }
T.INST = { id = 21, layer = 0, class = "", x = 200, y = 30, w = 1000, h = 800 }
T.PANEL = { id = 31, layer = 0, class = "", x = 353, y = 111, w = 1214, h = 726 }
rawset(T.host.window, "windowsOf", function() return S.windows end)
-- Nothing is drawn over the plug-in at an item's point.
rawset(T.host.window, "ownsPoint", function() return true end)
S.windows = { T.INST }
S.listed = { T.MAIN, T.LOGIC, T.LOAD_PANEL }
S.inside = false
-- S.insideAt: where the drawn menu answers now (default 300,200 100x80); another place after the item's
-- click is the item taken.
local inside = { name = "inside", cheap = true,
  test = function(_, answer) answer(S.inside and (S.insideAt or { x = 300, y = 200, w = 100, h = 80 }) or false) end }
-- The overlay in Logic's plug-in window, its MENU chosen in: "Load preset", the item opening the panel.
function T.avenger()
  T.at(T.LOGIC)
  local o = T.O.new("Avenger")
  o:addCustomButton({ label = "Undo", onActivate = function() end })
  o:attachEmbedded({ hosts = T.daw.all, control = { windows = "x" }, identify = function() return true end },
    { menus = { inside, T.O.menuTests.newWindow } })
  T.runDue()
  assert(o.active, T.dump())
  return o
end
function T.loadPreset(o)
  local ok, err
  assert(T.call("key", function()
    ok, err = o:chooseMenuItem({ label = "MENU", at = { 10, 10 }, menuItem = { 5, 5 }, retries = 0 })
  end) == "Ran")
  assert(ok == true, "opened: " .. T.what(err))
  S.inside = true
  T.tick(1)
  S.inside = false
end
-- The panel the item opened, in front.
function T.panelUp(o)
  S.windows = { T.INST, T.PANEL }
  T.show(T.LOAD_PANEL, { T.LOAD_PANEL })
  T.tick(2)
  assert(o:_heldOnMenu(), "held over the panel: " .. T.dump())
end
"##;

fn run_dialog(scenario: &str) {
    run_with_helpers("macos", &[("second Mac test helpers", MT2), ("the panel", DIALOG)], scenario)
}

/// Finding 8: Logic's "Load Preset..." panel, opened by an item of the plug-in's MENU, closes, and
/// Logic leaves the keyboard in its main window. The window of the press is brought back — once,
/// while it is listed with its class, title and size — and the ordinary match decides from there.
#[test]
fn the_plugin_window_is_brought_back_after_a_dialog_of_its_press_closes() {
    run_dialog(r##"
        local S = T.S
        local o = T.avenger()
        T.loadPreset(o)
        assert(#S.clicks == 2, "the opener and the item: " .. T.dump())
        T.panelUp(o)
        -- It closes; the keyboard is in Logic's main window.
        S.windows = { T.INST }
        S.listed = { T.MAIN, T.LOGIC }
        S.focusLands = function() S.front, S.chain = T.LOGIC, { T.LOGIC } end
        T.show(T.MAIN, { T.MAIN })
        T.tick(2)
        assert(#S.focusCalls == 1 and S.focusCalls[1] == 21, "the window of the press: " .. T.dump())
        assert(T.count("[menu] 'Avenger': the window that appeared after 'MENU' was pressed, 'Load Preset...' "
          .. "(Logic Pro X), has gone, and the keyboard is in 'Untitled - Tracks' (Logic Pro X) of the same "
          .. "application, which is no plug-in window — bringing back 'Inst 1' (Logic Pro X), where 'MENU' was "
          .. "pressed: accepted") == 1, T.dump())
        T.land()
        assert(o.active, "back in front: " .. T.dump())
        T.tick(10)
        T.event()
        assert(#S.focusCalls == 1, "once per press: " .. #S.focusCalls)
    "##);
}

/// Nothing is brought back when the window the keyboard went to after the dialog is another plug-in
/// window of the DAW, while the dialog is still listed (only behind another window), once the window
/// of the press changed its size, or after a window that came with no item clicked — a menu's own.
#[test]
fn nothing_is_brought_back_to_a_plugin_window_or_while_the_dialog_is_listed() {
    let scenario = |front: &str, listed: &str, resized: bool, item: bool| {
        format!(r##"
        local S = T.S
        local INST2 = {{ id = 22, title = "Inst 2", class = "AXWindow/AXDialog/", app = T.MAC_LOGIC,
          bounds = {{ x = 300, y = 40, w = 900, h = 700 }}, client = {{ x = 300, y = 40, w = 900, h = 700 }} }}
        S.listed = {{ T.MAIN, T.LOGIC, T.LOAD_PANEL, INST2 }}
        local o = T.avenger()
        if {item} then
          T.loadPreset(o)
        else
          o.controls[1].opensMenu = true
          T.press(o)
        end
        T.panelUp(o)
        S.windows = {{ T.INST }}
        S.listed = {listed}
        -- A table of its own, as the host lists one: the press kept the one it was handed.
        if {resized} then
          local smaller = table.clone(T.LOGIC)
          smaller.bounds = {{ x = 200, y = 30, w = 1000, h = 700 }}
          S.listed = {{ T.MAIN, smaller }}
        end
        T.show({front}, {{ {front} }})
        T.tick(2)
        assert(#S.focusCalls == 0, "nothing brought back: " .. T.dump())
        "##)
    };
    run_dialog(&scenario("INST2", "{ T.MAIN, T.LOGIC, INST2 }", false, true));
    run_dialog(&scenario("T.MAIN", "{ T.MAIN, T.LOGIC, T.LOAD_PANEL }", false, true));
    run_dialog(&scenario("T.MAIN", "{ T.MAIN, T.LOGIC }", true, true));
    run_dialog(&scenario("T.MAIN", "{ T.MAIN, T.LOGIC }", false, false));
}

/// O:afterDialog (since 0.14), for a press of the overlay's own outside a trip (finding 18, VPS
/// Avenger's ring): called in the item's onDone, Logic's panel coming forward is the dialog — the
/// overlay holding its place over it — and nothing bounds the wait once it was seen; fn(o, true) runs
/// once, on the turn after the plug-in's window is in front again with the overlay on it. With no
/// dialog within 8 seconds, fn(o, false). A second call drops the first, whose fn is never told; the
/// overlay out of the front, `not_in_front` and nothing waits; a call with no function raises.
#[test]
fn after_dialog_waits_for_the_dialog_an_item_opened_and_the_plugin_again() {
    run_dialog(r##"
        local S = T.S
        local o = T.avenger()
        local told = {}
        local ok, err
        assert(T.call("key", function()
          ok, err = o:chooseMenuItem({ label = "MENU", at = { 10, 10 }, menuItem = { 5, 5 }, retries = 0,
            onDone = function(m, chosen)
              if not chosen then return end
              local okD, errD = m:afterDialog(function(ov, seen)
                told[#told + 1] = { ov = ov, seen = seen, active = ov.active }
              end)
              assert(okD == true and errD == nil, T.what(errD))
            end })
        end) == "Ran")
        assert(ok == true, "opened: " .. T.what(err))
        S.inside = true
        T.tick(1)
        S.inside = false
        assert(T.count("[menu] 'Avenger': waits for the dialog that comes next — a window of pid") == 1, T.dump())
        T.panelUp(o)
        assert(T.count("[menu] 'Avenger': a dialog came forward — 'Load Preset...' (Logic Pro X)") == 1, T.dump())
        -- The user's time in the panel: past the 8 seconds, nothing told.
        T.run(9000)
        assert(#told == 0, "nothing bounds the wait once the dialog was seen: " .. #told)
        -- It closes; the window of the press brought back, the overlay on it.
        S.windows = { T.INST }
        S.listed = { T.MAIN, T.LOGIC }
        S.focusLands = function() S.front, S.chain = T.LOGIC, { T.LOGIC } end
        T.show(T.MAIN, { T.MAIN })
        T.tick(2)
        T.land()
        T.tick(1)
        assert(o.active and #told == 1 and told[1].seen == true and told[1].active and told[1].ov == o,
          "told once, back on the plug-in: " .. #told .. " " .. T.dump())
        assert(T.count("[menu] 'Avenger': 'Inst 1' (Logic Pro X) is in front again after the dialog") == 1, T.dump())
        T.tick(5)
        T.event()
        T.tick(1)
        assert(#told == 1, "once: " .. #told)
        -- No dialog: fn(o, false) at 8 seconds, not before.
        local seenNone = {}
        assert(o:afterDialog(function(_, seen) seenNone[#seenNone + 1] = seen end) == true)
        T.run(7800)
        assert(#seenNone == 0, "not before 8 seconds")
        T.run(300)
        assert(#seenNone == 1 and seenNone[1] == false and T.count("[menu] 'Avenger': no dialog came within 8000 ms") == 1,
          T.dump())
        -- A second call drops the first.
        local a, b = {}, {}
        o:afterDialog(function(_, seen) a[#a + 1] = seen end)
        o:afterDialog(function(_, seen) b[#b + 1] = seen end)
        assert(T.count("ms ago is dropped — another begins") == 1, T.dump())
        T.run(8100)
        assert(#a == 0 and #b == 1 and b[1] == false, "the second told alone: " .. #a .. " " .. #b)
        -- Raises for a mistake; not in front, nothing waits.
        assert(not pcall(o.afterDialog, o, "fn"), "raises")
        o:_deactivate()
        local okN, errN = o:afterDialog(function() error("never") end)
        assert(okN == false and T.is(errN, "not_in_front", "the overlay is not in front"), T.what(errN))
        T.run(8100)
    "##);
}

/// O:afterDialog called in an `onDone` of a choice with its `retries` at the default, which comes
/// once the item was taken and the dialog may be in front already: the window noted is the
/// overlay's own, the one the press was made in, never the dialog. "taken": the panel comes over the
/// menu, which the menu test then answers in another place, while the overlay holds its place over
/// the panel — seen at the call, and fn(o, true) once the plug-in is back, never the plug-in taken
/// for the dialog; the same where the menu test answers so only on a later tick, with no window
/// event after it. "left": the menu gone and nothing held over the panel, the overlay leaves the
/// front for it — the call in that onDone is accepted, the panel seen, and fn(o, true) once the
/// overlay is in front on the plug-in again.
#[test]
fn after_dialog_in_a_choices_on_done_notes_the_window_of_the_press() {
    let scenario = |how: &str| {
        let left = how == "left";
        let later = how == "taken later";
        format!(r##"
        local S = T.S
        local o = T.avenger()
        local told, chosen = {{}}, {{}}
        assert(T.call("key", function()
          o:chooseMenuItem({{ label = "MENU", at = {{ 10, 10 }}, menuItem = {{ 5, 5 }},
            onDone = function(m, ok, err)
              chosen[#chosen + 1] = tostring(ok) .. " " .. tostring(m.active)
              if not ok then return end
              local okD, errD = m:afterDialog(function(ov, seen)
                told[#told + 1] = {{ seen = seen, active = ov.active }}
              end)
              assert(okD == true and errD == nil, T.what(errD))
            end }})
        end) == "Ran")
        S.inside = true
        T.tick(1)
        assert(#S.clicks == 2, "the opener and the item: " .. T.dump())
        if {left} then
          -- The menu gone, and the panel not among the windows the menu test lists: nothing held.
          S.inside = false
          T.show(T.LOAD_PANEL, {{ T.LOAD_PANEL }})
          T.tick(1)
          assert(not o.active, "out of the front: " .. T.dump())
          assert(T.count("the overlay left the front after it") == 1, T.dump())
        else
          -- The panel over the menu, which the menu test now answers in another place — at once, or
          -- on a later tick of the menu tests, with no window event after it.
          if not {later} then S.insideAt = {{ x = 400, y = 300, w = 100, h = 80 }} end
          S.windows = {{ T.INST, T.PANEL }}
          T.show(T.LOAD_PANEL, {{ T.LOAD_PANEL }})
          T.tick(1)
          if {later} then
            assert(#chosen == 0, "not taken yet")
            S.insideAt = {{ x = 400, y = 300, w = 100, h = 80 }}
            T.tick(1)
          end
          S.inside, S.insideAt = false, nil
          assert(o:_heldOnMenu(), "held over the panel: " .. T.dump())
          assert(T.count("test 'inside' sees another window in its place") == 1, T.dump())
        end
        assert(#chosen == 1 and string.find(chosen[1], "^true"), table.concat(chosen, ","))
        assert(T.count("[menu] 'Avenger': waits for the dialog that comes next — a window of pid 6000 other than "
          .. "'Inst 1' (Logic Pro X)") == 1, T.dump())
        assert(T.count("[menu] 'Avenger': a dialog came forward — 'Load Preset...' (Logic Pro X)") == 1, T.dump())
        T.run(9000)
        assert(#told == 0, "nothing bounds the wait once the dialog was seen: " .. #told)
        -- The panel closes, and the plug-in has the keyboard again.
        S.windows = {{ T.INST }}
        S.listed = {{ T.MAIN, T.LOGIC }}
        T.show(T.LOGIC, {{ T.LOGIC }})
        T.tick(2)
        assert(o.active and #told == 1 and told[1].seen == true and told[1].active, "told once, back on the plug-in: "
          .. #told .. " " .. T.dump())
        assert(T.count("[menu] 'Avenger': 'Inst 1' (Logic Pro X) is in front again after the dialog") == 1, T.dump())
        assert(T.count("after the dialog, not on") == 0, T.dump())
        T.run(9000)
        assert(#told == 1, "once: " .. #told)
        "##)
    };
    run_dialog(&scenario("taken"));
    run_dialog(&scenario("taken later"));
    run_dialog(&scenario("left"));
}

/// The press's menu as a window of its own is no dialog — held over before its item was clicked
/// (the first answer placing nothing), or its item clicked in the very round that sees it come and
/// begins the hold: O:afterDialog called at the item's click (retries 0), the overlay holding its
/// place over the menu, notes the window of the press and does not take the menu for the dialog;
/// nor called before the press, while the menu is held over and nothing is clicked in it yet. The
/// menu closes with nothing after it: fn(o, false) at the bound, and no dialog seen. With Logic's
/// panel after the menu: the panel is the dialog, and fn(o, true) once the plug-in is back.
#[test]
fn after_dialog_does_not_take_the_presss_own_menu_window_for_the_dialog() {
    let scenario = |panel: bool, early: bool, before: bool| {
        format!(r##"
        local S = T.S
        local MENU_WIN = {{ id = 33, layer = 0, class = "", x = 300, y = 200, w = 120, h = 90 }}
        local MENU_FRONT = {{ id = 33, title = "", class = "AXWindow/AXUnknown/", app = T.MAC_LOGIC,
          bounds = {{ x = 300, y = 200, w = 120, h = 90 }}, client = {{ x = 300, y = 200, w = 120, h = 90 }} }}
        local o = T.avenger()
        local told = {{}}
        -- Early: the answers place nothing until S.place, so the hold over the menu begins before the
        -- click.
        local answers = 0
        S.place = not {early}
        local function item()
          answers += 1
          if not S.place then return false, "not yet" end
          return {{ 5, 5 }}
        end
        local function wait(m) assert(m:afterDialog(function(_, seen) told[#told + 1] = seen end) == true) end
        assert(T.call("key", function()
          if {before} then wait(o) end
          o:chooseMenuItem({{ label = "MENU", at = {{ 10, 10 }}, menuItem = item, retries = 0,
            onDone = function(m, ok)
              if ok and not {before} then wait(m) end
            end }})
        end) == "Ran")
        -- The menu, a window of Logic's, held over; its item clicked in it.
        S.windows = {{ T.INST, MENU_WIN }}
        T.show(MENU_FRONT, {{ MENU_FRONT }})
        T.tick(1)
        if {before} then
          assert(#S.clicks == 1 and o:_heldOnMenu() and answers >= 1, "held over the menu, its item not clicked yet: "
            .. #S.clicks .. " " .. answers)
          T.event()
          T.tick(1)
          assert(T.count("a dialog came forward") == 0, "the menu is no dialog before its click: " .. T.dump())
        end
        S.place = true
        T.tick(2)
        assert(#S.clicks == 2, "the opener and the item: " .. answers .. " " .. T.dump())
        assert(o:_heldOnMenu(), "held over the menu")
        assert(T.count("[menu] 'Avenger': waits for the dialog that comes next — a window of pid 6000 other than "
          .. "'Inst 1' (Logic Pro X)") == 1, T.dump())
        T.tick(2)
        assert(T.count("a dialog came forward") == 0, "the menu is no dialog: " .. T.dump())
        if {panel} then
          S.windows = {{ T.INST, T.PANEL }}
          T.show(T.LOAD_PANEL, {{ T.LOAD_PANEL }})
          T.tick(2)
          assert(T.count("[menu] 'Avenger': a dialog came forward — 'Load Preset...' (Logic Pro X)") == 1, T.dump())
        end
        S.windows = {{ T.INST }}
        S.listed = {{ T.MAIN, T.LOGIC }}
        T.show(T.LOGIC, {{ T.LOGIC }})
        T.tick(2)
        assert(o.active, "back on the plug-in: " .. T.dump())
        if {panel} then
          assert(#told == 1 and told[1] == true, "told after the panel and the plug-in: " .. #told)
        else
          assert(#told == 0, "nothing yet: " .. #told)
          T.run(8100)
          assert(#told == 1 and told[1] == false and T.count("a dialog came forward") == 0, "no dialog: " .. T.dump())
        end
        "##)
    };
    run_dialog(&scenario(false, true, false));
    run_dialog(&scenario(true, true, false));
    run_dialog(&scenario(false, false, false));
    run_dialog(&scenario(false, true, true));
}

/// A hold of keys on one plug-in window ends "left" once the overlay is in front on another it also
/// matches — a second instance of the plug-in — where the hold's keys, scoped to the first, would
/// never come: onEnd told once, with why, and nothing held any more. A recheck on the same window
/// ends nothing.
#[test]
fn a_hold_of_keys_ends_when_the_overlay_is_in_front_on_another_window() {
    run_dialog(r##"
        local S = T.S
        local INST2 = { id = 22, title = "Inst 2", class = "AXWindow/AXDialog/", app = T.MAC_LOGIC,
          bounds = { x = 300, y = 40, w = 900, h = 700 }, client = { x = 300, y = 40, w = 900, h = 700 } }
        S.listed = { T.MAIN, T.LOGIC, INST2 }
        local o = T.avenger()
        local ends = {}
        assert(o:holdKeys({ keys = { "PageUp", "PageDown" }, onKey = function() end,
          onEnd = function(_, how, info) ends[#ends + 1] = how .. ": " .. tostring(info.why) end }) == true)
        T.event()
        T.tick(1)
        assert(#ends == 0 and o._hold, "the same window: " .. table.concat(ends, ","))
        T.show(INST2, { INST2 })
        T.tick(1)
        assert(o.active and o:hwnd() == 22, "in front on the other: " .. T.dump())
        assert(#ends == 1 and ends[1] == "left: the overlay is in front on another window (22), not on the one its "
          .. "hold began on (21)", table.concat(ends, ","))
        assert(not o._hold and not S.holding["PageUp"] and not S.holding["PageDown"], "nothing held")
        assert(T.count("[keys] 'Avenger': its hold of PageUp, PageDown ends (left)") == 1, T.dump())
        T.tick(2)
        assert(#ends == 1, "once")
    "##);
}

/// `O:holdKeys`' `swallow` (runtime 0.16; the maintainer's decision of 2026-10-25): its keys are
/// the hold's for as long as it lasts, over the overlay's own claims of them, and do nothing — Tab
/// moves no focus and reaches neither the plug-in nor `onKey`, also while a menu flag lets the
/// module's other keys through, and also once the overlay has captured Tab again while the hold
/// stands; the module's sentence is said once per hold, interrupting, at the first key swallowed, and
/// nothing at the start. Every end gives them back: "ended" and "replaced" to the overlay — Tab moves
/// the focus again — and "left" with every other key. A mistake in `swallow` raises at the module's
/// line.
#[test]
fn hold_keys_swallows_its_swallow_keys_says_once_and_gives_them_back() {
    run_with_helpers("windows", &[], r##"
        local S = T.S
        local SAY = "Return confirms, Escape cancels."
        local o = T.overlay(nil, "Avenger")
        o.focus = 2
        o:_syncNativeKeys()
        local keys, ends = {}, {}
        local function hold()
          return o:holdKeys({ keys = { "PageUp", "Return", "Escape" }, swallow = { keys = { "Tab", "Shift+Tab" }, say = SAY },
            onKey = function(_, key)
              keys[#keys + 1] = key
              return key == "Return" or key == "Escape"
            end,
            onEnd = function(_, how) ends[#ends + 1] = how end })
        end
        local said = #S.speech
        local ok, err = hold()
        assert(ok == true and err == nil, T.what(err))
        assert(S.menuTaken["Tab"] and S.menuTaken["Shift+Tab"], "taken in a menu")
        assert(#S.speech == said, "nothing said at the start")
        assert(T.count("[keys] 'Avenger' holds PageUp, Return, Escape for its module until its onKey ends it; Tab, "
          .. "Shift+Tab swallowed meanwhile") == 1, T.dump())
        local at = o.focus
        T.tab()
        assert(o.focus == at, "Tab moves nothing")
        assert(#S.speech == said + 1 and S.speech[#S.speech].text == SAY and S.speech[#S.speech].interrupt == true,
          "said: " .. tostring(S.speech[#S.speech] and S.speech[#S.speech].text))
        assert(T.typed("Shift+Tab") == "taken" and T.typed("Tab") == "taken" and o.focus == at, "taken by the hook")
        -- A menu flag lets the module's other keys through; the swallowed ones stay the hold's.
        T.host.keys.menuOpen(true)
        assert(T.typed("PageUp") == "passed" and T.typed("Tab") == "taken", "through, and taken")
        T.host.keys.menuOpen(false)
        -- The overlay captures its keys again while the hold stands: Tab stays the hold's.
        o:_suspendCapturedKeys(true)
        o:_suspendCapturedKeys(false)
        assert(S.menuTaken["Tab"], "the hold's capture again")
        T.tab()
        assert(o.focus == at and #S.speech == said + 1, "still swallowed, said once")
        assert(T.count("[keys] 'Avenger': Tab swallowed by its hold — it does nothing: said \"" .. SAY .. "\"") == 1, T.dump())
        assert(T.count("swallowed by its hold — it does nothing, said once already") == 4, T.dump())
        assert(#keys == 0 and #ends == 0, "nothing for onKey: " .. table.concat(keys, ","))
        -- Its end gives Tab back to the overlay.
        assert(T.typed("Return") == "taken" and table.concat(ends, ",") == "ended", table.concat(ends, ","))
        assert(table.concat(keys, ",") == "Return", table.concat(keys, ","))
        assert(not S.menuTaken["Tab"] and not S.menuTaken["Shift+Tab"], "the overlay's own again")
        T.tab()
        assert(o.focus ~= at, "Tab moves again")
        -- A second hold says it again, once; replaced, it gives Tab back.
        said = #S.speech
        hold()
        T.tab()
        T.tab()
        assert(#S.speech == said + 1 and S.speech[#S.speech].text == SAY, "once again")
        local other = {}
        o:holdKeys({ keys = { "PageDown" }, onKey = function() end, onEnd = function(_, how) other[#other + 1] = how end })
        assert(table.concat(ends, ",") == "ended,replaced", table.concat(ends, ","))
        at = o.focus
        T.tab()
        assert(o.focus ~= at and not S.menuTaken["Tab"], "Tab the overlay's after the replaced hold")
        -- Left: every key goes.
        hold()
        o:_deactivate()
        assert(table.concat(ends, ",") == "ended,replaced,left", table.concat(ends, ","))
        assert(T.typed("Tab") == "application" and T.typed("Shift+Tab") == "application", "released with the front")
        -- Mistakes in `swallow` raise at the module's line, before anything else is asked.
        for _, case in ipairs({
          { "Tab", "`swallow` takes { keys, say }, got string" },
          { { keys = { "Tab" } }, "`swallow.say` must be the sentence said at the first key swallowed, got nil" },
          { { keys = {}, say = "x" }, "`swallow.keys` must be a list of 1 to 8 key specs" },
          { { keys = { "Tab" }, say = "x", also = true }, "`swallow` has no field 'also' (it has keys and say)" },
          { { keys = { "Return" }, say = "x" }, "names Return in its keys and in `swallow`: a key is held or swallowed, not both" },
          { { keys = { "Tab", "Tab" }, say = "x" }, "swallows one key twice: Tab and Tab" },
        }) do
          local okR, errR = pcall(function()
            return o:holdKeys({ keys = { "Return" }, swallow = case[1], onKey = function() end, onEnd = function() end })
          end)
          assert(not okR and string.find(tostring(errR), case[2], 1, true), tostring(errR))
          assert(string.find(tostring(errR), '[string "scenario"]:', 1, true) == 1, "the module's line: " .. tostring(errR))
        end
        assert(not o._hold, "nothing held")
    "##);
}

/// C10, for the ring's knobs (the maintainer's decision of 2026-10-24): `O:holdKeys` takes keys for
/// the module while the overlay is in front, over every claim of the overlay's own — Return reaches
/// onKey and activates no control — and Tab still moves. The harness keeps captures as the host
/// does, one of a key for the module, the newer replacing the older and a release of a replaced one
/// doing nothing: the hold's capture of Return replaces the overlay's, and a Return the overlay
/// lets go of or captures again while the hold stands stays the hold's. onKey's `true` ends it:
/// onEnd "ended", and the overlay's own captures of the keys are taken again — Return activates the
/// control again, a held Tab moves the focus again. A hold whose key cannot be captured holds
/// nothing and leaves the overlay its own. Another hold replaces it ("replaced"), leaving the front
/// ends it ("left"), each told once; not in front it holds nothing.
#[test]
fn hold_keys_takes_the_keys_over_the_overlays_own_and_gives_them_back() {
    run_with_helpers("windows", &[], r##"
        local S = T.S
        local o = T.overlay(nil, "Avenger")
        o.focus = 2
        o:_syncNativeKeys()
        local acted = 0
        o.controls[2].onActivate = function() acted += 1 end
        T.typed("Return")
        assert(acted == 1, "Return activates the control: " .. acted)
        local keys, ends = {}, {}
        local ok, err = o:holdKeys({ keys = { "PageUp", "PageDown", "Return", "Escape" },
          onKey = function(ov, key)
            assert(ov == o)
            keys[#keys + 1] = key
            return key == "Escape"
          end,
          onEnd = function(ov, how, info) ends[#ends + 1] = how .. ":" .. tostring(info.key or info.why) end })
        assert(ok == true and err == nil, T.what(err))
        assert(T.typed("PageUp") == "taken" and T.typed("PageDown") == "taken", "the hold's own keys")
        T.tab()
        assert(o.focus ~= 2, "Tab still moves")
        -- The overlay lets its Return go, takes it again and lets it go again: the hold keeps it.
        for _, focus in ipairs({ 0, 2, 0, 2 }) do
          o.focus = focus
          o:_syncNativeKeys()
          assert(T.typed("Return") == "taken" and acted == 1, "Return is the hold's, no control activated, at " .. focus)
        end
        assert(table.concat(keys, ",") == "PageUp,PageDown,Return,Return,Return,Return", table.concat(keys, ","))
        assert(T.typed("Escape") == "taken" and #ends == 1 and ends[1] == "ended:Escape", table.concat(ends, ","))
        assert(T.typed("PageUp") == "application" and T.typed("Escape") == "application", "the keys back")
        T.typed("Return")
        assert(acted == 2, "Return activates the control again: " .. acted)
        -- A held Tab: the overlay's own, taken for as long as it is in front, comes back too.
        o:holdKeys({ keys = { "Tab" }, onKey = function() return true end, onEnd = function() end })
        local at = o.focus
        T.tab()
        assert(o.focus == at and not o._hold, "Tab was the hold's, and ended it")
        T.tab()
        assert(o.focus ~= at, "Tab moves again")
        -- A key the host cannot capture: nothing held, Return the overlay's.
        o.focus = 2
        o:_syncNativeKeys()
        S.captureRaises = { PageDown = "no event tap" }
        local ok3, err3 = o:holdKeys({ keys = { "Return", "PageDown" }, onKey = function() end, onEnd = function() end })
        S.captureRaises = nil
        assert(ok3 == false and err3 ~= nil and not o._hold, T.what(err3))
        T.typed("Return")
        assert(acted == 3, "Return activates the control after the refusal: " .. acted)
        -- Replaced, then left.
        local a, b = {}, {}
        o:holdKeys({ keys = { "PageUp", "Return" }, onKey = function() end, onEnd = function(_, how) a[#a + 1] = how end })
        o:holdKeys({ keys = { "PageDown" }, onKey = function() end, onEnd = function(_, how) b[#b + 1] = how end })
        assert(table.concat(a, ",") == "replaced" and #b == 0, table.concat(a, ","))
        assert(T.typed("PageUp") == "application" and T.typed("PageDown") == "taken")
        T.typed("Return")
        assert(acted == 4, "Return the overlay's after the replaced hold: " .. acted)
        o:_deactivate()
        assert(table.concat(b, ",") == "left" and table.concat(a, ",") == "replaced", table.concat(b, ","))
        assert(T.typed("PageDown") == "application" and T.typed("Return") == "application", "released with the front")
        -- Not in front.
        local ok2, err2 = o:holdKeys({ keys = { "PageUp" }, onKey = function() end, onEnd = function() end })
        assert(ok2 == false and T.is(err2, "not_in_front", "the overlay is not in front"), T.what(err2))
        -- Mistakes raise.
        for _, bad in ipairs({
          {}, { keys = {}, onKey = print, onEnd = print }, { keys = { "PageUp", "PageUp" }, onKey = print, onEnd = print },
          { keys = { "PageUp" }, onEnd = print }, { keys = { "PageUp" }, onKey = print },
          { keys = { "PageUp" }, onKey = print, onEnd = print, ends = print },
        }) do
          assert(not pcall(o.holdKeys, o, bad), "raises")
        end
    "##);
}
