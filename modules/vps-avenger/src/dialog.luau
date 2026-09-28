-- Avenger's own warning box: Initialize's "Are you sure?", and Save's overwrite question.
--
-- The box is drawn INSIDE Avenger's window, by the project's account (geometry.luau): no window
-- of its own, no accessibility element, no notification. So nothing tells the overlay it is up
-- but what it looks like, and it gets an overlay of its own on the module's slot at the dialog
-- layer — Kontakt's "Content Missing" pattern (docs/api/overlay.md, O.layer): while its gate sees
-- the box, it takes the keyboard from the header overlay, and it hands it back when the box goes.
-- Its three stops are the box's own text, read, and its two buttons.
--
-- WHEN IT IS LOOKED FOR. Only after this module chose Initialize or Save in Avenger's MENU and
-- saw the menu take the choice — "armed" — and only until the box has come and gone, or
-- DIALOG_READS_MAX reads have not seen it. A read of a region this size is Vision's hundred-odd
-- milliseconds on a Mac, and polling one every half second for as long as Avenger is in front
-- would be a recogniser running for nobody. A count, not a clock: the reads the gate's poll makes,
-- one out at a time. The poll itself runs only while a box is expected (`D.expecting`, the box
-- overlay's `pollWhen`, main.luau): a context match twice a second in every DAW window for the
-- module's lifetime would read the focus chain for nobody as well — Logic's accessibility is the
-- slow one — and add one "could not tell yet" to the runtime's count at every poll while the
-- header is being read. What that leaves out, said so nobody reads more into it: a box opened some
-- other way — a sighted helper's click on MENU — is not seen, and the header overlay stays up over
-- it.
--
-- WHEN IT HAS GONE. Two readings in a row that are text without the pair — one could be a frame
-- drawn mid-redraw, or a word misread — and never a reading of nothing at all (none, blank,
-- failed), which says nothing about the box: those are counted apart, up to DIALOG_READS_MAX. A box
-- given up for gone while it is up would hand the keyboard back to the header over a modal box.
--
-- WHERE IT IS CLICKED. Where YES and NO are READ, not where the table puts them. The project's
-- figures are Initialize's box only; it never opened Save's, so the one geometry that would hold
-- for both is the words themselves. The table's pair is compared with what was read, in the log
-- and in the calibration picture, which is how a tester's run settles whether the two agree.
--
-- WHETHER IT IS A WINDOW AFTER ALL is unmeasured. If JUCE draws it as a window of its own, the
-- header overlay's menu test (newWindow) can see it as a new window of the process while the press
-- of MENU is still being compared against, the header overlay then holds its place with no keys,
-- and the buttons' clicks would be refused as covered — each said. TODO.md, "VPS Avenger".

local G = host.include("src/geometry.luau")
local H = host.include("src/header.luau")

local D = {}

-- Reads the box is looked for in, after the choice that should have opened it.
local DIALOG_READS_MAX = 8
-- Readings in a row, text without YES and NO, that say a box that was up has gone.
local GONE_READS = 2

-- The box being looked for or shown: { id = window, label = the choice, reads = n, fails = n,
-- misses = n, seen = box, reading = token }, or nil.
local armed = nil

local function forLog(text)
  local s = string.gsub(text or "", "[\r\n]+", " ")
  if #s > 80 then s = string.gsub(string.sub(s, 1, 80), "[\192-\255][\128-\191]*$", "") .. "…" end
  return s
end

local function centre(w) return w.x + w.w / 2, w.y + w.h / 2 end

-- YES and NO on one row, YES left of NO, among the reading's words; and the rows of text above
-- them. nil when the pair is not there. A zero read for an O and a five for an S ("N0", "YE5")
-- are the two slips a recogniser makes in these words and are taken for them. A word in a box the
-- recogniser shared out by length (`approx`, docs/api/ocr.md) is not taken: it would be a click
-- point anywhere along its line.
local function parseBox(r)
  local yes, no
  for _, w in ipairs(r.words or {}) do
    if not w.approx then
      local t = string.upper((string.gsub(w.text or "", "[^%w]", "")))
      t = string.gsub(string.gsub(t, "0", "O"), "5", "S")
      if t == "YES" and not yes then yes = w end
      if t == "NO" and not no then no = w end
    end
  end
  if not (yes and no) then return nil end
  local yx, yy = centre(yes)
  local nx, ny = centre(no)
  if nx <= yx or math.abs(ny - yy) > math.max(yes.h, no.h) / 2 + 2 then return nil end
  local rows = {}
  for _, line in ipairs(r.lines or {}) do
    if line.y + line.h <= math.min(yes.y, no.y) + 1 then
      local t = string.gsub(line.text or "", "^%s+", "")
      t = string.gsub(t, "%s+$", "")
      if t ~= "" then rows[#rows + 1] = t end
    end
  end
  return { yes = { yx, yy }, no = { nx, ny }, text = table.concat(rows, ", ") }
end

-- In a calibrating run, the box once per choice and zoom: crosshairs on YES and NO where they
-- were read (marks 1 and 2), and where the table puts them (3 and 4).
local shots = {}
local function boxShot(origin, k, rect, box, label)
  if not host.calibrating then return end
  local zoom = H.zoom(origin)
  local name = string.format("dialog-%s-%dpct", (string.gsub(label, "[^%w]+", "-")), zoom or 0)
  if shots[name] then return end
  shots[name] = true
  local marks = {
    { x = math.floor(box.yes[1] + 0.5), y = math.floor(box.yes[2] + 0.5) },
    { x = math.floor(box.no[1] + 0.5), y = math.floor(box.no[2] + 0.5) },
  }
  for _, p in ipairs({ G.YES, G.NO }) do
    local x, y = G.place(origin, k, p[1], p[2])
    marks[#marks + 1] = { x = x, y = y }
  end
  local path = host.path("calibration/" .. name .. ".png")
  -- Guarded, as the header's picture is (header.luau).
  local fine, ok, why = pcall(host.screen.saveMarked, path, { region = rect, marks = marks })
  if not fine then ok, why = false, ok end
  host.log.info(string.format("[avenger] [calibrate] warning box picture, (%d,%d)-(%d,%d): marks 1 and 2 "
    .. "YES and NO where they were read, 3 and 4 where the table puts them -> %s (%s)",
    rect[1], rect[2], rect[3], rect[4], path, ok and "true" or ("false: " .. tostring(why))))
end

local function read(origin, a)
  local k = H.factor(origin)
  local rect = k and G.placeRect(origin, k, G.DIALOG_BAND)
  if not rect then
    -- No factor now (the header is being read) or no room: no read, and counted, so a box that
    -- can never be looked for is not waited for for ever.
    a.reads += 1
    if a.reads >= DIALOG_READS_MAX then
      armed = nil
      if a.seen then host.window.recheck() end
      host.log.info(string.format("[avenger] '%s': the warning box could not be looked for (%s) — "
        .. "not looked for any more", a.label, k and "no room in the plug-in" or "no factor"))
    end
    return
  end
  local token = {}
  a.reading = token
  local client = { x = origin.client.x, y = origin.client.y }
  local ok, err = pcall(host.ocr.read, rect, { lang = H.lang(), key = "vps-avenger dialog" }, function(r)
    if armed ~= a or a.reading ~= token then return end
    a.reading = nil
    if r.status == "stale" then return end
    local box = r.status == "text" and parseBox(r) or nil
    if box then
      a.misses = 0
      local first = a.seen == nil
      a.seen = {
        yes = { math.floor(box.yes[1] - client.x + 0.5), math.floor(box.yes[2] - client.y + 0.5) },
        no = { math.floor(box.no[1] - client.x + 0.5), math.floor(box.no[2] - client.y + 0.5) },
        text = box.text,
      }
      if first then
        -- Where the table puts the pair, against where it was read: the measurement a tester's run
        -- brings back for Initialize's box, and the first figures at all for Save's.
        local tx, ty = G.place(origin, k, G.YES[1], G.YES[2])
        local ux, uy = G.place(origin, k, G.NO[1], G.NO[2])
        host.log.info(string.format("[avenger] '%s': Avenger's warning box is up: \"%s\"; YES read at "
          .. "(%.0f,%.0f), NO at (%.0f,%.0f) — (%+.0f,%+.0f) and (%+.0f,%+.0f) from where the table puts them",
          a.label, forLog(box.text), box.yes[1], box.yes[2], box.no[1], box.no[2],
          box.yes[1] - tx, box.yes[2] - ty, box.no[1] - ux, box.no[2] - uy))
        boxShot(origin, k, rect, box, a.label)
        host.window.recheck()
      end
      return
    end
    -- A read that read nothing, or could not be made, says nothing about the box — not that it
    -- has gone, not that it never came — and is counted apart, so a recogniser that has stopped
    -- answering is not asked every half second for as long as the box overlay is up.
    if r.status ~= "text" then
      a.fails += 1
      if a.fails >= DIALOG_READS_MAX then
        armed = nil
        host.log.info(string.format("[avenger] '%s': the warning box could not be read %d times (%s%s) — "
          .. "not looked for any more", a.label, a.fails, tostring(r.status),
          r.error and (": " .. tostring(r.error)) or ""))
        if a.seen then host.window.recheck() end
      end
      return
    end
    if a.seen then
      -- It was up; text without YES and NO. Twice in a row, it is gone: answered.
      a.misses = (a.misses or 0) + 1
      if a.misses < GONE_READS then return end
      armed = nil
      host.log.info(string.format("[avenger] '%s': the warning box has gone", a.label))
      host.window.recheck()
      return
    end
    a.reads += 1
    if a.reads >= DIALOG_READS_MAX then
      armed = nil
      host.log.info(string.format("[avenger] '%s': no warning box in %d reads after it was chosen — not "
        .. "looked for any more; last read: \"%s\"", a.label, a.reads, forLog(r.text)))
    end
  end)
  if not ok then
    a.reading = nil
    a.reads += 1
    host.log.info(string.format("[avenger] '%s': the warning box could not be read: %s", a.label, tostring(err)))
  end
end

-- `label` (Initialize or Save) was chosen in the MENU of window `id` — the one it was pressed in,
-- which the overlay may already have left for a box that is a window of its own: its box is
-- looked for from now on.
function D.arm(id, label)
  if id == nil then return end
  armed = { id = id, label = label, reads = 0, fails = 0, misses = 0 }
  host.log.info(string.format("[avenger] '%s' chosen: Avenger's warning box is looked for, up to %d reads",
    label, DIALOG_READS_MAX))
end

-- Whether a box is being looked for or shown: the box overlay's `pollWhen`, so that its poll runs
-- only then.
function D.expecting()
  return armed ~= nil
end

-- The box overlay's gate (O:gate): true while the box is seen over `origin`'s window. Starts a
-- read when none is out — asked at every evaluation, which the box overlay's poll makes one every
-- half second while a box is expected.
function D.gate(origin)
  local a = armed
  if not (a and origin and origin.id == a.id and origin.client) then return false end
  if not a.reading then read(origin, a) end
  return armed == a and a.seen ~= nil
end

-- The box overlay's stops: the box's text, then YES and NO — each clicked where it was read, in
-- the plug-in's own pixels (`rawOrigin`: neither framed nor scaled, because a read word's place
-- is on screen already).
function D.build(ov)
  ov:addStaticText({
    label = "Avenger asks",
    text = function()
      local s = armed and armed.seen
      if not s then return nil end
      return s.text ~= "" and s.text or "its text cannot be read"
    end,
  })
  ov:addHotspotButton({ label = "Yes", rawOrigin = true, at = function()
    local s = armed and armed.seen
    return s and { s.yes[1], s.yes[2] } or nil
  end })
  ov:addHotspotButton({ label = "No", rawOrigin = true, at = function()
    local s = armed and armed.seen
    return s and { s.no[1], s.no[2] } or nil
  end })
  return ov
end

return D
