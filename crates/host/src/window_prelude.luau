-- host.window matcher prelude.
-- The native side (Rust) provides host.window.list()/active() and host.os.
-- This adds the declarative, OS-gated matcher find()/findAll() on top, using
-- Luau's native string patterns. See docs/window-matching.md.

local W = host.window

local function ci(s)
  return string.lower(s or "")
end

-- A field matcher is a string (case-insensitive equals) or a table with one of:
-- exact / contains / prefix / suffix / pattern / regex / not.
local function match_field(spec, actual)
  actual = actual or ""
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
  if spec.pid ~= nil and app.pid ~= spec.pid then return false end
  return true
end

local PLATFORMS = { "windows", "macos", "linux" }

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

  -- Common fields.
  if m.title ~= nil and not match_field(m.title, win.title) then return false end
  if m.app ~= nil and not match_app(m.app, win.app) then return false end

  -- Current-OS block (e.g. windows = { class = ... }).
  local block = m[host.os.current]
  if block ~= nil then
    if block.class ~= nil and not match_field(block.class, win.class) then return false end
  end

  -- Escape hatch predicate, evaluated last.
  if type(m.where) == "function" and not m.where(win) then return false end

  return true
end

function W.find(m)
  for _, win in ipairs(W.list()) do
    if matches(m, win) then return win end
  end
  return nil
end

function W.findAll(m)
  local out = {}
  for _, win in ipairs(W.list()) do
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

function W.onTrigger(matcher, opts, cb)
  opts = opts or {}
  triggers[#triggers + 1] = { matcher = matcher or {}, on = opts.on or "activate", cb = cb }
end

function W._hasTriggers()
  return #triggers > 0
end

function W._dispatchActivate(win)
  for _, t in ipairs(triggers) do
    if t.on == "activate" and matches(t.matcher, win) then
      t.cb(win)
    end
  end
end
