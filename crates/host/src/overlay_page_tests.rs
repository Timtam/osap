//! An overlay's page (RG1a and RG1b of the VPS Avenger port's plan): the overlay runtime's
//! `O:addPage`, `O:inPlugin` and its trip, `o.away`, `O:pageBack`, `O:pageKey` and `O:pageAgain`,
//! run for real against the scripted host of `overlay_menu_tests.rs`, with the window prelude's real
//! matcher, daw-hosts loaded for real (`overlay_host_panel_tests.rs`), the scripted page of
//! `page_example_tests.rs` (`PAGE_WEBVIEW`) and the specificity arbiter (`SPECIFICITY_ARBITER`), so
//! that a sibling on a dialog layer takes the slot as it does in the host.
//!
//! What the design of 2026-10-12 says is pinned here, with no window. The page key in the plug-in
//! runs the module's `open` — its state and its focus posted, in that order — and only then shows the
//! page beside the plug-in's window; elsewhere, or with a menu open over the overlay, it shows
//! nothing and says why. When the page's window comes to the front the overlay's group goes AWAY:
//! no key of any kind, no scope, no announcement, and its onActivate and onDeactivate wait, paired
//! by whether the module was told — so a page round trip, a trip, and a box that came and went
//! during one are nothing to the module's hooks. Away ends at the plug-in's window, at another
//! application's, or by the bound on a way back, whichever order the triggers run in, with the keys
//! back and the arrival said; a page over the plug-in is hidden then, one beside it is not.
//!
//! A trip brings the plug-in's control forward (on Windows the control's id, on a Mac the window's),
//! arrives through a held context — the keyboard on the DAW's own controls does not keep it out, and
//! nothing leaks to an overlay outside the group — runs the module's `act` as a handler on the next
//! turn, quietly, and brings the page back after `after` posted the result, unless another
//! application's window is in front. Every end is here: another application, the page key in every
//! stage, the page closed, the window gone, the bound of 8 seconds, `trip:inFront()` answering
//! false, another module's overlay holding the slot, a hook that raised; one trip at a time; no
//! `identify` across trips in a Mac's stay; a zoom step inside a trip. And RG1b: a menu the next
//! click opens, then its close; a dialog, then the plug-in again; a sibling dialog overlay of the
//! module keeping the trip.
//!
//! **Every event through the real mailbox**, as the harness has it, and every scenario ends on the
//! harness's checks (`finish`) and on the hooks of every overlay paired: onActivate and
//! onDeactivate alternate, starting with onActivate (`T.pairsAll`).

use super::overlay_host_panel_tests::{run_with_helpers, SPECIFICITY_ARBITER};
use super::page_example_tests::PAGE_WEBVIEW;

/// The helpers, on top of the host-panel ones, the scripted page and the specificity arbiter.
const PG: &str = r##"
local S = T.S
local H = T.host
T.KEY = "Ctrl+Shift+Alt+F1"
T.modules("modules/overlay-runtime/")
local mac = rawget(H.os, "current") == "macos"

-- What the module and the runtime did, in order: the scripted page's own calls ("show", "hide",
-- "post <kind>"), and here clicks and focus requests.
do
  local click = H.input.click
  rawset(H.input, "click", function(x, y, o)
    S.order[#S.order + 1] = "click"
    return click(x, y, o)
  end)
  local focus = H.window.focus
  rawset(H.window, "focus", function(id)
    S.order[#S.order + 1] = "focus " .. tostring(id)
    return focus(id)
  end)
  -- A chord another program holds: the host's registration raises for it.
  local register = H.hotkey.register
  rawset(H.hotkey, "register", function(spec, fn)
    if S.registerRaises and S.registerRaises[spec] then error(S.registerRaises[spec], 0) end
    return register(spec, fn)
  end)
end
S.owns, S.ownsAsked = true, {}
rawset(H.window, "ownsPoint", function(id, x, y)
  S.ownsAsked[#S.ownsAsked + 1] = { id, x, y }
  if type(S.owns) == "function" then return S.owns(id, x, y) end
  return S.owns
end)
-- host.keys.check's reasons for a spec (S.refuse[spec]), none otherwise.
S.refuse = {}
rawset(H.keys, "check", function(spec)
  local why = S.refuse[spec]
  return { reasons = why and { why } or {} }
end)

-- The plug-in. On Windows a JUCE child control in REAPER's FX window; on a Mac Logic's plug-in
-- window "Inst 1", whose panel daw-hosts' entry gives, and Logic's main window.
T.PLUG = { id = 77, class = "JUCE_PlugIn", bounds = { x = 100, y = 50, w = 800, h = 600 },
  client = { x = 100, y = 50, w = 800, h = 600 } }
T.LOGIC_MAIN = { id = 20, title = "Untitled - Tracks", class = "AXWindow/AXStandardWindow/", app = T.MAC_LOGIC,
  bounds = { x = 0, y = 25, w = 1440, h = 875 }, client = { x = 0, y = 25, w = 1440, h = 875 } }
if mac then
  T.W, T.IN = T.LOGIC, { T.LOGIC }
  T.HEADER = { id = 25, class = "AXButton//", bounds = { x = 400, y = 45, w = 60, h = 20 } }
  T.CHROME = { T.HEADER, T.LOGIC }
  T.MAINWIN = T.LOGIC_MAIN
  S.listed = { T.LOGIC_MAIN, T.LOGIC, T.OTHER }
  T.HOSTS = T.daw.all
  T.HELD = T.LOGIC.id
else
  T.W, T.IN, T.CHROME = T.FX, { T.PLUG, T.WRAP, T.FX }, { T.LIST, T.FX }
  T.MAINWIN = T.MAIN
  S.controls = { T.LIST, T.WRAP, T.PLUG }
  T.HOSTS = { T.REAPER_WIN }
  T.HELD = T.PLUG.id
end
-- An accepted host.window.focus lands on W with the keyboard in the plug-in, unless the scenario
-- says where (S.landTo, S.landChain).
S.focusLands = function() S.front, S.chain = S.landTo or T.W, S.landChain or T.IN end

-- The overlay's hooks, in order: { label, "on" | "off" }.
S.hooks = {}
function T.hooked(ov)
  ov:onActivate(function(o) S.hooks[#S.hooks + 1] = { o.label, "on" } end)
  ov:onDeactivate(function(o) S.hooks[#S.hooks + 1] = { o.label, "off" } end)
end
function T.hookCount(label, kind)
  local n = 0
  for _, h in ipairs(S.hooks) do
    if h[1] == label and h[2] == kind then n += 1 end
  end
  return n
end
-- onActivate and onDeactivate of every overlay alternate, starting with onActivate.
function T.pairsAll()
  local want = {}
  for _, h in ipairs(S.hooks) do
    local w = want[h[1]] or "on"
    T.ok(h[2] == w, ("the hooks of '%s' are not paired: %s where %s was due"):format(h[1], h[2], w))
    want[h[1]] = w == "on" and "off" or "on"
  end
end

-- The page's description and hooks: `over` replaces fields (false removes one).
S.shown, S.backs = {}, {}
function T.pageSpec(over)
  local spec = {
    describe = function(on)
      return H.gui.webview({ file = "ui/index.html", title = "Synth page", onMessage = on.message,
        onReady = on.ready, onClose = on.close })
    end,
    key = T.KEY,
    open = function(o)
      o.page:post({ kind = "state" })
      o.page:post({ kind = "focus" })
    end,
    onMessage = function(o, m) if S.onMsg then return S.onMsg(o, m) end end,
    onShown = function(o, ok, why) S.shown[#S.shown + 1] = { ok = ok, why = why } end,
    onBack = function(o, ok, why) S.backs[#S.backs + 1] = { ok = ok, why = why } end,
  }
  for k, v in pairs(over or {}) do
    if v == false then spec[k] = nil else spec[k] = v end
  end
  return spec
end

-- The embedded binding: on Windows the JUCE control by its class, on a Mac the DAW's panel; its
-- identify counted.
S.identifies = 0
function T.bindingSpec(over)
  local spec = { hosts = T.HOSTS, control = { windows = "^JUCE_PlugIn" },
    identify = function() S.identifies += 1; return true end }
  for k, v in pairs(over or {}) do spec[k] = v end
  return spec
end

-- The synth's overlay: two buttons, its hooks recorded, its page (unless `page = false`), bound on
-- the synth's slot at the base layer with newWindow as its menu test (`menus`, false for none). Its
-- first evaluation runs before this returns.
function T.synth(opts)
  opts = opts or {}
  local ov = T.O.new(opts.label or "Synth")
  ov:addCustomButton({ label = "Preset", hotkey = "Alt+P", onActivate = function() S.acted = (S.acted or 0) + 1 end })
  ov:addCustomButton({ label = "Next", onActivate = function() end })
  T.hooked(ov)
  if opts.setup then opts.setup(ov) end
  if opts.page ~= false then T.page = ov:addPage(T.pageSpec(opts.page)) end
  local menus = opts.menus
  if menus == nil then menus = { T.O.menuTests.newWindow } end
  ov:bind(opts.bound or T.O.embedded(T.bindingSpec(opts.binding)), { slot = opts.slot or "com.platform.synth",
    specificity = opts.specificity or T.O.layer.base, menus = menus or nil, pollMatch = opts.pollMatch,
    pollWhen = opts.pollWhen })
  T.runDue()
  return ov
end

-- The page key, as the host delivers it: the epoch turns over, and its callback runs as a handler.
function T.key(spec)
  local fn = S.hotkeyFns[spec or T.KEY]
  T.ok(fn ~= nil, "no page key " .. tostring(spec or T.KEY))
  S.epoch += 1
  return T.call("hotkey", fn)
end
-- The page's message, its load, its window gone ("user": the host hid it and gave the front back;
-- "failed": the page is gone).
function T.msg(m) return T.call("page message", S.pageOpts.onMessage, m) end
function T.ready()
  S.pg.ready = true
  return T.call("page ready", S.pageOpts.onReady)
end
function T.closed(why)
  S.pg.shown = false
  if why == "failed" then
    S.pg.ready = false
    S.page.window = nil
  end
  return T.call("page close", S.pageOpts.onClose, why)
end
-- The page's foreground event: its show put it in front already.
function T.pageFront()
  T.ok(S.front == T.PAGE, "the page is not in front: " .. tostring(S.front and S.front.title))
  T.event()
end
-- The page shown at the key from the plug-in, loaded already, and its foreground event: away.
function T.openPage()
  S.pg.ready = true
  T.key()
  T.pageFront()
end

-- A trip from a page message: `over` replaces fields of the spec (false removes one). The default
-- act clicks once inside the plug-in and is done; S.actFn replaces it.
S.giveUps, S.afters = {}, {}
function T.tripSpec(over)
  local spec = {
    label = "Next preset",
    act = function(o, trip)
      S.order[#S.order + 1] = "act"
      S.acts = (S.acts or 0) + 1
      S.trip, S.actOverlay = trip, o
      if S.actFn then return S.actFn(o, trip) end
      local x, y = o:toScreen(10, 10)
      H.input.click(x, y)
      trip:done({ name = "Pad" })
    end,
    after = function(o, result)
      S.afters[#S.afters + 1] = result
      o.page:post({ kind = "state" })
      return S.afterWords
    end,
    onGiveUp = function(o, why)
      S.giveUps[#S.giveUps + 1] = why
      o.page:post({ kind = "error", text = why })
      return S.giveUpWords
    end,
  }
  for k, v in pairs(over or {}) do
    if v == false then spec[k] = nil else spec[k] = v end
  end
  return spec
end
function T.trip(over)
  S.started = nil
  S.onMsg = function(o, m) S.started = table.pack(o:inPlugin(T.tripSpec(over))) end
  T.msg({ action = "next" })
  T.ok(S.started ~= nil, "the message did not reach the module")
  return S.started[1], S.started[2]
end

-- The module disabled, as the host disables it: its pages hidden, its one-shot timers and pending
-- reads dropped, its overlays out of their slots. Enabled again: nothing happens until the next event.
function T.disable()
  S.pg.shown = false
  S.after, S.asyncs, S.reads = {}, {}, {}
  for _, c in ipairs(T.claims) do
    c.matching = false
    if c.active then
      c.active = false
      T.hostCall(c.off)
    end
  end
end

-- A claim of another module on the synth's slot at `spec`, made to match or not.
function T.another(spec)
  local id = H.arbiter.register("com.platform.synth", spec, function() end, function() end)
  T.claims[id].module = "com.platform.other"
  return id
end
function T.matching(id, m) H.arbiter.setMatching("com.platform.synth", id, m) end

-- What was said last, and whether it interrupted; S.order from `from` on.
function T.last() return S.speech[#S.speech] and S.speech[#S.speech].text or "" end
function T.lastInterrupts() return S.speech[#S.speech] and S.speech[#S.speech].interrupt end
function T.since(from)
  local out = {}
  for i = from + 1, #S.order do out[#out + 1] = S.order[i] end
  return table.concat(out, ",")
end
function T.saidSince(from)
  local out = {}
  for i = from + 1, #S.speech do out[#out + 1] = S.speech[i].text end
  return table.concat(out, " | ")
end
function T.posts(kind)
  local n = 0
  for _, p in ipairs(S.pg.posts) do
    if p.kind == kind then n += 1 end
  end
  return n
end
function T.holdsNothing()
  return next(S.hot) == nil and next(S.holding) == nil
end
-- Whether the module holds nothing but its page key: no hotkey, no capture.
function T.onlyPageKey()
  local n = 0
  for _, spec in pairs(S.hot) do
    if spec ~= T.KEY then return false end
    n += 1
  end
  return n == 1 and next(S.holding) == nil
end
-- An assertion whose failure prints the log, everything said and the order of calls.
function T.ok(cond, what)
  if cond then return end
  print(T.dump())
  local said = {}
  for _, s in ipairs(S.speech) do said[#said + 1] = s.text end
  print("said: " .. table.concat(said, " | "))
  print("order: " .. table.concat(S.order, ","))
  error(tostring(what or "assertion failed"), 2)
end
"##;

/// A fresh VM with the scripted host, the runtime, the prelude's matcher, daw-hosts, the scripted
/// page, the specificity arbiter and the helpers above; the host plays `os`. The scenario ends on
/// the harness's checks and on every overlay's hooks paired.
fn run_on(os: &str, scenario: &str) {
    let scenario = format!("{scenario}\nT.pairsAll()\n");
    run_with_helpers(
        os,
        &[("the scripted page", PAGE_WEBVIEW), ("specificity arbiter", SPECIFICITY_ARBITER), ("page helpers", PG)],
        &scenario,
    )
}

fn run(scenario: &str) {
    run_on("windows", scenario)
}

fn run_mac(scenario: &str) {
    run_on("macos", scenario)
}

// ---------------------------------------------------------------------------------------------
// RG1a: the page and away.
// ---------------------------------------------------------------------------------------------

/// The page key in the plug-in: the module's `open` posts the state, then the focus, and only then
/// is the page shown, with the keyboard, beside the FX window's bounds; `onShown(true)`.
#[test]
fn the_page_key_runs_open_then_shows_beside_the_plugin_and_the_order_is_state_focus_show() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.ok(ov.active and ov.page == T.page, "in front, its page on it")
        S.pg.ready = true
        local from = #S.order
        T.key()
        T.ok(T.since(from) == "post state,post focus,show", T.since(from))
        local a = S.pg.shows[1].avoid
        T.ok(a.x == 0 and a.y == 0 and a.w == 1000 and a.h == 700, "beside the FX window")
        T.ok(S.pg.shows[1].focus == true, "with the keyboard")
        T.ok(#S.shown == 1 and S.shown[1].ok == true and S.shown[1].why == nil, "onShown(true)")
        T.ok(T.count("[page] 'Synth': shown at the key beside window id 1: true") == 1, T.dump())
    "##);
}

/// Where the overlay is not in front the key shows nothing and says where it opens: the runtime's
/// words, or the module's `notHere`, interrupting, and logged with what is in front.
#[test]
fn the_page_key_refuses_where_the_overlay_is_not_in_front_and_says_not_here() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.show(T.OTHER)
        T.key()
        T.ok(#S.pg.shows == 0, "nothing shown")
        T.ok(T.last() == "Synth is not in front; its page opens from there." and T.lastInterrupts(), T.last())
        T.ok(T.count("[page] 'Synth': the page key with the overlay not in front — in front: 'Browser'") == 1, T.dump())
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.CHROME)
        T.synth({ page = { notHere = "The Synth window is not in front." } })
        T.key()
        T.ok(#S.pg.shows == 0 and T.last() == "The Synth window is not in front.", T.last())
    "##);
}

/// A menu open over the overlay — counted open by a test, or held over its popup's window — refuses
/// the key: the place would have been the popup.
#[test]
fn the_page_key_refuses_while_a_menu_is_open_over_the_overlay() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth({ menus = { { name = "seen", cheap = true,
          test = function(_, answer) answer(S.menuSeen == true) end } } })
        S.menuSeen = true
        T.tick()
        T.ok(ov:menuOpen(), "a menu counts open")
        T.key()
        T.ok(#S.pg.shows == 0, "nothing shown")
        T.ok(T.last() == "Synth: a menu is open; close it, then press the key again." and T.lastInterrupts(), T.last())
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth({ setup = function(o)
          o:addCustomButton({ label = "Menu", opensMenu = true, onActivate = function() end })
        end })
        ov.focus = 3
        S.epoch += 1
        T.call("key", S.captured["Return"])
        S.windows[#S.windows + 1] = { id = 900, layer = 101, class = "JUCE_Popup", x = 300, y = 200, w = 150, h = 200 }
        T.show(T.POPUP)
        T.ok(ov:_heldOnMenu(), "held over its popup: " .. T.dump())
        T.key()
        T.ok(#S.pg.shows == 0 and T.last() == "Synth: a menu is open; close it, then press the key again.", T.last())
    "##);
}

/// `open` that waits at a read while the user goes elsewhere: the place no longer holds after it,
/// so nothing is shown, and the log says why.
#[test]
fn open_that_waited_while_the_user_moved_shows_nothing() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth({ page = { open = function(o)
          T.waitPoint("open's read")
          o.page:post({ kind = "state" })
        end } })
        S.pg.ready = true
        T.key()
        T.ok(T.waiting("open's read"), "open waits")
        T.show(T.OTHER)
        T.queued()
        T.release("open's read", nil)
        T.queued()
        T.ok(#S.pg.shows == 0, "nothing shown")
        T.ok(T.count("[page] 'Synth': not shown — the overlay is") == 1, T.dump())
    "##);
}

/// A page not ready when the key is pressed is shown from its `onReady` — with `open` asked afresh —
/// only while the overlay is still on that control; after the user moved to another plug-in first,
/// it is not shown.
#[test]
fn a_page_not_ready_is_shown_from_ready_only_while_the_overlay_is_still_on_that_control() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.key()
        T.ok(#S.pg.shows == 1 and T.last() == "Building the page.", T.last())
        local from = #S.order
        T.ready()
        T.ok(T.since(from) == "post state,post focus,show", "open afresh, then the show: " .. T.since(from))
        T.ok(S.front == T.PAGE and #S.shown == 1 and S.shown[1].ok, "shown once it has loaded")
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.key()
        -- Another plug-in in the same window: another control.
        local other = { id = 78, class = "JUCE_PlugIn", bounds = T.PLUG.bounds, client = T.PLUG.client }
        S.controls = { T.LIST, T.WRAP, other }
        T.show(T.W, { other, T.WRAP, T.FX })
        T.ready()
        T.ok(#S.pg.shows == 1, "not shown over another plug-in")
        T.ok(T.count("[page] 'Synth': the page has loaded; not shown — the overlay is not on the control it was asked from") == 1, T.dump())
    "##);
}

/// The page is prepared on the turn after its overlay first comes to the front, and not again once
/// it has loaded, nor after a prepare that failed — the key builds it then; after `close("failed")`
/// the next arrival prepares again.
#[test]
fn the_page_is_prepared_at_the_first_arrival_and_not_again_after_a_prepare_failed() {
    run(r##"
        local S = T.S
        T.show(T.OTHER)
        T.synth()
        T.ok(S.pg.prepares == 0, "not before the plug-in came forward")
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(S.pg.prepares == 1, "prepared at the first arrival")
        T.ok(T.count("[page] 'Synth': prepared: building (the call took") == 1, T.dump())
        T.ready()
        T.show(T.OTHER)
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(S.pg.prepares == 1, "not again once it has loaded")
        T.closed("failed")
        T.show(T.OTHER)
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(S.pg.prepares == 2, "again after the page failed")
        -- A prepare that answers a failure is not asked again from an arrival.
        T.closed("failed")
        S.pg.prepareAnswers = { { false, "could not be built" } }
        T.show(T.OTHER)
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(S.pg.prepares == 3, "prepared once more")
        T.ok(T.count("not prepared (could not be built, the call took") == 1, T.dump())
        T.show(T.OTHER)
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(S.pg.prepares == 3, "not again from an arrival after a failure")
    "##);
}

/// The page's foreground event takes the group away, quietly: `o.away`, no onDeactivate, no hotkey
/// or capture held, the scope not pinned — and the page key still registered.
#[test]
fn the_pages_foreground_event_makes_the_group_away_quietly() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.ok(T.hookCount("Synth", "on") == 1 and not T.holdsNothing(), "in front, holding its keys")
        T.openPage()
        T.ok(ov.away and not ov.active, "away")
        T.ok(T.hookCount("Synth", "off") == 0, "no onDeactivate")
        local held = {}
        for _, spec in pairs(S.hot) do held[#held + 1] = spec end
        T.ok(#held == 1 and held[1] == T.KEY, "only the page key: " .. table.concat(held, " "))
        T.ok(next(S.holding) == nil, "no capture")
        T.ok(S.scopedTo == false, "the scope is not pinned")
        T.ok(T.count("[page] 'Synth': away — the page in front, holding window id 1 'FX: Track 1 \"test\"' of pid 4242, control 'JUCE_PlugIn' id 77") == 1, T.dump())
    "##);
}

/// Away holds no key of any kind: an overlay with tab hotkeys, a slider's arrows, Space and Return,
/// `typingWhen` and calibration on, arrived quietly in a trip, holds nothing — and a menu closing
/// over it takes nothing back.
#[test]
fn away_holds_no_key_of_any_kind() {
    run(r##"
        local S = T.S
        S.calibrating = true
        T.show(T.W, T.IN)
        local ov = T.synth({ setup = function(o)
          o:addTabControl({ tabs = { { label = "Osc", hotkey = "Alt+1" }, { label = "Filter", hotkey = "Alt+2" } } })
          o:addSlider({ label = "Cutoff", region = { 10, 10, 200, 30 }, thumbImage = "C:/t.png", from = 10, to = 200 })
          o:typingWhen(function() return false end)
        end, menus = { { name = "seen", cheap = true, test = function(_, answer) answer(S.menuSeen == true) end } } })
        T.ok(not T.holdsNothing(), "keys while in front")
        T.openPage()
        T.trip({ act = function(o, trip) S.trip = trip end })
        T.land()
        T.runDue()
        T.ok(ov.active and ov.away and S.trip ~= nil, "in front quietly, in the trip")
        local held = {}
        for _, spec in pairs(S.hot) do held[#held + 1] = spec end
        T.ok(#held == 1 and held[1] == T.KEY and next(S.holding) == nil, "nothing but the page key: " .. table.concat(held, " "))
        S.menuSeen = true
        T.tick(2)
        S.menuSeen = false
        T.tick(3)
        T.ok(#held == 1 and next(S.holding) == nil and T.hotkeys() == 1, "a menu closing takes nothing back")
        S.trip:done({})
        T.runDue()
    "##);
}

/// No foreground event of the page came (it has been seen not to): the page's first message
/// begins away before the module's handler runs, and the trip it starts holds the place.
#[test]
fn a_missed_page_event_is_recovered_at_the_pages_message() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        S.pg.ready = true
        T.key()
        T.ok(S.front == T.PAGE and ov.active and not ov.away, "the page in front, no event of it")
        local ok, why = T.trip()
        T.ok(ov.away, "away began at the message")
        T.ok(ok == true, "the trip starts: " .. tostring(why))
        T.ok(T.count("[page] 'Synth': the page has the keyboard, and no foreground event of it came") == 1, T.dump())
        T.ok(S.focusCalls[#S.focusCalls] == T.HELD, "the plug-in's control asked to the front")
        T.land()
        T.runDue()
        T.runDue()
        T.ok(S.acts == 1 and #S.afters == 1 and S.front == T.PAGE, "done, and the page back")
    "##);
}

/// A show Windows declines keeps the place asked for: granted a moment later — the page's window
/// coming to the front all the same — the group goes away as for any show. Declined, the module's
/// `onShown(false, why)` hears it; without one, the runtime says it in its own words.
#[test]
fn a_show_declined_and_granted_later_still_holds_the_place() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        S.pg.ready = true
        S.pg.answers = { { false, "declined by the system" } }
        T.key()
        T.ok(#S.shown == 1 and S.shown[1].ok == false and S.shown[1].why == "declined by the system", "onShown(false)")
        T.ok(S.front == T.W and ov.active and not ov.away, "the plug-in keeps the keyboard")
        S.front, S.chain = T.PAGE, { T.PAGE }
        T.event()
        T.ok(ov.away and not ov.active, "away at the late grant")
        T.ok(T.hookCount("Synth", "off") == 0, "quietly")
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth({ page = { onShown = false } })
        S.pg.ready = true
        S.pg.answers = { { false, "declined by the system" } }
        T.key()
        T.ok(T.last() == "The page did not come to the front; if it does not come by itself, the page key brings it."
          and T.lastInterrupts(), T.last())
        S.pg.answers = { { false, "could not be built" } }
        T.key()
        T.ok(T.last() == "The page did not come to the front: could not be built.", T.last())
        -- Another window lets the declined place go.
        S.pg.answers = { { false, "declined by the system" } }
        T.key()
        T.show(T.OTHER)
        T.ok(T.count("[page] 'Synth': the place asked for is let go — in front: 'Browser'") == 1, T.dump())
    "##);
}

/// From the page to another application: away ends there, the module hears onDeactivate once, at
/// the settle, and the page stays shown where it is.
#[test]
fn going_to_another_application_from_the_page_ends_away_with_ondeactivate_once_and_the_page_stays_shown() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.show(T.OTHER)
        T.ok(not ov.away, "away ends")
        T.ok(T.hookCount("Synth", "off") == 0, "the hook waits for the settle")
        T.runDue()
        T.ok(T.hookCount("Synth", "off") == 1, "onDeactivate, once")
        T.ok(S.pg.shown and S.pg.hides == 0, "the page stays shown")
        T.ok(T.count("[page] 'Synth': away ends — another application's window is in front") == 1, T.dump())
        T.show(T.W, T.IN)
        T.ok(T.hookCount("Synth", "on") == 2, "back in the plug-in: onActivate")
    "##);
}

/// Back in the plug-in by Alt+Tab: away ends, the keys come back and the arrival is said; a page
/// over the plug-in, shown and not in front, is hidden, and one beside it is left shown.
#[test]
fn coming_back_by_alt_tab_ends_away_and_a_page_over_the_plugin_is_hidden_and_one_beside_is_not() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.run(400)
        T.openPage()
        T.show(T.W, T.IN)
        T.ok(ov.active and not ov.away and T.holds("Return"), "in front with its keys")
        T.ok(S.pg.shown and S.pg.hides == 0, "a page beside the plug-in is left shown")
        local from = #S.speech
        T.run(450)
        T.ok(T.saidSince(from) == "Preset, button", "the arrival is said: " .. T.saidSince(from))
        T.ok(T.hookCount("Synth", "on") == 1 and T.hookCount("Synth", "off") == 0, "no hook: one stay to the module")
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        S.pg.ready = true
        S.pg.answers = { { true, "over the rectangle to avoid" } }
        T.key()
        T.ok(S.shown[1].ok and S.shown[1].why == "over the rectangle to avoid", "onShown(true, over)")
        T.pageFront()
        T.show(T.W, T.IN)
        T.ok(ov.active and S.pg.hides == 1 and not S.pg.shown, "a page over the plug-in is hidden")
        T.ok(T.count("[page] 'Synth': the page was over the plug-in's window; hidden as the user is back in it") == 1, T.dump())
    "##);
}

/// The way back — the page key in the page — ends at the plug-in's window, with the keys back, the
/// arrival said, and no second onActivate, whichever trigger runs first: the page's (it was added
/// before the overlay was bound) or the overlay's (as a sibling bound before O:addPage has it —
/// played here by putting the page's trigger last).
#[test]
fn the_return_ends_at_the_plugin_window_announced_with_keys_and_no_second_onactivate() {
    for page_last in [false, true] {
        let scenario = r##"
            local S = T.S
            T.show(T.W, T.IN)
            local ov = T.synth()
            T.run(400)
            if @PAGE_LAST@ then
              -- The page's trigger and focus callback were registered first, at addPage.
              local t = S.triggers
              S.triggers = { t[3], t[4], t[1], t[2] }
            end
            T.openPage()
            local from = #S.order
            T.key()
            T.ok(T.since(from) == "focus 77,hide", "the plug-in's control first, then the page hidden: " .. T.since(from))
            T.ok(#S.backs == 1 and S.backs[1].ok == true, "onBack(true)")
            T.ok(ov.away, "away until the plug-in's window comes")
            T.land()
            T.ok(ov.active and not ov.away and T.holds("Return"), "in front with its keys")
            local said = #S.speech
            T.run(450)
            T.ok(T.saidSince(said) == "Preset, button", "the arrival said once: " .. T.saidSince(said))
            T.ok(T.hookCount("Synth", "on") == 1 and T.hookCount("Synth", "off") == 0, "no second onActivate")
            T.ok(T.count("[page] 'Synth': away ends — the plug-in's window is in front") == 1, T.dump())
        "##;
        run(&scenario.replace("@PAGE_LAST@", if page_last { "true" } else { "false" }));
    }
}

/// The way back with the keyboard on the DAW's own controls: away ends, the overlay is not in front,
/// and the module hears onDeactivate at the settle — then onActivate when the keyboard lands in the
/// plug-in.
#[test]
fn the_return_with_the_keyboard_on_the_daws_chrome_pairs_the_hooks() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.key()
        S.landChain = T.CHROME
        T.land()
        T.ok(not ov.active and not ov.away, "away ended, not in front")
        T.runDue()
        T.ok(T.hookCount("Synth", "off") == 1, "onDeactivate at the settle")
        T.show(T.W, T.IN)
        T.ok(ov.active and T.hookCount("Synth", "on") == 2, "onActivate as the keyboard lands")
    "##);
}

/// Logic on the way back may bring its main window forward first: the return waits for the plug-in's
/// window, and ends there with the stay kept — no identify. When the plug-in's window does not come
/// within 8 seconds, away ends where the user is, and so does the stay: the next arrival asks
/// identify again.
#[test]
fn back_on_logic_tolerates_the_main_window_first_and_ends_by_its_bound_otherwise() {
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.ok(ov.active and S.identifies == 1, "in front, identified once: " .. T.dump())
        T.openPage()
        S.landTo, S.landChain = T.LOGIC_MAIN, { T.LOGIC_MAIN }
        T.key()
        T.ok(S.focusCalls[#S.focusCalls] == 21, "the plug-in's window asked to the front")
        T.land()
        T.ok(ov.away, "Logic's main window first: still on the way")
        T.ok(T.count("[page] 'Synth': the way back: 'Untitled - Tracks' (Logic Pro X) came first — waiting for the plug-in's window") == 1, T.dump())
        T.show(T.LOGIC, T.IN)
        T.ok(ov.active and not ov.away, "back")
        T.ok(S.identifies == 1, "no identify: the stay was held")
    "##);
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        S.landTo, S.landChain = T.LOGIC_MAIN, { T.LOGIC_MAIN }
        T.key()
        T.land()
        T.run(8100)
        T.ok(not ov.away, "the bound ends away")
        T.ok(T.count("[page] 'Synth': away ends — the way back did not reach the plug-in's window within 8000 ms") == 1, T.dump())
        T.show(T.LOGIC, T.IN)
        T.ok(ov.active and S.identifies == 2, "a new stay: identify asked again: " .. S.identifies)
    "##);
}

/// The way back to Logic with the keyboard on nothing anybody can read — what Logic answered on
/// the tester's way back of 2026-10-14, the plug-in's own view holding the keyboard: away ends at
/// the plug-in's window, and the overlay is back in front in the stay it left, with no identify,
/// the arrival said and its hooks paired.
#[test]
fn the_way_back_to_logic_with_the_keyboard_on_nothing_comes_back_in_the_stay() {
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.ok(ov.active and S.identifies == 1, T.dump())
        T.run(400)
        T.openPage()
        S.landChain = {}
        T.key()
        T.land()
        T.ok(ov.active and not ov.away, "back in front: " .. T.dump())
        T.ok(S.identifies == 1, "no identify: the stay went on: " .. S.identifies)
        local said = #S.speech
        T.run(450)
        T.ok(T.saidSince(said) == "Preset, button", "the arrival said once: " .. T.saidSince(said))
        T.ok(T.hookCount("Synth", "on") == 1 and T.hookCount("Synth", "off") == 0, "no second onActivate")
    "##);
}

/// A trip that lands with the keyboard on nothing anybody can read acts and comes back, as one that
/// lands on the DAW's own controls does: the held context does not ask where the keyboard is.
#[test]
fn a_trip_that_lands_with_the_keyboard_on_nothing_acts_and_comes_back() {
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.trip()
        S.landChain = {}
        T.land()
        T.runDue()
        T.ok(S.acts == 1, "act ran with the keyboard on nothing: " .. T.dump())
        T.runDue()
        T.ok(S.front == T.PAGE and #S.giveUps == 0, "and the page came back")
    "##);
}

/// The slot taken by another module's claim, more specific: the overlay's `[deactivate]` line names
/// the claim that was chosen, as the arbiter lists it — the active one, which the host marks before
/// it calls the old one off — and not the reason the overlay last left for, before it came back.
#[test]
fn another_claim_chosen_on_the_slot_is_named_when_the_overlay_leaves() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.ok(ov.active, T.dump())
        T.show(T.OTHER)
        T.ok(not ov.active and T.count("[deactivate] 'Synth' — in front now: 'Browser' (chrome.exe); ") == 1, T.dump())
        local left = T.count("; 'Browser' is not a")
        T.ok(left == 1, T.dump())
        T.show(T.W, T.IN)
        T.ok(ov.active, T.dump())
        local id = T.another(T.O.layer.dialog)
        T.matching(id, true)
        T.ok(not ov.active, "outranked")
        T.ok(T.count("[deactivate] 'Synth' — in front now: 'FX: Track 1 \"test\"' (reaper.exe); another claim "
          .. "on slot 'com.platform.synth' was chosen: com.platform.other at specificity 40") == 1, T.dump())
        T.ok(T.count("; 'Browser' is not a") == left, "the reason before it came back, said again: " .. T.dump())
        T.matching(id, false)
        T.ok(ov.active, "back when it lets go")
    "##);
}

/// The page's close box (or Alt+F4, or Command-W) is a way back: the plug-in's control asked to the
/// front, and away ends at the plug-in's window with the keys back.
#[test]
fn the_close_box_is_a_return() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth({ page = { onClose = function(o, why) S.closedWhy = why end } })
        T.openPage()
        T.closed("user")
        T.ok(S.focusCalls[#S.focusCalls] == T.HELD and S.closedWhy == "user", "the way back, then the module's onClose")
        T.ok(ov.away, "until the plug-in's window comes")
        T.land()
        T.ok(ov.active and not ov.away and T.holds("Return"), "back, with its keys")
    "##);
}

/// The module disabled while away — a trip waiting for the plug-in, a trip acting with a picture
/// out, or the page in front — and enabled again: the next event ends the trip ("the page was
/// closed") and away, the keys come back in the plug-in, and the hooks stay paired.
#[test]
fn a_disable_while_away_and_enable_recovers_at_the_next_event() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.trip()
        T.disable()
        T.show(T.W, T.IN)
        T.ok(#S.giveUps == 1 and S.giveUps[1] == "page closed", "the trip ends: " .. tostring(S.giveUps[1]))
        T.ok(T.last() == "Next preset: the page was closed; nothing more was done.", T.last())
        T.ok(ov.active and not ov.away and T.holds("Return"), "keys back in the plug-in")
        T.runDue()
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          T.host.ocr.recognize({ 100, 50, 300, 80 }, function() S.answered = true end)
        end
        T.trip()
        T.land()
        T.runDue()
        T.ok(S.acts == 1 and #S.reads == 1, "acting, a read out")
        T.disable()
        T.show(T.W, T.IN)
        T.ok(#S.giveUps == 1 and S.giveUps[1] == "page closed", "the trip ends")
        T.ok(ov.active and not ov.away and T.holds("Return"), "keys back")
        T.runDue()
        T.ok(S.answered == nil, "the dropped read never answers")
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.disable()
        T.show(T.OTHER)
        T.ok(not ov.away, "away ends at the next event")
        T.runDue()
        T.ok(T.hookCount("Synth", "off") == 1, "the deferred onDeactivate at the settle")
        T.show(T.W, T.IN)
        T.ok(ov.active and T.hookCount("Synth", "on") == 2, "and onActivate when it is back")
    "##);
}

/// onDeactivate is not called while away — the page's overlay let go for a sibling box during a
/// trip — and the box, still up when away ends, gets its onActivate at the settle, while the page's
/// overlay, no longer in front, gets its onDeactivate.
#[test]
fn ondeactivate_is_not_called_while_away_and_a_sibling_that_came_during_a_trip_gets_its_onactivate_at_the_settle() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        local box = T.O.new("Box")
        box:addCustomButton({ label = "OK", onActivate = function() end })
        box:gate(function() return S.boxUp == true end)
        T.hooked(box)
        box:bind(T.O.embedded(T.bindingSpec()), { slot = "com.platform.synth", specificity = T.O.layer.dialog,
          pollMatch = 500, pollWhen = function() return true end })
        T.runDue()
        T.openPage()
        T.ok(box.away, "the box is of the group")
        S.actFn = function(o, trip) S.trip = trip end
        T.trip()
        T.land()
        T.runDue()
        T.ok(ov.active and ov.away, "arrived quietly")
        S.boxUp = true
        T.poll(500)
        T.ok(box.active and not ov.active, "the box took the slot")
        T.ok(T.hookCount("Synth", "off") == 0 and T.hookCount("Box", "on") == 0, "no hook while away")
        T.ok(S.trip.overlay == box, "trip.overlay names the box")
        S.trip:done({})
        T.runDue()
        T.pageFront()
        T.key()
        T.land()
        T.ok(box.active and not box.away, "the box is up when away ends")
        T.runDue()
        T.ok(T.hookCount("Box", "on") == 1, "the box's onActivate at the settle")
        T.ok(T.hookCount("Synth", "off") == 1, "the page's overlay is not in front: onDeactivate")
    "##);
}

// ---------------------------------------------------------------------------------------------
// RG1a: the trip.
// ---------------------------------------------------------------------------------------------

/// A trip from a page message: the plug-in's control asked to the front, and once its window's
/// event has come, `act` runs on the next turn with the overlay active — quietly: nothing said, no
/// onActivate, no key held. A picture with a callback, a click in its answer, `done`; `after` posts
/// the state, and the page comes back; its event leaves the group away.
#[test]
fn a_trip_clicks_and_reads_with_the_overlay_active_and_says_nothing_on_arrival() {
    run(r##"
        local S = T.S
        local H = T.host
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.run(400)
        T.openPage()
        local said = #S.speech
        S.actFn = function(o, trip)
          T.ok(o == ov and o.active and o.away, "act with the overlay active, away")
          T.ok(T.onlyPageKey(), "no key of the overlay's held")
          H.screen.snapshot({ region = { 110, 60, 300, 80 } }, function(before)
            if not trip:inFront() then return end
            local x, y = o:toScreen(10, 10)
            H.input.click(x, y)
            trip:done({ name = "Pad" })
          end)
        end
        local from = #S.order
        local ok = T.trip()
        T.ok(ok == true, "started")
        T.land()
        T.ok(S.acts == nil, "act waits for the module's next turn")
        T.runDue()
        T.runDue()
        T.ok(T.since(from) == "focus 77,act,snapshot(cb),click,post state,show", T.since(from))
        T.ok(S.clicks[#S.clicks][1] == 110 and S.clicks[#S.clicks][2] == 60, "inside the plug-in")
        T.ok(#S.afters == 1 and S.afters[1].name == "Pad", "after has the result")
        T.ok(#S.speech == said, "nothing said: " .. T.saidSince(said))
        T.ok(T.hookCount("Synth", "on") == 1, "no onActivate")
        T.pageFront()
        T.ok(ov.away and not ov.active, "away again in the page")
        T.ok(T.count("[page] 'Synth': trip 'Next preset' in the plug-in after 0 ms; the keyboard on JUCE_PlugIn") == 1, T.dump())
        T.ok(T.count("[activate] 'Synth' — away, quiet (trip 'Next preset')") == 1, T.dump())
    "##);
}

/// On a Mac the panel's verdict is kept for the stay, and the stay is held across trips and the
/// way back: identify is asked once, at the first arrival.
#[test]
fn no_identify_within_the_stay_across_trips() {
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.ok(S.identifies == 1, "identified once")
        T.openPage()
        for _ = 1, 2 do
          T.ok(T.trip() == true, "started")
          T.land()
          T.runDue()
          T.runDue()
          T.pageFront()
        end
        T.ok(S.acts == 2 and S.focusCalls[1] == 21, "two trips, each to the window")
        T.key()
        T.land()
        T.ok(ov.active and not ov.away, "back")
        T.ok(S.identifies == 1, "identify not asked again: " .. S.identifies)
    "##);
}

/// The held context takes the plug-in although the keyboard is on the DAW's own controls: REAPER's
/// FX list on Windows, Logic's header on a Mac.
#[test]
fn a_held_context_takes_the_plugin_although_the_keyboard_is_on_the_daws_controls() {
    for mac in [false, true] {
        let scenario = r##"
            local S = T.S
            T.show(T.W, T.IN)
            local ov = T.synth()
            T.openPage()
            T.trip()
            S.landChain = T.CHROME
            T.land()
            T.runDue()
            T.ok(S.acts == 1, "act ran with the keyboard on the DAW's controls")
            T.runDue()
            T.ok(S.front == T.PAGE and #S.giveUps == 0, "and the page came back")
        "##;
        if mac { run_mac(scenario) } else { run(scenario) }
    }
}

/// The held context is the group's, and its answer is kept apart from the binding's: an overlay on
/// another slot sharing the binding does not come up during the trip with the keyboard on the FX
/// list, gets no held context, and its next evaluation asks its own context.
#[test]
fn the_held_context_does_not_leak_to_an_overlay_outside_the_group_sharing_the_binding() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local b = T.O.embedded(T.bindingSpec())
        local ov = T.synth({ bound = b })
        local other = T.O.new("Other")
        other:addCustomButton({ label = "Other", onActivate = function() end })
        T.hooked(other)
        other:bind(b, { slot = "com.platform.else", specificity = T.O.layer.base })
        T.runDue()
        T.ok(ov.active and other.active, "both in front")
        T.openPage()
        T.ok(not other.away and not other.active, "the other let go, as for any window")
        T.trip({ act = function(o, trip) S.trip = trip end })
        S.landChain = T.CHROME
        T.land()
        T.runDue()
        T.ok(ov.active and not other.active, "the group's overlay is up, the other is not")
        T.ok(#other.contexts == 1, "no held context for the other")
        T.ok(other.contexts[1].match() == false, "its own context: the keyboard on the FX list")
        S.trip:done({})
        T.runDue()
        T.ok(other.contexts[1].origin() == nil, "nothing of the trip kept for the binding")
    "##);
}

/// Logic's main window first, on the way to the plug-in: no end of the trip, which arrives when the
/// plug-in's window comes.
#[test]
fn logics_main_window_first_does_not_end_the_trip() {
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.trip()
        S.landTo, S.landChain = T.LOGIC_MAIN, { T.LOGIC_MAIN }
        T.land()
        T.runDue()
        T.ok(#S.giveUps == 0 and S.acts == nil, "still on the way")
        T.show(T.LOGIC, T.IN)
        T.runDue()
        T.ok(S.acts == 1 and #S.giveUps == 0, "arrived")
    "##);
}

/// Another application's window ends the trip in every stage — on the way, acting, waiting for a
/// menu, waiting for a dialog: onGiveUp("another application"), the sentence behind the screen
/// reader, the page not brought back over it, away ending there, and onDeactivate once.
#[test]
fn another_applications_window_ends_the_trip_at_every_stage_and_the_page_is_not_brought_back() {
    for stage in ["forward", "acting", "menu", "dialog"] {
        let scenario = r##"
            local S = T.S
            local stage = "@STAGE@"
            T.show(T.W, T.IN)
            local ov = T.synth()
            T.openPage()
            S.actFn = function(o, trip)
              S.trip = trip
              if stage == "menu" then trip:afterMenu(function() end) end
              if stage == "dialog" then trip:afterDialog(function() end) end
            end
            T.trip()
            if stage ~= "forward" then
              T.land()
              T.runDue()
              T.ok(S.acts == 1, "acting")
            end
            local shows = #S.pg.shows
            T.show(T.OTHER)
            T.runDue()
            T.ok(S.giveUps[1] == "another application", stage .. ": " .. tostring(S.giveUps[1]))
            T.ok(T.last() == "Next preset: another window came to the front; nothing more was done."
              and T.lastInterrupts() == false, T.last())
            T.ok(#S.pg.shows == shows, "the page is not brought back")
            T.ok(not ov.away, "away ends")
            T.runDue()
            T.ok(T.hookCount("Synth", "off") == 1, "onDeactivate once")
        "##;
        run(&scenario.replace("@STAGE@", stage));
    }
}

/// The page key gives a trip up in every stage and shows the page — even one still in front, since
/// a focus on its way could undo it — with the module's onGiveUp("key") first. In `back`, the work
/// is done: `after` runs, then the page is shown.
#[test]
fn the_page_key_gives_a_trip_up_and_shows_the_page_in_every_stage() {
    for stage in ["forward", "acting", "waiting", "back"] {
        let scenario = r##"
            local S = T.S
            local stage = "@STAGE@"
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            S.actFn = function(o, trip)
              S.trip = trip
              if stage == "waiting" then trip:afterDialog(function() end) end
              if stage == "back" then
                T.host.ocr.recognize({ 110, 60, 300, 80 }, function() trip:done({ name = "Pad" }) end)
              end
            end
            T.trip()
            if stage ~= "forward" then
              T.land()
              T.runDue()
            end
            if stage == "back" then
              local r = S.reads[#S.reads]
              T.call("answer", r.cb, { status = "none", text = "", lines = {}, words = {} })
            end
            local shows, from = #S.pg.shows, #S.order
            T.key()
            T.ok(#S.pg.shows == shows + 1 and S.front == T.PAGE, stage .. ": the page shown")
            if stage == "back" then
              T.ok(#S.afters == 1 and #S.giveUps == 0, "the back step: after, not onGiveUp")
              T.ok(T.since(from) == "post state,show", T.since(from))
            else
              T.ok(S.giveUps[1] == "key" and #S.afters == 0, "onGiveUp(key)")
              T.ok(T.since(from) == "post error,show", T.since(from))
              T.ok(T.count("[page] 'Synth': trip 'Next preset' given up at the key after 0 ms in " .. (stage == "waiting" and "waiting" or stage)) == 1, T.dump())
            end
            T.pageFront()
            T.ok(T.trip() == true, "the next trip starts")
        "##;
        run(&scenario.replace("@STAGE@", stage));
    }
}

/// The page closed ends a trip: its close box, its failure, and the module's own page:close(), seen
/// as the page not shown at the next event. "The page was closed", and nothing is shown.
#[test]
fn the_page_closed_gives_a_trip_up() {
    for how in ["user", "failed", "module"] {
        let scenario = r##"
            local S = T.S
            local how = "@HOW@"
            T.show(T.W, T.IN)
            local ov = T.synth()
            T.openPage()
            T.trip()
            local shows = #S.pg.shows
            if how == "module" then
              T.page:close()
              T.land()
            else
              T.closed(how)
            end
            T.ok(S.giveUps[1] == "page closed", how .. ": " .. tostring(S.giveUps[1]))
            T.ok(T.last() == "Next preset: the page was closed; nothing more was done." and T.lastInterrupts() == false, T.last())
            T.ok(#S.pg.shows == shows, "nothing shown")
            if how ~= "module" then T.land() end
            T.ok(ov.active and not ov.away, "back in the plug-in")
            T.runDue()
        "##;
        run(&scenario.replace("@HOW@", how));
    }
}

/// One trip at a time: a second message's inPlugin, and O:pageBack, answer "busy" while one is
/// under way, logged, with no second focus asked.
#[test]
fn one_trip_at_a_time_and_back_during_a_trip_is_busy() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.ok(T.trip() == true, "the first starts")
        local ok, why = T.trip()
        T.ok(ok == false and why == "busy", "the second: " .. tostring(why))
        local okB, whyB = ov:pageBack()
        T.ok(okB == false and whyB == "busy", "the way back: " .. tostring(whyB))
        T.ok(#S.focusCalls == 1, "one focus asked")
        T.ok(T.count("[page] 'Synth': trip 'Next preset' not started — busy") == 1, T.dump())
        T.ok(T.count("[page] 'Synth': the way back not taken — busy") == 1, T.dump())
        T.land()
        T.runDue()
        T.runDue()
    "##);
}

/// A message that reaches the module after the user went elsewhere — the page no longer has the
/// keyboard — starts no trip and asks nothing of the system; nor does one from a page that came
/// forward without the page key.
#[test]
fn a_message_while_the_page_does_not_have_the_keyboard_starts_no_trip() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.front, S.chain = T.OTHER, { T.OTHER }
        local ok, why = T.trip()
        T.ok(ok == false and why == "not in front" and #S.focusCalls == 0, tostring(why))
        T.ok(T.count("[page] 'Synth': trip 'Next preset' not started — not in front") == 1, T.dump())
    "##);
    // A page the module showed itself holds no place: the overlay let go as for any window, and a
    // trip is refused, "nothing held".
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        S.pg.ready = true
        T.page:show({ focus = true })
        T.pageFront()
        T.ok(T.count("[page] 'Synth': the page came to the front without the page key — nothing held") == 1, T.dump())
        T.ok(not ov.away and T.hookCount("Synth", "off") == 1, "let go, its onDeactivate")
        local ok, why = T.trip()
        T.ok(ok == false and why == "nothing held", tostring(why))
    "##);
}

/// The plug-in's window never comes: 8 seconds after the focus the trip is given up, "not in front".
/// The error is posted and the page shown again where the page or another window of the plug-in's
/// application is in front; where another application's is, nothing is shown.
#[test]
fn the_8_second_bound_gives_the_trip_up_when_the_plugin_never_comes() {
    for front in ["page", "main", "other"] {
        let scenario = r##"
            local S = T.S
            local front = "@FRONT@"
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            T.trip()
            if front == "main" then
              T.show(T.MAINWIN)
            elseif front == "other" then
              S.front, S.chain = T.OTHER, { T.OTHER }
            end
            local shows = #S.pg.shows
            T.run(7900)
            T.ok(#S.giveUps == 0, "still looked for at 7.9 s")
            T.run(300)
            T.ok(S.giveUps[1] == "not in front", front .. ": " .. tostring(S.giveUps[1]))
            T.ok(T.posts("error") == 1, "the error posted")
            if front == "other" then
              T.ok(#S.pg.shows == shows and T.last() == "Next preset: another window came to the front; nothing more was done.", T.last())
            else
              T.ok(#S.pg.shows == shows + 1, front .. ": the page shown again")
            end
            T.ok(T.count("[page] 'Synth': trip 'Next preset': the plug-in's window did not come within 8000 ms") == 1, T.dump())
        "##;
        run(&scenario.replace("@FRONT@", front));
    }
}

/// The plug-in's window gone: listed no more, a trip is refused ("gone"); the plug-in unloaded from
/// it on the way, the trip is given up ("gone") and the page comes back with the module's error. On
/// a Mac, the window back with another title is another plug-in: "gone".
#[test]
fn a_window_gone_refuses_the_trip_and_one_gone_midway_gives_it_up() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.listed = { T.MAIN, T.OTHER }
        local ok, why = T.trip()
        T.ok(ok == false and why == "gone" and #S.focusCalls == 0, tostring(why))
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.trip()
        S.controls = { T.LIST, T.WRAP }
        T.land()
        T.runDue()
        T.ok(S.giveUps[1] == "gone" and S.acts == nil, tostring(S.giveUps[1]))
        T.ok(S.front == T.PAGE and T.posts("error") == 1, "the page back with the error")
        T.ok(T.count("is gone — the plug-in's control is not in it any more") == 1, T.dump())
    "##);
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.trip()
        S.landTo = { id = 21, title = "Inst 2", class = T.LOGIC.class, app = T.MAC_LOGIC,
          bounds = T.LOGIC.bounds, client = T.LOGIC.client }
        S.landChain = { S.landTo }
        T.land()
        T.runDue()
        T.ok(S.giveUps[1] == "gone" and S.acts == nil, tostring(S.giveUps[1]))
    "##);
}

/// A focus the system declines starts no trip: "declined", nothing held in the group's contexts.
#[test]
fn a_focus_declined_starts_no_trip() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        S.focusAnswer = false
        local ok, why = T.trip()
        T.ok(ok == false and why == "declined", tostring(why))
        T.ok(#ov.contexts == 1, "no held context")
        S.focusAnswer = true
        T.ok(T.trip() == true, "the next one starts")
        T.land()
        T.runDue()
        T.runDue()
    "##);
}

/// Another module's overlay holding the slot on the plug-in's window, at least as specific as the
/// page's overlay: the trip is given up on the turn after the window's event, "outranked", not after
/// 8 seconds. A lower claim elected for a moment is replaced by this VM's own report, and the trip
/// arrives.
#[test]
fn another_modules_overlay_on_the_slot_gives_the_trip_up_on_the_next_turn() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.trip()
        local id = T.another(T.O.layer.content)
        T.matching(id, true)
        T.land()
        T.runDue()
        T.runDue()
        T.ok(S.giveUps[1] == "outranked" and S.acts == nil, tostring(S.giveUps[1]))
        T.ok(S.front == T.PAGE, "the page back")
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        T.trip()
        local id = T.another(T.O.layer.chrome)
        T.matching(id, true)
        T.land()
        T.runDue()
        T.ok(#S.giveUps == 0 and S.acts == 1, "arrived: " .. tostring(S.giveUps[1]))
        T.runDue()
    "##);
}

/// Every hook of a trip runs as a handler, where a read may wait: `act`, `after`, `onGiveUp`, and
/// the `fn` of afterMenu and of afterDialog each take a picture without a callback, and none raises
/// (the scripted host raises where it could not wait, and `finish` fails on it).
#[test]
fn every_trip_hook_runs_as_a_handler_where_a_read_may_wait() {
    run(r##"
        local S = T.S
        local H = T.host
        local function picture(who)
          H.screen.snapshot({ region = { 110, 60, 300, 80 } })
          S.pictures = (S.pictures or 0) + 1
          S.order[#S.order + 1] = "picture by " .. who
        end
        T.show(T.W, T.IN)
        T.synth({ menus = { { name = "seen", cheap = true, test = function(_, answer) answer(S.menuSeen == true) end } } })
        T.openPage()
        local spec = {
          act = function(o, trip)
            picture("act")
            trip:afterMenu(function(o2, t2, seen)
              picture("afterMenu")
              t2:afterDialog(function(o3, t3, seen2)
                picture("afterDialog")
                t3:done({})
              end)
            end)
          end,
          after = function(o) picture("after") end,
          onGiveUp = function(o) picture("onGiveUp") end,
        }
        T.trip(spec)
        T.land()
        T.runDue()
        S.menuSeen = true
        T.tick()
        S.menuSeen = false
        T.tick(2)
        T.runDue()
        T.show(T.MAINWIN)
        T.runDue()
        T.show(T.W, T.IN)
        T.runDue()
        T.runDue()
        T.ok(S.pictures == 4, "act, afterMenu, afterDialog, after: " .. tostring(S.pictures))
        T.pageFront()
        T.trip(spec)
        T.key()
        T.ok(S.pictures == 5, "onGiveUp at the key: " .. tostring(S.pictures))
    "##);
}

/// A hook that raises: an `act` gives the trip up ("failed", logged) and the page comes back with
/// the module's error; an `after` that raises does not stop the page from coming back.
#[test]
fn a_hook_that_raises_gives_the_trip_up_and_the_page_comes_back() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.trip({ act = function() error("the act broke") end })
        T.land()
        T.runDue()
        T.runDue()
        T.ok(S.giveUps[1] == "failed", tostring(S.giveUps[1]))
        T.ok(T.count("[page] 'Synth': act failed: ") == 1 and T.count("the act broke") == 1, T.dump())
        T.ok(S.front == T.PAGE and T.posts("error") == 1, "the page back with the error")
        T.pageFront()
        T.trip({ after = function(o) o.page:post({ kind = "state" }); error("after broke") end })
        T.land()
        T.runDue()
        T.runDue()
        T.ok(T.count("[page] 'Synth': after failed: ") == 1, T.dump())
        T.ok(S.front == T.PAGE, "the page back all the same")
    "##);
}

/// `done` from a plain call — chooseMenuItem's onDone, reached from the menu tick, and from the
/// arbiter's call as the overlay leaves for a sibling box — runs the back step on the next turn.
#[test]
fn done_from_a_plain_call_runs_the_back_step_on_the_next_turn() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          o:chooseMenuItem({ label = "Zoom", at = { 50, 20 }, menuItem = { 10, 10 }, retries = 0,
            onDone = function(_, chosen) trip:done({ chosen = chosen }) end })
        end
        T.trip()
        T.land()
        T.runDue()
        S.windows[#S.windows + 1] = { id = 900, layer = 101, class = "JUCE_Popup", x = 150, y = 70, w = 150, h = 200 }
        T.tick()
        T.ok(S.trip.live == false and #S.afters == 0, "done in the tick; the back step waits for the next turn")
        T.runDue()
        T.ok(#S.afters == 1 and S.afters[1].chosen == true, "the back step on the next turn")
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        local box = T.O.new("Box")
        box:addCustomButton({ label = "OK", onActivate = function() end })
        box:gate(function() return S.boxUp == true end)
        T.hooked(box)
        box:bind(T.O.embedded(T.bindingSpec()), { slot = "com.platform.synth", specificity = T.O.layer.dialog,
          pollMatch = 500 })
        T.runDue()
        T.openPage()
        S.actFn = function(o, trip)
          o:chooseMenuItem({ label = "Zoom", at = { 50, 20 }, menuItem = { 10, 10 },
            onDone = function(_, chosen) S.inPlain = true; trip:done({ chosen = chosen }) end })
        end
        T.trip()
        T.land()
        T.runDue()
        S.windows[#S.windows + 1] = { id = 900, layer = 101, class = "JUCE_Popup", x = 150, y = 70, w = 150, h = 200 }
        T.tick()
        T.ok(S.inPlain == nil, "the item clicked, waiting to be seen taken")
        S.boxUp = true
        T.poll(500)
        T.ok(box.active and S.inPlain == true and #S.afters == 0, "onDone in the arbiter's call, done scheduled")
        T.runDue()
        T.ok(#S.afters == 1, "the back step on the next turn")
    "##);
}

/// `trip:inFront()` answering false ENDS the trip, so an act that just returns leaves nothing under
/// way: REAPER's main window came forward while the act waited at a read — nothing clicked, the
/// trip given up "not in front" and the page back; the same with another application's window —
/// "another application", nothing shown; and the next inPlugin starts.
#[test]
fn in_front_false_ends_the_trip_so_an_act_that_returns_leaves_nothing_under_way() {
    for other in [false, true] {
        let scenario = r##"
            local S = T.S
            local other = @OTHER@
            T.show(T.W, T.IN)
            T.synth()
            T.openPage()
            S.actFn = function(o, trip)
              T.waitPoint("act's read")
              if not trip:inFront() then return end
              T.host.input.click(o:toScreen(10, 10))
              trip:done({})
            end
            T.trip()
            T.land()
            T.runDue()
            T.ok(T.waiting("act's read"), "the act waits")
            local clicks, shows = #S.clicks, #S.pg.shows
            if other then
              S.front, S.chain = T.OTHER, { T.OTHER }
            else
              S.front, S.chain = T.MAINWIN, { T.MAINWIN }
            end
            T.event()
            T.release("act's read", nil)
            T.queued()
            T.runDue()
            T.ok(#S.clicks == clicks, "nothing clicked")
            T.ok(S.giveUps[1] == (other and "another application" or "not in front"), tostring(S.giveUps[1]))
            if other then
              T.ok(#S.pg.shows == shows, "not shown over another application")
            else
              T.ok(#S.pg.shows == shows + 1 and S.front == T.PAGE, "the page back")
              T.pageFront()
              T.ok(T.trip() == true, "the next trip starts")
              T.land()
              T.runDue()
              T.runDue()
            end
        "##;
        run(&scenario.replace("@OTHER@", if other { "true" } else { "false" }));
    }
}

/// An act that returns without `done` or a check — a module's bug — is ended by the page key, which
/// says how long it was stuck; the next trip starts.
#[test]
fn a_stuck_act_is_ended_by_the_page_key() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.actFn = function() end
        T.trip()
        T.land()
        T.runDue()
        T.run(3000)
        T.ok(S.trip.live, "stuck: no bound ends an act")
        T.key()
        T.ok(S.giveUps[1] == "key" and S.front == T.PAGE, "given up at the key, the page shown")
        T.ok(T.count("[page] 'Synth': trip 'Next preset' given up at the key after 3000 ms in acting") == 1, T.dump())
        T.pageFront()
        S.actFn = nil
        T.ok(T.trip() == true, "the next trip starts")
        T.land()
        T.runDue()
        T.runDue()
    "##);
}

/// The page does not come back after a trip (Windows declined) with the plug-in's window in front:
/// the runtime's sentence and `after`'s words, interrupting; away ends with the keys back and no
/// announcement; and a late grant — the page's window coming to the front all the same — begins away
/// again.
#[test]
fn a_show_that_does_not_bring_the_page_back_ends_away_with_keys_and_no_announcement() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.run(400)
        T.openPage()
        S.afterWords = "Preset Pad"
        T.trip()
        T.land()
        S.pg.answers = { { false, "declined by the system" } }
        T.runDue()
        T.runDue()
        T.ok(T.last() == "The page did not come to the front; if it does not come by itself, the page key brings it. Preset Pad"
          and T.lastInterrupts(), T.last())
        T.ok(not ov.away and ov.active and T.holds("Return"), "away ended, keys back")
        local said = #S.speech
        T.run(450)
        T.ok(#S.speech == said, "no announcement: " .. T.saidSince(said))
        S.front, S.chain = T.PAGE, { T.PAGE }
        T.event()
        T.ok(ov.away and not ov.active, "the late grant: away again")
    "##);
}

/// The way back asks for the plug-in's control first (on a Mac its window), then hides the page, then
/// tells the module; declined, the page stays, still away, and onBack(false, "declined").
#[test]
fn the_way_back_focuses_the_control_first_then_hides() {
    for mac in [false, true] {
        let scenario = r##"
            local S = T.S
            T.show(T.W, T.IN)
            local ov = T.synth()
            T.openPage()
            local from = #S.order
            T.ok(ov:pageBack() == true, "accepted")
            T.ok(T.since(from) == "focus " .. T.HELD .. ",hide", T.since(from))
            T.ok(#S.backs == 1 and S.backs[1].ok == true, "onBack(true)")
            T.land()
            T.ok(ov.active, "back")
            T.openPage()
            S.focusAnswer = false
            from = #S.order
            local ok, why = ov:pageBack()
            T.ok(ok == false and why == "declined" and T.since(from) == "focus " .. T.HELD, tostring(why) .. " " .. T.since(from))
            T.ok(S.backs[2].ok == false and S.backs[2].why == "declined" and S.pg.shown and ov.away, "the page stays")
        "##;
        if mac { run_mac(scenario) } else { run(scenario) }
    }
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth({ page = { onBack = false } })
        T.openPage()
        S.focusAnswer = false
        ov:pageBack()
        T.ok(T.last() == "Synth: the plug-in's window would not come to the front." and T.lastInterrupts(), T.last())
        S.listed = { T.MAIN, T.OTHER }
        local ok, why = ov:pageBack()
        T.ok(ok == false and why == "gone" and T.last() == "Synth: the plug-in's window is gone.", T.last())
    "##);
}

/// A zoom step inside a trip keeps the held place: on Windows the control keeps its id and the next
/// click is placed on its new rectangle; on a Mac the window's new size is the stay's, and the panel
/// is worked out again for it. No identify in either; the page comes back beside the new bounds.
#[test]
fn a_zoom_step_inside_a_trip_keeps_the_held_place() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        local asked = S.identifies
        T.openPage()
        S.actFn = function(o, trip)
          T.PLUG.client = { x = 120, y = 50, w = 1000, h = 750 }
          T.PLUG.bounds = T.PLUG.client
          T.FX.bounds = { x = 0, y = 0, w = 1200, h = 850 }
          T.turn()
          local x, y = o:toScreen(10, 10)
          T.host.input.click(x, y)
          trip:done({})
        end
        T.trip()
        T.land()
        T.runDue()
        T.ok(S.clicks[#S.clicks][1] == 130 and S.clicks[#S.clicks][2] == 60, "the new rectangle")
        T.runDue()
        local a = S.pg.shows[#S.pg.shows].avoid
        T.ok(a.w == 1200 and a.h == 850, "the page beside the new bounds")
        T.ok(S.identifies == asked, "no identify")
    "##);
    run_mac(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        local stayWas = ov:origin().stay
        T.openPage()
        S.actFn = function(o, trip)
          S.trip = trip
        end
        T.trip()
        T.land()
        T.runDue()
        -- Logic resizes the window for the plug-in's new zoom: its event.
        T.LOGIC.client = { x = 200, y = 30, w = 1300, h = 1000 }
        T.LOGIC.bounds = T.LOGIC.client
        T.event()
        T.ok(S.trip.live and ov.active, "the trip goes on")
        T.ok(ov:origin().stay == stayWas and ov:origin().client.w == 1298, "the same stay, the new panel")
        S.trip:done({})
        T.runDue()
        local a = S.pg.shows[#S.pg.shows].avoid
        T.ok(a.w == 1300 and a.h == 1000, "the page beside the new bounds")
        T.ok(S.identifies == 1, "no identify in the trip")
        T.ok(T.count("is 1300x1000 now — a zoom step inside trip 'Next preset'; the stay goes on") == 1, T.dump())
    "##);
}

/// The page key's spec is checked at O:addPage — a spec the host refuses raises, at the module's
/// line — and against the hotkeys of each overlay of the group as it binds: its own controls', and a
/// sibling's. A describe that reads not all of `on`, a call after binding, and a second page in the
/// group raise. O:pageKey answers false and keeps the old key; a registration the system refuses is
/// logged, not raised.
#[test]
fn the_page_key_spec_is_checked_at_addpage_and_at_each_group_overlays_bind() {
    run(r##"
        local S = T.S
        local O = T.O
        S.redescribe = true
        S.refuse["Ctrl+Alt+Del"] = "reserved"
        local function raises(fn, text)
          local ok, err = pcall(fn)
          T.ok(not ok and string.find(tostring(err), text, 1, true), "expected a raise with '" .. text .. "': " .. tostring(err))
        end
        local a = O.new("A")
        raises(function() a:addPage(T.pageSpec({ key = "Ctrl+Alt+Del" })) end, "addPage's key Ctrl+Alt+Del is refused for a hotkey (reserved)")
        raises(function() a:addPage(T.pageSpec({ describe = function(on)
          return T.host.gui.webview({ file = "ui/index.html", title = "x", onMessage = on.message })
        end })) end, "did not read on.ready")
        raises(function() a:addPage(T.pageSpec({ open = false })) end, "addPage needs `open`")
        raises(function() a:addPage(T.pageSpec({ colour = 1 })) end, "addPage has no field 'colour'")
        -- A control's hotkey: raised at the bind.
        T.show(T.W, T.IN)
        raises(function() T.synth({ label = "B", page = { key = "Alt+P" } }) end,
          "the page key Alt+P is the hotkey of control 'Preset' of overlay 'B'")
    "##);
    run(r##"
        local S = T.S
        local O = T.O
        local function raises(fn, text)
          local ok, err = pcall(fn)
          T.ok(not ok and string.find(tostring(err), text, 1, true), "expected a raise with '" .. text .. "': " .. tostring(err))
        end
        T.show(T.W, T.IN)
        local ov = T.synth()
        raises(function() ov:addPage(T.pageSpec()) end, "must be called BEFORE binding")
        -- A sibling whose control has the page key.
        local sib = O.new("Sib")
        sib:addCustomButton({ label = "Clash", hotkey = T.KEY, onActivate = function() end })
        raises(function() sib:bind(O.embedded(T.bindingSpec()), { slot = "com.platform.synth", specificity = O.layer.dialog }) end,
          "the page key " .. T.KEY .. " is the hotkey of control 'Clash' of overlay 'Sib'")
        -- A second page in the group.
        local second = O.new("Second")
        second:addCustomButton({ label = "OK", onActivate = function() end })
        S.redescribe = true
        second:addPage(T.pageSpec({ key = "Ctrl+Shift+Alt+F2" }))
        raises(function() second:bind(O.embedded(T.bindingSpec()), { slot = "com.platform.synth", specificity = O.layer.dialog }) end,
          "a second page in the same group")
        -- O:pageKey: refused, and the old key kept.
        local ok, why = ov:pageKey("Alt+P")
        T.ok(ok == false and why == "a control's hotkey", tostring(why))
        S.refuse["Ctrl+Alt+Del"] = "reserved"
        ok, why = ov:pageKey("Ctrl+Alt+Del")
        T.ok(ok == false and why == "reserved", tostring(why))
        S.registerRaises = { ["Ctrl+Shift+F9"] = "held by another program" }
        ok, why = ov:pageKey("Ctrl+Shift+F9")
        T.ok(ok == false and why == "refused: held by another program", tostring(why))
        S.pg.ready = true
        T.key()
        T.ok(S.front == T.PAGE, "the old key still works")
        T.pageFront()
        ok = ov:pageKey("Ctrl+Shift+F8")
        T.ok(ok == true and S.hotkeyFns["Ctrl+Shift+F8"] ~= nil, "a new key")
        T.key("Ctrl+Shift+F8")
        T.land()
        T.ok(ov.active, "the new key goes back")
    "##);
    run(r##"
        local S = T.S
        S.registerRaises = { [T.KEY] = "held by another program" }
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.ok(ov.page ~= nil, "added all the same")
        T.ok(T.count("[page] 'Synth': the page key " .. T.KEY .. " is not registered: held by another program") == 1, T.dump())
    "##);
}

/// O:pageAgain: a trip under way is given up ("page closed"), the page closed and described again,
/// `o.page` the new one; away ends where the user is; the page is prepared at the next arrival.
#[test]
fn page_again_closes_redescribes_gives_a_trip_up_and_prepares() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        local first = ov.page
        T.trip()
        S.redescribe = true
        local again = ov:pageAgain()
        T.ok(S.giveUps[1] == "page closed", tostring(S.giveUps[1]))
        T.ok(again ~= first and ov.page == again and S.page == again, "the new page")
        T.ok(not ov.away, "away ends")
        S.pg.ready = false
        local prepares = S.pg.prepares
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(S.pg.prepares == prepares + 1, "prepared at the next arrival")
        T.ok(ov:pageAgain() ~= again and S.pg.prepares == prepares + 2, "prepared at once, in front")
        T.runDue()
    "##);
}

/// A DAW whose entry says it drops the first click after its window comes forward, and a module
/// that gave a neutral point: one click there at each trip's arrival, before `act`, and never on
/// the way back. Without the entry's word, none. A covered point is logged, and `act` runs.
#[test]
fn prime_clicks_once_at_arrival_only_with_the_entrys_field_and_the_modules_point() {
    run(r##"
        local S = T.S
        local entry = {}
        for k, v in pairs(T.REAPER_WIN) do entry[k] = v end
        entry.swallowsFirstClick = true
        T.show(T.W, T.IN)
        T.synth({ binding = { hosts = { entry } }, page = { prime = { x = 5, y = 5 } } })
        T.openPage()
        local from = #S.order
        T.trip()
        T.land()
        T.runDue()
        T.ok(T.since(from) == "focus 77,click,act,click,post state,show", T.since(from))
        T.ok(S.clicks[1][1] == 105 and S.clicks[1][2] == 55, "at the module's point")
        T.ok(T.count("[page] 'Synth': primed at 105,55 (the DAW's entry says it drops the first click)") == 1, T.dump())
        T.runDue()
        T.pageFront()
        local clicks = #S.clicks
        T.key()
        T.land()
        T.ok(#S.clicks == clicks, "never on the way back")
        T.openPage()
        S.owns = false
        T.trip()
        T.land()
        T.runDue()
        T.ok(S.acts == 2 and T.count("the prime at 105,55 is covered by another window — not clicked") == 1, T.dump())
        T.runDue()
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth({ page = { prime = { x = 5, y = 5 } } })
        T.openPage()
        local from = #S.order
        T.trip()
        T.land()
        T.runDue()
        T.ok(T.since(from) == "focus 77,act,click,post state,show", "no prime without the entry's word: " .. T.since(from))
        T.runDue()
    "##);
}

// ---------------------------------------------------------------------------------------------
// RG1b: waits inside a trip.
// ---------------------------------------------------------------------------------------------

/// "The menu my next click opens": afterMenu counts as a press (newWindow takes its list), the right
/// click opens a popup window in front, over which the overlay holds its place with no keys while
/// the keys go through to it; the popup goes, the hold ends with no key taken back (away), and fn
/// runs with `seen` on the next turn, W in front — done, and the page back.
#[test]
fn a_popups_close_ends_a_menu_wait() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          local ok = trip:afterMenu(function(o2, t2, seen)
            S.seenArg = seen
            S.order[#S.order + 1] = "fn"
            t2:done({})
          end)
          T.ok(ok == true, "the press counts")
          T.host.input.click(o:toScreen(400, 300))
        end
        T.trip()
        T.land()
        T.runDue()
        S.windows[#S.windows + 1] = { id = 900, layer = 101, class = "JUCE_Popup", x = 300, y = 200, w = 150, h = 200 }
        T.show(T.POPUP)
        T.ok(ov.active and ov:_heldOnMenu() and T.onlyPageKey(), "held over the popup, no keys")
        S.passed = { { key = "Down" }, { key = "Return" } }
        T.tick()
        T.ok(T.count("[menu] 'Synth': went through to the menu: Down, Return") == 1, T.dump())
        table.remove(S.windows)
        T.show(T.W, T.IN)
        T.ok(T.onlyPageKey(), "the hold ended; nothing taken back")
        T.tick()
        T.ok(S.seenArg == nil, "fn on the next turn")
        T.runDue()
        T.ok(S.seenArg == true, "fn with the menu seen")
        T.runDue()
        T.ok(S.front == T.PAGE and #S.afters == 1, "done, the page back")
    "##);
}

/// A menu that never opens: fn runs with `seen` false once MENU_LOOK_MS have passed since the call.
#[test]
fn a_menu_that_never_opens_is_told_after_the_look_bound() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          trip:afterMenu(function(o2, t2, seen)
            S.seenArg = seen
            t2:done({})
          end)
        end
        T.trip()
        T.land()
        T.runDue()
        T.run(7900)
        T.ok(S.seenArg == nil, "still looked for")
        T.run(300)
        T.ok(S.seenArg == false, "told: no menu")
        T.runDue()
        T.ok(#S.afters == 1, "done")
    "##);
}

/// "The dialog that comes next": chooseMenuItem chooses, its onDone asks for the dialog; the popup's
/// own last event is not taken for it; a window of the same process coming forward is; minutes pass —
/// nothing bounds the wait once a dialog was seen — and the plug-in's window again runs fn, done,
/// and the page comes back. The dialog's pid is logged.
#[test]
fn a_dialog_then_the_plugin_again_ends_a_dialog_wait() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          o:chooseMenuItem({ label = "Load", at = { 20, 20 }, menuItem = { 10, 10 }, retries = 0,
            onDone = function(_, chosen)
              S.chosen = chosen
              trip:afterDialog(function(o2, t2, seen)
                S.seenArg = seen
                t2:done({})
              end)
            end })
        end
        T.trip()
        T.land()
        T.runDue()
        S.windows[#S.windows + 1] = { id = 900, layer = 101, class = "JUCE_Popup", x = 150, y = 70, w = 150, h = 200 }
        T.show(T.POPUP)
        T.tick()
        T.ok(S.chosen == true, "chosen")
        T.event()
        T.runDue()
        T.ok(T.count("'Komplete Kontrol' came forward, but it is the menu's window, not the dialog") == 1, T.dump())
        table.remove(S.windows)
        local dialog = { id = 950, title = "Load preset", class = "#32770", app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 200, y = 100, w = 600, h = 400 }, client = { x = 208, y = 130, w = 584, h = 362 } }
        T.show(dialog)
        T.runDue()
        T.ok(T.count("[page] 'Synth': trip 'Next preset': a dialog came forward — 'Load preset', window id 950 of pid 4242") == 1, T.dump())
        T.run(60000)
        T.ok(S.seenArg == nil and S.trip.live, "nothing bounds the wait once the dialog came")
        T.show(T.W, T.IN)
        T.runDue()
        T.ok(S.seenArg == true, "fn: the dialog was seen")
        T.runDue()
        T.ok(S.front == T.PAGE and #S.afters == 1, "the page back")
    "##);
}

/// A dialog that never comes: fn runs with `seen` false once TRIP_LOOK_MS have passed since the call.
#[test]
fn a_dialog_that_never_comes_is_told_after_the_bound() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          trip:afterDialog(function(o2, t2, seen)
            S.seenArg = seen
            t2:done({})
          end)
        end
        T.trip()
        T.land()
        T.runDue()
        T.run(8100)
        T.ok(S.seenArg == false, "told: no dialog")
        T.runDue()
        T.ok(#S.afters == 1 and S.front == T.PAGE, "done, the page back")
    "##);
}

/// A sibling dialog overlay of the same module — a box on the dialog layer of the same slot, with a
/// gate, `pollMatch` and `pollWhen` — takes the slot during the act through the arbiter: the header
/// lets go and the box comes up, both quietly — nothing said, no key, no hook — and `trip.overlay`
/// names the box; the box goes, the header is back, `done`, and the page comes back.
#[test]
fn a_sibling_dialog_overlay_of_the_same_module_keeps_the_trip() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        local box = T.O.new("Box")
        box:addCustomButton({ label = "OK", onActivate = function() end })
        box:gate(function() return S.boxUp == true end)
        T.hooked(box)
        box:bind(T.O.embedded(T.bindingSpec()), { slot = "com.platform.synth", specificity = T.O.layer.dialog,
          pollMatch = 500, pollWhen = function() return S.boxExpected == true end })
        T.runDue()
        T.run(400)
        T.openPage()
        S.actFn = function(o, trip) S.trip = trip end
        T.trip()
        T.land()
        T.runDue()
        local said = #S.speech
        S.boxUp, S.boxExpected = true, true
        T.poll(500)
        T.ok(box.active and box.away and not ov.active, "the box took the slot, quietly")
        T.ok(S.trip.overlay == box and T.onlyPageKey(), "trip.overlay is the box; no key")
        S.boxUp = false
        T.poll(500)
        T.ok(ov.active and not box.active and S.trip.overlay == ov, "the box gone, the header back")
        T.run(450)
        T.ok(#S.speech == said, "nothing said: " .. T.saidSince(said))
        T.ok(T.hookCount("Box", "on") == 0 and T.hookCount("Synth", "off") == 0, "no hook")
        S.trip:done({})
        T.runDue()
        T.ok(S.front == T.PAGE and #S.afters == 1, "the page back")
    "##);
}

/// An overlay of the module on another slot is not the page's group: it lets go for the page with
/// its own onDeactivate, is not away, and comes up during a trip with its keys and its hooks as
/// today.
#[test]
fn a_sibling_of_another_slot_is_not_the_group() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        local ov = T.synth()
        local other = T.O.new("Other")
        other:addCustomButton({ label = "Other", hotkey = "Alt+O", onActivate = function() end })
        T.hooked(other)
        other:bind(T.O.embedded(T.bindingSpec()), { slot = "com.platform.else", specificity = T.O.layer.base })
        T.runDue()
        T.openPage()
        T.ok(not other.away and not other.active and T.hookCount("Other", "off") == 1, "its own onDeactivate")
        S.actFn = function(o, trip) S.trip = trip end
        T.trip()
        T.land()
        T.runDue()
        T.ok(other.active and not other.away and T.hookCount("Other", "on") == 2, "up with its hook")
        local held = false
        for _, spec in pairs(S.hot) do if spec == "Alt+O" then held = true end end
        T.ok(held, "and its keys")
        S.trip:done({})
        T.runDue()
    "##);
}

/// One wait at a time — a second raises — and afterMenu on an overlay with no menu tests raises; so
/// does a wait asked for once the trip is done.
#[test]
fn two_waits_at_once_raise_and_aftermenu_without_menu_tests_raises() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        S.actFn = function(o, trip)
          trip:afterDialog(function() end)
          local ok, err = pcall(trip.afterMenu, trip, function() end)
          S.err1 = tostring(err)
          trip:done({})
          ok, err = pcall(trip.afterDialog, trip, function() end)
          S.err2 = tostring(err)
        end
        T.trip()
        T.land()
        T.runDue()
        T.ok(string.find(S.err1, "afterMenu while a dialog wait is pending — one wait at a time", 1, true), S.err1)
        T.ok(string.find(S.err2, "afterDialog is for the trip's act and its continuations, while it is acting (it is over)", 1, true), S.err2)
        T.runDue()
    "##);
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth({ menus = false })
        T.openPage()
        S.actFn = function(o, trip)
          local ok, err = pcall(trip.afterMenu, trip, function() end)
          S.err = tostring(err)
          trip:done({})
        end
        T.trip()
        T.land()
        T.runDue()
        T.ok(string.find(S.err, "afterMenu needs menu tests on overlay 'Synth'", 1, true), S.err)
        T.runDue()
    "##);
}

/// On Windows the trip asks for the plug-in's CONTROL, never the FX window: the host lands the
/// keyboard in it (HW1a).
#[test]
fn the_trip_focuses_the_plugins_control_on_windows() {
    run(r##"
        local S = T.S
        T.show(T.W, T.IN)
        T.synth()
        T.openPage()
        T.trip()
        T.ok(#S.focusCalls == 1 and S.focusCalls[1] == 77, "the control's id: " .. tostring(S.focusCalls[1]))
        T.land()
        T.runDue()
        T.runDue()
        T.pageFront()
        T.key()
        for _, id in ipairs(S.focusCalls) do T.ok(id == 77, "never the FX window's id: " .. tostring(id)) end
        T.land()
    "##);
}
