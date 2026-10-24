-- host.window matcher prelude.
-- The native side (Rust) provides host.window.list()/apps()/active() and host.os.
-- This adds the declarative, OS-gated matcher find()/findAll() on top, using
-- Luau's native string patterns. See docs/window-matching.md.

local W = host.window

local function ci(s)
  return string.lower(s or "")
end

local PLATFORMS = { "windows", "macos", "linux" }

-- Is this table keyed by operating system rather than by match mode?
--
-- The two vocabularies are disjoint — `windows`/`macos`/`linux` against
-- `exact`/`contains`/`prefix`/`suffix`/`pattern`/`regex`/`not` — so one grammar can carry
-- both without a marker, and a module only reaches for the OS keys at the one field that
-- genuinely differs. Everything around it stays a single shared spelling.
local function os_keyed(t)
  for _, p in ipairs(PLATFORMS) do
    if t[p] ~= nil then return true end
  end
  return false
end

-- Resolves a possibly OS-keyed value to the one for the platform we are on.
--
-- Absent for this platform means NO MATCH, not "unconstrained" — the same rule the
-- matcher's platform blocks already follow. A field that named three systems and
-- accidentally matched everything on a fourth would be a Windows-only module quietly
-- claiming a Mac window.
local function for_this_os(v)
  if type(v) == "table" and os_keyed(v) then
    return v[host.os.current]
  end
  return v
end

-- Exposed for module code that has to branch outside a matcher — the control-class
-- pattern of an embedded plugin, mostly. `host.os.pick { windows = …, macos = … }`.
function host.os.pick(t)
  return for_this_os(t)
end

-- A field matcher is a string (case-insensitive equals) or a table with one of:
-- exact / contains / prefix / suffix / pattern / regex / not — or one keyed by OS,
-- carrying any of those per platform.
local function match_field(spec, actual)
  actual = actual or ""
  spec = for_this_os(spec)
  if spec == nil then return false end
  if type(spec) == "string" then
    return ci(actual) == ci(spec)
  elseif type(spec) == "table" then
    if spec["not"] ~= nil then return not match_field(spec["not"], actual) end
    if spec.exact ~= nil then return ci(actual) == ci(spec.exact) end
    if spec.contains ~= nil then return string.find(ci(actual), ci(spec.contains), 1, true) ~= nil end
    if spec.prefix ~= nil then return string.sub(ci(actual), 1, #spec.prefix) == ci(spec.prefix) end
    if spec.suffix ~= nil then return #spec.suffix == 0 or string.sub(ci(actual), -#spec.suffix) == ci(spec.suffix) end
    -- `pattern` and `regex` are both evaluated as Luau string patterns for now.
    if spec.pattern ~= nil then return string.find(actual, spec.pattern) ~= nil end
    if spec.regex ~= nil then return string.find(actual, spec.regex) ~= nil end
  end
  return false
end

local function match_app(spec, app)
  if spec.name ~= nil and not match_field(spec.name, app.name) then return false end
  if spec.exe ~= nil and not match_field(spec.exe, app.exe) then return false end
  -- The macOS identity. Empty on Windows, so a matcher that asks for it there does not
  -- match — which is why a cross-platform matcher asks for `name`, or puts `bundleId` in
  -- its `macos` block.
  if spec.bundleId ~= nil and not match_field(spec.bundleId, app.bundleId) then return false end
  if spec.pid ~= nil and app.pid ~= spec.pid then return false end
  return true
end

-- The accessibility identity of a macOS window, taken apart.
--
-- The backend publishes it as ONE string in `class`, `AXRole/AXSubrole/AXIdentifier`, with
-- both separators always present and a missing part empty — "AXWindow/AXStandardWindow/",
-- "AXGroup//NI.Kontakt.Main". It is one field because `class` is what every module's
-- detection already matches on, and a second field would have meant a second code path in
-- every module. Splitting it here lets a matcher name the part it cares about instead of
-- writing a pattern around the separators.
local function ax_parts(class)
  local role, sub, ident = string.match(class or "", "^([^/]*)/([^/]*)/(.*)$")
  return role, sub, ident
end

local function matches(m, win)
  -- OS gating: if the matcher names any platform blocks but not the current OS,
  -- it does not match here.
  local hasPlatform, currentPresent = false, false
  for _, p in ipairs(PLATFORMS) do
    if m[p] ~= nil then
      hasPlatform = true
      if p == host.os.current then currentPresent = true end
    end
  end
  if hasPlatform and not currentPresent then return false end

  -- Explicit gating, for a matcher that needs no platform block at all — everything it
  -- asks for is shared, and it simply is not meant to run everywhere.
  if m.os ~= nil then
    local allowed = false
    for _, p in ipairs(m.os) do
      if p == host.os.current then allowed = true end
    end
    if not allowed then return false end
  end

  -- Common fields.
  if m.title ~= nil and not match_field(m.title, win.title) then return false end
  if m.app ~= nil and not match_app(m.app, win.app) then return false end

  -- Current-OS block. It takes the same grammar as the top level, so identity that has no
  -- shared spelling — a Win32 class here, an accessibility role there — lives in exactly
  -- one table per platform and the rest of the matcher stays written once.
  local block = m[host.os.current]
  if block ~= nil then
    if block.title ~= nil and not match_field(block.title, win.title) then return false end
    if block.app ~= nil and not match_app(block.app, win.app) then return false end
    if block.class ~= nil and not match_field(block.class, win.class) then return false end
    if block.axRole ~= nil or block.axSubrole ~= nil or block.axIdentifier ~= nil then
      local role, sub, ident = ax_parts(win.class)
      if block.axRole ~= nil and not match_field(block.axRole, role) then return false end
      if block.axSubrole ~= nil and not match_field(block.axSubrole, sub) then return false end
      if block.axIdentifier ~= nil and not match_field(block.axIdentifier, ident) then
        return false
      end
    end
  end

  -- Escape hatch predicate, evaluated last.
  if type(m.where) == "function" and not m.where(win) then return false end

  return true
end

-- What is wrong with a matcher, in words, or nil. Every call that takes one — find, findAll,
-- test and onTrigger — raises with it at the caller's line, because the arguments alone decide
-- it (D2): a matcher is a declaration, and a mistake in one never fails in a way anybody would
-- guess from the matcher. A string matcher, or `app = "game.exe"` (a string where a table goes),
-- constrains nothing, so it matched every window; a misspelt key or a `class` at the top level was
-- read by nobody, so it constrained nothing either; a field matcher with a misspelt mode matched
-- nothing, without a word. So every key is one of the grammar's, at every level, and of its type.
-- The blocks for the other platforms are checked too, so that a mistake in one shows on whichever
-- platform the module is tried on. Plain table work, a few microseconds a matcher.
local TOP_KEYS = { "title", "app", "os", "windows", "macos", "linux", "where" }
local BLOCK_KEYS = { "title", "app", "class", "axRole", "axSubrole", "axIdentifier" }
local APP_KEYS = { "name", "exe", "bundleId", "pid" }
local MODES = { "not", "exact", "contains", "prefix", "suffix", "pattern", "regex" }

local function listed(keys)
  return table.concat(keys, ", ", 1, #keys - 1) .. " and " .. keys[#keys]
end

local function known(keys, k)
  return table.find(keys, k) ~= nil
end

-- A key in words: `'titel'`, `1`, or what it is when it is neither a string nor a number.
local function keyWords(k)
  if type(k) == "string" then return "'" .. k .. "'" end
  if type(k) == "number" then return tostring(k) end
  return "a " .. type(k) .. " key"
end

-- The first key of `t` that `keys` does not name, as the problem `<path> has no key …`.
local function unknownKey(t, keys, path, its)
  for k in pairs(t) do
    if type(k) ~= "string" or not known(keys, k) then
      return path .. " has no key " .. keyWords(k) .. "; " .. its .. listed(keys)
    end
  end
  return nil
end

local function typed(v, path, want)
  if v ~= nil and type(v) ~= want then
    return path .. " is a " .. want .. ", not a " .. type(v)
  end
  return nil
end

-- A field matcher: a string, or a table of one match mode — `not` holding a field matcher, the
-- others a string — or a table keyed by platform, each holding a field matcher that is not keyed
-- by platform again (`for_this_os` resolves one level).
local function fieldProblem(v, path, platformsAllowed)
  if type(v) == "string" then return nil end
  if type(v) ~= "table" then
    return path .. " is a string or a table, not a " .. type(v)
  end
  if platformsAllowed and os_keyed(v) then
    local bad = unknownKey(v, PLATFORMS, path, "a field matcher keyed by platform has ")
    if bad then return bad end
    for _, p in ipairs(PLATFORMS) do
      if v[p] ~= nil then
        local inner = fieldProblem(v[p], path .. "." .. p, false)
        if inner then return inner end
      end
    end
    return nil
  end
  local bad = unknownKey(v, MODES, path, "a field matcher's keys are ")
  if bad then return bad end
  if v["not"] ~= nil then
    local inner = fieldProblem(v["not"], path .. ".not", true)
    if inner then return inner end
  end
  for _, mode in ipairs(MODES) do
    if mode ~= "not" then
      local wrong = typed(v[mode], path .. "." .. mode, "string")
      if wrong then return wrong end
    end
  end
  return nil
end

local function appProblem(a, path)
  if type(a) ~= "table" then return path .. " is a table, not a " .. type(a) end
  local bad = unknownKey(a, APP_KEYS, path, "its keys are ")
  if bad then return bad end
  for _, k in ipairs({ "name", "exe", "bundleId" }) do
    if a[k] ~= nil then
      local inner = fieldProblem(a[k], path .. "." .. k, true)
      if inner then return inner end
    end
  end
  return typed(a.pid, path .. ".pid", "number")
end

local function blockProblem(b, path)
  if type(b) ~= "table" then return path .. " is a table, not a " .. type(b) end
  local bad = unknownKey(b, BLOCK_KEYS, path, "a platform block's keys are ")
  if bad then return bad end
  if b.app ~= nil then
    local inner = appProblem(b.app, path .. ".app")
    if inner then return inner end
  end
  for _, k in ipairs(BLOCK_KEYS) do
    if k ~= "app" and b[k] ~= nil then
      local inner = fieldProblem(b[k], path .. "." .. k, true)
      if inner then return inner end
    end
  end
  return nil
end

local function matcherProblem(m)
  if type(m) ~= "table" then return "the matcher is a table, not a " .. type(m) end
  -- AutoHotkey's ahk_class has a place of its own here, and it is not the top level.
  if m.class ~= nil then
    return "matcher has no key 'class'; a window class goes in the block of its platform: windows = { class = … }"
  end
  local bad = unknownKey(m, TOP_KEYS, "matcher", "its keys are ")
  if bad then return bad end
  if m.title ~= nil then
    local inner = fieldProblem(m.title, "matcher.title", true)
    if inner then return inner end
  end
  if m.app ~= nil then
    local inner = appProblem(m.app, "matcher.app")
    if inner then return inner end
  end
  if m.os ~= nil then
    if type(m.os) ~= "table" then return "matcher.os is a table, not a " .. type(m.os) end
    for k, p in pairs(m.os) do
      if type(k) ~= "number" then
        return "matcher.os has no key " .. keyWords(k) .. "; it is a list of platform names, { \"windows\", \"macos\" }"
      end
      local wrong = typed(p, "matcher.os[" .. k .. "]", "string")
      if wrong then return wrong end
    end
  end
  for _, p in ipairs(PLATFORMS) do
    if m[p] ~= nil then
      local inner = blockProblem(m[p], "matcher." .. p)
      if inner then return inner end
    end
  end
  return typed(m.where, "matcher.where", "function")
end

-- Raises with what is wrong with `m`, at the line that called `call`.
local function checkMatcher(m, call)
  local problem = matcherProblem(m)
  if problem then
    -- Level 3: this function, the call that checks, and then its caller.
    error(call .. ": " .. problem, 3)
  end
end

-- The windows a matcher could possibly match, listed from the applications it names and
-- from nobody else.
--
-- `W.list()` with no filter asks every running application for its windows, and on macOS
-- each of those is a call into another process with a timeout of its own; one that never
-- answers costs the whole timeout, on the thread that carries the keyboard. The third Mac
-- session measured two seconds of every three-second stall going exactly there, for a
-- matcher that had named its application all along. So the `app` clauses — the top-level one
-- and the one in this platform's block, both of which `matches` will test again — are run
-- over `W.apps()` first, which asks nobody anything, and only those applications' windows
-- are listed. A matcher with no `app` clause gets the full listing, as before; a matcher
-- whose application is not running gets an empty one without a single question asked.
local function candidates(m)
  local clauses = {}
  if m.app ~= nil then clauses[#clauses + 1] = m.app end
  local block = m[host.os.current]
  if block ~= nil and block.app ~= nil then clauses[#clauses + 1] = block.app end
  if #clauses == 0 then return W.list() end
  local pids = {}
  for _, app in ipairs(W.apps()) do
    local ok = true
    for _, spec in ipairs(clauses) do
      if not match_app(spec, app) then ok = false break end
    end
    if ok then pids[#pids + 1] = app.pid end
  end
  if #pids == 0 then return {} end
  return W.list({ pids = pids })
end

function W.find(m)
  checkMatcher(m, "host.window.find")
  for _, win in ipairs(candidates(m)) do
    if matches(m, win) then return win end
  end
  return nil
end

function W.findAll(m)
  checkMatcher(m, "host.window.findAll")
  local out = {}
  for _, win in ipairs(candidates(m)) do
    if matches(m, win) then
      out[#out + 1] = win
    end
  end
  return out
end

-- Tests whether a window matches a declarative matcher (used by host.overlay:attach). Checked at
-- every call, as find's is: a test is plain table work, and the check a few more lookups.
function W.test(matcher, win)
  checkMatcher(matcher, "host.window.test")
  if type(win) ~= "table" then
    error("host.window.test: the window is a table, not a " .. type(win), 2)
  end
  return matches(matcher, win)
end

-- Window triggers (auto-activation). The host calls W._dispatchActivate(win) on
-- every foreground change; here we run the matching registered callbacks.
local triggers = {}

-- Every trigger gets a number when it is registered, one more than the last. An activation
-- carries the newest number there was when it arrived (`_triggerSeq`): it may run later, after
-- an earlier event of a busy module, and a trigger that event registered does not get it — its
-- own initial report is still to come.
local triggerSeq = 0

local focusCbs = {}

-- `initial = true` also reports the window already in front — once, on the next tick, and not
-- from inside this call: a callback often uses locals the file assigns further down, and a
-- disabled module's callback must not run at all. The trigger is PRIMED here, and the host is
-- asked for a report (`_requestInitial`, which queues this VM's owner for the tick); the host
-- then calls `_dispatchInitial` with the window in front. A real activation dispatched first
-- un-primes it, so the same foreground is never reported twice.
--
-- `onTrigger(matcher, cb)` is the same as `onTrigger(matcher, nil, cb)`: `host.ocr.recognize`
-- lets its options be left out before a callback, and the two-argument form used to raise
-- "attempt to index function value" from in here. Everything a registration can get wrong raises HERE, at the caller's
-- line, and not from the first dispatch — a callback that is not a function used to be stored
-- and fail only when the window came forward, as an error in some later event.
function W.onTrigger(matcher, opts, cb)
  if cb == nil and type(opts) == "function" then
    opts, cb = nil, opts
  end
  -- nil is every window here, as `{}` is.
  if matcher ~= nil then
    checkMatcher(matcher, "host.window.onTrigger")
  end
  if opts ~= nil and type(opts) ~= "table" then
    error("host.window.onTrigger: opts is a table or nil, not a " .. type(opts), 2)
  end
  if type(cb) ~= "function" then
    error("host.window.onTrigger: the callback is a function, not a " .. type(cb), 2)
  end
  opts = opts or {}
  if opts.initial ~= nil and type(opts.initial) ~= "boolean" then
    error("host.window.onTrigger: initial is true or false, not a " .. type(opts.initial), 2)
  end
  triggerSeq += 1
  local t = {
    matcher = matcher or {},
    on = opts.on or "activate",
    cb = cb,
    initial = opts.initial == true,
    primed = false,
    seq = triggerSeq,
  }
  triggers[#triggers + 1] = t
  if t.initial then
    t.primed = true
    -- Absent only in a test's stand-in host.
    if W._requestInitial then W._requestInitial() end
  end
end

-- Focus-change callbacks: fired when the keyboard focus moves, including within
-- the same top-level window (used to detect focus entering an embedded plugin), each
-- handed the round's `info`. The callback is checked here for the reason onTrigger's is.
function W.onFocus(cb)
  if type(cb) ~= "function" then
    error("host.window.onFocus: the callback is a function, not a " .. type(cb), 2)
  end
  focusCbs[#focusCbs + 1] = cb
end

function W._hasTriggers()
  return #triggers > 0 or #focusCbs > 0
end

-- The number of the newest trigger, 0 with none: what an activation carries when it arrives.
function W._triggerSeq()
  return #triggers > 0 and triggerSeq or 0
end

-- Whether `_dispatchInitial(win, reprime)` would have anything to do: a trigger still waiting
-- for its report or, re-arming, any `initial` trigger at all. The host asks this before it
-- asks the system for the window in front, so enabling a module that never asked for a report
-- costs no foreground query.
function W._wantsInitial(reprime)
  for _, t in ipairs(triggers) do
    if t.primed or (reprime and t.initial) then
      return true
    end
  end
  return false
end

-- Both dispatches are TIMED, and report the slow ones with their own size.
--
-- This is the last unmeasured stretch of the activation path. The OS calls underneath are
-- now answered once per epoch (one question in four hundred reaches the system) and the
-- conversion to Lua costs nothing measurable, yet a window switch still blocks the pump
-- thread for 400 ms — which was past the point where Windows stopped waiting for our keyboard
-- hook while it shared this thread, and is still 400 ms that every captured key and hotkey
-- waits for its callback. So the cost is in here, and "here" is one callback per registered
-- overlay, in each module's own VM.
--
-- Reported per VM with a count, because the shape of the number is the diagnosis: one
-- expensive callback reads differently from a hundred cheap ones, and a single VM standing
-- out differently again from all nine sharing the load.
--
-- The time is the dispatch's own: what its module's handler spent parked at a wait meanwhile is
-- left out. `waited`, which the host passes, says how long that handler has been parked so far
-- (milliseconds); it is absent where nothing waits.
local SLOW_DISPATCH_MS = 40

local function parked(waited)
  return waited and waited() or 0
end

-- `beforeFirst`, when the host passes one, runs once before the first matching callback:
-- the host's preparation for the reads that callback is about to make (opening desktop
-- duplication for a module that reads through it — see capture_source.rs). Before it, not
-- after them all, because by then that first read has already paid for the opening.
--
-- `upto` is the newest trigger number when the activation arrived (`_triggerSeq`; nil: every
-- trigger): a trigger registered after that and before this dispatch — by an earlier event of
-- the module that ran first — is neither reached nor un-primed; one a callback of this very
-- dispatch registers is.
function W._dispatchActivate(win, beforeFirst, waited, upto)
  local started = host.now and host.now() or nil
  local parkedBefore = parked(waited)
  local fired = 0
  local atStart = triggerSeq
  upto = upto or atStart
  local function reached(t)
    return t.seq <= upto or t.seq > atStart
  end
  for _, t in ipairs(triggers) do
    -- This activation IS the report of what is in front now, for every trigger it reaches: one
    -- still waiting for its initial report has had it (see onTrigger).
    if reached(t) then t.primed = false end
  end
  for _, t in ipairs(triggers) do
    -- Again here, for a trigger a callback of this very dispatch registered: the loop reaches
    -- it and reports this window to it, so its initial report would be the same one twice.
    if reached(t) then
      t.primed = false
      if t.on == "activate" and matches(t.matcher, win) then
        fired = fired + 1
        if fired == 1 and beforeFirst then
          beforeFirst()
        end
        t.cb(win)
      end
    end
  end
  if started then
    local ms = host.now() - started - (parked(waited) - parkedBefore)
    if ms >= SLOW_DISPATCH_MS then
      host.log.info(string.format(
        "[dispatch] activate %d ms — %d of %d trigger(s) fired", ms, fired, #triggers
      ))
    end
  end
  return fired
end

-- The report `onTrigger { initial = true }` asked for, called by the host on the tick with the
-- window in front (`nil` when there is none). Every primed trigger is un-primed FIRST and only
-- then called, so one that raises cannot leave the rest primed for a later, different window:
-- as with an activation, an error skips the module's later triggers for this report. With
-- `reprime` — the module was just enabled — every `initial` trigger is primed again first.
-- `beforeFirst` is the host's preparation before the first matching callback, as in
-- `_dispatchActivate`, and `waited` the same. Returns how many callbacks ran.
function W._dispatchInitial(win, reprime, beforeFirst, waited)
  local started = host.now and host.now() or nil
  local parkedBefore = parked(waited)
  local due = {}
  for _, t in ipairs(triggers) do
    if reprime and t.initial then t.primed = true end
    if t.primed then
      t.primed = false
      due[#due + 1] = t
    end
  end
  local fired = 0
  if win ~= nil then
    for _, t in ipairs(due) do
      if t.on == "activate" and matches(t.matcher, win) then
        fired = fired + 1
        if fired == 1 and beforeFirst then
          beforeFirst()
        end
        t.cb(win)
      end
    end
  end
  if started then
    local ms = host.now() - started - (parked(waited) - parkedBefore)
    if ms >= SLOW_DISPATCH_MS then
      host.log.info(string.format(
        "[dispatch] initial %d ms — %d of %d trigger(s) fired", ms, fired, #due
      ))
    end
  end
  return fired
end

-- `info`: the round's — `entered`, where a module's host.window.recheck told of its own click.
-- One table for the round, handed to every callback of the VM.
function W._dispatchFocus(waited, info)
  local started = host.now and host.now() or nil
  local parkedBefore = parked(waited)
  info = info or {}
  for _, cb in ipairs(focusCbs) do
    cb(info)
  end
  if started then
    local ms = host.now() - started - (parked(waited) - parkedBefore)
    if ms >= SLOW_DISPATCH_MS then
      host.log.info(string.format(
        "[dispatch] focus %d ms — %d callback(s)", ms, #focusCbs
      ))
    end
  end
end
