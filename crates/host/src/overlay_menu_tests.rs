//! The overlay runtime's menu tests, run for real against a scripted host.
//!
//! `modules/overlay-runtime/src/main.luau` is loaded the way the host loads a code module —
//! wrapped in `function(host)` — and handed a host written below in Luau. The clock is the
//! test's: `host.now()` answers what the scenario says, and the runtime's `host.timer.every`
//! callback is called one 150 ms tick at a time, so minutes of a menu run in microseconds and
//! every tick's decision can be looked at. What the runtime decides reaches the scripted host
//! as `host.keys.menuOpen(b)` calls, hotkey registrations, saved pictures and log lines, and
//! that is what the scenarios assert on.
//!
//! The scripted host is strict: a field it does not script raises, naming it, so a change that
//! makes the runtime reach for something new fails here loudly instead of passing on a nil.
//!
//! Written for the maintainer's decisions of 2026-09-26: a module names how its menus are seen
//! (`menus = { … }`), no timer decides whether a menu counts as open, one missed read is not a
//! close, and in a calibrating run a menu-opening control takes three pictures. And for the
//! review that followed: a test that has not answered holds up none of the others.
//!
//! And for the NVDA test of that phase: Komplete Kontrol's own menu in REAPER is a popup WINDOW
//! that comes to the front, and the overlay left the front with it and did not come back. Those
//! scenarios bind the overlay for real (`attachEmbedded` against a scripted REAPER FX window, its
//! focus chain and its controls) and drive it with window events, so the runtime's own context
//! match decides — and the menu shots, whose first picture no longer delays the click.
//!
//! And for the maintainer's decisions of 2026-09-27: the overlay holds its place, with no keys,
//! while its own menu's window is in front and a test sees the menu; when that ends, the ordinary
//! match decides, as for any other window change. One step goes beyond ReaHotkey: Komplete
//! Kontrol most likely only hides its menu's window (the likeliest reading of the diagnostic log,
//! not measured), and the keyboard can stay in that hidden window, so when
//! `host.window.foreground()` says the keyboard is still in the menu's own window and it is not
//! shown, the window of the press is brought back — once, through `host.window.focus`, its answer
//! logged and nothing announced; when it says anything else, that reading is logged instead.
//!
//! The scripted host answers as the real one does, which the first version of that step did not
//! model and was green only because of it (the review of 2026-09-27): a hidden window is never
//! `active()`; `active()` and `focusChain()` are kept for the epoch, which a window or focus event,
//! a captured key, a `timer.after` coming due, a snapshot's answer and `host.window.focus` turn
//! over and the menu tick does not (`T.turn` for anything else); on Windows the focus chain is
//! empty only when the foreground window is hidden or absent; and `foreground()` is read afresh.
//! And, after the review of that step: `host.window.focus` is accepted and completed later, when
//! the window's own thread gets to it, with its foreground event (`T.land`) — what Windows'
//! `SetForegroundWindow` most likely does for another process's window, not measured — unless a
//! scenario says it lands at once; `list()` lists titled windows only; and a `pollMatch` poll can
//! be run (`T.poll`), which turns nothing over either.

use mlua::{Function, Lua, Table};

const RUNTIME: &str = include_str!("../../../modules/overlay-runtime/src/main.luau");

/// The scripted host and the helpers the scenarios use, returned as the table `T`.
const HARNESS: &str = r##"
local T = {}

-- REAPER as the NVDA test of 2026-09-26 met it: the FX window, Komplete Kontrol's control inside
-- it in REAPER's wrapper, and REAPER's FX list to the left. The popup KK opens for its own menu
-- is a window of reaper.exe titled "Komplete Kontrol".
T.FX = { id = 1, title = 'FX: Track 1 "test"', class = "#32770",
  app = { pid = 4242, exe = "reaper.exe" },
  bounds = { x = 0, y = 0, w = 1000, h = 700 }, client = { x = 8, y = 30, w = 984, h = 662 } }
T.LIST = { id = 5, class = "SysListView32", bounds = { x = 8, y = 60, w = 84, h = 600 },
  client = { x = 8, y = 60, w = 84, h = 600 } }
T.WRAP = { id = 6, class = "reaperPluginHostWrapProc", bounds = { x = 96, y = 46, w = 808, h = 608 },
  client = { x = 96, y = 46, w = 808, h = 608 } }
T.KK = { id = 7, class = "Qt671NI_6_7_1_R3QWindowIcon{8d61}", bounds = { x = 100, y = 50, w = 800, h = 600 },
  client = { x = 100, y = 50, w = 800, h = 600 } }
T.POPUP = { id = 900, title = "Komplete Kontrol", class = "Qt671NI_6_7_1_R3QWindowPopupDropShadowSaveBits",
  app = { pid = 4242, exe = "reaper.exe" }, bounds = { x = 400, y = 180, w = 180, h = 220 },
  client = { x = 400, y = 180, w = 180, h = 220 } }
T.MAIN = { id = 100, title = "REAPER v7", class = "REAPERwnd", app = { pid = 4242, exe = "reaper.exe" },
  bounds = { x = 0, y = 0, w = 1600, h = 900 }, client = { x = 8, y = 30, w = 1584, h = 862 } }
T.OTHER = { id = 5000, title = "Browser", class = "Chrome_WidgetWin_1", app = { pid = 777, exe = "chrome.exe" },
  bounds = { x = 0, y = 0, w = 800, h = 600 }, client = { x = 0, y = 0, w = 800, h = 600 } }
-- REAPER as daw-hosts matches it, with its chrome classes.
T.HOST = { class = "#32770", chrome = { "^SysListView32", "^reaperPluginHostWrapProc", "^#32770$" } }

local S = {
  now = 1000,
  native = false,   -- host.keys.nativeMenuOpen()
  find = false,     -- host.element.find(id, "", Menu): a boolean, or function(id) -> boolean
  windows = { { id = 100, layer = 0, class = "REAPERwnd", x = 0, y = 0, w = 1600, h = 900 } },
  passed = {},      -- host.keys.passedThrough(), drained on read
  pixel = { r = 20, g = 20, b = 20 },
  focus = { class = "Qt661QWindowIcon{0}1", id = 7 },
  chain = nil,      -- the focus chain of a shown foreground window, when set; { S.focus } otherwise
  chainRaises = nil, -- a message host.window.focusChain raises with, when set
  -- The foreground window: the one that gets the keyboard. One with `hidden = true` (T.hide) is
  -- still the foreground window, but not shown. What host.window.active() and focusChain() make
  -- of it is below: the host's rules, not the scenario's.
  front = T.FX,
  busy = false,     -- on a Mac: the frontmost application is not answering
  remembered = nil, -- on a Mac: the window active() last reported fresh, served while busy
  listed = nil,     -- what host.window.list() lists from (default T.LISTED): shown, titled ones
  lists = 0,        -- host.window.list calls
  listPids = nil,   -- the `pids` the last host.window.list call named
  listRaises = nil, -- a message host.window.list raises with, when set
  fgAsks = 0,       -- host.window.foreground calls
  fgRaises = nil,   -- a message host.window.foreground raises with, when set
  kept = { epoch = -1 }, -- what active() and focusChain() answered in the current epoch
  focusCalls = {},  -- the id of every host.window.focus(id), in order
  focusAnswer = true, -- what host.window.focus answers
  focusLands = nil, -- function(id): what an accepted host.window.focus changes, when it lands
  focusAtOnce = false, -- it lands inside the call; otherwise it waits for T.land()
  landing = nil,    -- the id of an accepted host.window.focus that has not landed yet
  focusRaises = nil, -- a message host.window.focus raises with, when set
  polls = {},       -- ms -> the pollMatch poll's callback (host.timer.every at another interval)
  controls = {},    -- host.window.controls()
  triggers = {},    -- the callbacks host.window.onTrigger / onFocus were handed
  origin = {
    id = 7, class = "Qt661QWindowIcon{0}1", app = { pid = 4242 },
    client = { x = 100, y = 50, w = 800, h = 600 }, bounds = { x = 92, y = 19, w = 816, h = 639 },
  },
  calibrating = false,
  exists = {},      -- host.resource.exists(path)
  menuOpen = {},    -- every host.keys.menuOpen(b), in order
  logs = {},
  speech = {},
  shots = {},       -- { path, region, snapshot?, at } per saved picture
  snaps = 0,        -- host.screen.snapshot calls: a capture on the event loop
  asyncs = {},      -- host.screen.snapshotAsync requests not answered yet
  order = {},       -- "snapshot", "snapshotAsync", "save", "acted", in the order they happened
  captured = {},    -- spec -> callback, as host.keys.capture was last handed it
  holding = {},     -- spec -> token, while that capture has not been released
  tokens = {},      -- token -> spec
  hot = {},         -- hotkey id -> spec, while registered
  finds = 0,
  nativeAsks = 0,
  windowLists = 0,
  pixels = 0,
  registered = 0,
  unregistered = 0,
  nextId = 0,
  epoch = 0,
  every = nil,
  after = {},
}
T.S = S

local function strict(name, t)
  return setmetatable(t, { __index = function(_, k)
    error(("the scripted host has no %s.%s"):format(name, tostring(k)), 2)
  end })
end
T.strict = strict

local function nextId() S.nextId += 1; return S.nextId end

-- The platform the scripted host plays: Windows, unless the scenario runs through `run_mac`.
local function mac() return rawget(T.host.os, "current") == "macos" end

-- An answer the host keeps for the rest of the epoch, as the real one keeps active() and
-- focusChain(): asked once, and the same answer — nil included — until the epoch turns over.
local function kept(name, read)
  if S.kept.epoch ~= S.epoch then S.kept = { epoch = S.epoch } end
  local slot = S.kept[name]
  if slot == nil then
    slot = { value = read() }
    S.kept[name] = slot
  end
  return slot.value
end

-- A snapshot handle: the region it holds, when, and whether it was let go.
function T.snap(region)
  return { region = region, time = S.now, released = false,
    release = function(s) s.released = true end }
end

-- REAPER's titled, shown top-level windows and another application's: what host.window.list
-- finds, filtered by pid, unless a scenario sets S.listed.
T.LISTED = { T.MAIN, T.FX, T.OTHER }

T.host = strict("host", {
  now = function() return S.now end,
  -- The epoch as the host keeps it: turned over by a window or focus event, a captured key, a
  -- timer.after coming due, a snapshot's answer and host.window.focus — and NOT by the menu tick,
  -- a host.timer.every. See T.turn for everything else that turns it over.
  epoch = function() return S.epoch end,
  os = strict("host.os", {
    current = "windows",
    pick = function(v) if type(v) == "table" then return v[T.host.os.current] end return v end,
  }),
  path = function(p) return "C:/modules/overlay-runtime/" .. p end,
  log = strict("host.log", { info = function(s) S.logs[#S.logs + 1] = s end }),
  speech = strict("host.speech", { output = function(text, opts)
    S.speech[#S.speech + 1] = { text = text, interrupt = opts and opts.interrupt }
  end }),
  timer = strict("host.timer", {
    -- The menu tick every 150 ms, one per VM; any other interval is a pollMatch poll (joinPoll),
    -- one per interval, run by T.poll.
    every = function(ms, fn)
      if ms == 150 then
        assert(S.every == nil, "one menu timer per VM")
        S.every = fn
      else
        assert(S.polls[ms] == nil, "one poll timer per interval")
        S.polls[ms] = fn
      end
    end,
    after = function(ms, fn) S.after[#S.after + 1] = { at = S.now + ms, fn = fn } end,
  }),
  keys = strict("host.keys", {
    nativeMenuOpen = function() S.nativeAsks += 1; return S.native end,
    menuOpen = function(b) S.menuOpen[#S.menuOpen + 1] = b end,
    passedThrough = function() local p = S.passed; S.passed = {}; return p end,
    normalize = function(spec) return spec end,
    describe = function(spec) return spec end,
    check = function() return { reasons = {} } end,
    capture = function(spec, fn)
      local id = nextId()
      S.captured[spec] = fn
      S.holding[spec] = id
      S.tokens[id] = spec
      return id
    end,
    release = function(id)
      local spec = S.tokens[id]
      if spec and S.holding[spec] == id then S.holding[spec] = nil end
      S.tokens[id] = nil
    end,
    -- The window the hook scopes captured keys to (the one in front at the call), or false.
    scope = function(on) S.scopedTo = on and S.front and S.front.id or false end,
  }),
  element = strict("host.element", {
    type = { Menu = 50009, MenuItem = 50011, Button = 50000, Pane = 50033, Window = 50032 },
    find = function(id, name, ctype)
      assert(name == "" and ctype == 50009, "only the menu idiom is scripted")
      S.finds += 1
      if type(S.find) == "function" then return S.find(id) end
      return S.find
    end,
  }),
  window = strict("host.window", {
    -- The foreground window as the host reports it: read once per epoch and kept — a tick does
    -- not turn the epoch over, so a window that hid since then is still reported until something
    -- does — and nil for a window that is not shown (Windows' `window_info`). On a Mac, an
    -- application that does not answer is served the window it last reported.
    active = function()
      return kept("active", function()
        if mac() and S.busy then return S.remembered end
        local w = S.front
        if w and w.hidden then w = nil end
        S.remembered = w
        return w
      end)
    end,
    windowsOf = function(pid)
      assert(pid == 4242, "the plug-in's process")
      S.windowLists += 1
      return S.windows
    end,
    -- Kept for the epoch like active(). On Windows the chain falls back to the foreground window
    -- itself and drops every link that is not visible, so it is empty only when the foreground
    -- window is hidden or absent. On a Mac it is also empty while the application does not answer.
    focusChain = function()
      if S.chainRaises then error(S.chainRaises, 0) end
      return kept("chain", function()
        local w = S.front
        if not w or w.hidden or (mac() and S.busy) then return {} end
        local c = S.chain or { S.focus }
        assert(#c > 0 or mac(), "the scenario scripts an empty focus chain for a shown window, "
          .. "which Windows never reports")
        return c
      end)
    end,
    -- The window that gets the keyboard, shown or not, read afresh on every call — never kept.
    -- nil with nothing in front, and on a Mac while the application does not answer.
    foreground = function()
      S.fgAsks += 1
      if S.fgRaises then error(S.fgRaises, 0) end
      if mac() and S.busy then return nil end
      local w = S.front
      if not w then return nil end
      return { id = w.id, pid = w.app and w.app.pid, shown = not w.hidden }
    end,
    -- The titled, shown top-level windows, of the given processes only when `pids` is given: an
    -- untitled window is not listed, although active() reports it.
    list = function(filter)
      S.lists += 1
      local pids = filter and filter.pids
      S.listPids = pids
      if S.listRaises then error(S.listRaises, 0) end
      local out = {}
      for _, w in ipairs(S.listed or T.LISTED) do
        if not w.hidden and (w.title or "") ~= ""
          and (pids == nil or table.find(pids, w.app.pid) ~= nil) then
          out[#out + 1] = w
        end
      end
      return out
    end,
    -- Asks for window `id` to be brought to the front, and turns the epoch over, as the host
    -- does. Accepted (S.focusAnswer), the change the scenario scripts in S.focusLands is made when
    -- Windows makes it: SetForegroundWindow on another process's window most likely returns once
    -- the request is accepted, and the window becomes the foreground window when its own thread
    -- gets to it, with its foreground event after — so by default nothing has changed when the
    -- call returns, and T.land() plays the change and its event. S.focusAtOnce makes it inside
    -- the call, with no event: the less likely case. Declined, nothing changes.
    focus = function(id)
      S.focusCalls[#S.focusCalls + 1] = id
      if S.focusRaises then error(S.focusRaises, 0) end
      if S.focusAnswer and S.focusLands then
        if S.focusAtOnce then S.focusLands(id) else S.landing = id end
      end
      S.epoch += 1
      return S.focusAnswer
    end,
    controls = function() return S.controls end,
    -- The matchers these scenarios bind with name a window class and nothing else.
    test = function(m, w) return w ~= nil and m.class ~= nil and w.class == m.class end,
    onTrigger = function(_, _, cb) S.triggers[#S.triggers + 1] = cb end,
    onFocus = function(cb) S.triggers[#S.triggers + 1] = cb end,
  }),
  hotkey = strict("host.hotkey", {
    register = function(spec)
      S.registered += 1
      local id = nextId()
      S.hot[id] = spec
      return id
    end,
    unregister = function(id)
      S.unregistered += 1
      S.hot[id] = nil
    end,
  }),
  screen = strict("host.screen", {
    pixel = function() S.pixels += 1; return S.pixel end,
    saveMarked = function(path, opts)
      S.order[#S.order + 1] = "save"
      S.shots[#S.shots + 1] = { path = path, region = opts.region, marks = #opts.marks, at = S.now }
      return true
    end,
    -- One capture on the event loop, kept.
    snapshot = function(opts)
      S.snaps += 1
      S.order[#S.order + 1] = "snapshot"
      return T.snap(opts.region)
    end,
    -- Taken off the event loop at `at` (or at once) and answered on the tick after: see T.runDue.
    snapshotAsync = function(opts, cb)
      S.asyncs[#S.asyncs + 1] = { region = opts.region, at = opts.at or S.now, cb = cb, asked = S.now }
    end,
    save = function(path, opts)
      S.order[#S.order + 1] = "save"
      local s = opts and opts.snapshot
      assert(s == nil or not s.released, "a released snapshot was saved")
      S.shots[#S.shots + 1] = { path = path, snapshot = s, region = s and s.region or opts.region,
        taken = s and s.time, at = S.now }
      return true
    end,
  }),
  resource = strict("host.resource", { exists = function(p) return S.exists[p] == true end }),
  -- An arbiter with the one rule these scenarios need: a claim that matches is activated, one that
  -- stops matching is deactivated. One claimant per slot, so nothing is outranked.
  arbiter = strict("host.arbiter", {
    register = function(_, _, on, off)
      local id = nextId()
      S.claims[id] = { on = on, off = off, matching = false }
      return id
    end,
    setMatching = function(_, id, m)
      local c = S.claims[id]
      if m and not c.matching then
        c.matching = true
        c.on()
      elseif not m and c.matching then
        c.matching = false
        c.off()
      end
    end,
    winner = function()
      for id, c in pairs(S.claims) do if c.matching then return id end end
      return nil
    end,
    winnerSpecificity = function() return 0 end,
    participants = function() return { { module = "com.platform.komplete-kontrol" } } end,
  }),
})
S.claims = {}
-- `host.calibrating` is a plain value, which the strict table would refuse while it is nil.
setmetatable(T.host, { __index = function(_, k)
  if k == "calibrating" then return S.calibrating end
  error(("the scripted host has no host.%s"):format(tostring(k)), 2)
end })

-- An overlay over plug-in control 7 that is in front and holds its keys, with `tests` as its
-- menu tests (nil: none). Two controls: one that opens a menu, as Kontakt's snapshot dropdown
-- does, and one that does not.
function T.overlay(tests, label)
  local o = T.O.new(label or "Kontakt 8 in a DAW")
  o:addCustomButton({ label = "Snapshot menu", hotkey = "Alt+M", opensMenu = true,
    onActivate = function() S.acted = (S.acted or 0) + 1; S.order[#S.order + 1] = "acted" end })
  o:addCustomButton({ label = "Next snapshot", hotkey = "Ctrl+N", onActivate = function() end })
  if tests ~= nil then o:_watchMenus(tests) end
  T.front(o)
  return o
end

-- Komplete Kontrol in REAPER, bound for real: `attachEmbedded` against the scripted FX window,
-- with REAPER's chrome classes, so the runtime's own context match — the focus gate included —
-- decides whether the overlay is in front, through the arbiter on KK's slot. It starts with the
-- keyboard in KK, so it is active. `tests` as its menu tests; `setup(o)`, when given, runs
-- before it binds; `pollMatch`, when given, is the binding's poll interval (see T.poll).
function T.embedded(tests, label, setup, pollMatch)
  S.front, S.controls, S.chain = T.FX, { T.LIST, T.WRAP, T.KK }, { T.KK, T.WRAP, T.FX }
  T.turn()
  local o = T.O.new(label or "Komplete Kontrol")
  o:addCustomButton({ label = "Komplete Kontrol menu", hotkey = "Alt+M", opensMenu = true,
    onActivate = function() S.acted = (S.acted or 0) + 1; S.order[#S.order + 1] = "acted" end })
  o:addCustomButton({ label = "Search library browser", hotkey = "Alt+S", onActivate = function() end })
  if setup then setup(o) end
  o:attachEmbedded({ hosts = { T.HOST }, control = "Qt%d+.-QWindowIcon" },
    { slot = "com.platform.kontakt", menus = tests, pollMatch = pollMatch })
  return o
end

-- Windows completes an accepted host.window.focus: the change the scenario scripted in
-- S.focusLands, then the foreground event it causes.
function T.land()
  local id = S.landing
  assert(id ~= nil, "no accepted host.window.focus is waiting to land")
  S.landing = nil
  S.focusLands(id)
  T.event()
end

-- The binding's pollMatch poll of `ms` comes due, as the host runs a host.timer.every: nothing
-- turns the epoch over.
function T.poll(ms)
  local fn = S.polls[ms]
  assert(fn ~= nil, "no poll of " .. tostring(ms) .. " ms")
  fn()
end

-- Something turns the epoch over that the scenario does not otherwise play: another module's
-- image result, any module's timer.after, a key. What the host kept for the epoch is asked again.
function T.turn() S.epoch += 1 end

-- A window or focus event, as the host delivers one: the epoch turns over first, and every
-- overlay's trigger rechecks.
function T.event()
  S.epoch += 1
  local w = S.front
  if w and w.hidden then w = nil end
  for _, cb in ipairs(S.triggers) do cb(w) end
end

-- `win` comes to the front, shown, with the keyboard on `chain` (default: the window itself), and
-- the host says so.
function T.show(win, chain)
  if win then win.hidden = nil end
  S.front = win
  S.chain = chain or (win and { win } or {})
  T.event()
end

-- `win` is hidden, and stays the foreground window if it was: Komplete Kontrol's menu after
-- Escape. No event says so.
function T.hide(win) win.hidden = true end

-- The overlay comes to the front, as the arbiter's activation leaves it: active, its origin
-- known, its keys held.
function T.front(o)
  o.active = true
  o.activeCtx = { origin = function() return S.origin end }
  if o.focus == 0 then o.focus = 1 end
  o:_registerHotkeys()
end

-- The host's order: one-shot timers that came due (the epoch turned over once before them), then
-- the snapshots' answers (once more before those), then — in T.tick — the recurring tick, which
-- turns nothing over.
function T.runDue()
  for _, a in ipairs(S.after) do
    if a.at <= S.now then
      S.epoch += 1
      break
    end
  end
  local i = 1
  while i <= #S.after do
    local a = S.after[i]
    if a.at <= S.now then
      table.remove(S.after, i)
      a.fn()
    else
      i += 1
    end
  end
  -- The pictures asked of snapshotAsync that are due: taken at their time, answered now.
  for _, r in ipairs(S.asyncs) do
    if r.at <= S.now then
      S.epoch += 1
      break
    end
  end
  i = 1
  while i <= #S.asyncs do
    local r = S.asyncs[i]
    if r.at <= S.now then
      table.remove(S.asyncs, i)
      S.order[#S.order + 1] = "snapshotAsync"
      local s = T.snap(r.region)
      s.time = r.at
      r.cb(s, nil, { waited = r.at - r.asked, frames = 1 })
    else
      i += 1
    end
  end
end

-- One tick of the menu timer, 150 ms after the last, with any timer.after that came due first.
function T.tick(n)
  for _ = 1, n or 1 do
    S.now += 150
    T.runDue()
    if S.every then S.every() end
  end
end

-- Ticks for `ms` of the scenario's time.
function T.run(ms)
  local stop = S.now + ms
  while S.now < stop do T.tick(1) end
end

-- The control that opens a menu (the first), focused and pressed with Return, as the captured
-- key delivers it: the epoch turns over first.
function T.press(o)
  o.focus = 1
  assert(S.holding["Return"], "Return is not captured, so it would not reach the overlay")
  S.epoch += 1
  S.captured["Return"]()
end

-- One of the overlay's own keys (Tab), as the captured key delivers it.
function T.tab()
  assert(S.holding["Tab"], "Tab is not captured, so it would not reach the overlay")
  S.epoch += 1
  S.captured["Tab"]()
end

-- Whether the overlay holds captured key `spec` now.
function T.holds(spec) return S.holding[spec] ~= nil end

-- How many hotkeys are registered now.
function T.hotkeys()
  local n = 0
  for _ in pairs(S.hot) do n += 1 end
  return n
end

function T.open() return S.menuOpen[#S.menuOpen] end

-- How often the recorded menuOpen value changed from entry `from` on.
function T.flips(from)
  local n, last = 0, S.menuOpen[from]
  for i = from + 1, #S.menuOpen do
    if S.menuOpen[i] ~= last then n += 1 end
    last = S.menuOpen[i]
  end
  return n
end

function T.history(from)
  local out = {}
  for i = from or 1, #S.menuOpen do out[#out + 1] = S.menuOpen[i] and "1" or "0" end
  return table.concat(out)
end

-- Log lines containing `text` (plain), from line `from` on.
function T.count(text, from)
  local n = 0
  for i = from or 1, #S.logs do
    if string.find(S.logs[i], text, 1, true) then n += 1 end
  end
  return n
end

function T.dump() return table.concat(S.logs, "\n") end

return T
"##;

/// A fresh VM with the scripted host and the runtime loaded into it; the scenario runs with the
/// harness as the global `T` (and the runtime as `T.O`). The host plays Windows.
fn run(scenario: &str) {
    run_on("windows", scenario)
}

/// The same with the host playing a Mac: `host.os.current` is "macos" from before the runtime
/// loads, an empty focus chain is allowed for a shown window, and `S.busy` is an application
/// that does not answer.
fn run_mac(scenario: &str) {
    run_on("macos", scenario)
}

fn run_on(os: &str, scenario: &str) {
    let lua = Lua::new();
    let t: Table = lua.load(HARNESS).set_name("harness").eval().expect("the harness loads");
    let host: Table = t.get("host").unwrap();
    let os_table: Table = host.raw_get("os").unwrap();
    os_table.raw_set("current", os).unwrap();
    let wrapped: Function = lua
        .load(format!("return function(host) {RUNTIME}\nend"))
        .set_name("overlay-runtime/src/main.luau")
        .eval()
        .expect("the runtime compiles");
    let o: Table = wrapped.call(host).expect("the runtime loads against the scripted host");
    t.set("O", o).unwrap();
    lua.globals().set("T", t).unwrap();
    if let Err(e) = lua.load(scenario).set_name("scenario").exec() {
        panic!("{e}");
    }
}

// ---------------------------------------------------------------------------------------------
// No timer decides the state.
// ---------------------------------------------------------------------------------------------

/// The 2026-09-26 session: Kontakt's snapshot menu seen for a minute, then the keys flipping
/// every 1.2 s once a 30 s cap ran out. Now a menu a test sees for five minutes keeps the keys
/// for five minutes, with one line when it opened and no change after.
#[test]
fn a_menu_seen_for_minutes_keeps_the_keys_with_no_change() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.accessibility })
        assert(S.registered == 2, "the overlay holds its two hotkeys")
        T.press(o)
        assert(S.acted == 1, "the control acted")
        assert(#S.menuOpen == 0 or T.open() == false, "a press alone counts no menu as open")
        S.find = true
        T.tick()
        assert(T.open() == true, "the test sees it on the next tick")
        local from = #S.menuOpen
        T.run(5 * 60 * 1000)
        assert(T.open() == true and T.flips(from) == 0, "no change in five minutes: " .. T.history(from))
        assert(T.count("gave up its per-control hotkeys") == 1, T.dump())
        assert(T.count("took back") == 0, T.dump())
        assert(S.registered == 2 and S.unregistered == 2, "given up once, never taken back")
        assert(T.count("a menu is open — test 'accessibility' sees it, after 'Snapshot menu' was "
          .. "pressed; keyboard focus on [Qt661QWindowIcon{0}1] id=7") == 1, T.dump())
        -- While it is open the test is asked on every tick.
        local finds = S.finds
        T.tick(10)
        assert(S.finds - finds == 10, "asked " .. (S.finds - finds) .. " times in 10 ticks")
    "#);
}

/// A menu that opens and closes gives the keys to it exactly once and takes them back exactly
/// once, and the hotkeys are registered again once.
#[test]
fn a_menu_opens_and_closes_exactly_once() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.nativePopup })
        T.tick(4)
        local from = #S.menuOpen
        S.native = true
        T.run(3000)
        S.native = false
        T.run(3000)
        assert(T.flips(from) == 2, T.history(from))
        assert(T.open() == false)
        assert(T.count("gave up its per-control hotkeys") == 1, T.dump())
        assert(T.count("took back its per-control hotkeys") == 1, T.dump())
        assert(T.count("the menu has closed — no test sees it any more") == 1, T.dump())
        assert(T.count("no control of this overlay opened it") == 1, "nobody pressed anything: " .. T.dump())
        assert(S.registered == 4, "registered at the start and once when taken back: " .. S.registered)
    "#);
}

/// One answer that misses is not a close; the second in a row is.
#[test]
fn one_missed_round_is_not_a_close_and_two_are() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.accessibility })
        T.press(o)
        S.find = true
        T.tick(3)
        local from = #S.menuOpen
        S.find = false
        T.tick(1)
        assert(T.open() == true, "one missed read keeps it open")
        S.find = true
        T.tick(3)
        assert(T.open() == true and T.flips(from) == 0, T.history(from))
        S.find = false
        T.tick(1)
        assert(T.open() == true, "one miss again")
        T.tick(1)
        assert(T.open() == false, "two in a row close it")
        assert(T.flips(from) == 1, T.history(from))
        assert(T.count("took back") == 1)
    "#);
}

/// Leaving the front drops the state, a waiting round included; back in front with the menu
/// still there, the tests see it again as a new sighting.
#[test]
fn leaving_the_front_starts_the_menu_state_again() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.accessibility })
        T.press(o)
        S.find = true
        T.tick(2)
        assert(T.open() == true)
        o:_deactivate()
        assert(T.open() == false, "deactivation hands the flag back")
        assert(T.count("left the front with a menu counted open") == 1, T.dump())
        local finds = S.finds
        T.run(5000)
        assert(S.finds == finds, "nothing is asked for an overlay that is not in front")
        T.front(o)
        T.run(1500)
        assert(T.open() == true, "the backstop sees it again")
        assert(T.count("and no control of this overlay opened it") == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The list, and rounds.
// ---------------------------------------------------------------------------------------------

/// Several tests in one list: the menu is open while ANY of them sees it, and the log names the
/// one that saw it first.
#[test]
fn several_tests_in_one_list() {
    run(r#"
        local S = T.S
        S.own = false
        local own = { name = "snapshot list", cheap = true, test = function(_, answer) answer(S.own) end }
        local o = T.overlay({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow, own })
        T.tick(2)
        local from = #S.menuOpen
        S.native = true
        T.tick(1)
        assert(T.open() == true)
        assert(T.count("test 'nativePopup' sees it") == 1, T.dump())
        -- Handed from one test to the other with no gap: still one menu.
        S.own = true
        T.tick(1)
        S.native = false
        T.tick(5)
        assert(T.open() == true and T.flips(from) == 1, T.history(from))
        S.own = false
        T.tick(2)
        assert(T.open() == false and T.flips(from) == 2, T.history(from))
        -- A bare function is a test too, named by its place in the list.
        S.bare = false
        local p = T.overlay({ function(_, answer) answer(S.bare) end }, "Second")
        S.bare = true
        T.run(1500)
        assert(T.count("'Second': a menu is open — test 'test 1' sees it") == 1, T.dump())
    "#);
}

/// An asynchronous test: it is not asked again while its answer is outstanding, the state stays
/// as it is meanwhile — no timeout decides anything — and the late answer counts when it comes.
/// An answer to a question asked before the overlay left the front is ignored.
#[test]
fn an_asynchronous_test_answering_late() {
    run(r#"
        local S = T.S
        S.asked = 0
        local slow = function(_, answer)
          S.asked += 1
          S.pending = answer
        end
        local o = T.overlay({ { name = "image", test = slow } })
        T.press(o)
        T.tick(1)
        assert(S.asked == 1 and S.pending ~= nil, "it was asked")
        T.run(10000)
        assert(S.asked == 1, "not asked again while the first answer is outstanding: " .. S.asked)
        assert(T.open() ~= true, "no answer, no menu: " .. T.history(1))
        assert(T.count("menu test 'image' has not answered for 20 ticks") == 1, T.dump())
        local answer = S.pending
        S.pending = nil
        answer(true)
        assert(T.open() == true, "the late answer opens it at once")
        assert(T.count("test 'image' sees it") == 1, T.dump())
        answer(false)
        assert(T.count("answered twice to one question") == 1, "a second answer is ignored and said")
        assert(T.open() == true)
        T.tick(1)
        assert(S.asked == 2, "asked again on the next tick")
        -- Answers false, twice: closed.
        S.pending(false)
        T.tick(1)
        S.pending(false)
        assert(T.open() == false, T.history(1))
        -- A question still out when the overlay leaves the front: its own answer is dropped.
        -- The press opens the look window again, so the test is asked on the next tick.
        T.press(o)
        T.tick(1)
        assert(S.asked == 4, "asked after the press: " .. S.asked)
        local stale = S.pending
        o:_deactivate()
        T.front(o)
        stale(true)
        assert(T.open() ~= true, "an answer to a question asked before leaving is ignored: " .. T.history(1))
        assert(T.count("answered twice") == 1, "a dropped answer is not a second answer")
        T.press(o)
        T.tick(1)
        assert(S.asked == 5, "asked afresh after the return: " .. S.asked)
        S.pending(true)
        assert(T.open() == true, "the fresh answer counts")
    "#);
}

/// A test that raises has answered false, and says so once.
#[test]
fn a_test_that_raises_counts_as_not_seeing_a_menu() {
    run(r#"
        local S = T.S
        local o = T.overlay({ { name = "broken", cheap = true, test = function() error("no pixel") end },
          T.O.menuTests.nativePopup })
        T.run(3000)
        assert(T.open() == false)
        assert(T.count("menu test 'broken' failed") == 1, T.dump())
        S.native = true
        T.tick(1)
        assert(T.open() == true, "the other test still counts")
    "#);
}

/// A module's own pixel test: a synchronous colour check that answers at once. Declared cheap,
/// it runs on every tick with nothing pressed.
#[test]
fn a_modules_own_pixel_test() {
    run(r#"
        local S = T.S
        local lit = {
          name = "menu border",
          cheap = true,
          test = function(o, answer)
            local c = o:origin().client
            local p = T.host.screen.pixel(c.x + 412, c.y + 140)
            answer(p ~= nil and p.r > 200 and p.g > 200 and p.b > 200)
          end,
        }
        local o = T.overlay({ lit })
        T.tick(8)
        assert(S.pixels == 8, "cheap: asked on every tick, " .. S.pixels)
        S.pixel = { r = 240, g = 240, b = 240 }
        T.tick(1)
        assert(T.open() == true)
        assert(T.count("test 'menu border' sees it, and no control of this overlay opened it") == 1, T.dump())
        S.pixel = { r = 20, g = 20, b = 20 }
        T.tick(2)
        assert(T.open() == false)
    "#);
}

// ---------------------------------------------------------------------------------------------
// The building blocks.
// ---------------------------------------------------------------------------------------------

/// nativePopup is cheap: asked on every tick; accessibility is not: once in 8 ticks with nothing
/// pressed, every tick after a press and while a menu is open.
#[test]
fn the_cheap_building_block_runs_every_tick_and_the_accessibility_walk_on_the_backstop() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.nativePopup, T.O.menuTests.accessibility })
        T.tick(80)
        assert(S.nativeAsks == 80, "nativePopup every tick: " .. S.nativeAsks)
        assert(S.finds == 10, "accessibility one tick in eight: " .. S.finds)
        T.press(o)
        local finds = S.finds
        T.tick(20)
        assert(S.finds - finds == 20, "every tick after a press: " .. (S.finds - finds))
        T.run(10000)
        finds = S.finds
        T.tick(80)
        assert(S.finds - finds == 10, "back on the backstop once the press is past: " .. (S.finds - finds))
    "#);
}

/// newWindow: a window of the plug-in's process that was not there at the press is the menu,
/// its going is the close, and with the menu gone the test stops comparing.
#[test]
fn new_window_sees_a_window_that_appeared_since_the_press() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.overlay({ T.O.menuTests.newWindow })
        T.tick(4)
        assert(S.windowLists == 0, "nothing is listed without a press")
        -- A window appearing with nothing pressed is not a menu.
        S.windows = { base, { id = 150, layer = 0, class = "", x = 1, y = 1, w = 10, h = 10 } }
        T.tick(4)
        assert(T.open() ~= true)
        S.windows = { base }
        T.press(o)
        assert(S.windowLists == 1, "the list is taken at the press")
        T.tick(2)
        assert(T.open() ~= true)
        S.windows = { base, { id = 200, layer = 101, class = "", x = 1136, y = 191, w = 132, h = 296 } }
        T.tick(1)
        assert(T.open() == true)
        assert(T.count("newWindow: a window appeared for the plug-in since the press — id 200, "
          .. "layer 101, 132x296 at 1136,191") == 1, T.dump())
        assert(T.count("test 'newWindow' sees it, after 'Snapshot menu' was pressed") == 1, T.dump())
        T.run(3000)
        assert(T.count("a window appeared") == 1, "said once, not per tick")
        S.windows = { base }
        T.tick(2)
        assert(T.open() == false)
        assert(T.count("newWindow: the window that appeared has gone") == 1, T.dump())
        -- The press is spent: a later window is not that menu.
        local lists = S.windowLists
        S.windows = { base, { id = 300, layer = 0, class = "", x = 1, y = 1, w = 10, h = 10 } }
        T.run(3000)
        assert(T.open() == false and S.windowLists == lists, "no comparing without a press")
    "#);
}

/// newWindow forgets a press when the user is back in the overlay (one of its own keys with no
/// menu open), and when the overlay leaves the front.
#[test]
fn new_window_forgets_the_press_when_the_user_is_back_in_the_overlay() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.overlay({ T.O.menuTests.newWindow })
        T.press(o)
        T.tick(3)
        T.tab()
        S.windows = { base, { id = 400, layer = 0, class = "", x = 5, y = 5, w = 50, h = 20 } }
        T.run(3000)
        assert(T.open() ~= true, "the Tab ended the press: " .. T.history(1))
        S.windows = { base }
        T.press(o)
        o:_deactivate()
        T.front(o)
        S.windows = { base, { id = 500, layer = 0, class = "", x = 5, y = 5, w = 50, h = 20 } }
        T.run(3000)
        assert(T.open() ~= true, "leaving the front ended it too")
        -- Pressing again starts comparing again, against the list as it is now.
        T.press(o)
        S.windows = { base, { id = 500, layer = 0, class = "", x = 5, y = 5, w = 50, h = 20 },
          { id = 600, layer = 101, class = "", x = 5, y = 5, w = 50, h = 20 } }
        T.tick(1)
        assert(T.open() == true)
        assert(T.count("id 600") == 1 and T.count("id 500") == 0, T.dump())
    "#);
}

/// accessibility asks the plug-in control's tree for any Menu element.
#[test]
fn accessibility_asks_the_origin_for_a_menu_element() {
    run(r#"
        local S = T.S
        local asked = {}
        S.find = function(id) asked[#asked + 1] = id; return id == 7 end
        local o = T.overlay({ T.O.menuTests.accessibility })
        T.press(o)
        T.tick(1)
        assert(asked[1] == 7, "asked about the origin control")
        assert(T.open() == true)
    "#);
}

/// Return and Escape that went through to an open menu are logged, and an Escape is followed:
/// "still seen" when the menu stays, "closed after" when it goes.
#[test]
fn keys_that_went_through_are_logged_and_an_escape_is_followed() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.nativePopup })
        S.native = true
        T.tick(2)
        S.passed = { { vk = 27, mask = 0, key = "Escape" } }
        T.tick(3)
        assert(T.count("went through to the menu: Escape") == 1, T.dump())
        assert(T.count("still seen two ticks after Escape went through") == 1, T.dump())
        S.passed = { { vk = 13, mask = 0, key = "Return" }, { vk = 27, mask = 0, key = "Escape" } }
        T.tick(1)
        S.native = false
        T.tick(2)
        assert(T.count("went through to the menu: Return, Escape") == 1, T.dump())
        assert(T.count("and was seen on 1 tick(s) after Escape went through") == 1, T.dump())
        -- Keys that went through while no menu of this overlay was open are not reported.
        S.passed = { { vk = 9, mask = 0, key = "Tab" } }
        T.tick(1)
        assert(T.count("went through to the menu: Tab") == 0)
    "#);
}

// ---------------------------------------------------------------------------------------------
// Each test on its own: what the review of 2026-09-26 found.
// ---------------------------------------------------------------------------------------------

/// A test that never answers holds up none of the others: nativePopup is still asked on every
/// tick, opens the menu and closes it. The silent one is named in the log once.
#[test]
fn a_test_that_never_answers_holds_up_none_of_the_others() {
    run(r#"
        local S = T.S
        local mute = { name = "mute", cheap = true, test = function() end }
        local o = T.overlay({ mute, T.O.menuTests.nativePopup })
        T.tick(2)
        S.native = true
        T.tick(1)
        assert(T.open() == true, "nativePopup is still asked: " .. T.history(1))
        S.native = false
        T.tick(2)
        assert(T.open() == false, T.history(1))
        assert(S.nativeAsks == 5, "every tick: " .. S.nativeAsks)
        T.tick(20)
        assert(T.count("menu test 'mute' has not answered for 20 ticks — it is not asked again "
          .. "until it does, and its word stands meanwhile (no menu)") == 1, T.dump())
    "#);
}

/// A slow test that sees the menu keeps it open while its next answer is out, however many
/// ticks a quicker sibling says no; one miss of its own is not a close, the second is.
#[test]
fn a_slow_test_that_sees_the_menu_keeps_it_open_while_it_works() {
    run(r#"
        local S = T.S
        local pend
        local slow = { name = "image", test = function(_, answer) pend = answer end }
        local o = T.overlay({ T.O.menuTests.nativePopup, slow })
        T.press(o)
        T.tick(1)
        pend(true)
        assert(T.open() == true)
        local from = #S.menuOpen
        T.tick(1)
        local p1 = pend
        T.tick(5)
        assert(T.open() == true and T.flips(from) == 0, "no flicker while it works: " .. T.history(from))
        p1(false)
        T.tick(1)
        local p2 = pend
        assert(p2 ~= p1, "asked again once it answered")
        T.tick(3)
        assert(T.open() == true, "one miss of its own is not a close: " .. T.history(from))
        p2(false)
        assert(T.open() == false, "the second is: " .. T.history(from))
        assert(T.flips(from) == 1, T.history(from))
    "#);
}

/// A test that saw the menu and then never answers keeps it open — no timer ends its word — until
/// the overlay leaves the front, and the log names it.
#[test]
fn a_test_that_saw_the_menu_and_went_silent_keeps_it_until_the_overlay_leaves() {
    run(r#"
        local S = T.S
        local pend
        local slow = { name = "image", test = function(_, answer) pend = answer end }
        local o = T.overlay({ T.O.menuTests.nativePopup, slow })
        T.press(o)
        T.tick(1)
        pend(true)
        T.tick(1)
        T.run(60000)
        assert(T.open() == true, "its word stands: " .. T.history(1))
        assert(T.count("menu test 'image' has not answered for 20 ticks — it is not asked again "
          .. "until it does, and its word stands meanwhile (that a menu is open)") == 1, T.dump())
        o:_deactivate()
        T.front(o)
        T.tick(3)
        assert(T.open() == false and o._hotkeysSuspended == false, "back in front, in charge: " .. T.history(1))
    "#);
}

/// A failure while the keys are handed over does not leave the state half-changed: the focus
/// that cannot be told is said as such, and a hand-over that raises is completed by the next
/// tick.
#[test]
fn a_failure_while_handing_the_keys_over_is_completed_by_the_next_tick() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.nativePopup })
        T.tick(2)
        rawset(T.host.window, "focusChain", function() error("no focus today") end)
        S.native = true
        T.tick(1)
        assert(T.open() == true, T.history(1))
        assert(T.count("keyboard focus on an unknown element (") == 1, T.dump())
        assert(T.count("gave up its per-control hotkeys") == 1, T.dump())
        -- The first attempt to hand the keys back raises; the tick after it tries again.
        local refused = false
        rawset(T.host.keys, "menuOpen", function(b)
          if b == false and not refused then
            refused = true
            error("the hook is busy")
          end
          S.menuOpen[#S.menuOpen + 1] = b
        end)
        S.native = false
        T.tick(2)
        assert(T.count("handing the keys back failed: ") == 1, T.dump())
        assert(T.open() == false and o._hotkeysSuspended == false, T.history(1))
        assert(T.count("took back its per-control hotkeys") == 1, T.dump())
        S.native = true
        T.tick(1)
        assert(T.open() == true, "and the next menu is seen as usual")
    "#);
}

/// While a cheap test sees the menu, the accessibility walk is not paid; once it stops, the walk
/// is asked again.
#[test]
fn a_cheap_test_that_sees_the_menu_spares_the_accessibility_walk() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.nativePopup, T.O.menuTests.accessibility })
        T.run(10000)
        S.native = true
        T.tick(1)
        local f = S.finds
        T.tick(40)
        assert(S.finds == f, "no walk while nativePopup sees the menu: " .. (S.finds - f))
        S.native = false
        T.tick(2)
        assert(T.open() == false)
        assert(S.finds > f, "asked again once nativePopup stops seeing it")
    "#);
}

/// A sibling that takes two ticks to answer does not push the accessibility walk off its beat:
/// one walk in eight ticks, and a menu it sees is noticed.
#[test]
fn the_backstop_keeps_its_beat_beside_a_slow_sibling() {
    run(r#"
        local S = T.S
        local slow = { name = "slow image", cheap = true, test = function(_, answer)
          T.host.timer.after(151, function() answer(false) end)
        end }
        local o = T.overlay({ slow, T.O.menuTests.accessibility })
        T.tick(80)
        assert(S.finds == 10, "one walk in eight ticks: " .. S.finds)
        S.find = true
        T.run(1500)
        assert(T.open() == true, T.history(1))
    "#);
}

/// Any value but nil and false is an answer of "seen": a search's hit handed straight on.
#[test]
fn any_value_but_nil_and_false_counts_as_seen() {
    run(r#"
        local S = T.S
        S.hit = nil
        local img = { name = "image", cheap = true, test = function(_, answer) answer(S.hit) end }
        local o = T.overlay({ img })
        T.tick(3)
        assert(T.open() ~= true)
        S.hit = { x = 1, y = 2, w = 3, h = 4 }
        T.tick(1)
        assert(T.open() == true, "a hit is a sighting")
        S.hit = nil
        T.tick(2)
        assert(T.open() == false)
    "#);
}

/// Two overlays of one module in front: a menu over either takes the hotkeys of both, because
/// the pass-through lets every captured key of the application through.
#[test]
fn every_overlay_of_the_module_in_front_gives_up_its_hotkeys() {
    run(r#"
        local S = T.S
        local a = T.overlay({ T.O.menuTests.nativePopup }, "A")
        local b = T.overlay({ function(_, answer) answer(false) end }, "B")
        T.tick(2)
        S.native = true
        T.tick(1)
        assert(a._hotkeysSuspended == true and b._hotkeysSuspended == true, "both out")
        assert(T.count("'A' gave up") == 1 and T.count("'B' gave up") == 1, T.dump())
        S.native = false
        T.tick(2)
        assert(a._hotkeysSuspended == false and b._hotkeysSuspended == false, "both back")
        assert(T.count("'A' took back") == 1 and T.count("'B' took back") == 1, T.dump())
    "#);
}

/// newWindow keeps to the process it saw at the press: a program that comes to the front before
/// the overlay hears of it is not listed, and its windows are not the plug-in's menu.
#[test]
fn new_window_keeps_to_the_process_it_saw_at_the_press() {
    run(r#"
        local S = T.S
        S.origin.app = nil -- an embedded origin is a control, with no application
        local fg = { id = 1, title = "FX", app = { pid = 4242, exe = "reaper.exe" } }
        rawset(T.host.window, "active", function() return fg end)
        local lists = {
          [4242] = { S.windows[1] },
          [777] = { { id = 5000, layer = 0, class = "Chrome_WidgetWin_1", x = 0, y = 0, w = 800, h = 600 } },
        }
        local asked = {}
        rawset(T.host.window, "windowsOf", function(pid)
          asked[pid] = (asked[pid] or 0) + 1
          return lists[pid] or {}
        end)
        local o = T.overlay({ T.O.menuTests.newWindow })
        T.press(o)
        T.tick(1)
        fg = { id = 9, title = "Browser", app = { pid = 777, exe = "chrome.exe" } }
        T.tick(3)
        assert(T.open() ~= true, "another program's windows are not the menu: " .. T.history(1))
        assert(asked[777] == nil, "the other program is never listed")
        assert(T.count("newWindow: a window appeared") == 0, T.dump())
        lists[4242] = { S.windows[1], { id = 200, layer = 101, class = "", x = 1, y = 1, w = 100, h = 200 } }
        T.tick(1)
        assert(T.open() == true, "the plug-in's own popup is still seen")
    "#);
}

/// accessibilityAfterPress: nothing is walked and nothing seen without a press; a press arms it
/// until the menu it opened has closed, a key of the overlay's own arrives with no menu open, or
/// the overlay leaves the front. An element that stays in the tree (Kontakt 8's snapshot bar on
/// Cinematic Studio Strings, 2026-09-26) keeps the overlay out only until the user leaves the
/// plug-in and comes back.
#[test]
fn accessibility_after_press_asks_only_from_a_press_until_that_press_is_done() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.nativePopup, T.O.menuTests.accessibilityAfterPress })
        S.find = true
        T.run(5000)
        assert(T.open() ~= true and S.finds == 0, "not asked without a press: " .. S.finds)
        -- A press; the menu it opened is seen, closes, and the test is done with that press.
        S.find = false
        T.press(o)
        T.tick(1)
        assert(S.finds == 1, "armed by the press")
        S.find = true
        T.tick(1)
        assert(T.open() == true)
        assert(T.count("test 'accessibilityAfterPress' sees it, after 'Snapshot menu' was pressed") == 1, T.dump())
        S.find = false
        T.tick(2)
        assert(T.open() == false)
        local f = S.finds
        S.find = true
        T.run(3000)
        assert(T.open() == false and S.finds == f, "done with that press: " .. (S.finds - f))
        -- A press that opens nothing, then Tab: done with it too.
        S.find = false
        T.press(o)
        T.tick(2)
        T.tab()
        S.find = true
        T.run(3000)
        assert(T.open() == false, "the Tab ended it: " .. T.history(1))
        -- The element that stays: out while it is there, back in after leaving and returning.
        S.find = false
        T.press(o)
        S.find = true
        T.tick(1)
        assert(T.open() == true)
        T.run(60000)
        assert(T.open() == true, "still there, still counted")
        o:_deactivate()
        T.front(o)
        T.run(3000)
        assert(T.open() == false and o._hotkeysSuspended == false, "back in charge: " .. T.history(1))
    "#);
}

/// A key of the overlay's own that still reaches it while a menu is open (a tab's hotkey) does
/// not end the press the menu came from, and leaving the front ends the look window after a
/// press.
#[test]
fn an_own_key_during_a_menu_and_the_look_window_after_leaving() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.overlay({ T.O.menuTests.newWindow })
        T.press(o)
        S.windows = { base, { id = 200, layer = 101, class = "", x = 1, y = 1, w = 10, h = 10 } }
        T.tick(1)
        assert(T.open() == true)
        T.tab()
        T.tick(3)
        assert(T.open() == true, "still compared against the press: " .. T.history(1))
        local p = T.overlay({ T.O.menuTests.nativePopup, T.O.menuTests.accessibility }, "Second")
        o:_deactivate()
        T.run(10000)
        T.press(p)
        p:_deactivate()
        T.front(p)
        local f = S.finds
        T.tick(8)
        assert(S.finds - f == 1, "back on the backstop after leaving: " .. (S.finds - f))
    "#);
}

// ---------------------------------------------------------------------------------------------
// A menu that is a window of its own, in front: the overlay holds its place.
// ---------------------------------------------------------------------------------------------

/// The NVDA test of 2026-09-26, Komplete Kontrol in REAPER: Space on "Komplete Kontrol menu"
/// opened KK's menu as a popup window of reaper.exe, which came to the front, and the overlay
/// left the front with it. Now it stays in front, out — holding no keys at all while that window
/// is in front, and with no say in the pass-through flag — and its tests go on running while the
/// menu is up, whatever else the window events say.
#[test]
fn a_menu_window_of_the_same_process_after_a_press_keeps_the_overlay_out_and_its_tests_running() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        S.gates = 0
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.accessibility,
          T.O.menuTests.newWindow }, nil, function(o)
            o:gate(function() S.gates += 1; return true end)
            -- A text-field test re-syncs the focus keys on every recheck (ON:EAR has one).
            o:typingWhen(function() return false end)
          end)
        assert(o.active, "in front with the keyboard in KK: " .. T.dump())
        assert(T.hotkeys() == 2 and T.holds("Tab") and T.holds("Return"), "holding its keys")
        T.tick(2)
        T.press(o)
        assert(S.acted == 1, "the control acted")
        -- KK opens its menu as a window of reaper.exe, and it comes to the front.
        S.windows = { base, { id = 900, layer = 0, class = "Qt671QWindowPopup", x = 400, y = 180, w = 180, h = 220 } }
        local gates = S.gates
        T.show(T.POPUP)
        assert(o.active, "still in front: " .. T.dump())
        assert(T.count("[deactivate]") == 0, T.dump())
        assert(o._menu.open == true, "its menu counts as open at once")
        assert(T.hotkeys() == 0 and not T.holds("Tab") and not T.holds("Shift+Tab")
          and not T.holds("Return") and not T.holds("Space"), "no keys at all while the menu is in front")
        assert(T.open() == false, "no say in the pass-through flag: " .. T.history(1))
        assert(T.count("'Komplete Kontrol': 'Komplete Kontrol' (reaper.exe) came to the front after "
          .. "'Komplete Kontrol menu' was pressed and a test sees a menu — the overlay holds its "
          .. "place while it is up, holding no keys") == 1, T.dump())
        assert(T.count("test 'newWindow' sees it, after 'Komplete Kontrol menu' was pressed") == 1, T.dump())
        assert(o:origin() and o:origin().id == T.KK.id, "its origin is KK, behind the menu")
        assert(S.gates == gates, "the landmark gate is not asked while the menu is in front")
        -- Its tests go on while the menu is up; more events inside the menu change nothing, and
        -- nothing takes its keys back meanwhile.
        local lists = S.windowLists
        T.tick(10)
        assert(S.windowLists - lists == 10, "newWindow asked on every tick: " .. (S.windowLists - lists))
        local from, registered = #S.menuOpen, S.registered
        for _ = 1, 5 do
          T.show(T.POPUP)
          T.tick(4)
        end
        assert(o.active and o._menu.open and #S.menuOpen == from, "nothing written: " .. T.history(from))
        assert(S.registered == registered and T.hotkeys() == 0 and not T.holds("Tab"), "still no keys")
        assert(T.count("[deactivate]") == 0 and T.count("gave up its per-control hotkeys") == 1, T.dump())
        assert(T.count("holds its place") == 1, "said once")
    "#);
}

/// When the menu's window closes and the plug-in takes the keyboard straight back, the ordinary
/// match ends the hold, and the overlay's keys come back with it.
#[test]
fn the_plugin_taking_the_keyboard_back_from_the_menu_brings_the_keys_back() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        assert(not T.holds("Tab"))
        S.windows = { base }
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active and T.count("the plug-in has the keyboard again") == 1, T.dump())
        assert(T.holds("Tab") and T.holds("Return"), "its navigation keys at once")
        assert(T.hotkeys() == 0 and T.count("gave up its per-control hotkeys") == 1,
          "its hotkeys not while the menu still counts as open, and not taken to be given up again")
        assert(T.open() == true, "the menu still counts as open, so they pass: " .. T.history(1))
        T.tick(1)
        assert(T.open() == false and T.hotkeys() == 2, "hotkeys back on the next tick: " .. T.history(1))
        assert(T.count("took back its per-control hotkeys") == 1, T.dump())
        local spoken = #S.speech
        T.tab()
        assert(#S.speech > spoken)
    "#);
}

/// When the hold ends, the ordinary match decides, exactly as for any other window change — as
/// ReaHotkey does, whose plug-in context is REAPER's FX window active with the plug-in found in it.
/// A moment with nothing in front, while the tests still see the menu, keeps the hold. REAPER's FX
/// window in front with the keyboard on the window itself is REAPER's: the overlay leaves the front,
/// and nothing brings a window back — that is only for the menu's own window left in front. The
/// keyboard in KK brings the overlay back, its gate (a library's landmark) deciding as ever; on
/// REAPER's FX list it is REAPER's, as ever.
#[test]
fn when_the_hold_ends_the_ordinary_match_decides_as_for_any_window_change() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        S.gateOk = true
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.accessibility,
          T.O.menuTests.newWindow }, nil, function(o)
            o:gate(function() return S.gateOk end)
          end)
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "Qt671QWindowPopup", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        T.tick(3)
        assert(o._menu.open == true and o:_heldOnMenu() and not T.holds("Tab"))
        -- The menu closes: first nothing in front, then the FX window, the keyboard on the window
        -- itself — which REAPER's own focus gate counts as REAPER's.
        S.windows = { base }
        T.show(nil, {})
        assert(o.active and o:_heldOnMenu(), "nothing in front, the tests still see it: " .. T.dump())
        T.show(T.FX)
        assert(not o.active, "the FX window with the keyboard on it is REAPER's: " .. T.dump())
        assert(T.count("no longer holding its place — in front now: 'FX: Track 1 \"test\"' (reaper.exe)") == 1, T.dump())
        assert(T.count("[deactivate] 'Komplete Kontrol' — in front now: 'FX: Track 1") == 1, T.dump())
        assert(#S.focusCalls == 0, "no window brought back")
        assert(not T.holds("Tab") and T.hotkeys() == 0 and S.scopedTo == false, "nothing held")
        -- Into KK with the menu having loaded something else: the gate says no.
        S.gateOk = false
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(not o.active, "the gate decides: " .. T.dump())
        S.gateOk = true
        -- Into KK: the ordinary match brings it back, through an activation that takes its keys.
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active and T.holds("Tab") and T.hotkeys() == 2 and S.scopedTo == T.FX.id, T.dump())
        assert(T.count("the plug-in has the keyboard again") == 0, "the hold ended when it left")
        local spoken = #S.speech
        T.run(600)
        assert(#S.speech > spoken, "the activation speaks its control")
        -- On REAPER's FX list: REAPER's, as ever.
        T.show(T.FX, { T.LIST, T.FX })
        assert(not o.active, "the FX list is REAPER's: " .. T.dump())
        T.tick(10)
        assert(#S.focusCalls == 0)
    "#);
}

/// Alt+Tab to another application takes the overlay out as it always did, from the menu, and
/// coming back needs the keyboard in the plug-in again. Nothing is brought back: the window in
/// front is not the one the overlay held over.
#[test]
fn alt_tab_to_another_application_still_takes_the_overlay_out() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        assert(o.active)
        T.show(T.OTHER)
        assert(not o.active, T.dump())
        assert(T.count("[deactivate] 'Komplete Kontrol' — in front now: 'Browser' (chrome.exe)") == 1, T.dump())
        assert(T.count("no longer holding its place — in front now: 'Browser' (chrome.exe)") == 1, T.dump())
        assert(T.open() == false and #S.menuOpen > 0, "the flag handed back")
        -- Back to REAPER with the keyboard on the FX window itself: it left, so it waits for the
        -- plug-in as it always did.
        S.windows = { base }
        T.show(T.FX)
        assert(not o.active)
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active)
        assert(T.count("the plug-in has the keyboard again") == 0, "the hold ended when it left: " .. T.dump())
        assert(#S.focusCalls == 0, "no window brought back")
    "#);
}

/// Only the overlay's own menu is held: a window of the same process with no press behind it, a
/// window of another process after a press, a window of the same process that no test takes for
/// a menu (REAPER's main window, there before the press), the window of the press itself with the keyboard on REAPER's FX list while a test sees a menu that
/// did not take the front, and a popup after the press was forgotten (Tab with no menu open) —
/// even with a test that sees a menu then — all take the overlay out. None was held, so nothing
/// is brought back.
#[test]
fn a_window_that_is_not_the_overlays_menu_takes_it_out() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        S.own = false
        local own = { name = "own", cheap = true, test = function(_, answer) answer(S.own) end }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.accessibility,
          T.O.menuTests.newWindow, own })
        local function backInKK()
          S.windows = { base }
          T.show(T.FX, { T.KK, T.WRAP, T.FX })
          assert(o.active, "back in KK: " .. T.dump())
        end
        local popup = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        -- No press: a popup of reaper.exe coming to the front is not the overlay's menu.
        S.windows = popup
        T.show(T.POPUP)
        assert(not o.active, "no press behind it: " .. T.dump())
        assert(S.windowLists == 0, "nothing compared without a press")
        backInKK()
        -- A press, then a window of another process.
        T.press(o)
        T.show(T.OTHER)
        assert(not o.active, "another process: " .. T.dump())
        backInKK()
        -- A press, then REAPER's main window: the same process, but there at the press, and no
        -- test sees a menu.
        T.press(o)
        T.show(T.MAIN)
        assert(not o.active, "no test sees a menu there: " .. T.dump())
        backInKK()
        -- A press, a menu that does not take the front (newWindow sees it), and the keyboard on
        -- REAPER's FX list: the window of the press is never held.
        T.press(o)
        S.windows = popup
        T.tick(1)
        assert(o._menu.open, "newWindow sees the menu")
        T.show(T.FX, { T.LIST, T.FX })
        assert(not o.active, "the FX list is REAPER's: " .. T.dump())
        backInKK()
        -- A press, then Tab with no menu open: the press is forgotten, so the popup after it is not
        -- its menu.
        T.press(o)
        T.tick(2)
        T.tab()
        S.windows = popup
        S.own = true
        T.show(T.POPUP)
        assert(not o.active, "the press was forgotten: " .. T.dump())
        assert(T.count("holds its place") == 0, T.dump())
        assert(#S.focusCalls == 0, "nothing was held, so nothing is brought back")
    "#);
}

/// The menu closes and a window of the same process other than the overlay's comes to the front
/// (REAPER's main window): the tests are asked, none sees a menu now, and the overlay leaves the
/// front at once. The word, which says "open" for one more miss, is not a sighting. And when the
/// menu closes with no window event handled, the tick decides again once the tests stop seeing it
/// — without asking them a second time on that tick.
#[test]
fn the_menu_closing_over_another_window_of_the_process_takes_the_overlay_out() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local popup = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        S.asks = 0
        local counted = { name = "counted", cheap = true, test = function(_, answer) S.asks += 1; answer(false) end }
        local o = T.embedded({ T.O.menuTests.newWindow, counted })
        T.press(o)
        S.windows = popup
        T.show(T.POPUP)
        assert(o.active)
        -- The popup has gone and no tick has run since: newWindow's word still says open.
        S.windows = { base }
        T.show(T.MAIN)
        assert(not o.active, "out at once: no test sees a menu now: " .. T.dump())
        assert(T.count("no longer holding its place — in front now: 'REAPER v7' (reaper.exe)") == 1, T.dump())
        -- Where the keyboard goes, read afresh, is in the log: nothing was brought back.
        local reading = "[menu] 'Komplete Kontrol': no test sees the menu any more; the keyboard goes "
          .. "to id=100 of pid 4242, shown, and the menu's window 'Komplete Kontrol' (reaper.exe) is "
          .. "id=900 of pid 4242 — nothing is brought back"
        assert(T.count(reading) == 1, T.dump())
        assert(T.open() == false)
        -- Again, with no event for the window that took the front. (The ticks first run the
        -- recheck an activation schedules, so that the one below is the tick's own.) Nothing turns
        -- the epoch over, so the host still answers the popup as the window in front, from the
        -- epoch of its event; the window that gets the keyboard, read afresh, is REAPER's main
        -- window, shown, so nothing is brought back.
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active)
        T.tick(3)
        T.press(o)
        S.windows = popup
        T.show(T.POPUP)
        assert(o:_heldOnMenu())
        S.windows = { base }
        S.front, S.chain = T.MAIN, { T.MAIN }
        local asks, fgAsks = S.asks, S.fgAsks
        T.tick(1)
        assert(o.active and o._menu.open, "one miss is not a close")
        T.tick(1)
        assert(not o.active, "the tick decides again: " .. T.dump())
        assert(S.asks - asks == 2, "asked once per tick: " .. (S.asks - asks))
        assert(T.count("no longer holding its place — in front now: 'Komplete Kontrol' (reaper.exe)") == 1, T.dump())
        assert(T.count("[deactivate] 'Komplete Kontrol' — in front now: 'Komplete Kontrol' (reaper.exe)") == 1, T.dump())
        assert(S.fgAsks - fgAsks == 1 and #S.focusCalls == 0, "REAPER's main window gets the keyboard, shown: nothing brought back")
        -- The line that says so, where `in front now:` names the window kept for the epoch.
        assert(T.count(reading) == 2, T.dump())
        assert(T.count("bringing back") == 0, T.dump())
    "#);
}

/// A dialog that an item of a native menu opened — Komplete Kontrol standalone, Edit, then
/// Preferences — comes to the front while the tests' word still says the menu is open (one miss
/// is not a close). No test sees a menu there now, so the overlay is not held: it leaves the front
/// at once, as it always did, and the dialog's own overlay keeps the keys it takes — nothing
/// unpins them a tick later.
#[test]
fn a_dialog_that_a_native_menus_item_opened_is_not_held_and_its_overlay_keeps_the_keys() {
    run(r##"
        local S = T.S
        local KKW = { id = 300, title = "Komplete Kontrol", class = "NINormalWindow",
          app = { pid = 4242, exe = "Komplete Kontrol.exe" },
          bounds = { x = 0, y = 0, w = 1000, h = 700 }, client = { x = 8, y = 30, w = 984, h = 662 } }
        local PREFS = { id = 301, title = "Preferences", class = "#32770",
          app = { pid = 4242, exe = "Komplete Kontrol.exe" },
          bounds = { x = 100, y = 100, w = 600, h = 500 }, client = { x = 108, y = 130, w = 584, h = 462 } }
        S.front, S.chain = KKW, { KKW }
        local o = T.O.new("Komplete Kontrol")
        o:addCustomButton({ label = "Edit menu", hotkey = "Alt+E", opensMenu = true, onActivate = function() end })
        o:attach({ class = "NINormalWindow" }, { slot = "com.platform.komplete-kontrol",
          menus = { T.O.menuTests.nativePopup } })
        local prefs = T.O.new("Komplete Kontrol Preferences")
        prefs:addCustomButton({ label = "Close", onActivate = function() end })
        prefs:attach({ class = "#32770" })
        assert(o.active and not prefs.active, T.dump())
        T.press(o)
        S.native = true
        T.tick(1)
        assert(o._menu.open, "the Edit menu is seen")
        S.native = false
        T.show(PREFS)
        assert(not o.active, "not held: no test sees a menu now: " .. T.dump())
        assert(T.count("holds its place") == 0, T.dump())
        assert(T.count("[deactivate] 'Komplete Kontrol' — in front now: 'Preferences' (Komplete Kontrol.exe)") == 1, T.dump())
        assert(prefs.active and T.holds("Tab") and S.scopedTo == PREFS.id, "the dialog's overlay has the keys")
        T.tick(5)
        assert(prefs.active and S.scopedTo == PREFS.id, "nothing unpins them later: " .. tostring(S.scopedTo))
        assert(T.holds("Tab") and T.open() == false, T.history(1))
    "##);
}

/// A window an item of the menu opened that stays open — a non-modal one — while the user goes
/// back into the plug-in: the hold ends with the plug-in having the keyboard, the press is over,
/// so newWindow stops counting that window as the menu, and the keys come back instead of passing
/// through for as long as it exists. After that, the window in front again is not held.
#[test]
fn a_window_the_menu_opened_that_stays_up_does_not_keep_the_keys_once_the_plugin_has_the_keyboard() {
    run(r##"
        local S = T.S
        local base = S.windows[1]
        local W = { id = 950, title = "Plug-in Info", class = "Qt671QWindowIcon{info}", app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 200, y = 200, w = 300, h = 200 }, client = { x = 200, y = 200, w = 300, h = 200 } }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        S.windows = { base, { id = 950, layer = 0, class = "Qt671QWindowIcon{info}", x = 200, y = 200, w = 300, h = 200 } }
        T.show(W)
        T.tick(2)
        assert(o.active and o:_heldOnMenu(), "held over the window the menu opened: " .. T.dump())
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active and T.count("the plug-in has the keyboard again") == 1, T.dump())
        T.tick(2)
        assert(o._menu.open == false and T.open() == false and T.hotkeys() == 2, "the menu is over: " .. T.history(1))
        assert(T.count("took back its per-control hotkeys") == 1, T.dump())
        T.tick(20)
        assert(o._menu.open == false and T.open() == false and T.hotkeys() == 2 and T.holds("Tab"))
        -- That window in front again: not held (the press is over), so the overlay leaves the front.
        T.show(W)
        assert(not o.active and T.count("holds its place") == 1, T.dump())
        assert(#S.focusCalls == 0)
    "##);
}

/// Nothing in front keeps a hold only while the tests still see the menu. On a Mac that is also
/// Command+Tab to an application with no window: once the menu is gone the overlay leaves the
/// front instead of keeping its place and its claim on the slot, and nothing is brought back —
/// nothing in front is not the window it held over. Back in the host, the ordinary match decides.
#[test]
fn nothing_in_front_holds_only_a_menu_the_tests_still_see() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        S.windows = { base }
        T.show(nil, {})
        assert(o.active and o:_heldOnMenu(), "a menu the tests still see: the hold stands")
        T.tick(2)
        assert(not o.active, "the tests no longer see it: out: " .. T.dump())
        assert(T.count("no longer holding its place — in front now: nothing") == 1, T.dump())
        assert(T.count("[deactivate] 'Komplete Kontrol' — in front now: 'nothing' (?)") == 1, T.dump())
        assert(#S.focusCalls == 0, "nothing brought back")
        assert(T.count("no test sees the menu any more; the keyboard goes to no window the platform "
          .. "names, and the menu's window 'Komplete Kontrol' (reaper.exe) is id=900 of pid 4242 — "
          .. "nothing is brought back") == 1, T.dump())
        T.show(T.FX)
        assert(not o.active, "the keyboard on the FX window is REAPER's: " .. T.dump())
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        -- Through an activation, which took its keys once: no hold outlived the leaving to hand
        -- them back a second time.
        assert(o.active and T.holds("Tab") and T.hotkeys() == 2, T.dump())
        assert(T.count("took back its per-control hotkeys") == 0, T.dump())
        T.show(nil, {})
        assert(not o.active, "nothing in front is not held without a press: " .. T.dump())
        assert(T.count("[deactivate]") == 2 and not T.holds("Tab"))
    "#);
}

/// A round of the tests asked by a window event, while the overlay holds its place, is not a tick:
/// a test whose answer is outstanding is named in the log after 20 TICKS, however many events came
/// in between.
#[test]
fn a_round_asked_by_an_event_does_not_count_as_a_tick_for_an_outstanding_answer() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        -- Cheap, so it is asked beside newWindow, which sees the popup.
        local silent = { name = "silent", cheap = true, test = function() end }
        local o = T.embedded({ T.O.menuTests.newWindow, silent })
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        assert(o:_heldOnMenu())
        for _ = 1, 30 do T.show(T.POPUP) end
        T.tick(18)
        assert(T.count("menu test 'silent' has not answered") == 0, T.dump())
        T.tick(2)
        assert(T.count("menu test 'silent' has not answered for 20 ticks") == 1, T.dump())
    "#);
}

/// A hold the tick ends rather than a window event — the plug-in's focus event handled late, or
/// macOS reporting no change of the focused window — with the plug-in having the keyboard: the
/// hotkeys come back with the line that says so, once, like every other hand-back.
#[test]
fn a_hold_the_tick_ends_with_the_plugin_having_the_keyboard_takes_the_hotkeys_back_with_its_line() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        T.tick(4)
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        T.tick(4)
        assert(T.count("gave up its per-control hotkeys") == 1, T.dump())
        S.windows = { base }
        S.front, S.chain = T.FX, { T.KK, T.WRAP, T.FX }
        -- The focus event is not handled yet, but something else turns the epoch over (another
        -- module's image result, say), so the tick reads the world as it is now.
        T.turn()
        T.tick(3)
        assert(o.active and T.count("the plug-in has the keyboard again") == 1, T.dump())
        assert(T.hotkeys() == 2 and T.count("took back its per-control hotkeys") == 1, T.dump())
        assert(T.open() == false and T.holds("Tab"))
        assert(T.count("[deactivate]") == 0 and #S.focusCalls == 0, T.dump())
    "#);
}

/// An overlay holding its place over its menu's window has no say in the module's pass-through
/// flag and hotkeys: another overlay of the module in front over a window the menu led to, with no
/// menu of its own, keeps its keys.
#[test]
fn a_held_overlay_has_no_say_in_what_another_overlay_of_the_module_holds() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.embedded({ T.O.menuTests.newWindow })
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        assert(o:_heldOnMenu() and o._menu.open)
        local p = T.overlay({ T.O.menuTests.nativePopup }, "Komplete Kontrol Preferences")
        T.tick(3)
        assert(o:_heldOnMenu() and o._menu.open, "still held, its menu open")
        assert(T.open() == false, "no menu over the overlay in front: " .. T.history(1))
        assert(p._hotkeysSuspended == false and T.count("'Komplete Kontrol Preferences' gave up") == 0, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// The keyboard left in the menu's own window, not shown: the window of the press comes back.
// ---------------------------------------------------------------------------------------------

/// The maintainer's diagnostic run of 2026-09-27, as its log has it: Komplete Kontrol's menu is a
/// full-screen Qt window of reaper.exe, id 8327976, and the FX window of the press is 3149930. The
/// menu shots turned the epoch over while the menu was up, and a poll read the window in front
/// then. After Escape the menu's window went from newWindow's list with no event; on the tick that
/// closed the menu host.window.active() still answered the menu's window from that epoch ("in
/// front is id=8327976"), the focus chain read then was empty — on Windows, a hidden or absent
/// foreground window — the first key was lost, and the overlay let go. The scenario plays the
/// likeliest reading of that log, not measured: KK only hid the window, and it stayed the
/// foreground window. Now host.window.foreground(), read afresh, says where the keyboard is: in
/// the menu's own window, not shown. The window of the press is brought back — once, its answer
/// logged, nothing announced. Windows most likely makes the change on REAPER's thread, after the
/// call has returned: asked again at once, the foreground window is still the hidden one, nothing
/// in front to host.window.active(), so the overlay leaves the front. REAPER's foreground event
/// follows, with the keyboard in KK: the ordinary match brings the overlay back through an
/// activation, which speaks, and the first key after that is the overlay's.
#[test]
fn the_kk_menu_hidden_after_escape_brings_the_window_of_the_press_back_and_the_first_key_is_the_overlays() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        T.FX.id = 3149930
        local MENU = { id = 8327976, title = "Komplete Kontrol",
          class = "Qt671NI_6_7_1_R3QWindowIcon{f6865b51-2a08-4850-b0ea-1b8e454c58b7}",
          app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 0, y = 0, w = 1920, h = 1032 }, client = { x = 0, y = 0, w = 1920, h = 1032 } }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.accessibility,
          T.O.menuTests.newWindow })
        T.tick(2)
        T.press(o)
        S.windows = { base, { id = MENU.id, layer = 0, class = MENU.class, x = 0, y = 0, w = 1920, h = 1032 } }
        T.show(MENU)
        assert(o:_heldOnMenu() and not T.holds("Tab"), T.dump())
        T.tick(3)
        -- The menu shots' answers turn the epoch over while the menu is up, and another overlay's
        -- poll reads the window in front in that epoch.
        T.turn()
        assert(T.host.window.active() == MENU)
        -- Escape: KK hides the menu's window. It stays the foreground window, and no event comes.
        T.hide(MENU)
        S.windows = { base }
        -- Brought back, the FX window is in front with the keyboard in KK, as REAPER leaves it.
        S.focusLands = function(id)
          assert(id == T.FX.id, "the window of the press")
          S.front, S.chain = T.FX, { T.KK, T.WRAP, T.FX }
        end
        local spoken, asks = #S.speech, S.fgAsks
        T.tick(1)
        assert(#S.focusCalls == 0 and o:_heldOnMenu(), "one miss is not a close: " .. T.dump())
        assert(T.host.window.active() == MENU, "the host still answers the menu's window, kept for the epoch")
        T.tick(1)
        assert(#S.focusCalls == 1 and S.focusCalls[1] == 3149930, "the window of the press, once: " .. T.dump())
        assert(S.fgAsks - asks == 1, "one reading of the window that gets the keyboard: " .. (S.fgAsks - asks))
        assert(T.count("[menu] 'Komplete Kontrol': no test sees the menu any more, but its window "
          .. "'Komplete Kontrol' (reaper.exe) still gets the keyboard and is not shown — bringing back "
          .. "'FX: Track 1 \"test\"' (reaper.exe), where 'Komplete Kontrol menu' was pressed: accepted") == 1, T.dump())
        assert(#S.speech == spoken, "nothing announced")
        -- Not landed yet: the hidden menu window still has the foreground, which the host reads as
        -- nothing in front now that the call has turned the epoch over. The hold is over.
        assert(not o.active and not o:_heldOnMenu(), T.dump())
        assert(T.count("no longer holding its place — in front now: nothing") == 1, T.dump())
        assert(T.count("[deactivate] 'Komplete Kontrol' — in front now: 'nothing' (?)") == 1, T.dump())
        assert(T.count("the keyboard goes to") == 0, "it was brought back, so no other reading is logged")
        assert(o._menu.open == false and T.open() == false, "the menu is over: " .. T.history(1))
        -- Windows completes it: the FX window in front, the keyboard in KK, and REAPER's event.
        T.land()
        assert(o.active and T.holds("Tab") and T.holds("Return") and S.scopedTo == T.FX.id,
          "its keys, scoped to the FX window: " .. T.dump())
        assert(T.hotkeys() == 2 and T.count("took back its per-control hotkeys") == 0,
          "through an activation, which takes them: " .. T.dump())
        T.run(600)
        assert(#S.speech > spoken, "the activation speaks its control")
        -- The first key after that reaches the overlay.
        spoken = #S.speech
        T.tab()
        assert(#S.speech > spoken, "Tab moved in the overlay")
        -- Never again for this press.
        T.tick(20)
        T.event()
        assert(#S.focusCalls == 1, "once: " .. #S.focusCalls)
    "#);
}

/// The same with the keyboard on the FX window itself once it is back, which REAPER's chrome gate
/// counts as REAPER's: the overlay has left the front, the landing does not bring it back, and
/// REAPER's focus event a moment later — the keyboard in KK — does, through an ordinary
/// activation, which speaks. Here the epoch has turned over since the hide, so
/// host.window.active() answers nothing in front (a hidden window is none to it): the step goes by
/// host.window.foreground() either way.
#[test]
fn the_window_of_the_press_back_with_the_keyboard_on_the_fx_window_lets_the_ordinary_match_bring_the_overlay_back() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.accessibility,
          T.O.menuTests.newWindow })
        T.tick(2)
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "Qt671QWindowPopup", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        T.tick(3)
        assert(o:_heldOnMenu() and not T.holds("Tab"), T.dump())
        T.hide(T.POPUP)
        S.windows = { base }
        T.turn()
        assert(T.host.window.active() == nil, "a hidden window is no window in front to active()")
        S.focusLands = function(id) S.front, S.chain = T.FX, { T.FX } end
        local spoken = #S.speech
        T.tick(2)
        assert(#S.focusCalls == 1 and S.focusCalls[1] == T.FX.id, T.dump())
        assert(T.count("its window 'Komplete Kontrol' (reaper.exe) still gets the keyboard and is not "
          .. "shown — bringing back 'FX: Track 1 \"test\"' (reaper.exe), where 'Komplete Kontrol menu' "
          .. "was pressed: accepted") == 1, T.dump())
        assert(#S.speech == spoken, "nothing announced")
        assert(not o.active, "not landed yet: nothing in front: " .. T.dump())
        assert(T.count("no longer holding its place — in front now: nothing") == 1, T.dump())
        assert(T.count("[deactivate] 'Komplete Kontrol' — in front now: 'nothing' (?)") == 1, T.dump())
        -- It lands with the keyboard on the FX window: REAPER's, so the overlay stays out.
        local acts = T.count("[activate]")
        T.land()
        assert(not o.active, "the keyboard on the FX window is REAPER's: " .. T.dump())
        assert(T.count("[activate]") == acts and #S.speech == spoken, T.dump())
        -- REAPER's focus event: the ordinary match activates the overlay, which speaks.
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active and T.holds("Tab") and T.holds("Return") and T.hotkeys() == 2, T.dump())
        T.run(600)
        assert(#S.speech > spoken, "the activation speaks its control")
        spoken = #S.speech
        T.tab()
        assert(#S.speech > spoken, "Tab moved in the overlay")
        T.tick(20)
        T.event()
        assert(#S.focusCalls == 1, "once: " .. #S.focusCalls)
    "#);
}

/// A window event after Escape, before the tick has closed the menu: newWindow's word still says
/// open for one more miss, and the window in front is none to host.window.active() (the menu's is
/// hidden), so the hold stands, as over nothing in front while the tests still see a menu — the
/// word decides there, not a reading of the keyboard. The tick that closes the menu brings the
/// window of the press back. Here Windows makes the change inside the call — the less likely case,
/// played so that the path where the overlay never leaves the front is covered: the ordinary
/// match, asked again at once, finds the plug-in, the hold ends with the keys back and nothing
/// said, and the gate is asked again.
#[test]
fn an_event_before_the_menu_has_closed_decides_nothing_and_the_tick_that_closes_it_brings_the_window_back() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        S.gates = 0
        S.focusAtOnce = true
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow }, nil, function(o)
          o:gate(function() S.gates += 1; return true end)
        end)
        T.tick(2)
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        T.tick(2)
        assert(o:_heldOnMenu())
        -- A window event is handled scoped to the popup meanwhile (another overlay's keys, say).
        T.host.keys.scope(true)
        assert(S.scopedTo == T.POPUP.id)
        T.hide(T.POPUP)
        S.windows = { base }
        S.focusLands = function(id) S.front, S.chain = T.FX, { T.KK, T.WRAP, T.FX } end
        local spoken, gates = #S.speech, S.gates
        T.event()
        assert(#S.focusCalls == 0 and o.active and o:_heldOnMenu(), "the word still says open: " .. T.dump())
        T.tick(1)
        assert(#S.focusCalls == 0 and o:_heldOnMenu(), "one miss is not a close: " .. T.dump())
        T.tick(1)
        assert(#S.focusCalls == 1 and S.focusCalls[1] == T.FX.id, T.dump())
        assert(T.count("where 'Komplete Kontrol menu' was pressed: accepted") == 1, T.dump())
        assert(o.active and not o:_heldOnMenu() and T.count("[deactivate]") == 0, T.dump())
        assert(T.count("the plug-in has the keyboard again") == 1, T.dump())
        assert(S.gates > gates, "the gate is asked again")
        assert(T.holds("Tab") and S.scopedTo == T.FX.id and T.hotkeys() == 2, T.dump())
        assert(#S.speech == spoken, "nothing announced")
        T.tab()
        assert(#S.speech > spoken, "the first key after Escape is the overlay's")
        T.tick(10)
        assert(#S.focusCalls == 1 and o.active)
    "#);
}

/// An overlay with `pollMatch` (Kontakt's library overlays, under the KK header) rechecks on its
/// poll, which asks the tests itself and turns nothing over, so it reads the window in front that
/// host.window.active() kept from before the menu hid. A hold needs a test seeing the menu there
/// and then, so the poll's first miss ends it, while the word still says open — and the step runs
/// then, not on the tick that closes the menu: foreground() names the menu's own window, not
/// shown, and the window of the press is brought back. Not landed yet, nothing is in front to
/// active(), and the word still says open, so the hold stands over nothing in front; the landing,
/// with the keyboard in KK, ends it with the overlay never having left the front. The tick then
/// closes the menu and the hotkeys come back, nothing said, and the first key is the overlay's.
#[test]
fn a_poll_that_asks_the_tests_itself_brings_the_window_back_at_the_first_miss() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow }, nil, nil, 500)
        T.tick(4)
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        T.tick(2)
        -- Something turns the epoch over while the menu is up, and the poll reads the window in
        -- front in that epoch: the menu's.
        T.turn()
        T.poll(500)
        assert(o.active and o:_heldOnMenu() and #S.focusCalls == 0, "the poll sees the menu: " .. T.dump())
        -- Escape: the menu's window is hidden, with no event.
        T.hide(T.POPUP)
        S.windows = { base }
        S.focusLands = function(id) S.front, S.chain = T.FX, { T.KK, T.WRAP, T.FX } end
        local spoken, deact = #S.speech, T.count("[deactivate]")
        T.poll(500)
        assert(#S.focusCalls == 1 and S.focusCalls[1] == T.FX.id, "at the poll's first miss: " .. T.dump())
        assert(T.count("still gets the keyboard and is not shown — bringing back 'FX: Track 1 \"test\"' "
          .. "(reaper.exe), where 'Komplete Kontrol menu' was pressed: accepted") == 1, T.dump())
        assert(o._menu.open == true and T.count("the menu has closed") == 0, "the word still says open: " .. T.dump())
        assert(o.active and o:_heldOnMenu() and T.count("[deactivate]") == deact,
          "nothing in front while the word says open: the hold stands: " .. T.dump())
        -- Windows completes it with the keyboard in KK.
        T.land()
        assert(o.active and not o:_heldOnMenu() and T.count("[deactivate]") == deact, T.dump())
        assert(T.count("the plug-in has the keyboard again") == 1, T.dump())
        assert(T.holds("Tab") and T.holds("Return") and S.scopedTo == T.FX.id, T.dump())
        T.tick(2)
        assert(o._menu.open == false and T.open() == false and T.hotkeys() == 2, T.history(1))
        assert(T.count("took back its per-control hotkeys") == 1, T.dump())
        assert(#S.speech == spoken, "nothing announced")
        T.tab()
        assert(#S.speech > spoken, "the first key is the overlay's")
        T.tick(10)
        T.poll(500)
        T.event()
        assert(#S.focusCalls == 1, "once: " .. #S.focusCalls)
    "#);
}

/// The platform declines to bring the window back: the log says so, nothing is announced, and it
/// is not asked again — the hold is over, and the overlay leaves the front as for any window
/// change, until the ordinary match finds the plug-in again. A call that raises is declined too,
/// with its reason.
#[test]
fn a_declined_refocus_is_logged_announces_nothing_and_is_not_asked_again() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local popup = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        local function menuHidden()
          T.press(o)
          S.windows = popup
          T.show(T.POPUP)
          T.tick(2)
          assert(o:_heldOnMenu(), T.dump())
          T.hide(T.POPUP)
          S.windows = { base }
          T.tick(2)
        end
        -- The activation has spoken its control before anything below.
        T.tick(4)
        S.focusAnswer = false
        local spoken = #S.speech
        menuHidden()
        assert(#S.focusCalls == 1, T.dump())
        assert(T.count("where 'Komplete Kontrol menu' was pressed: declined") == 1, T.dump())
        assert(#S.speech == spoken, "nothing announced")
        -- The hold ends over the window the host kept for the epoch; the call turned the epoch
        -- over, so the deactivation reads the hidden window as nothing in front.
        assert(T.count("no longer holding its place — in front now: 'Komplete Kontrol' (reaper.exe)") == 1, T.dump())
        assert(not o.active and T.count("[deactivate] 'Komplete Kontrol' — in front now: 'nothing' (?)") == 1, T.dump())
        T.tick(10)
        T.event()
        assert(#S.focusCalls == 1, "not asked again")
        -- Windows brings the FX window back by itself later, with the keyboard in KK.
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active, T.dump())
        T.tick(3)
        -- A call that raises.
        S.focusAnswer, S.focusRaises = true, "window gone"
        spoken = #S.speech
        menuHidden()
        assert(#S.focusCalls == 2, T.dump())
        assert(T.count("where 'Komplete Kontrol menu' was pressed: declined (window gone)") == 1, T.dump())
        assert(#S.speech == spoken and not o.active, T.dump())
    "#);
}

/// Accepted, but nothing changed — the menu's hidden window still gets the keyboard. Once for one
/// press: the ordinary match, asked again, finds no plug-in, the hold is over, and nothing asks a
/// second time, on a tick or an event. A new press may bring the window back once more.
#[test]
fn the_window_of_the_press_is_brought_back_never_twice_for_one_press() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local popup = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        T.press(o)
        S.windows = popup
        T.show(T.POPUP)
        T.tick(2)
        T.hide(T.POPUP)
        S.windows = { base }
        T.tick(2)
        assert(#S.focusCalls == 1 and T.count(": accepted") == 1, T.dump())
        assert(not o.active, "nothing changed: the ordinary match finds no plug-in: " .. T.dump())
        assert(T.count("no longer holding its place — in front now: nothing") == 1, T.dump())
        local asks = S.fgAsks
        T.tick(10)
        for _ = 1, 3 do T.event() end
        assert(#S.focusCalls == 1, "once for the press: " .. #S.focusCalls)
        assert(S.fgAsks == asks, "and not even asked where the keyboard is, with no hold")
        -- Back in KK, and a new press: once more.
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active)
        T.tick(3)
        T.press(o)
        S.windows = popup
        T.show(T.POPUP)
        T.tick(2)
        T.hide(T.POPUP)
        S.windows = { base }
        T.tick(2)
        T.event()
        T.tick(4)
        assert(#S.focusCalls == 2, "once for the second press: " .. #S.focusCalls)
    "#);
}

/// The window of the press is brought back only while it is listed: a menu item can close the FX
/// window or unload the plug-in, and Windows gives a gone window's handle to a later one. Not
/// listed for its process any more, or listed with another class, it is not brought forward —
/// said once in the log, once per press. host.window.list lists titled windows only, so an
/// untitled window of the press is not brought back either, and the line says that is one of the
/// reasons; a listing that raises is logged with its reason.
#[test]
fn a_window_of_the_press_that_has_gone_or_whose_handle_was_reused_is_not_brought_back() {
    run(r#"
        local S = T.S
        local base = S.windows[1]
        local popup = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        local function menuHidden()
          S.listed = nil
          T.show(T.FX, { T.KK, T.WRAP, T.FX })
          assert(o.active, T.dump())
          T.tick(3)
          T.press(o)
          S.windows = popup
          T.show(T.POPUP)
          T.tick(2)
          assert(o:_heldOnMenu(), T.dump())
          T.hide(T.POPUP)
          S.windows = { base }
        end
        -- The FX window is gone.
        menuHidden()
        S.listed = { T.MAIN, T.OTHER }
        local lists = S.lists
        T.tick(2)
        assert(#S.focusCalls == 0 and not o.active, T.dump())
        assert(S.lists - lists == 1, "listed once: " .. (S.lists - lists))
        assert(S.listPids and #S.listPids == 1 and S.listPids[1] == 4242, "only the press's process is listed")
        local notListed = "is not listed for its process with its class (gone, untitled, or its id "
          .. "now another window's), so nothing is brought back"
        assert(T.count("[menu] 'Komplete Kontrol': no test sees the menu any more, but its window "
          .. "'Komplete Kontrol' (reaper.exe) still gets the keyboard and is not shown — the window of "
          .. "the press, 'FX: Track 1 \"test\"' (reaper.exe), " .. notListed) == 1, T.dump())
        T.tick(10)
        T.event()
        assert(#S.focusCalls == 0 and S.lists - lists == 1, "once per press")
        -- Its handle now belongs to another window of the process, of another class.
        menuHidden()
        S.listed = { T.MAIN, { id = T.FX.id, title = "Render to File", class = "REAPERRenderDlg",
          app = { pid = 4242, exe = "reaper.exe" }, bounds = T.FX.bounds, client = T.FX.client } }
        T.tick(2)
        assert(#S.focusCalls == 0 and not o.active, T.dump())
        assert(T.count(notListed) == 2, T.dump())
        -- The window of the press has no title: active() reports it, list() does not.
        T.FX.title = ""
        menuHidden()
        T.tick(2)
        T.FX.title = 'FX: Track 1 "test"'
        assert(#S.focusCalls == 0 and not o.active, T.dump())
        assert(T.count("the window of the press, '' (reaper.exe), " .. notListed) == 1, T.dump())
        -- The listing raises: not looked for, and said with its reason.
        menuHidden()
        S.listRaises = "no answer"
        T.tick(2)
        S.listRaises = nil
        assert(#S.focusCalls == 0 and not o.active, T.dump())
        assert(T.count("the window of the press, 'FX: Track 1 \"test\"' (reaper.exe), could not be "
          .. "looked for (no answer), so nothing is brought back") == 1, T.dump())
        assert(T.count(notListed) == 3, "not called gone: " .. T.dump())
        -- And listed as it was: brought back.
        menuHidden()
        T.tick(2)
        assert(#S.focusCalls == 1 and S.focusCalls[1] == T.FX.id, T.dump())
        assert(T.count("the keyboard goes to") == 0, "the reading matched every time: " .. T.dump())
    "#);
}

/// Only the menu's own window — the first window held over after the press — not shown and still
/// getting the keyboard, of the press's process. Nothing is brought back for: a dialog the menu led
/// to, hidden with the keyboard in it; a submenu that is a window of its own, held after the menu's
/// and hidden with the keyboard in it; the menu's window shown, with the keyboard in it, when a
/// module's own test stops seeing the menu there; REAPER's main window; the FX window itself;
/// another application; nothing at all; a reading that raises; a hidden window of another process
/// that has been given the menu's handle; and a hidden window of the process that was never held.
/// Wherever the hold ends with no test seeing the menu, the reading is in the log, once — it is what
/// says why nothing was brought back; a hold that ends on the window of the press or on another
/// application's reads nothing.
#[test]
fn the_window_of_the_press_comes_back_only_while_the_menus_own_window_gets_the_keyboard_unshown() {
    run(r##"
        local S = T.S
        local base = S.windows[1]
        local popWin = { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 }
        local PREFS = { id = 970, title = "Preferences", class = "Qt671QWindowIcon{prefs}",
          app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 200, y = 200, w = 600, h = 500 }, client = { x = 200, y = 200, w = 600, h = 500 } }
        local prefsWin = { id = 970, layer = 0, class = "Qt671QWindowIcon{prefs}", x = 200, y = 200, w = 600, h = 500 }
        local OK = { id = 971, class = "Qt671QWindowIcon{ok}", bounds = { x = 700, y = 650, w = 80, h = 30 } }
        local SUB = { id = 901, title = "Komplete Kontrol", class = "Qt671QWindowPopup",
          app = { pid = 4242, exe = "reaper.exe" }, bounds = { x = 580, y = 200, w = 180, h = 220 },
          client = { x = 580, y = 200, w = 180, h = 220 } }
        local subWin = { id = 901, layer = 0, class = "", x = 580, y = 200, w = 180, h = 220 }
        S.own = false
        local own = { name = "own", cheap = true, test = function(_, answer) answer(S.own) end }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow, own })
        -- The line a hold that ends with no test seeing the menu leaves when nothing is brought back.
        local function reading(where)
          return T.count("[menu] 'Komplete Kontrol': no test sees the menu any more; the keyboard goes "
            .. "to " .. where .. ", and the menu's window 'Komplete Kontrol' (reaper.exe) is id=900 of "
            .. "pid 4242 — nothing is brought back")
        end
        local function inKK()
          S.windows = { base }
          T.show(T.FX, { T.KK, T.WRAP, T.FX })
          assert(o.active, "in KK: " .. T.dump())
          T.tick(3)
        end
        local function held()
          inKK()
          T.press(o)
          S.windows = { base, popWin }
          T.show(T.POPUP)
          T.tick(2)
          assert(o:_heldOnMenu(), T.dump())
        end
        -- An item of the menu opens a dialog: the popup goes, and the dialog — a window that
        -- appeared since the press, with the keyboard in it — is held. The dialog is hidden with
        -- the keyboard still in it: not the menu's own window.
        held()
        S.windows = { base, prefsWin }
        T.show(PREFS, { OK, PREFS })
        T.tick(4)
        assert(o.active and o:_heldOnMenu(), "held over the dialog: " .. T.dump())
        T.hide(PREFS)
        S.windows = { base }
        T.tick(2)
        assert(not o.active and #S.focusCalls == 0, "a dialog the menu led to: " .. T.dump())
        assert(reading("id=970 of pid 4242, not shown") == 1, T.dump())
        -- A submenu that is a window of its own, held after the menu's window: both hidden, the
        -- keyboard in the submenu. Not the menu's own window either — the first one held over is.
        held()
        S.windows = { base, popWin, subWin }
        T.show(SUB)
        T.tick(2)
        assert(o.active and o:_heldOnMenu(), "held over the submenu: " .. T.dump())
        T.hide(T.POPUP)
        T.hide(SUB)
        S.windows = { base }
        T.tick(2)
        assert(not o.active and #S.focusCalls == 0, "a submenu window: " .. T.dump())
        assert(reading("id=901 of pid 4242, not shown") == 1, T.dump())
        -- The menu's own window, shown, with the keyboard in it. A module's own test is what sees
        -- the menu (the window was there before the press, so newWindow does not count it), and it
        -- stops seeing one there.
        inKK()
        S.windows = { base, popWin }
        T.press(o)
        S.own = true
        T.show(T.POPUP)
        assert(o:_heldOnMenu(), T.dump())
        S.own = false
        T.tick(4)
        assert(not o.active and #S.focusCalls == 0, "the menu's window is shown: " .. T.dump())
        assert(reading("id=900 of pid 4242, shown") == 1, T.dump())
        -- REAPER's main window.
        held()
        S.windows = { base }
        T.show(T.MAIN)
        assert(not o.active and #S.focusCalls == 0, "REAPER's main window: " .. T.dump())
        assert(reading("id=100 of pid 4242, shown") == 1, T.dump())
        -- The FX window itself, the keyboard on it: REAPER's, and the window of the press. The hold
        -- ends on the window of the press, which reads nothing.
        local fgAsks = S.fgAsks
        held()
        S.windows = { base }
        T.show(T.FX)
        assert(not o.active and #S.focusCalls == 0, "the FX window: " .. T.dump())
        -- Another application: the same.
        held()
        S.windows = { base }
        T.show(T.OTHER)
        assert(not o.active and #S.focusCalls == 0, "another application: " .. T.dump())
        assert(S.fgAsks == fgAsks and T.count("the keyboard goes to") == 4, T.dump())
        -- Nothing at all.
        held()
        S.windows = { base }
        T.show(nil)
        T.tick(2)
        assert(not o.active and #S.focusCalls == 0, "nothing in front: " .. T.dump())
        assert(reading("no window the platform names") == 1, T.dump())
        -- A reading that raises: nothing is known, so nothing is done.
        held()
        T.hide(T.POPUP)
        S.windows = { base }
        S.fgRaises = "no answer"
        T.tick(2)
        S.fgRaises = nil
        assert(not o.active and #S.focusCalls == 0, "a reading that raises: " .. T.dump())
        assert(reading("a window that could not be read (no answer)") == 1, T.dump())
        -- Another process's hidden window, given the menu's handle.
        held()
        S.windows = { base }
        S.front = { id = 900, title = "", class = "Chrome_WidgetWin_0", hidden = true,
          app = { pid = 777, exe = "chrome.exe" }, bounds = T.POPUP.bounds, client = T.POPUP.client }
        T.tick(2)
        assert(not o.active and #S.focusCalls == 0, "another process: " .. T.dump())
        assert(reading("id=900 of pid 777, not shown") == 1, T.dump())
        -- A hidden window of the process that was never held: a press, and the popup gets the
        -- keyboard hidden before any test saw it.
        inKK()
        local asks = S.fgAsks
        T.press(o)
        S.front = T.POPUP
        T.hide(T.POPUP)
        T.event()
        assert(not o.active and #S.focusCalls == 0, "never held: " .. T.dump())
        assert(S.fgAsks == asks, "not even asked, with no hold")
        assert(T.count("bringing back") == 0 and T.count("is not listed for its process") == 0, T.dump())
        assert(T.count("the keyboard goes to") == 7,
          "one reading per hold that ended with no test seeing the menu: " .. T.dump())
    "##);
}

/// A dialog the menu led to closes, and the menu's own window — hidden — gets the keyboard back:
/// that is the menu's own window, so the window of the press is brought back, once. The rule is
/// where the keyboard is, not which window the hold was over last. Windows completes it later, as
/// in the KK case: the overlay leaves the front, and the landing with the keyboard in KK brings it
/// back through an activation.
#[test]
fn the_menus_own_window_getting_the_keyboard_back_after_a_dialog_brings_the_window_of_the_press_back() {
    run(r##"
        local S = T.S
        local base = S.windows[1]
        local PREFS = { id = 970, title = "Preferences", class = "Qt671QWindowIcon{prefs}",
          app = { pid = 4242, exe = "reaper.exe" },
          bounds = { x = 200, y = 200, w = 600, h = 500 }, client = { x = 200, y = 200, w = 600, h = 500 } }
        local OK = { id = 971, class = "Qt671QWindowIcon{ok}", bounds = { x = 700, y = 650, w = 80, h = 30 } }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        T.press(o)
        S.windows = { base, { id = 900, layer = 0, class = "", x = 400, y = 180, w = 180, h = 220 } }
        T.show(T.POPUP)
        S.windows = { base, { id = 970, layer = 0, class = "Qt671QWindowIcon{prefs}", x = 200, y = 200, w = 600, h = 500 } }
        T.show(PREFS, { OK, PREFS })
        T.tick(3)
        assert(o:_heldOnMenu(), T.dump())
        -- The dialog closes; the menu's hidden window gets the keyboard again.
        S.windows = { base }
        S.front = T.POPUP
        T.hide(T.POPUP)
        T.event()
        S.focusLands = function(id) S.front, S.chain = T.FX, { T.KK, T.WRAP, T.FX } end
        T.tick(2)
        assert(#S.focusCalls == 1 and S.focusCalls[1] == T.FX.id, T.dump())
        assert(T.count("its window 'Komplete Kontrol' (reaper.exe) still gets the keyboard and is not shown") == 1, T.dump())
        assert(not o.active and T.count("[deactivate] 'Komplete Kontrol' — in front now: 'nothing' (?)") == 1, T.dump())
        local spoken = #S.speech
        T.land()
        assert(o.active and T.holds("Tab") and T.hotkeys() == 2, T.dump())
        T.run(600)
        assert(#S.speech > spoken, "the activation speaks its control")
        assert(#S.focusCalls == 1)
    "##);
}

/// On a Mac the window that gets the keyboard is the frontmost application's focused window, and
/// it cannot be read while that application does not answer: host.window.foreground() is nil then,
/// and nothing is brought back — even while host.window.active() serves the menu's window from
/// memory and the focus chain is empty, the state an earlier version acted on. A focused menu
/// window that is shown does not bring anything back either.
#[test]
fn on_a_mac_nothing_is_brought_back_from_an_application_that_does_not_answer_or_a_shown_window() {
    run_mac(r#"
        local S = T.S
        local base = S.windows[1]
        local popup = { base, { id = 900, layer = 101, class = "", x = 400, y = 180, w = 180, h = 220 } }
        local o = T.embedded({ T.O.menuTests.nativePopup, T.O.menuTests.newWindow })
        assert(o.active, T.dump())
        T.press(o)
        S.windows = popup
        T.show(T.POPUP)
        T.tick(2)
        assert(o:_heldOnMenu(), T.dump())
        -- The menu closes and REAPER stops answering.
        S.windows = { base }
        S.busy = true
        T.turn()
        assert(T.host.window.active() == T.POPUP and #T.host.window.focusChain() == 0)
        local asks = S.fgAsks
        T.tick(2)
        assert(S.fgAsks - asks == 1 and #S.focusCalls == 0, "unreadable: " .. T.dump())
        assert(not o.active and T.count("no longer holding its place — in front now: 'Komplete Kontrol' (reaper.exe)") == 1, T.dump())
        assert(T.count("no test sees the menu any more; the keyboard goes to no window the platform "
          .. "names, and the menu's window 'Komplete Kontrol' (reaper.exe) is id=900 of pid 4242 — "
          .. "nothing is brought back") == 1, "the reading, not the window served from memory: " .. T.dump())
        -- It answers again, the keyboard in KK.
        S.busy = false
        T.show(T.FX, { T.KK, T.WRAP, T.FX })
        assert(o.active, T.dump())
        T.tick(3)
        -- The menu's window stays the focused window once no test sees a menu there, and reads as
        -- shown, as any focused window does here that is not minimised.
        T.press(o)
        S.windows = popup
        T.show(T.POPUP)
        T.tick(2)
        assert(o:_heldOnMenu(), T.dump())
        S.windows = { base }
        T.tick(2)
        assert(not o.active and #S.focusCalls == 0, "shown: " .. T.dump())
        assert(T.count("the keyboard goes to id=900 of pid 4242, shown") == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// Declaring the tests.
// ---------------------------------------------------------------------------------------------

/// `menus = true` raises at attach, naming the building blocks; so does a list with a gap, an
/// entry that is not a test, and a mistyped building block.
#[test]
fn menus_true_raises_naming_the_building_blocks() {
    run(r#"
        local o = T.O.new("sforzando")
        local ok, err = pcall(function() o:attach({ title = "sforzando" }, { menus = true }) end)
        assert(not ok, "menus = true must raise")
        err = tostring(err)
        for _, name in ipairs({ "'sforzando'", "`menus = true`", "O.menuTests.nativePopup",
          "O.menuTests.newWindow", "O.menuTests.accessibility", "function(o, answer)" }) do
          assert(string.find(err, name, 1, true), name .. " missing from: " .. err)
        end
        ok, err = pcall(function() o:_watchMenus({ T.O.menuTests.nativePopup, 42 }) end)
        assert(not ok and string.find(tostring(err), "menu test 2 is a number", 1, true), tostring(err))
        ok, err = pcall(function() o:_watchMenus({ nil, T.O.menuTests.nativePopup }) end)
        assert(not ok and string.find(tostring(err), "not in the list", 1, true), tostring(err))
        ok, err = pcall(function() return T.O.menuTests.newwindow end)
        assert(not ok and string.find(tostring(err), "nativePopup, newWindow, accessibility and accessibilityAfterPress", 1, true), tostring(err))
        ok, err = pcall(function() o:_watchMenus({ { name = "x", test = function() end, pressed = 1 } }) end)
        assert(not ok and string.find(tostring(err), "`pressed` must be a function", 1, true), tostring(err))
        -- `false` and an empty list are "no tests", and run nothing.
        local q = T.O.new("quiet")
        q:_watchMenus({})
        assert(q._menu == nil)
        assert(T.S.every == nil, "no timer for an overlay without tests")
    "#);
}

/// With no tests, a press does nothing to the keys, and the declaration report says so.
#[test]
fn a_menu_opening_control_without_tests_is_reported_and_keeps_the_keys() {
    run(r#"
        local S = T.S
        local o = T.overlay(nil, "No tests")
        T.press(o)
        T.run(2000)
        assert(#S.menuOpen == 0, "nothing writes the flag: " .. T.history(1))
        assert(T.count("gave up") == 0)
        -- The report runs where the overlay binds.
        local p = T.O.new("Unwatched")
        p:addHotspotButton({ label = "Menu", at = { 10, 10 }, opensMenu = true })
        rawset(T.host.window, "onTrigger", function() end)
        rawset(T.host.window, "onFocus", function() end)
        rawset(T.host.window, "test", function() return false end)
        p:attach({ title = "x" })
        assert(T.count("'Menu') opens a menu, but the overlay lists no menu tests") == 1, T.dump())
    "#);
}

// ---------------------------------------------------------------------------------------------
// Calibration.
// ---------------------------------------------------------------------------------------------

/// In a calibrating run, pressing a control that opens a menu saves three pictures of the
/// origin's rectangle — before, ~600 ms and ~1500 ms after — numbered so nothing is overwritten.
/// In a normal run it saves none.
#[test]
fn menu_shots_are_taken_in_a_calibrating_run_only() {
    run(r#"
        local S = T.S
        local o = T.overlay({ T.O.menuTests.nativePopup })
        assert(S.captured["Ctrl+Alt+Shift+S"] == nil, "no calibration keys outside a calibrating run")
        T.press(o)
        T.run(3000)
        assert(#S.shots == 0 and S.snaps == 0 and #S.asyncs == 0, "no pictures outside a calibrating run")
        S.calibrating = true
        local pressedAt = S.now
        T.press(o)
        T.run(2000)
        assert(#S.shots == 3, #S.shots .. " pictures")
        local base = "C:/modules/overlay-runtime/calibration/Kontakt-8-in-a-DAW-Snapshot-menu"
        -- Written in the order they are in: the later two, then the one of the press.
        assert(S.shots[1].path == base .. "-menu-after-600.png", S.shots[1].path)
        assert(S.shots[2].path == base .. "-menu-after-1500.png", S.shots[2].path)
        assert(S.shots[3].path == base .. "-menu-before.png", S.shots[3].path)
        assert(S.shots[3].taken == pressedAt, "the before picture is of the press")
        assert(S.shots[1].taken - pressedAt == 600, S.shots[1].taken - pressedAt)
        assert(S.shots[2].taken - pressedAt == 1500, S.shots[2].taken - pressedAt)
        for _, s in ipairs(S.shots) do
          assert(s.snapshot ~= nil and s.snapshot.released, "written from a snapshot, which is let go")
          local r = s.region
          assert(r[1] == 92 and r[2] == 19 and r[3] == 908 and r[4] == 658,
            "the origin's whole rectangle: " .. table.concat(r, ","))
        end
        assert(T.count("menu shot just before the control acts, (92,19)-(908,658), content at (100,50)") == 1, T.dump())
        assert(T.count(base .. "-menu-after-1500.png (true)") == 1, T.dump())
        -- The next press is numbered, and so is one after a restart that finds files on disk.
        T.press(o)
        T.run(2000)
        assert(S.shots[5].path == base .. "-2-menu-after-1500.png", S.shots[5].path)
        assert(S.shots[6].path == base .. "-2-menu-before.png", S.shots[6].path)
    "#);
}

/// The menu shots put one capture between the key and the click and nothing else: no picture is
/// written and no control's point is looked up before the control acts. The later two are
/// captured off the event loop at their time and written when they arrive, and the first is
/// written after them — not straight after the click, when the menu's first answer and the user's
/// first key arrive. The 2026-09-26 session: ~700 ms before Diva's preset menu opened, in a debug
/// build, because the first PNG was encoded before the click.
#[test]
fn the_menu_shots_put_one_capture_before_the_click_and_write_afterwards() {
    run(r#"
        local S = T.S
        S.calibrating = true
        local o = T.overlay({ T.O.menuTests.nativePopup })
        -- A control whose visibility and point cost a lookup each: not asked on the press.
        local asked = 0
        o:addHotspotButton({ label = "Costly", at = function() asked += 1; return { 10, 10 } end,
          when = function() asked += 1; return true end })
        local pressedAt = S.now
        local from = #S.order
        T.press(o)
        assert(S.order[from + 1] == "snapshot" and S.order[from + 2] == "acted" and #S.order == from + 2,
          "one capture, then the control acts, and nothing is written yet: "
            .. table.concat(S.order, ",", from + 1))
        assert(asked == 0, "no control's point or visibility asked before the click: " .. asked)
        assert(#S.shots == 0 and S.snaps == 1, "the picture is kept, not written")
        assert(#S.asyncs == 2 and S.asyncs[1].at == pressedAt + 600 and S.asyncs[2].at == pressedAt + 1500,
          "the later two are asked of the capture thread for their time")
        T.tick(3)
        assert(#S.shots == 0, "nothing written while the menu opens")
        T.tick(1)
        assert(#S.shots == 1 and S.shots[1].taken == pressedAt + 600, "the 600 ms one when it arrives")
        T.run(2000)
        assert(#S.shots == 3 and S.snaps == 1, "the later two captured off the event loop: " .. S.snaps)
        assert(S.shots[3].taken == pressedAt and string.find(S.shots[3].path, "-menu-before.png", 1, true),
          "the one of the press written last: " .. S.shots[3].path)
        -- The harness's `save` refuses a released snapshot, so it was kept until it was written.
        assert(S.shots[3].snapshot.released, "and let go after")
        assert(T.count("(true)") == 3, T.dump())
        -- A capture that fails says why, and neither the control nor the other pictures stop.
        rawset(T.host.screen, "snapshot", function() error("the budget is spent") end)
        local acted = S.acted
        T.press(o)
        assert(S.acted == acted + 1, "the control acted")
        T.run(2000)
        assert(T.count("menu shot just before the control acts") == 2, T.dump())
        assert(T.count("(false: ") == 1 and T.count("the budget is spent") == 1, T.dump())
        assert(#S.shots == 5, "the other two were written: " .. #S.shots)
    "#);
}

/// The numbering is seeded from the files already on disk. And in a calibrating run the overlay
/// in front holds the three calibration keys, said once in the log.
#[test]
fn menu_shots_do_not_overwrite_earlier_ones() {
    run(r#"
        local S = T.S
        S.calibrating = true
        S.exists["calibration/Kontakt-8-in-a-DAW-Snapshot-menu-menu-before.png"] = true
        S.exists["calibration/Kontakt-8-in-a-DAW-Snapshot-menu-2-menu-before.png"] = true
        local o = T.overlay({ T.O.menuTests.nativePopup })
        for _, k in ipairs({ "Ctrl+Alt+Shift+S", "Ctrl+Alt+Shift+T", "Ctrl+Alt+Shift+V" }) do
          assert(S.captured[k] ~= nil, k .. " is not captured")
        end
        assert(T.count("[calibrate] armed") == 1, T.dump())
        T.press(o)
        T.run(2000)
        assert(S.shots[3].path == "C:/modules/overlay-runtime/calibration/Kontakt-8-in-a-DAW-Snapshot-menu-3-menu-before.png",
          S.shots[3].path)
    "#);
}
