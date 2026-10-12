-- The host's wait points, built once per VM by the host (task.rs), and by the scripted test host
-- from the same Rust (overlay_menu_tests.rs), so the rules a scenario meets are the real ones.
-- Loaded twice per VM, under the name of the library whose calls it carries: as `host.ocr` for
-- host.ocr.recognize, and as `host.screen` for the host.screen calls that wait, so a traceback
-- through a wait names the call's own library.
--
-- A call without a callback waits: in a handler, as a `coroutine.yield` here, while the event loop
-- goes on. Where it cannot wait, `recognize` raises the message for that place at the caller's
-- line; a host.screen call runs on the event loop instead (`blocking`), which the host logs.
-- `recognize` with a callback — the last argument a function, or exactly two arguments of which
-- the second is one — asks the read service and returns at once (`submit`).
--
-- A Rust function cannot yield through mlua, so the wait is this file's `yield`: the host's
-- `start` checks and queues what the call asks for, and the handler's coroutine then stops here
-- until the host resumes it with the answer. Everything below is an upvalue, captured when the VM
-- was built and before any module code ran: a module that replaces coroutine.*, error, select,
-- type or table.* changes nothing here, and Luau's `debug` cannot reach another function's
-- upvalues.
--
--   where_()          "handler": the innermost running handler of this VM is calling, in its own
--                     coroutine; "foreign": one of its handlers runs, but another coroutine is
--                     calling; "none": no handler of it runs
--   disown()          module code resumed the handler's coroutine: the host forgets it
--
-- The host resumes a wait with the nonce, then `true` and the answer's values, or `false` and the
-- message the call raises at the caller's line — an answer that could not be handed over.
--
-- recognize(start, submit, NIL_CB, CANNOT_WAIT, OWN_COROUTINE) builds host.ocr.recognize:
--   start(nonce, what, opts)  queues the read for the innermost running handler of this VM, whose
--                             own coroutine must be the one calling; raises for a mistake in the
--                             call, at the wait
--   submit(what, opts, cb)    the callback form: checks the call and queues the read; raises for a
--                             mistake in it
--   CANNOT_WAIT               what it raises where it cannot wait: outside every handler, in a
--                             function the host calls and waits for, or one Luau cannot suspend
--   OWN_COROUTINE             what it raises in a coroutine the module made
--
-- point(name, start, blocking) builds a host.screen call that waits:
--   start(nonce, ...)         queues what the call asks for, as `recognize`'s start does
--   blocking(why, ...)        the call made on the event loop, where it cannot wait: `why` is
--                             where_'s word for the place, or "cannot-wait"
return function(where_, disown, yield, isyieldable, rawequal, error, select, type, pack, unpack, WAIT, RESUMED)
  -- Where the calling coroutine is, as a wait asks it: "handler" only where Luau can stop it.
  local function place()
    local w = where_()
    if w == "handler" and not isyieldable() then
      -- A metamethod, a sort comparator, a gsub function, a for loop's iterator, an xpcall
      -- handler, or Lua the host itself called (an arbiter's onActivate, an onChange, an
      -- included file's top level): Luau cannot stop the handler there.
      w = "cannot-wait"
    end
    return w
  end

  -- Level 1 is this function, 2 the call below it, 3 whoever called the host's function: a raise
  -- names the module's line.
  local function wait(name, start, ...)
    local nonce = {} -- a fresh table: no module code ever holds it
    start(nonce, ...)
    local back = pack(yield(WAIT, nonce))
    if not rawequal(back[1], nonce) then
      -- Not the host: module code resumed this coroutine itself. It is no handler any more, and
      -- what it waited for is withdrawn.
      disown()
      error(name .. RESUMED, 3)
    end
    if not back[2] then error(back[3], 3) end
    return unpack(back, 3, back.n)
  end

  -- `local r = pack(…); return unpack(…)` rather than a tail call, so the wait's level 3 is the
  -- module's line.
  return {
    recognize = function(start, submit, NIL_CB, CANNOT_WAIT, OWN_COROUTINE)
      return function(...)
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
        local w = place()
        if w == "foreign" then error(OWN_COROUTINE, 2) end
        if w ~= "handler" then error(CANNOT_WAIT, 2) end
        local r = pack(wait("host.ocr.recognize", start, what, opts))
        return unpack(r, 1, r.n)
      end
    end,

    point = function(name, start, blocking)
      return function(...)
        local w = place()
        if w ~= "handler" then
          local r = pack(blocking(w, ...))
          return unpack(r, 1, r.n)
        end
        local r = pack(wait(name, start, ...))
        return unpack(r, 1, r.n)
      end
    end,
  }
end
