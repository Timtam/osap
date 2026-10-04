//! The overlay runtime under handlers (runtime 0.3.0, step B3 of b0-final.md), run for real against
//! the scripted host of `overlay_menu_tests.rs`, whose events go through the host's own mailbox and
//! whose tests' wait point (`T.waitPoint`) parks a handler as a read will once handlers wait.
//!
//! Every callback of a module is a handler of the host, one at a time per module; a hook the runtime
//! calls there may wait, and the module's other events wait behind it. So:
//!
//! - **The mark before the hooks.** A sentence notes where the overlay is before its control's
//!   `text`, `current` or `verticalName` runs, and is not said when the hook outlived it — the
//!   overlay left, its window changed — nor is an OCR button's click made. Nothing changed: said.
//! - **The scan in a pass, the sentence outside it** (H1 of the plan). The hooks asked on every scan
//!   — `when`, a relational `identify`, `present`, the menu tests — run in ONE coroutine of the
//!   runtime's own per scan, where a host wait cannot stop the handler; the `text` after the scan
//!   runs in the handler and waits there. A `recognize` in a `when` is "in a coroutine the module
//!   made".
//! - **A stored `identify` is asked in the handler** (M5). The overlay's origin is resolved before a
//!   pass; one reached inside a pass all the same is not asked there, not counted, not kept for the
//!   epoch, and asked on the module's next turn. The menu tick asks its tests in a pass, and
//!   rechecks an overlay holding its place outside it.
//! - **A binding is first evaluated on the next tick**, not inside the call, where nothing could
//!   wait; **`identify` and `present` are guarded**: a raise is no verdict, said once, and
//!   `_activate` is not left half done — nor by an origin that raises as the overlay comes up.
//!
//! And for the review of 2026-10-04: a module's overlays share its one key scope and flag — the
//! last to leave sets them back, each pins before it captures and releases before it unpins; a
//! hook that yields — a `when` alone, a `when` in a scan, a menu test, which fails alone and lets
//! the tick go on; the menu tick's one pass for every overlay; and a menu opened during a hook,
//! not one open before it, keeping the sentence.
//!
//! What the runtime reads before a press — a stepper's value only — is held to it on ON:EAR's tiles
//! in `overlay_scale_tests.rs`.

use super::overlay_focus_read_tests::FR;
use super::overlay_menu_tests::run_with;

/// A few more helpers, after the focus-read ones.
const HD: &str = r##"
local S = T.S

-- Tab or Return as the captured key delivers it, and what the mailbox made of it: "Ran", "Parked"
-- (its handler waits), "Queued" (the module was busy).
function T.key(spec)
  assert(S.holding[spec], spec .. " is not captured, so it would not reach the overlay")
  S.epoch += 1
  return T.call("key", S.captured[spec])
end

-- A window of an application with an overlay attached to its class, in front, the keyboard on it.
function T.win(id)
  return { id = id, title = "Synth " .. id, class = "SynthWnd", app = { pid = 77, exe = "synth.exe" },
    bounds = { x = 0, y = 0, w = 800, h = 600 }, client = { x = 0, y = 0, w = 800, h = 600 } }
end

-- An overlay over Komplete Kontrol's control in REAPER (the harness's FX window), bound for real
-- with `spec`'s identify and present, on KK's slot; not yet evaluated. With `opts.menus`, its first
-- control opens a menu (T.press presses it).
function T.kk(label, spec, opts)
  local o = T.O.new(label or "Plug-in")
  if opts and opts.menus then
    o:addCustomButton({ label = "Komplete Kontrol menu", hotkey = "Alt+M", opensMenu = true, onActivate = function() end })
  end
  o:addCustomButton({ label = "Preset", hotkey = "Alt+P", onActivate = function() end })
  spec.hosts = { T.HOST }
  spec.control = spec.control or "Qt%d+.-QWindowIcon"
  opts = opts or {}
  opts.slot = opts.slot or "com.platform.kontakt"
  o:attachEmbedded(spec, opts)
  return o
end

-- The keyboard in `kk` (default: KK) in REAPER's FX window, with no event. The plug-in's control is
-- on the focus chain only, so a binding's pattern meets it once per evaluation.
function T.inKK(kk)
  S.front, S.controls, S.chain = T.FX, { T.LIST, T.WRAP }, { kk or T.KK, T.WRAP, T.FX }
  T.turn()
end

-- Komplete Kontrol's control, made anew: the same class and place, another id.
function T.newKK(id)
  return { id = id, class = T.KK.class, bounds = T.KK.bounds, client = T.KK.client }
end
"##;

fn run(scenario: &str) {
    run_with("windows", &format!("{FR}\n{HD}"), scenario)
}

// ---------------------------------------------------------------------------------------------
// The mark before the hooks.
// ---------------------------------------------------------------------------------------------

/// Return on an OCR button whose `text` waits, and the arbiter takes the overlay out meanwhile —
/// a plain call, which reaches a waiting module: when the hook goes on, nothing is said, nothing is
/// read and nothing is clicked, and the log says why, in the `[read]` lines' words.
#[test]
fn a_waiting_text_then_a_deactivation_says_nothing_and_clicks_nothing() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addOCRButton({ label = "Bank", region = { 300, 20, 400, 36 },
            text = function() return T.waitPoint("text") end })
        end)
        o.focus = 3
        o:_syncNativeKeys()
        local said, reads = #S.speech, #S.reads
        assert(T.key("Return") == "Parked", "the press waits in the control's text")
        assert(T.waiting("text"))
        T.hostCall(o._deactivate, o)
        assert(not o.active)
        assert(T.release("text", "Bank 2"))
        assert(#S.speech == said, "nothing said: " .. T.said(said))
        assert(#S.reads == reads, "nothing read")
        assert(#S.clicks == 0, "nothing clicked")
        assert(T.count("[read] 'Bank' not spoken after its text: the overlay is no longer active") == 1, T.dump())
        assert(T.count("[read] 'Bank': not clicking — the overlay is no longer active") == 1, T.dump())
    "#);
}

/// An attached overlay — its window is whatever is in front — whose `text` waits while another
/// window of its application comes to the front: the window event waits behind the handler, and
/// the sentence, about the window the key was pressed in, is not said.
#[test]
fn an_attached_overlay_whose_window_changed_during_the_wait_says_nothing() {
    run(r#"
        local S = T.S
        local A, B = T.win(1), T.win(2)
        S.front, S.chain = A, { A }
        T.turn()
        local o = T.O.new("Synth")
        o:addCustomButton({ label = "Init", onActivate = function() end })
        o:addStaticText({ label = "Battery", text = function() return T.waitPoint("text") end })
        o:attach({ class = "SynthWnd" })
        T.runDue()
        assert(o.active and o:hwnd() == 1, T.dump())
        T.run(400)
        local said = #S.speech
        assert(S.speech[said].text == "Init, button", T.said(0))
        assert(T.key("Tab") == "Parked")
        S.front, S.chain = B, { B }
        T.event()
        assert(T.release("text", "80 percent"))
        assert(#S.speech == said, "nothing said: " .. T.said(said))
        assert(T.count("[read] 'Battery' not spoken after its text: the overlay is on another window now") == 1, T.dump())
        T.tick()
        assert(o.active and #S.speech == said, "the window event that waited says nothing either: " .. T.said(said))
    "#);
}

/// An embedded overlay whose `text` waits while nothing changes: the epoch turns, as every
/// delivery turns it, the origin is resolved again, and the sentence is said.
#[test]
fn an_embedded_overlay_with_nothing_changed_says_its_sentence_after_the_wait() {
    run(r#"
        local S = T.S
        T.inKK()
        local o = T.O.new("Synth")
        o:addCustomButton({ label = "Init", onActivate = function() end })
        o:addStaticText({ label = "Battery", text = function() return T.waitPoint("text") end })
        o:attachEmbedded({ hosts = { T.HOST }, control = "Qt%d+.-QWindowIcon" }, { slot = "com.platform.kontakt" })
        T.runDue()
        assert(o.active, T.dump())
        T.run(400)
        assert(T.key("Tab") == "Parked")
        T.turn()
        assert(T.release("text", "80 percent"))
        assert(T.last().text == "Battery, 80 percent", T.last().text)
        assert(T.count("not spoken after its") == 0, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The scan in a pass; the sentence in the handler (H1).
// ---------------------------------------------------------------------------------------------

/// A `text` reached through Tab runs in the Tab's handler, outside the scan's pass, and waits
/// there: the handler parks, and the sentence is said when the wait is answered.
#[test]
fn a_text_reached_through_a_tab_waits_in_the_handler_not_in_the_scan() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addStaticText({ label = "Battery", text = function()
            local _, inPass = T.O._passes()
            S.textInPass = inPass
            return T.waitPoint("text")
          end })
        end)
        o.focus = 2
        assert(T.key("Tab") == "Parked", "the Tab's handler waits in the text")
        assert(o.focus == 3 and S.textInPass == false, "the text runs outside the scan's pass")
        assert(T.release("text", "80 percent"))
        assert(T.last().text == "Battery, 80 percent", T.last().text)
    "#);
}

/// The same with a read: in a task of the tests' entry, whose `recognize` waits, a `text` reached
/// through focusNext waits as a handler does — the case is "handler", not "a coroutine the module
/// made" — and the reading is said once it comes.
#[test]
fn a_recognize_in_a_text_reached_through_focus_next_waits_as_the_handler() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addStaticText({ label = "Reading",
            text = function() return T.host.ocr.recognize({ region = { 9701, 5, 9731, 15 } }).text end })
        end)
        o.focus = 2
        local t = T.task.run(function() o:focusNext() end)
        assert(T.task.alive(t), "it waits: " .. T.dump())
        assert(o.focus == 3)
        T.settle()
        assert(not T.task.alive(t) and #S.taskErrors == 0, table.concat(S.taskErrors, " | "))
        assert(T.last().text == "Reading, 9701,5", T.last().text)
    "#);
}

/// A `when` that reads with `recognize` runs in the scan's pass, a coroutine of the runtime's own:
/// there it cannot wait, even where a handler could — the case the host calls "a coroutine the
/// module made" — and the control is hidden, with the line every failing `when` has.
#[test]
fn a_recognize_in_a_when_is_in_a_coroutine_the_module_made() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addStaticText({ label = "Peek",
            when = function() return T.host.ocr.recognize({ region = { 9702, 5, 9732, 15 } }).text == "x" end })
        end)
        o.focus = 2
        local t = T.task.run(function() o:focusNext() end)
        assert(not T.task.alive(t), "nothing waited")
        assert(o.focus == 1, "Peek hidden, the ring wrapped: " .. o.focus)
        assert(T.count("Synth: 'Peek' — its `when` failed (") == 1, T.dump())
        assert(T.count("host.ocr.recognize cannot wait in a coroutine the module made") == 1, T.dump())
    "#);
}

/// Twenty hidden controls in one scan: twenty `when`s asked, in ONE coroutine.
#[test]
fn twenty_whens_in_one_scan_are_one_coroutine() {
    run(r#"
        local S = T.S
        local asked = 0
        local o = T.fields(function(o)
          for i = 1, 20 do
            o:addStaticText({ label = "Hidden " .. i, when = function() asked += 1; return false end })
          end
        end)
        o.focus = 2
        local before = T.O._passes()
        assert(T.key("Tab") == "Ran")
        local after, inPass = T.O._passes()
        assert(o.focus == 1, "past the twenty to the first control: " .. o.focus)
        assert(asked == 20, "every when asked once: " .. asked)
        assert(after - before == 1 and not inPass, "one coroutine: " .. (after - before))
    "#);
}

// ---------------------------------------------------------------------------------------------
// A stored identify is asked in the handler (M5).
// ---------------------------------------------------------------------------------------------

/// Komplete Kontrol makes its control anew while its overlay is in front, and before any event of
/// that reaches the module, another overlay's `when` asks for this one's origin inside its scan —
/// where the stored `identify` for the new control would have to wait: it is not asked there,
/// nothing raises, and the scan goes on. In the same epoch, outside a pass, the origin is resolved
/// again rather than found empty, and the identify waits in that handler; the recheck the overlay
/// asked for finds the verdict kept.
#[test]
fn a_stored_identify_reached_inside_a_scan_is_asked_in_the_next_handler() {
    run(r#"
        local S = T.S
        T.inKK()
        local asked = 0
        local A = T.kk("Plug-in", { identify = function(c)
          asked += 1
          if c.id == 77 then T.waitPoint("identify") end
          return true
        end })
        T.runDue()
        assert(A.active and asked == 1, T.dump())
        local B = T.fields(function(o)
          o:addStaticText({ label = "Peek", when = function() return A:origin() ~= nil end })
        end)
        T.inKK(T.newKK(77))
        B.focus = 2
        local due = #S.after
        assert(T.key("Tab") == "Ran", "nothing waited in the scan")
        assert(B.focus == 1, "Peek not shown: the plug-in's origin is not known inside the scan")
        assert(asked == 1, "the stored identify is not asked inside the scan")
        assert(T.count("attachEmbedded: [" .. T.KK.class .. "] id=77, identify is not asked inside a scan "
          .. "of the overlay's hooks — 'Plug-in' looks again on its next turn") == 1, T.dump())
        assert(T.count("its `when` failed") == 0 and #S.taskErrors == 0, T.dump())
        assert(#S.after == due + 1, "one recheck asked for")
        assert(T.call("timer", function() got = A:origin() end) == "Parked",
          "the same epoch, outside a pass: asked, and waiting in the handler")
        assert(asked == 2 and T.waiting("identify"))
        assert(T.release("identify"))
        assert(got and got.id == 77, "the plug-in's new control")
        T.runDue()
        assert(A.active and asked == 2, "the next turn's recheck, with the verdict kept: " .. T.dump())
    "#);
}

/// An overlay holding its place over its own menu, whose menu goes while Komplete Kontrol makes its
/// control anew and takes the keyboard back with no event yet: the tick that sees the menu closed
/// rechecks the overlay outside its pass, so the stored `identify` for the new control is asked in
/// the tick's handler and waits there — and the hold ends with the plug-in's keyboard.
#[test]
fn the_menu_tick_rechecks_a_held_overlay_outside_its_pass() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        T.inKK()
        local asked, waits = 0, false
        local o = T.kk("Komplete Kontrol", { identify = function()
          asked += 1
          if waits then T.waitPoint("identify") end
          return true
        end }, { menus = { T.O.menuTests.nativePopup, T.O.menuTests.newWindow } })
        T.runDue()
        assert(o.active and asked == 1, T.dump())
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        assert(o.active and o:_heldOnMenu(), T.dump())
        S.windows = { base }
        T.inKK(T.newKK(77))
        waits = true
        local status
        for _ = 1, 4 do
          S.now += 150
          status = T.call("timer", S.every)
          if status == "Parked" then break end
        end
        assert(status == "Parked", "the tick's recheck waits in the stored identify: " .. tostring(status) .. "\n" .. T.dump())
        assert(asked == 2 and T.waiting("identify"), "asked once, for the new control: " .. asked)
        assert(T.count("identify is not asked inside a scan") == 0, T.dump())
        assert(T.release("identify"))
        assert(o.active and not o:_heldOnMenu(), T.dump())
        assert(T.count("the plug-in has the keyboard again") == 1, T.dump())
        assert(o:origin().id == 77)
    "#);
}

// ---------------------------------------------------------------------------------------------
// The first evaluation on the next tick; identify and present guarded.
// ---------------------------------------------------------------------------------------------

/// A binding made at a module's top level — a plain call, where nothing can wait — asks nothing as
/// it binds: its first evaluation, `identify` included, runs on the module's next turn.
#[test]
fn a_binding_at_load_asks_no_identify_until_the_first_tick() {
    run(r#"
        local S = T.S
        T.inKK()
        local asked, o = 0, nil
        T.hostCall(function()
          o = T.kk("Plug-in", { identify = function() asked += 1; return true end })
        end)
        assert(asked == 0 and not o.active, "nothing asked as it binds")
        assert(#S.after == 1 and S.after[1].at == S.now, "its first evaluation, on the next turn")
        T.runDue()
        assert(asked == 1 and o.active, T.dump())
    "#);
}

/// A stored `identify` that raises is no verdict: "could not tell yet", asked again at every
/// recheck and not counted towards the eight, said once. One that answers later brings the overlay
/// up. A relational one that raises is "no" for that evaluation, said once per control.
#[test]
fn an_identify_that_raises_is_no_verdict_and_is_said_once() {
    run(r#"
        local S = T.S
        T.inKK()
        local asked, fails = 0, true
        local o = T.kk("Plug-in", { identify = function()
          asked += 1
          if fails then error("no tree yet") end
          return true
        end })
        T.runDue()
        assert(asked == 1 and not o.active)
        for _ = 1, 10 do T.event() end
        assert(asked == 11 and not o.active, "asked at every recheck, not counted towards eight: " .. asked)
        assert(T.count("identify raised: ") == 1, T.dump())
        assert(T.count("no tree yet — taken as could not tell yet, and not counted — 'Plug-in'") == 1, T.dump())
        assert(T.count("identify could not tell") == 0, T.dump())
        fails = false
        T.event()
        assert(o.active and asked == 12, T.dump())
    "#);
    run(r#"
        local S = T.S
        T.inKK()
        local asked, fails = 0, true
        local o = T.kk("Relational", { cacheIdentity = false, identify = function()
          asked += 1
          if fails then error("no host around it") end
          return true
        end })
        T.runDue()
        T.event()
        T.event()
        assert(not o.active and asked >= 3, asked)
        assert(T.count("no host around it — taken as no this time — 'Relational'") == 1, T.dump())
        fails = false
        T.event()
        assert(o.active, T.dump())
    "#);
}

/// The arbiter brings an overlay to the front in an epoch whose origin is not resolved yet, and its
/// stored `identify` raises there: the overlay comes up whole — its keys taken, its scope its
/// window — rather than active and holding nothing, and the raise is said once.
#[test]
fn an_activation_with_an_identify_that_raises_is_not_left_half_done() {
    run(r#"
        local S = T.S
        T.inKK()
        local o = T.kk("Plug-in", { identify = function(c)
          if c.id == 78 then error("its tree went away") end
          return true
        end })
        T.runDue()
        assert(o.active)
        T.hostCall(o._deactivate, o)
        assert(not o.active and not T.holds("Tab"))
        T.inKK(T.newKK(78))
        T.hostCall(o._activate, o)
        assert(o.active and T.holds("Tab") and T.holds("Return"), "its keys: " .. T.dump())
        assert(T.hotkeys() >= 1 and S.scopedTo == T.FX.id, "its hotkey and its scope")
        assert(T.count("its tree went away — taken as could not tell yet, and not counted") == 1, T.dump())
    "#);
}

/// A binding's `present` that raises is taken as not matching, said once per binding, and the
/// recheck that asked it goes on to its end: the overlay leaves the front, and comes back once
/// `present` answers again.
#[test]
fn a_present_that_raises_does_not_match_and_the_recheck_goes_on() {
    run(r#"
        local S = T.S
        T.inKK()
        local raise = false
        local o = T.kk("Library", { present = function()
          if raise then error("half drawn") end
          return true
        end })
        T.runDue()
        assert(o.active, T.dump())
        raise = true
        T.event()
        assert(not o.active, "not matching, and the recheck said so to the arbiter: " .. T.dump())
        T.event()
        T.event()
        assert(T.count("attachEmbedded: 'Library': its binding's present raised: ") == 1, T.dump())
        assert(T.count("half drawn — taken as not matching") == 1, T.dump())
        assert(#S.taskErrors == 0, table.concat(S.taskErrors, " | "))
        raise = false
        T.event()
        assert(o.active, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The module's key scope, shared by its overlays (the review of 2026-10-04).
// ---------------------------------------------------------------------------------------------

/// Two overlays of one module on two windows, bound in that order — Komplete Kontrol's standalone
/// overlay and its Preferences overlay. Coming back from Preferences, the standalone overlay comes
/// up first and pins the module's scope to its window; the Preferences overlay leaving after it
/// leaves that pin, rather than set the scope back to every window. And each pins before it
/// captures, and releases before it unpins: no moment in which its keys are taken in every window.
#[test]
fn a_modules_overlays_share_its_scope_and_the_last_to_leave_sets_it_back() {
    run(r#"
        local S = T.S
        local MAIN, PREFS = T.win(1), T.win(2)
        PREFS.class = "PrefsWnd"
        S.front, S.chain = MAIN, { MAIN }
        T.turn()
        local sa = T.O.new("Standalone")
        sa:addCustomButton({ label = "Browse", onActivate = function() end })
        sa:attach({ class = "SynthWnd" }, { slot = "test.standalone" })
        local prefs = T.O.new("Preferences")
        prefs:addCustomButton({ label = "OK", onActivate = function() end })
        prefs:attach({ class = "PrefsWnd" })
        -- Every scope call, with how many keys each overlay holds at it.
        local calls = {}
        local scope = T.host.keys.scope
        rawset(T.host.keys, "scope", function(on)
          calls[#calls + 1] = ("%s sa=%d prefs=%d"):format(on and "pin" or "unpin",
            #(sa.capturedKeys or {}), #(prefs.capturedKeys or {}))
          scope(on)
        end)
        T.runDue()
        assert(sa.active and not prefs.active and S.scopedTo == MAIN.id, T.dump())
        T.show(PREFS)
        assert(not sa.active and prefs.active and S.scopedTo == PREFS.id, T.dump())
        T.show(MAIN)
        assert(sa.active and not prefs.active, T.dump())
        assert(S.scopedTo == MAIN.id, "the standalone overlay's pin stands: " .. tostring(S.scopedTo))
        assert(T.holds("Tab") and #prefs.capturedKeys == 0 and #sa.capturedKeys > 0)
        assert(#calls == 4, table.concat(calls, " | "))
        assert(calls[1] == "pin sa=0 prefs=0" and calls[2] == "unpin sa=0 prefs=0" and calls[3] == "pin sa=0 prefs=0",
          table.concat(calls, " | "))
        local held = tonumber(string.match(calls[4], "^pin sa=0 prefs=(%d+)$") or "")
        assert(held and held > 0, "pinned before it captured, while the Preferences overlay still held its keys: "
          .. table.concat(calls, " | "))
        assert(T.open() == false, "the flag is what the overlay still in front says")
        -- The last to leave sets it back.
        T.show(T.OTHER)
        assert(not sa.active and not prefs.active and S.scopedTo == false, T.dump())
        assert(calls[#calls] == "unpin sa=0 prefs=0", calls[#calls])
    "#);
}

// ---------------------------------------------------------------------------------------------
// Coming to the front with an origin that raises; hooks that yield.
// ---------------------------------------------------------------------------------------------

/// The overlay's origin raises as it comes to the front — a module's `control` function, say: the
/// raise is said, and the overlay comes up whole, its keys taken and its scope its window, as on a
/// window whose handle it does not know — not stopped half way by the same raise one call later.
#[test]
fn an_origin_that_raises_as_the_overlay_comes_to_the_front_is_said_and_it_comes_up_whole() {
    run(r#"
        local S = T.S
        local o = T.fields()
        T.hostCall(o._deactivate, o)
        assert(not o.active and not T.holds("Tab"))
        o.activeCtx = { origin = function() error("the control went away", 0) end }
        T.hostCall(o._activate, o)
        assert(o.active and T.holds("Tab") and T.holds("Return"), "its keys: " .. T.dump())
        assert(S.scopedTo == T.FX.id, "its scope")
        assert(T.count("[overlay] Synth: its origin raised as it came to the front: the control went away") == 1, T.dump())
        assert(o:origin() == nil, "unknown for the rest of the epoch")
        T.turn()
        assert(not pcall(o.origin, o), "asked again in the next")
    "#);
}

/// A `when` of its own that yields: asked on its own — the `when` of the control Return presses —
/// it is ended, and hides its control as a `when` that raises does, said once; asked in a Tab's
/// scan, the yield reaches past that guard and the scan ends with the error, and with it the Tab.
#[test]
fn a_when_that_yields_hides_its_control_alone_and_ends_a_scan() {
    run(r#"
        local S = T.S
        local o = T.fields(function(o)
          o:addCustomButton({ label = "Peek", when = function() coroutine.yield(); return true end,
            onActivate = function() S.peeked = true end })
        end)
        o.focus = 3
        o:_syncNativeKeys()
        assert(T.key("Return") == "Ran")
        assert(not S.peeked and T.last().text == "Peek is not available now", T.said(0))
        assert(T.count("Synth: 'Peek' — its `when` failed (a hook asked on every scan yielded; it must return at once)") == 1, T.dump())
        S.tasksMayRaise = true
        o.focus = 2
        local said = #S.speech
        assert(T.key("Tab") == "Ran")
        assert(#S.taskErrors == 1 and string.find(S.taskErrors[1], "a hook asked on every scan yielded; it must return at once", 1, true),
          table.concat(S.taskErrors, " | "))
        assert(#S.speech == said, "the Tab ended with the scan, nothing said: " .. T.said(said))
    "#);
}

// ---------------------------------------------------------------------------------------------
// The menu tick: one pass for every overlay's tests; a test that yields fails alone.
// ---------------------------------------------------------------------------------------------

/// The tests of two overlays of one module are asked in one pass of the menu tick: one coroutine
/// for both, made by the tick.
#[test]
fn the_menu_tick_asks_every_overlays_tests_in_one_pass() {
    run(r#"
        local S = T.S
        local seen = {}
        local function test(name)
          return { name = name, cheap = true, test = function(_, answer)
            local made, inPass = T.O._passes()
            seen[#seen + 1] = { made = made, inPass = inPass }
            answer(false)
          end }
        end
        T.overlay({ test("first") }, "First")
        T.overlay({ test("second") }, "Second")
        local before = T.O._passes()
        T.tick()
        assert(#seen == 2, "both asked: " .. #seen)
        assert(seen[1].inPass and seen[2].inPass, "in a pass")
        assert(seen[1].made == before + 1 and seen[2].made == before + 1,
          "the same one: " .. seen[1].made .. ", " .. seen[2].made .. " after " .. before)
    "#);
}

/// A menu test that yields has failed, as one that raises has: said once, counted as not seeing a
/// menu — and the tick goes on rather than end with it. The tests after it on that tick are asked
/// on the next, by when the one that yielded is asked in a pass of its own, where it fails alone.
#[test]
fn a_menu_test_that_yields_fails_alone_and_the_tick_goes_on() {
    run(r#"
        local S = T.S
        local asked = 0
        local a = T.overlay({ { name = "yields", cheap = true, test = function() asked += 1; coroutine.yield() end } }, "First")
        local b = T.overlay({ { name = "sees", cheap = true, test = function(_, answer) answer(true) end } }, "Second")
        local from = #S.menuOpen
        T.tick()
        assert(asked == 1, asked)
        assert(T.count("[menu] 'First': menu test 'yields' failed: a hook asked on every scan yielded; it must return at once "
          .. "— counted as not seeing a menu") == 1, T.dump())
        assert(#S.taskErrors == 0, "the tick went on: " .. table.concat(S.taskErrors, " | "))
        assert(#S.menuOpen > from and T.open() == false, "the state applied on that tick: " .. T.history(from + 1))
        assert(not b:menuOpen(), "the second overlay's test is asked on the next tick")
        T.tick()
        assert(asked == 2, "asked again, in a pass of its own")
        assert(b:menuOpen() and T.open() == true, T.dump())
        T.tick()
        assert(asked == 3)
        assert(T.count("menu test 'yields' failed") == 1, "said once: " .. T.dump())
        assert(#S.taskErrors == 0, table.concat(S.taskErrors, " | "))
        assert(not a:menuOpen())
    "#);
}

// ---------------------------------------------------------------------------------------------
// After a hook, a menu opened since counts; one open before it does not.
// ---------------------------------------------------------------------------------------------

/// A sentence whose `text` waits: a menu that was open over the overlay before the hook does not
/// keep it from being said — it is no news after the hook — but one that opened while the hook
/// waited does, and the log says so.
#[test]
fn after_a_hook_only_a_menu_opened_since_keeps_the_sentence() {
    run(r#"
        local S = T.S
        local o = T.O.new("Synth")
        o:addCustomButton({ label = "Init", onActivate = function() end })
        o:addStaticText({ label = "Battery", text = function() return T.waitPoint("text") end })
        o:_watchMenus({ function(_, answer) answer(false) end })
        T.front(o)
        o:_menuSet(true, "a test")
        assert(o:menuOpen())
        o.focus = 1
        assert(T.key("Tab") == "Parked")
        assert(T.release("text", "80 percent"))
        assert(T.last().text == "Battery, 80 percent", T.said(0))
        o:_menuSet(false)
        o.focus = 1
        local said = #S.speech
        assert(T.key("Tab") == "Parked")
        T.hostCall(o._menuSet, o, true, "a test")
        assert(T.release("text", "90 percent"))
        assert(#S.speech == said, "nothing said: " .. T.said(said))
        assert(T.count("[read] 'Battery' not spoken after its text: a menu opened over the overlay since") == 1, T.dump())
    "#);
}
