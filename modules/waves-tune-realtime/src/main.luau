-- Waves Tune Real-Time inside a DAW: the scale's note rules, the scale and its root.
--
-- Every coordinate is from the plug-in's own top-left at its 800x550 size, measured on Windows
-- in REAPER (2026-10-16) from a probe and from captures of the plug-in before and after clicks.
-- Only D's rule marker was seen in each state; the other notes' points are from the same grid
-- and still want a calibrator screenshot (Ctrl+Alt+Shift+S).

local O = host.require("com.platform.overlay")
local daw = host.require("com.platform.daw-hosts")

local ov = O.new("Waves Tune Real-Time")

-- The strip above the keys holds one marker per key. A click on it moves that note to its next
-- rule, in this order and round again; with Group Octaves on, every octave of the note follows.
-- Clicking a key itself only names it in the read-out under the keyboard.
local RULES = { "legal", "illegal", "pitch up", "pitch down", "bypass" }

-- C3's octave: 158 px from x 320, twelve markers evenly spaced. D's centre, 353, was measured.
local function markerX(semitone) return math.floor(320 + (semitone + 0.5) * 158 / 12 + 0.5) end
local MARKER_Y = 242

local GROUP_OCTAVES = { 690, 505 }
local MAGENTA = { 209, 52, 206 }

local function near(c, ref)
  return math.abs(c.r - ref[1]) + math.abs(c.g - ref[2]) + math.abs(c.b - ref[3]) < 60
end
local function white(c) return c.r > 200 and c.g > 200 and c.b > 200 end

-- An authored point on screen in whole pixels, or nil while the overlay has no origin.
local function screenPoint(o, x, y)
  local sx, sy = o:toScreen(x, y)
  if sx then return { math.floor(sx), math.floor(sy) } end
  return nil
end

-- The rule from the square around the marker, read whole: the plug-in draws the marker a pixel
-- or two off depending on where its window sits, so no single pixel can be trusted. The marker is
-- about 11x13: magenta for illegal and the two arrows, light grey (114) for bypass, none at all
-- for legal. Of the magenta ones, the minus is a white bar two rows high; an arrow's white spans
-- six rows, and its top tip lies on the side away from where it points.
-- Plus Group Octaves' pixel, which says whether a step reaches every octave.
local BOX_X, BOX_Y = 6, 7
local function readRule(o, x)
  local pts, offs = {}, {}
  for dy = -BOX_Y, BOX_Y do
    for dx = -BOX_X, BOX_X do
      local p = screenPoint(o, x + dx, MARKER_Y + dy)
      if not p then return nil end
      table.insert(pts, p)
      table.insert(offs, { dx = dx, dy = dy })
    end
  end
  local g = screenPoint(o, GROUP_OCTAVES[1], GROUP_OCTAVES[2])
  if not g then return nil end
  table.insert(pts, g)
  local c = host.screen.pixels(pts)
  if not c then return nil end

  local magenta, grey, whites = 0, 0, {}
  for i = 1, #pts - 1 do
    if white(c[i]) then
      table.insert(whites, offs[i])
    elseif near(c[i], MAGENTA) then
      magenta += 1
    elseif near(c[i], { 114, 113, 114 }) then
      grey += 1
    end
  end
  local rule
  if grey > 40 then
    rule = "bypass"
  elseif magenta < 40 then
    rule = "legal"
  else
    local top, bottom, sum = math.huge, -math.huge, 0
    for _, p in ipairs(whites) do
      top, bottom, sum = math.min(top, p.dy), math.max(bottom, p.dy), sum + p.dx
    end
    if #whites == 0 or bottom - top <= 2 then
      rule = "illegal"
    else
      local topSum, topN = 0, 0
      for _, p in ipairs(whites) do
        if p.dy == top then topSum, topN = topSum + p.dx, topN + 1 end
      end
      rule = topSum / topN < sum / #whites and "pitch up" or "pitch down"
    end
  end
  return near(c[#c], MAGENTA) and rule or rule .. ", this octave only"
end

-- Posted to the plug-in's window, the pointer left where it is: a moving pointer is something a
-- screen reader may speak over the announcement. Measured: Waves takes posted clicks, and four in
-- a row with no gap between them count as four.
local function clickAt(o, x, y)
  local p = screenPoint(o, x, y)
  if p then host.input.click(p[1], p[2], { window = o:hwnd(), via = "post", move = false }) end
end

-- nil is a reading that could not be made: nothing to say.
local function say(text) if text then host.speech.output(text, { interrupt = true }) end end

local NOTES = { "C", "C sharp", "D", "D sharp", "E", "F", "F sharp", "G", "G sharp", "A", "A sharp", "B" }
for i, name in ipairs(NOTES) do
  local x = markerX(i - 1)
  local note = ov:addStepper({
    label = name,
    text = function(o) return readRule(o, x) or "unknown" end,
    -- Right is one click; Left is the rest of the round. Only the new rule is said, not the note
    -- again: `false` tells the runtime to announce nothing, and this watch says the rule once it
    -- has changed, unless the focus moved or a later key came meanwhile, which says its own.
    onStep = function(dir, o)
      local was, mark = readRule(o, x), o:here()
      for _ = 1, dir > 0 and 1 or #RULES - 1 do clickAt(o, x, MARKER_Y) end
      local function sayIfHere(now)
        if o:stillHere(mark, { focus = true, keys = true }) then say(now) end
      end
      o:watch({ read = function() return readRule(o, x) end, was = was, within = 600, onDone = sayIfHere, onGiveUp = sayIfHere })
      return false
    end,
  })
  -- Space stays the DAW's, to play and hear a change: the runtime captures Space on every control
  -- not marked `editable`, and that flag is all it reads. addStepper has no option for it.
  note.editable = true
end

-- Every button here is clicked by a posted click (clickAt), the pointer left alone, and says
-- itself afterwards: once what it reads has changed, or after 900 ms with what it reads then.
-- `opts.also(o)` adds what else the press changed, after a comma.
local function button(label, at, read, opts)
  ov:addCustomButton({
    label = label,
    editable = true, -- Space stays the DAW's (see the notes above); Return activates
    opensMenu = opts and opts.opensMenu,
    text = read,
    onActivate = function(o)
      local was = read(o)
      if was == "dimmed" then return say(was) end
      clickAt(o, at[1], at[2])
      if opts and opts.opensMenu then return end
      local function sayNow(now)
        local also = now and opts and opts.also and opts.also(o)
        say(also and now .. ", " .. also or now)
      end
      o:watch({ read = read, was = was, within = 900, onDone = sayNow, onGiveUp = sayNow })
    end,
  })
end

-- A button's lit state, from a pixel clear of its lettering: magenta is lit, black is not, and
-- grey is dimmed — a button that does nothing now, as Sharp with B as the root (no B sharp).
local function lit(p, on, off)
  return function(o)
    local at = screenPoint(o, p[1], p[2])
    if not at then return nil end
    local c = host.screen.pixel(at[1], at[2])
    if not c then return nil end
    if near(c, MAGENTA) then return on end
    if near(c, { 0, 0, 0 }) then return off end
    if near(c, { 101, 101, 102 }) then return "dimmed" end
    return nil
  end
end

button("Group Octaves", GROUP_OCTAVES, lit(GROUP_OCTAVES, "on", "off"))

-- The scale's name in its box. A click on the name steps to the next scale, through 44 of them
-- and round again; the arrow at the box's right opens all of them as a native popup menu
-- (#32768), the current one ticked, which a screen reader reads by itself.
local SCALE_NAME = { 360, 463, 530, 488 }
local function scaleName(o)
  local r = o:toScreenRect(SCALE_NAME, { whole = true })
  if not r then return nil end
  local reading = host.ocr.recognize(r, { lang = "en" })
  return reading.status == "text" and reading.text or nil
end
button("Scale", { 540, 475 }, scaleName, { opensMenu = true })

-- The root: flat, sharp and A to G, the chosen one lit as Group Octaves is.
local FLAT, SHARP = { 580, 467 }, { 616, 467 }
local LETTERS = { "A", "B", "C", "D", "E", "F", "G" }
local function letterAt(i) return { 330 + (i - 1) * 49 - 16, 515 } end

-- The root as it is lit now, "B flat", in one read; nil with no letter lit.
local function rootName(o)
  local pts = {}
  for i, p in ipairs({ FLAT, SHARP }) do pts[i] = screenPoint(o, p[1], p[2]) end
  for i = 1, #LETTERS do pts[2 + i] = screenPoint(o, letterAt(i)[1], letterAt(i)[2]) end
  if #pts < 2 + #LETTERS then return nil end
  local c = host.screen.pixels(pts)
  if not c then return nil end
  for i, letter in ipairs(LETTERS) do
    if near(c[2 + i], MAGENTA) then
      return "root " .. letter .. (near(c[1], MAGENTA) and " flat" or near(c[2], MAGENTA) and " sharp" or "")
    end
  end
  return nil
end

button("Flat", { 590, 475 }, lit(FLAT, "on", "off"), { also = rootName })
button("Sharp", { 626, 475 }, lit(SHARP, "on", "off"), { also = rootName })
for i, n in ipairs(LETTERS) do
  local p = letterAt(i)
  button("Root " .. n, { p[1] + 16, 515 }, lit(p, "selected", "not selected"))
end

-- Says the scale's name afterwards: the edited mark, " *", goes from it.
button("Reset Scale", { 720, 475 }, scaleName)

-- "WAVES TUNE REAL-TIME" under the pitch meter, read at x 442 to 597, y 212 to 221.
local WORDMARK = { 430, 205, 610, 228 }

ov:attachEmbedded({
  hosts = daw.all,
  -- Every Waves plug-in is drawn in this class, its WaveShell version in the name.
  control = { windows = "^WaveShell.* Waves OpenGL Window Class$" },
  identify = function(ctrl)
    local ox, oy = math.floor(ctrl.client.x), math.floor(ctrl.client.y)
    local r = host.ocr.recognize({ ox + WORDMARK[1], oy + WORDMARK[2], ox + WORDMARK[3], oy + WORDMARK[4] }, { lang = "en" })
    if r.status ~= "text" then return nil end
    return string.find(string.lower(r.text), "tune real", 1, true) ~= nil
  end,
}, { menus = { O.menuTests.nativePopup } })
