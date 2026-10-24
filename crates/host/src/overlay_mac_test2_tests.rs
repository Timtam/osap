//! The second Mac test of VPS Avenger's page (2026-10-24), its findings in the overlay runtime and
//! daw-hosts, run for real against the scripted host of `overlay_host_panel_tests.rs` with the
//! window prelude's real matcher, and daw-hosts loaded from its file.
//!
//! Logic's "Load Preset..." panel is not a plug-in window (finding 5): every embedded overlay had
//! asked its identity question in it. And the focus key's own click into the plug-in's panel, after
//! which Logic reports no focus at all, begins a stay there (finding 9, the maintainer's decision of
//! 2026-10-24); any other empty focus chain still begins none.

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
local inside = { name = "inside", cheap = true,
  test = function(_, answer) answer(S.inside and { x = 300, y = 200, w = 100, h = 80 } or false) end }
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
