-- The two waits of a task, host.ocr.recognize and host.ocr.recognizeMany: built once per VM by
-- the host (task.rs), and by the scripted test host from this same file
-- (overlay_menu_tests.rs), so the rules a scenario meets are the real ones.
--
-- A Rust function cannot yield through mlua, so the wait is this file's `yield`: the host's
-- `start` checks and queues the read, and the task's coroutine then stops here until the host
-- resumes it with the reading. Everything below is an upvalue, captured when the VM was built
-- and before any module code ran: a module that replaces coroutine.* or error changes nothing
-- here, and Luau's `debug` cannot reach another function's upvalues.
--
--   start(name, opts, nonce)  queues the read for the innermost running task of this VM, whose
--                             own coroutine must be the one calling; raises for a mistake in
--                             the call, at the wait
--   where_()                  "task": the innermost running task of this VM is calling, in its
--                             own coroutine; "foreign": one of its tasks runs, but another
--                             coroutine is calling; "none": no task of this VM runs
--   legacy(name, opts, why)   the old blocking call, for a place that cannot wait: true and its
--                             answer, or false and the message to raise there
--   disown()                  module code resumed the task's coroutine: the host forgets it
return function(start, where_, legacy, disown, yield, isyieldable, rawequal, error, WAIT, RESUMED)
  -- Level 1 is this function, 2 the call below, 3 whoever called host.ocr.recognize: a raise
  -- names the module's line.
  local function wait(name, opts)
    local w = where_()
    if w == "task" and not isyieldable() then
      -- A metamethod, a sort comparator, a gsub function, a for loop's iterator, an xpcall
      -- handler, or Lua the host itself called (an arbiter's onActivate, an onChange, an
      -- included file's top level): Luau cannot stop the task there.
      w = "cannot-wait"
    end
    if w ~= "task" then
      local ran, v = legacy(name, opts, w)
      if not ran then error(v, 3) end
      return v
    end
    local nonce = {} -- a fresh table: no module code ever holds it
    start(name, opts, nonce)
    local got, value = yield(WAIT, nonce)
    if not rawequal(got, nonce) then
      -- Not the host: module code resumed this coroutine itself. It is no task any more, and
      -- its read is withdrawn.
      disown()
      error(name .. RESUMED, 3)
    end
    return value
  end
  -- `local v = …; return v` rather than a tail call, so the wait's level 3 is the module's line.
  return {
    recognize = function(opts)
      local v = wait("host.ocr.recognize", opts)
      return v
    end,
    recognizeMany = function(opts)
      local v = wait("host.ocr.recognizeMany", opts)
      return v
    end,
  }
end
