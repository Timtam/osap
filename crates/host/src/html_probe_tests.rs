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
//! and the automatic show never brings the application window.
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

-- An event of the probe's module through its mailbox. The probe never waits, so each runs at
-- once: one that waited or was dropped would leave a check that something did not happen
-- nothing to check, and stops the scenario here instead.
local function ran(kind, f, ...)
  local got = T.call(kind, f, ...)
  assert(got == "Ran", kind .. ": " .. got)
end

-- The page's own window: a window of this application. And the same page's application window.
T.PAGE = { id = 4000, title = "HTML probe", class = "wxWindowNR", app = { pid = 9, exe = "automation-platform.exe" },
  bounds = { x = 1000, y = 0, w = 520, h = 640 }, client = { x = 1008, y = 30, w = 504, h = 602 } }
T.APP_PAGE = { id = 4001, title = "HTML probe window", class = "wxWindowNR", app = { pid = 9, exe = "automation-platform.exe" },
  bounds = { x = 1000, y = 0, w = 520, h = 640 }, client = { x = 1008, y = 30, w = 504, h = 602 } }

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

-- host.gui.webview: the pages — the tool page (S.pg, S.page, S.pageOpts) and the application
-- window (`window = "app"`: S.apg, S.appPage, S.appOpts), each described once. `show` answers the
-- page's `answers` in turn ({ true } when they run out); accepted with focus, the page is in front.
-- Declined, it is shown and the window in front stays. `prepare` answers true once the page is
-- ready (T.ready, T.appReady), "not ready yet" before. S.order names the application window's
-- shows and hides "show app" and "hide app".
local function pageState()
  return { shows = {}, hides = 0, posts = {}, answers = {}, shown = false, prepares = 0, ready = false }
end
S.pg, S.apg = pageState(), pageState()
rawset(T.host, "gui", T.strict("host.gui", {
  webview = function(opts)
    local isApp = opts.window == "app"
    assert(opts.window == nil or isApp, "the tool page by default, or an application window: " .. tostring(opts.window))
    local st, win, tag, described = S.pg, T.PAGE, "", S.page
    if isApp then
      st, win, tag, described = S.apg, T.APP_PAGE, " app", S.appPage
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
        S.front, S.chain = win, { win }
        S.epoch += 1
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
  T.source("tools/html-probe/src/main.luau")(T.host)
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
function T.closed(why) ran("page close", S.pageOpts.onClose, why) end
function T.appClosed(why) ran("page close", S.appOpts.onClose, why) end
-- A button of the page activated: `released` whether a key or a pointer went up in the page since
-- the button before, as the page reports it (true unless said otherwise). Each page counts its own
-- presses, as page.js does.
local presses = { tool = { counts = {}, previous = nil }, app = { counts = {}, previous = nil } }
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
"##;

/// A fresh VM with the scripted host — its events through the real mailbox — the runtime, the
/// prelude's matcher, the host-panel helpers and the ones above, on Windows; the scenario ends on
/// the harness's checks.
fn run(scenario: &str) {
    run_with_helpers("windows", &[("probe helpers", PR)], scenario)
}

/// What the probe declares at load: its three keys, the second left to Windows' own delivery
/// (`{ foreground = true }`, W2 b), the setting for W2 c — off by default, labelled with its
/// test — its page, and the window and focus events it follows.
#[test]
fn the_probe_loads_with_its_keys_its_setting_and_its_page() {
    run(r##"
        local S = T.S
        T.probe()
        T.ok(S.hotkeyFns[T.F7] and S.hotkeyFns[T.F8] and S.hotkeyFns[T.F9], "the three keys")
        T.ok(S.hotkeyOpts[T.F8] and S.hotkeyOpts[T.F8].foreground == true, "F8 is Windows' own WM_HOTKEY")
        T.ok(S.hotkeyOpts[T.F7] == false and S.hotkeyOpts[T.F9] == false, "the others arrive through the hook")
        local auto = S.defined.autoShow
        T.ok(auto and auto.default == false, "the automatic show is a setting, off by default")
        T.ok(T.has(auto.label, "test W2 c"), auto.label)
        T.ok(S.pageOpts.file == "ui/index.html" and S.pageOpts.title == "HTML probe", "the page")
        T.ok(S.pageOpts.window == nil, "a tool page, by default")
        T.ok(S.appOpts.file == "ui/index.html" and S.appOpts.title == "HTML probe window" and S.appOpts.window == "app",
          "the same page in an application window")
        T.ok(#S.fgCbs == 1 and #S.focusCbs == 1, "one window trigger and one focus callback")
        T.ok(#S.pg.shows == 0 and #S.apg.shows == 0, "nothing is shown at load")
        T.ok(T.count("[html-probe] revision 4 ready") == 1, "the revision, in the log at load")
        T.ok(T.count("the page's button 'Show as an application window' shows it in a window of its own") == 1,
          "the start line says how to get the application window")
        local said = T.report()
        T.ok(T.has(said, "HTML probe revision 4. The page's button Show as an application window"), said)
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
        T.ok(told.kind == "window" and told.window == "tool", "the tool page is told it is one")
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
        T.ok(told.kind == "window" and told.window == "app", "the application window is told it is one")
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
