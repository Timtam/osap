//! VPS Avenger's page, its own Settings section (the plan's slice 10:
//! `modules/vps-avenger/src/page.luau`, `ui/`), loaded for real against the scripted host of
//! `overlay_avenger_tests.rs`: the avenger_control tool's Windows-only Settings block made the page's
//! section on both systems, writing the module's five settings — the ones its settings dialog shows —
//! with `set_setting` and `reset_settings`, and told them back with a `settings` message.
//!
//! What the plan says is pinned here. The page's `ready` gets the state, then the settings; every
//! change — the dialog's or the page's — ends with the settings posted, so the dialog and the page
//! always show the same values: the store's, the key registered, the page's `activates` and the
//! watcher's pace (`T.agree`). A setting from the page is stored with no trip, and answered once,
//! with what it refused in the error region and nothing said; a value equal to the stored one is not
//! set. The page key is checked as the dialog's is, and refused also where it has no modifier (the
//! tool's rule); the key in effect stays registered. The Mac page mode changed from the page closes
//! the page it was sent from, with Close.wav and a sentence, and the new page is told nothing before
//! its own `ready`. Reset sets every setting to its default, the Mac mode last. Values stored outside
//! their schema are set back at load. A message with no setting, another module's, or a value of
//! another kind changes nothing.

use super::overlay_avenger_tests::{run, run_mac};

/// The page key on a Mac: the tool's chord (`Ctrl` is Command and `Win` is Control in a key spec).
const MAC_KEY: &str = "Ctrl+Shift+Win+F1";

/// The helpers: whether the dialog and the page agree, the page's own messages, the way back.
const SET: &str = r##"
local S = T.S
T.KEYS = { "roster", "pageKey", "sounds", "pageLeavesDawActive", "drumSqCadence" }

-- The dialog and the page agree: the last settings message posted to the page in use holds the
-- store's five values; the store's key is the one page key registered; the page object's
-- `activates` is the Mac mode's opposite; the watcher's timer runs at the store's pace.
function T.agree(what)
  local m = T.lastPost("settings")
  T.ok(m ~= nil, what .. ": no settings posted")
  for _, k in ipairs(T.KEYS) do
    T.ok(m.values[k] == S.settings[k], string.format("%s: %s posted %s, stored %s", what, k, tostring(m.values[k]),
      tostring(S.settings[k])))
  end
  local keys = {}
  for spec in pairs(S.pageKeys) do keys[#keys + 1] = spec end
  T.ok(#keys == 1 and keys[1] == S.settings.pageKey, what .. ": the keys registered " .. table.concat(keys, ", "))
  T.ok(T.page().opts.activates == not S.settings.pageLeavesDawActive, what .. ": activates "
    .. tostring(T.page().opts.activates))
  T.ok(S.polls[S.settings.drumSqCadence] ~= nil, what .. ": no watcher every " .. tostring(S.settings.drumSqCadence) .. " ms")
end

-- The page, with the keyboard, sets `key` to `value`, or resets (`key` nil: every setting).
function T.set(key, value)
  T.pageMsg({ action = "set_setting", args = { key = key, value = value }, focused = true })
end
function T.reset(key)
  T.pageMsg({ action = "reset_settings", args = { key = key }, focused = true })
end

-- The page's messages from post `from` on, of kind `kind` (nil: every kind).
function T.postsSince(from, kind)
  local out = {}
  local posts = T.page().posts
  for i = from + 1, #posts do
    if kind == nil or posts[i].kind == kind then out[#out + 1] = posts[i] end
  end
  return out
end

-- Avenger at 80 % with the page open on it, loaded, in front.
function T.pageUp()
  local header, box, W = T.avengerAt80("Init Preset")
  T.pageReady()
  T.openPage()
  T.ok(S.front == T.PAGE_WIN, "the page in front")
  return header, box, W
end

-- Back to Avenger with the page key, Avenger in front again.
function T.back(W, spec)
  S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
  T.pageKey(spec)
  T.event()
end
"##;

fn scenario(s: &str) -> String {
    format!("{SET}\n{}", s.replace("@KEY@", MAC_KEY))
}

/// The page's own `ready` gets the state, then one settings message: the five defaults, the
/// expansions lists to choose from, the key in effect and the default key in words — small. The
/// host's onReady of a reload posts the state only (the reloaded document sends its own `ready`),
/// and that `ready` gets the settings again.
#[test]
fn ready_gets_the_state_then_the_settings() {
    run_mac(&scenario(r##"
        T.avengerAt80("Init Preset")
        local n = #T.page().posts
        T.pageMsg({ action = "ready" })
        local got = T.postsSince(n)
        T.ok(#got == 2 and got[1].kind == "state" and got[2].kind == "settings", "the state, then the settings: " .. #got)
        local m = got[2]
        T.ok(m.values.roster == "Factory Standard" and m.values.pageKey == "@KEY@" and m.values.sounds == true
          and m.values.pageLeavesDawActive == false and m.values.drumSqCadence == 250, "the five defaults")
        T.ok(table.concat(m.rosters, "|") == "Factory Standard|Complete", "the lists to choose from")
        T.ok(m.keyWords == "@KEY@" and m.defaultKeyWords == "@KEY@", "the keys in words")
        T.ok(#T.jsonEncode(m) < 1024, "small: " .. #T.jsonEncode(m))
        T.agree("ready")
        T.pageReady()
        T.nameIs("LD Lead 1")
        T.openPage()
        S.now += 5000
        n = #T.page().posts
        T.pageReady()
        got = T.postsSince(n)
        T.ok(#got == 1 and got[1].kind == "state", "the reload's onReady: the state only")
        n = #T.page().posts
        T.pageMsg({ action = "ready" })
        got = T.postsSince(n)
        T.ok(#got == 2 and got[1].kind == "state" and got[2].kind == "settings", "the reload's ready: both")
        T.agree("the reload")
    "##));
}

/// Sounds off from the page: stored at once, with no trip — nothing asked to the front — and
/// answered with one settings message and the error region cleared, the focus not moved and nothing
/// said; the page's next show and way back play nothing. The same message while the page does not
/// have the keyboard is not acted on.
#[test]
fn a_setting_from_the_page_is_stored_and_answered_with_the_settings() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageUp()
        local sets, said, from = S.settingSets, #S.speech, #S.pageOrder
        T.set("sounds", false)
        T.ok(S.settings.sounds == false and S.settingSets == sets + 1, "stored, once")
        T.ok(T.pageSince(from) == "post settings,post error", T.pageSince(from))
        T.ok(T.lastPost("error").text == "" and T.lastPost("error").focus == nil, "the error region cleared, the focus "
          .. "left where it is: " .. T.lastPost("error").text)
        T.ok(#S.speech == said, "nothing said: " .. T.said(said + 1))
        T.ok(T.count("[avenger] [page] setting sounds from the page: true -> false") == 1, T.dump())
        T.agree("sounds off")
        local sounds = #S.sounds
        T.back(W)
        T.openPage()
        T.ok(#S.sounds == sounds, "no Close.wav, no Open.wav: " .. tostring(S.sounds[#S.sounds]))
        -- Not in front: not acted on.
        S.front, S.chain = W, { W }
        sets = S.settingSets
        T.set("sounds", true)
        T.ok(S.settings.sounds == false and S.settingSets == sets, "not acted on")
        T.ok(T.count("[avenger] [page] 'set_setting' not acted on: the page does not have the keyboard") == 1, T.dump())
    "##));
}

/// Whatever the settings dialog changes is posted to the page, and the two agree after each; the
/// Mac mode from the dialog describes the page again, which is told nothing before its own `ready`.
/// The dialog's OK with nothing changed — every one of the five set to its value — is answered by
/// five equal settings messages, says nothing and describes nothing.
#[test]
fn the_dialog_and_the_page_always_agree() {
    run_mac(&scenario(r##"
        T.presets()
        T.pageUp()
        for _, c in ipairs({ { "roster", "Complete" }, { "pageKey", "Ctrl+Shift+F2" }, { "sounds", false },
          { "drumSqCadence", 400 } }) do
          local n = #T.page().posts
          T.setting(c[1], c[2])
          T.ok(#T.postsSince(n, "settings") >= 1, c[1] .. ": no settings posted")
          T.agree("the dialog's " .. c[1])
        end
        T.setting("pageLeavesDawActive", true)
        T.ok(#S.pages == 2 and #T.page().posts == 0, "described again, and nothing posted to the new page")
        T.pageMsg({ action = "ready" })
        T.agree("the dialog's Mac mode")
        local said, pages, n = #S.speech, #S.pages, #T.page().posts
        for _, k in ipairs(T.KEYS) do T.setting(k, S.settings[k]) end
        local got = T.postsSince(n, "settings")
        T.ok(#got == 5, "five settings messages: " .. #got)
        for _, m in ipairs(got) do
          for _, k in ipairs(T.KEYS) do T.ok(m.values[k] == S.settings[k], "equal: " .. k) end
        end
        T.ok(#S.speech == said and #S.pages == pages, "nothing said, nothing described: " .. T.said(said + 1))
        T.agree("the dialog's OK")
    "##));
}

/// A page key from the page: registered, the old one let go, said as the dialog's is, and the key's
/// words posted; the key away from Avenger names it. One the platform cannot hold, one with no
/// modifier (the tool's rule), and a tap are refused in the error region with the reason in words —
/// nothing said, the focus not moved — and the key in effect stays registered and stored, as for the
/// runtime's own refusals, a control's key and a registration that raised; the dialog's key with no
/// modifier is refused too, and said. Nothing written is never set. Reset to Default registers the
/// default, said; the key in effect written again, in any spelling, is "unchanged".
#[test]
fn a_page_key_from_the_page_is_registered_and_one_refused_keeps_the_old() {
    run_mac(&scenario(r##"
        local header, _, W = T.pageUp()
        T.set("pageKey", "Ctrl+Shift+F2")
        T.ok(S.settings.pageKey == "Ctrl+Shift+F2" and S.pageKeys["Ctrl+Shift+F2"] ~= nil and S.pageKeys["@KEY@"] == nil,
          "the new key, the old one let go")
        T.ok(T.lastSaid() == "Page key changed to Ctrl+Shift+F2.", T.said())
        T.ok(T.lastPost("settings").keyWords == "Ctrl+Shift+F2" and T.lastPost("error").text == "", "its words posted")
        T.agree("the new key")
        local function refused(v, why)
          local said, sets = #S.speech, S.settingSets
          T.set("pageKey", v)
          T.ok(S.settings.pageKey == "Ctrl+Shift+F2" and S.pageKeys["Ctrl+Shift+F2"] ~= nil and S.pageKeys[v] == nil,
            v .. ": refused, the key in effect kept")
          T.ok(T.lastPost().kind == "error" and T.lastPost().text == "Page Key: " .. v .. " cannot be the page key (" .. why
            .. "); it stays Ctrl+Shift+F2", T.lastPost().text)
          T.ok(T.lastPost().focus == nil, v .. ": the focus left where it is")
          T.ok(#S.speech == said, v .. ": nothing said: " .. T.said(said + 1))
          T.ok(S.settingSets > sets, v .. ": set, and set back")
          T.agree(v)
        end
        S.badKeys["Win+Alt+Q"] = { "voiceover" }
        refused("Win+Alt+Q", "Control with Option is the screen reader's")
        refused("F2", "it needs at least one modifier key")
        S.badKeys["Alt tap"] = { "tap" }
        refused("Alt tap", "a modifier alone is no key")
        -- The runtime's own refusals (O:pageKey): a control of the ring's key (`clash`), a key the
        -- system keeps (register's nil and its error value), and a claim another program holds now
        -- (register's id and its error value: the runtime lets the new claim go, the old key stays).
        local hk = header.controls[1].hotkey
        header.controls[1].hotkey = "Ctrl+Shift+F7"
        refused("Ctrl+Shift+F7", "a control of the ring has it")
        header.controls[1].hotkey = hk
        S.registerRefuses["Ctrl+Shift+F8"] = T.E("taken", "no hotkey can be registered now")
        refused("Ctrl+Shift+F8", "the system refused it: no hotkey can be registered now")
        S.registerNotHeld["Ctrl+Shift+F9"] = T.E("taken", "RegisterHotKey failed for 'Ctrl+Shift+F9' (already in use by another app?)")
        refused("Ctrl+Shift+F9", "the system refused it: RegisterHotKey failed for 'Ctrl+Shift+F9' (already in use by another app?)")
        -- The dialog's key with no modifier: refused too, and said.
        T.setting("pageKey", "F3")
        T.ok(S.settings.pageKey == "Ctrl+Shift+F2" and T.lastSaid() == "F3 cannot be the page key; it stays Ctrl+Shift+F2.",
          T.said())
        T.agree("the dialog's F3")
        local sets = S.settingSets
        T.set("pageKey", "   ")
        T.ok(T.lastPost("error").text == "Page Key: no key was written" and S.settingSets == sets, T.lastPost("error").text)
        -- Reset to Default.
        T.reset("pageKey")
        T.ok(S.settings.pageKey == "@KEY@" and S.pageKeys["@KEY@"] ~= nil and S.pageKeys["Ctrl+Shift+F2"] == nil, "the default")
        T.ok(T.lastSaid() == "Page key changed to @KEY@.", T.said())
        T.agree("Reset to Default")
        sets = S.settingSets
        T.set("pageKey", " @KEY@ ")
        T.ok(T.lastPost("error").text == "Page Key: unchanged, @KEY@" and S.settingSets == sets, T.lastPost("error").text)
        -- The same key in another spelling: the same key (host.keys.normalize), unchanged, and the
        -- spelling stored kept.
        T.set("pageKey", "win+shift+CTRL+f1")
        T.ok(T.lastPost("error").text == "Page Key: unchanged, @KEY@" and S.settingSets == sets
          and S.settings.pageKey == "@KEY@", T.lastPost("error").text)
        -- The new key away from Avenger: the tool's sentence names it.
        T.set("pageKey", "Ctrl+Shift+F2")
        S.listed = { W, T.OTHER }
        T.show(T.OTHER)
        T.pageKey("Ctrl+Shift+F2")
        T.ok(T.lastSaid() == "It seems that the VPS Avenger plug-in window is not at the front. Please open the plug-in window, "
          .. "then press Ctrl+Shift+F2 again.", T.said())
    "##));
    // On Windows the same answers, back to the tool's Windows chord.
    run(&scenario(r##"
        T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.run(400)
        T.ok(header.active, "identified")
        T.pageReady()
        T.openPage()
        T.set("pageKey", "Ctrl+Shift+F2")
        T.ok(S.pageKeys["Ctrl+Shift+F2"] ~= nil and T.lastSaid() == "Page key changed to Ctrl+Shift+F2.", T.said())
        T.reset("pageKey")
        T.ok(S.settings.pageKey == "Ctrl+Shift+Alt+F1" and S.pageKeys["Ctrl+Shift+Alt+F1"] ~= nil
          and T.lastSaid() == "Page key changed to Ctrl+Shift+Alt+F1.", T.said())
        T.agree("Windows")
    "##));
}

/// The Mac page mode from the page closes the page it was sent from and describes it again —
/// `activates` the mode's opposite — with Close.wav and a sentence that says the setting and how to
/// open the page, and nothing of what the mode does; posts nothing to the new page before its own
/// `ready`; the key then builds it, saying so, and shows it once it has loaded, its `ready` bringing
/// the state and the settings. And back the same way. From the dialog: no sound and nothing said.
/// On Windows the page is described again as well, with the same words.
#[test]
fn the_mac_page_mode_from_the_page_describes_the_page_again_and_says_so() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageUp()
        local function change(to, said)
          local old, sounds, pages = T.page(), #S.sounds, #S.pages
          local posts = #old.posts
          T.set("pageLeavesDawActive", to)
          T.ok(#S.pages == pages + 1 and old.closed and T.page().opts.activates == not to, "described again: " .. #S.pages)
          T.ok(#S.sounds == sounds + 1 and S.sounds[#S.sounds] == "sounds/Close.wav", tostring(S.sounds[#S.sounds]))
          T.ok(T.lastSaid() == said and S.speech[#S.speech].interrupt == true, T.said())
          T.ok(#T.page().posts == 0 and #old.posts == posts, "nothing posted to either page")
          -- The page's close gave the front back to Avenger, as the host's does; the new page is built
          -- by the key, and shown once it has loaded.
          S.pageLoaded = false
          S.front, S.chain = W, { W }
          T.event()
          T.openPage()
          T.ok(T.lastSaid() == "Building the page." and #T.page().shows == 1, T.said())
          T.pageReady()
          T.ok(#T.page().shows == 2 and S.front == T.PAGE_WIN, "shown once it has loaded")
          local n = #T.page().posts
          T.pageMsg({ action = "ready" })
          local got = T.postsSince(n)
          T.ok(got[#got - 1].kind == "state" and got[#got].kind == "settings", "its ready: the state and the settings")
          T.agree(tostring(to))
        end
        change(true, "Leave the DAW Active is on now. Press @KEY@ to open the page again.")
        change(false, "Leave the DAW Active is off now. Press @KEY@ to open the page again.")
        local sounds, said = #S.sounds, #S.speech
        T.setting("pageLeavesDawActive", true)
        T.ok(#S.pages == 4 and #S.sounds == sounds and #S.speech == said, "the dialog's: no sound, nothing said")
    "##));
    run(&scenario(r##"
        T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.run(400)
        T.pageReady()
        T.openPage()
        T.set("pageLeavesDawActive", true)
        T.ok(#S.pages == 2 and T.page().opts.activates == false, "described again on Windows too")
        -- The same words as on a Mac, none about what the mode does: on Windows it changes nothing.
        T.ok(T.lastSaid() == "Leave the DAW Active is on now. Press Ctrl+Shift+Alt+F1 to open the page again.", T.said())
    "##));
}

/// The expansions list from the page: the state's list follows, then the settings, and nothing in
/// the error region. Where Avenger's own list was read this session, the setting is stored and the
/// list shown stays Avenger's, which a note says. A list that is not one of the two is refused.
#[test]
fn the_roster_from_the_page_posts_its_list_and_says_where_avengers_own_is_held() {
    run_mac(&scenario(r##"
        T.presets()
        T.pageUp()
        local from = #S.pageOrder
        T.set("roster", "Complete")
        T.ok(T.pageSince(from) == "post state,post settings,post error", T.pageSince(from))
        T.ok(table.concat(T.lastPost("state").state.expansions, "|") == "Alpha|Beta|Gamma|Delta", "the roster's four")
        T.ok(T.lastPost("settings").values.roster == "Complete" and T.lastPost("error").text == "", T.lastPost("error").text)
        T.agree("Complete")
        local St = S.included["modules/vps-avenger/src/state.luau"][1]
        St.setAvengerList({ "Gamma", "Alpha" })
        T.set("roster", "Factory Standard")
        T.ok(S.settings.roster == "Factory Standard", "stored")
        T.ok(table.concat(T.lastPost("state").state.expansions, "|") == "Gamma|Alpha", "Avenger's list shown still")
        T.ok(T.lastPost("error").text == "Expansions List: Factory Standard is kept; the list shown stays VPS Avenger's own "
          .. "for this session", T.lastPost("error").text)
        T.agree("Factory Standard")
        local sets = S.settingSets
        T.set("roster", "Unknown List")
        T.ok(S.settingSets == sets and S.settings.roster == "Factory Standard"
          and T.lastPost("error").text == "Expansions List: Unknown List is not one of the expansions lists", T.lastPost("error").text)
        T.ok(St.avengerList ~= nil, "Avenger's list kept")
    "##));
}

/// DRUM SQ's pace from the page: the watcher armed again at it, as from the dialog. A pace out of
/// its bounds, one with a fraction, and no number are refused in the error region, the focus not
/// moved, never set, and the settings posted with the pace in effect, so the field shows it again.
#[test]
fn the_drum_sq_cadence_from_the_page_rearms_the_watcher_and_a_bad_one_is_refused() {
    run_mac(&scenario(r##"
        T.pageUp()
        T.set("drumSqCadence", 400)
        T.ok(S.settings.drumSqCadence == 400 and S.polls[400] ~= nil and S.polls[250] == nil, "every 400 ms")
        T.ok(T.count("[avenger] [watch] every 400 ms") == 1, T.dump())
        T.agree("400")
        for _, v in ipairs({ 1500, 250.5, "abc", 50 }) do
          local sets, n = S.settingSets, #T.page().posts
          T.set("drumSqCadence", v)
          T.ok(S.settingSets == sets and S.settings.drumSqCadence == 400 and S.polls[400] ~= nil, tostring(v) .. ": not set")
          T.ok(T.lastPost("error").text == "DRUM SQ Auto Follow: " .. tostring(v)
            .. " is no whole number of milliseconds from 100 to 1000", T.lastPost("error").text)
          T.ok(T.lastPost("error").focus == nil, tostring(v) .. ": the focus left where it is")
          local m = T.postsSince(n, "settings")
          T.ok(#m == 1 and m[1].values.drumSqCadence == 400, tostring(v) .. ": the pace in effect posted")
        end
        T.agree("after the refusals")
    "##));
}

/// Reset Saved Settings: every setting at its default — the default key registered — the Mac mode
/// last, so the page is described again once, after everything else, and posted nothing to; Close.wav
/// and the spoken sentence; the new page's `ready` answers the defaults. Avenger's own list is left as
/// it is (the tool's reset left what it held in memory). With the Mac mode already off: the note, and
/// nothing described. With the default key refused: the note says which key stays, and why.
#[test]
fn reset_sets_every_setting_to_its_default_the_mac_mode_last() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageUp()
        local St = S.included["modules/vps-avenger/src/state.luau"][1]
        St.setAvengerList({ "Gamma", "Alpha" })
        T.setting("roster", "Complete")
        T.setting("pageKey", "Ctrl+Shift+F2")
        T.setting("sounds", false)
        T.setting("drumSqCadence", 400)
        T.setting("pageLeavesDawActive", true)
        S.pageLoaded = false
        S.front, S.chain = W, { W }
        T.event()
        T.openPage("Ctrl+Shift+F2")
        T.pageReady()
        T.ok(S.front == T.PAGE_WIN and #S.pages == 2, "the new page in front")
        local old, sounds = T.page(), #S.sounds
        local logs = #S.logs
        T.reset()
        for _, k in ipairs(T.KEYS) do
          T.ok(S.settings[k] == S.settingDefs[k].default, k .. " at its default: " .. tostring(S.settings[k]))
        end
        T.ok(S.pageKeys["@KEY@"] ~= nil and S.pageKeys["Ctrl+Shift+F2"] == nil and S.polls[250] ~= nil, "the key, the watcher")
        T.ok(#S.pages == 3 and old.closed and T.page().opts.activates == true and #T.page().posts == 0,
          "described again once, and nothing posted to the new page")
        T.ok(#S.sounds == sounds + 1 and S.sounds[#S.sounds] == "sounds/Close.wav", tostring(S.sounds[#S.sounds]))
        T.ok(T.lastSaid() == "Every setting is back at its default. Leave the DAW Active is off now. Press @KEY@ to open the "
          .. "page again.", T.said())
        local order = {}
        for i = logs + 1, #S.logs do
          local k = string.match(S.logs[i], "%[avenger%] %[page%] reset: (%a+) ")
          if k then order[#order + 1] = k end
        end
        T.ok(table.concat(order, ",") == "roster,sounds,drumSqCadence,pageKey,pageLeavesDawActive", table.concat(order, ","))
        T.ok(St.avengerList ~= nil, "Avenger's list left as it is")
        T.pageMsg({ action = "ready" })
        T.agree("after the reset")
        -- Again, the Mac mode off already: the note, nothing described.
        S.pageLoaded = false
        S.front, S.chain = W, { W }
        T.event()
        T.openPage()
        T.pageReady()
        T.setting("sounds", false)
        local pages = #S.pages
        T.reset()
        T.ok(#S.pages == pages and S.settings.sounds == true, "nothing described")
        T.ok(T.lastPost("error").text == "Reset Saved Settings: every setting is back at its default", T.lastPost("error").text)
        T.agree("the second reset")
        -- The default key refused: it says which key stays.
        T.setting("pageKey", "Ctrl+Shift+F2")
        S.badKeys["@KEY@"] = { "reserved" }
        T.reset()
        T.ok(S.settings.pageKey == "Ctrl+Shift+F2" and S.pageKeys["Ctrl+Shift+F2"] ~= nil, "the key in effect stays")
        T.ok(T.lastPost("error").text == "Reset Saved Settings: every setting is back at its default but the page key, which "
          .. "stays Ctrl+Shift+F2 (the system keeps it)", T.lastPost("error").text)
        T.agree("the third reset")
    "##));
}

/// A stored value outside its setting's schema — a list the module does not have, a pace out of its
/// bounds or with a fraction, which `define` hands back as stored — is set back to its default at
/// load and logged, so the dialog, the page and the watcher show the same value.
#[test]
fn values_stored_outside_their_schema_are_set_back_at_load() {
    run_mac(&scenario(r##"
        S.settings.roster, S.settings.drumSqCadence = "Unknown List", 5000
        T.pageUp()
        T.ok(S.settings.roster == "Factory Standard" and S.settings.drumSqCadence == 250, "the defaults stored")
        T.ok(T.count("[avenger] [page] the stored roster setting: Unknown List is not one of the expansions lists — "
          .. "Factory Standard instead") == 1, T.dump())
        T.ok(T.count("[avenger] [page] the stored drumSqCadence setting: 5000 is no whole number of milliseconds from 100 to "
          .. "1000 — 250 instead") == 1, T.dump())
        T.pageMsg({ action = "ready" })
        T.agree("at load")
    "##));
    run_mac(&scenario(r##"
        S.settings.drumSqCadence = 250.5
        T.pageUp()
        T.ok(S.settings.drumSqCadence == 250 and S.polls[250] ~= nil, "250")
        T.ok(T.count("the stored drumSqCadence setting: 250.5 is no whole number") == 1, T.dump())
        T.pageMsg({ action = "ready" })
        T.agree("at load")
    "##));
}

/// A setting message that names no setting, another module's (the runtime's calibration switch),
/// or a value of another kind is refused with its sentence, and nothing is set.
#[test]
fn a_setting_message_with_no_or_another_key_or_kind_changes_nothing() {
    run_mac(&scenario(r##"
        T.pageUp()
        local sets = S.settingSets
        local before = {}
        for _, k in ipairs(T.KEYS) do before[k] = S.settings[k] end
        for _, c in ipairs({
          { args = {}, text = "Settings: no setting was named" },
          { args = { key = "calibrate", value = true }, text = "Settings: calibrate is no setting of this page" },
          { args = { key = "sounds", value = "yes" }, text = "Sounds: on or off was not sent" },
          { args = { key = "pageLeavesDawActive", value = 1 }, text = "Leave the DAW Active: on or off was not sent" },
          { args = { key = "roster", value = 3 }, text = "Expansions List: 3 is not one of the expansions lists" },
          { args = { key = "pageKey", value = 5 }, text = "Page Key: no key was written" },
          { args = { key = "drumSqCadence" }, text = "DRUM SQ Auto Follow: no number of milliseconds was sent" },
        }) do
          T.pageMsg({ action = "set_setting", args = c.args, focused = true })
          T.ok(T.lastPost().kind == "error" and T.lastPost().text == c.text, T.lastPost().text)
        end
        T.reset("calibrate")
        T.ok(T.lastPost().text == "Reset Saved Settings: calibrate is no setting of this page", T.lastPost().text)
        T.ok(S.settingSets == sets, "nothing set: " .. (S.settingSets - sets))
        for _, k in ipairs(T.KEYS) do T.ok(S.settings[k] == before[k], k .. " unchanged") end
        T.agree("after the refusals")
    "##));
}
