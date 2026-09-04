-- Hotkey test B — the other half of the pair. The procedure, and why the pair exists, is
-- written once, at the top of tools/hotkey-test-a/src/main.luau.
--
-- B loads second, so it is the one that WAITS: it holds a standing claim on the same
-- combination and takes it over the moment A gives it up. Hearing "B" is therefore the
-- interesting outcome in test 1 and the failing one in test 2.

local KEY = "Ctrl+Alt+Win+F8"
local ME = "B"

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
