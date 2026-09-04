-- Hotkey test A — half of a pair. Its twin is tools/hotkey-test-b.
--
-- WHAT THIS IS FOR. Only one module can hold a key combination at a time. Both halves ask
-- for the same one, so exactly one of them answers when it is pressed — and which one is
-- the answer to two questions nobody has been able to measure from the machine this was
-- written on, because both need a person at a keyboard:
--
--   1. Does the key PASS ON when its holder is disabled? Until 2026-09-03 it did not: a
--      registration that lost the contest was not even recorded, so there was nothing left
--      to hand it to, and only a restart brought it back.
--   2. Does a module KEEP its key across a reload? The first fix broke this: hotkey ids
--      only ever go up, so a reloaded module came back looking like the newest claim and
--      handed its own key away for the rest of the session.
--
-- HOW TO RUN IT (both halves, and nothing else):
--
--     .\run-dev.ps1 -Tools -Only hotkey-test-a,hotkey-test-b
--
-- Both speak one line as they load, so you know they are in. Then:
--
--   TEST 1 — the key passes on when the holder is disabled.
--     a) Press Ctrl+Alt+Win+F8.                  Expected: "A".
--        (A loads first, and the module that loaded first keeps the combination.)
--     b) Open the manager from the tray, Installed tab, untick "Hotkey test A".
--     c) Press Ctrl+Alt+Win+F8 again.            Expected: "B", with no restart.
--        FAILS AS: silence. That is the old behaviour.
--     d) Tick "Hotkey test A" again.
--     e) Press Ctrl+Alt+Win+F8.                  Expected: "B", UNCHANGED.
--        Whoever holds a combination keeps it; A coming back does not take it away, because
--        loading earlier is not a reason to interrupt a module that is using something. An
--        earlier draft of this list expected "A" here and was simply wrong about the rule it
--        was testing. (Known and undecided: this does not survive a restart — at start-up
--        nobody holds anything, so load order decides again and A has it back.)
--
--   TEST 2 — a module keeps its own key across a reload. This is the one I got wrong twice.
--     a) Press Ctrl+Alt+Win+F8.                  Expected: "A".
--     b) In the manager, select "Hotkey test A" and press the Reload button.
--     c) Press Ctrl+Alt+Win+F8.                  Expected: "A" — unchanged.
--        FAILS AS: "B". A reload would then be silently costing a module its hotkey.
--
-- A conflict dialog naming both modules is expected on load and after each toggle; it is
-- the platform saying which module has the combination, and it is not a failure.

local KEY = "Ctrl+Alt+Win+F8"
local ME = "A"

return {
  activate = function()
    -- The return is the registration's id whether or not this module wins the combination:
    -- a losing claim is recorded and honoured the moment the holder gives it up. Before the
    -- change it returned 0 here and the callback was dropped.
    local id = host.hotkey.register(KEY, function()
      -- One letter, because the question is only ever "which of the two answered".
      host.speech.output(ME)
    end)
    host.log.info(("[hotkey-test-%s] claimed %s, id %s"):format(ME, KEY, tostring(id)))
    host.speech.output("hotkey test " .. ME .. " loaded")
  end,
}
