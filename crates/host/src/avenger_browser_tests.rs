//! VPS Avenger's expansion browser from the page (the plan's slice 9:
//! `modules/vps-avenger/src/actions/browser.luau`, `src/read/browser.luau`, the reads of
//! `src/read/expansion.luau` and `src/read/open.luau`, the database's lookups in `src/presets.luau`),
//! loaded for real against the scripted host of `overlay_avenger_tests.rs`, with a model of
//! Avenger's browser (`S.browser`, below) that the painted picture and the text reads are made
//! from, and that the module's clicks, wheel and track clicks move.
//!
//! What the plan says is pinned here. The view is chosen in the list its field opens with the left
//! button and the caption read again; nothing is chosen where an expansion is open, and Close is not
//! clicked where the caption reads a view (there it would open the view list). FAV is read before
//! and after its click, and with FAV on every row's star. An expansion on the grid is opened by the
//! middle of its words — never the tool's point for its index in the roster — and one off the grid
//! is looked for a wheel notch at a time, each a change wait from the picture last read on the
//! list's top and bottom bands with a still spell, turning at the end and saying a name Avenger does
//! not list; a wheel that moves nothing hands the list to its track for the session; the pointer's
//! hover in the middle is not the list moving. The tiles are named by their column, a name on two
//! lines is one, a poor read is a name only where it stands out. The expansion opened is the one its
//! columns list. Categories and rows are found and clicked where they are read; a preset of another
//! category has its category clicked first; Select is a click and Load a double-click, the name
//! watched after them and never the database's posted for it; a reversed wheel is turned round. The
//! star is clicked at its row's height and read after a change wait. All clicks nothing; Go to
//! Category is the database's with no trip. Avenger's own list is read from its top down, kept for
//! the session and shown in place of the roster, and dropped when the grid names one it lacks. A
//! give-up mid-search moves and clicks nothing more. No picture is held at the end of any of it.
//!
//! And what the review of 2026-10-18 found: a wheel is turned round only in an order known, never
//! the roster's, and only by two steps running; the grid is placed by the view read now, never one
//! remembered; its rows are told by its tiles, not by bins from the area's top; Avenger's list is
//! gathered by where most names were read before and each place's most-read name, so one misread
//! neither doubles it nor is kept; and a name read at neither end is "not in VPS Avenger's list"
//! only where the list moved and an order known puts it away from the places read.

use super::overlay_avenger_tests::run_mac;

/// The preset database the scenarios use, the page on Avenger with a model of its browser, and the
/// model.
const BROWSER: &str = r##"
local S = T.S
local EX = T.source("modules/vps-avenger/src/tables/expansion.luau")(T.host)
T.EX = EX

-- ── The catalog ──────────────────────────────────────────────────────────────
-- Twenty expansions in the data module's format, NOT the real data: "Melodic Techno 2" to "5" alike
-- but for their number, for the similarity margin; "Cinematic Pads" with twelve categories and a
-- Strings category of thirty presets, and "X4 Shared Name" in two of its categories (Keys and Pads).
-- Each other expansion has Arp, Bass, Leads and Pads, two presets each, named "<code> <category>
-- Line <n>", the code "X" and the expansion's place.
do
  local names = { "Acid Breaks", "Ambient Worlds", "Bass Monsters", "Cinematic Pads", "Dark Techno", "Deep House",
    "Electro Pop", "Future Bass", "Hardstyle Leads", "Melodic Techno 2", "Melodic Techno 3", "Melodic Techno 4",
    "Melodic Techno 5", "Organic Textures", "Psytrance", "Retro Synths", "Synthwave", "Trap Essentials", "Vocal Chops",
    "World Percussion" }
  T.BROWSER_NAMES = names
  local files = {}
  local index = { format = 1, exported = "2026-10-18 10:00", factoryStandard = { "Acid Breaks", "Bass Monsters",
    "Cinematic Pads" }, expansions = {} }
  for i, n in ipairs(names) do
    local cats = { { "Arp", 2 }, { "Bass", 2 }, { "Leads", 2 }, { "Pads", 2 } }
    if n == "Cinematic Pads" then
      cats = { { "Arp", 2 }, { "Bass", 2 }, { "Bells", 2 }, { "Chords", 2 }, { "Drums", 2 }, { "Effects", 2 }, { "Keys", 3 },
        { "Leads", 2 }, { "Mallets", 2 }, { "Pads", 3 }, { "Plucks", 2 }, { "Strings", 30 } }
    end
    local presets, categories, start = {}, {}, 1
    for _, c in ipairs(cats) do
      for j = 1, c[2] do
        local pname = string.format("X%d %s Line %02d", i, c[1], j)
        if n == "Cinematic Pads" and j == 3 and (c[1] == "Keys" or c[1] == "Pads") then pname = "X4 Shared Name" end
        presets[#presets + 1] = { name = pname, osc = { "Osc" }, macros = { { "", "", "", "", "" } } }
      end
      categories[#categories + 1] = { name = c[1], start = start, count = c[2] }
      start += c[2]
    end
    local file = "data/expansions/e" .. i .. ".json"
    local pnames = {}
    for k, p in ipairs(presets) do pnames[k] = p.name end
    index.expansions[i] = { name = n, file = file, presets = pnames }
    files[file] = T.jsonEncode({ format = 1, name = n, categories = categories, presets = presets })
  end
  files["data/index.json"] = T.jsonEncode(index)
  T.BROWSER_FIXTURE = files
end

-- Expansion `name`'s lists from the catalog: its categories, and each category's presets.
function T.listsOf(name)
  local idx = T.jsonDecode(T.BROWSER_FIXTURE["data/index.json"])
  for _, x in ipairs(idx.expansions) do
    if x.name == name then
      local f = T.jsonDecode(T.BROWSER_FIXTURE[x.file])
      local cats, presets, all = {}, {}, {}
      for _, c in ipairs(f.categories) do
        cats[#cats + 1] = c.name
        presets[c.name] = {}
        for i = c.start, c.start + c.count - 1 do table.insert(presets[c.name], f.presets[i].name) end
      end
      for i, p in ipairs(f.presets) do all[i] = p.name end
      return cats, presets, all
    end
  end
  error("no expansion " .. tostring(name))
end

-- ── The model ────────────────────────────────────────────────────────────────
-- S.browser: what Avenger's browser shows — { mode = "grid" | "open", grid = …, open = … }:
--   grid  { view, names (Avenger's order), firstRow = the first row shown (from 0), perNotch = rows a
--         notch moves (1), reversed = the wheel moves it the other way, wheelDead, trackDead,
--         twoLines = { [name] = { line 1, line 2 } }, misread = { [name] = as read }, opens = { [name] =
--         an `open` it opens }, caption = what the caption reads (default the view; false: nothing),
--         cutTop = the first row's words on the area's top edge, hoverOnScroll = a notch paints a
--         hover in the middle band and moves nothing, nudge = { [name] = table points its tile's
--         words lie lower than its row's } }
--   open  T.openOf's: { name, categories, presets = { [category] = names }, category (the one whose
--         presets show), firstCategory, firstRow (from 0), fav, stars = { [preset] = true }, blueStars
--         = { [preset] = true } (lit the tool's blue, not bright), perNotch, reversed, wheelDead,
--         stuck = Close does nothing, categoryDead = a category click selects nothing, categoryLate =
--         ms a category click paints a hover first and selects only after, showsCategory = the
--         category shown whatever is clicked, misread = { [category] = as read } }
-- S.browserLater: Avenger answering later — { at, fn }, each fn run on the first turn at or after
-- `at`, before the host's answers of that turn.
-- Painted under the popups (T.paintAvenger): each tile and row a small box of its own colour beside
-- its words, FAV and the stars lit; read as text (T.browserReads, kept with the picture): the caption,
-- the area (also the page key's "expansion area"), the two columns. Every click, notch and track click
-- the module makes moves it and paints it again.
function T.openOf(name, extra)
  local cats, presets = T.listsOf(name)
  local o = { name = name, categories = cats, presets = presets, category = cats[1], firstCategory = 0, firstRow = 0,
    fav = false, stars = {}, perNotch = 1 }
  for k, v in pairs(extra or {}) do o[k] = v end
  return o
end

-- The grid's shape in view `view`: its columns (EX.columns), the rows shown, and the table y of
-- shown row r's middle (from 0), and how far below it the words are.
function T.gridShape(view)
  local cols = EX.columns(view)
  if view == "List" then return cols, 5, function(r) return EX.LIST_Y[1] + 36 * r end, 0 end
  if view == "8 per Row" then return cols, 8, function(r) return EX.GRID8_Y0 + EX.GRID8_Y_STEP * r end, 5 end
  local n = #cols.xs
  return cols, n, function(r) return EX.AREA[2] + (r + 0.5) * (EX.AREA[4] - EX.AREA[2]) / n end, 5
end

-- Text `text` centred at table point (tx, ty), at most `maxW` across: its words side by side, each
-- letter 2.5 points across or narrower, 6 tall, on screen from Avenger's corner (cx, cy) at k.
local function wordsAt(text, tx, ty, maxW, cx, cy, k)
  local cw = math.min(2.5, maxW / math.max(1, #text))
  local x = tx - #text * cw / 2
  local out = {}
  for w in string.gmatch(text, "%S+") do
    out[#out + 1] = { text = w, x = cx + k * x, y = cy + k * (ty - 3), w = k * #w * cw, h = k * 6 }
    x = x + (#w + 1) * cw
  end
  return out
end

local function lineOf(words)
  local x1, y1, x2, y2 = math.huge, math.huge, -math.huge, -math.huge
  local texts = {}
  for _, w in ipairs(words) do
    texts[#texts + 1] = w.text
    x1, y1 = math.min(x1, w.x), math.min(y1, w.y)
    x2, y2 = math.max(x2, w.x + w.w), math.max(y2, w.y + w.h)
  end
  return { text = table.concat(texts, " "), x = x1, y = y1, w = x2 - x1, h = y2 - y1, words = words }
end

local function readingOf(lines)
  if #lines == 0 then return { status = "none", text = "", words = {}, lines = {} } end
  local all, texts = {}, {}
  for _, l in ipairs(lines) do
    texts[#texts + 1] = l.text
    for _, w in ipairs(l.words) do all[#all + 1] = w end
  end
  return { status = "text", text = table.concat(texts, "\n"), words = all, lines = lines }
end

-- The tiles shown: { name, index (in the grid's names), tx, ty (the tile's middle), text = { lines } },
-- and the rows of open's lists shown.
function T.shownTiles()
  local g = S.browser.grid
  local cols, rows, rowY, dy = T.gridShape(g.view)
  local out = {}
  for r = 0, rows - 1 do
    for c = 0, #cols.xs - 1 do
      local i = (g.firstRow + r) * #cols.xs + c + 1
      local name = g.names[i]
      if name then
        local ty = rowY(r) + dy + (g.nudge and g.nudge[name] or 0)
        if g.cutTop and r == 0 then ty = EX.AREA[2] + 2 end
        local lines = g.twoLines and g.twoLines[name] or { g.misread and g.misread[name] or name }
        out[#out + 1] = { name = name, index = i, tx = cols.xs[c + 1], ty = ty, row = r, lines = lines,
          maxW = 2 * cols.half * 0.85 }
      end
    end
  end
  return out
end
function T.shownCategories()
  local o = S.browser.open
  local out = {}
  for i = 0, EX.CATEGORY_VISIBLE_ROWS - 1 do
    local name = o.categories[o.firstCategory + i + 1]
    if name then out[#out + 1] = { name = name, index = o.firstCategory + i + 1, ty = EX.CATEGORY_Y0 + i * EX.CATEGORY_Y_STEP } end
  end
  return out
end
function T.shownPresets()
  local o = S.browser.open
  local list = o.presets[o.showsCategory or o.category] or {}
  local out = {}
  for j = 0, EX.PRESET_VISIBLE_ROWS - 1 do
    local name = list[o.firstRow + j + 1]
    if name then out[#out + 1] = { name = name, index = o.firstRow + j + 1, ty = EX.PRESET_Y0 + j * EX.PRESET_Y_STEP } end
  end
  return out
end

local function colour(i) return { 50 + (i * 37) % 170, 50 + (i * 71) % 170, 50 + (i * 13) % 170 } end

function T.browserShapes(shapes, cx, cy, k)
  local b = S.browser
  if not b then return end
  local function rect(x1, y1, x2, y2, c)
    shapes[#shapes + 1] = { kind = "rect", x1 = cx + k * x1, y1 = cy + k * y1, x2 = cx + k * x2, y2 = cy + k * y2, c = c }
  end
  if b.mode == "grid" then
    for _, t in ipairs(T.shownTiles()) do rect(t.tx - 6, t.ty - 9, t.tx + 6, t.ty - 5, colour(t.index)) end
  else
    local o = b.open
    for _, c in ipairs(T.shownCategories()) do rect(18, c.ty - 3, 22, c.ty + 3, colour(c.index)) end
    for _, p in ipairs(T.shownPresets()) do
      rect(74, p.ty - 3, 78, p.ty + 3, colour(p.index + 100))
      if o.fav and o.stars[p.name] then rect(215.5, p.ty - 4.5, 224.5, p.ty + 4.5, { 235, 235, 235 }) end
      if o.fav and o.blueStars and o.blueStars[p.name] then rect(215.5, p.ty - 4.5, 224.5, p.ty + 4.5, { 60, 90, 200 }) end
    end
    if o.fav then rect(212 - 12.5, 45, 212 + 12.5, 55, { 220, 220, 220 }) end
  end
  if b.hover then rect(100, 130, 135, 150, { 90, 90, 130 }) end
end

function T.browserReads(cx, cy, k)
  local b = S.browser
  if not b then return nil end
  local out = {}
  if b.mode == "grid" then
    local g = b.grid
    local caption = g.caption
    if caption == nil then caption = g.view end
    out["expansion view"] = caption and readingOf({ lineOf(wordsAt(caption, 48, 51, 70, cx, cy, k)) }) or readingOf({})
    local byRow = {}
    for _, t in ipairs(T.shownTiles()) do
      for n, line in ipairs(t.lines) do
        local key = t.row * 2 + n
        byRow[key] = byRow[key] or {}
        local y = #t.lines == 2 and (t.ty + (n == 1 and -3.5 or 3.5)) or t.ty
        for _, w in ipairs(wordsAt(line, t.tx, y, t.maxW, cx, cy, k)) do table.insert(byRow[key], w) end
      end
    end
    local keys = {}
    for key in pairs(byRow) do keys[#keys + 1] = key end
    table.sort(keys)
    local lines = {}
    for _, key in ipairs(keys) do lines[#lines + 1] = lineOf(byRow[key]) end
    out["browser area"] = readingOf(lines)
    out["categories"], out["presets"] = readingOf({}), readingOf({})
  else
    out["expansion view"] = readingOf({})
    local cl, pl = {}, {}
    for _, c in ipairs(T.shownCategories()) do
      local text = b.open.misread and b.open.misread[c.name] or c.name
      cl[#cl + 1] = lineOf(wordsAt(text, EX.CATEGORY_X, c.ty, 50, cx, cy, k))
    end
    for _, p in ipairs(T.shownPresets()) do pl[#pl + 1] = lineOf(wordsAt(p.name, EX.PRESET_X, p.ty, 130, cx, cy, k)) end
    out["categories"], out["presets"] = readingOf(cl), readingOf(pl)
    local all = {}
    for _, l in ipairs(cl) do all[#all + 1] = l end
    for _, l in ipairs(pl) do all[#all + 1] = l end
    out["browser area"] = readingOf(all)
  end
  out["expansion area"] = out["browser area"]
  return out
end

-- Moves list `list` ("grid", "categories", "presets") by `rows` (down positive), as far as it goes,
-- and paints Avenger again where it moved: whether it moved.
function T.browserMove(list, rows)
  local b = S.browser
  local function clamp(v, total, shown) return math.max(0, math.min(v, math.max(0, total - shown))) end
  local from, to
  if list == "grid" then
    local g = b.grid
    local cols, shown = T.gridShape(g.view)
    from = g.firstRow
    to = clamp(from + rows, math.ceil(#g.names / #cols.xs), shown)
    g.firstRow = to
  elseif list == "categories" then
    local o = b.open
    from = o.firstCategory
    to = clamp(from + rows, #o.categories, EX.CATEGORY_VISIBLE_ROWS)
    o.firstCategory = to
  else
    local o = b.open
    from = o.firstRow
    to = clamp(from + rows, #(o.presets[o.showsCategory or o.category] or {}), EX.PRESET_VISIBLE_ROWS)
    o.firstRow = to
  end
  if to == from then return false end
  T.repaint(S.painted.spec)
  return true
end

-- The wheel and the clicks, as Avenger answers them.
S.onScroll = function(_, notches, x)
  local b = S.browser
  if not b then return end
  local tx = (x - S.painted.cx) / S.painted.k
  local m = b.mode == "grid" and b.grid or b.open
  local list = b.mode == "grid" and "grid" or (tx < 71 and "categories" or "presets")
  if m.hoverOnScroll then
    b.hover = not b.hover
    T.repaint(S.painted.spec)
    return
  end
  if m.wheelDead then return end
  local rows = (notches < 0 and 1 or -1) * (m.perNotch or 1)
  if m.reversed then rows = -rows end
  T.browserMove(list, rows)
end
S.browserClick = function(x, y)
  local b = S.browser
  if not b then return end
  local cx, cy, k = S.painted.cx, S.painted.cy, S.painted.k
  local tx, ty = (x - cx) / k, (y - cy) / k
  local pop = S.painted.spec.popup
  if pop and tx >= pop[1] and tx < pop[3] and ty >= pop[2] and ty < pop[4] then return end
  b.clicked = b.clicked or {}
  local function near(px, py, r) return math.abs(tx - px) <= r and math.abs(ty - py) <= r end
  local function note(what) b.clicked[#b.clicked + 1] = what end
  if b.mode == "grid" then
    local g = b.grid
    if math.abs(tx - 228) <= 2 then
      note("track " .. (ty > 140 and "down" or "up"))
      if g.trackDead then return end
      local _, shown = T.gridShape(g.view)
      T.browserMove("grid", (ty > 140 and 1 or -1) * math.max(1, shown - 1))
      return
    end
    if near(15, 50, 6) then return note("view") end
    for _, t in ipairs(T.shownTiles()) do
      local half = #t.lines == 2 and 7 or 4
      if math.abs(tx - t.tx) <= t.maxW / 2 and math.abs(ty - t.ty) <= half then
        note(t.name)
        if g.opens and g.opens[t.name] then
          b.mode, b.open = "open", g.opens[t.name]
          T.repaint(S.painted.spec)
        end
        return
      end
    end
    return
  end
  local o = b.open
  if near(15, 50, 6) then
    note("close")
    if not o.stuck then
      b.mode = "grid"
      T.repaint(S.painted.spec)
    end
    return
  end
  if math.abs(tx - 212) <= 12.5 and math.abs(ty - 50) <= 5 then
    note("fav")
    o.fav = not o.fav
    T.repaint(S.painted.spec)
    return
  end
  if math.abs(tx - 12) <= 2 then
    note("category track")
    T.browserMove("categories", (ty > 140 and 1 or -1) * (EX.CATEGORY_VISIBLE_ROWS - 1))
    return
  end
  if math.abs(tx - 228) <= 2 then
    note("preset track")
    T.browserMove("presets", (ty > 140 and 1 or -1) * (EX.PRESET_VISIBLE_ROWS - 1))
    return
  end
  if math.abs(tx - 220) <= 5 then
    for _, p in ipairs(T.shownPresets()) do
      if math.abs(ty - p.ty) <= 4.5 then
        note("star " .. p.name)
        if o.fav and not o.starDead then
          o.stars[p.name] = not o.stars[p.name]
          T.repaint(S.painted.spec)
        end
        return
      end
    end
    return
  end
  if tx >= 16 and tx < 70 then
    for _, c in ipairs(T.shownCategories()) do
      if math.abs(ty - c.ty) <= 5 then
        note("category " .. c.name)
        if o.categoryDead then return end
        if o.categoryLate then
          T.repaint(S.painted.spec)
          S.browserLater[#S.browserLater + 1] = { at = S.now + o.categoryLate, fn = function()
            o.category, o.firstRow = c.name, 0
            T.repaint(S.painted.spec)
          end }
          return
        end
        o.category, o.firstRow = c.name, 0
        T.repaint(S.painted.spec)
        return
      end
    end
    return
  end
  if tx >= 72 and tx < 212 then
    for _, p in ipairs(T.shownPresets()) do
      if math.abs(ty - p.ty) <= 4 then
        note("preset " .. p.name)
        return
      end
    end
  end
end

S.browserLater = {}
do
  local runDue = T.runDue
  T.runDue = function()
    local i = 1
    while i <= #S.browserLater do
      local l = S.browserLater[i]
      if S.now >= l.at then
        table.remove(S.browserLater, i)
        l.fn()
      else
        i += 1
      end
    end
    runDue()
  end
end

-- What every read reads: the browser's regions from the model kept with the picture read (the
-- snapshot's, or the screen's), the name's region as S.nameText, the rest as `words` (T.words).
function T.browserWords(words)
  T.words(words or {})
  local base = S.ocr
  S.ocr = function(r, name, snap)
    local pic = snap and snap.picture or S.picture
    local reads = pic and pic.browser
    if name and reads and reads[name] ~= nil then return reads[name] end
    if name == nil and S.nameText and T.region(r) == "152,95,440,119" then return S.nameText end
    return base(r, name, snap)
  end
end

-- The page open on Avenger at 80 % in REAPER's floating window, the browser as `model` says, with
-- the catalog above installed (`opts.files` instead; `opts.noDatabase`: none) and the roster setting
-- `opts.roster` ("Complete" unless it says). The header overlay, the box overlay, the window.
function T.browserPage(model, opts)
  opts = opts or {}
  if not opts.noDatabase then T.presets(opts.files or T.BROWSER_FIXTURE) end
  S.settings.roster = opts.roster or "Complete"
  S.browser = model
  local header, box, W = T.avengerAt80(opts.name or "Init Preset")
  T.pageReady()
  T.paintAvenger(98, 84, 1.6, opts.spec or {})
  T.browserWords(opts.words)
  T.openPage()
  T.event()
  return header, box, W
end
-- A grid model in view `view` of `names` (default the catalog's twenty), with `extra` over it.
function T.grid(view, names, extra)
  local g = { view = view, names = names or T.BROWSER_NAMES, firstRow = 0, perNotch = 1 }
  for key, v in pairs(extra or {}) do g[key] = v end
  return { mode = "grid", grid = g }
end
function T.settle(n) T.tick(n or 3) end
function T.st() return T.lastPost("state").state end
function T.err() return T.lastPost("error").text end
-- The index of `name` in the list the page was last posted.
function T.indexIn(name)
  for i, n in ipairs(T.st().expansions or {}) do if n == name then return i - 1 end end
  error("not posted: " .. name)
end
-- Table point (x, y) on screen at 80 % in the floating window.
function T.at80(x, y) return math.floor(98 + 1.6 * x + 0.5) .. "," .. math.floor(84 + 1.6 * y + 0.5) end
-- The scrolls from entry `from` on, as "x,y,notches" joined.
function T.scrollsSince(from)
  local out = {}
  for i = (from or 0) + 1, #S.scrolls do out[#out + 1] = S.scrolls[i][1] .. "," .. S.scrolls[i][2] .. "," .. S.scrolls[i][3] end
  return table.concat(out, " ")
end
-- Every change wait asked from here on, with its options.
function T.recordWaits()
  local asked = {}
  local snapshot = rawget(T.host.screen, "snapshot")
  rawset(T.host.screen, "snapshot", function(opts, cb)
    if opts and opts.change then asked[#asked + 1] = { region = opts.region, change = opts.change } end
    return snapshot(opts, cb)
  end)
  return asked
end
-- `misread` painted only while the grid shows what its n-th move took it to (T.browserMove's moves
-- that moved it, by the wheel or the track).
function T.misreadAtMove(n, misread)
  local move, moved = T.browserMove, 0
  T.browserMove = function(list, rows)
    local g = S.browser.grid
    if list == "grid" then g.misread = moved + 1 == n and misread or nil end
    local did = move(list, rows)
    if did and list == "grid" then moved += 1 end
    return did
  end
end
-- The grid's wheel's sign, as the module keeps it for the session.
function T.gridSign()
  local B = S.included["modules/vps-avenger/src/actions/browser.luau"][1]
  local _, sign = B.ways()
  return sign.grid
end
-- Back to Avenger with the page key, Avenger in front again.
function T.back(W)
  S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
  T.pageKey()
  T.event()
end
"##;

/// Avenger's own list of twelve, in an order of its own, as a scenario's Luau `own`.
const OWN: &str = r##"        local own = { "Synthwave", "Psytrance", "Dark Techno", "Acid Breaks", "Vocal Chops", "Deep House",
          "Future Bass", "Retro Synths", "Bass Monsters", "Electro Pop", "Trap Essentials", "Hardstyle Leads" }"##;

fn scenario(s: &str) -> String {
    format!("{BROWSER}\n{s}")
}

// ---------------------------------------------------------------------------------------------
// Through src/act.luau: the view, Close and FAV.
// ---------------------------------------------------------------------------------------------

/// Expansion View: the field at (15, 50) — (122, 164) — clicked with the LEFT button, the view list
/// seen painted inside Pp.VIEW_LIST, List's row at (27, 60) — (141, 180) — chosen in it, and 8 per
/// Row's at (27, 154.5) — (141, 331) — another time; the caption read again: the view asked posted,
/// no expansion open and none selected. Another view read after: the note. The view read already:
/// nothing clicked, "unchanged". A popup that never takes its row: the field clicked again.
#[test]
fn the_view_list_is_opened_by_its_field_and_the_caption_is_read_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("8 per Row"))
        T.ok(T.st().expansion_view == "8 per Row" and T.st().expansion_open == false, "the key read the view")
        local function choose(view, popup, after)
          T.tripFrom({ action = "set_expansion_view", args = { view = view }, focused = true }, W)
          local c = S.clicks[#S.clicks]
          T.ok(T.clickAt() == "122,164" and c.button ~= "right", "the field with the left button: " .. T.clickAt())
          T.paintAvenger(98, 84, 1.6, { popup = popup })
          T.tick(3)
          local item = T.clickAt()
          S.browser.grid.view = after
          T.repaint({})
          T.settle(5)
          return item
        end
        local item = choose("List", { 8, 52, 80, 166 }, "List")
        T.ok(item == "141,180", "List's row: " .. item)
        T.ok(T.st().expansion_view == "List" and T.st().expansion_open == false and T.st().selected_expansion == ""
          and T.lastPost().kind == "restoreFocus", "posted: " .. tostring(T.st().expansion_view))
        T.ok(T.count("[menu] ") > 0 and T.count("drawn: armed on 'Expansion View'") >= 1, T.dump())
        T.event()
        item = choose("8 per Row", { 8, 52, 80, 166 }, "4 per Row")
        T.ok(item == "141,331", "8 per Row's row: " .. item)
        T.ok(T.err() == "Expansion View: Expansion View reads 4 per Row, not 8 per Row", T.err())
        T.ok(T.st().expansion_view == "4 per Row", "the view read is posted")
        -- The view read already: nothing clicked.
        T.event()
        local clicks = #S.clicks
        T.tripFrom({ action = "set_expansion_view", args = { view = "4 per Row" }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Expansion View: unchanged, Expansion View 4 per Row", T.err())
        -- A view Avenger does not have: refused with no trip.
        local from = #S.pageOrder
        T.pageMsg({ action = "set_expansion_view", args = { view = "9 per Row" }, focused = true })
        T.ok(T.err() == "Expansion View: 9 per Row is not one of VPS Avenger's views"
          and not string.find(T.pageSince(from), "focus", 1, true), T.err())
        -- A popup that never takes its row: the field clicked again to close it.
        T.event()
        T.tripFrom({ action = "set_expansion_view", args = { view = "List" }, focused = true }, W)
        T.paintAvenger(98, 84, 1.6, { popup = { 8, 52, 80, 166 } })
        T.run(8400)
        T.ok(T.count("its popup was left open (untaken) — clicking its opener again to close it") == 1, T.dump())
        T.ok(T.clickAt() == "122,164", "the field again: " .. T.clickAt())
        T.repaint({})
        T.settle(4)
        T.ok(T.err() == "Expansion View: the menu did not take the choice", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// An open expansion — the caption an icon, the columns its lists — refuses a view with nothing
/// clicked: the same point would close it. A caption reading a view refuses Close with nothing
/// clicked: that click would open the view list.
#[test]
fn no_view_is_chosen_and_no_expansion_closed_where_the_caption_says_otherwise() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        T.ok(T.st().expansion_open == true and T.st().selected_expansion == "Synthwave"
          and T.st().selected_expansion_from_database == true, "the key: Synthwave open, named by the database")
        local clicks = #S.clicks
        T.tripFrom({ action = "set_expansion_view", args = { view = "List" }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Expansion View: an expansion is open in VPS Avenger; press Close Expansion "
          .. "to close it first", T.err())
        T.event()
        S.browser = T.grid("8 per Row")
        T.repaint({})
        T.tripFrom({ action = "close_expansion", args = {}, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Close Expansion: no expansion is open in VPS Avenger", T.err())
        T.ok(T.st().expansion_view == "8 per Row" and T.st().expansion_open == false, "the grid posted")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Close at (15, 50) — (122, 164) — where the area reads the expansion open: the caption read again
/// reads a view, and the open expansion is gone from the state — its lists, FAV, the stars, the
/// category; the focus on the expansions' list, the tool's. Still open after: the error.
#[test]
fn close_reads_the_view_again_and_forgets_the_open_expansion() {
    run_mac(&scenario(r##"
        local o = T.openOf("Synthwave", { fav = true, stars = { ["X17 Arp Line 01"] = true } })
        local _, _, W = T.browserPage({ mode = "open", open = o, grid = { view = "8 per Row", names = T.BROWSER_NAMES,
          firstRow = 0 } })
        local st = T.st()
        T.ok(st.expansion_favorite_view == true and st.favorite_preset_states and st.favorite_preset_states["0"] == true
          and st.favorite_preset_states["1"] == false and st.expansion_categories ~= nil, "the key read FAV and the stars")
        T.tripFrom({ action = "close_expansion", args = {}, focused = true }, W)
        T.ok(T.clickAt() == "122,164", "Close: " .. T.clickAt())
        T.settle(4)
        st = T.st()
        T.ok(st.expansion_open == false and st.expansion_view == "8 per Row" and st.selected_expansion == ""
          and st.expansion_categories == nil and st.expansion_favorite_view == nil and st.favorite_preset_states == nil
          and st.selected_preset_category == nil, "closed: " .. tostring(st.expansion_open))
        T.ok(T.lastPost().kind == "focus" and T.lastPost().id == "expansion-search-results", tostring(T.lastPost().id))
        -- Open again, and Close does nothing.
        T.event()
        S.browser.mode, S.browser.open = "open", T.openOf("Synthwave", { stuck = true })
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        T.tripFrom({ action = "close_expansion", args = {}, focused = true }, W)
        T.run(2400)
        T.ok(T.err() == "Close Expansion: VPS Avenger still shows the expansion open", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// FAV at (212, 50) — (437, 164): `enabled` equal to what FAV reads clicks nothing; otherwise it is
/// clicked and read after, and with FAV on every row's star is posted by the preset's place. No
/// expansion open: refused.
#[test]
fn the_favorites_view_is_read_before_and_after_its_click() {
    run_mac(&scenario(r##"
        local o = T.openOf("Synthwave", { category = "Bass", stars = { ["X17 Bass Line 01"] = true } })
        local _, _, W = T.browserPage({ mode = "open", open = o })
        T.ok(T.st().expansion_favorite_view == false, "FAV read OFF at the key")
        local clicks = #S.clicks
        T.tripFrom({ action = "toggle_expansion_favorite_view", args = { enabled = false }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "The favorites view: unchanged, FAV OFF", T.err())
        T.event()
        T.tripFrom({ action = "toggle_expansion_favorite_view", args = { enabled = true }, focused = true }, W)
        T.ok(T.clickAt() == "437,164", "FAV: " .. T.clickAt())
        T.settle(4)
        local st = T.st()
        T.ok(st.expansion_favorite_view == true and st.favorite_preset_states["2"] == true
          and st.favorite_preset_states["3"] == false and st.favorite_preset_states["0"] == nil,
          "FAV ON and the stars: " .. T.jsonEncode(st.favorite_preset_states or {}))
        T.ok(T.lastPost().kind == "restoreFocus", T.lastPost().kind)
        -- Off again: the stars gone.
        T.event()
        T.tripFrom({ action = "toggle_expansion_favorite_view", args = {}, focused = true }, W)
        T.settle(4)
        T.ok(T.st().expansion_favorite_view == false and T.st().favorite_preset_states == nil, "FAV OFF")
        -- The grid shown: refused.
        T.event()
        S.browser = T.grid("8 per Row")
        T.repaint({})
        clicks = #S.clicks
        T.tripFrom({ action = "toggle_expansion_favorite_view", args = { enabled = true }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "The favorites view: no expansion is open in VPS Avenger", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// Opening an expansion.
// ---------------------------------------------------------------------------------------------

/// Avenger's grid in 4 per Row lists sixteen expansions in an order of its own, not the roster's:
/// "Dark Techno", the roster's fifth (index 4), is Avenger's third, its words centred at (144.4, 82)
/// below the tile's middle — clicked there, (329, 215), never at the tool's point for index 4,
/// (36.9, 119) — (157, 274) — and with no scroll. Then the columns read: Dark Techno's presets and
/// categories score it, so it is posted open, not marked, with the database's lists, FAV, and the
/// category its rows read as the page's category; the focus on its heading.
#[test]
fn an_expansion_on_the_grid_is_opened_by_the_middle_of_its_words() {
    run_mac(&scenario(r##"
        local own = { "Synthwave", "Psytrance", "Dark Techno", "Acid Breaks", "Vocal Chops", "Deep House", "Future Bass",
          "Retro Synths", "Bass Monsters", "Electro Pop", "Trap Essentials", "Hardstyle Leads", "Ambient Worlds",
          "Organic Textures", "World Percussion", "Cinematic Pads" }
        local model = T.grid("4 per Row", own, { opens = { ["Dark Techno"] = T.openOf("Dark Techno") } })
        local _, _, W = T.browserPage(model)
        local i = T.indexIn("Dark Techno")
        T.ok(i == 4, "the roster's fifth: " .. i)
        T.tripFrom({ action = "open_expansion", args = { index = i, page_start = 0, visible_index = i }, focused = true }, W)
        T.settle(4)
        T.ok(#S.scrolls == 0, "no scroll: " .. T.scrollsSince(0))
        T.ok(S.browser.clicked[1] == "Dark Techno" and T.clickAt(#S.clicks) == "329,215", "the words' middle: "
          .. T.clickAt(#S.clicks))
        T.ok(T.count("clicking 'Dark Techno' at (329,215), the middle of its words (read 'Dark Techno', exact)") == 1, T.dump())
        local st = T.st()
        T.ok(st.expansion_open == true and st.selected_expansion == "Dark Techno" and st.selected_expansion_from_database == nil,
          "open, named by the word clicked: " .. tostring(st.selected_expansion))
        T.ok(table.concat(st.expansion_categories, "|") == "Arp|Bass|Leads|Pads" and st.expansion_lists_from_database == true
          and st.expansion_favorite_view == false and st.selected_preset_category == "Arp", "its lists, FAV, the category")
        T.ok(T.lastPost().kind == "focus" and T.lastPost().id == "open-expansion-name", tostring(T.lastPost().id))
        T.ok(T.count("the tool's place, page start 0, tile 4 — the first place to look is the grid as it stands") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// "Future Bass", Avenger's eighth, below the List view's five: three steps down, each one wheel
/// notch down at the grid's middle (117, 140) — (285, 308) — and a change wait from the picture last
/// read, on the area's top and bottom quarters, with a still spell of 60 ms; found on the third, its
/// words clicked at (117.5, 217) — (286, 431); "Looking for …" said once, behind the screen reader.
#[test]
fn a_name_off_the_grid_is_found_a_notch_at_a_time() {
    run_mac(&scenario(r##"
        local model = T.grid("List", nil, { opens = { ["Future Bass"] = T.openOf("Future Bass") } })
        local _, _, W = T.browserPage(model)
        local waits = T.recordWaits()
        local said = #S.speech
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Future Bass"), page_start = 0, visible_index = 7 },
          focused = true }, W)
        T.settle(8)
        T.ok(T.scrollsSince(0) == "285,308,-1 285,308,-1 285,308,-1", "three notches down: " .. T.scrollsSince(0))
        T.ok(#waits >= 3, "a change wait per step: " .. #waits)
        for n = 1, 3 do
          local c = waits[n].change
          T.ok(c.from ~= nil and c.settle == 60 and c.timeout == 2000 and #c.watch == 2, "step " .. n .. "'s wait")
          -- The area's quarters: (10, 56)-(225, 98) and (10, 182)-(225, 224) at 80 %.
          T.ok(T.region(c.watch[1]) == "114,174,458,241" and T.region(c.watch[2]) == "114,375,458,442",
            T.region(c.watch[1]) .. " " .. T.region(c.watch[2]))
        end
        T.ok(T.clickAt(#S.clicks) == "286,431" and S.browser.clicked[1] == "Future Bass", "the tile: " .. T.clickAt(#S.clicks))
        local looks = 0
        for i = said + 1, #S.speech do
          if S.speech[i].text == "Looking for Future Bass in VPS Avenger's list." then
            looks += 1
            T.ok(S.speech[i].interrupt == false, "behind the screen reader")
          end
        end
        T.ok(looks == 1, "said once: " .. T.said(said + 1))
        T.ok(T.count("its place 8 lies after the 1 to 5 read") == 1 and T.count("the grid of expansions moves by the wheel") == 1,
          T.dump())
        T.ok(T.st().selected_expansion == "Future Bass", "opened")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// With the roster "Factory Standard" the grid's names are in no order the module knows, so the
/// tool's page start decides the first way: 64, down — until the list stops, then up until "Bass
/// Monsters" is read, and clicked. A name on neither side: both ends reached, then "… is not in VPS
/// Avenger's list", nothing clicked on the grid.
#[test]
fn the_search_turns_at_the_end_and_says_a_name_avenger_does_not_list() {
    run_mac(&scenario(r##"
        local model = T.grid("List", nil, { firstRow = 5, opens = { ["Bass Monsters"] = T.openOf("Bass Monsters") } })
        local _, _, W = T.browserPage(model, { roster = "Factory Standard" })
        T.ok(table.concat(T.st().expansions, "|") == "Acid Breaks|Bass Monsters|Cinematic Pads", "the roster")
        T.tripFrom({ action = "open_expansion", args = { index = 1, page_start = 64, visible_index = 1 }, focused = true }, W)
        T.run(9000)
        local down, up = 0, 0
        for _, s in ipairs(S.scrolls) do if s[3] < 0 then down += 1 else up += 1 end end
        T.ok(down == 11 and up == 13, "down to the end, then up: " .. down .. " down, " .. up .. " up")
        T.ok(T.count("not read in the grid of expansions — looking down (the first place to look)") == 1
          and T.count("the grid of expansions moves no further — looking up") == 1, T.dump())
        T.ok(S.browser.clicked[1] == "Bass Monsters" and T.st().selected_expansion == "Bass Monsters", "found and opened")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local names = {}
        for _, n in ipairs(T.BROWSER_NAMES) do if n ~= "Cinematic Pads" then names[#names + 1] = n end end
        local _, _, W = T.browserPage(T.grid("List", names, { firstRow = 3 }), { roster = "Factory Standard" })
        T.tripFrom({ action = "open_expansion", args = { index = 2, page_start = 0, visible_index = 2 }, focused = true }, W)
        T.run(12000)
        T.ok(#S.clicks == 0 and S.browser.clicked == nil, "nothing clicked: " .. #S.clicks)
        T.ok(T.err() == "Opening an expansion: Cinematic Pads is not in VPS Avenger's list", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A grid whose wheel moves nothing: the first notch's change wait comes back unchanged at its bound,
/// the track's end toward the step at (228, 222) — (463, 439) — is clicked and pages the list, and the
/// list is the track's from then on: one notch in all, the log line once, the next search on the
/// track alone, up at (228, 58) — (463, 177). At a real end, where neither moves anything either way,
/// the way stays not known, and a name not read there is said as not read in a list that did not
/// move — never as not in Avenger's list.
#[test]
fn a_wheel_that_moves_nothing_hands_the_list_to_its_track_for_the_session() {
    run_mac(&scenario(r##"
        local model = T.grid("List", nil, { wheelDead = true, opens = { ["World Percussion"] = T.openOf("World Percussion"),
          ["Acid Breaks"] = T.openOf("Acid Breaks") } })
        local _, _, W = T.browserPage(model)
        local waits = T.recordWaits()
        T.tripFrom({ action = "open_expansion", args = { index = 19, page_start = 64, visible_index = 0 }, focused = true }, W)
        T.run(4000)
        T.ok(#S.scrolls == 1, "one notch: " .. T.scrollsSince(0))
        -- Every step's change wait, the track's too: the bands, and a still spell.
        local steps = 0
        for _, w in ipairs(waits) do
          if w.change.watch and #w.change.watch == 2 and T.region(w.change.watch[1]) == "114,174,458,241" then
            steps += 1
            T.ok(w.change.settle == 60 and w.change.from ~= nil, "a step's wait: " .. tostring(w.change.settle))
          end
        end
        T.ok(steps == 5, "the notch and four track clicks: " .. steps)
        local tracks = 0
        for _, c in ipairs(S.browser.clicked or {}) do if c == "track down" then tracks += 1 end end
        T.ok(tracks == 4 and T.clickAt(1) == "463,439", "the track's end, four times: " .. tracks .. " " .. T.clickAt(1))
        T.ok(T.count("the wheel moved nothing in the grid of expansions; its scrollbar's track did — the track from here on") == 1,
          T.dump())
        T.ok(T.st().selected_expansion == "World Percussion", "found by the track")
        -- Closed again, and the next search on the track alone.
        T.event()
        S.browser.mode = "grid"
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = 0, page_start = 0, visible_index = 0 }, focused = true }, W)
        T.run(3000)
        T.ok(#S.scrolls == 1, "still one notch: " .. T.scrollsSince(0))
        T.ok(S.browser.clicked[#S.browser.clicked] == "Acid Breaks" and T.st().selected_expansion == "Acid Breaks", "found")
        local ups = 0
        for i = 1, #S.clicks do if T.clickAt(i) == "463,177" then ups += 1 end end
        T.ok(ups == 4, "up the track: " .. ups)
        T.ok(T.count("its scrollbar's track did — the track from here on") == 1, "the line once")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("List", { "Acid Breaks", "Psytrance", "Synthwave", "Deep House" }))
        T.tripFrom({ action = "open_expansion", args = { index = 2, page_start = 0, visible_index = 2 }, focused = true }, W)
        T.run(12000)
        T.ok(#S.scrolls == 2 and #S.clicks == 2, "a notch and a track click each way: " .. #S.scrolls .. " " .. #S.clicks)
        T.ok(T.err() == "Opening an expansion: Bass Monsters was not read, and VPS Avenger's list did not move", T.err())
        local B = S.included["modules/vps-avenger/src/actions/browser.luau"][1]
        local means = B.ways()
        T.ok(means.grid == nil, "the way not known: " .. tostring(means.grid))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The pointer's hover over a tile, painted in the middle of the area after a notch that moved
/// nothing: the step's change wait watches only the top and bottom quarters, so it comes back
/// unchanged at its bound, the track moves nothing either, and the list counts as stopped — the
/// search turns and ends with the name not read in a list that did not move.
#[test]
fn the_pointers_hover_is_not_taken_for_the_list_moving() {
    run_mac(&scenario(r##"
        S.honourMinPixels = true
        local _, _, W = T.browserPage(T.grid("List", nil, { hoverOnScroll = true, trackDead = true }),
          { roster = "Factory Standard" })
        local names = {}
        for i = 4, 20 do names[#names + 1] = T.BROWSER_NAMES[i] end
        S.browser.grid.names = names
        T.repaint({})
        T.tripFrom({ action = "open_expansion", args = { index = 1, page_start = 0, visible_index = 1 }, focused = true }, W)
        T.run(12000)
        T.ok(#S.scrolls == 2, "a notch each way: " .. T.scrollsSince(0))
        T.ok(T.count("step down in the grid of expansions: unchanged by the wheel and the track — at its end") == 1
          and T.count("step up in the grid of expansions: unchanged by the wheel and the track — at its end") == 1, T.dump())
        T.ok(T.err() == "Opening an expansion: Bass Monsters was not read, and VPS Avenger's list did not move", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The grid's words, named (Br.tiles of the reading the model gives): a host row of eight names in
/// 8 per Row, one line, is split by column; "Melodic Techno" over "5" is one tile, its point between
/// the two lines; "Melodlc Techno 3" is taken for Melodic Techno 3 by similarity, which stands out
/// from 2, 4 and 5 by the margin, and "Melodic Techno" alone is none of them; a name the catalog does
/// not have is kept as read; and a row whose words lie on the area's top edge is left out.
#[test]
fn tiles_are_named_by_their_column_and_a_name_on_two_lines_is_one() {
    run_mac(&scenario(r##"
        local names = { "Acid Breaks", "Melodic Techno 2", "Melodic Techno 3", "Melodic Techno 4", "Melodic Techno 5",
          "Our Own Pack", "Psytrance", "Synthwave", "Deep House" }
        local model = T.grid("8 per Row", names, { twoLines = { ["Melodic Techno 5"] = { "Melodic Techno", "5" } },
          misread = { ["Melodic Techno 3"] = "Melodlc Techno 3", ["Melodic Techno 4"] = "Melodic Techno" } })
        T.presets(T.BROWSER_FIXTURE)
        S.browser = model
        local header = T.avengerAt80("Init Preset")
        T.paintAvenger(98, 84, 1.6, {})
        local Br = S.included["modules/vps-avenger/src/read/browser.luau"][1]
        local P = S.included["modules/vps-avenger/src/read/picture.luau"][1]
        local pl = P.placer(header:origin(), 1.6, 80)
        local reads = T.browserReads(98, 84, 1.6)
        T.ok(#reads["browser area"].lines == 3, "the first row one line, its two-line name's second, the next row: "
          .. #reads["browser area"].lines)
        local tiles = Br.tiles(reads["browser area"], "8 per Row", pl)
        local got = {}
        for _, t in ipairs(tiles) do got[#got + 1] = t.name .. "/" .. t.how .. "/" .. t.column end
        T.ok(table.concat(got, " ") == "Acid Breaks/exact/1 Melodic Techno 2/exact/2 Melodic Techno 3/fuzzy/3 "
          .. "Melodic Techno/as read/4 Melodic Techno 5/exact/5 Our Own Pack/as read/6 Psytrance/exact/7 Synthwave/exact/8 "
          .. "Deep House/exact/1", table.concat(got, " "))
        -- Melodic Techno 5's point: its column's middle (133, 68 + 5) — between its two lines.
        for _, t in ipairs(tiles) do
          if t.name == "Melodic Techno 5" then
            T.ok(t.x == math.floor(98 + 1.6 * 133 + 0.5) and t.y == math.floor(84 + 1.6 * 73 + 0.5), t.x .. "," .. t.y)
          end
        end
        T.ok(T.count("col 5 'Melodic Techno' over '5', gap ") == 1, T.dump())
        -- The first row on the area's top edge: left out.
        S.browser.grid.cutTop = true
        tiles = Br.tiles(T.browserReads(98, 84, 1.6)["browser area"], "8 per Row", pl)
        T.ok(#tiles == 1 and tiles[1].name == "Deep House" and T.count("at the area's edge left out") == 1,
          #tiles .. " " .. T.dump())
    "##));
}

/// What opened is what the columns list: another expansion's presets — the error, and that one
/// posted, marked as named by the database; an open expansion the database does not name — the
/// tool's words for an unknown open expansion; nothing changed — "nothing VPS Avenger shows changed".
#[test]
fn the_expansion_opened_is_said_by_what_it_lists() {
    run_mac(&scenario(r##"
        local strange = { name = "?", categories = { "Arp", "Bass" }, presets = { Arp = { "Unknown One" }, Bass = { "Unknown Two" } },
          category = "Arp", firstCategory = 0, firstRow = 0, fav = false, stars = {} }
        local model = T.grid("8 per Row", nil, { opens = { ["Dark Techno"] = T.openOf("Retro Synths"), ["Deep House"] = strange } })
        local _, _, W = T.browserPage(model)
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
        T.settle(4)
        T.ok(T.err() == "Opening an expansion: VPS Avenger opened Retro Synths, by the presets it lists (from the database)",
          T.err())
        local st = T.st()
        T.ok(st.selected_expansion == "Retro Synths" and st.selected_expansion_from_database == true and st.expansion_open == true
          and st.expansion_categories ~= nil, "posted as Retro Synths, marked")
        T.event()
        S.browser.mode = "grid"
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Deep House") }, focused = true }, W)
        T.settle(4)
        T.ok(T.err() == "Opening an expansion: VPS Avenger shows an expansion open; which one cannot be read now", T.err())
        T.ok(T.st().expansion_open == true and T.st().selected_expansion == "" and T.st().expansion_categories == nil, "open, not named")
        T.event()
        S.browser.mode = "grid"
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Psytrance") }, focused = true }, W)
        T.run(2400)
        T.ok(T.err() == "Opening an expansion: nothing VPS Avenger shows changed", T.err())
        T.ok(T.st().expansion_open == false, "the grid posted")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A grid whose words name no expansion moves nothing and clicks nothing.
#[test]
fn a_grid_that_names_nothing_moves_nothing() {
    run_mac(&scenario(r##"
        local misread = {}
        for _, n in ipairs(T.BROWSER_NAMES) do misread[n] = "qq" end
        local _, _, W = T.browserPage(T.grid("8 per Row", nil, { misread = misread }))
        T.tripFrom({ action = "open_expansion", args = { index = 3 }, focused = true }, W)
        T.settle(4)
        T.ok(#S.scrolls == 0 and #S.clicks == 0, "nothing moved or clicked")
        T.ok(T.err() == "Opening an expansion: the names in VPS Avenger's browser cannot be read now", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// With the roster "Complete" and Avenger's list not read, the roster's order is no order the
/// module knows: Avenger's grid lists the catalog's fifteenth to nineteenth first, then the first to
/// the fourteenth, then the twentieth, so a step down shows places of the roster that went back.
/// That is no wheel turned round: World Percussion, the grid's last, is reached fifteen notches
/// down, every one down, and the grid's wheel keeps its sign for the session. Nor is a grid in the
/// catalog's reverse order, whose roster places go the other way at every step: Acid Breaks, looked
/// for up first by those places, is found down after the top, and the wheel is not turned round.
#[test]
fn a_grid_in_no_order_known_never_has_its_wheel_turned_round() {
    run_mac(&scenario(r##"
        local names = {}
        for i = 15, 19 do names[#names + 1] = T.BROWSER_NAMES[i] end
        for i = 1, 14 do names[#names + 1] = T.BROWSER_NAMES[i] end
        names[#names + 1] = T.BROWSER_NAMES[20]
        local model = T.grid("List", names, { opens = { ["World Percussion"] = T.openOf("World Percussion") } })
        local _, _, W = T.browserPage(model)
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("World Percussion"), page_start = 0 },
          focused = true }, W)
        T.run(9000)
        local down, up = 0, 0
        for _, s in ipairs(S.scrolls) do if s[3] < 0 then down += 1 else up += 1 end end
        T.ok(down == 15 and up == 0, "fifteen notches down: " .. down .. " down, " .. up .. " up")
        T.ok(T.count("its place 20 lies after the 15 to 19 read") == 1 and T.count("turned round") == 0, T.dump())
        T.ok(T.gridSign() == 1, "the grid's wheel as it was: " .. T.gridSign())
        T.ok(S.browser.clicked[#S.browser.clicked] == "World Percussion" and T.st().selected_expansion == "World Percussion",
          "opened")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local names = {}
        for i = 20, 1, -1 do names[#names + 1] = T.BROWSER_NAMES[i] end
        local model = T.grid("List", names, { opens = { ["Acid Breaks"] = T.openOf("Acid Breaks") } })
        local _, _, W = T.browserPage(model)
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Acid Breaks"), page_start = 0 }, focused = true }, W)
        T.run(12000)
        T.ok(T.count("its place 1 lies before the 16 to 20 read") == 1 and T.count("turned round") == 0, T.dump())
        T.ok(T.gridSign() == 1, "the grid's wheel as it was: " .. T.gridSign())
        T.ok(T.scrollsSince(1) == string.rep(" 285,308,-1", 15):sub(2), T.scrollsSince(0))
        T.ok(S.browser.clicked[#S.browser.clicked] == "Acid Breaks" and T.st().selected_expansion == "Acid Breaks", "opened")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The key read 8 per Row; then Avenger shows List with its caption not read, Dark Techno on the
/// screen. Opening it — its words split across 8 per Row's columns if the view remembered were
/// taken — is refused with nothing moved or clicked, the view posted as not read; so is reading
/// Avenger's list; and Close, where the caption is not read and the area reads the grid, clicks
/// nothing either.
#[test]
fn the_grid_is_placed_by_the_view_read_now_never_one_remembered() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("8 per Row", nil, { opens = { ["Dark Techno"] = T.openOf("Dark Techno") } }))
        T.ok(T.st().expansion_view == "8 per Row", "the key read 8 per Row")
        local g = S.browser.grid
        g.view, g.caption, g.firstRow = "List", false, 2
        T.repaint({})
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
        T.settle(4)
        T.ok(#S.scrolls == 0 and #S.clicks == 0, "nothing moved or clicked: " .. #S.scrolls .. " " .. #S.clicks)
        T.ok(T.err() == "Opening an expansion: the Expansion View cannot be read now", T.err())
        T.ok(T.st().expansion_view == nil and T.st().expansion_open == false, "the view posted as not read")
        T.event()
        T.tripFrom({ action = "close_expansion", args = {}, focused = true }, W)
        T.settle(4)
        T.ok(#S.clicks == 0 and T.err() == "Close Expansion: the Expansion View cannot be read now", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("8 per Row"))
        T.ok(T.st().expansion_view == "8 per Row", "the key read 8 per Row")
        local g = S.browser.grid
        g.view, g.caption, g.firstRow = "List", false, 2
        T.repaint({})
        T.event()
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.settle(4)
        T.ok(#S.scrolls == 0 and #S.clicks == 0, "nothing moved: " .. #S.scrolls .. " " .. #S.clicks)
        T.ok(T.err() == "Reading Avenger's list: the Expansion View cannot be read now", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The grid's rows told by its tiles, not by bins of the pitch from the area's top: in 8 per Row,
/// Future Bass — the first row's eighth — has its words 4 points lower than its row's (a descender,
/// a grid scrolled to an offset), past where such a bin would end. It stays in the first row, after
/// Electro Pop and before Hardstyle Leads, the second row's first.
#[test]
fn the_grids_rows_are_told_by_its_tiles_not_by_bins_from_the_top() {
    run_mac(&scenario(r##"
        local names = {}
        for i = 1, 16 do names[i] = T.BROWSER_NAMES[i] end
        T.presets(T.BROWSER_FIXTURE)
        S.browser = T.grid("8 per Row", names, { nudge = { ["Future Bass"] = 4 } })
        local header = T.avengerAt80("Init Preset")
        T.paintAvenger(98, 84, 1.6, {})
        local Br = S.included["modules/vps-avenger/src/read/browser.luau"][1]
        local P = S.included["modules/vps-avenger/src/read/picture.luau"][1]
        local pl = P.placer(header:origin(), 1.6, 80)
        local area = pl:rect(EX.AREA)
        local tiles = Br.tiles(T.browserReads(98, 84, 1.6)["browser area"], "8 per Row", pl)
        local got = {}
        for _, t in ipairs(tiles) do got[#got + 1] = t.name end
        T.ok(table.concat(got, "|") == table.concat(names, "|"), table.concat(got, "|"))
        for _, t in ipairs(tiles) do
          if t.name == "Future Bass" then
            T.ok(math.floor((t.y - area[2]) / (EX.GRID8_Y_STEP * 1.6)) == 1, "past the first bin: " .. t.y)
          end
        end
    "##));
}

/// A give-up during a step's change wait — the page key, another application's window, the page's
/// close box — scrolls and clicks nothing more, claims no open expansion, and holds no picture once
/// the wait has answered.
#[test]
fn a_give_up_mid_search_moves_and_clicks_nothing_more() {
    for end in ["key", "another application", "page closed"] {
        let s = r##"
            local ending = "@END@"
            local _, _, W = T.browserPage(T.grid("List", nil, { wheelDead = true }))
            T.tripFrom({ action = "open_expansion", args = { index = 19 }, focused = true }, W)
            T.tick(2)
            T.ok(#S.scrolls == 1, "the first notch out")
            if ending == "key" then
              S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = T.PAGE_WIN, { T.PAGE_WIN } end
              T.pageKey()
            elseif ending == "another application" then
              S.listed = { W, T.OTHER }
              T.show(T.OTHER)
            else
              T.pageClosed("user")
            end
            local clicks = #S.clicks
            T.run(6000)
            T.ok(#S.scrolls == 1 and #S.clicks == clicks, ending .. ": nothing more: " .. #S.scrolls .. " " .. #S.clicks)
            T.ok(T.lastPost("state").state.expansion_open == false, ending .. ": no open expansion claimed")
            T.ok(T.held() == 0, ending .. ": no picture held: " .. T.held())
        "##
        .replace("@END@", end);
        run_mac(&scenario(&s));
    }
}

// ---------------------------------------------------------------------------------------------
// An open expansion: its categories, its rows, the stars.
// ---------------------------------------------------------------------------------------------

/// Cinematic Pads open, its twelve categories nine at a time. Bass, shown, clicked at the middle of
/// its words, (42, 86) — (165, 222) — and its rows read as Bass's: posted as the page's category.
/// Plucks, the eleventh, below the nine: the categories' list stepped down a notch at a time at its
/// middle (42, 146) — (165, 318) — by the database's order, until it is read, then clicked at (165,
/// 446). Rows of another category after the click: the error, by their names from the database; no
/// rows read: the note.
#[test]
fn a_category_is_clicked_where_it_is_read_and_its_presets_say_it_was_taken() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Cinematic Pads") })
        T.ok(T.st().selected_expansion == "Cinematic Pads" and #T.st().expansion_categories == 12, "the key read it open")
        T.tripFrom({ action = "select_expansion_category", args = { index = 1, total = 12 }, focused = true }, W)
        T.settle(4)
        T.ok(S.browser.clicked[1] == "category Bass" and T.clickAt(#S.clicks) == "165,222" and #S.scrolls == 0,
          "Bass where it was read: " .. T.clickAt(#S.clicks))
        T.ok(T.st().selected_preset_category == "Bass" and T.lastPost().kind == "restoreFocus", tostring(T.st().selected_preset_category))
        T.event()
        T.tripFrom({ action = "select_expansion_category", args = { index = 10, total = 12 }, focused = true }, W)
        T.settle(6)
        T.ok(T.scrollsSince(0) == "165,318,-1 165,318,-1", "two notches down: " .. T.scrollsSince(0))
        T.ok(S.browser.clicked[#S.browser.clicked] == "category Plucks" and T.clickAt(#S.clicks) == "165,446",
          "Plucks: " .. T.clickAt(#S.clicks))
        T.ok(T.count("its place 11 lies after the 1 to 9 read") == 1 and T.st().selected_preset_category == "Plucks", T.dump())
        -- The rows another category's after the click.
        T.event()
        S.browser.open.showsCategory = "Arp"
        T.repaint({})
        T.tripFrom({ action = "select_expansion_category", args = { index = 7, total = 12 }, focused = true }, W)
        T.run(2400)
        T.ok(T.err() == "Choosing a category: VPS Avenger lists the presets of Arp (by their names, from the database)", T.err())
        T.ok(T.st().selected_preset_category == "Plucks", "the page's category not moved")
        -- No rows read.
        T.event()
        S.browser.open.showsCategory = "None"
        T.repaint({})
        T.tripFrom({ action = "select_expansion_category", args = { index = 6, total = 12 }, focused = true }, W)
        T.run(2400)
        T.ok(T.err() == "Choosing a category: Keys chosen; which presets VPS Avenger lists cannot be read now", T.err())
        -- Refused with no trip: no index; no expansion known open.
        local from = #S.pageOrder
        T.pageMsg({ action = "select_expansion_category", args = {}, focused = true })
        T.ok(T.err() == "Choosing a category: no category was named" and not string.find(T.pageSince(from), "focus ", 1, true),
          T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Synthwave open on Arp: X17 Arp Line 02, the second row, found by its name and clicked at the
/// middle of its words, (142, 72.5) — (325, 200); the name watched as after ◀ and ▶ and the strip and
/// the macros read after it; the name changed — posted, with the database's expansion and its place.
/// Load is a double-click at the first row, (325, 180). The same name: "unchanged". None read: the
/// error, and the preset posted as not read — never the database's name for the row.
#[test]
fn a_preset_row_is_found_by_its_name_then_selected_or_loaded() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") }, { words = { preset = "X17 Arp Line 01" } })
        T.ok(T.st().loaded_expansion_preset_index == 0, "the key placed the preset")
        local function row(action, i, keep)
          return { action = action, args = { preset_index = i, visible_index = i, category_preset_index = i, category_count = 2,
            category_index = 0, category_total = 4, keep_preset_category = keep or "Arp" }, focused = true }
        end
        local snaps = S.snaps
        T.tripFrom(row("select_expansion_preset", 1), W)
        T.ok(T.clickAt(#S.clicks) == "325,200" and S.browser.clicked[1] == "preset X17 Arp Line 02", "the row: " .. T.clickAt(#S.clicks))
        T.nameReads("X17 Arp Line 02")
        local st = T.st()
        T.ok(st.preset == "X17 Arp Line 02" and st.expansion == "Synthwave" and st.expansion_from_database == true
          and st.loaded_expansion_preset_index == 1 and st.selected_preset_category == "Arp", tostring(st.preset))
        T.ok(S.snaps >= snaps + 3, "the browser, the strip and the macros pictured")
        T.ok(T.count("[avenger] [browser] 'Selecting a preset': clicking X17 Arp Line 02 at (325,200), the middle of its words") == 1,
          T.dump())
        -- Load: both clicks at the row.
        T.event()
        local clicks = #S.clicks
        T.tripFrom(row("load_expansion_preset", 0), W)
        T.tick(2)
        T.ok(#S.clicks == clicks + 2 and T.clickAt(clicks + 1) == "325,180" and T.clickAt(clicks + 2) == "325,180", "a double-click")
        T.nameReads("X17 Arp Line 01")
        T.ok(T.st().preset == "X17 Arp Line 01" and T.st().loaded_expansion_preset_index == 0, tostring(T.st().preset))
        -- The same name after the click.
        T.event()
        T.tripFrom(row("select_expansion_preset", 0), W)
        T.nameReads("X17 Arp Line 01")
        T.ok(T.err() == "Selecting a preset: unchanged, X17 Arp Line 01", T.err())
        -- Nothing read: never the database's name.
        T.event()
        T.tripFrom(row("load_expansion_preset", 1), W)
        T.tick(2)
        T.nameReads(nil)
        T.ok(T.err() == "Loading a preset: the preset name cannot be read now" and T.st().preset == nil, T.err())
        -- A preset the page cannot have meant: refused with no trip.
        T.pageMsg({ action = "mark_expansion_preset_favorite", args = { preset_index = 99 }, focused = true })
        T.ok(T.err() == "Mark as favorite: no preset was named", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The page shows every category and the rows read are Arp's; the preset asked is X17 Bass Line 01,
/// in Bass: Bass is found among the categories and clicked first, (165, 222), its rows read, and then
/// the row, (325, 180). The page's category comes back "__all__".
#[test]
fn a_preset_of_another_category_has_its_category_clicked_first() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        T.tripFrom({ action = "select_expansion_preset", args = { preset_index = 2, keep_preset_category = "__all__" },
          focused = true }, W)
        T.settle(4)
        T.ok(table.concat(S.browser.clicked, "|") == "category Bass|preset X17 Bass Line 01", table.concat(S.browser.clicked, "|"))
        T.ok(T.clickAt(1) == "165,222" and T.clickAt(2) == "325,180", T.clickAt(1) .. " " .. T.clickAt(2))
        T.nameReads("X17 Bass Line 01")
        T.ok(T.st().preset == "X17 Bass Line 01" and T.st().selected_preset_category == "__all__",
          tostring(T.st().selected_preset_category))
        T.ok(T.count("the rows read as Arp's, X17 Bass Line 01 is in Bass — its category first") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Strings, thirty presets, its rows 6 to 19 shown, and a wheel that moves the list the other way:
/// the first notch down at the presets' middle (142, 141) — (325, 310) — shows rows 5 to 18 and the
/// second rows 4 to 17, the places read went back two steps running, so the presets' wheel is turned
/// round for the session and the search starts again from there, and eight notches later row 25 is
/// read and clicked at (142, 222.5) — (325, 440).
#[test]
fn a_preset_far_down_is_reached_a_notch_at_a_time_and_a_reversed_wheel_is_turned() {
    run_mac(&scenario(r##"
        local o = T.openOf("Cinematic Pads", { category = "Strings", firstRow = 5, reversed = true })
        local _, _, W = T.browserPage({ mode = "open", open = o })
        local _, _, all = T.listsOf("Cinematic Pads")
        T.ok(all[49] == "X4 Strings Line 25", all[49])
        T.tripFrom({ action = "select_expansion_preset", args = { preset_index = 48, keep_preset_category = "Strings" },
          focused = true }, W)
        T.settle(12)
        T.ok(T.scrollsSince(0) == "325,310,-1 325,310,-1 325,310,1 325,310,1 325,310,1 325,310,1 325,310,1 325,310,1 "
          .. "325,310,1 325,310,1", T.scrollsSince(0))
        T.ok(T.count("the wheel moved the list of presets up two steps running, where it was turned down (the places read "
          .. "29, then 28) — turned round for the session, and the search started again from here") == 1, T.dump())
        T.ok(S.browser.clicked[#S.browser.clicked] == "preset X4 Strings Line 25" and T.clickAt(#S.clicks) == "325,440",
          T.clickAt(#S.clicks))
        T.nameReads("X4 Strings Line 25")
        T.ok(T.st().preset == "X4 Strings Line 25" and T.st().loaded_expansion_preset_index == 48, tostring(T.st().preset))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The page knows Synthwave open, and Avenger shows Dark Techno: choosing a category, selecting a
/// preset and marking one are each refused with nothing clicked, and the expansion Avenger shows is
/// posted as the one open, marked as named by the database.
#[test]
fn another_expansion_shown_refuses_its_rows_and_is_posted_marked() {
    for (action, args, label) in [
        ("select_expansion_category", "{ index = 1, total = 4 }", "Choosing a category"),
        ("select_expansion_preset", "{ preset_index = 0, keep_preset_category = \"Arp\" }", "Selecting a preset"),
        ("mark_expansion_preset_favorite", "{ preset_index = 0 }", "Mark as favorite"),
    ] {
        let s = r##"
            local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
            T.ok(T.st().selected_expansion == "Synthwave", "the key read Synthwave")
            S.browser.open = T.openOf("Dark Techno", { fav = true })
            T.repaint({})
            T.event()
            local clicks = #S.clicks
            T.tripFrom({ action = "@ACTION@", args = @ARGS@, focused = true }, W)
            T.run(2400)
            T.ok(#S.clicks == clicks and T.err() == "@LABEL@: VPS Avenger shows another expansion, Dark Techno, by the "
              .. "presets it lists (from the database)", T.err())
            local st = T.st()
            T.ok(st.selected_expansion == "Dark Techno" and st.selected_expansion_from_database == true
              and st.expansion_open == true, "posted as Dark Techno, marked")
            T.ok(T.held() == 0, "every picture released: " .. T.held())
        "##
        .replace("@ACTION@", action)
        .replace("@ARGS@", args)
        .replace("@LABEL@", label);
        run_mac(&scenario(&s));
    }
}

/// A category click Avenger answers with a hover first and the category only 600 ms later: the
/// first picture that changed still lists the rows of the category before, so it is looked on from
/// until the rows read as the one clicked, and that is posted.
#[test]
fn a_category_click_is_looked_on_from_until_its_rows_change() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave", { categoryLate = 600 }) })
        T.tripFrom({ action = "select_expansion_category", args = { index = 1, total = 4 }, focused = true }, W)
        T.run(2400)
        T.ok(S.browser.clicked[1] == "category Bass", tostring(S.browser.clicked[1]))
        T.ok(T.count("the picture changed and what this action changes did not") >= 1, T.dump())
        T.ok(T.st().selected_preset_category == "Bass" and T.lastPost().kind == "restoreFocus",
          T.err() .. " " .. tostring(T.st().selected_preset_category))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// X4 Shared Name is in Cinematic Pads' Keys and in its Pads: with the rows reading Pads', the name
/// is placed in Pads by the category the other rows read as, and the Pads one is found where it is
/// shown and clicked, (142, 85) — (325, 220), with nothing scrolled.
#[test]
fn a_name_held_twice_is_placed_by_the_category_its_rows_read_as() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Cinematic Pads", { category = "Pads" }) })
        local _, _, all = T.listsOf("Cinematic Pads")
        T.ok(all[22] == "X4 Shared Name" and all[15] == "X4 Shared Name", all[22] .. " " .. all[15])
        T.tripFrom({ action = "select_expansion_preset", args = { preset_index = 21, keep_preset_category = "Pads" },
          focused = true }, W)
        T.ok(#S.scrolls == 0 and S.browser.clicked[#S.browser.clicked] == "preset X4 Shared Name"
          and T.clickAt(#S.clicks) == "325,220", T.clickAt(#S.clicks) .. " " .. T.dump())
        T.nameReads("X4 Shared Name")
        T.ok(T.st().preset == "X4 Shared Name", tostring(T.st().preset))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The step guard counts the steps of one way: a search whose first way took four steps to its end
/// has the guard's whole count for the other. With the guard set to fourteen, Cinematic Pads is found
/// twelve steps up after four down.
#[test]
fn the_step_guard_counts_each_way_afresh() {
    run_mac(&scenario(r##"
        local model = T.grid("List", nil, { firstRow = 12, opens = { ["Cinematic Pads"] = T.openOf("Cinematic Pads") } })
        local _, _, W = T.browserPage(model, { roster = "Factory Standard" })
        S.included["modules/vps-avenger/src/actions/browser.luau"][1].GUARD = 14
        T.tripFrom({ action = "open_expansion", args = { index = 2, page_start = 64 }, focused = true }, W)
        T.run(12000)
        local down, up = 0, 0
        for _, s in ipairs(S.scrolls) do if s[3] < 0 then down += 1 else up += 1 end end
        T.ok(down == 4 and up == 12, down .. " down, " .. up .. " up")
        T.ok(T.count("still moving after") == 0 and T.st().selected_expansion == "Cinematic Pads", T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Not read at either end is said as what it is. A grid whose wheel and track move nothing, Melodic
/// Techno 3 off it: not read, in a list that did not move. Avenger's list read this session, and its
/// first two, Synthwave and Psytrance, misread wherever they are shown: Synthwave cannot be read now —
/// never "not in VPS Avenger's list", which holds it, though no place read lies next to it. Effects, one of Cinematic Pads' categories in the database, misread: it cannot be
/// read now. And Strings, the twelfth in the database, where Avenger lists ten categories and the
/// list moved past its tenth: not in VPS Avenger's list of categories.
#[test]
fn a_name_not_read_is_never_said_to_be_missing_from_avengers_list() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("List", nil, { wheelDead = true, trackDead = true }))
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Melodic Techno 3") }, focused = true }, W)
        T.run(12000)
        T.ok(T.err() == "Opening an expansion: Melodic Techno 3 was not read, and VPS Avenger's list did not move", T.err())
        T.ok(#S.clicks == 2 and S.browser.clicked[1] == "track down" and S.browser.clicked[2] == "track up", "the tracks only")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(&r##"
@OWN@
        local _, _, W = T.browserPage(T.grid("List", own))
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.run(8000)
        T.ok(T.st().expansions_from_avenger == true, "Avenger's list read")
        T.event()
        S.browser.grid.misread = { Synthwave = "Qqqqq Zzzzz", Psytrance = "Wwwww Vvvvv" }
        T.repaint({})
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Synthwave") }, focused = true }, W)
        T.run(12000)
        T.ok(T.err() == "Opening an expansion: Synthwave cannot be read in VPS Avenger's list now", T.err())
        local tiles = 0
        for _, c in ipairs(S.browser.clicked or {}) do if not string.find(c, "track", 1, true) then tiles += 1 end end
        T.ok(tiles == 0 and T.st().expansions_from_avenger == true, "nothing clicked on the grid, Avenger's list kept")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##.replace("@OWN@", OWN)));
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Cinematic Pads", { misread = { Effects = "Qqqqq" } }) })
        T.tripFrom({ action = "select_expansion_category", args = { index = 5, total = 12 }, focused = true }, W)
        T.run(12000)
        T.ok(T.err() == "Choosing a category: Effects cannot be read in VPS Avenger's list of categories now", T.err())
        T.ok(#S.scrolls == 8, "down to the end and back: " .. T.scrollsSince(0))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local cats = T.listsOf("Cinematic Pads")
        local ten = {}
        for i = 1, 10 do ten[i] = cats[i] end
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Cinematic Pads", { categories = ten }) })
        T.ok(#T.st().expansion_categories == 12, "the database's twelve")
        T.tripFrom({ action = "select_expansion_category", args = { index = 11, total = 12 }, focused = true }, W)
        T.run(12000)
        T.ok(T.err() == "Choosing a category: Strings is not in VPS Avenger's list of categories", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Strings, its rows 2 to 15 shown, a wheel that moves the list the other way: the first notch down
/// takes it up to its top, the second moves nothing — the end, so the walk turns — and the first
/// notch up takes it down. Two steps running went the other way: the wheel is turned round and the
/// walk started again from there, down, the way the database's order says, not on up the way it had
/// turned — ten notches later row 25 is read and clicked.
#[test]
fn a_reversed_wheel_met_at_the_lists_end_starts_the_walk_again() {
    run_mac(&scenario(r##"
        local o = T.openOf("Cinematic Pads", { category = "Strings", firstRow = 1, reversed = true })
        local _, _, W = T.browserPage({ mode = "open", open = o })
        T.tripFrom({ action = "select_expansion_preset", args = { preset_index = 48, keep_preset_category = "Strings" },
          focused = true }, W)
        T.run(9000)
        T.ok(T.scrollsSince(0) == "325,310,-1 325,310,-1 325,310,1" .. string.rep(" 325,310,1", 10), T.scrollsSince(0))
        T.ok(T.count("the list of presets moves no further — looking up") == 1
          and T.count("the wheel moved the list of presets down two steps running, where it was turned up (the places read "
          .. "25, then 26) — turned round for the session, and the search started again from here") == 1, T.dump())
        T.ok(S.browser.clicked[#S.browser.clicked] == "preset X4 Strings Line 25" and T.clickAt(#S.clicks) == "325,440",
          T.clickAt(#S.clicks))
        T.nameReads("X4 Strings Line 25")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Mark as favorite: refused with FAV read off. With FAV on, the star at (220, the row's 60) —
/// (450, 180) — clicked, a change wait on the star's box from the picture the row was found in, and
/// the star read after: a favorite, posted by the preset's place. A star that does not change: the
/// note.
#[test]
fn a_favorite_is_marked_by_its_star_and_read_after() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        local clicks = #S.clicks
        T.tripFrom({ action = "mark_expansion_preset_favorite", args = { preset_index = 0 }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Mark as favorite: the favorites view is off in VPS Avenger; turn FAV on first",
          T.err())
        T.event()
        S.browser.open.fav = true
        T.repaint({})
        local waits = T.recordWaits()
        T.tripFrom({ action = "mark_expansion_preset_favorite", args = { preset_index = 0 }, focused = true }, W)
        T.ok(T.clickAt(#S.clicks) == "450,180" and S.browser.clicked[#S.browser.clicked] == "star X17 Arp Line 01", T.clickAt(#S.clicks))
        T.settle(4)
        T.ok(#waits == 1 and waits[1].change.from ~= nil and T.region(waits[1].change.watch[1]) == "442,172,458,188",
          "the star's box: " .. #waits .. " " .. (waits[1] and T.region(waits[1].change.watch[1]) or "none"))
        T.ok(T.st().favorite_preset_states["0"] == true and T.lastPost().kind == "restoreFocus",
          T.jsonEncode(T.st().favorite_preset_states or {}))
        T.event()
        S.browser.open.starDead = true
        T.tripFrom({ action = "mark_expansion_preset_favorite", args = { preset_index = 1 }, focused = true }, W)
        T.run(2400)
        T.ok(T.err() == "Mark as favorite: unchanged, not a favorite", T.err())
        T.ok(T.st().favorite_preset_states["0"] == true and T.st().favorite_preset_states["1"] == false, "both kept")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Mark as favorite with FAV not read — its pixels' grids not answered — is refused with nothing
/// clicked: whether the stars show cannot be told.
#[test]
fn a_favorite_is_not_marked_where_fav_is_not_read() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave", { fav = true }) })
        local cells = rawget(T.host.screen, "cells")
        rawset(T.host.screen, "cells", function(opts, cb)
          -- FAV's box at 80 %, (417, 156)-(457, 172), and nothing else this trip asks above y 176.
          if opts.region[4] <= 176 then return nil, "the scenario's: not answered" end
          return cells(opts, cb)
        end)
        local clicks = #S.clicks
        T.tripFrom({ action = "mark_expansion_preset_favorite", args = { preset_index = 0 }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Mark as favorite: the favorites view cannot be read now", T.err())
        T.ok(T.count("expansion: FAV not read") >= 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A star click that changes nothing of the star while Avenger keeps redrawing its box: every
/// change wait answers a change, the star read after each is the one before, and at the bound the
/// answer is "unchanged" — never the star said as changed.
#[test]
fn a_star_redrawn_but_not_changed_is_said_unchanged() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave", { fav = true, starDead = true }) })
        local stop = 0
        local function again()
          if S.now < stop then
            T.repaint(S.painted.spec)
            S.browserLater[#S.browserLater + 1] = { at = S.now + 150, fn = again }
          end
        end
        T.tripFrom({ action = "mark_expansion_preset_favorite", args = { preset_index = 0 }, focused = true }, W)
        T.ok(S.browser.clicked[#S.browser.clicked] == "star X17 Arp Line 01", tostring(S.browser.clicked[#S.browser.clicked]))
        stop = S.now + 3000
        again()
        T.run(3600)
        T.ok(T.count("the picture changed and the star did not") >= 3, T.dump())
        T.ok(T.err() == "Mark as favorite: unchanged, not a favorite", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A favourite's star lit the tool's blue, not bright: with FAV turned on, X17 Arp Line 02's blue star
/// reads as a favourite, and X17 Arp Line 01's unlit one does not.
#[test]
fn a_star_lit_blue_is_a_favorite() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave", { blueStars = { ["X17 Arp Line 02"] = true } }) })
        T.tripFrom({ action = "toggle_expansion_favorite_view", args = { enabled = true }, focused = true }, W)
        T.settle(4)
        local st = T.st()
        T.ok(st.expansion_favorite_view == true and st.favorite_preset_states["1"] == true
          and st.favorite_preset_states["0"] == false, T.jsonEncode(st.favorite_preset_states or {}))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// All: no click — the name read and placed in the expansion open, its rows read, the page's category
/// "__all__", the focus on All.
#[test]
fn all_reads_the_name_and_the_rows_and_clicks_nothing() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        S.nameText = "X17 Arp Line 02"
        T.tripFrom({ action = "select_all_expansion_presets", args = {}, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == 0 and #S.scrolls == 0, "nothing clicked")
        local st = T.st()
        T.ok(st.preset == "X17 Arp Line 02" and st.loaded_expansion_preset_index == 1 and st.selected_preset_category == "__all__",
          tostring(st.selected_preset_category))
        T.ok(T.lastPost().kind == "focus" and T.lastPost().id == "expansion-category-all", tostring(T.lastPost().id))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Go to Category is the database's: no trip — nothing brought forward, nothing pictured — the
/// category of the name last read posted as the page's, the preset placed, the focus on the
/// categories. And its five reasons, the tool's, in this module's words.
#[test]
fn go_to_category_is_the_databases_and_makes_no_trip() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") }, { words = { preset = "X17 Bass Line 02" } })
        local snaps, from = S.snaps, #S.pageOrder
        T.pageMsg({ action = "go_to_loaded_preset_category", args = {}, focused = true })
        T.ok(S.snaps == snaps and T.pageSince(from) == "post state,post error,post focus", T.pageSince(from))
        T.ok(T.st().selected_preset_category == "Bass" and T.st().loaded_expansion_preset_index == 3
          and T.lastPost().id == "open-expansion-categories", tostring(T.st().selected_preset_category))
        local function keyWith(words)
          T.back(W)
          T.browserWords(words)
          T.openPage()
          T.event()
        end
        for _, case in ipairs({
          { "", "the preset's name has not been read" },
          { "My Own Sound", "the preset loaded is not in the database" },
          { "X1 Arp Line 01", "the preset loaded is in another expansion, Acid Breaks, from the database" },
        }) do
          keyWith({ preset = case[1] })
          T.pageMsg({ action = "go_to_loaded_preset_category", args = {}, focused = true })
          T.ok(T.err() == "Go to Category: " .. case[2], T.err())
        end
    "##));
    // A preset no category of the file holds.
    run_mac(&scenario(r##"
        local files = table.clone(T.BROWSER_FIXTURE)
        local x = T.jsonDecode(files["data/expansions/e17.json"])
        x.categories[4].count = 1
        files["data/expansions/e17.json"] = T.jsonEncode(x)
        T.browserPage({ mode = "open", open = T.openOf("Synthwave") }, { files = files, words = { preset = "X17 Pads Line 02" } })
        T.pageMsg({ action = "go_to_loaded_preset_category", args = {}, focused = true })
        T.ok(T.err() == "Go to Category: the preset loaded has no category in the database", T.err())
    "##));
    run_mac(&scenario(r##"
        T.browserPage({ mode = "open", open = T.openOf("Synthwave") }, { noDatabase = true, words = { preset = "X17 Bass Line 02" } })
        T.pageMsg({ action = "go_to_loaded_preset_category", args = {}, focused = true })
        T.ok(T.err() == "Go to Category: the preset database is not installed", T.err())
    "##));
}

// ---------------------------------------------------------------------------------------------
// Correct open Expansion (the tool's Windows block, the page's Settings section).
// ---------------------------------------------------------------------------------------------

/// The catalog with Dark Techno's second Arp preset named as Synthwave's first, in the index and in
/// its file: what one shared name does to every read that expects an expansion.
const SHARED: &str = r##"
        local files = table.clone(T.BROWSER_FIXTURE)
        local idx = T.jsonDecode(files["data/index.json"])
        idx.expansions[5].presets[2] = "X17 Arp Line 01"
        files["data/index.json"] = T.jsonEncode(idx)
        local dark = T.jsonDecode(files["data/expansions/e5.json"])
        dark.presets[2].name = "X17 Arp Line 01"
        files["data/expansions/e5.json"] = T.jsonEncode(dark)
        T.BROWSER_FIXTURE = files
"##;

/// Synthwave clicked, and Avenger opened Dark Techno, whose rows hold one name Synthwave has too: the
/// open's read and every key after it expect Synthwave, which scores by that one name, so the page
/// keeps saying Synthwave. Correct open Expansion clicks nothing, scrolls nothing, reads the name and
/// ONE picture of the browser — the caption and the two columns once — with no expansion expected,
/// so the expansion the columns list wins: Dark Techno, posted as the database named it, with its
/// lists, the category its rows read as for the page's (another expansion than the page had), and
/// the preset placed in it; the note, and the log names what the page had.
#[test]
fn correct_open_expansion_reads_with_nothing_expected_and_clicks_nothing() {
    run_mac(&scenario(&format!("{SHARED}{}", r##"
        local model = T.grid("8 per Row", nil, { opens = { Synthwave = T.openOf("Dark Techno") } })
        local _, _, W = T.browserPage(model, { files = files })
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Synthwave") }, focused = true }, W)
        T.settle(4)
        T.ok(T.st().selected_expansion == "Synthwave" and T.st().selected_expansion_from_database == nil,
          "taken for Synthwave, the one expected: " .. tostring(T.st().selected_expansion))
        T.event()
        T.back(W)
        T.openPage()
        T.event()
        T.ok(T.st().selected_expansion == "Synthwave", "the key still says Synthwave: " .. tostring(T.st().selected_expansion))
        S.nameText = "X5 Arp Line 01"
        local clicks, scrolls, reads, snaps, ups = #S.clicks, #S.scrolls, #S.recognized, S.snaps, T.ups()
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and #S.scrolls == scrolls and T.ups() == ups, "nothing clicked or scrolled")
        local named = {}
        for i = reads + 1, #S.recognized do
          local n = S.recognized[i].name or "the name"
          named[n] = (named[n] or 0) + 1
        end
        T.ok(named["expansion view"] == 1 and named.categories == 1 and named.presets == 1 and named["browser area"] == nil
          and named["the name"] == 1, T.jsonEncode(named))
        T.ok(S.snaps == snaps + 1, "one picture: " .. (S.snaps - snaps))
        local st = T.st()
        T.ok(st.expansion_open == true and st.selected_expansion == "Dark Techno" and st.selected_expansion_from_database == true,
          "Dark Techno, named by the database: " .. tostring(st.selected_expansion))
        T.ok(table.concat(st.expansion_categories, "|") == "Arp|Bass|Leads|Pads" and st.expansion_presets[2] == "X17 Arp Line 01"
          and st.selected_preset_category == "Arp" and st.expansion_favorite_view == false, "its lists, the category, FAV")
        T.ok(st.preset == "X5 Arp Line 01" and st.loaded_expansion_preset_index == 0, "the name read, placed in it")
        T.ok(T.err() == "Correct open Expansion: corrected to Dark Techno, from the database", T.err())
        T.ok(T.lastPost("error").focus == true, "the note with the focus")
        T.ok(T.count("[avenger] [browser] 'Correct open Expansion': the page had Synthwave; Dark Techno by what it lists (12)")
          == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##)));
}

/// The expansion the page has, read open again: "unchanged", unmarked where it was opened by its
/// words, the page's category — All — kept, and with FAV on the rows' stars. The grid where the page
/// had Synthwave open: no expansion open, posted with the view, and Synthwave's lists and category
/// gone. Nothing settled: "cannot be told now", and every value of the browser posted as not read —
/// never the one the page had. Open, without the database: which one cannot be read. Another window
/// in front at the picture: the trip's give-up, and nothing of the browser read.
#[test]
fn correct_open_expansion_says_unchanged_none_open_or_cannot_tell() {
    run_mac(&scenario(r##"
        local model = T.grid("8 per Row", nil, { opens = { Synthwave = T.openOf("Synthwave") } })
        local _, _, W = T.browserPage(model)
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Synthwave") }, focused = true }, W)
        T.settle(4)
        T.event()
        T.tripFrom({ action = "select_all_expansion_presets", args = {}, focused = true }, W)
        T.settle()
        T.ok(T.st().selected_preset_category == "__all__" and T.st().selected_expansion_from_database == nil, "All")
        T.ok(T.st().expansion_favorite_view == false and T.st().favorite_preset_states == nil, "FAV off")
        T.event()
        -- FAV switched on in Avenger, the second row's star lit.
        S.browser.open.fav, S.browser.open.stars = true, { ["X17 Arp Line 02"] = true }
        T.repaint({})
        T.event()
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        T.settle()
        local st = T.st()
        T.ok(T.err() == "Correct open Expansion: unchanged, Synthwave", T.err())
        T.ok(st.selected_expansion == "Synthwave" and st.selected_expansion_from_database == nil
          and st.selected_preset_category == "__all__" and st.expansion_categories ~= nil, "kept, unmarked")
        T.ok(st.expansion_favorite_view == true and st.favorite_preset_states and st.favorite_preset_states["1"] == true
          and st.favorite_preset_states["0"] == false, "FAV on and the stars: " .. T.jsonEncode(st.favorite_preset_states or {}))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local model = T.grid("8 per Row", nil, { opens = { Synthwave = T.openOf("Synthwave") } })
        local _, _, W = T.browserPage(model)
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Synthwave") }, focused = true }, W)
        T.settle(4)
        T.ok(T.st().selected_expansion == "Synthwave" and T.st().expansion_categories ~= nil
          and T.st().selected_preset_category == "Arp", "Synthwave known open: " .. tostring(T.st().selected_preset_category))
        T.event()
        -- Avenger shows the grid again, which the page has not read.
        S.browser.mode = "grid"
        T.repaint({})
        T.event()
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        T.settle()
        T.ok(T.err() == "Correct open Expansion: no expansion is open in VPS Avenger", T.err())
        local st = T.st()
        T.ok(st.expansion_view == "8 per Row" and st.expansion_open == false and st.selected_expansion == ""
          and st.expansion_categories == nil and st.expansion_presets == nil and st.selected_preset_category == nil,
          "the grid posted, Synthwave's lists and category gone: " .. tostring(st.selected_preset_category))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        T.ok(T.st().selected_expansion == "Synthwave", "the key read Synthwave")
        S.browser.open = { name = "?", categories = {}, presets = {}, firstCategory = 0, firstRow = 0, fav = false, stars = {} }
        T.repaint({})
        T.event()
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        T.settle()
        local st = T.st()
        T.ok(T.err() == "Correct open Expansion: whether an expansion is open in VPS Avenger, and which, cannot be told now",
          T.err())
        T.ok(st.expansion_open == nil and st.selected_expansion == nil and st.expansion_view == nil
          and st.expansion_categories == nil and st.expansion_favorite_view == nil, "not read: "
          .. tostring(st.selected_expansion))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") }, { noDatabase = true })
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        T.settle()
        T.ok(T.err() == "Correct open Expansion: VPS Avenger shows an expansion open; which one cannot be read without the "
          .. "preset database", T.err())
        T.ok(T.st().expansion_open == true and T.st().selected_expansion == "", "open, not named")
    "##));
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        local reads = #S.recognized
        S.onSnapshotWait = function()
          S.onSnapshotWait = nil
          S.listed = { W, T.OTHER }
          S.front, S.chain = T.OTHER, { T.OTHER }
        end
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        T.event()
        T.settle()
        T.ok(T.lastPost("error").text == "Correct open Expansion: another window came to the front; nothing more was done",
          T.lastPost("error").text)
        for i = reads + 1, #S.recognized do
          T.ok(S.recognized[i].name == nil, "nothing of the browser read: " .. tostring(S.recognized[i].name))
        end
        T.ok(T.held() == 0, "the picture released: " .. T.held())
    "##));
}

/// The bound on Correct open Expansion's Luau work, the runtime's trip around it included: 6,089
/// turns when measured (the fixture's twenty expansions scored, the rows placed, the state posted).
const ALIGN_TURNS: u64 = 6100;

/// One press is one picture and one text read of it, answered in the same turn as the arrival —
/// no second read later, which the tool made 0.45 s on where its first settled nothing — and the
/// Luau work of the whole trip, the catalog's scoring included, stays far under a loop over the
/// browser picture's 92,536 pixels at 80 %.
#[test]
fn correct_open_expansion_reads_once_and_waits_for_no_time() {
    let s = r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        S.browser.open = T.openOf("Dark Techno")
        T.repaint({})
        T.event()
        local snaps, after, t0 = S.snaps, #S.after, T.interrupts()
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        local spent = T.interrupts() - t0
        T.ok(T.err() == "Correct open Expansion: corrected to Dark Techno, from the database", "answered at the arrival: " .. T.err())
        T.ok(S.snaps == snaps + 1, "one picture: " .. (S.snaps - snaps))
        T.settle(4)
        T.ok(S.snaps == snaps + 1, "and no other later: " .. (S.snaps - snaps))
        T.ok(spent <= @BOUND@ and spent < 92536, string.format("the trip spent %d turns; it may spend %d", spent, @BOUND@))
        print(string.format("ALIGN TURNS: %d", spent))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##
    .replace("@BOUND@", &(ALIGN_TURNS + 3000).to_string());
    run_mac(&scenario(&s));
}

/// Two expansions that list the same names: Dark Techno's two Arp presets carry Synthwave's names, in
/// the index and in its file. Dark Techno clicked, and Avenger opened Synthwave: the open's read
/// expects Dark Techno, which scores by those names, so the page says Dark Techno. Correct open
/// Expansion, expecting none, finds the two listing what was read as much — the tool took the first
/// of its roster — and names neither: which one is open cannot be read, and every value of the
/// browser the page had for Dark Techno is posted as not read; the log names the two.
#[test]
fn correct_open_expansion_names_neither_of_two_that_list_what_was_read_as_much() {
    run_mac(&scenario(r##"
        local files = table.clone(T.BROWSER_FIXTURE)
        local idx = T.jsonDecode(files["data/index.json"])
        idx.expansions[5].presets[1] = "X17 Arp Line 01"
        idx.expansions[5].presets[2] = "X17 Arp Line 02"
        files["data/index.json"] = T.jsonEncode(idx)
        local dark = T.jsonDecode(files["data/expansions/e5.json"])
        dark.presets[1].name = "X17 Arp Line 01"
        dark.presets[2].name = "X17 Arp Line 02"
        files["data/expansions/e5.json"] = T.jsonEncode(dark)
        T.BROWSER_FIXTURE = files
        local model = T.grid("8 per Row", nil, { opens = { ["Dark Techno"] = T.openOf("Synthwave") } })
        local _, _, W = T.browserPage(model, { files = files })
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
        T.settle(4)
        T.ok(T.st().selected_expansion == "Dark Techno" and T.st().expansion_categories ~= nil
          and T.st().selected_expansion_from_database == nil, "taken for Dark Techno: " .. tostring(T.st().selected_expansion))
        T.event()
        local clicks, scrolls = #S.clicks, #S.scrolls
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and #S.scrolls == scrolls, "nothing clicked or scrolled")
        T.ok(T.err() == "Correct open Expansion: VPS Avenger shows an expansion open; which one cannot be read now", T.err())
        local st = T.st()
        T.ok(st.expansion_open == true and st.selected_expansion == "" and st.selected_expansion_from_database == nil
          and st.expansion_categories == nil and st.expansion_presets == nil and st.selected_preset_category == nil,
          "neither named, Dark Techno's lists gone: " .. tostring(st.selected_expansion))
        T.ok(T.count("two expansions list what was read: Dark Techno and Synthwave, by their presets named (12 each)") == 1,
          T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// Avenger's own list.
// ---------------------------------------------------------------------------------------------

/// Read Avenger's List: the grid moved to its top with change waits only — no text read on the way
/// up — then read and stepped down to its end, every read merged after the longest run it begins
/// with; twelve expansions in Avenger's order, posted in place of the roster as Avenger's, the one
/// read as "Synthwav" marked as the database's spelling; the note and the focus on the search. The
/// key's open and a change of the roster setting show it still; a search then goes by its order. A
/// give-up halfway keeps nothing.
#[test]
fn avengers_list_is_read_from_its_top_down_and_shown_in_place_of_the_roster() {
    run_mac(&scenario(r##"
        local own = { "Synthwave", "Psytrance", "Dark Techno", "Acid Breaks", "Vocal Chops", "Deep House", "Future Bass",
          "Retro Synths", "Bass Monsters", "Electro Pop", "Trap Essentials", "Hardstyle Leads" }
        local model = T.grid("List", own, { firstRow = 4, misread = { Synthwave = "Synthwav" },
          opens = { Synthwave = T.openOf("Synthwave") } })
        local _, _, W = T.browserPage(model)
        local read0, said = #S.recognized, #S.speech
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.run(8000)
        local reads = 0
        for i = read0 + 1, #S.recognized do if S.recognized[i].name == "browser area" then reads += 1 end end
        T.ok(reads == 9, "the first look, the top and seven steps down: " .. reads)
        local st = T.st()
        T.ok(table.concat(st.expansions, "|") == table.concat(own, "|") and st.expansions_from_avenger == true
          and st.expansions_from_database == nil, table.concat(st.expansions or {}, "|"))
        T.ok(st.expansions_marks and st.expansions_marks[1] == true and st.expansions_marks[2] == false and #st.expansions_marks == 12,
          "Synthwave marked")
        local p = T.page().posts
        T.ok(p[#p - 1].kind == "focus" and p[#p - 1].id == "expansion-search-input" and T.err() == "Reading Avenger's list: VPS "
          .. "Avenger lists 12 expansions; this list is VPS Avenger's for this session", T.err())
        local told = 0
        for i = said + 1, #S.speech do
          if S.speech[i].text == "Reading the expansions VPS Avenger lists." and S.speech[i].interrupt == false then told += 1 end
        end
        T.ok(told == 1, "said once, behind the screen reader: " .. T.said(said + 1))
        -- The key again, and the roster setting changed: Avenger's list still.
        T.back(W)
        T.openPage()
        T.event()
        T.ok(T.st().expansions_from_avenger == true and #T.st().expansions == 12, "the key shows it")
        T.setting("roster", "Factory Standard")
        T.ok(T.st().expansions_from_avenger == true and #T.st().expansions == 12, "the roster setting does not replace it")
        -- A search by Avenger's order: Synthwave, its first, is above the rows shown at the end.
        T.tripFrom({ action = "open_expansion", args = { index = 0, page_start = 64 }, focused = true }, W)
        T.run(3000)
        T.ok(T.count("its place 1 lies before the 8 to 12 read") == 1 and T.st().selected_expansion == "Synthwave", T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("List", nil, { firstRow = 4 }))
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.tick(22)
        local scrolls = #S.scrolls
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = T.PAGE_WIN, { T.PAGE_WIN } end
        T.pageKey()
        T.run(4000)
        T.ok(#S.scrolls == scrolls, "nothing more moved")
        local st = T.st()
        T.ok(st.expansions_from_database == true and #st.expansions == 20 and st.expansions_from_avenger == nil, "the roster stays")
        T.ok(S.included["modules/vps-avenger/src/state.luau"][1].avengerList == nil, "nothing kept")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Avenger's list read, then the grid shows an expansion it does not have: the list is dropped, the
/// roster shown again, and the action's note says so.
#[test]
fn a_grid_name_the_session_list_lacks_drops_it() {
    run_mac(&scenario(r##"
        local model = T.grid("List", { "Acid Breaks", "Deep House", "Synthwave" }, { opens = { ["Deep House"] = T.openOf("Deep House") } })
        local _, _, W = T.browserPage(model)
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.run(10000)
        T.ok(table.concat(T.st().expansions, "|") == "Acid Breaks|Deep House|Synthwave", table.concat(T.st().expansions, "|"))
        T.event()
        S.browser.grid.names = { "Acid Breaks", "Deep House", "Psytrance", "Synthwave" }
        T.repaint({})
        T.tripFrom({ action = "open_expansion", args = { index = 1 }, focused = true }, W)
        T.settle(4)
        T.ok(T.err() == "Opening an expansion: VPS Avenger lists Psytrance, which this session's list does not have; the "
          .. "database's list is shown again — read Avenger's list again", T.err())
        T.ok(T.st().expansions_from_database == true and #T.st().expansions == 20 and T.st().selected_expansion == "Deep House",
          "the roster again, Deep House open")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// One tile misread in one read of Avenger's list — Dark Techno read "Qqqqq Zzzzz" after the second
/// step down — is outvoted by the other reads of its place: the list kept is Avenger's twelve, each
/// once, and the log says which place's reads disagreed. Where the grid moves by its track, a page at
/// a time, and the one tile two reads share is misread, the second places nowhere: nothing is kept,
/// and the answer says a part of the list could not be read.
#[test]
fn one_misread_in_avengers_list_neither_doubles_it_nor_is_kept() {
    run_mac(&scenario(&r##"
@OWN@
        local _, _, W = T.browserPage(T.grid("List", own))
        T.misreadAtMove(2, { ["Dark Techno"] = "Qqqqq Zzzzz" })
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.run(8000)
        T.ok(T.count("col 1 'Qqqqq Zzzzz' (no name, kept as read)") == 1, "misread once: " .. T.dump())
        local st = T.st()
        T.ok(table.concat(st.expansions, "|") == table.concat(own, "|") and st.expansions_from_avenger == true,
          table.concat(st.expansions or {}, "|"))
        T.ok(T.err() == "Reading Avenger's list: VPS Avenger lists 12 expansions; this list is VPS Avenger's for this "
          .. "session", T.err())
        T.ok(T.count("place 3 read as 'Dark Techno' 2 times, 'Qqqqq Zzzzz' 1 time — Dark Techno taken") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##.replace("@OWN@", OWN)));
    run_mac(&scenario(&r##"
@OWN@
        local _, _, W = T.browserPage(T.grid("List", own, { wheelDead = true }))
        T.misreadAtMove(1, { ["Vocal Chops"] = "Qqqqq Zzzzz" })
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.run(12000)
        T.ok(T.err() == "Reading Avenger's list: a part of the list could not be read; the database's list stays", T.err())
        T.ok(T.count("a read that overlaps nothing read before — a part of the list may lie unread here") == 1, T.dump())
        local st = T.st()
        T.ok(st.expansions_from_database == true and st.expansions_from_avenger == nil
          and S.included["modules/vps-avenger/src/state.luau"][1].avengerList == nil, "nothing kept")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##.replace("@OWN@", OWN)));
}

/// Read Avenger's List where the read at the top reads no name — every tile there too short to keep —
/// and the reads after it do: the list's top was not read, so nothing is kept, and the answer says a
/// part of the list could not be read.
#[test]
fn avengers_list_whose_top_reads_nothing_is_not_kept() {
    run_mac(&scenario(&r##"
@OWN@
        local misread = {}
        for _, n in ipairs(own) do misread[n] = "qq" end
        local _, _, W = T.browserPage(T.grid("List", own, { misread = misread }))
        T.misreadAtMove(0, nil)
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.run(8000)
        T.ok(T.err() == "Reading Avenger's list: a part of the list could not be read; the database's list stays", T.err())
        T.ok(T.count("the list not read whole (nothing read at the top)") == 1 and #S.scrolls == 1, T.dump())
        T.ok(T.st().expansions_from_avenger == nil, "nothing kept")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##.replace("@OWN@", OWN)));
}

/// Read Avenger's List with a wheel that moves the grid the other way: "up" took it to its end, so the
/// first step "down" reads names before the ones read. The wheel is turned round for the session and
/// the list read again from its real top; the list kept is Avenger's twelve, in its order.
#[test]
fn avengers_list_read_with_a_reversed_wheel_is_read_again_from_its_top() {
    run_mac(&scenario(&r##"
@OWN@
        local _, _, W = T.browserPage(T.grid("List", own, { firstRow = 4, reversed = true }))
        T.tripFrom({ action = "read_expansion_list", args = {}, focused = true }, W)
        T.run(24000)
        T.ok(T.count("the wheel moved the grid up, where it was turned down — turned round for the session, and the list "
          .. "read again from its top") == 1, T.dump())
        T.ok(table.concat(T.st().expansions, "|") == table.concat(own, "|") and T.st().expansions_from_avenger == true,
          table.concat(T.st().expansions or {}, "|"))
        T.ok(T.gridSign() == -1, "turned round: " .. T.gridSign())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##.replace("@OWN@", OWN)));
}

/// Avenger's list gathered (B.gather) from reads of its grid: each read placed by where most of its
/// names were read before, so a tile misread in the overlap places nothing; a read that overlaps
/// nothing is a gap, one that begins before the read before it went backward, one with no tile
/// nothing; each place the name most of its reads read, even over a surer one, the surer on a tie of
/// counts; and a place
/// read as two names as often and as surely, or two places read as one name, is no list.
#[test]
fn avengers_list_is_gathered_by_where_most_of_its_names_were_read_before() {
    run_mac(&scenario(r##"
        T.presets(T.BROWSER_FIXTURE)
        T.avengerAt80("Init Preset")
        local B = S.included["modules/vps-avenger/src/actions/browser.luau"][1]
        local HOW = { Ee = "fuzzy", Xx = "as read" }
        local function read(text)
          local out = {}
          for n in string.gmatch(text, "%S+") do out[#out + 1] = { key = string.lower(n), name = n, how = HOW[n] or "exact" } end
          return out
        end
        local g = B.gather()
        T.ok(g.add(read("Aa Bb Cc Dd Ee")) == "merged", "the first")
        T.ok(g.add(read("Bb Xx Dd Ee Ff")) == "merged", "a misread in the overlap")
        T.ok(g.add(read("Dd Ee Ff Gg Hh")) == "merged", "two steps on")
        T.ok(g.add(read("Ii Jj Kk")) == "gap", "past what was read")
        T.ok(g.add(read("Zz Aa Bb")) == "backward", "before")
        T.ok(g.add(read("")) == "empty", "nothing")
        local names, marks, told = g.result()
        T.ok(table.concat(names, " ") == "Aa Bb Cc Dd Ee Ff Gg Hh", table.concat(names or {}, " "))
        T.ok(marks[5] == true and marks[4] == nil, "Ee the database's spelling")
        T.ok(#told == 1 and told[1] == "place 3 read as 'Cc' 1 time, 'Xx' 1 time — Cc taken", tostring(told[1]))
        local twice = B.gather()
        twice.add(read("Aa Bb Cc Dd Ee"))
        twice.add(read("Cc Dd Ee Aa Gg"))
        local none, why = twice.result()
        T.ok(none == nil and why == "places 1 and 6 both read as Aa", tostring(why))
        -- Twice read as Ee by similarity, once exactly as Gg: the most reads.
        local most = B.gather()
        most.add(read("Aa Bb Ee"))
        most.add(read("Bb Ee Ff"))
        most.add(read("Bb Gg Ff"))
        names = most.result()
        T.ok(names and table.concat(names, " ") == "Aa Bb Ee Ff", table.concat(names or {}, " "))
        local even = B.gather()
        even.add(read("Aa Bb Cc"))
        even.add(read("Bb Qq"))
        none, why = even.result()
        T.ok(none == nil and why == "place 3 read as Cc and as Qq as often", tostring(why))
    "##));
}

// ---------------------------------------------------------------------------------------------
// What it costs, and the calibrating run.
// ---------------------------------------------------------------------------------------------

/// The turns of Luau's loops and calls one read of the grid may spend (T.interrupts): 64 tiles in
/// 8 per Row, eight of them misread, looked up among a catalog of 124 expansions — the most it spent
/// when measured on 2026-10-18, BROWSER_TILES, plus a margin. Fewer than the pixels of the area at
/// 80 % (344 by 269, 92,536 of them), so a loop in Luau over the area's pixels, one turn a pixel, is
/// more than the read may spend; and the same read again, its names looked up already, at most a
/// quarter of it (10,377 turns when measured: the columns, the log, the order).
const BROWSER_TILES: u64 = 49400;

#[test]
fn the_browsers_reads_stay_small_on_the_event_loop() {
    let s = r##"
        local A = { "Amber", "Bright", "Cold", "Dusty", "Electric", "Frozen", "Golden", "Hollow", "Iron", "Jade", "Lunar",
          "Misty", "Neon" }
        local N = { "Keys", "Leads", "Pads", "Plucks", "Drums", "Bass", "Strings", "Voices", "Worlds", "Waves", "Bells" }
        local names = {}
        for i = 1, 124 do names[i] = string.format("%s %s %d", A[(i * 7) % 13 + 1], N[(i * 5) % 11 + 1], i) end
        local files = { }
        local index = { format = 1, exported = "2026-10-18", factoryStandard = {}, expansions = {} }
        for i, n in ipairs(names) do
          local p = string.format("C%03d Pad Line 01", i)
          index.expansions[i] = { name = n, file = "data/e" .. i .. ".json", presets = { p } }
          files["data/e" .. i .. ".json"] = T.jsonEncode({ format = 1, name = n, categories = { { name = "Pads", start = 1, count = 1 } },
            presets = { { name = p, osc = { "Osc" }, macros = { { "", "", "", "", "" } } } } })
        end
        files["data/index.json"] = T.jsonEncode(index)
        T.presets(files)
        local shown, misread = {}, {}
        for i = 1, 64 do shown[i] = names[i <= 62 and i * 2 - 1 or (i - 62) * 2] end
        for i = 1, 8 do
          local n = shown[i * 8]
          misread[n] = string.gsub(n, "e", "c", 1)
        end
        S.browser = T.grid("8 per Row", shown, { misread = misread })
        local header = T.avengerAt80("Init Preset")
        T.paintAvenger(98, 84, 1.6, {})
        local Br = S.included["modules/vps-avenger/src/read/browser.luau"][1]
        local P = S.included["modules/vps-avenger/src/read/picture.luau"][1]
        local pl = P.placer(header:origin(), 1.6, 80)
        local reading = T.browserReads(98, 84, 1.6)["browser area"]
        Br.names()
        local t0 = T.interrupts()
        local tiles = Br.tiles(reading, "8 per Row", pl)
        local spent = T.interrupts() - t0
        t0 = T.interrupts()
        Br.tiles(reading, "8 per Row", pl)
        local again = T.interrupts() - t0
        T.ok(#tiles == 64 and Br.named(tiles) >= 56, "the tiles: " .. #tiles .. ", named " .. Br.named(tiles))
        T.ok(spent <= @BOUND@ and spent < 92536, string.format("the read spent %d turns; it may spend %d", spent, @BOUND@))
        T.ok(again <= spent / 4, string.format("again, %d turns of the first's %d", again, spent))
        print(string.format("BROWSER TILES: %d turns, %d again", spent, again))
    "##
    .replace("@BOUND@", &(BROWSER_TILES + 3000).to_string());
    run_mac(&scenario(&s));
}

/// In a calibrating run, a search of the grid writes `browser-grid-<view>-<zoom>pct.png` once per
/// view and zoom, marked at every tile's point and where a step turns the wheel and clicks the track,
/// and the expansion opened `browser-open-<zoom>pct.png`, marked at FAV, the rows and the stars; a
/// second search in the same view and zoom writes none.
#[test]
fn a_calibrating_run_writes_the_browsers_pictures_once_per_view_and_zoom() {
    run_mac(&scenario(r##"
        local model = T.grid("4 per Row", nil, { opens = { ["Dark Techno"] = T.openOf("Dark Techno", { fav = true }),
          ["Acid Breaks"] = T.openOf("Acid Breaks", { fav = true }) } })
        local _, _, W = T.browserPage(model)
        T.calibrate(true)
        local function pictures()
          local out = {}
          for _, m in ipairs(S.marked) do
            local name = string.match(m.path, "calibration/(browser.+)$")
            if name then out[#out + 1] = { name = name, marks = #m.marks } end
          end
          return out
        end
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
        T.settle(4)
        local got = pictures()
        T.ok(#got == 2 and got[1].name == "browser-grid-4-per-row-80pct.png" and got[1].marks == 16 + 3
          and got[2].name == "browser-open-80pct.png" and got[2].marks == 1 + 2 + 2, T.jsonEncode(got))
        T.ok(T.count("[avenger] [calibrate] browser-grid-4-per-row-80pct picture") == 1, T.dump())
        T.event()
        S.browser.mode = "grid"
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Acid Breaks") }, focused = true }, W)
        T.settle(4)
        T.ok(#pictures() == 2, "the same view and zoom again: none")
    "##));
}
