//! The overlay runtime in front of one of this application's own windows (runtime 0.5.1), run for
//! real against the scripted host of `overlay_menu_tests.rs`.
//!
//! A window table says whether it is this application's own (`own`, which the host sets from the
//! window's process). An overlay bound by class alone took a dialog of the application's for the
//! plug-in's — a standard dialog is a `#32770`, as REAPER's FX window is — and asked its context,
//! its gate and its menu tests there, over UIA among them, at every focus change, from the event
//! loop the dialog waits for. Now an overlay lets go there and asks nothing: no context, no gate, no
//! menu test, nothing of the window that lost the foreground — on a slot by reporting that it does
//! not match, on its own by leaving. And one the arbiter elects meanwhile, on a report it made in
//! the other program's window, does not come up: not in front of such a window, nor in front of
//! the hidden window the tray icon puts in front for its menu, which `active()` does not report.

use super::overlay_menu_tests::run_with;

/// One of the application's own windows: a standard dialog, `#32770`, the class REAPER's FX window
/// has too, so a binding by class alone would take it for the plug-in's. Its OK button has the
/// keyboard. And UIA's `findAny`, counted.
const OWN: &str = r##"
local S = T.S
T.OWN = { id = 3000, title = "Automation Platform", class = "#32770", own = true,
  app = { pid = 5150, exe = "automation-platform.exe" },
  bounds = { x = 300, y = 200, w = 400, h = 200 }, client = { x = 303, y = 226, w = 394, h = 171 } }
T.OK = { id = 3001, class = "Button", bounds = { x = 600, y = 360, w = 80, h = 24 },
  client = { x = 600, y = 360, w = 80, h = 24 } }
S.findAnys = 0
rawset(T.host.element, "findAny", function()
  S.findAnys += 1
  return 1
end)

-- An arbiter that elects as the host's does (lib.rs, arbiter_resolve): at every report that
-- changes a claim, the matching claim of highest specificity — on a tie the later — and on a
-- change the old winner's onDeactivate, then the new one's onActivate; a report made inside one
-- of them is settled by the loop, not by a call inside the call.
function T.electing()
  local claims, n, active, resolving, dirty = {}, 0, nil, false, false
  local function resolve()
    if resolving then dirty = true return end
    resolving, dirty = true, true
    while dirty do
      dirty = false
      local best = nil
      for id, c in pairs(claims) do
        if c.matching and (best == nil or c.spec > claims[best].spec
          or (c.spec == claims[best].spec and id > best)) then
          best = id
        end
      end
      if best ~= active then
        local old = active
        active = best
        if old then T.hostCall(claims[old].off) end
        if best then T.hostCall(claims[best].on) end
      end
    end
    resolving = false
  end
  rawset(T.host.arbiter, "register", function(_, spec, on, off)
    n += 1
    claims[n] = { spec = spec, on = on, off = off, matching = false }
    return n
  end)
  rawset(T.host.arbiter, "setMatching", function(_, id, m)
    if claims[id].matching == m then return end
    claims[id].matching = m
    resolve()
  end)
  rawset(T.host.arbiter, "winner", function() return active end)
  rawset(T.host.arbiter, "winnerSpecificity", function() return active and claims[active].spec end)
end
"##;

fn run(scenario: &str) {
    run_with("windows", OWN, scenario);
}

/// An overlay bound to any `#32770` — on a slot, or on its own — whose gate asks UIA, as a landmark
/// or an identity check does: up over REAPER's FX window, it lets go as the application's own
/// dialog comes to the front, and neither that event, a focus change inside the dialog, nor the
/// ticks after it ask UIA anything. Back in REAPER, both come up again.
#[test]
fn an_overlay_lets_go_in_a_window_of_this_application_and_asks_nothing_there() {
    run(r##"
        local S = T.S
        local function overlay(label, opts)
          local o = T.O.new(label)
          o:addCustomButton({ label = "OK", onActivate = function() end })
          o:gate(function() return T.host.element.findAny(1, { "Plug-in" }, { 50032 }) ~= nil end)
          o:attach({ class = "#32770" }, opts)
          return o
        end
        S.front, S.chain = T.FX, { T.KK, T.WRAP, T.FX }
        T.turn()
        local slotted = overlay("On a slot", { slot = "com.example.box" })
        local plain = overlay("On its own")
        T.runDue()
        assert(slotted.active and plain.active, "both up over REAPER's FX window: " .. T.dump())
        local asked, finds = S.findAnys, S.finds
        assert(asked >= 2, "each gate asked: " .. asked)
        T.show(T.OWN, { T.OK, T.OWN })
        assert(not slotted.active and not plain.active, "neither stays up there: " .. T.dump())
        assert(S.findAnys == asked, "nothing asked of UIA: " .. (S.findAnys - asked))
        assert(T.count("[deactivate] 'On a slot' — in front now: 'Automation Platform' (automation-platform.exe)") == 1
          and T.count("[deactivate] 'On its own' — in front now: 'Automation Platform' (automation-platform.exe)") == 1,
          T.dump())
        assert(not T.holds("Tab") and not T.holds("Return"), "no key held there")
        -- A focus change inside the dialog, and time passing: still nothing asked.
        T.show(T.OWN, { T.OWN })
        T.tick(3)
        assert(S.findAnys == asked and S.finds == finds, "asked: " .. (S.findAnys - asked))
        assert(not slotted.active and not plain.active)
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(slotted.active and plain.active, "back over REAPER's FX window: " .. T.dump())
        assert(S.findAnys > asked, "and the gates asked again there")
    "##);
}

/// Two overlays on one slot, both bound by class alone to any `#32770`. The more specific one —
/// what a plug-in has loaded, here only once its gate sees it — is bound first, so its recheck runs
/// first; the base one came up over REAPER's FX window before it and has been outranked since,
/// skipping its rechecks, so its last report still says it matches. `front` is how the application
/// takes the keyboard.
fn elected_meanwhile(front: &str) {
    run(&format!(
        r##"
        local S = T.S
        T.electing()
        local function overlay(label, specificity, gate)
          local o = T.O.new(label)
          o:addCustomButton({{ label = "OK", onActivate = function() end }})
          if gate then o:gate(gate) end
          o:attach({{ class = "#32770" }}, {{ slot = "com.example.box", specificity = specificity }})
          return o
        end
        S.front, S.chain = T.FX, {{ T.KK, T.WRAP, T.FX }}
        T.turn()
        S.loaded = false
        local content = overlay("Content", 2, function() return S.loaded end)
        local base = overlay("Base", 1)
        T.runDue()
        assert(base.active and not content.active, "the base one up first: " .. T.dump())
        S.loaded = true
        T.show(T.FX, {{ T.KK, T.WRAP, T.FX }})
        assert(content.active and not base.active, "outranked now: " .. T.dump())
        local ups = 0
        local up = base._activate
        base._activate = function(self) ups += 1; return up(self) end
        {front}
        assert(not content.active and not base.active, "neither stays up there: " .. T.dump())
        assert(ups == 0, "the base one, elected on its report from REAPER, did not come up: " .. T.dump())
        assert(not T.holds("Tab") and not T.holds("Return"), "no key held there")
        T.show(T.FX, {{ T.KK, T.WRAP, T.FX }})
        assert(content.active and not base.active, "back over REAPER's FX window: " .. T.dump())
    "##
    ));
}

/// The application's own dialog comes to the front: the winner lets go at its recheck, and the
/// arbiter elects the base one on its report from REAPER before the base one's own recheck has
/// run. Called to come up, it reports that it does not match instead.
#[test]
fn an_overlay_elected_on_a_report_from_another_window_does_not_come_up_in_ours() {
    elected_meanwhile("T.show(T.OWN, { T.OK, T.OWN })");
}

/// The tray icon puts a hidden window of the application in front for its menu, which `active()`
/// does not report: no recheck sees one of our windows, and the winner lets go because nothing is
/// in front. The base one, elected then, reads `host.window.foreground()`, whose `own` says the
/// keyboard is ours, and does not come up either.
#[test]
fn an_overlay_elected_while_the_trays_hidden_window_has_the_keyboard_does_not_come_up() {
    elected_meanwhile("S.front = T.OWN; T.hide(T.OWN); T.event()");
}
