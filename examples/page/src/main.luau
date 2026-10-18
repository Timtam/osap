-- A page whose buttons act in another window (Windows): the example of docs/building-a-page.md.
--
-- Press Ctrl+Shift+Win+Alt+F12 in a Notepad window: an HTML page comes up beside it, with two
-- buttons that click into Notepad's text and read the line clicked on, and one that goes back
-- to Notepad. The same key pressed in the page goes back too. A click in the text only moves
-- Notepad's caret, so nothing here changes what is written.
--
-- Every click is a round trip, as every action in another window is while the page has the
-- front: Notepad is asked to the front first, clicked once it is there — on its foreground
-- event, never after a time — the line is read, and the page is brought back with what was
-- read, the keyboard focus on it. Any other window coming to the front on the way gives the
-- trip up.

local KEY = "Ctrl+Shift+Win+Alt+F12"
local NOTEPAD = { app = { name = "notepad" } }

-- Where each button clicks, as fractions of Notepad's client area: the point is worked out from
-- the area's top left — the window's origin — and its size, so it is inside the text at any
-- size of the window. A plug-in, whose layout does not grow with its window, is given offsets
-- in pixels from the origin instead.
local POINTS = {
  middle = { x = 0.5, y = 0.5, words = "the middle" },
  lower = { x = 0.25, y = 0.75, words = "lower left" },
}

-- How far above and below the click the line is read, in pixels: the line at the click and its
-- neighbours, of which the row nearest the click is taken.
local LINE = 24

local page -- described below; the functions before it reach it only when they run
local target = nil -- the Notepad window the page was last shown beside, as a window table
local wanted = false -- a show answered `not_ready`: shown from onReady, if still wanted
local ready = false -- the page is built and has loaded
local prepareFailed = false -- a prepare failed: from then on only the key builds the page
local trip = nil -- the click under way: { point, id, stage = "forward" | "read" }

-- A line for the page's status, its live region: the screen reader says it while the page is in
-- front.
local function status(text)
  page:post({ kind = "status", text = text })
end

-- Whether window `id` is the one in front now — read afresh, not the answer kept for the epoch,
-- since the user may have gone elsewhere after the event being handled came.
local function inFront(id)
  local front = host.window.foreground()
  return front ~= nil and front.id == id
end

-- What a `show` that answered false is said as, by its error value's code. Declined
-- (`refused`), Windows kept the keyboard where it was: the page is shown without it — or hidden,
-- where it would be over the window — and has been seen to come to the front by itself a moment
-- later, so the user is not told it is not there. Any other code: its message.
local function notShown(err)
  if err.code == host.errors.REFUSED then
    return "Windows did not bring the page to the front; if it does not come by itself, the key brings it."
  end
  return "The page did not come to the front: " .. err .. "."
end

-- Window `w` as it is now — it may have moved — or nil when it is gone.
local function current(w)
  if not w then
    return nil
  end
  for _, x in ipairs(host.window.list({ pids = { w.app.pid } })) do
    if x.id == w.id then
      return x
    end
  end
  return nil
end

-- The page beside `w`, with the keyboard. `true` with an error value beside it is the page in
-- front over part of Notepad, with no room beside it (`too_large`): shown all the same.
local function showBeside(w)
  target = w
  page:post({ kind = "target", title = w.title })
  local ok, err = page:show({ focus = true, avoid = w.bounds })
  if ok then
    wanted = false
  elseif err.code == host.errors.NOT_READY then
    -- The first show builds the page; onReady says when it has loaded.
    wanted = true
    host.speech.output("Building the page.", { interrupt = true })
  else
    wanted = false
    host.speech.output(notShown(err), { interrupt = true })
  end
end

-- The page built ahead of the key. A build that failed is not started again from here, where
-- nobody asked for it: the key builds the page from then on, and says why it cannot.
local function prepare()
  local ok, err = page:prepare()
  if not ok and err.code ~= host.errors.NOT_READY then
    prepareFailed = true
    host.log.info("Page demo: the page was not prepared (" .. err .. "); the key builds it from now on.")
  end
end

-- Back to Notepad: its window to the front FIRST, while the page still has it, then the page
-- hidden. Declined, the page stays in front rather than leave the keyboard to whichever window
-- Windows picks.
local function backToTarget()
  trip = nil
  local w = current(target)
  if not w then
    status("The Notepad window is gone.")
    return
  end
  if not host.window.focus(w.id) then
    status("Notepad would not come to the front.")
    return
  end
  page:hide()
end

-- Point `p` of window `w` in screen coordinates: the client area's top left, plus the fractions
-- of its size, in whole pixels.
local function pointIn(w, p)
  local c = w.client
  return c.x + math.floor(c.w * p.x), c.y + math.floor(c.h * p.y)
end

-- The band of `w`'s client area around height `y`, as the corners a read takes: whole numbers,
-- inside the client area.
local function lineAround(w, y)
  local c = w.client
  return { c.x, math.max(c.y, y - LINE), c.x + c.w, math.min(c.y + c.h, y + LINE) }
end

-- What reading `r` says about the line at height `y`: the row read nearest to it.
local function lineWords(r, y)
  if r.status == "failed" then
    return "It could not be read: " .. r.error .. "."
  end
  local best, distance = nil, math.huge
  for _, row in ipairs(r.lines) do
    local d = math.abs(row.y + row.h / 2 - y)
    if d < distance then
      best, distance = row, d
    end
  end
  if not best then
    return "Nothing is written there."
  end
  return "The line there: " .. best.text
end

-- The round trip given up: another window came to the front, so the user went elsewhere. The
-- page is not brought back over them. Said without interrupting, so that it comes after what
-- the screen reader says about the window they went to rather than cutting it off.
local function giveUp()
  local clicked = trip ~= nil and trip.stage == "read"
  trip = nil
  if clicked then
    host.speech.output("Clicked, but another window came to the front before the line was read.", { interrupt = false })
  else
    host.speech.output("Another window came to the front; nothing was clicked.", { interrupt = false })
  end
end

-- The end of a round trip: `text` to the page as the trip's result, then the page back beside
-- `w`, with the keyboard. Posted first, while Notepad has the keyboard: the page puts it into its
-- status line and the keyboard focus on it (ui/page.js), so that the page comes back on the
-- result — a page coming to the front is announced, and the announcement cuts off a sentence
-- said just before it. Windows may refuse the page the front; then the host's own speech says
-- so, and the result, which goes to the log as well. The page keeps it, focused, for when it
-- comes by itself or by the key.
local function comeBack(w, text)
  trip = nil
  page:post({ kind = "result", text = text })
  local ok, err = page:show({ focus = true, avoid = w.bounds })
  if not ok then
    host.log.info("Page demo: the page did not come back (" .. err .. "); the result waits on it: " .. text)
    host.speech.output(notShown(err) .. " " .. text, { interrupt = true })
  end
end

-- The round trip's second step, Notepad (`w`) in front: one click, if nothing else is drawn at
-- the point, then a read of the line there; the page comes back in the read's answer — if
-- Notepad is still in front then.
local function arrived(w)
  if not inFront(w.id) then
    giveUp() -- its foreground event waited, and the user has gone on since
    return
  end
  local mine = trip
  mine.stage = "read"
  local x, y = pointIn(w, mine.point)
  if host.window.ownsPoint(w.id, x, y) == false then
    comeBack(w, "Something covers Notepad there; nothing was clicked.")
    return
  end
  host.input.click(x, y)
  host.ocr.recognize(lineAround(w, y), function(r)
    if trip ~= mine then
      return -- given up meanwhile: another window came, the key was pressed, or the page closed
    end
    if not inFront(mine.id) then
      giveUp() -- the user went elsewhere, and that window's foreground event has not come yet
      return
    end
    comeBack(w, "Clicked " .. mine.point.words .. ". " .. lineWords(r, y))
  end)
end

-- A button that clicks: the round trip's first step, never a second on top of one. Notepad is
-- asked to the front; the click waits for its foreground event (below).
local function clickAndRead(p)
  if trip then
    status("The last click is still under way.")
    return
  end
  local w = current(target)
  if not w then
    status("The Notepad window is gone.")
    return
  end
  trip = { point = p, id = w.id, stage = "forward" }
  if not host.window.focus(w.id) then
    trip = nil
    status("Notepad would not come to the front; nothing was clicked.")
  end
end

-- What the page sends: the action of the button pressed, or a problem its script ran into. A
-- press counts only while the page is in front: one that reached the module after the user went
-- elsewhere is not acted on.
local function onPageMessage(m)
  if type(m) ~= "table" then
    return
  end
  if m.kind == "problem" then
    host.log.info("Page demo: the page reports: " .. tostring(m.text))
    return
  end
  if not inFront(page.window) then
    return
  end
  if m.action == "back" then
    backToTarget()
  elseif POINTS[m.action] then
    clickAndRead(POINTS[m.action])
  end
end

-- The page has loaded. Shown now only if it was asked for and the user is still in that Notepad
-- window: the first build takes seconds, and a page that came up over wherever the user went
-- meanwhile would take the keyboard from them.
local function onPageReady()
  ready = true
  if wanted and target and inFront(target.id) then
    showBeside(current(target) or target)
  else
    wanted = false
  end
end

-- The page's window went without the module asking: "user" (its close box, or Alt+F4 in it —
-- only hidden) or "failed" (gone: built again by the next show, or when Notepad next comes
-- forward), with an error value beside it that says how.
local function onPageClose(how)
  trip = nil
  if how == "failed" then
    ready = false
  end
end

page = host.gui.webview({
  file = "ui/index.html",
  title = "Click helper", -- what the screen reader says as the page comes to the front
  onMessage = onPageMessage,
  onReady = onPageReady,
  onClose = onPageClose,
})

-- Every window that comes to the front. While a round trip is under way, Notepad's arrival is
-- the moment to click, and any other window means the user went elsewhere: the trip is given
-- up, whichever step it is at. Otherwise a Notepad window coming forward builds the page ahead
-- of the first key, so that WebView2's first start, which takes seconds, is not in the key's way.
host.window.onTrigger({}, { initial = true }, function(w)
  if trip then
    if w.id ~= trip.id then
      giveUp()
    elseif trip.stage == "forward" then
      arrived(w)
    end
    return
  end
  if not ready and not prepareFailed and host.window.test(NOTEPAD, w) then
    prepare()
  end
end)

-- The key: in a Notepad window it brings the page beside it; in the page it goes back.
host.hotkey.register(KEY, function()
  if inFront(page.window) then
    backToTarget()
    return
  end
  trip = nil
  local w = host.window.active()
  if not (w and host.window.test(NOTEPAD, w)) then
    host.speech.output("Press this key in a Notepad window.", { interrupt = true })
    return
  end
  showBeside(w)
end)

host.log.info("Page demo loaded: " .. host.keys.describe(KEY) .. " in a Notepad window shows the page.")
host.speech.output("Page demo ready. Press " .. host.keys.describe(KEY) .. " in a Notepad window.", { interrupt = true })
