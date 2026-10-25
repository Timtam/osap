//! A trip's hand-over (RG2 of the VPS Avenger port's plan): the overlay runtime's
//! `trip:handOver{ kind, keys?, onKey?, ends?, onEnd }`, run for real against the scripted host of
//! `overlay_menu_tests.rs` with the page helpers of `overlay_page_tests.rs` — the window prelude's
//! real matcher, daw-hosts loaded for real, the scripted page and the specificity arbiter — and the
//! hook's decision for a typed key (`T.typed`): through to the application and recorded while the
//! module's flag counts for the window in front, taken by a capture otherwise.
//!
//! What the plan of 2026-10-17 says is pinned here. `typing` and `browse` hold their end keys with
//! the module's flag set and its scope pinned to the plug-in's window, so the keys go through to the
//! plug-in and are recorded; the menu tick drains the record, and the first end key is the event —
//! its end asked of the module's `ends` on the next turn, with the plug-in in front and the keys
//! still held, and an end key that comes meanwhile kept for a "no". `keys` captures its keys for the
//! module's `onKey` and writes no flag. Every end — its own, the page key, another application, the
//! page closed (a disable among them), the window gone, outranked, a hook that raised, `done` —
//! gives the keys back at once, what the record still holds dropped and the scope left pinned for
//! an overlay of the module still in front, and tells `onEnd` exactly once; a window of the
//! plug-in's own application, the page itself and time end nothing, and the runtime's other writes
//! of the flag and the scope wait for the hand-over. A key the hook took after all — the flag did
//! not count — is no event: pinned again and said.
//!
//! **Every event through the real mailbox**, as the harness has it, and every scenario ends on the
//! harness's checks (`finish`) and on the hooks of every overlay paired (`T.pairsAll`).

use super::overlay_host_panel_tests::{run_with_helpers, SPECIFICITY_ARBITER};
use super::overlay_page_tests::PG;
use super::page_example_tests::PAGE_WEBVIEW;

/// The hand-over's helpers, on top of the page helpers.
const HO: &str = r##"
local S = T.S
local H = T.host
-- A spec the host cannot read for a key: host.keys.normalize answers nil for it.
do
  local normalize = H.keys.normalize
  rawset(H.keys, "normalize", function(spec)
    if spec == "Nope+Q" then return nil end
    return normalize(spec)
  end)
end

-- The hand-over's spec, its hooks recorded: `over` replaces fields (false removes one). `ends`
-- answers S.endsFn's way (default: over), `onEnd` S.onEndFn's (default: done, with a name, on its
-- own end). S.endsAsked: each `ends`' info and whether the plug-in was in front; S.ended: each
-- `onEnd`'s how, info and error value, `as` its how or, given up, the error's code, whether the trip
-- was still live, and whether a key of the hand-over's was still held then.
S.endsAsked, S.ended = {}, {}
function T.spec(over)
  local spec = {
    kind = "typing",
    ends = function(o, trip, info, answer)
      local f = H.window.foreground()
      S.endsAsked[#S.endsAsked + 1] = { info = info, inW = f ~= nil and f.id == T.W.id, overlay = o }
      S.order[#S.order + 1] = "ends"
      if S.endsFn then return S.endsFn(o, trip, info, answer) end
      answer(true)
    end,
    onEnd = function(o, trip, how, info, err)
      T.ok((how == "given_up") == (err ~= nil) and (how == "ended" or how == "done" or how == "given_up"),
        "onEnd's how is ended, done or given_up, with an error value given up only: " .. tostring(how) .. " " .. T.what(err))
      local as = err and err.code or how
      S.ended[#S.ended + 1] = { how = how, err = err, as = as, info = info, live = trip.live, held = T.hoHeld() }
      S.order[#S.order + 1] = "onEnd " .. as
      if S.onEndFn then return S.onEndFn(o, trip, how, info, err) end
      if how == "ended" then trip:done({ name = "Pad" }) end
    end,
  }
  for k, v in pairs(over or {}) do
    if v == false then spec[k] = nil else spec[k] = v end
  end
  return spec
end

-- A trip whose act hands the keyboard over with T.spec(over), arrived: what handOver answered. The
-- hand-over's tokens are kept (S.hoTokens), so that its keys can be told from the overlay's own.
function T.handOver(over)
  S.handed = nil
  S.actFn = function(o, trip)
    S.handed = table.pack(trip:handOver(T.spec(over)))
    S.hoTokens = {}
    for spec, id in pairs(S.holding) do S.hoTokens[spec] = id end
  end
  T.trip()
  T.land()
  T.runDue()
  T.ok(S.handed ~= nil, "the act ran")
  return S.handed[1], S.handed[2]
end

-- Whether a key the hand-over captured is still held.
function T.hoHeld()
  for _, id in pairs(S.hoTokens or {}) do
    if S.tokens[id] then return true end
  end
  return false
end

-- A key typed, then the tick that drains the record, then the module's next turn: what T.typed said.
function T.typedAndTicked(key)
  local how = T.typed(key)
  T.tick()
  T.runDue()
  return how
end

-- The hand-over's keys back, and nothing of it holding the module's flag or scope: with the page in
-- front or another application's window, the group holds no key either.
function T.allBack(what)
  T.ok(not T.hoHeld(), (what or "") .. ": a key of the hand-over's is still held")
  T.ok(T.open() ~= true, (what or "") .. ": the menu flag is still true")
  T.ok(not S.scopedTo, (what or "") .. ": the scope is still pinned to " .. tostring(S.scopedTo))
end
"##;

/// A fresh VM with the page helpers and the ones above; the host plays `os`. The scenario ends on
/// the harness's checks and on every overlay's hooks paired.
fn run_on(os: &str, scenario: &str) {
    let scenario = format!("{scenario}\nT.pairsAll()\n");
    run_with_helpers(
        os,
        &[
            ("the scripted page", PAGE_WEBVIEW),
            ("specificity arbiter", SPECIFICITY_ARBITER),
            ("page helpers", PG),
            ("hand-over helpers", HO),
        ],
        &scenario,
    )
}

fn run(scenario: &str) {
    run_on("windows", scenario)
}

fn run_mac(scenario: &str) {
    run_on("macos", scenario)
}

fn both(scenario: &str) {
    run(scenario);
    run_mac(scenario);
}

// ---------------------------------------------------------------------------------------------
// What it holds, and its own end.
// ---------------------------------------------------------------------------------------------

/// `typing` holds Return and Escape, the module's flag set and its scope pinned to the plug-in's
/// window; the group holds nothing else, nothing is said and the page is not shown. The page key
/// gives it all back.
#[test]
fn typing_holds_return_and_escape_for_the_plugin_with_the_flag_and_the_scope_on_its_window() {
    both(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        local said, shows = #S.speech, #S.pg.shows
        local ok, why = T.handOver()
        T.ok(ok == true and why == nil, tostring(why))
        T.ok(S.holding["Return"] and S.holding["Escape"], "Return and Escape held")
        T.ok(T.open() == true and S.scopedTo == T.W.id, "the flag, and the scope on " .. T.W.id .. ": " .. tostring(S.scopedTo))
        T.ok(S.holding["Tab"] == nil and next(S.hot) ~= nil and T.hotkeys() == 1, "nothing else: the page key alone")
        T.ok(#S.speech == said and #S.pg.shows == shows, "nothing said, nothing shown")
        T.ok(S.trip.stage == "waiting" and ov.active and ov.away, "the trip waits, the group quiet")
        T.ok(T.count("[page] 'Synth': trip 'Next preset' hands the keyboard over (typing): Return, Escape let through "
          .. "to window id " .. T.W.id .. ", the module's scope pinned there; it ends on one of them, the page key or "
          .. "another application — nothing bounds it") == 1, T.dump())
        T.key()
        T.ok(S.ended[1].as == "cancelled", tostring(S.ended[1] and S.ended[1].as))
        T.pageFront()
        T.allBack("after the page key")
        T.ok(T.onlyPageKey(), "the page key alone")
    "##);
}

/// A Return let through ends it: the tick sees it in the record, `ends` runs on the module's next
/// turn with the plug-in in front, and `onEnd(o, t, "ended", { key = "Return", passed = { "Return" } })`
/// runs once, the keys back by then; its `done` leads to the back step and the page.
#[test]
fn a_return_let_through_ends_it_on_the_next_turn_and_on_end_is_told_once_with_the_record() {
    both(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver()
        T.ok(T.typed("Return") == "passed", "through to the plug-in")
        T.ok(#S.endsAsked == 0, "nothing before the tick")
        T.tick()
        T.ok(#S.endsAsked == 0 and T.count("Return went through to the plug-in — the hand-over's end is looked at") == 1,
          "ends waits for the module's next turn: " .. T.dump())
        T.runDue()
        T.ok(#S.endsAsked == 1 and S.endsAsked[1].inW and S.endsAsked[1].info.key == "Return"
          and S.endsAsked[1].info.passed[1] == "Return", "ends asked with the plug-in in front")
        T.ok(#S.ended == 1 and S.ended[1].as == "ended" and S.ended[1].info.key == "Return"
          and #S.ended[1].info.passed == 1 and S.ended[1].info.passed[1] == "Return", "onEnd told")
        T.ok(S.ended[1].held == false and S.ended[1].live == true, "released by then, the trip acting")
        T.ok(T.count("[page] 'Synth': trip 'Next preset': the hand-over ends — ended, after ") == 1, T.dump())
        T.runDue()
        T.ok(#S.afters == 1 and S.afters[1].name == "Pad" and S.front == T.PAGE, "the back step, the page back")
        T.ok(#S.ended == 1, "told once")
        T.pageFront()
        T.allBack()
    "##);
}

/// Escape ends a typing hand-over too, and is the key `onEnd` gets.
#[test]
fn escape_ends_a_typing_hand_over_and_is_the_key_on_end_gets() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver()
        T.ok(T.typedAndTicked("Escape") == "passed", "through")
        T.ok(#S.ended == 1 and S.ended[1].info.key == "Escape", tostring(S.ended[1] and S.ended[1].info.key))
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
}

/// `browse` holds only Return: an Escape let through by the flag is recorded and logged, and is no
/// end; the Return after it is.
#[test]
fn browse_holds_only_return_and_an_escape_let_through_is_no_end() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver({ kind = "browse", ends = false })
        T.ok(S.holding["Return"] and not S.holding["Escape"], "Return alone")
        T.ok(T.typedAndTicked("Escape") == "passed", "Escape goes through")
        T.ok(#S.ended == 0 and T.count("[page] 'Synth': trip 'Next preset': Escape went through to the plug-in — not an end") == 1,
          T.dump())
        T.ok(T.typedAndTicked("Return") == "passed", "Return goes through")
        T.ok(#S.ended == 1 and S.ended[1].info.key == "Return" and S.ended[1].info.passed[1] == "Escape"
          and S.ended[1].info.passed[2] == "Return", "ended, with both in the record")
        T.ok(T.count("hands the keyboard over (browse): Return let through to window id " .. T.W.id) == 1, T.dump())
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
}

/// `ends` answering no: the hand-over goes on, still holding, waiting; the next Return is looked at,
/// and `onEnd` is told once.
#[test]
fn ends_false_hands_over_again_and_the_next_return_is_looked_at() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.endsFn = function(o, trip, info, answer) answer(#S.endsAsked >= 2) end
        T.handOver()
        T.typedAndTicked("Return")
        T.ok(#S.endsAsked == 1 and #S.ended == 0, "asked, not over")
        T.ok(T.hoHeld() and T.open() == true and S.trip.stage == "waiting", "still holding, waiting")
        T.ok(T.count("[page] 'Synth': trip 'Next preset': its module says the hand-over goes on") == 1, T.dump())
        T.typedAndTicked("Return")
        T.ok(#S.endsAsked == 2 and #S.ended == 1 and S.ended[1].as == "ended", "the second Return ends it")
        T.ok(#S.ended[1].info.passed == 2, "both in the record")
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
}

/// A Return while `ends` waits at a read is kept: when `ends` answers no, it is the next event at
/// once.
#[test]
fn a_return_while_ends_waits_is_kept_and_looked_at_if_ends_says_no() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.endsFn = function(o, trip, info, answer) S.answer = answer end
        T.handOver()
        T.typedAndTicked("Return")
        T.ok(#S.endsAsked == 1 and S.answer ~= nil and T.hoHeld(), "ends waits; the keys held")
        T.ok(T.typedAndTicked("Return") == "passed", "the second Return goes through")
        T.ok(T.count("Return went through to the plug-in while its end is asked — kept") == 1, T.dump())
        S.endsFn = function(o, trip, info, answer) answer(true) end
        T.call("answer", S.answer, false)
        T.ok(T.count("went through to the plug-in while its end was asked — the hand-over's end is looked at") == 1, T.dump())
        T.runDue()
        T.ok(#S.endsAsked == 2 and #S.ended == 1 and #S.ended[1].info.passed == 2, "the kept Return ended it")
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
}

// ---------------------------------------------------------------------------------------------
// The trip's other ends.
// ---------------------------------------------------------------------------------------------

/// The page key ends a hand-over while it holds, once its end key was seen, and while `ends` waits:
/// the keys back in the key's handler, `onEnd("given_up", …, err)` (`cancelled`) before `onGiveUp`,
/// the page shown; a late answer of `ends` changes nothing.
#[test]
fn the_page_key_ends_a_hand_over_while_it_holds_and_while_ends_waits_and_shows_the_page() {
    for stage in ["holding", "seen", "checking"] {
        let scenario = r##"
            local S = T.S
            local stage = "@STAGE@"
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            S.endsFn = function(o, trip, info, answer) S.answer = answer end
            T.handOver()
            if stage ~= "holding" then
              T.typed("Return")
              T.tick()
              if stage == "checking" then T.runDue() end
            end
            T.ok(#S.endsAsked == (stage == "checking" and 1 or 0), stage .. ": ends asked " .. #S.endsAsked)
            local shows, from = #S.pg.shows, #S.order
            T.key()
            T.ok(not T.hoHeld() and T.open() ~= true, stage .. ": the keys back in the key's handler")
            T.ok(#S.ended == 1 and S.ended[1].as == "cancelled" and S.ended[1].info.key == nil, stage .. ": onEnd(page key)")
            T.ok(T.since(from) == "onEnd cancelled,post error,show", T.since(from))
            T.ok(S.giveUps[1] == "cancelled" and #S.pg.shows == shows + 1, "onGiveUp(key), the page shown")
            T.runDue()
            if S.answer then T.call("answer", S.answer, true) end
            T.ok(#S.ended == 1 and #S.afters == 0, stage .. ": the late answer changes nothing")
            if stage == "checking" then
              T.ok(T.count("its hand-over's end answered again, or after it ended — ignored") == 1, T.dump())
            end
            T.pageFront()
            T.allBack(stage)
        "##;
        run(&scenario.replace("@STAGE@", stage));
    }
}

/// Another application's window gives the keys back at its event, and `onEnd("given_up", …, err)`
/// (`moved_on`) is told on the next turn; the page is not shown over it.
#[test]
fn another_applications_window_gives_the_keys_back_at_its_event_and_on_end_after() {
    both(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.handOver()
        local shows = #S.pg.shows
        T.show(T.OTHER)
        T.allBack("right after the event")
        T.ok(#S.ended == 0, "onEnd on the next turn")
        T.runDue()
        T.ok(#S.ended == 1 and S.ended[1].as == "moved_on" and S.ended[1].info.key == nil,
          tostring(S.ended[1] and S.ended[1].as))
        T.ok(#S.pg.shows == shows and S.giveUps[1] == "moved_on", "not shown over it")
        T.runDue()
        T.ok(T.onlyPageKey(), "the page key alone")
    "##);
}

/// A later give-up that comes to the end first — the page key, before the turn after another
/// application's window — is the one `onEnd` and `onGiveUp` both hear: the same error value
/// (`cancelled`), not the first give-up's for one and the key's for the other.
#[test]
fn the_page_key_before_the_turn_after_another_window_is_the_give_up_both_hear() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver()
        T.show(T.OTHER)
        T.allBack("right after the event")
        T.ok(#S.ended == 0 and #S.giveUps == 0, "the rest on the next turn")
        T.key()
        T.ok(#S.ended == 1 and S.ended[1].as == "cancelled", tostring(S.ended[1] and S.ended[1].as))
        T.ok(#S.giveUpErrs == 1 and rawequal(S.ended[1].err, S.giveUpErrs[1]), "one error value: " .. T.what(S.giveUpErrs[1]))
        T.runDue()
        T.ok(#S.ended == 1 and #S.giveUpErrs == 1, "told once")
        T.pageFront()
        T.allBack("after the key")
    "##);
}

/// A window of the plug-in's own application — REAPER's main window, Logic's — is no end: the quiet
/// overlay lets go, and the flag and the scope stay as the hand-over set them (the guards in
/// _deactivate and _menuFlagLeft; applyMenuState's and _menuHold's are the next test's), logged
/// once; back in the plug-in, Return ends it.
#[test]
fn a_window_of_the_plugins_application_is_no_end_and_keeps_the_flag_and_the_scope() {
    both(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.handOver()
        local n = #S.menuOpen
        T.show(T.MAINWIN)
        T.ok(not ov.active, "the quiet overlay let go")
        T.ok(#S.menuOpen == n and T.open() == true and S.scopedTo == T.W.id and T.hoHeld(), "flag and scope untouched")
        T.ok(T.count("came forward during the hand-over — not an end") == 1, T.dump())
        T.tick(3)
        ov:_menuFlagLeft()
        T.ok(#S.menuOpen == n, "no write from the tick or _menuFlagLeft: " .. T.history(n))
        T.show(T.W, T.IN)
        T.show(T.MAINWIN)
        T.show(T.W, T.IN)
        T.ok(T.count("came forward during the hand-over — not an end") == 1, "said once: " .. T.dump())
        T.ok(#S.menuOpen == n and #S.ended == 0, "still holding")
        T.runDue()
        T.ok(T.typedAndTicked("Return") == "passed", "through")
        T.ok(#S.ended == 1 and S.ended[1].as == "ended", tostring(S.ended[1] and S.ended[1].as))
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
}

/// The runtime's own writes of the flag and the scope wait for the hand-over: an overlay of the same
/// module outside the group — on another slot, with menu tests, sharing the binding — in front in the
/// plug-in's window during it: the menu tick's state, applied for that overlay, writes no flag; its
/// leaving writes neither the flag nor the scope; its hold over a menu's window of the plug-in's
/// process writes no flag. The hand-over's end sets them back — the scope every window when nothing
/// of the module is in front, and left pinned to the plug-in's window for that overlay when it is.
#[test]
fn the_runtimes_own_writes_of_the_flag_and_the_scope_wait_for_the_hand_over() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local b = T.O.embedded(T.bindingSpec())
        local ov = T.synth({ bound = b })
        local other = T.O.new("Other")
        other:addCustomButton({ label = "Other", onActivate = function() end })
        T.hooked(other)
        other:bind(b, { slot = "com.platform.else", specificity = T.O.layer.base, menus = { T.O.menuTests.newWindow } })
        T.runDue()
        T.openPage()
        T.handOver()
        T.ok(other.active and not other.away and ov.away, "the other overlay is in front, not of the group")
        local n = #S.menuOpen
        T.tick(3)
        T.ok(#S.menuOpen == n and T.open() == true, "no flag written for the other overlay: " .. T.history(n))
        T.show(T.OTHER)
        T.ok(T.open() == false and S.scopedTo == false, "set back by the hand-over's end")
        T.runDue()
        T.ok(#S.ended == 1 and S.ended[1].as == "moved_on", tostring(S.ended[1] and S.ended[1].as))
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(other.active and S.scopedTo == T.W.id, "the other overlay back as before")
    "##);
    // The other overlay leaving during the hand-over: neither the flag nor the scope written.
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local b = T.O.embedded(T.bindingSpec())
        local ov = T.synth({ bound = b })
        local other = T.O.new("Other")
        other:addCustomButton({ label = "Other", onActivate = function() end })
        T.hooked(other)
        other:bind(b, { slot = "com.platform.else", specificity = T.O.layer.base, menus = { T.O.menuTests.newWindow } })
        T.runDue()
        T.openPage()
        T.handOver()
        local n = #S.menuOpen
        T.show(T.MAINWIN)
        T.ok(not other.active and #S.menuOpen == n and T.open() == true and S.scopedTo == T.W.id,
          "the leaving wrote nothing: " .. T.history(n) .. " " .. tostring(S.scopedTo))
        T.show(T.W, T.IN)
        T.runDue()
        T.key()
        T.ok(#S.ended == 1 and S.ended[1].as == "cancelled", "ended at the key")
        T.pageFront()
    "##);
    // The other overlay's control pressed and a menu of the plug-in's process in a window of its own
    // in front: the other overlay holds its place over it, and its hold writes no flag.
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local b = T.O.embedded(T.bindingSpec())
        local ov = T.synth({ bound = b })
        local other = T.O.new("Other")
        other:addCustomButton({ label = "Other", onActivate = function() end })
        T.hooked(other)
        other:bind(b, { slot = "com.platform.else", specificity = T.O.layer.base, menus = { T.O.menuTests.newWindow } })
        T.runDue()
        T.openPage()
        T.handOver()
        T.ok(other.active and not other.away, "the other overlay is in front")
        -- A press of its control, in the handler of the key that pressed it.
        T.call("key", function() other:_menuOpening({ label = "Other" }) end)
        local n = #S.menuOpen
        S.windows = { S.windows[1], { id = T.POPUP.id, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        T.ok(T.count("came to the front after 'Other' was pressed and a test sees a menu — the overlay holds its place") == 1,
          T.dump())
        T.ok(#S.menuOpen == n and T.open() == true and S.holding["Return"] and S.holding["Escape"],
          "no flag written by the hold, the hand-over's keys held: " .. T.history(n))
        T.tick(3)
        T.ok(#S.menuOpen == n and T.open() == true, "nor by the ticks over the menu: " .. T.history(n))
        T.key()
        T.ok(#S.ended == 1 and S.ended[1].as == "cancelled", "ended at the key")
        T.pageFront()
    "##);
    // The hand-over's own end, the other overlay still in front: the scope stays pinned for it.
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local b = T.O.embedded(T.bindingSpec())
        local ov = T.synth({ bound = b })
        local other = T.O.new("Other")
        other:addCustomButton({ label = "Other", onActivate = function() end })
        T.hooked(other)
        other:bind(b, { slot = "com.platform.else", specificity = T.O.layer.base, menus = { T.O.menuTests.newWindow } })
        T.runDue()
        T.openPage()
        T.handOver()
        T.ok(other.active and not other.away, "the other overlay is in front")
        T.ok(T.typedAndTicked("Return") == "passed", "through")
        T.ok(#S.ended == 1 and S.ended[1].as == "ended", tostring(S.ended[1] and S.ended[1].as))
        -- (The other overlay's own keys stay held: its Space, which the hand-over did not take.)
        T.ok(not S.holding["Return"] and not S.holding["Escape"] and T.open() == false and S.scopedTo == T.W.id,
          "the keys back, the scope left pinned for the other overlay: " .. tostring(S.scopedTo))
        T.ok(T.count("the hand-over ends — ended, after ") == 1 and T.count("the scope left to the overlay in front") == 1,
          T.dump())
        T.runDue()
        T.pageFront()
    "##);
}

/// The page closed ends it: its close box and its failure in their handler, the module's own
/// page:close() at the next tick.
#[test]
fn the_page_closed_ends_it() {
    for how in ["user", "failed", "module"] {
        let scenario = r##"
            local S = T.S
            local how = "@HOW@"
            T.show(T.W, T.IN)
            local ov = T.synth()
            T.openPage()
            T.handOver()
            if how == "module" then
              T.page:close()
              T.ok(T.hoHeld(), "nothing seen yet")
              T.tick()
              T.ok(not T.hoHeld(), "the tick saw the page not shown")
              T.ok(T.count("[page] 'Synth': the page is not shown any more — trip 'Next preset' ends with its hand-over") == 1, T.dump())
              T.runDue()
            else
              T.closed(how)
            end
            T.ok(not T.hoHeld() and #S.ended == 1 and S.ended[1].as == "closed", how .. ": " .. tostring(S.ended[1] and S.ended[1].as))
            T.ok(S.giveUps[1] == "closed", tostring(S.giveUps[1]))
            T.runDue()
            T.ok(#S.ended == 1, "once")
            if how ~= "module" then T.ok(ov.active and not ov.away, "back in the plug-in") end
        "##;
        run(&scenario.replace("@HOW@", how));
    }
}

/// The plug-in's window gone ends it: on Windows not listed any more, or the plug-in's control not
/// in it; on a Mac the window back under another title.
#[test]
fn the_plugins_window_gone_ends_it() {
    for case in ["unlisted", "unloaded"] {
        let scenario = r##"
            local S = T.S
            local case = "@CASE@"
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            T.handOver()
            if case == "unlisted" then
              S.listed = { T.MAIN, T.OTHER }
              T.show(T.MAINWIN)
            else
              S.controls = { T.LIST, T.WRAP }
              T.event()
            end
            T.ok(not T.hoHeld(), case .. ": the keys back at the event")
            T.runDue()
            T.ok(#S.ended == 1 and S.ended[1].as == "gone" and S.giveUps[1] == "gone", case .. ": " .. tostring(S.giveUps[1]))
            T.runDue()
        "##;
        run(&scenario.replace("@CASE@", case));
    }
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver()
        T.show({ id = T.LOGIC.id, title = "Inst 2", class = T.LOGIC.class, app = T.MAC_LOGIC,
          bounds = T.LOGIC.bounds, client = T.LOGIC.client })
        T.ok(not T.hoHeld(), "the keys back")
        T.runDue()
        T.ok(#S.ended == 1 and S.ended[1].as == "gone", tostring(S.ended[1] and S.ended[1].as))
        T.runDue()
    "##);
}

/// A disable mid hand-over takes the captures out and forgets the flag and the scope; the enable
/// brings the captures back in every window with no flag. The first tick ends the trip ("page
/// closed") and releases them; so does a Return first, which a capture takes with the page not shown.
/// `onEnd` once either way, nothing held. A capture of a hand-over that has ended releases itself.
#[test]
fn a_disable_and_enable_mid_hand_over_ends_it_at_the_first_tick_and_a_stale_capture_releases_itself() {
    for first in ["tick", "return"] {
        let scenario = r##"
            local S = T.S
            local first = "@FIRST@"
            T.show(T.W, T.IN)
            local ov = T.synth()
            T.openPage()
            T.handOver()
            local onReturn = S.captured["Return"]
            T.disable()
            T.enable()
            T.ok(T.hoHeld() and S.scopedTo == false and T.open() == false, "back in every window, no flag")
            if first == "tick" then
              T.tick()
              T.ok(not T.hoHeld(), "released at the tick")
              T.runDue()
            else
              T.ok(T.typed("Return") == "taken", "a capture takes it: no flag")
              T.ok(not T.hoHeld(), "released in the key's handler")
              T.ok(T.count("Return was taken by the hand-over's capture with the page not shown") == 1, T.dump())
            end
            T.ok(#S.ended == 1 and S.ended[1].as == "closed", first .. ": " .. tostring(S.ended[1] and S.ended[1].as))
            T.ok(T.last() == "Next preset: the page was closed; nothing more was done.", T.last())
            T.runDue()
            T.ok(#S.ended == 1, "once")
            T.ok(not T.hoHeld() and T.open() ~= true, first .. ": the keys and the flag back")
            -- The old capture's callback, come late: released, nothing more.
            T.call("key", onReturn, {})
            T.ok(T.count("Return came to a capture of a hand-over that has ended — released") == 1, T.dump())
            T.ok(#S.ended == 1, "still once")
        "##;
        run(&scenario.replace("@FIRST@", first));
    }
}

/// A Return the hook took instead of letting it through — the module's own code cleared its flag —
/// is no event: logged, the scope and the flag set again, the runtime's sentence behind the screen
/// reader; still waiting. The next Return passes and ends it.
#[test]
fn a_return_taken_instead_of_let_through_pins_again_says_so_and_is_no_end() {
    both(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver()
        T.host.keys.menuOpen(false)
        T.ok(T.typed("Return") == "taken", "taken by the capture")
        T.ok(T.count("[page] 'Synth': trip 'Next preset': Return was taken, not let through — the menu flag did not "
          .. "count for the window in front") == 1, T.dump())
        T.ok(T.last() == "Next preset: Return did not reach the plug-in; press it again." and T.lastInterrupts() == false,
          T.last())
        T.ok(T.open() == true and S.scopedTo == T.W.id, "pinned again")
        T.ok(S.trip.stage == "waiting" and #S.endsAsked == 0, "no event")
        T.ok(T.typedAndTicked("Return") == "passed", "the next one passes")
        T.ok(#S.ended == 1 and S.ended[1].as == "ended", "and ends it")
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
}

/// `done` while the hand-over holds: the keys back in `done`, and `onEnd("done")` in the back step,
/// before `after`.
#[test]
fn done_while_it_holds_releases_and_on_end_hears_done_before_after() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          S.handed = table.pack(trip:handOver(T.spec()))
          S.hoTokens = { Return = S.holding["Return"], Escape = S.holding["Escape"] }
          trip:done({ name = "early" })
          S.heldAfterDone, S.toldAtDone = T.hoHeld(), #S.ended
        end
        T.trip()
        local from = #S.order
        T.land()
        T.runDue()
        T.ok(S.handed[1] == true and S.heldAfterDone == false and S.toldAtDone == 0, "released in done; told later")
        T.ok(#S.ended == 1 and S.ended[1].as == "done" and #S.afters == 1 and S.afters[1].name == "early", "told, then after")
        T.ok(string.find(T.since(from), "onEnd done,post state", 1, true) ~= nil, T.since(from))
        T.pageFront()
        T.allBack()
    "##);
}

/// `onEnd` is told exactly once on every end of the trip: its own, the page key, another
/// application, the page closed, the window gone, outranked, a hook that raised, `done`, and a
/// disable and enable — however many events, ticks and turns come after.
#[test]
fn on_end_is_told_exactly_once_on_every_end() {
    for end in ["ended", "page key", "another application", "page closed", "gone", "outranked", "failed", "done", "disable"] {
        let scenario = r##"
            local S = T.S
            local ending = "@END@"
            T.show(T.W, T.IN)
            local ov = T.synth()
            T.openPage()
            if ending == "failed" then S.endsFn = function() error("ends broke") end end
            if ending == "done" then
              S.endsFn = function(o, trip, info, answer) trip:done({}) end
            end
            T.handOver()
            if ending == "ended" or ending == "failed" or ending == "done" then
              T.typedAndTicked("Return")
            elseif ending == "page key" then
              T.key()
            elseif ending == "another application" then
              T.show(T.OTHER)
            elseif ending == "page closed" then
              T.closed("user")
            elseif ending == "gone" then
              S.controls = { T.LIST, T.WRAP }
              T.event()
            elseif ending == "outranked" then
              local id = T.another(T.O.layer.content)
              T.matching(id, true)
              T.typedAndTicked("Return")
            elseif ending == "disable" then
              T.disable()
              T.enable()
            end
            for _ = 1, 4 do
              T.tick()
              T.runDue()
            end
            T.event()
            T.runDue()
            -- Its own end and done as themselves; every other end given up, with its error's code.
            local want = ({ ended = "ended", done = "done", ["page key"] = "cancelled", ["another application"] = "moved_on",
              ["page closed"] = "closed", gone = "gone", outranked = "taken", failed = "failed", disable = "closed" })[ending]
            T.ok(#S.ended == 1 and S.ended[1].as == want, ending .. ": " .. #S.ended .. " " .. tostring(S.ended[1] and S.ended[1].as))
            T.ok(not T.hoHeld(), ending .. ": the keys back")
            if ending == "failed" then T.ok(T.count("[page] 'Synth': ends failed: ") == 1, T.dump()) end
        "##;
        run(&scenario.replace("@END@", end));
    }
}

// ---------------------------------------------------------------------------------------------
// `keys`.
// ---------------------------------------------------------------------------------------------

/// A `keys` hand-over captures its keys for `onKey`, as a handler with the plug-in in front, and
/// writes no flag: PageUp is answered and goes on, Return answers true and is its event, a key after
/// it is not handed to `onKey`, and `onEnd("ended", { key = "Return" })` runs with the plug-in in
/// front.
#[test]
fn a_keys_hand_over_captures_its_keys_for_on_key_writes_no_flag_and_ends_when_on_key_says() {
    both(r##"
        local S = T.S
        local H = T.host
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        local mark = #S.menuOpen
        S.got = {}
        T.handOver({ kind = "keys", keys = { "PageUp", "PageDown", "Return" }, ends = false,
          onKey = function(o, trip, key, mods)
            H.screen.snapshot({ region = { 110, 60, 300, 80 } })
            local f = H.window.foreground()
            S.got[#S.got + 1] = { key = key, inW = f.id == T.W.id, mods = mods }
            return key == "Return"
          end })
        T.ok(S.holding["PageUp"] and S.holding["PageDown"] and S.holding["Return"] and S.scopedTo == T.W.id, "captured, pinned")
        T.ok(T.count("hands the keyboard over (keys): PageUp, PageDown, Return captured for the module in window id "
          .. T.W.id .. ", the module's scope pinned there; it ends on its onKey") == 1, T.dump())
        T.ok(T.typed("PageUp") == "taken", "taken for the module")
        T.ok(#S.got == 1 and S.got[1].key == "PageUp" and S.got[1].inW and type(S.got[1].mods) == "table", "onKey had it")
        T.ok(#S.ended == 0 and T.hoHeld(), "goes on")
        T.ok(T.typed("Return") == "taken", "Return for the module too")
        T.ok(T.typed("PageDown") == "taken" and #S.got == 2, "a key after the event: not handed to onKey")
        T.ok(T.count("PageDown pressed while the hand-over's end is looked at — not handed to the module") == 1, T.dump())
        T.runDue()
        T.ok(#S.ended == 1 and S.ended[1].as == "ended" and S.ended[1].info.key == "Return"
          and S.ended[1].info.passed[1] == "PageUp" and S.ended[1].info.passed[2] == "Return", "ended by onKey")
        for i = mark + 1, #S.menuOpen do T.ok(S.menuOpen[i] ~= true, "no flag written") end
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
}

/// A `keys` hand-over is ended by the page key and by a window change as every hand-over is; a key
/// pressed with another window in front is not handed to the module.
#[test]
fn a_keys_hand_over_is_ended_by_the_page_key_and_a_window_change_too() {
    for how in ["key", "other"] {
        let scenario = r##"
            local S = T.S
            local how = "@HOW@"
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            S.got = 0
            T.handOver({ kind = "keys", keys = { "PageUp" }, ends = false,
              onKey = function() S.got += 1 end })
            -- A window of the plug-in's application in front, the trip going on: a captured key that
            -- reaches its callback then is not the module's.
            T.show(T.MAINWIN)
            T.call("key", S.captured["PageUp"], {})
            T.ok(S.got == 0 and T.count("PageUp pressed with ") == 1 and T.count("in front — not handed to the module") == 1,
              T.dump())
            T.show(T.W, T.IN)
            T.runDue()
            if how == "key" then
              T.key()
              T.ok(S.ended[1].as == "cancelled", tostring(S.ended[1] and S.ended[1].as))
              T.pageFront()
            else
              T.show(T.OTHER)
              T.runDue()
              T.ok(S.ended[1].as == "moved_on", tostring(S.ended[1] and S.ended[1].as))
            end
            T.ok(#S.ended == 1 and S.got == 0 and not T.hoHeld(), "once, nothing handed")
        "##;
        run(&scenario.replace("@HOW@", how));
    }
}

// ---------------------------------------------------------------------------------------------
// Keys swallowed while it lasts (`swallow`, runtime 0.16).
// ---------------------------------------------------------------------------------------------

/// `swallow = { keys, say }` takes its keys for as long as the hand-over lasts, of every kind: the
/// hook takes them even while a `typing` or `browse` hand-over's flag lets its other keys through
/// (`menu = "taken"`), so they reach neither the plug-in nor the record of keys gone through, nor the
/// module's `onKey`; nothing is said at the start, the module's sentence once at the first of them,
/// interrupting, and never again in that hand-over; the end keys end it as ever, and the swallowed
/// keys go back with them. A second hand-over says its sentence again, once.
#[test]
fn swallowed_keys_do_nothing_say_once_and_reach_neither_the_plugin_nor_the_module() {
    for kind in ["typing", "browse", "keys"] {
        let scenario = r##"
            local S = T.S
            local kind = "@KIND@"
            local SAY = "Return confirms, Escape cancels."
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            S.got = {}
            local function spec()
              local over = { swallow = { keys = { "Tab", "Shift+Tab" }, say = SAY } }
              if kind == "keys" then
                over.kind, over.keys, over.ends = "keys", { "PageUp", "Return" }, false
                over.onKey = function(o, trip, key)
                  S.got[#S.got + 1] = key
                  return key == "Return"
                end
              elseif kind == "browse" then
                over.kind = "browse"
              end
              return over
            end
            for round = 1, 2 do
              S.ended, S.endsAsked, S.got = {}, {}, {}
              local said = #S.speech
              local ok, why = T.handOver(spec())
              T.ok(ok == true and why == nil, T.what(why))
              T.ok(S.holding["Tab"] and S.holding["Shift+Tab"] and S.menuTaken["Tab"] and S.menuTaken["Shift+Tab"],
                "captured, taken in a menu")
              T.ok(S.hoTokens["Tab"] ~= nil and S.hoTokens["Shift+Tab"] ~= nil, "the hand-over's")
              T.ok(#S.speech == said, "nothing said at the start")
              T.ok(T.count("the module's scope pinned there; Tab, Shift+Tab swallowed there; it ends on ") == round, T.dump())
              T.ok(T.typed("Tab") == "taken", kind .. ": Tab taken")
              T.ok(#S.passed == 0, "not recorded as gone through")
              T.ok(#S.speech == said + 1 and T.last() == SAY and T.lastInterrupts() == true, T.saidSince(said))
              T.ok(T.typed("Shift+Tab") == "taken" and T.typed("Tab") == "taken", "again")
              T.ok(#S.speech == said + 1, "said once: " .. T.saidSince(said))
              T.ok(T.count("[page] 'Synth': trip 'Next preset': Tab swallowed — it does nothing and never reaches the "
                .. "plug-in: said \"" .. SAY .. "\"") == round, T.dump())
              T.ok(T.count("swallowed — it does nothing and never reaches the plug-in, said once already") == 2 * round, T.dump())
              T.tick()
              T.runDue()
              T.ok(#S.ended == 0 and #S.endsAsked == 0 and #S.got == 0 and S.trip.stage == "waiting",
                "no end, nothing for the module")
              -- The end key ends it as ever; the swallowed keys go back with the rest.
              T.ok(T.typedAndTicked("Return") == (kind == "keys" and "taken" or "passed"), "the end key")
              T.ok(#S.ended == 1 and S.ended[1].as == "ended" and S.ended[1].info.key == "Return", tostring(S.ended[1]))
              T.ok(table.concat(S.ended[1].info.passed, ",") == "Return", "nothing swallowed among what went through: "
                .. table.concat(S.ended[1].info.passed, ","))
              T.ok(table.concat(S.got, ",") == (kind == "keys" and "Return" or ""), "onKey: " .. table.concat(S.got, ","))
              T.ok(not S.holding["Tab"] and not S.holding["Shift+Tab"] and not T.hoHeld(), "given back")
              T.ok(T.count("released with Tab, Shift+Tab, swallowed") == round, T.dump())
              T.ok(#S.speech == said + 1, "nothing more said: " .. T.saidSince(said))
              T.runDue()
              T.pageFront()
              T.allBack()
            end
        "##;
        let scenario = scenario.replace("@KIND@", kind);
        run(&scenario);
        run_mac(&scenario);
    }
}

/// Every end gives the swallowed keys back, with the rest, at that moment: its own, the page key,
/// another application, the page closed, the window gone, a hook that raised, `done`, and a disable
/// and enable — where a swallowed key that a capture takes with the page not shown ends the trip
/// itself, as an end key does. A swallowed key's capture that comes after the end releases itself
/// and says nothing.
#[test]
fn every_end_gives_the_swallowed_keys_back() {
    for end in ["ended", "page key", "another application", "page closed", "gone", "failed", "done", "disable", "disable tab"] {
        let scenario = r##"
            local S = T.S
            local ending = "@END@"
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            if ending == "failed" then S.endsFn = function() error("ends broke") end end
            if ending == "done" then
              S.endsFn = function(o, trip, info, answer) trip:done({}) end
            end
            T.handOver({ swallow = { keys = { "Tab", "Shift+Tab" }, say = "Return keeps the name, Escape cancels." } })
            local onTab = S.captured["Tab"]
            T.ok(S.hoTokens["Tab"] ~= nil and S.hoTokens["Shift+Tab"] ~= nil, "swallowed")
            if ending == "ended" or ending == "failed" or ending == "done" then
              T.typedAndTicked("Return")
            elseif ending == "page key" then
              T.key()
            elseif ending == "another application" then
              T.show(T.OTHER)
            elseif ending == "page closed" then
              T.closed("user")
            elseif ending == "gone" then
              S.controls = { T.LIST, T.WRAP }
              T.event()
            elseif ending == "disable" then
              T.disable()
              T.enable()
            elseif ending == "disable tab" then
              T.disable()
              T.enable()
              T.ok(T.typed("Tab") == "taken", "a capture takes it: no flag")
              T.ok(T.count("Tab was taken by the hand-over's capture with the page not shown") == 1, T.dump())
            end
            for _ = 1, 4 do
              T.tick()
              T.runDue()
            end
            T.event()
            T.runDue()
            local want = ({ ended = "ended", done = "done", ["page key"] = "cancelled", ["another application"] = "moved_on",
              ["page closed"] = "closed", gone = "gone", failed = "failed", disable = "closed", ["disable tab"] = "closed" })[ending]
            T.ok(#S.ended == 1 and S.ended[1].as == want, ending .. ": " .. #S.ended .. " " .. tostring(S.ended[1] and S.ended[1].as))
            T.ok(not T.hoHeld() and S.hoTokens["Tab"] ~= nil, ending .. ": the keys back, the swallowed ones among them")
            -- The old capture's callback, come late: released, nothing said.
            local said = #S.speech
            T.call("key", onTab, {})
            T.ok(T.count("Tab came to a capture of a hand-over that has ended — released") == 1, T.dump())
            T.ok(#S.speech == said and #S.ended == 1, "nothing said, still once")
        "##;
        run(&scenario.replace("@END@", end));
    }
}

/// A swallowed key whose capture's callback runs with another window in front — a window of the
/// plug-in's application, the trip going on — does nothing and says nothing; the sentence waits for
/// the first one pressed with the plug-in in front.
#[test]
fn a_swallowed_key_with_another_window_in_front_says_nothing() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver({ kind = "browse", swallow = { keys = { "Tab" }, say = "To go back, press Return." } })
        local said = #S.speech
        T.show(T.MAINWIN)
        T.call("key", S.captured["Tab"], {})
        T.ok(#S.speech == said and T.count("Tab swallowed with ") == 1 and T.count("in front — it does nothing; nothing said") == 1,
          T.dump())
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(T.typed("Tab") == "taken" and #S.speech == said + 1 and T.last() == "To go back, press Return.", T.saidSince(said))
        T.ok(#S.ended == 0, "still going on")
        T.key()
        T.pageFront()
        T.allBack()
    "##);
}

// ---------------------------------------------------------------------------------------------
// Where it may be asked, and how it answers.
// ---------------------------------------------------------------------------------------------

/// handOver raises, naming the caller's line: outside the trip's act (in `after`), while another
/// wait is pending, from inside its own `ends`, and for a spec it cannot hold.
#[test]
fn hand_over_raises_outside_acting_with_a_wait_or_another_hand_over_pending_and_for_a_bad_spec() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        local errs = {}
        local function try(name, trip, spec)
          local ok, err = pcall(function() return trip:handOver(spec) end)
          errs[name] = (not ok) and tostring(err) or "did not raise"
        end
        S.actFn = function(o, trip)
          try("not a table", trip, "typing")
          try("unknown field", trip, T.spec({ wait = 3 }))
          try("bad kind", trip, T.spec({ kind = "voice" }))
          try("no onEnd", trip, T.spec({ onEnd = false }))
          try("keys without keys", trip, T.spec({ kind = "keys", ends = false, onKey = function() end }))
          try("keys without onKey", trip, T.spec({ kind = "keys", ends = false, keys = { "PageUp" } }))
          try("keys with ends", trip, T.spec({ kind = "keys", keys = { "PageUp" }, onKey = function() end }))
          try("typing with onKey", trip, T.spec({ onKey = function() end }))
          try("ends not a function", trip, T.spec({ ends = 5 }))
          try("unreadable key", trip, T.spec({ keys = { "Return", "Nope+Q" } }))
          try("a key twice", trip, T.spec({ keys = { "Return", "Return" } }))
          try("nine keys", trip, T.spec({ keys = { "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9" } }))
          try("no keys", trip, T.spec({ keys = {} }))
          try("swallow not a table", trip, T.spec({ swallow = "Tab" }))
          try("swallow's field", trip, T.spec({ swallow = { keys = { "Tab" }, say = "x", when = 1 } }))
          try("swallow without keys", trip, T.spec({ swallow = { say = "x" } }))
          try("swallow nine keys", trip, T.spec({ swallow = { keys = { "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9" },
            say = "x" } }))
          try("swallow without say", trip, T.spec({ swallow = { keys = { "Tab" } } }))
          try("swallow empty say", trip, T.spec({ swallow = { keys = { "Tab" }, say = "" } }))
          try("swallow unreadable", trip, T.spec({ swallow = { keys = { "Nope+Q" }, say = "x" } }))
          try("swallow twice", trip, T.spec({ swallow = { keys = { "Tab", "Tab" }, say = "x" } }))
          try("swallow an end key", trip, T.spec({ swallow = { keys = { "Tab", "Escape" }, say = "x" } }))
          trip:afterDialog(function() end)
          try("a wait pending", trip, T.spec())
          S.trip = trip
        end
        T.trip()
        T.land()
        T.runDue()
        local want = {
          ["not a table"] = "handOver takes { kind, keys?, onKey?, ends?, onEnd, swallow? }, got string",
          ["unknown field"] = "handOver has no field 'wait'",
          ["bad kind"] = "kind must be \"typing\", \"browse\" or \"keys\", got voice",
          ["no onEnd"] = "needs `onEnd`, a function(overlay, trip, how, info), got nil",
          ["keys without keys"] = "of kind \"keys\" needs `keys`",
          ["keys without onKey"] = "of kind \"keys\" needs `onKey`",
          ["keys with ends"] = "of kind \"keys\" takes no `ends`",
          ["typing with onKey"] = "of kind \"typing\" takes no `onKey`",
          ["ends not a function"] = "`ends` must be a function(overlay, trip, info, answer), got number",
          ["unreadable key"] = "key Nope+Q is no key spec",
          ["a key twice"] = "names one key twice: Return and Return",
          ["nine keys"] = "`keys` must be a list of 1 to 8 key specs",
          ["no keys"] = "`keys` must be a list of 1 to 8 key specs",
          ["swallow not a table"] = "handOver's `swallow` takes { keys, say }, got string",
          ["swallow's field"] = "handOver's `swallow` has no field 'when' (it has keys and say)",
          ["swallow without keys"] = "handOver's `swallow.keys` must be a list of 1 to 8 key specs",
          ["swallow nine keys"] = "handOver's `swallow.keys` must be a list of 1 to 8 key specs",
          ["swallow without say"] = "handOver's `swallow.say` must be the sentence said at the first key swallowed, got nil",
          ["swallow empty say"] = "`swallow.say` must be the sentence said at the first key swallowed, got an empty string",
          ["swallow unreadable"] = "handOver's swallowed key Nope+Q is no key spec",
          ["swallow twice"] = "handOver swallows one key twice: Tab and Tab",
          ["swallow an end key"] = "handOver names Escape in its keys and in `swallow`: a key is held or swallowed, not both",
          ["a wait pending"] = "handOver while a dialog wait is pending — one wait at a time",
        }
        for name, text in pairs(want) do
          local err = errs[name] or "not tried"
          T.ok(string.find(err, text, 1, true) ~= nil, name .. ": " .. err)
          T.ok(string.find(err, '[string "scenario"]:', 1, true) == 1, name .. " names the caller's line: " .. err)
        end
        T.ok(next(S.ended) == nil and S.holding["Return"] == nil, "nothing held, nothing told")
        S.trip:done({})
        T.runDue()
        T.pageFront()
        -- From inside its own `ends`, and from `after`, once the trip is over.
        S.endsFn = function(o, trip, info, answer)
          local ok, err = pcall(trip.handOver, trip, T.spec())
          S.inEnds = (not ok) and tostring(err) or "did not raise"
          answer(true)
        end
        S.handed = nil
        S.actFn = function(o, trip) S.handed = table.pack(trip:handOver(T.spec())) end
        T.trip({ after = function(o, result)
          local ok, err = pcall(S.trip.handOver, S.trip, T.spec())
          S.inAfter = (not ok) and tostring(err) or "did not raise"
        end })
        T.land()
        T.runDue()
        T.typedAndTicked("Return")
        T.ok(string.find(S.inEnds or "", "trip 'Next preset': handOver while its hand-over is under way — one wait at a time",
          1, true) ~= nil, tostring(S.inEnds))
        T.runDue()
        T.ok(string.find(S.inAfter or "", "handOver is for the trip's act and its continuations, while it is acting (it is over)",
          1, true) ~= nil, tostring(S.inAfter))
        T.pageFront()
    "##);
}

/// The plug-in not in front at the call: `trip:inFront()` gives the trip up, and handOver answers
/// `false` and the give-up's own error value, the one `onGiveUp` is handed for the same moment —
/// `moved_on` with another application's window in front, `not_in_front` with nothing in front —
/// with nothing captured or written, and the trip no longer live.
#[test]
fn hand_over_with_the_plugin_not_in_front_answers_the_give_ups_own_error() {
    for (front, code, message) in [
        ("T.OTHER", "moved_on", "a window of another application came to the front, or one of this application's that is not the page"),
        ("nil", "not_in_front", "the plug-in's window does not have the keyboard"),
    ] {
        run(&format!(
            r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          S.front = {front}
          S.chain = S.front and {{ S.front }} or {{}}
          local n = #S.menuOpen
          S.handed = table.pack(trip:handOver(T.spec()))
          S.wrote, S.heldInAct, S.liveAfter = #S.menuOpen - n, next(S.holding), trip.live
        end
        T.trip()
        T.land()
        T.runDue()
        T.ok(S.handed[1] == false and T.is(S.handed[2], "{code}", "{message}"), T.what(S.handed[2]))
        T.ok(S.heldInAct == nil and S.wrote == 0 and S.liveAfter == false, "nothing captured, no flag, the trip over")
        T.ok(#S.giveUps == 1 and rawequal(S.giveUpErrs[1], S.handed[2]) and #S.ended == 0,
          "onGiveUp handed the same error value; no onEnd: no hand-over began")
    "##
        ));
    }
}

/// A capture the host cannot make (a Mac without the Accessibility grant): handOver answers
/// `false, err`, the capture's own error value, what it captured is released, no flag is written, the
/// scope is set back, and the trip goes on. The capture registered nothing, so nothing else is let go.
#[test]
fn a_capture_that_fails_answers_its_error_and_leaves_nothing_held() {
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.captureRaises = { Escape = "the event tap could not be created (Accessibility)" }
        S.actFn = function(o, trip)
          local n = #S.menuOpen
          S.handed = table.pack(trip:handOver(T.spec()))
          S.wrote = #S.menuOpen - n
          S.held = next(S.holding)
          trip:done({})
        end
        T.trip()
        T.land()
        T.runDue()
        T.ok(S.handed[1] == false and T.is(S.handed[2], "denied", "the event tap could not be created (Accessibility)"),
          T.what(S.handed[2]))
        T.ok(S.held == nil and S.wrote == 0 and S.releasedAll == nil and S.scopedTo == false, "nothing held or written")
        T.ok(T.count("the keyboard is not handed over — Escape is not captured: the event tap could not be created "
          .. "(Accessibility)") == 1, T.dump())
        S.captureRaises = nil
        T.runDue()
        T.ok(#S.afters == 1 and #S.ended == 0 and #S.giveUps == 0, "the trip went on to its done")
        T.pageFront()
    "##);
}

/// A Return the record held from before the hand-over — one that reached a menu earlier — is no end
/// of it: dropped at its start, and logged.
#[test]
fn keys_let_through_before_the_hand_over_are_not_its_end() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          S.passed[#S.passed + 1] = { vk = 13, mask = 0, key = "Return" }
          S.handed = table.pack(trip:handOver(T.spec()))
          S.hoTokens = { Return = S.holding["Return"], Escape = S.holding["Escape"] }
        end
        T.trip()
        T.land()
        T.runDue()
        T.ok(T.count("[page] 'Synth': trip 'Next preset': went through before the hand-over, not its end: Return") == 1, T.dump())
        T.tick(3)
        T.runDue()
        T.ok(#S.endsAsked == 0 and #S.ended == 0 and T.hoHeld(), "no event")
        T.key()
        T.pageFront()
        T.allBack()
    "##);
}

/// What the record still holds at the end is dropped with the keys, and logged: a Return let through
/// after the tick saw the end, before the end was looked at, is no event of anything later — the menu
/// tick's or the next hand-over's.
#[test]
fn what_went_through_after_the_end_is_dropped_at_the_release_and_logged() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver()
        T.typed("Return")
        T.tick()
        T.ok(T.typed("Return") == "passed", "a second Return through before the end is looked at")
        T.runDue()
        T.ok(#S.ended == 1 and S.ended[1].as == "ended" and #S.ended[1].info.passed == 1, "ended by the first")
        T.ok(T.count("the hand-over ends — ended, after ") == 1 and T.count("; went through after the end: Return") == 1,
          T.dump())
        T.ok(#S.passed == 0, "the record empty")
        T.runDue()
        T.pageFront()
        T.tick(2)
        T.handOver()
        T.ok(T.count("went through before the hand-over") == 0, T.dump())
        T.tick(2)
        T.runDue()
        T.ok(#S.ended == 1 and #S.endsAsked == 1, "no event of the next hand-over")
        T.key()
        T.pageFront()
        T.allBack()
    "##);
}

/// Nothing bounds a hand-over: ten minutes on, it still holds, and nothing was given up.
#[test]
fn no_bound_ends_a_hand_over() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth({ menus = false })
        T.openPage()
        T.handOver()
        T.run(600000)
        T.ok(T.hoHeld() and T.open() == true and #S.ended == 0 and #S.giveUps == 0 and S.trip.live, "still holding")
        T.key()
        T.ok(#S.ended == 1 and S.ended[1].as == "cancelled", "the page key ends it")
        T.ok(T.count("[page] 'Synth': trip 'Next preset': the hand-over ends — page key, after 600000 ms") == 1, T.dump())
        T.pageFront()
    "##);
}

/// The tick hears a hand-over of an overlay with no menu tests — the hand-over starts the VM's one
/// tick — and the menu's log has no line for the keys it took.
#[test]
fn the_tick_hears_a_hand_over_on_an_overlay_without_menu_tests_and_the_menu_log_has_no_line_for_its_keys() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth({ menus = false })
        T.openPage()
        T.ok(S.every == nil, "no tick before the hand-over")
        T.handOver()
        T.ok(S.every ~= nil, "the hand-over started it")
        T.ok(T.typedAndTicked("Return") == "passed", "through")
        T.ok(#S.ended == 1 and S.ended[1].as == "ended", "the tick delivered it")
        T.ok(T.count("went through to the menu") == 0, T.dump())
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.handOver()
        T.typedAndTicked("Escape")
        T.ok(T.count("went through to the menu") == 0 and #S.ended == 1, T.dump())
        T.runDue()
        T.pageFront()
    "##);
}

/// The end key with another module's overlay at least as specific holding the slot on the plug-in's
/// window: the trip is given up, `taken`, on the turn after the tick saw the key.
#[test]
fn the_end_key_with_another_modules_overlay_on_the_slot_is_outranked() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.handOver()
        local id = T.another(T.O.layer.content)
        T.matching(id, true)
        T.ok(not ov.active and T.hoHeld(), "the other module's overlay holds the slot; the keys still held")
        T.typed("Return")
        T.tick()
        T.ok(#S.giveUps == 0 and T.hoHeld(), "decided on the next turn")
        T.runDue()
        T.ok(not T.hoHeld() and S.giveUps[1] == "taken" and #S.ended == 1 and S.ended[1].as == "taken"
          and #S.endsAsked == 0, tostring(S.giveUps[1]))
        T.ok(T.count("[page] 'Synth': trip 'Next preset' given up — outranked, after 150 ms in waiting") == 1, T.dump())
    "##);
}

/// Every hook of a hand-over runs as a handler, where a read may wait: `onKey`, `ends`, and `onEnd`
/// on its own end, at the page key, after another application's window and after `done` each take a
/// picture without a callback, and none raises (the scripted host raises where it could not wait,
/// and `finish` fails on it).
#[test]
fn every_hand_over_hook_runs_as_a_handler_where_a_read_may_wait() {
    run(r##"
        local S = T.S
        local H = T.host
        S.pictures = {}
        local function picture(who)
          H.screen.snapshot({ region = { 110, 60, 300, 80 } })
          S.pictures[#S.pictures + 1] = who
        end
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.endsFn = function(o, trip, info, answer) picture("ends"); answer(true) end
        S.onEndFn = function(o, trip, how, _, err) picture("onEnd " .. (err and err.code or how)); if how == "ended" then trip:done({}) end end
        T.handOver()
        T.typedAndTicked("Return")
        T.runDue()
        T.pageFront()
        T.handOver()
        T.key()
        T.pageFront()
        T.handOver()
        T.show(T.OTHER)
        T.runDue()
        T.show(T.W, T.IN)
        T.runDue()
        T.openPage()
        S.onEndFn = function(o, trip, how, _, err) picture("onEnd " .. (err and err.code or how)) end
        S.actFn = function(o, trip)
          trip:handOver(T.spec())
          trip:done({})
        end
        T.trip()
        T.land()
        T.runDue()
        T.runDue()
        T.pageFront()
        S.onEndFn = function(o, trip, how, _, err) picture("onEnd " .. (err and err.code or how)); if how == "ended" then trip:done({}) end end
        T.handOver({ kind = "keys", keys = { "PageUp" }, ends = false,
          onKey = function() picture("onKey"); return true end })
        T.typed("PageUp")
        T.runDue()
        T.runDue()
        T.pageFront()
        T.ok(table.concat(S.pictures, ",") == "ends,onEnd ended,onEnd cancelled,onEnd moved_on,onEnd done,"
          .. "onKey,onEnd ended", table.concat(S.pictures, ","))
    "##);
}

/// A hook of the hand-over that raises gives the trip up, `failed`, and `onEnd` is told once:
/// `ends` raising, `onKey` raising, and `onEnd` raising on its own end — told once, then the trip
/// given up.
#[test]
fn a_hand_over_hook_that_raises_gives_the_trip_up_failed_and_on_end_is_told_once() {
    for hook in ["ends", "onKey", "onEnd"] {
        let scenario = r##"
            local S = T.S
            local hook = "@HOOK@"
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            if hook == "ends" then
              S.endsFn = function() error("ends broke") end
              T.handOver()
              T.typedAndTicked("Return")
            elseif hook == "onKey" then
              T.handOver({ kind = "keys", keys = { "PageUp" }, ends = false, onKey = function() error("onKey broke") end })
              T.typed("PageUp")
            else
              S.onEndFn = function(o, trip, how) if how == "ended" then error("onEnd broke") end end
              T.handOver()
              T.typedAndTicked("Return")
            end
            T.ok(not T.hoHeld(), hook .. ": the keys back")
            T.runDue()
            T.runDue()
            T.ok(S.giveUps[1] == "failed", hook .. ": " .. tostring(S.giveUps[1]))
            T.ok(#S.ended == 1 and S.ended[1].as == (hook == "onEnd" and "ended" or "failed"), hook .. ": told once, "
              .. tostring(S.ended[1] and S.ended[1].as))
            T.ok(T.count("[page] 'Synth': " .. hook .. " failed: ") == 1, T.dump())
            T.ok(S.front == T.PAGE, "the page back with the error")
            T.pageFront()
        "##;
        run(&scenario.replace("@HOOK@", hook));
    }
}

/// On a Mac the stay is held through a hand-over and its end: no `identify` is asked again.
#[test]
fn no_identify_and_the_stay_held_through_a_hand_over() {
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.ok(S.identifies == 1, "identified once")
        T.openPage()
        T.handOver()
        T.show(T.MAINWIN)
        T.show(T.W, T.IN)
        T.runDue()
        T.typedAndTicked("Return")
        T.runDue()
        T.pageFront()
        T.ok(#S.ended == 1 and S.identifies == 1, "identify not asked again: " .. S.identifies)
    "##);
}

/// The host's count of a key's quick repeats reaches the module's `onKey`, after the `mods`, as a
/// capture's callback gets it (runtime 0.12): `press`, or nil where the host hands none.
#[test]
fn a_keys_hand_over_hands_on_key_the_press_after_the_mods() {
    both(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.got = {}
        T.handOver({ kind = "keys", keys = { "PageUp", "Return" }, ends = false,
          onKey = function(o, trip, key, mods, press)
            S.got[#S.got + 1] = key .. " " .. (press and tostring(press.count) or "none")
            return key == "Return"
          end })
        T.ok(T.typed("PageUp", nil, { count = 2 }) == "taken", "taken for the module")
        T.ok(T.typed("Return") == "taken", "Return for the module too")
        T.ok(table.concat(S.got, " | ") == "PageUp 2 | Return none", table.concat(S.got, " | "))
        T.runDue()
        T.ok(#S.ended == 1 and S.ended[1].as == "ended" and S.ended[1].info.key == "Return", "ended by onKey")
        T.runDue()
        T.pageFront()
        T.allBack()
    "##);
}
