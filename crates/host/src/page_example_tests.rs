//! The page example (`examples/page`), the module docs/building-a-page.md is written around,
//! loaded for real against the scripted host of `overlay_menu_tests.rs`, with the window
//! prelude's real matcher as `overlay_host_panel_tests.rs` has it, and a scripted
//! `host.gui.webview`: a page object that records what it was asked and answers what the
//! scenario scripts — as `html_probe_tests.rs` does for the probe.
//!
//! What the guide says the example does is pinned here, with no window: nothing is built at
//! load; a Notepad window coming to the front prepares the page, and nothing else does; the key
//! in Notepad shows the page beside it, with the keyboard, and in the page goes back — Notepad
//! asked to the front first, the page hidden after, and kept when Windows declines; a page that
//! loads after the user went elsewhere is not shown over them; and a button's round trip clicks
//! only once Notepad's foreground event has come, at the point worked out from its client area,
//! after asking whether anything else is drawn there, reads the line there with a callback, and
//! brings the page back in the reading's answer, the result posted to it as a `result` just
//! before the show, while Notepad has the keyboard — the page puts the keyboard focus on it
//! (`ui/page.js`, which no test here runs). A press while a round trip is under way, or a message
//! of any button while the page is not in front, starts nothing; a declined focus or a covered
//! point clicks nothing; another window coming forward at any step of the trip — its event before
//! Notepad's, Notepad's event late, or the window in front changed by the time the line is read —
//! gives the trip up, said without interrupting, and the page is not brought back over it; a page
//! Windows does not bring back is said through the host's speech with the result, a declined one
//! in words of its own, and logged; the key pressed during the read gives the round trip up; and
//! a prepare that failed is not tried again when Notepad comes forward.
//!
//! **Every event through the real mailbox**: the key, the page's message, its `onReady` and
//! `onClose`, a window coming forward and the reading's answer each run as a handler of the
//! example's module (`T.call`) — each checked to have run at once, as the example never waits —
//! and every scenario ends on the harness's own checks (`finish`): nothing asked of the scripted
//! host that it does not have, no handler that raised.

use super::overlay_host_panel_tests::run_with_helpers;

/// `host.gui.webview` as the scenarios script it: one page object, recording what it was asked
/// and answering what a scenario scripts, its window `T.PAGE` — a window of this application.
/// Shared with `overlay_page_tests.rs`, where the overlay runtime describes and shows the page.
/// A page described a second time (`O:pageAgain`) is allowed only where a scenario sets
/// `S.redescribe`, and shares the first one's record.
pub(crate) const PAGE_WEBVIEW: &str = r##"
local S = T.S
T.PAGE = { id = 4000, title = "Click helper", class = "wxWindowNR", own = true,
  app = { pid = 9, exe = "automation-platform.exe", name = "automation-platform" },
  bounds = { x = 900, y = 100, w = 520, h = 640 }, client = { x = 908, y = 130, w = 504, h = 602 } }

-- host.gui.webview: the page, described once. `show` answers the page's `answers` in turn, and
-- when they run out `not_ready` ("not ready yet") until the page has loaded (T.ready), true after;
-- accepted with focus, the page is in front. Declined (`refused`), it is shown and the window in
-- front stays. A scenario's answers are what the host answers: `true`, `true, err` or `false, err`.
S.pg = { shows = {}, hides = 0, posts = {}, answers = {}, shown = false, prepares = 0, ready = false,
  prepareAnswers = {} }
rawset(T.host, "gui", T.strict("host.gui", {
  webview = function(opts)
    assert(S.page == nil or S.redescribe, "the page is described once")
    local st = S.pg
    local p = {}
    function p.show(self, o)
      assert(self == p, "page:show is a method")
      st.shows[#st.shows + 1] = { focus = o and o.focus, avoid = o and o.avoid }
      S.order[#S.order + 1] = "show"
      local a = table.remove(st.answers, 1) or (st.ready and { true } or { false, T.E("not_ready", "not ready yet") })
      if a[1] or T.is(a[2], "refused") then
        st.shown = true
        p.window = T.PAGE.id
      end
      if a[1] and not (o and o.focus == false) then
        S.front, S.chain = T.PAGE, { T.PAGE }
        S.epoch += 1
      end
      return a[1], a[2]
    end
    function p.hide(self)
      assert(self == p, "page:hide is a method")
      st.hides += 1
      st.shown = false
      S.order[#S.order + 1] = "hide"
    end
    function p.post(self, v)
      assert(self == p, "page:post is a method")
      st.posts[#st.posts + 1] = v
      S.order[#S.order + 1] = "post " .. tostring(v.kind)
    end
    function p.prepare(self)
      assert(self == p, "page:prepare is a method")
      st.prepares += 1
      local a = table.remove(st.prepareAnswers, 1)
      if a then return a[1], a[2] end
      if st.ready then return true end
      return false, T.E("not_ready", "not ready yet")
    end
    function p.isShown(self) return st.shown end
    function p.focus(self) return st.shown end
    function p.close(self) st.shown = false; p.window = nil end
    S.page, S.pageOpts = p, opts
    return p
  end,
}))

"##;

/// The example's helpers, on top of the host-panel ones: Notepad, the page, the window events,
/// the reading's answer, and the example loaded.
const EX: &str = r##"
local S = T.S
T.KEY = "Ctrl+Shift+Win+Alt+F12"

-- An event of the example's module through its mailbox. The example never waits, so each runs
-- at once: one that waited or was dropped stops the scenario here.
local function ran(kind, f, ...)
  local got = T.call(kind, f, ...)
  assert(got == "Ran", kind .. ": " .. got)
end

-- Notepad, as Windows 11 describes it (the page's own window is the shared script's, T.PAGE).
T.NOTEPAD = { id = 300, title = "Untitled - Notepad", class = "Notepad",
  app = { pid = 3000, exe = "Notepad.exe", name = "Notepad" },
  bounds = { x = 100, y = 100, w = 800, h = 600 }, client = { x = 108, y = 131, w = 784, h = 561 } }
S.listed = { T.MAIN, T.FX, T.OTHER, T.NOTEPAD }

-- host.window.onTrigger, with or without options: T.arrive plays a window coming to the front,
-- its callback an event of the module's.
S.fgCbs, S.fgOpts = {}, {}
rawset(T.host.window, "onTrigger", function(m, opts, cb)
  if cb == nil then
    cb, opts = opts, nil
  end
  S.fgCbs[#S.fgCbs + 1] = cb
  S.fgOpts[#S.fgOpts + 1] = { matcher = m, opts = opts }
end)
function T.arrive(win)
  S.front, S.chain = win, { win }
  S.epoch += 1
  for _, cb in ipairs(S.fgCbs) do ran("window", cb, win) end
end
S.owns = true
S.ownsAsked = {}
rawset(T.host.window, "ownsPoint", function(id, x, y)
  S.ownsAsked[#S.ownsAsked + 1] = { id, x, y }
  S.order[#S.order + 1] = "ownsPoint"
  return S.owns
end)
do
  local focus = T.host.window.focus
  rawset(T.host.window, "focus", function(id)
    S.order[#S.order + 1] = "focus " .. tostring(id)
    return focus(id)
  end)
  local submit = T.submit
  T.submit = function(what, opts, cb)
    S.order[#S.order + 1] = "read"
    return submit(what, opts, cb)
  end
end

-- An activation of `win` that waited behind the module's other events: by the time it runs, the
-- window in front is already another one (`now`), whose own event has not come yet.
function T.arriveLate(win, now)
  S.front, S.chain = now, { now }
  S.epoch += 1
  for _, cb in ipairs(S.fgCbs) do ran("window", cb, win) end
end

-- The example, loaded as the host loads it; its clicks in S.order as well.
function T.example()
  T.modules("examples/page/")
  local click = T.host.input.click
  rawset(T.host.input, "click", function(x, y, o)
    S.order[#S.order + 1] = "click"
    return click(x, y, o)
  end)
  T.source("examples/page/src/main.luau")(T.host)
end

-- The key pressed: the epoch turns over, and its callback runs as a handler.
function T.key()
  local fn = S.hotkeyFns[T.KEY]
  assert(fn ~= nil, "no hotkey " .. T.KEY)
  S.epoch += 1
  ran("hotkey", fn)
end
-- The page sends `m`; the page has loaded; the page's window goes ("user" alone, or "failed" with
-- the host's error value beside it, as onClose(how, err) has them).
function T.msg(m) ran("page message", S.pageOpts.onMessage, m) end
function T.ready()
  S.pg.ready = true
  ran("page ready", S.pageOpts.onReady)
end
function T.closed(how)
  ran("page close", S.pageOpts.onClose, how, how == "failed" and T.E("failed", "the page failed") or nil)
end

-- The read asked last, answered through the module's mailbox: `rows` are { text, y } of the rows
-- read, 16 high (none: nothing read there), or `failed` is the message of the error value it could
-- not be read with, `code` its code (default `failed`).
function T.answer(rows, failed, code)
  local r = S.reads[#S.reads]
  assert(r and not r.answered, "no read waits")
  r.answered = true
  local g = r.region
  local lines, words, texts = {}, {}, {}
  for k, row in ipairs(rows or {}) do
    local box = { text = row[1], x = g[1], y = row[2], w = g[3] - g[1], h = 16 }
    lines[k] = { text = row[1], x = box.x, y = box.y, w = box.w, h = box.h, words = { box } }
    words[k] = box
    texts[k] = row[1]
  end
  local status = failed and "failed" or (#lines > 0 and "text" or "none")
  ran("answer", r.cb, { x = g[1], y = g[2], w = g[3] - g[1], h = g[4] - g[2], status = status,
    text = table.concat(texts, "\n"), lines = lines, words = words, lang = failed and "" or "en-US",
    error = failed and T.E(code or "failed", failed) or nil })
end

-- What was said last, and whether it interrupted; the page's status line as it was last posted
-- — a status or a round trip's result — and the kind of that post.
function T.last() return S.speech[#S.speech] and S.speech[#S.speech].text or "" end
function T.lastInterrupts() return S.speech[#S.speech] and S.speech[#S.speech].interrupt end
function T.status()
  for i = #S.pg.posts, 1, -1 do
    local k = S.pg.posts[i].kind
    if k == "status" or k == "result" then return S.pg.posts[i].text, k end
  end
  return nil
end
-- S.order from entry `from` on, as one string.
function T.since(from)
  local out = {}
  for i = from + 1, #S.order do out[#out + 1] = S.order[i] end
  return table.concat(out, ",")
end

-- An assertion whose failure prints the log, everything said and the order of calls.
function T.ok(cond, what)
  if cond then return end
  print(T.dump())
  local said = {}
  for _, s in ipairs(S.speech) do said[#said + 1] = s.text end
  print("said: " .. table.concat(said, " | "))
  print("order: " .. table.concat(S.order, ","))
  error(tostring(what or "assertion failed"), 2)
end

-- Notepad came forward and the page was prepared and has loaded; the key shows it beside
-- Notepad, in front.
function T.shown()
  T.arrive(T.NOTEPAD)
  T.ready()
  T.key()
  T.ok(S.front == T.PAGE, "the page is in front")
end
"##;

/// A fresh VM with the scripted host — its events through the real mailbox — the runtime, the
/// prelude's matcher, the host-panel helpers and the ones above, on Windows; the scenario ends
/// on the harness's checks.
fn run(scenario: &str) {
    run_with_helpers("windows", &[("the scripted page", PAGE_WEBVIEW), ("page example helpers", EX)], scenario)
}

/// What the example declares at load: its key, its page — a tool page, from its own folder —
/// and one window trigger that also reports the window in front at load. Nothing is built and
/// nothing is shown; it says how to start, naming the key in the words the host's own
/// `host.keys.describe` has for it on Windows (the scripted one hands a spec back unchanged).
#[test]
fn the_example_loads_with_its_key_and_its_page_and_builds_nothing() {
    let spoken = crate::backend::describe_spec_for(
        crate::backend::KeyOs::Windows,
        "Ctrl+Shift+Win+Alt+F12",
        crate::backend::KeyStyle::Spoken,
    )
    .expect("the example's key parses");
    let scenario = r##"
        local S = T.S
        rawset(T.host.keys, "describe", function(spec, opts)
          assert(spec == T.KEY and opts == nil, "the example's key, in the spoken style")
          return "@SPOKEN@"
        end)
        T.example()
        T.ok(S.hotkeyFns[T.KEY] ~= nil, "the key")
        T.ok(S.pageOpts.file == "ui/index.html" and S.pageOpts.title == "Click helper", "the page")
        T.ok(S.pageOpts.window == nil, "a tool page, by default")
        T.ok(#S.fgCbs == 1 and S.fgOpts[1].opts and S.fgOpts[1].opts.initial == true, "one trigger, initial")
        T.ok(#S.pg.shows == 0 and S.pg.prepares == 0, "nothing built or shown at load")
        T.ok(T.last() == "Page demo ready. Press Control+Alt+Shift+Windows+F12 in a Notepad window.", T.last())
        T.ok(S.logs[#S.logs] == "Page demo loaded: Control+Alt+Shift+Windows+F12 in a Notepad window shows the page.", S.logs[#S.logs])
    "##;
    run(&scenario.replace("@SPOKEN@", &spoken));
}

/// A Notepad window coming to the front prepares the page, and not once it has loaded; after the
/// page failed, the next Notepad window prepares it again. Another window never does, and
/// preparing shows nothing. A prepare that answers a failure is logged and not tried again from
/// there, and the key still builds the page.
#[test]
fn notepad_coming_forward_prepares_the_page_until_it_has_loaded() {
    run(r##"
        local S = T.S
        T.example()
        T.arrive(T.OTHER)
        T.ok(S.pg.prepares == 0, "not for another window")
        T.arrive(T.NOTEPAD)
        T.ok(S.pg.prepares == 1, "prepared for Notepad")
        T.ready()
        T.ok(#S.pg.shows == 0, "prepared, and not shown: nobody asked for it")
        T.arrive(T.OTHER)
        T.arrive(T.NOTEPAD)
        T.ok(S.pg.prepares == 1, "not again once it has loaded")
        T.closed("failed")
        T.arrive(T.NOTEPAD)
        T.ok(S.pg.prepares == 2, "again after the page failed")
        -- Failed: not tried again when Notepad comes forward, and the key builds the page.
        S.pg.ready = false
        T.arrive(T.OTHER)
        S.pg.prepareAnswers = { { false, T.E("failed", "could not be built") } }
        T.arrive(T.NOTEPAD)
        T.ok(S.pg.prepares == 3, "prepared once more")
        T.ok(S.logs[#S.logs] == "Page demo: the page was not prepared (could not be built); the key builds it from now on.", S.logs[#S.logs])
        T.arrive(T.OTHER)
        T.arrive(T.NOTEPAD)
        T.ok(S.pg.prepares == 3, "not again from the trigger after a failure")
        T.key()
        T.ok(#S.pg.shows == 1 and T.last() == "Building the page.", "the key builds it: " .. T.last())
    "##);
}

/// The key outside Notepad says where to press it and shows nothing. In Notepad, the first press
/// builds the page and says so; the page comes once it has loaded, beside Notepad, with the
/// keyboard, and with Notepad's title posted to it. Pressed in the page, the key goes back:
/// Notepad asked to the front FIRST, the page hidden after; declined, the page stays.
#[test]
fn the_key_shows_the_page_beside_notepad_and_in_the_page_goes_back() {
    run(r##"
        local S = T.S
        T.example()
        S.front = T.OTHER
        T.turn()
        T.key()
        T.ok(T.last() == "Press this key in a Notepad window." and #S.pg.shows == 0, T.last())
        S.front = T.NOTEPAD
        T.turn()
        T.key()
        T.ok(#S.pg.shows == 1 and S.pg.shows[1].focus == true, "asked once, with the keyboard")
        local a = S.pg.shows[1].avoid
        T.ok(a.x == 100 and a.y == 100 and a.w == 800 and a.h == 600, "beside Notepad: its bounds avoided")
        T.ok(S.pg.posts[1].kind == "target" and S.pg.posts[1].title == "Untitled - Notepad", "Notepad's title, for the page")
        T.ok(T.last() == "Building the page.", T.last())
        T.ready()
        T.ok(#S.pg.shows == 2 and S.front == T.PAGE, "shown once it has loaded")
        local from = #S.order
        T.key()
        T.ok(T.since(from) == "focus 300,hide", "Notepad first, then the page: " .. T.since(from))
        -- Declined: the page stays in front, and says so.
        T.arrive(T.NOTEPAD)
        T.key()
        T.ok(S.front == T.PAGE, "shown again")
        S.focusAnswer = false
        from = #S.order
        T.key()
        T.ok(T.since(from) == "focus 300,post status", "not hidden: " .. T.since(from))
        T.ok(T.status() == "Notepad would not come to the front.", T.status())
        -- A show Windows declines is said as the keyboard kept where it was, not as a page that
        -- is not there; any other reason as the page not coming.
        S.focusAnswer = true
        T.arrive(T.NOTEPAD)
        S.pg.answers = { { false, T.E("refused", "declined by the system") } }
        T.key()
        T.ok(T.last() == "Windows did not bring the page to the front; if it does not come by itself, the key brings it.", T.last())
        S.pg.answers = { { false, T.E("failed", "could not be built") } }
        T.key()
        T.ok(T.last() == "The page did not come to the front: could not be built.", T.last())
    "##);
}

/// A page that loads after the user went elsewhere is not shown over them; the next key shows
/// it at once.
#[test]
fn a_page_that_loads_after_the_user_went_elsewhere_is_not_shown() {
    run(r##"
        local S = T.S
        T.example()
        S.front = T.NOTEPAD
        T.turn()
        T.key()
        T.ok(T.last() == "Building the page.", T.last())
        T.arrive(T.OTHER)
        T.ready()
        T.ok(#S.pg.shows == 1 and S.front == T.OTHER, "not shown over another window")
        T.arrive(T.NOTEPAD)
        T.key()
        T.ok(#S.pg.shows == 2 and S.front == T.PAGE, "the next key shows it at once")
    "##);
}

/// The round trip, on events only: a button asks Notepad to the front and clicks nothing until
/// it is there; on its foreground event, the point is asked about and clicked — the middle of its
/// client area, from the area's top left — and the line there read with a callback, in whole
/// corners inside the client area; the page comes back only in the reading's answer, beside
/// Notepad and with the keyboard, the result posted as a `result` just before the show, while
/// Notepad is still in front, naming the row nearest the click, and nothing said by the host's
/// speech. A second press while the round trip is under way asks nothing, said in a status. The
/// other button clicks lower left, and a line with nothing on it is said so.
#[test]
fn a_button_clicks_once_notepad_is_in_front_and_the_page_comes_back_with_the_line() {
    run(r##"
        local S = T.S
        T.example()
        T.shown()
        local shows, focuses = #S.pg.shows, #S.focusCalls
        local from = #S.order
        T.msg({ action = "middle" })
        T.ok(#S.focusCalls == focuses + 1 and S.focusCalls[#S.focusCalls] == 300, "Notepad asked to the front")
        T.ok(#S.clicks == 0 and #S.reads == 0, "nothing clicked or read before it is there")
        T.msg({ action = "lower" })
        T.ok(#S.focusCalls == focuses + 1, "a second press while one is under way asks nothing")
        T.ok(T.status() == "The last click is still under way.", T.status())
        T.arrive(T.NOTEPAD)
        T.ok(#S.clicks == 1, "one click")
        local x, y = S.clicks[1][1], S.clicks[1][2]
        T.ok(x == 108 + 392 and y == 131 + 280, "the middle of the client area: " .. x .. "," .. y)
        local asked = S.ownsAsked[#S.ownsAsked]
        T.ok(asked[1] == 300 and asked[2] == x and asked[3] == y, "whether Notepad is drawn there, asked first")
        local r = S.reads[1].region
        T.ok(r[1] == 108 and r[2] == 387 and r[3] == 892 and r[4] == 435, "the line around the click: " .. T.region(r))
        T.ok(#S.pg.shows == shows, "the page waits for the reading")
        local said = #S.speech
        T.answer({ { "first line", 380 }, { "second line", 403 } })
        T.ok(#S.speech == said, "the result is the page's to say, not the host's speech")
        T.ok(#S.pg.shows == shows + 1 and S.front == T.PAGE, "the page back in the reading's answer")
        local back = S.pg.shows[#S.pg.shows]
        T.ok(back.focus == true and back.avoid.x == 100 and back.avoid.w == 800, "beside Notepad, with the keyboard")
        T.ok(T.since(from) == "focus 300,post status,ownsPoint,click,read,post result,show", T.since(from))
        T.ok(T.status() == "Clicked the middle. The line there: second line", T.status())
        -- The other button: lower left, a line with nothing on it.
        T.msg({ action = "lower" })
        T.arrive(T.NOTEPAD)
        T.ok(#S.clicks == 2 and S.clicks[2][1] == 108 + 196 and S.clicks[2][2] == 131 + 420,
          "a quarter across, three quarters down: " .. S.clicks[2][1] .. "," .. S.clicks[2][2])
        T.answer({})
        local text, kind = T.status()
        T.ok(S.front == T.PAGE and text == "Clicked lower left. Nothing is written there." and kind == "result", text)
        -- A read that failed is said with its reason.
        T.msg({ action = "middle" })
        T.arrive(T.NOTEPAD)
        T.answer(nil, "the text recogniser has not answered a region for 5 s", "timeout")
        T.ok(T.status() == "Clicked the middle. It could not be read: the text recogniser has not answered a region for 5 s.", T.status())
    "##);
}

/// Nothing is clicked when something else is drawn at the point — the page comes back all the
/// same, on that result, posted first — when Notepad would not come to the front, or when another
/// window comes forward first; each is said, and the next press is a round trip of its own.
#[test]
fn a_covered_point_a_declined_focus_or_another_window_clicks_nothing() {
    run(r##"
        local S = T.S
        T.example()
        T.shown()
        -- Covered.
        S.owns = false
        T.msg({ action = "middle" })
        local from = #S.order
        T.arrive(T.NOTEPAD)
        T.ok(T.since(from) == "ownsPoint,post result,show", "the result first, then the page: " .. T.since(from))
        T.ok(#S.clicks == 0 and #S.reads == 0, "nothing clicked through a window drawn over it, nothing read")
        T.ok(S.front == T.PAGE and T.status() == "Something covers Notepad there, or it has closed; nothing was clicked.", T.status())
        S.owns = true
        -- Declined.
        S.focusAnswer = false
        local focuses = #S.focusCalls
        T.msg({ action = "middle" })
        T.ok(#S.focusCalls == focuses + 1, "asked")
        local text, kind = T.status()
        T.ok(text == "Notepad would not come to the front; nothing was clicked." and kind == "status", text)
        S.focusAnswer = true
        T.msg({ action = "middle" })
        T.ok(#S.focusCalls == focuses + 2, "the next press asks again: nothing was left under way")
        -- Another window first.
        T.arrive(T.OTHER)
        T.ok(#S.clicks == 0, "nothing clicked")
        T.ok(T.last() == "Another window came to the front; nothing was clicked.", T.last())
        T.ok(T.lastInterrupts() == false, "said behind what the screen reader says about that window")
        T.arrive(T.NOTEPAD)
        T.ok(#S.clicks == 0, "and not when Notepad comes later")
    "##);
}

/// A press that reaches the module while the page is not in front — it waited while the user went
/// elsewhere — is not acted on, whichever button sent it; a problem the page reports is logged all
/// the same. A page Windows does not bring back after the click is said through the host's
/// speech, with the result, and logged; the page has the result as well, for when it comes. The
/// key pressed while the line is being read gives the round trip up: its reading changes
/// nothing.
#[test]
fn a_press_away_from_the_page_a_page_not_brought_back_and_a_trip_given_up() {
    run(r##"
        local S = T.S
        T.example()
        T.shown()
        T.arrive(T.OTHER)
        local focuses, hides = #S.focusCalls, S.pg.hides
        T.msg({ action = "middle" })
        T.ok(#S.focusCalls == focuses and #S.clicks == 0, "a press while the page is not in front starts nothing")
        T.msg({ action = "back" })
        T.ok(#S.focusCalls == focuses and S.pg.hides == hides, "nor does Back: the user is not pulled to Notepad")
        T.msg({ kind = "problem", text = "boom (page.js:3)" })
        T.ok(S.logs[#S.logs] == "Page demo: the page reports: boom (page.js:3)", S.logs[#S.logs])
        T.arrive(T.NOTEPAD)
        T.key()
        T.ok(S.front == T.PAGE, "the page again")
        -- Not brought back.
        T.msg({ action = "middle" })
        T.arrive(T.NOTEPAD)
        S.pg.answers = { { false, T.E("refused", "declined by the system") } }
        T.answer({ { "hello", 403 } })
        T.ok(T.status() == "Clicked the middle. The line there: hello", T.status())
        T.ok(S.logs[#S.logs] == "Page demo: the page did not come back (declined by the system); the result waits on it: Clicked the middle. The line there: hello", S.logs[#S.logs])
        T.ok(T.last() == "Windows did not bring the page to the front; if it does not come by itself, the key brings it. Clicked the middle. The line there: hello", T.last())
        T.ok(T.lastInterrupts() == true, "said at once: the user is in Notepad")
        -- Any other reason is said as the page not coming; the result is on the page as well.
        T.arrive(T.NOTEPAD)
        T.key()
        T.ok(S.front == T.PAGE, "the page again")
        T.msg({ action = "middle" })
        T.arrive(T.NOTEPAD)
        S.pg.answers = { { false, T.E("failed", "could not be built") } }
        T.answer({ { "hello again", 403 } })
        T.ok(T.status() == "Clicked the middle. The line there: hello again", T.status())
        T.ok(T.last() == "The page did not come to the front: could not be built. Clicked the middle. The line there: hello again", T.last())
        -- Given up: the key pressed in Notepad while the line is read shows the page; the
        -- reading's answer comes to nothing.
        T.key()
        T.ok(S.front == T.PAGE, "shown by the key")
        T.msg({ action = "lower" })
        T.arrive(T.NOTEPAD)
        T.key()
        local shows, posts = #S.pg.shows, #S.pg.posts
        T.answer({ { "late", 403 } })
        T.ok(#S.pg.shows == shows and #S.pg.posts == posts, "the reading after the key changes nothing")
    "##);
}

/// The user goes elsewhere in the middle of a round trip, and the trip is given up at whichever
/// step it is: nothing more is clicked, the page is not brought back over the window they went
/// to, and it is said once, without interrupting. Notepad's foreground event that waited until
/// another window was in front clicks nothing; another window's event while the line is read
/// gives the trip up before the reading comes; and a reading that comes after the window in front
/// changed, before that window's event, does not bring the page back either.
#[test]
fn going_elsewhere_during_a_round_trip_gives_it_up_at_any_step() {
    run(r##"
        local S = T.S
        T.example()
        T.shown()
        -- Notepad's event waited, and another window is in front when it runs.
        T.msg({ action = "middle" })
        T.arriveLate(T.NOTEPAD, T.OTHER)
        T.ok(#S.clicks == 0 and #S.reads == 0, "nothing clicked or read")
        T.ok(T.last() == "Another window came to the front; nothing was clicked." and T.lastInterrupts() == false, T.last())
        -- Another window's event while the line is read.
        T.arrive(T.NOTEPAD)
        T.key()
        T.ok(S.front == T.PAGE, "the page again")
        T.msg({ action = "middle" })
        T.arrive(T.NOTEPAD)
        T.ok(#S.clicks == 1 and #S.reads == 1, "clicked and reading")
        local shows, posts, said = #S.pg.shows, #S.pg.posts, #S.speech
        T.arrive(T.OTHER)
        T.ok(T.last() == "Clicked, but another window came to the front before the line was read." and T.lastInterrupts() == false, T.last())
        T.answer({ { "late", 403 } })
        T.ok(#S.pg.shows == shows and #S.pg.posts == posts and #S.speech == said + 1, "the reading brings nothing back")
        -- The window in front changed before the reading, and its event has not come yet.
        T.arrive(T.NOTEPAD)
        T.key()
        T.msg({ action = "lower" })
        T.arrive(T.NOTEPAD)
        shows, posts = #S.pg.shows, #S.pg.posts
        S.front = T.OTHER
        T.answer({ { "late", 403 } })
        T.ok(#S.pg.shows == shows and #S.pg.posts == posts, "not shown over the window the user went to")
        T.ok(T.last() == "Clicked, but another window came to the front before the line was read.", T.last())
        said = #S.speech
        T.arrive(T.OTHER)
        T.ok(#S.speech == said and #S.pg.shows == shows, "its event, when it comes, adds nothing")
    "##);
}
