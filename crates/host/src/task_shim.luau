-- host.ocr.recognize, built once per VM by the host (task.rs), and by the scripted test host from
-- the same Rust (overlay_menu_tests.rs), so the rules a scenario meets are the real ones.
--
-- One call, two forms. With a callback — the last argument a function, or exactly two arguments of
-- which the second is one — it asks the read service and returns at once (`submit`); it waits
-- nowhere and works anywhere. Without one it waits for the reading: in a handler, as a
-- `coroutine.yield` here, while the event loop goes on; where it cannot wait, it is the old
-- blocking call (`legacy`).
--
-- A Rust function cannot yield through mlua, so the wait is this file's `yield`: the host's
-- `start` checks and queues the read, and the handler's coroutine then stops here until the host
-- resumes it with the reading. Everything below is an upvalue, captured when the VM was built
-- and before any module code ran: a module that replaces coroutine.*, error, select or type
-- changes nothing here, and Luau's `debug` cannot reach another function's upvalues.
--
--   submit(what, opts, cb)    the callback form: checks the call and queues the read; raises for
--                             a mistake in it
--   start(what, opts, nonce)  queues the read for the innermost running handler of this VM,
--                             whose own coroutine must be the one calling; raises for a mistake
--                             in the call, at the wait
--   where_()                  "handler": the innermost running handler of this VM is calling, in
--                             its own coroutine; "foreign": one of its handlers runs, but another
--                             coroutine is calling; "none": no handler of it runs
--   legacy(what, opts, why)   the old blocking call, for a place that cannot wait: true and the
--                             reading (or the list and the table by name), or false and the
--                             message to raise there
--   disown()                  module code resumed the handler's coroutine: the host forgets it
return function(start, where_, legacy, disown, submit, yield, isyieldable, rawequal, error, select, type, WAIT,
                RESUMED, NIL_CB)
  -- Level 1 is this function, 2 the call below, 3 whoever called host.ocr.recognize: a raise
  -- names the module's line.
  local function wait(what, opts)
    local w = where_()
    if w == "handler" and not isyieldable() then
      -- A metamethod, a sort comparator, a gsub function, a for loop's iterator, an xpcall
      -- handler, or Lua the host itself called (an arbiter's onActivate, an onChange, an
      -- included file's top level): Luau cannot stop the handler there.
      w = "cannot-wait"
    end
    if w ~= "handler" then
      local ran, a, b = legacy(what, opts, w)
      if not ran then error(a, 3) end
      return a, b
    end
    local nonce = {} -- a fresh table: no module code ever holds it
    start(what, opts, nonce)
    local got, a, b = yield(WAIT, nonce)
    if not rawequal(got, nonce) then
      -- Not the host: module code resumed this coroutine itself. It is no handler any more, and
      -- its read is withdrawn.
      disown()
      error("host.ocr.recognize" .. RESUMED, 3)
    end
    return a, b
  end
  -- `local a, b = …; return a, b` rather than a tail call, so the wait's level 3 is the module's
  -- line.
  return {
    recognize = function(...)
      local n = select("#", ...)
      local what, opts, cb = ...
      if n == 2 and type(opts) == "function" then
        opts, cb, n = nil, opts, 3
      end
      if n >= 3 then
        if cb == nil then error(NIL_CB, 2) end
        submit(what, opts, cb)
        return
      end
      local a, b = wait(what, opts)
      return a, b
    end,
  }
end
