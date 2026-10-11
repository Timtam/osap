//! The HTML page probe (`tools/html-probe`), loaded for real against the scripted host of
//! `overlay_menu_tests.rs`, with daw-hosts and the window prelude's real matcher as
//! `overlay_host_panel_tests.rs` has them, and a scripted `host.gui.webview`: a page object that
//! records what it was asked and answers what the scenario scripts.
//!
//! Written for the review of 2026-09-28: the probe is what the maintainer runs with NVDA, once,
//! and a mistake in it found then costs the session. So its decisions are pinned here, with no
//! window: the first show builds and the page comes when it is ready; the show key pressed in the
//! page goes back — the plug-in forward first, the page hidden after, and kept when Windows declines
//! — and says where the keyboard landed; the report says what happened since it was last pressed, counts only the keys a person
//! pressed in the page (not the report chord's own modifiers), and starts anew; twenty presses of
//! one button are twenty, and a press that arrives twice is said; W3 clicks only once the plug-in
//! window is in front — onto its own background, never its title bar, never where the last one
//! went, or not at all — brings the page back in the answer of a picture, says whether the plug-in
//! window kept its place and size, never starts a second round trip on top of one, and says when
//! the page loses the front again; a page shown over the plug-in window is said once; the page is
//! prepared when a plug-in window first comes forward, and said when it is ready; the first report
//! says the probe's revision; and the automatic show comes for an arrival, never for a return.
//! Revised after the maintainer's first NVDA run (2026-09-28) and its review. Revision 4
//! (2026-10-11): the same page in an application window too, for W6 again — built at the first
//! switch to it, not with the tool page; the page's switch button shows the other beside the
//! plug-in window first and hides the one after, so the front never passes through the plug-in
//! window, and keeps the one when the other is not shown; the keys work on the one shown last; the
//! way back leaves the application window open, and gives up a switch still waiting for its build;
//! and the automatic show never brings the application window. Revision 6 (2026-10-12): the probe
//! on a Mac, for the tester's VoiceOver session (MW1–MW19 in TODO.md) — its keys Command-Shift
//! chords off VoiceOver's Control-Option and off the window probe's F9 and F10; F8 the same page
//! described `activates = false` (MW1b), the other pages left as they are; a captured key that
//! shows the page F7 shows without the keyboard and then updates its live region (MW2, MW16), and
//! says so if it ever fires in a window of the application's own; W3's click on the plug-in
//! window's title bar, away from the last one; the page's modifiers in a Mac's words; a show's
//! `true` said as the front asked for, the page's focus line its answer; W3 going on when a focus
//! event finds the plug-in window with the keyboard, for the plug-in window that comes with no
//! activation; the window server asked where the page covers the plug-in window (MW4); and the
//! automatic show never bringing the page without activation — and on Windows everything as it
//! was, but for W3 going on on such a focus event there too. Revision 5 (2026-10-11, on another
//! line of work): W10 on Windows — F10 remembers the pointer's point in the plug-in window; the
//! page reads the plug-in's elements and tries a way of acting in it, once or in a series, each
//! attempt step by step on the answer of the one before and said without cutting NVDA off; the
//! round trip clicks on the plug-in window's arrival and brings the page back after the change
//! wait; a show key gives a run up. Revision 7 (2026-10-12): the two together — W10 on Windows
//! as revision 5 had it, but for its dump and an element's state, asked with a callback now, and
//! on a Mac no W10 key, the pages told that W10 is not offered there, and a W10 request that came
//! all the same logged, not run.
//!
//! **Every event through the real mailbox**, as the host delivers it under way B: a key of the
//! probe's, a page's message, its `onReady` and `onClose`, a window and a focus event, the
//! picture's answer and a setting changed in the dialog each run as a handler of the probe's
//! module (`T.call`, `T.later`) — each checked to have run at once, as the probe never waits —
//! and every scenario ends on the harness's own checks (`finish`): nothing asked of the scripted
//! host that it does not have, no handler that raised. The harness
//! is the host-panel tests' (`run_with_helpers`), so what the scripted host gains, the probe's
//! scenarios get. (Written before way B, these loaded the harness's Luau half alone; since B1a its
//! `T.runDue` hands a picture's answer to `T.call`, which only the Rust half gives, and the three
//! W3 scenarios stopped at their first `T.runDue`.)

use super::overlay_host_panel_tests::run_with_helpers;

/// The probe's helpers, on top of the host-panel ones: the calls the probe makes that the
/// harness does not script, the page, and the probe loaded.
const PR: &str = r##"
local S = T.S
T.F7, T.F8, T.F9 = "Ctrl+Shift+Win+Alt+F7", "Ctrl+Shift+Win+Alt+F8", "Ctrl+Shift+Win+Alt+F9"
T.F10 = "Ctrl+Shift+Win+Alt+F10"

-- An event of the probe's module through its mailbox. The probe never waits, so each runs at
-- once: one that waited or was dropped would leave a check that something did not happen
-- nothing to check, and stops the scenario here instead.
local function ran(kind, f, ...)
  local got = T.call(kind, f, ...)
  assert(got == "Ran", kind .. ": " .. got)
end

-- The page's own window: a window of this application. And the same page's application window,
-- and on a Mac the page without activation's.
T.PAGE = { id = 4000, title = "HTML probe", class = "wxWindowNR", app = { pid = 9, exe = "automation-platform.exe" },
  bounds = { x = 1000, y = 0, w = 520, h = 640 }, client = { x = 1008, y = 30, w = 504, h = 602 }, own = true }
T.APP_PAGE = { id = 4001, title = "HTML probe window", class = "wxWindowNR", app = { pid = 9, exe = "automation-platform.exe" },
  bounds = { x = 1000, y = 0, w = 520, h = 640 }, client = { x = 1008, y = 30, w = 504, h = 602 }, own = true }
T.NA_PAGE = { id = 4002, title = "HTML probe without activation", class = "AXWindow/AXFloatingWindow/",
  app = { pid = 9, exe = "automation-platform" }, bounds = { x = 1000, y = 38, w = 520, h = 640 },
  client = { x = 1000, y = 60, w = 520, h = 618 }, own = true }

-- host.settings: defined with a default, read, and changed by the scenario (T.set).
S.defined, S.settings, S.settingCbs = {}, {}, {}
rawset(T.host, "settings", T.strict("host.settings", {
  define = function(k, default, opts)
    S.defined[k] = { default = default, label = opts and opts.label }
    if S.settings[k] == nil then S.settings[k] = default end
  end,
  get = function(k)
    assert(S.defined[k] ~= nil, "host.settings.get: '" .. tostring(k) .. "' was never defined")
    return S.settings[k]
  end,
  onChange = function(k, cb) S.settingCbs[k] = cb end,
}))
-- Changed in the module manager's dialog: the value at once, its onChange through the mailbox
-- always, run in the next tick's queued phase — as the host delivers a setting from the dialog.
function T.set(k, v)
  S.settings[k] = v
  local cb = S.settingCbs[k]
  if cb then
    local got = T.later("setting", cb, v)
    assert(got == "Queued", "setting: " .. got)
    assert(T.queued() == 1, "the setting alone waited, and ran")
  end
end
rawset(T.host, "json", T.strict("host.json", { encode = function() return "{...}" end }))

-- host.window.onTrigger (with or without opts) and onFocus, apart: T.arrive plays a window
-- coming to the front, T.focusMoves the keyboard moving inside it — each callback an event of the
-- module's, through its mailbox.
S.fgCbs, S.focusCbs = {}, {}
rawset(T.host.window, "onTrigger", function(_, opts, cb)
  if cb == nil then cb = opts end
  S.fgCbs[#S.fgCbs + 1] = cb
end)
rawset(T.host.window, "onFocus", function(cb) S.focusCbs[#S.focusCbs + 1] = cb end)
function T.arrive(win, chain)
  S.front, S.chain = win, chain or { win }
  S.epoch += 1
  for _, cb in ipairs(S.fgCbs) do ran("window", cb, win) end
end
function T.focusMoves(chain)
  S.chain = chain
  S.epoch += 1
  for _, cb in ipairs(S.focusCbs) do ran("focus", cb) end
end
S.owns = true
S.ownsAsked = {}
rawset(T.host.window, "ownsPoint", function(id, x, y)
  S.ownsAsked[#S.ownsAsked + 1] = { id, x, y }
  return S.owns
end)
do
  local focus = T.host.window.focus
  rawset(T.host.window, "focus", function(id)
    S.order[#S.order + 1] = "focus " .. tostring(id)
    return focus(id)
  end)
end

-- host.gui.webview: the pages — the tool page (S.pg, S.page, S.pageOpts), the application window
-- (`window = "app"`: S.apg, S.appPage, S.appOpts) and the tool page described `activates = false`
-- (S.npg, S.naPage, S.naOpts), each described once. `show` answers the page's `answers` in turn
-- ({ true } when they run out); accepted with focus, the page is in front — on a Mac (T.mac) only
-- once AppKit makes its window the key window (T.keyed), as this application's activation comes on
-- the event loop after the call, unless the application is the active one already (one of its
-- windows in front, but for the page without activation, which has the keyboard only lent) or the
-- page is the one without activation, whose window is the key window at the call. Declined, it is
-- shown and the window in front stays. `prepare` answers true once the page is ready (T.ready,
-- T.appReady, T.naReady), "not ready yet" before. S.order names the application window's shows
-- and hides "show app" and "hide app", the page without activation's "show na" and "hide na".
local function pageState()
  return { shows = {}, hides = 0, posts = {}, answers = {}, shown = false, prepares = 0, ready = false }
end
S.pg, S.apg, S.npg = pageState(), pageState(), pageState()
rawset(T.host, "gui", T.strict("host.gui", {
  webview = function(opts)
    local isApp = opts.window == "app"
    local isNa = opts.activates == false
    assert(opts.window == nil or isApp, "the tool page by default, or an application window: " .. tostring(opts.window))
    assert(opts.activates == nil or (isNa and not isApp), "activates = false, for a tool page, or nothing: " .. tostring(opts.activates))
    local st, win, tag, described = S.pg, T.PAGE, "", S.page
    if isApp then
      st, win, tag, described = S.apg, T.APP_PAGE, " app", S.appPage
    elseif isNa then
      st, win, tag, described = S.npg, T.NA_PAGE, " na", S.naPage
    end
    assert(described == nil, "each page described once")
    local p = {}
    function p.show(self, o)
      assert(self == p, "page:show is a method")
      st.shows[#st.shows + 1] = { focus = o and o.focus, avoid = o and o.avoid, at = S.now }
      S.order[#S.order + 1] = "show" .. tag
      local a = table.remove(st.answers, 1) or { true }
      if a[1] or a[2] == "declined by the system" then
        st.shown = true
        p.window = win.id
      end
      if a[1] and not (o and o.focus == false) then
        if S.asks and not isNa and not (S.front and S.front.own and S.front ~= T.NA_PAGE) then
          S.asked = win
        else
          S.front, S.chain = win, { win }
          S.epoch += 1
        end
      end
      return a[1], a[2]
    end
    function p.hide(self)
      assert(self == p, "page:hide is a method")
      st.hides += 1
      st.shown = false
      S.order[#S.order + 1] = "hide" .. tag
    end
    function p.post(self, v) st.posts[#st.posts + 1] = v end
    function p.prepare(self)
      assert(self == p, "page:prepare is a method")
      st.prepares += 1
      if st.ready then return true end
      return false, "not ready yet"
    end
    function p.isShown(self) return st.shown end
    function p.focus(self) return st.shown end
    function p.close(self) st.shown = false; p.window = nil end
    if isApp then
      S.appPage, S.appOpts = p, opts
    elseif isNa then
      S.naPage, S.naOpts = p, opts
    else
      S.page, S.pageOpts = p, opts
    end
    return p
  end,
}))

-- The probe, loaded as the host loads it, with daw-hosts beside it; its hotkeys' options kept.
function T.probe()
  T.modules("tools/html-probe/")
  S.hotkeyOpts = {}
  local register = T.host.hotkey.register
  rawset(T.host.hotkey, "register", function(spec, fn, opts)
    S.hotkeyOpts[spec] = opts or false
    return register(spec, fn)
  end)
  -- W10's reads of the plug-in: the pointer (T.cursor, default inside the FX client), and the
  -- plug-in's elements (S.dump, a list of { name, ctype, bounds } as host.element.dump gives),
  -- asked with a callback — never waited for — and answered by T.dumped.
  S.cursor = S.cursor or { x = 120, y = 90 }
  rawset(T.host.input, "cursorPos", function() return S.cursor end)
  S.dump = S.dump or {}
  S.dumpCbs = {}
  rawset(T.host.element, "dump", function(id, cb)
    assert(type(cb) == "function", "the probe asks for the plug-in's elements with a callback")
    S.dumpedId = id
    S.dumpCbs[#S.dumpCbs + 1] = cb
  end)
  -- W10's acting, recorded: a click with options or a callback in S.acts, its callback answered
  -- on a later tick through the module's mailbox (T.acted), as the host answers it; a plain click
  -- — W3's, the round trip's — in S.clicks as ever; the pointer put somewhere in S.moves.
  S.acts, S.actedCbs, S.moves = {}, {}, {}
  rawset(T.host.input, "click", function(x, y, opts, cb)
    if opts == nil and cb == nil then
      S.clicks[#S.clicks + 1] = { x, y }
    else
      S.acts[#S.acts + 1] = { x = x, y = y, via = opts and opts.via, window = opts and opts.window, move = opts and opts.move }
    end
    if cb then S.actedCbs[#S.actedCbs + 1] = cb end
    return true
  end)
  rawset(T.host.input, "move", function(x, y) S.moves[#S.moves + 1] = { x, y } end)
  -- host.screen.snapshot with a callback as the harness takes it, with its change wait and key
  -- kept; without one, the scripted host's own.
  local snapshot = T.host.screen.snapshot
  rawset(T.host.screen, "snapshot", function(opts, cb)
    if cb == nil then return snapshot(opts) end
    snapshot(opts, cb)
    local r = S.asyncs[#S.asyncs]
    r.change, r.key = opts.change, opts.key
  end)
  T.source("tools/html-probe/src/main.luau")(T.host)
end

-- W10: every click callback waiting answered `(made, why)`, each through the module's mailbox,
-- as the host answers one on a later tick.
function T.acted(made, why)
  local cbs = S.actedCbs
  S.actedCbs = {}
  for _, cb in ipairs(cbs) do ran("acted", cb, made, why) end
end
-- W10: every dump waiting answered with S.dump (and `why`, for one that is not all of the tree),
-- each through the module's mailbox, as the host answers one on a later turn.
function T.dumped(why)
  local cbs = S.dumpCbs
  S.dumpCbs = {}
  for _, cb in ipairs(cbs) do ran("dumped", cb, S.dump, why) end
end
-- The oldest read with a callback not answered yet, answered with `text` ("" reads nothing).
function T.readNext(text)
  for _, r in ipairs(S.reads) do
    if not r.answered then
      r.answered = true
      S.epoch += 1
      ran("answer", r.cb, { status = text ~= "" and "text" or "none", text = text, lines = {}, words = {} })
      return r
    end
  end
  error("no read is waiting: " .. T.dump())
end
-- The oldest picture asked with a callback, answered now with `info` (a plain one's by default);
-- the snapshot it handed over.
function T.picture(info)
  local r = table.remove(S.asyncs, 1)
  assert(r ~= nil, "no picture is waiting: " .. T.dump())
  S.epoch += 1
  local s = T.snap(r.region)
  ran("answer", r.cb, s, nil, info or { waited = 0, frames = 1 })
  return s
end

-- A key of the probe's pressed: the epoch turns over, and its callback runs as a handler.
function T.key(spec)
  local fn = S.hotkeyFns[spec]
  assert(fn ~= nil, "no hotkey " .. spec)
  S.epoch += 1
  ran("hotkey", fn)
end
-- The page sends `m`; the page is ready; the page's window goes ("user" or "failed"): each the
-- page's event, through the module's mailbox, as the host hands it on. The `app` ones are the
-- application window's.
function T.msg(m) ran("page message", S.pageOpts.onMessage, m) end
function T.appMsg(m) ran("page message", S.appOpts.onMessage, m) end
function T.ready()
  S.pg.ready = true
  ran("page ready", S.pageOpts.onReady)
end
function T.appReady()
  S.apg.ready = true
  ran("page ready", S.appOpts.onReady)
end
function T.naMsg(m) ran("page message", S.naOpts.onMessage, m) end
function T.naReady()
  S.npg.ready = true
  ran("page ready", S.naOpts.onReady)
end
function T.closed(why) ran("page close", S.pageOpts.onClose, why) end
function T.appClosed(why) ran("page close", S.appOpts.onClose, why) end
-- A button of the page activated: `released` whether a key or a pointer went up in the page since
-- the button before, as the page reports it (true unless said otherwise). Each page counts its own
-- presses, as page.js does.
local presses = { tool = { counts = {}, previous = nil }, app = { counts = {}, previous = nil }, na = { counts = {}, previous = nil } }
local function pressIn(which, send, action, released, detail)
  local c = presses[which]
  c.counts[action] = (c.counts[action] or 0) + 1
  local m = { kind = "action", action = action, count = c.counts[action], detail = detail or 1,
    previous = c.previous, released = released ~= false, releases = released ~= false and { "mouseup" } or {} }
  c.previous = action
  send(m)
end
function T.press(action, released, detail) pressIn("tool", T.msg, action, released, detail) end
function T.appPress(action, released, detail) pressIn("app", T.appMsg, action, released, detail) end
function T.naPress(action, released, detail) pressIn("na", T.naMsg, action, released, detail) end

-- What was said last, and everything said.
function T.last() return S.speech[#S.speech] and S.speech[#S.speech].text or "" end
function T.said()
  local out = {}
  for _, s in ipairs(S.speech) do out[#out + 1] = s.text end
  return table.concat(out, " | ")
end
-- The report key's words.
function T.report()
  T.key(T.F9)
  return T.last()
end
function T.has(text, part) return string.find(text, part, 1, true) ~= nil end

-- An assertion whose failure prints the whole log and everything said.
function T.ok(cond, what)
  if cond then return end
  print(T.dump())
  print("said: " .. T.said())
  error(tostring(what or "assertion failed"), 2)
end

-- The page shown beside the FX window and in front, from F7, its build over.
function T.shown()
  S.front = T.FX
  T.turn()
  S.pg.answers = { { false, "not ready yet" } }
  T.key(T.F7)
  S.now += 300
  T.ready()
  T.ok(S.front == T.PAGE, "the page is in front")
  T.report()
end

-- On a Mac: the probe's keys there, by their Windows names — F9's role is Command-Shift-F11, the
-- window probe holding Command-Shift-F9 — REAPER's floating plug-in window (T.FLOAT) in front, and
-- a show with focus that asks for the front (S.asks).
function T.mac()
  T.F7, T.F8, T.F9, T.F12 = "Cmd+Shift+F7", "Cmd+Shift+F8", "Cmd+Shift+F11", "Cmd+Shift+F12"
  S.listed = T.LISTED_MAC
  S.front = T.FLOAT
  S.asks = true
end
-- On a Mac, the page whose show asked for the front gets it: this application's activation names
-- the page's window in front, and the page's document has the focus — each an event of the
-- module's.
function T.keyed(win)
  assert(S.asked == win, "the show asked for the front for " .. tostring(win and win.title) .. ", not "
    .. tostring(S.asked and S.asked.title))
  S.asked = nil
  T.arrive(win)
  local opts = win == T.APP_PAGE and S.appOpts or win == T.NA_PAGE and S.naOpts or S.pageOpts
  ran("page message", opts.onMessage, { kind = "focus", target = "the document" })
end
-- The captured key pressed: its callback run as a handler of the module, as a captured key is.
function T.captured(spec)
  local fn = S.captured[spec]
  assert(fn ~= nil, "no capture " .. spec)
  S.epoch += 1
  ran("key", fn, { shift = true, ctrl = true, alt = false, win = false })
end
-- On a Mac, the tool page shown beside REAPER's floating plug-in window and in front, from F7, its
-- build over and its window made the key window.
function T.shownMac()
  S.front = T.FLOAT
  T.turn()
  S.pg.answers = { { false, "not ready yet" } }
  T.key(T.F7)
  S.now += 300
  T.ready()
  T.ok(S.front == T.FLOAT, "the front asked for")
  T.keyed(T.PAGE)
  T.ok(S.front == T.PAGE, "the page is in front")
  T.report()
end
"##;

/// A fresh VM with the scripted host — its events through the real mailbox — the runtime, the
/// prelude's matcher, the host-panel helpers and the ones above, on Windows; the scenario ends on
/// the harness's checks.
fn run(scenario: &str) {
    run_with_helpers("windows", &[("probe helpers", PR)], scenario)
}

/// `run`, with the host playing macOS.
fn run_mac(scenario: &str) {
    run_with_helpers("macos", &[("probe helpers", PR)], scenario)
}

/// What the probe declares at load: its three keys, the second left to Windows' own delivery
/// (`{ foreground = true }`, W2 b), the setting for W2 c — off by default, labelled with its
/// test — its page, and the window and focus events it follows.
#[test]
fn the_probe_loads_with_its_keys_its_setting_and_its_page() {
    run(r##"
        local S = T.S
        T.probe()
        T.ok(S.hotkeyFns[T.F7] and S.hotkeyFns[T.F8] and S.hotkeyFns[T.F9] and S.hotkeyFns[T.F10], "the four keys")
        T.ok(S.hotkeyOpts[T.F8] and S.hotkeyOpts[T.F8].foreground == true, "F8 is Windows' own WM_HOTKEY")
        T.ok(S.hotkeyOpts[T.F7] == false and S.hotkeyOpts[T.F9] == false and S.hotkeyOpts[T.F10] == false, "the others arrive through the hook")
        local auto = S.defined.autoShow
        T.ok(auto and auto.default == false, "the automatic show is a setting, off by default")
        T.ok(T.has(auto.label, "test W2 c"), auto.label)
        T.ok(S.pageOpts.file == "ui/index.html" and S.pageOpts.title == "HTML probe", "the page")
        T.ok(S.pageOpts.window == nil, "a tool page, by default")
        T.ok(S.appOpts.file == "ui/index.html" and S.appOpts.title == "HTML probe window" and S.appOpts.window == "app",
          "the same page in an application window")
        T.ok(#S.fgCbs == 1 and #S.focusCbs == 1, "one window trigger and one focus callback")
        T.ok(#S.pg.shows == 0 and #S.apg.shows == 0, "nothing is shown at load")
        T.ok(T.count("[html-probe] revision 7 ready — Ctrl+Shift+Win+Alt+F7 shows the page (keyboard hook), "
          .. "Ctrl+Shift+Win+Alt+F8 shows it (WM_HOTKEY), Ctrl+Shift+Win+Alt+F9 says what happened, "
          .. "Ctrl+Shift+Win+Alt+F10 remembers the pointer's point in the plug-in for W10; ") == 1,
          "the revision, in the log at load, with Windows' keys")
        T.ok(T.count("in Alt+Tab and on the taskbar (W6 again)") == 1 and T.count("captured") == 0, "Windows' words")
        T.ok(S.naPage == nil and next(S.captured) == nil, "no page without activation and no captured key on Windows")
        T.ok(T.count("the page's button 'Show as an application window' shows it in a window of its own") == 1,
          "the start line says how to get the application window")
        local said = T.report()
        T.ok(T.has(said, "HTML probe revision 7. The page's button Show as an application window puts it in a window of its own."), said)
        T.ok(not T.has(said, "without activation"), said)
    "##);
}

/// The first press builds the page and says so; the page comes by itself once it is ready,
/// beside the FX window; in front, the report says it was accepted, and when the page had the
/// focus. Pressed in the page, the key goes back: the plug-in window forward FIRST, the page
/// hidden after. Without a plug-in window in front, the key says so and shows nothing.
#[test]
fn the_first_press_builds_the_page_and_the_same_key_in_the_page_goes_back() {
    run(r##"
        local S = T.S
        T.probe()
        S.front = T.OTHER
        T.turn()
        T.key(T.F7)
        T.ok(T.last() == "No plug-in window is in front." and #S.pg.shows == 0, T.last())
        S.front = T.FX
        T.turn()
        S.pg.answers = { { false, "not ready yet" } }
        T.key(T.F7)
        T.ok(#S.pg.shows == 1 and S.pg.shows[1].focus == true, "asked once, with the keyboard")
        local avoid = S.pg.shows[1].avoid
        T.ok(avoid.x == 0 and avoid.y == 0 and avoid.w == 1000 and avoid.h == 700, "beside the FX window")
        T.ok(T.last() == "Building the page.", T.last())
        S.now += 400
        T.ready()
        T.ok(#S.pg.shows == 2 and S.front == T.PAGE, "shown once it is ready")
        S.now += 20
        T.msg({ kind = "focus", target = "the document" })
        local said = T.report()
        T.ok(T.has(said, "F7 (keyboard hook) (after the build): accepted, 400 ms after the key's callback; in front: the page"), said)
        T.ok(T.has(said, "the page has the focus 420 ms after the key's callback"), said)
        local from = #S.order
        T.key(T.F7)
        T.ok(S.order[from + 1] == "focus 1" and S.order[from + 2] == "hide", "the plug-in first, then the page: " .. table.concat(S.order, ","))
        said = T.report()
        T.ok(T.has(said, "F7, pressed in the page: back to the plug-in accepted"), said)
        -- And shown again: a second show of the built page answers at once.
        S.front = T.FX
        T.turn()
        T.key(T.F8)
        said = T.report()
        T.ok(T.has(said, "F8 (WM_HOTKEY): accepted, 0 ms after the key's callback; in front: the page"), said)
    "##);
}

/// Every answer that is not a success is a result with its time and the window in front, and is
/// said at once; the page's own "not in front" after an accepted show is its own result.
#[test]
fn a_refused_show_is_said_with_its_reason_and_what_is_in_front() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.front = T.FX
        T.turn()
        S.pg.answers = { { false, "declined by the system" } }
        T.key(T.F7)
        T.ok(T.last() == "Page not shown: declined by the system", T.last())
        local said = T.report()
        T.ok(T.has(said, "F7 (keyboard hook): not shown, declined by the system, 0 ms after the key's callback; in front: the plug-in window"), said)
        -- Accepted, and yet another window is in front.
        S.front = T.FX
        T.turn()
        S.pg.answers = { { true } }
        local show = S.page.show
        S.page.show = function(self, o)
          local ok, why = show(self, o)
          S.front = T.MAIN
          S.epoch += 1
          return ok, why
        end
        T.key(T.F7)
        said = T.report()
        T.ok(T.has(said, "F7 (keyboard hook): ACCEPTED BUT NOT IN FRONT"), said)
    "##);
}

/// W4: the report counts the keys a person pressed in the page — key downs, not the report
/// chord's own modifiers, nor the masking key the hook presses for a chord it swallowed — names
/// them, and counts anew after each report. Every event is still in the log.
#[test]
fn the_report_counts_the_keys_pressed_in_the_page_not_its_own_chord() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        for _, k in ipairs({ "Enter", " ", "x", "PageDown", "Escape" }) do
          T.msg({ kind = "key", type = "keydown", key = k, code = k, target = "button 'Announce'" })
          T.msg({ kind = "key", type = "keyup", key = k, code = k, target = "button 'Announce'" })
        end
        -- The report chord: its four modifiers reach the page, its F9 does not (RegisterHotKey
        -- takes it, the hook being out of the chain while the page is in front). A masking key
        -- reaches the page only when the hook swallowed a chord in the moment it was going out.
        for _, k in ipairs({ "Control", "Shift", "Meta", "Alt", "Unidentified" }) do
          T.msg({ kind = "key", type = "keydown", key = k, ctrl = true, shift = true, alt = true, meta = true })
        end
        local said = T.report()
        T.ok(T.has(said, "Key presses the page saw: 5: Enter, Space, x, PageDown, Escape."), said)
        T.ok(T.count("W4 key: keydown Ctrl+Alt+Shift+Win+Unidentified") == 1, "the masking key is logged")
        T.ok(T.count("W4 key: keyup Escape") == 1, "key ups are logged")
        said = T.report()
        T.ok(T.has(said, "No results since the last report.") and T.has(said, "Key presses the page saw: 0."), said)
    "##);
}

/// What the page measures of the host's policy at load, and what it reports besides, goes into the
/// log; a close by the user is a result.
#[test]
fn the_pages_own_checks_are_logged_and_a_close_is_a_result() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        T.msg({ kind = "loaded", hasFocus = false, evalRuns = "refused", rtc = "function", clipboardWrite = true })
        T.msg({ kind = "policy", directive = "script-src", blocked = "eval" })
        T.msg({ kind = "check", what = "a fetch of its own page.css", status = "answered 200" })
        T.msg({ kind = "visibility", state = "hidden" })
        T.msg("not a table")
        T.ok(T.count("code built from a string refused; RTCPeerConnection is function; navigator.clipboard.writeText is there") == 1, "the load line")
        T.ok(T.count("page: the policy refused script-src for eval") == 1, "the policy line")
        T.ok(T.count("page check: a fetch of its own page.css — answered 200") == 1, "the check line")
        T.ok(T.count("page: visibility {...}") == 1 and T.count("page sent not a table") == 1, "anything else is logged")
        T.closed("user")
        local said = T.report()
        T.ok(T.has(said, "the page was closed: user"), said)
    "##);
}

/// W5: twenty presses of "Announce" are twenty in the report, each posts the live region and
/// speaks through the host in the same callback; a press whose message arrives twice leaves the
/// page's count behind the probe's, and that is a result.
#[test]
fn twenty_presses_are_twenty_and_a_press_that_arrives_twice_is_said() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        for i = 1, 20 do
          T.msg({ kind = "action", action = "announce", count = i, detail = 0 })
        end
        T.ok(#S.pg.posts == 20 and S.pg.posts[20].kind == "live", "the live region, twenty times")
        T.ok(T.last() == "Host speech 20.", T.last())
        local said = T.report()
        T.ok(T.has(said, "announce 20") and not T.has(said, "W5:"), said)
        T.msg({ kind = "action", action = "announce", count = 21, detail = 0 })
        T.msg({ kind = "action", action = "announce", count = 21, detail = 0 })
        said = T.report()
        T.ok(T.has(said, "W5: the page counted 21 presses of 'announce', the probe 22 messages"), said)
        T.ok(T.has(said, "announce 2"), said)
    "##);
}

/// W3, on events only: the plug-in window is asked forward and nothing is clicked until it is
/// there; the click goes onto the window's own background — clear of REAPER's FX list and of the
/// plug-in's wrapper, by the most room there is, never its title bar — and the page is asked back
/// only in the answer of the picture taken after the click. A second press while the round trip is
/// under way starts nothing: it is a W5 result, not a second click (two clicks on a title bar
/// maximised the FX window on 2026-09-28). The report says the page came back, that the plug-in
/// window kept its place and size, and, when a blur follows with another window in front, that it
/// lost the front again. A window drawn over the spot: no click, and the page comes back all the
/// same.
#[test]
fn w3_clicks_the_plugin_windows_own_background_once_it_is_in_front_and_never_twice() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.controls = { T.LIST, T.WRAP }
        local shows, focuses = #S.pg.shows, #S.focusCalls
        T.press("click-plugin")
        T.ok(#S.focusCalls == focuses + 1 and S.focusCalls[#S.focusCalls] == 1, "the plug-in window asked forward")
        T.ok(#S.clicks == 0, "no click before it is in front")
        T.press("click-plugin", false, 0)
        T.ok(#S.focusCalls == focuses + 1, "a second press while it is not in front yet starts nothing")
        T.msg({ kind = "blur" })
        S.now += 40
        T.arrive(T.FX)
        T.ok(#S.clicks == 1, "one click")
        local x, y = S.clicks[1][1], S.clicks[1][2]
        T.ok(x == 956 and y == 78, "on the background right of the plug-in's wrapper: " .. x .. "," .. y)
        T.ok(y >= T.FX.client.y + 8, "never the title bar")
        for _, c in ipairs(S.controls) do
          local b = c.bounds
          T.ok(x < b.x - 8 or x > b.x + b.w + 8 or y < b.y - 8 or y > b.y + b.h + 8, "clear of " .. c.class)
        end
        T.ok(T.count("clicked at 956,78 (its own background, 35 px from its nearest control or edge)") == 1, "logged")
        T.ok(S.ownsAsked[#S.ownsAsked][1] == 1, "asked whether anything covers it first")
        T.ok(#S.asyncs == 1 and #S.pg.shows == shows, "the page waits for the picture")
        T.press("click-plugin", false, 0)
        T.arrive(T.FX)
        T.ok(#S.focusCalls == focuses + 1 and #S.clicks == 1, "a press between the click and its picture starts nothing either")
        S.now += 30
        T.runDue()
        T.ok(#S.pg.shows == shows + 1 and S.front == T.PAGE, "the page asked back in the picture's answer")
        -- The page's own window takes the keyboard for a moment: a blur with the page in front.
        T.msg({ kind = "blur" })
        S.now += 50
        S.front = T.MAIN
        S.epoch += 1
        T.msg({ kind = "blur" })
        local said = T.report()
        T.ok(T.has(said, "W3: the page came back, accepted, 70 ms after the press; in front: the page; the plug-in window kept its place and size. It has kept the front so far."), said)
        T.ok(T.count("W3: the page lost the front again") == 1, "once, for the blur with REAPER in front")
        T.ok(T.has(said, "W3: the page lost the front again 50 ms after it came back; in front: 'REAPER v7'"), said)
        T.ok(T.count("W5: 'click-plugin' again while the round trip before it is under way — ignored") == 2, "both said")
        T.ok(T.count("W5 hint: 'click-plugin' activated again") == 2, "and hinted at in the log")
        T.ok(not T.has(said, "arriving twice"), "a hint is no result: " .. said)
        -- Covered: no click, said, and the page back all the same.
        S.front = T.PAGE
        T.turn()
        S.owns = false
        local shown = #S.pg.shows
        T.press("click-plugin")
        T.arrive(T.FX)
        T.ok(#S.clicks == 1, "nothing clicked through a window drawn over it")
        T.ok(T.last() == "Something covers the plug-in window; no click.", T.last())
        T.runDue()
        T.ok(#S.pg.shows == shown + 1 and S.front == T.PAGE, "the page back, not left behind")
        said = T.report()
        T.ok(T.has(said, "W3: the page came back (no click), accepted"), said)
        T.ok(T.count("MW4") == 0, "no level asked after on Windows")
    "##);
}

/// The log's case: NVDA's activation of "Click into the plug-in" (click detail 1), the round trip,
/// and then the Enter itself activating the button again once the page was back (detail 0), with
/// nothing released in the page in between. That is a hint in the log, not a result — an Enter
/// whose key-up went to the plug-in window looks the same — and the press is a round trip of its
/// own, which clicks elsewhere than the one before, so the two clicks are never a double click.
#[test]
fn a_second_activation_after_the_page_is_back_is_a_round_trip_that_clicks_elsewhere() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.controls = { T.LIST, T.WRAP }
        T.press("click-plugin", true, 1)
        T.arrive(T.FX)
        T.runDue()
        T.ok(S.front == T.PAGE and #S.clicks == 1, "one round trip, the page back")
        local focuses = #S.focusCalls
        T.press("click-plugin", false, 0)
        T.ok(#S.focusCalls == focuses + 1, "asked forward again")
        T.ok(T.count("W5 hint: 'click-plugin' activated again with no key-up or pointer release in the page since its last activation (click detail 0)") == 1, "the hint")
        T.arrive(T.FX)
        T.ok(#S.clicks == 2, "clicked again")
        local a, b = S.clicks[1], S.clicks[2]
        T.ok(math.abs(a[1] - b[1]) >= 16 or math.abs(a[2] - b[2]) >= 16, "and elsewhere: " .. a[1] .. "," .. a[2] .. " then " .. b[1] .. "," .. b[2])
        T.ok(b[1] == 956 and b[2] == 98, "the next best spot: " .. b[1] .. "," .. b[2])
        T.runDue()
        -- A third comes back to the first spot: only the one just before is kept away from.
        T.press("click-plugin", true, 1)
        T.arrive(T.FX)
        T.ok(S.clicks[3][1] == 956 and S.clicks[3][2] == 78, "the best spot again")
        local said = T.report()
        T.ok(not T.has(said, "W5"), said)
    "##);
}

/// No spot of the plug-in window's own background clear of its controls — its whole client area
/// taken by one control: nothing is clicked, and the round trip is the change of windows alone.
#[test]
fn w3_without_a_clear_background_clicks_nothing_and_still_brings_the_page_back() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.controls = { { id = 9, class = "reaperPluginHostWrapProc", bounds = T.FX.client, client = T.FX.client } }
        T.press("click-plugin")
        T.arrive(T.FX)
        T.ok(#S.clicks == 0, "nothing clicked")
        T.ok(T.count("it has no spot of its own background clear of its controls, so nothing is clicked") == 1, "said in the log")
        T.runDue()
        T.ok(S.front == T.PAGE, "the page back all the same")
        local said = T.report()
        T.ok(T.has(said, "W3: the page came back (no click), accepted"), said)
    "##);
}

/// W5 for every button: the same button activated again with nothing released in the page since
/// is a hint in the log, with what was released before each press; the report counts the presses
/// and says nothing of it.
#[test]
fn every_button_hints_at_a_press_that_may_have_arrived_twice() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        T.press("announce", true, 1)
        T.press("announce", false, 0)
        T.press("announce", true, 1)
        T.press("back", false, 1)
        local said = T.report()
        T.ok(T.count("W5 hint: 'announce' activated again") == 1, "once")
        T.ok(T.count("W5 hint: 'back'") == 0, "another button after it is no repeat")
        T.ok(T.has(said, "announce 3. Key presses"), said)
        T.ok(not T.has(said, "twice"), said)
        T.ok(T.count("released in the page since the last button: mouseup") == 2, "what was released is logged")
    "##);
}

/// A plug-in window that leaves no room beside it: the page goes over part of it, and the show
/// answers true with a note. Said once for a run of such shows; a show with room beside it ends
/// the run.
#[test]
fn a_page_over_the_plugin_window_is_said_once_for_a_run_of_such_shows() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        local over = "over the rectangle to avoid"
        S.front = T.FX
        T.turn()
        S.pg.answers = { { true, over } }
        T.key(T.F8)
        T.ok(T.last() == "The page covers part of the plug-in window: there is no room beside it.", T.last())
        T.ok(S.speech[#S.speech].interrupt == false, "not over NVDA saying the page")
        local said = T.report()
        T.ok(T.has(said, "F8 (WM_HOTKEY): accepted, 0 ms after the key's callback; in front: the page (over part of the plug-in window: no room beside it)"), said)
        local spoken = #S.speech
        S.front = T.FX
        T.turn()
        S.pg.answers = { { true, over } }
        T.key(T.F8)
        T.ok(#S.speech == spoken, "not said again")
        S.front = T.FX
        T.turn()
        S.pg.answers = { { true } }
        T.key(T.F8)
        S.front = T.FX
        T.turn()
        S.pg.answers = { { true, over } }
        T.key(T.F8)
        T.ok(T.last() == "The page covers part of the plug-in window: there is no room beside it.", "said again after a show with room")
    "##);
}

/// The tool page is built, hidden, when a plug-in window first comes to the front — and not again
/// once it is ready; after a failure, the next plug-in window prepares it again. Other windows do
/// not. The application window is not built with it: only W6 again needs it, and its build would
/// hold the event loop in every run (W8).
#[test]
fn the_page_is_prepared_when_a_plugin_window_first_comes_forward() {
    run(r##"
        local S = T.S
        T.probe()
        T.arrive(T.OTHER)
        T.ok(S.pg.prepares == 0, "not for another application's window")
        T.arrive(T.FX)
        T.ok(S.pg.prepares == 1, "for the first plug-in window")
        T.arrive(T.MAIN)
        T.arrive(T.FX)
        T.ok(S.pg.prepares == 2, "asked again while it is being built — a build under way answers not ready")
        T.ok(#S.pg.shows == 0, "prepared, not shown")
        T.ready()
        T.ok(#S.pg.shows == 0, "ready, and still not shown: nothing was asked")
        T.ok(T.last() == "HTML probe ready.", "said, so F7 need not be guessed at: " .. T.last())
        T.ok(S.speech[#S.speech].interrupt == false, "without cutting NVDA off")
        T.arrive(T.MAIN)
        T.arrive(T.FX)
        T.ok(S.pg.prepares == 2, "not once it is ready")
        T.closed("failed")
        S.pg.ready = false
        T.arrive(T.MAIN)
        T.arrive(T.FX)
        T.ok(S.pg.prepares == 3, "again after the page failed")
        T.ok(S.apg.prepares == 0 and #S.apg.shows == 0, "the application window waits for the first switch to it")
    "##);
}

/// W7: after "back to the plug-in", where the keyboard landed — read once the window is in front,
/// and once more on the first focus event after, if it moved — and nothing after that.
#[test]
fn w7_says_where_the_keyboard_landed_on_the_events_that_follow() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.now += 10
        T.msg({ kind = "action", action = "back", count = 1, detail = 0 })
        S.now += 25
        T.arrive(T.FX, { T.FX })
        S.now += 5
        T.focusMoves({ T.LIST, T.FX })
        T.focusMoves({ T.WRAP, T.FX })
        local said = T.report()
        T.ok(T.has(said, "the page's button: back to the plug-in accepted"), said)
        T.ok(T.has(said, "W7: the page's button: the keyboard landed on the plug-in window itself (#32770), 25 ms after"), said)
        T.ok(T.has(said, "W7: then the keyboard moved on to SysListView32, 30 ms after"), said)
        T.ok(not T.has(said, "reaperPluginHostWrapProc"), "the watch ended with the first focus event")
    "##);
}

/// W7, declined: when Windows will not bring the plug-in window forward, the page stays, in front,
/// rather than leave the keyboard to whichever window Windows picks — said, and a result.
#[test]
fn going_back_declined_leaves_the_page_in_front() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.focusAnswer = false
        local hides = S.pg.hides
        T.key(T.F7)
        T.ok(S.pg.hides == hides and S.pg.shown, "the page stays")
        T.ok(T.last() == "Could not switch to the plug-in.", T.last())
        local said = T.report()
        T.ok(T.has(said, "F7, pressed in the page: back to the plug-in DECLINED; the page stays; in front: the page"), said)
    "##);
}

/// W2 c: with the setting on, the page comes for a plug-in window that arrives from somewhere else
/// — another application, or REAPER's main window — and never for a return to it from the page, or
/// the same window again. Off, never.
#[test]
fn the_automatic_show_comes_for_an_arrival_never_for_a_return() {
    run(r##"
        local S = T.S
        T.probe()
        T.arrive(T.OTHER)
        T.arrive(T.FX)
        T.ok(#S.pg.shows == 0, "off by default")
        T.set("autoShow", true)
        T.arrive(T.OTHER)
        T.arrive(T.FX)
        T.ok(#S.pg.shows == 1, "an arrival from another application")
        T.arrive(T.PAGE)
        T.arrive(T.FX)
        T.ok(#S.pg.shows == 1, "not for a return from the page")
        T.arrive(T.FX)
        T.ok(#S.pg.shows == 1, "not for the same window again")
        T.arrive(T.MAIN)
        T.arrive(T.FX)
        T.ok(#S.pg.shows == 2, "an arrival from REAPER's main window")
        local said = T.report()
        T.ok(T.has(said, "automatic show: accepted, 0 ms after the plug-in window came forward; in front: the page"), said)
        T.set("autoShow", false)
        T.arrive(T.OTHER)
        T.arrive(T.FX)
        T.ok(#S.pg.shows == 2, "off again")
        T.ok(T.count("automatic show is now off") == 1, "the change is logged")
    "##);
}

/// W6 again (revision 4): the page's "Show as an application window" shows the same page in its
/// application window beside the plug-in window, with the keyboard — built first if it is not yet,
/// the tool page staying until then — and only then hides the tool page, so the front goes from
/// page to page and never through the plug-in window; the keys then work on it. Its way back to
/// the plug-in leaves it open, F7 from the plug-in window brings it back beside it, and its own
/// switch button goes back to the tool page the same way. Each page, once loaded, is told which
/// window it is in, for its switch button's label; each counts its own presses, so presses of one
/// button in both pages are no W5 result.
#[test]
fn the_switch_shows_the_page_in_an_application_window_and_back() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        T.msg({ kind = "loaded" })
        local told = S.pg.posts[#S.pg.posts]
        T.ok(told.kind == "window" and told.window == "tool" and told.w10 == true,
          "the tool page is told it is one, and that W10 is offered")
        -- The switch, the application window not built yet: asked beside the FX window, with the
        -- keyboard; the tool page stays in front while it is built, and goes once it is shown.
        S.apg.answers = { { false, "not ready yet" } }
        local from = #S.order
        S.now += 10
        T.press("switch")
        T.ok(S.order[from + 1] == "show app" and S.order[from + 2] == nil, "the tool page stays: " .. table.concat(S.order, ","))
        T.ok(S.pg.shown and S.front == T.PAGE, "in front while the other is built")
        T.ok(T.last() == "Building the page.", T.last())
        local asked = S.apg.shows[1]
        T.ok(asked.focus == true and asked.avoid.x == 0 and asked.avoid.w == 1000, "beside the FX window, with the keyboard")
        S.now += 200
        T.appReady()
        T.ok(#S.apg.shows == 2 and S.front == T.APP_PAGE, "shown once it is ready")
        T.ok(S.order[from + 2] == "show app" and S.order[from + 3] == "hide", "then the tool page hidden: " .. table.concat(S.order, ","))
        T.ok(not S.pg.shown, "the tool page is gone")
        T.appMsg({ kind = "loaded" })
        told = S.apg.posts[#S.apg.posts]
        T.ok(told.kind == "window" and told.window == "app" and told.w10 == true,
          "the application window is told it is one, and that W10 is offered")
        local said = T.report()
        T.ok(T.has(said, "switch to the application window (after the build): accepted, 200 ms after the page's button; in front: the page"), said)
        T.ok(T.has(said, "window switch 1"), said)
        -- Its way back: the plug-in window forward, the application window left open.
        local hides = S.apg.hides
        T.appPress("back")
        T.ok(S.apg.hides == hides and S.apg.shown, "left open")
        T.ok(S.focusCalls[#S.focusCalls] == T.FX.id, "the FX window asked forward")
        said = T.report()
        T.ok(T.has(said, "the page's button: back to the plug-in accepted; the application window stays open"), said)
        -- F7 from the FX window: the application window beside it again; pressed in it, back.
        S.front = T.FX
        T.turn()
        T.key(T.F7)
        T.ok(#S.apg.shows == 3 and S.front == T.APP_PAGE and #S.pg.shows == 2, "F7 shows the page in use")
        T.key(T.F7)
        T.ok(S.apg.hides == hides, "still open")
        said = T.report()
        T.ok(T.has(said, "F7 (keyboard hook): accepted, 0 ms after the key's callback; in front: the page"), said)
        T.ok(T.has(said, "F7, pressed in the page: back to the plug-in accepted; the application window stays open"), said)
        -- The same button pressed in both pages is no press arriving twice.
        T.appPress("announce")
        T.press("announce")
        said = T.report()
        T.ok(not T.has(said, "W5"), said)
        T.ok(S.apg.posts[#S.apg.posts].kind == "live", "the live region of the page pressed in")
        -- Its own switch goes back to the tool page, the same way round.
        from = #S.order
        T.appPress("switch")
        T.ok(S.order[from + 1] == "show" and S.order[from + 2] == "hide app", table.concat(S.order, ","))
        T.ok(S.front == T.PAGE, "the tool page in front")
        T.appClosed("user")
        said = T.report()
        T.ok(T.has(said, "switch to the tool page: accepted"), said)
        T.ok(T.has(said, "the application window was closed: user"), said)
    "##);
}

/// The automatic show (W2 c) never brings the application window: it is reached as any program's
/// window is, and coming by itself it would take the keyboard from the plug-in at every return from
/// another desktop or program, and leave where the user put it. A return from it is no arrival
/// either. Switched back to the tool page, an arrival shows that again.
#[test]
fn the_automatic_show_never_brings_the_application_window() {
    run(r##"
        local S = T.S
        T.probe()
        T.set("autoShow", true)
        T.shown()
        S.apg.ready = true
        T.press("switch")
        T.ok(S.front == T.APP_PAGE, "the application window in front")
        local shows, toolShows = #S.apg.shows, #S.pg.shows
        T.arrive(T.APP_PAGE)
        T.arrive(T.FX)
        T.arrive(T.OTHER)
        T.arrive(T.FX)
        T.ok(#S.apg.shows == shows and #S.pg.shows == toolShows, "neither page, for a return or an arrival")
        T.ok(T.count("automatic show: none — the application window is the page in use") == 1, "the arrival is logged")
        T.key(T.F7)
        T.ok(#S.apg.shows == shows + 1 and S.front == T.APP_PAGE, "F7 brings it")
        T.appPress("switch")
        T.ok(S.front == T.PAGE and not S.apg.shown, "back to the tool page")
        T.arrive(T.OTHER)
        T.arrive(T.FX)
        T.ok(#S.pg.shows == toolShows + 2 and #S.apg.shows == shows + 1, "an arrival shows the tool page again")
    "##);
}

/// A switch the system declines leaves the page in use where it was, in front, and takes the
/// other away again — unless that one was open before; a way back to the plug-in while the other
/// is still being built gives the switch up, so it does not come once it is ready.
#[test]
fn a_switch_not_shown_keeps_the_page_in_use_and_a_way_back_gives_one_up() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.apg.ready = true
        S.apg.answers = { { false, "declined by the system" } }
        local hides = S.pg.hides
        T.press("switch")
        T.ok(S.pg.shown and S.pg.hides == hides and S.front == T.PAGE, "the tool page stays, in front")
        T.ok(not S.apg.shown and S.apg.hides == 1, "the other goes again")
        T.ok(T.last() == "Page not shown: declined by the system", T.last())
        local said = T.report()
        T.ok(T.has(said, "switch to the application window: not shown, declined by the system, 0 ms after the page's button; in front: the page"), said)
        -- F7 still works on the tool page.
        T.key(T.F7)
        T.ok(not S.pg.shown and S.focusCalls[#S.focusCalls] == T.FX.id, "back to the plug-in from the tool page")
        -- Not built yet, and the way back pressed before it is: it never comes.
        S.front = T.FX
        T.turn()
        T.key(T.F7)
        S.apg.ready = false
        S.apg.answers = { { false, "not ready yet" } }
        T.press("switch")
        T.press("back")
        local shows = #S.apg.shows
        T.appReady()
        T.ok(#S.apg.shows == shows and not S.apg.shown, "given up")
        said = T.report()
        T.ok(T.has(said, "the application window is ready: prepared, not shown"), said)
    "##);
}

/// W10, the target: F10 remembers the pointer's point in the plug-in window as an offset into its
/// client area and says it, and reads it afresh each press. Without a plug-in window it says so.
#[test]
fn f10_remembers_the_pointer_point_in_the_plugin_window() {
    run(r##"
        local S = T.S
        T.probe()
        -- No plug-in window in front: nothing remembered.
        S.front = T.OTHER
        T.turn()
        T.key(T.F10)
        T.ok(T.last() == "No plug-in window is in front." and T.count("no plug-in window is in front or remembered") == 1, T.last())
        -- On the FX window, the pointer at 120,90 — 112,60 into its client area (8,30).
        S.front = T.FX
        S.cursor = { x = 120, y = 90 }
        T.turn()
        T.key(T.F10)
        T.ok(T.last() == "Target remembered: 112, 60 in the plug-in window.", T.last())
        T.ok(T.count("target remembered at 120,90 on screen (112,60 in the plug-in window") == 1, T.dump())
    "##);
}

/// W10, the elements: "Read the plug-in's elements" dumps the plug-in window with a callback,
/// keeps the named ones with a rectangle inside its client area (at most 60), and posts them to
/// the page that asked once the dump has answered — the remembered point first — each by name and
/// type, those that share both marked, since a press acts on the first of them; how many share is
/// in the log, and so is a dump that is not all of the tree. Without a plug-in window it says so.
#[test]
fn reading_the_elements_posts_the_named_ones_inside_the_client_area() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.cursor = { x = 120, y = 90 }
        T.key(T.F10)
        -- A dump with: a named button inside the client, one with no name, one off the client area,
        -- a collapsed one, two buttons of the same name, and one of that name of another type. The
        -- unnamed, the outside and the collapsed are left out.
        S.dump = {
          { name = "Bypass", ctype = 50000, bounds = { x = 100, y = 100, w = 40, h = 20 } },
          { name = "", ctype = 50000, bounds = { x = 200, y = 100, w = 40, h = 20 } },
          { name = "Off screen", ctype = 50000, bounds = { x = 2000, y = 100, w = 40, h = 20 } },
          { name = "Collapsed", ctype = 50000, bounds = { x = 100, y = 200, w = 0, h = 0 } },
          { name = "Preset", ctype = 50000, bounds = { x = 300, y = 100, w = 40, h = 20 } },
          { name = "Preset", ctype = 50000, bounds = { x = 400, y = 100, w = 40, h = 20 } },
          { name = "Preset", ctype = 50020, bounds = { x = 500, y = 100, w = 40, h = 20 } },
        }
        local posts = S.pg.posts
        local before = #posts
        T.msg({ kind = "w10", ["do"] = "read" })
        T.ok(S.dumpedId == T.FX.id, "the remembered plug-in window was dumped")
        T.ok(#posts == before and T.count("named element(s)") == 0, "nothing posted before the dump has answered")
        T.dumped("the program did not answer within 1000 ms")
        local last = posts[#posts]
        T.ok(T.count("W10: the plug-in's elements are not all of its tree: the program did not answer within 1000 ms") == 1,
          T.dump())
        T.ok(last.kind == "w10elements" and #last.elements == 4, "the named elements inside the client area")
        T.ok(last.elements[1].value == "el:1" and last.elements[1].label == "Bypass (type 50000)", last.elements[1].label)
        T.ok(last.elements[2].label == "Preset (type 50000), one of 2 alike: a press acts on the first of them", last.elements[2].label)
        T.ok(last.elements[3].label == last.elements[2].label and last.elements[3].value == "el:3", "both marked")
        T.ok(last.elements[4].label == "Preset (type 50020)", "another type is another element")
        T.ok(last.target == "112, 60", "the remembered point goes with it")
        T.ok(T.count("W10: 4 named element(s) of the plug-in inside its client area read; 2 of them share their name and type with another") == 1, T.dump())
        -- The plug-in window gone: said, and nothing posted.
        local n = #posts
        S.listed = { T.MAIN, T.OTHER }
        T.msg({ kind = "w10", ["do"] = "read" })
        T.ok(T.last() == "No plug-in window remembered. Press F10 with the pointer on the control first.", T.last())
        T.ok(#posts == n, "nothing posted")
    "##);
}

/// W10, "Try it": a run with no plug-in window remembered says so and starts nothing; a bad or
/// absent repeat count is one. (The acting ways themselves are measured live, with NVDA.)
#[test]
fn trying_without_a_plugin_window_says_so() {
    run(r##"
        local S = T.S
        T.probe()
        -- Nothing remembered, nothing in front: Try it says so.
        S.front = T.OTHER
        T.turn()
        T.msg({ kind = "w10", ["do"] = "try", way = "post", target = "point", ["repeat"] = "20" })
        T.ok(T.last() == "No plug-in window remembered. Press F10 with the pointer on the control first.", T.last())
        T.ok(T.count("W10: try") == 0, "nothing was tried")
        -- An unknown W10 request is logged, not run.
        T.msg({ kind = "w10", ["do"] = "wat" })
        T.ok(T.count("an unknown W10 request 'wat'") == 1, T.dump())
    "##);
}

/// W10, one posted click, each step on the answer of the one before: the pointer on the target
/// first — a control lit by it is lit in the baseline too — the read before, the baseline, the
/// click posted to the plug-in window at the target, its answer on a later tick, the change wait
/// against the baseline, the read after from the changed picture, and the result: a result line,
/// said without cutting NVDA off, and in the report.
#[test]
fn a_posted_attempt_goes_step_by_step_and_says_what_happened() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        S.cursor = { x = 120, y = 90 }
        T.key(T.F10)
        T.msg({ kind = "w10", ["do"] = "try", way = "post", target = "point", ["repeat"] = "1" })
        T.ok(#S.moves == 1 and S.moves[1][1] == 120 and S.moves[1][2] == 90, "the pointer on the target before anything is read")
        T.ok(#S.reads == 1 and S.reads[1].key == "w10" and T.region(S.reads[1].region) == "56,30,184,154", T.dump())
        T.ok(#S.asyncs == 0 and #S.acts == 0, "nothing pictured or clicked before the read is answered")
        T.readNext("Basic Saw")
        T.ok(#S.asyncs == 1 and S.asyncs[1].change == nil, "the baseline")
        local base = T.picture()
        T.ok(#S.acts == 1, "one click")
        local a = S.acts[1]
        T.ok(a.x == 120 and a.y == 90 and a.via == "post" and a.window == T.FX.id and a.move == nil,
          "posted to the plug-in window at the target")
        T.ok(#S.asyncs == 0, "no change wait before the click is answered")
        T.acted(true)
        local w = S.asyncs[1]
        T.ok(#S.asyncs == 1 and w.change and w.change.from == base and w.change.settle == 150 and w.change.timeout == 1500,
          "the change wait, against the baseline")
        T.picture({ waited = 210, frames = 6, changed = true, settled = true })
        T.ok(base.released, "the baseline let go of")
        T.ok(#S.reads == 2 and S.reads[2].key == "w10after" and S.reads[2].snapshot ~= nil, "the read after, from the changed picture")
        T.readNext("Bright Lead")
        local want = 'W10 post on the remembered point (1/1): made; front: the page throughout; picture: changed, settled 210 ms '
          .. 'after the wait began (the last 150 ms of them still); text: "Basic Saw" to "Bright Lead"; state: —'
        T.ok(T.count("RESULT " .. want) == 1, T.dump())
        T.ok(T.last() == want and S.speech[#S.speech].interrupt == false, "said without cutting NVDA off: " .. T.last())
        T.ok(T.has(T.report(), want), "and in the report")
    "##);
}

/// W10, an element's state: asked with a callback — the event loop never waits for the plug-in —
/// of the plug-in window with its title as the container, first, before the read; and again once
/// the change wait has answered, before the read after; each step only on the answer of the one
/// before, and the two states in the result.
#[test]
fn an_elements_state_is_asked_with_a_callback_before_and_after() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        T.key(T.F10)
        S.dump = { { name = "Bypass", ctype = 50002, bounds = { x = 100, y = 100, w = 40, h = 20 } } }
        T.msg({ kind = "w10", ["do"] = "read" })
        T.dumped()
        local probes = {}
        rawset(T.host.element, "stateProbe", function(id, container, name, ctype, cb)
          assert(type(cb) == "function", "the state is asked with a callback")
          probes[#probes + 1] = { id = id, container = container, name = name, ctype = ctype, cb = cb }
        end)
        local function answer(i, st)
          local got = T.call("probed", probes[i].cb, st)
          T.ok(got == "Ran", "the state's answer: " .. got)
        end
        T.msg({ kind = "w10", ["do"] = "try", way = "watch", target = "el:1", ["repeat"] = "1" })
        local p = probes[1]
        T.ok(#probes == 1 and p.id == T.FX.id and p.container == T.FX.title and p.name == "Bypass" and p.ctype == 50002,
          "the state asked first, of the plug-in window by its title")
        T.ok(#S.reads == 0, "nothing read before the state has answered")
        answer(1, { toggle = 0 })
        T.readNext("Basic Saw")
        T.picture()
        T.picture({ waited = 200, frames = 5, changed = true, settled = true })
        T.ok(#probes == 2 and #S.reads == 1, "the state after asked once the wait answered, before the read after")
        answer(2, { toggle = 1 })
        T.ok(#S.reads == 2 and S.reads[2].key == "w10after", "the read after, once the state has answered")
        T.readNext("Bright Lead")
        T.ok(T.count('RESULT W10 watch on "Bypass" (1/1): made; front: the page throughout; picture: changed, settled 200 ms '
          .. 'after the wait began (the last 150 ms of them still); text: "Basic Saw" to "Bright Lead"; state: off to on') == 1,
          T.dump())
    "##);
}

/// W10, a series: ten posted clicks, each attempt only once the one before has been answered, the
/// pointer on the target before each; the attempts in the log, and one summary at the end — a
/// result, said once. The run is over then: the next "Try it" is not ignored.
#[test]
fn a_series_of_ten_ends_in_one_summary() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        T.key(T.F10)
        T.msg({ kind = "w10", ["do"] = "try", way = "post", target = "point", ["repeat"] = "10" })
        for i = 1, 10 do
          T.ok(#S.acts == i - 1 and #S.moves == i, "attempt " .. i .. " waits for the one before")
          T.readNext("Preset " .. i)
          T.picture()
          T.acted(true)
          T.picture({ waited = 200, frames = 5, changed = i ~= 7, settled = i ~= 7 })
          T.readNext(i == 7 and ("Preset " .. i) or ("Preset " .. (i + 1)))
        end
        T.ok(#S.acts == 10 and #S.moves == 10, "ten clicks")
        T.ok(T.count("[html-probe] W10 post on the remembered point (10/10): made") == 1
          and T.count("RESULT W10 post on the remembered point (") == 0, "each attempt in the log, none a result of its own")
        T.ok(T.count("(7/10): made; front: the page throughout; picture: no change; text: unchanged") == 1, T.dump())
        local summary = "W10 post on the remembered point x10: made 10, picture changed 9, text changed 9, front kept 10"
        T.ok(T.count("RESULT " .. summary) == 1, T.dump())
        T.ok(T.last() == summary, T.last())
        T.msg({ kind = "w10", ["do"] = "try", way = "post", target = "point", ["repeat"] = "1" })
        T.ok(T.count("W10: try 'post' on target 'point', 1 time(s)") == 1 and T.count("while a run is under way") == 0, T.dump())
    "##);
}

/// W10, the round trip as W3's: the plug-in window asked to the front and nothing clicked until it
/// is there; the click at the target on its arrival, then the change wait, and the page back only
/// once that wait has answered — so the click never lands after the page is back. The report says
/// a run that waits for the plug-in window; F7 gives it up, and the window's arrival then clicks
/// nothing.
#[test]
fn the_round_trip_clicks_on_the_plugins_arrival_and_brings_the_page_back_after_the_wait() {
    run(r##"
        local S = T.S
        T.probe()
        T.shown()
        T.key(T.F10)
        local shows = #S.pg.shows
        T.msg({ kind = "w10", ["do"] = "try", way = "round", target = "point", ["repeat"] = "1" })
        T.ok(#S.moves == 1, "the pointer on the target first")
        T.readNext("Basic Saw")
        T.picture()
        T.ok(S.focusCalls[#S.focusCalls] == T.FX.id, "the plug-in window asked to the front")
        T.ok(#S.clicks == 0 and #S.asyncs == 0 and #S.pg.shows == shows, "nothing clicked or shown before it is there")
        T.ok(T.has(T.report(), "A W10 run is under way, 0 of 1 done, waiting for the plug-in window to come to the front"), T.last())
        T.arrive(T.FX)
        T.ok(#S.clicks == 1 and S.clicks[1][1] == 120 and S.clicks[1][2] == 90, "clicked at the target once it is in front")
        T.ok(#S.asyncs == 1 and S.asyncs[1].change ~= nil and #S.pg.shows == shows, "the change wait, the page still away")
        T.picture({ waited = 180, frames = 4, changed = true, settled = true })
        T.ok(#S.pg.shows == shows + 1 and S.pg.shows[#S.pg.shows].focus == true, "the page back once the wait answered")
        T.readNext("Bright Lead")
        T.ok(T.count("RESULT W10 round on the remembered point (1/1): made; front: changed — now the page;") == 1, T.dump())
        -- Another, given up by F7 while it waits for the plug-in window.
        T.msg({ kind = "w10", ["do"] = "try", way = "round", target = "point", ["repeat"] = "1" })
        T.readNext("Bright Lead")
        local base = T.picture()
        T.key(T.F7)
        T.ok(T.count("W10: the run of 'round' given up by F7 after 0 of 1 attempt(s)") == 1, T.dump())
        T.ok(base.released, "its baseline let go of")
        local clicks = #S.clicks
        T.arrive(T.FX)
        T.ok(#S.clicks == clicks, "nothing clicked for a run given up")
    "##);
}

/// On a Mac (revision 6, and 7): the keys are Command-Shift chords — nothing on VoiceOver's
/// Control-Option, and not the window probe's Command-Shift-F9 or F10, which ship in the Mac's
/// download — F8 is registered without `foreground` and shows the page described
/// `activates = false`, and the one captured key, Command-Shift-F12, is a capture, not a hotkey.
/// The tool page and the application window are described as on Windows; the log at load and the
/// first report say the keys in the Mac's words. W10 is Windows' alone: no key of its own, nothing
/// of it at load, each page told as it loads that W10 is not offered — so its part stays hidden —
/// and a W10 request that came all the same logged, not run.
#[test]
fn on_a_mac_the_probe_takes_command_shift_keys_a_captured_key_and_a_page_without_activation() {
    run_mac(r##"
        local S = T.S
        T.mac()
        T.probe()
        T.ok(S.hotkeyFns[T.F7] and S.hotkeyFns[T.F8] and S.hotkeyFns[T.F9], "the three hotkeys, Command-Shift")
        T.ok(S.hotkeyFns["Cmd+Shift+F9"] == nil and S.hotkeyFns["Ctrl+Shift+Win+Alt+F7"] == nil,
          "neither the window probe's key nor VoiceOver's Control-Option")
        T.ok(S.hotkeyOpts[T.F7] == false and S.hotkeyOpts[T.F8] == false and S.hotkeyOpts[T.F9] == false,
          "no foreground option: every hotkey on a Mac is the system's own delivery")
        T.ok(S.captured[T.F12] ~= nil and S.hotkeyFns[T.F12] == nil, "F12 is captured, not registered")
        T.ok(S.hotkeyFns["Cmd+Shift+F10"] == nil and S.hotkeyFns[T.F10] == nil, "no W10 key on a Mac")
        T.ok(S.pageOpts.title == "HTML probe" and S.pageOpts.activates == nil and S.pageOpts.window == nil, "the tool page")
        T.ok(S.appOpts.window == "app" and S.appOpts.activates == nil, "the application window")
        T.ok(S.naOpts.file == "ui/index.html" and S.naOpts.title == "HTML probe without activation"
          and S.naOpts.window == nil and S.naOpts.activates == false, "the page without activation")
        T.ok(#S.npg.shows == 0 and S.npg.prepares == 0, "not built at load")
        T.ok(T.count("[html-probe] revision 7 ready — Cmd+Shift+F7 shows the page (hotkey), Cmd+Shift+F8 shows it "
          .. "without activation (activates = false, MW1b), Cmd+Shift+F11 says what happened; ") == 1, T.dump())
        T.ok(T.count("for W10") == 0 and T.count("F10") == 0, "nothing of W10 at load")
        T.ok(T.count("in Command-Tab and the Dock (MW10, MW11)") == 1, "the application window, in a Mac's words")
        T.ok(T.count("automatic show is off; Cmd+Shift+F12, captured, shows it without the keyboard (MW2) and then "
          .. "updates its live region (MW16)") == 1, T.dump())
        local said = T.report()
        T.ok(T.has(said, "HTML probe revision 7. The page's button Show as an application window puts it in a window "
          .. "of its own. Cmd+Shift+F8 shows it without activation, and Cmd+Shift+F12 shows it without the keyboard."), said)
        -- Each page, loaded, is told that W10 is not offered here.
        T.msg({ kind = "loaded" })
        local told = S.pg.posts[#S.pg.posts]
        T.ok(told.kind == "window" and told.window == "tool" and told.w10 == false, "the tool page keeps W10's part hidden")
        T.naMsg({ kind = "loaded" })
        told = S.npg.posts[#S.npg.posts]
        T.ok(told.kind == "window" and told.window == "tool" and told.w10 == false, "so does the page without activation")
        -- A W10 request all the same: logged, nothing read, tried or said.
        local spoken = #S.speech
        T.msg({ kind = "w10", ["do"] = "read" })
        T.msg({ kind = "w10", ["do"] = "try", way = "post", target = "point", ["repeat"] = "1" })
        T.ok(T.count("a W10 request 'read', and W10 is not offered on this platform") == 1
          and T.count("a W10 request 'try', and W10 is not offered on this platform") == 1, T.dump())
        T.ok(S.dumpedId == nil and #S.acts == 0 and #S.moves == 0 and #S.reads == 0 and #S.speech == spoken,
          "nothing read, tried or said")
    "##);
}

/// MW1b on a Mac: F8 pressed in a page goes back, as F7 does; from the plug-in window it shows the
/// page without activation — built at its first press, shown once ready — and F7 pressed in that
/// one goes back too, while F7 from the plug-in window still shows the tool page, and so does the
/// captured key, without the keyboard, whichever page was used last. A key leaves the other pages
/// as they are: the page without activation takes the keyboard with the tool page shown beside the
/// plug-in window without it, and leaving, it leaves that one shown — MW1b's window of the
/// application's own that AppKit may give the keyboard to. The page without activation's switch
/// button goes to the application window, which stays open when F8 is pressed later.
#[test]
fn on_a_mac_f8_shows_the_page_without_activation_and_leaves_the_others_as_they_are() {
    run_mac(r##"
        local S = T.S
        T.mac()
        T.probe()
        T.shownMac()
        local asked = S.pg.shows[2]
        T.ok(asked.focus == true and asked.avoid.x == T.FLOAT.bounds.x and asked.avoid.w == T.FLOAT.bounds.w,
          "beside REAPER's floating window")
        local from = #S.order
        T.key(T.F8)
        T.ok(S.order[from + 1] == "focus 11" and S.order[from + 2] == "hide", "pressed in the page, back: " .. table.concat(S.order, ","))
        T.ok(#S.npg.shows == 0, "and nothing shown")
        local said = T.report()
        T.ok(T.has(said, "F8, pressed in the page: back to the plug-in accepted"), said)
        -- From the plug-in window: built first, then shown with the keyboard; the tool page is not asked.
        S.front = T.FLOAT
        T.turn()
        S.npg.answers = { { false, "not ready yet" } }
        T.key(T.F8)
        T.ok(#S.npg.shows == 1 and S.npg.shows[1].focus == true and S.npg.shows[1].avoid.x == T.FLOAT.bounds.x, "asked beside it")
        T.ok(T.last() == "Building the page.", T.last())
        S.now += 50
        T.naReady()
        T.ok(#S.npg.shows == 2 and S.front == T.NA_PAGE and #S.pg.shows == 2, "shown once ready")
        said = T.report()
        T.ok(T.has(said, "F8 (page without activation) (after the build): accepted, 50 ms after the key's callback; in front: the page"), said)
        -- F7 pressed in it goes back; F7 from the plug-in window shows the tool page.
        T.key(T.F7)
        T.ok(not S.npg.shown and S.focusCalls[#S.focusCalls] == T.FLOAT.id, "back to the plug-in")
        S.front = T.FLOAT
        T.turn()
        T.key(T.F7)
        T.ok(#S.pg.shows == 3 and #S.npg.shows == 2 and S.front == T.FLOAT, "F7 asks for the front for the tool page")
        S.now += 40
        T.keyed(T.PAGE)
        T.ok(S.front == T.PAGE, "F7 shows the tool page")
        said = T.report()
        T.ok(T.has(said, "F7, pressed in the page: back to the plug-in accepted"), said)
        T.ok(T.has(said, "F7 (hotkey): the front asked for, 0 ms after the key's callback; in front so far: the plug-in window F7"), said)
        T.ok(T.has(said, "F7 (hotkey): the page has the focus 40 ms after the key's callback"), said)
        T.ok(not T.has(said, "ACCEPTED BUT NOT IN FRONT"), said)
        -- A key leaves the other pages as they are: the tool page shown without the keyboard, then
        -- F8 — the page without activation with the keyboard, beside it — and leaving, that one
        -- leaves the tool page shown.
        T.key(T.F7)
        S.front = T.FLOAT
        T.turn()
        T.captured(T.F12)
        T.ok(S.pg.shown and S.pg.shows[#S.pg.shows].focus == false and S.front == T.FLOAT, "the tool page, without the keyboard")
        from = #S.order
        T.key(T.F8)
        T.ok(S.order[from + 1] == "show na" and S.order[from + 2] == nil, table.concat(S.order, ","))
        T.ok(S.front == T.NA_PAGE and S.pg.shown, "beside the tool page shown without the keyboard")
        from = #S.order
        T.key(T.F8)
        T.ok(S.order[from + 1] == "focus 11" and S.order[from + 2] == "hide na" and S.order[from + 3] == nil, table.concat(S.order, ","))
        T.ok(S.pg.shown and not S.npg.shown, "the tool page still shown")
        -- The page without activation used last: the captured key is still the tool page's, and
        -- updates its live region.
        S.front = T.FLOAT
        T.turn()
        local naShows = #S.npg.shows
        T.captured(T.F12)
        T.ok(S.pg.posts[#S.pg.posts].kind == "live" and #S.npg.shows == naShows, "the tool page's live region, not the other page shown")
        -- Its switch button: the application window, then it goes.
        S.front = T.FLOAT
        T.turn()
        T.key(T.F8)
        from = #S.order
        T.naMsg({ kind = "action", action = "switch", count = 1, detail = 1, released = true, releases = { "mouseup" } })
        T.ok(S.order[from + 1] == "show app" and S.order[from + 2] == "hide na", table.concat(S.order, ","))
        T.keyed(T.APP_PAGE)
        said = T.report()
        T.ok(T.has(said, "switch to the application window: the front asked for, 0 ms after the page's button; in front so far: "
          .. "the page without activation"), said)
        -- F8 from the plug-in window with the application window in use: it stays open.
        T.appPress("back")
        S.front = T.FLOAT
        T.turn()
        local appHides = S.apg.hides
        T.key(T.F8)
        T.ok(S.front == T.NA_PAGE and S.apg.hides == appHides and S.apg.shown, "the application window stays open")
    "##);
}

/// W3 on a Mac (MW4): the click goes onto the plug-in window's title bar — half way down it, across
/// its middle — and not into its content, which `controls()` cannot tell apart from its buttons
/// there; a second round trip clicks twice APART further along it, so two are never a double
/// click, and a third comes back to the middle. Logic's plug-in window, whose content begins at its
/// frame, has no title bar to click: nothing is clicked, and the page comes back all the same.
#[test]
fn w3_on_a_mac_clicks_the_plugin_windows_title_bar_away_from_the_last_click() {
    run_mac(r##"
        local S = T.S
        T.mac()
        T.probe()
        T.shownMac()
        S.controls = { { id = 30, class = "AXGroup//", bounds = T.FLOAT.client, client = T.FLOAT.client } }
        T.press("click-plugin")
        T.ok(S.focusCalls[#S.focusCalls] == T.FLOAT.id and #S.clicks == 0, "asked forward, nothing clicked yet")
        T.arrive(T.FLOAT)
        T.ok(#S.clicks == 1 and S.clicks[1][1] == 550 and S.clicks[1][2] == 49,
          "the title bar's middle: " .. S.clicks[1][1] .. "," .. S.clicks[1][2])
        T.ok(S.clicks[1][2] < T.FLOAT.client.y, "above the window's content")
        T.ok(T.count("clicked at 550,49 (its title bar)") == 1, T.dump())
        T.ok(S.ownsAsked[#S.ownsAsked][1] == T.FLOAT.id, "asked whether anything covers it first")
        T.runDue()
        T.ok(S.front == T.FLOAT and S.pg.shows[#S.pg.shows].focus == true, "the page asked back in the picture's answer")
        T.keyed(T.PAGE)
        T.press("click-plugin")
        T.arrive(T.FLOAT)
        T.ok(S.clicks[2][1] == 582 and S.clicks[2][2] == 49, "further along: " .. S.clicks[2][1] .. "," .. S.clicks[2][2])
        T.runDue()
        T.keyed(T.PAGE)
        T.press("click-plugin")
        T.arrive(T.FLOAT)
        T.ok(S.clicks[3][1] == 550, "the middle again: " .. S.clicks[3][1])
        T.runDue()
        T.keyed(T.PAGE)
        local said = T.report()
        T.ok(T.has(said, "W3: the page asked back, 0 ms after the press; in front so far: the plug-in window; the plug-in "
          .. "window kept its place and size."), said)
        T.ok(T.has(said, "W3 back: the page has the focus 0 ms after the page was asked back"), said)
        T.ok(not T.has(said, "came back") and not T.has(said, "MW4:"), said)
        T.ok(T.count("MW4: the tool page's place is not known, so whether its level follows the keyboard is not seen") == 3, T.dump())
        -- Logic's plug-in window: no title bar of its own in the host's geometry, so no click.
        T.key(T.F7)
        S.front = T.LOGIC
        T.turn()
        T.key(T.F7)
        T.keyed(T.PAGE)
        T.ok(S.front == T.PAGE and S.pg.shows[#S.pg.shows].avoid.x == T.LOGIC.bounds.x, "beside Logic's window")
        T.press("click-plugin")
        T.arrive(T.LOGIC)
        T.ok(#S.clicks == 3, "nothing clicked")
        T.ok(T.count("it has no title bar of its own to click, so nothing is clicked") == 1, T.dump())
        T.runDue()
        said = T.report()
        T.ok(T.has(said, "W3: the page asked back (no click), 0 ms after the press; in front so far: the plug-in window"), said)
    "##);
}

/// W3 on a Mac, told by a focus event alone. Only an application's activation is a foreground event
/// there, so the plug-in window focused in the application that is the active one already — REAPER,
/// the page described `activates = false` having had the keyboard only lent — comes with no
/// activation at all, and the plug-in window focused after its application's activation named
/// another of its windows first comes with a focus event after it. W3 goes on when a focus event
/// finds the plug-in window with the keyboard, once, and not before: a focus event with the page
/// still in front, or the activation naming REAPER's main window, leaves it waiting, and the report
/// says so.
#[test]
fn w3_on_a_mac_goes_on_when_a_focus_event_finds_the_plugin_window_with_the_keyboard() {
    run_mac(r##"
        local S = T.S
        T.mac()
        T.probe()
        local MAIN = { id = 14, title = "REAPER v7.27 - Untitled", class = "AXWindow/AXStandardWindow/", app = T.MAC_REAPER,
          bounds = { x = 0, y = 38, w = 1440, h = 862 }, client = { x = 0, y = 60, w = 1440, h = 840 } }
        -- The page without activation, the key window at the call.
        S.front = T.FLOAT
        T.turn()
        T.key(T.F8)
        T.ok(S.front == T.NA_PAGE, "the page without activation, in front at the call")
        T.naMsg({ kind = "focus", target = "the document" })
        T.report()
        T.naPress("click-plugin")
        T.ok(S.focusCalls[#S.focusCalls] == T.FLOAT.id and #S.clicks == 0, "the plug-in window asked forward")
        T.focusMoves({ T.NA_PAGE })
        T.ok(#S.clicks == 0, "nothing while the page has the keyboard")
        local said = T.report()
        T.ok(T.has(said, "W3 is still waiting for the plug-in window to come to the front."), said)
        -- REAPER the active application all along: no activation, the keyboard in the plug-in window.
        S.front = T.FLOAT
        T.focusMoves({ T.FLOAT })
        T.ok(#S.clicks == 1 and S.clicks[1][1] == 550 and S.clicks[1][2] == 49, "the title bar, on the focus event")
        T.ok(T.count("W3: the plug-in window has the keyboard, told by a focus event") == 1, T.dump())
        T.focusMoves({ T.FLOAT })
        T.ok(#S.clicks == 1 and #S.asyncs == 1, "once")
        T.runDue()
        T.ok(S.front == T.NA_PAGE, "the page without activation back, in front at the call")
        said = T.report()
        T.ok(T.has(said, "W3: the page came back, accepted") and T.has(said, "in front: the page;"), said)
        -- The tool page, the activating panel: REAPER's activation names its main window first, and
        -- the plug-in window has the keyboard with the focus event after it.
        T.key(T.F7)
        S.front = T.FLOAT
        T.turn()
        T.key(T.F7)
        T.keyed(T.PAGE)
        T.report()
        T.press("click-plugin")
        T.arrive(MAIN)
        T.ok(#S.clicks == 1, "not on REAPER's main window")
        S.front = T.FLOAT
        T.focusMoves({ T.FLOAT })
        T.ok(#S.clicks == 2 and S.clicks[2][1] == 582, "the title bar, away from the last click: " .. S.clicks[2][1])
        T.runDue()
        T.keyed(T.PAGE)
        said = T.report()
        T.ok(T.has(said, "W3: the page asked back, ") and T.has(said, "W3 back: the page has the focus"), said)
    "##);
}

/// MW4 on a Mac: whether the page's level follows the keyboard is seen only where the page is over
/// part of the plug-in window. Once the plug-in window is in front, the window server is asked at
/// the middle of the part the page covers — before the click on the title bar, which goes on as
/// ever — and the answer is a result: the page still drawn over the plug-in window there, or the
/// plug-in window drawn over it. Beside the plug-in window the level is not seen, and only the log
/// says so. On Windows nothing of this is asked.
#[test]
fn mw4_on_a_mac_asks_the_window_server_where_the_page_covers_the_plugin_window() {
    run_mac(r##"
        local S = T.S
        T.mac()
        T.probe()
        -- The page put over part of REAPER's plug-in window, at its bottom right, for want of room.
        local OVER = { id = T.PAGE.id, title = "HTML probe", class = "AXWindow/AXFloatingWindow/", app = T.PAGE.app, own = true,
          bounds = { x = 680, y = 378, w = 320, h = 360 }, client = { x = 680, y = 400, w = 320, h = 338 } }
        S.listed = { T.FLOAT, T.CHAIN, T.LOGIC, OVER }
        S.front = T.FLOAT
        T.turn()
        S.pg.answers = { { true, "over the rectangle to avoid" } }
        T.key(T.F7)
        T.keyed(T.PAGE)
        T.report()
        -- The window server: where the page is, the page, while it floats.
        local floating = true
        rawset(T.host.window, "ownsPoint", function(id, x, y)
          S.ownsAsked[#S.ownsAsked + 1] = { id, x, y }
          local b = OVER.bounds
          return not (floating and x >= b.x and x < b.x + b.w and y >= b.y and y < b.y + b.h)
        end)
        T.press("click-plugin")
        T.arrive(T.FLOAT)
        local level, bar = S.ownsAsked[#S.ownsAsked - 1], S.ownsAsked[#S.ownsAsked]
        T.ok(level[1] == T.FLOAT.id and level[2] == 840 and level[3] == 558, "the middle of the part the page covers, first")
        T.ok(bar[2] == 550 and bar[3] == 49 and #S.clicks == 1, "then the title bar, clicked as ever")
        T.runDue()
        T.keyed(T.PAGE)
        local said = T.report()
        T.ok(T.has(said, "MW4: the plug-in window is in front, and something is STILL DRAWN OVER it at 840,558, where the tool "
          .. "page covers it W3:"), said)
        floating = false
        T.press("click-plugin")
        T.arrive(T.FLOAT)
        T.runDue()
        T.keyed(T.PAGE)
        said = T.report()
        T.ok(T.has(said, "MW4: the plug-in window is in front, and drawn over the tool page where that covers it, at 840,558 W3:"), said)
        -- Beside it: not seen, in the log only.
        OVER.bounds = { x = 1000, y = 38, w = 520, h = 640 }
        local asked = #S.ownsAsked
        T.press("click-plugin")
        T.arrive(T.FLOAT)
        T.ok(#S.ownsAsked == asked + 1, "the title bar alone")
        T.runDue()
        T.keyed(T.PAGE)
        said = T.report()
        T.ok(not T.has(said, "MW4"), said)
        T.ok(T.count("MW4: the tool page is beside the plug-in window, so whether its level follows the keyboard is not seen") == 1, T.dump())
    "##);
}

/// The automatic show (W2 c) on a Mac shows the page F7 shows, never the page without activation,
/// which only F8 shows: with that page the one used last, the plug-in window arriving from another
/// application's window brings the tool page.
#[test]
fn on_a_mac_the_automatic_show_never_brings_the_page_without_activation() {
    run_mac(r##"
        local S = T.S
        T.mac()
        T.probe()
        local FINDER = { id = 31, title = "Documents", class = "AXWindow/AXStandardWindow/",
          app = { pid = 7000, exe = "Finder", name = "Finder", bundleId = "com.apple.finder" },
          bounds = { x = 0, y = 38, w = 800, h = 500 }, client = { x = 0, y = 60, w = 800, h = 478 } }
        T.set("autoShow", true)
        S.front = T.FLOAT
        T.turn()
        T.key(T.F8)
        T.ok(S.front == T.NA_PAGE and #S.npg.shows == 1, "the page without activation, by F8")
        T.key(T.F8)
        T.ok(not S.npg.shown, "and back")
        local shows = #S.pg.shows
        T.arrive(FINDER)
        T.arrive(T.FLOAT)
        T.ok(#S.npg.shows == 1 and #S.pg.shows == shows + 1 and S.pg.shows[#S.pg.shows].focus == true, "the tool page")
        T.keyed(T.PAGE)
        local said = T.report()
        T.ok(T.has(said, "automatic show: the front asked for"), said)
    "##);
}

/// MW2 and MW16 on a Mac: the captured key from the plug-in window shows the page in use beside it
/// without the keyboard, saying nothing; pressed again while it is shown so, it updates the page's
/// live region, again saying nothing over it. Before the page is built, it builds it and says so.
/// Should it ever fire while a window of this application has the keyboard — where no capture
/// applies — that is a result, and nothing else happens. And the page's modifiers are said in the
/// Mac's words, in Apple's order.
#[test]
fn on_a_mac_the_captured_key_shows_the_page_without_the_keyboard_then_updates_its_live_region() {
    run_mac(r##"
        local S = T.S
        T.mac()
        T.probe()
        T.arrive(T.FLOAT)
        T.ok(S.pg.prepares == 1, "prepared when the plug-in window first came forward")
        S.pg.answers = { { false, "not ready yet" } }
        T.captured(T.F12)
        T.ok(T.last() == "Building the page. Press the key again once it is ready.", T.last())
        T.ready()
        T.ok(not S.pg.shown, "not shown by itself")
        local spoken = #S.speech
        T.captured(T.F12)
        local asked = S.pg.shows[#S.pg.shows]
        T.ok(asked.focus == false and asked.avoid.x == T.FLOAT.bounds.x, "beside the plug-in window, without the keyboard")
        T.ok(S.pg.shown and S.front == T.FLOAT, "the plug-in window keeps the keyboard")
        T.ok(#S.speech == spoken, "nothing said")
        local said = T.report()
        T.ok(T.has(said, "F12 (captured, without the keyboard): the tool page shown, 0 ms after the key's callback; in front: the plug-in window"), said)
        spoken = #S.speech
        local shows = #S.pg.shows
        T.captured(T.F12)
        local live = S.pg.posts[#S.pg.posts]
        T.ok(live.kind == "live" and live.text == "Live region update 1, while the page does not have the keyboard.", tostring(live.text))
        T.ok(#S.speech == spoken and #S.pg.shows == shows, "nothing said over it, and not shown again")
        said = T.report()
        T.ok(T.has(said, "MW16: live region update 1 posted to the tool page, which does not have the keyboard; in front: the plug-in window"), said)
        -- In the page: no capture applies there; should this one fire, it is said and nothing happens.
        T.key(T.F7)
        T.keyed(T.PAGE)
        T.ok(S.front == T.PAGE, "the page with the keyboard")
        local posts, hides = #S.pg.posts, S.pg.hides
        T.captured(T.F12)
        T.ok(T.last() == "The captured key fired in a window of this application.", T.last())
        T.ok(#S.pg.posts == posts and S.pg.hides == hides and S.front == T.PAGE, "nothing else")
        said = T.report()
        T.ok(T.has(said, "MW16: the captured key FIRED while a window of this application had the keyboard; in front: the page"), said)
        -- What the page gets instead: its key, in the Mac's words.
        T.msg({ kind = "key", type = "keydown", key = "F12", code = "F12", ctrl = true, alt = true, shift = true, meta = true, target = "the document" })
        T.ok(T.count("W4 key: keydown Control+Option+Shift+Cmd+F12 (code F12) on the document") == 1, T.dump())
        said = T.report()
        T.ok(T.has(said, "Key presses the page saw: 1: Control+Option+Shift+Cmd+F12."), said)
    "##);
}
