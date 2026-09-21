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
  for _, win in ipairs(candidates(m)) do
    if matches(m, win) then return win end
  end
  return nil
end

function W.findAll(m)
  local out = {}
  for _, win in ipairs(candidates(m)) do
    if matches(m, win) then
      out[#out + 1] = win
    end
  end
  return out
end

-- Tests whether a window matches a declarative matcher (used by host.overlay:attach).
function W.test(matcher, win)
  return matches(matcher, win)
end

-- Window triggers (auto-activation). The host calls W._dispatchActivate(win) on
-- every foreground change; here we run the matching registered callbacks.
local triggers = {}

local focusCbs = {}

function W.onTrigger(matcher, opts, cb)
  opts = opts or {}
  triggers[#triggers + 1] = { matcher = matcher or {}, on = opts.on or "activate", cb = cb }
end

-- Focus-change callbacks: fired when the keyboard focus moves, including within
-- the same top-level window (used to detect focus entering an embedded plugin).
function W.onFocus(cb)
  focusCbs[#focusCbs + 1] = cb
end

function W._hasTriggers()
  return #triggers > 0 or #focusCbs > 0
end

-- Both dispatches are TIMED, and report the slow ones with their own size.
--
-- This is the last unmeasured stretch of the activation path. The OS calls underneath are
-- now answered once per epoch (one question in four hundred reaches the system) and the
-- conversion to Lua costs nothing measurable, yet a window switch still blocks the pump
-- thread for 400 ms — which is past the point where Windows stops waiting for our keyboard
-- hook and delivers the keystroke without us. So the cost is in here, and "here" is one
-- callback per registered overlay, in each module's own VM.
--
-- Reported per VM with a count, because the shape of the number is the diagnosis: one
-- expensive callback reads differently from a hundred cheap ones, and a single VM standing
-- out differently again from all nine sharing the load.
local SLOW_DISPATCH_MS = 40

-- `beforeFirst`, when the host passes one, runs once before the first matching callback:
-- the host's preparation for the reads that callback is about to make (opening desktop
-- duplication for a module that reads through it — see capture_source.rs). Before it, not
-- after them all, because by then that first read has already paid for the opening.
function W._dispatchActivate(win, beforeFirst)
  local started = host.now and host.now() or nil
  local fired = 0
  for _, t in ipairs(triggers) do
    if t.on == "activate" and matches(t.matcher, win) then
      fired = fired + 1
      if fired == 1 and beforeFirst then
        beforeFirst()
      end
      t.cb(win)
    end
  end
  if started then
    local ms = host.now() - started
    if ms >= SLOW_DISPATCH_MS then
      host.log.info(string.format(
        "[dispatch] activate %d ms — %d of %d trigger(s) fired", ms, fired, #triggers
      ))
    end
  end
end

function W._dispatchFocus()
  local started = host.now and host.now() or nil
  for _, cb in ipairs(focusCbs) do
    cb()
  end
  if started then
    local ms = host.now() - started
    if ms >= SLOW_DISPATCH_MS then
      host.log.info(string.format(
        "[dispatch] focus %d ms — %d callback(s)", ms, #focusCbs
      ))
    end
  end
end
