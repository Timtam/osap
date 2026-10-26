//! VPS Avenger's expansion browser from the page (the plan's slice 9:
//! `modules/vps-avenger/src/actions/browser.luau`, `src/read/browser.luau`, the reads of
//! `src/read/expansion.luau` and `src/read/open.luau`, the tool's tables in `src/tables/pages.luau`,
//! the database's lists in `src/presets.luau`), loaded for real against the scripted host of
//! `overlay_avenger_tests.rs`, with a model of Avenger's browser (`S.browser`, below) that the
//! painted picture and the header's text are made from, and that the module's clicks move.
//!
//! Since the third Mac test of the page (2026-10-25) and the maintainer's decision of that day, the
//! browser goes by the preset database and by POSITION, as the avenger_control tool does: nothing
//! below its header is read as text — the scenarios check that no text read asks for the area, the
//! categories or the preset rows — every expansion, category and preset is clicked at the tool's
//! point for its place, its lists scrolled by the tool's scrollbar clicks, each followed by a change
//! wait and not made again at a point that moved nothing; and the header confirms: its name after an
//! Open (another closed again), the preset's name after Load (another said, and where the lists
//! stand forgotten) — a Select is a preview, after which no name is read, nothing is forgotten and
//! nothing is said of the name. The browser's state is the header's name, a picture test of its
//! title band and of the categories' scrollbar (pixels only), and the user's word: Confirm
//! Expansion View confirms the grid for its window, view and zoom, as the tool's action sets
//! AVGExpansionOpen to 0 — `none` alone, the live log's header, never says the grid, and the
//! user's word never outranks a text read there. The view is chosen in the list its field opens
//! with the left button; Close is not clicked where the browser reads closed, and where nothing
//! tells, the view list its click may open is told by its pixels and closed again. FAV is read
//! before and after its click; a star is clicked at its row and read after a change wait. All
//! clicks nothing; Go to Category is the database's with no trip; Correct open Expansion reads the
//! header once. The gate's line — of every browser action and of the page key — and the path's
//! line are in the log. No picture is held at the end of any of it.
//!
//! And scenarios on the shipped data (the preset database): the four rosters the tool offers, and
//! Quantum State, the 103rd of Nyhdroe's 129 in 8 per Row, reached by the tool's clicks on the
//! grid's scrollbar from a page not known, opened and named by its header.

use super::overlay_avenger_tests::run_mac;

/// The preset database the scenarios use, the page on Avenger with a model of its browser, and the
/// model.
const BROWSER: &str = r##"
local S = T.S
local EX = T.source("modules/vps-avenger/src/tables/expansion.luau")(T.host)
T.EX = EX

-- ── The catalog ──────────────────────────────────────────────────────────────
-- Twenty expansions in the data module's format, NOT the real data: "Cinematic Pads" with twelve
-- categories and a Strings category of thirty presets, and "X4 Shared Name" in two of its
-- categories (Keys and Pads). Each other expansion has Arp, Bass, Leads and Pads, two presets each,
-- named "<code> <category> Line <n>", the code "X" and the expansion's place.
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

-- The shipped preset database (modules/vps-avenger-presets) as the data module's files, each read
-- off the repository when it is first asked for: for the scenarios that run on the real data.
T.SHIPPED = setmetatable({}, { __index = function(_, rel) return T.repoText("modules/vps-avenger-presets/" .. rel) end })

-- Expansion `name`'s lists from the catalog (`files`, the fixture above unless it says): its
-- categories, and each category's presets.
function T.listsOf(name, files)
  files = files or T.BROWSER_FIXTURE
  local idx = T.jsonDecode(files["data/index.json"])
  for _, x in ipairs(idx.expansions) do
    if x.name == name then
      local f = T.jsonDecode(files[x.file])
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
--   grid  { view, names (Avenger's order), pageRows = the first row shown on each of the tool's
--         pages (default { 0 }), page (from 0), firstRow, trackDead = its scrollbar moves nothing,
--         opens = { [name] = an `open` it opens }, caption = what the caption reads (default the
--         view; false: nothing), header = false: something drawn in the header that no text is read
--         in (`none`; default nothing drawn, `blank`), barLit = the categories' scrollbar's place lit
--         (as a view whose tiles reach there might), titleLit = a few light pixels in the title band,
--         viewList: a click on the view field opens the view list — painted over the grid as a
--         popup, never read as text — and a second one closes it (S.browser.listOpen), but not with
--         listStuck }
--   open  T.openOf's: { name, categories, presets = { [category] = names }, category (the one whose
--         presets show), firstCategory, presetPage (from 0, the tool's pages for the category's
--         size), fav, stars = { [preset] = true }, blueStars = { [preset] = true } (lit the tool's
--         blue, not bright), stuck = Close does nothing, categoryDead = a category click selects
--         nothing, header = what the header's name reads instead of `name` (false: nothing
--         recognised, over the title drawn there), titleDrawn = false: no title drawn in the band
--         either, lit = the place (from 1) of the preset row shown lit all over, as Avenger draws the
--         selected one, litReach = how far it reaches up and down from the row's middle (6; 9 reaches
--         into the next row's star box, as Avenger's corner placed a unit or more high does),
--         selectLit = a click on a row draws it lit (`lit`) }
-- S.browserLater: Avenger answering later — { at, fn }, each fn run on the first turn at or after
-- `at`, before the host's answers of that turn.
-- Painted under the popups (T.paintAvenger): each tile and row a small box of its own colour, an
-- open expansion's title in its band and its categories' scrollbar's track, FAV and the stars lit;
-- read as text (T.browserReads, kept with the picture): the caption and the header's name only.
-- Every click the module makes that moves it paints it again.
function T.openOf(name, extra, files)
  local cats, presets = T.listsOf(name, files)
  local o = { name = name, categories = cats, presets = presets, category = cats[1], firstCategory = 0, presetPage = 0,
    fav = false, stars = {} }
  for k, v in pairs(extra or {}) do o[k] = v end
  return o
end

-- The module's own tables of where things lie (src/tables/pages.luau), once it is loaded.
local function Pg() return S.included["modules/vps-avenger/src/tables/pages.luau"][1] end

-- The grid's shape in view `view`: its rows shown, and the table y of shown row r's middle (from 0)
-- and x of column c's, and how far below the middle the words are. 8 per Row as the tool's tiles
-- (23 + 27.5 c, 68 + 20 r), List as its first page (from 73, 36 apart).
function T.gridShape(view)
  if view == "List" then return 1, 5, function(r) return EX.LIST_Y[1] + 36 * r end, function() return EX.LIST_X end, 0 end
  if view == "8 per Row" then
    return 8, 8, function(r) return EX.GRID8_Y0 + EX.GRID8_Y_STEP * r end,
      function(c) return EX.GRID8_X0 + c * EX.GRID8_X_STEP end, 5
  end
  error("the model draws List and 8 per Row only, not " .. tostring(view))
end

-- The tiles shown: { name, index (in the grid's names, from 1), tx, ty (where its words are), row }.
function T.shownTiles()
  local g = S.browser.grid
  local cols, rows, rowY, colX, dy = T.gridShape(g.view)
  local out = {}
  for r = 0, rows - 1 do
    for c = 0, cols - 1 do
      local i = (g.firstRow + r) * cols + c + 1
      local name = g.names[i]
      if name then out[#out + 1] = { name = name, index = i, tx = colX(c), ty = rowY(r) + dy, row = r } end
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
-- The rows of the category shown, on its page: { name, index (in the category, from 1), ty }.
function T.shownPresets()
  local o = S.browser.open
  local list = o.presets[o.category] or {}
  local rows, adjust = Pg().presetRows(o.presetPage, #list)
  local out = {}
  for r = 0, EX.PRESET_VISIBLE_ROWS - 1 do
    local i = rows[r]
    if i ~= nil and list[i + 1] then
      out[#out + 1] = { name = list[i + 1], index = i + 1, ty = EX.PRESET_Y0 + r * EX.PRESET_Y_STEP + adjust }
    end
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
    local g = b.grid
    for _, t in ipairs(T.shownTiles()) do rect(t.tx - 6, t.ty - 9, t.tx + 6, t.ty - 5, colour(t.index)) end
    if g.barLit then rect(9, 70, 15, 210, { 120, 120, 120 }) end
    if g.titleLit then rect(80, 47, 82, 49, { 230, 230, 230 }) end
  else
    local o = b.open
    -- The title's glyphs in its band (luminance 208, strokes a unit wide every 2.5, from y 46 to 51,
    -- as the second Mac test's pictures draw "Factory" and "African House"), and the categories'
    -- scrollbar's track lit over a third of its place.
    if o.titleDrawn ~= false then
      local n = #o.name
      for i = 0, n - 1 do
        local x = 83 - n * 1.25 + i * 2.5
        rect(x, 46, x + 1, 51, { 208, 208, 208 })
      end
    end
    rect(10, 70, 12, 210, { 100, 100, 100 })
    for _, c in ipairs(T.shownCategories()) do rect(18, c.ty - 3, 22, c.ty + 3, colour(c.index)) end
    for _, p in ipairs(T.shownPresets()) do
      if o.lit == p.index then
        local h = o.litReach or 6
        rect(72, p.ty - h, 226, p.ty + h, { 62, 62, 62 })
      end
      rect(74, p.ty - 3, 78, p.ty + 3, colour(p.index + 100))
      -- With FAV on, every row's star drawn dim (read/expansion.luau, E.DRAWN), a marked one lit.
      if o.fav then rect(218, p.ty - 2, 222, p.ty + 2, { 64, 64, 64 }) end
      if o.fav and o.stars[p.name] then rect(215.5, p.ty - 4.5, 224.5, p.ty + 4.5, { 235, 235, 235 }) end
      if o.fav and o.blueStars and o.blueStars[p.name] then rect(215.5, p.ty - 4.5, 224.5, p.ty + 4.5, { 60, 90, 200 }) end
    end
    if o.fav then rect(212 - 12.5, 45, 212 + 12.5, 55, { 220, 220, 220 }) end
  end
end

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

-- What a text read of the browser reads: its header. Nothing below it: the module never asks for
-- it (T.nothingBelowTheHeader), and a read there would answer as the scripted host's default.
function T.browserReads(cx, cy, k)
  local b = S.browser
  if not b then return nil end
  local out = {}
  if b.mode == "grid" then
    local g = b.grid
    local caption = g.caption
    if caption == nil then caption = g.view end
    out["expansion view"] = caption and readingOf({ lineOf(wordsAt(caption, 48, 51, 70, cx, cy, k)) }) or readingOf({})
    if g.header == false then
      out["expansion name"] = readingOf({})
    else
      out["expansion name"] = { status = "blank", text = "", words = {}, lines = {}, skipped = true }
    end
  else
    out["expansion view"] = readingOf({})
    local header = b.open.header
    if header == nil then header = b.open.name end
    out["expansion name"] = header and readingOf({ lineOf(wordsAt(header, 83, 48.5, 110, cx, cy, k)) }) or readingOf({})
  end
  return out
end

-- The grid on page `page` of its tool's pages, and Avenger painted again where it moved: whether
-- it moved.
function T.gridPage(page)
  local g = S.browser.grid
  local rows = g.pageRows or { 0 }
  page = math.clamp(page, 0, #rows - 1)
  if page == (g.page or 0) then return false end
  g.page, g.firstRow = page, rows[page + 1]
  T.repaint(S.painted.spec)
  return true
end

-- The clicks, as Avenger answers them.
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
      -- The tool's scrollbar points: its top a page up, its bottom a page down, and the middle of three
      -- pages the middle one.
      local p = g.page or 0
      local to = ty <= 100 and p - 1 or (ty >= 200 and p + 1 or ((#(g.pageRows or {}) == 3 and ty >= 140 and ty <= 180) and 1 or p))
      note(string.format("scrollbar %g", ty))
      if not g.trackDead then T.gridPage(to) end
      return
    end
    if near(15, 50, 6) then
      note("view")
      if g.viewList and not (b.listOpen and g.listStuck) then
        b.listOpen = not b.listOpen
        T.paintAvenger(cx, cy, k, b.listOpen and { popup = { 8, 52, 80, 166 } } or {})
      end
      return
    end
    for _, t in ipairs(T.shownTiles()) do
      -- A tile takes a click on its words and on its icon above them (from 10 above the words).
      if math.abs(tx - t.tx) <= 12 and ty >= t.ty - 10 and ty <= t.ty + 4 then
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
    note("category scrollbar " .. (ty > 140 and "bottom" or "top"))
    local to = ty > 140 and math.max(0, #o.categories - EX.CATEGORY_VISIBLE_ROWS) or 0
    if to ~= o.firstCategory then
      o.firstCategory = to
      T.repaint(S.painted.spec)
    end
    return
  end
  if math.abs(tx - 228) <= 2 then
    note("preset scrollbar " .. (ty > 140 and "down" or "up"))
    local pages = #Pg().presetPages(#(o.presets[o.category] or {}))
    if #(o.presets[o.category] or {}) <= EX.PRESET_VISIBLE_ROWS then pages = 1 end
    local to = math.clamp(o.presetPage + (ty > 140 and 1 or -1), 0, pages - 1)
    if to ~= o.presetPage then
      o.presetPage = to
      T.repaint(S.painted.spec)
    end
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
        o.category, o.presetPage = c.name, 0
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
        -- With `selectLit`, the row clicked drawn selected, as Avenger draws it.
        if o.selectLit and o.lit ~= p.index then
          o.lit = p.index
          T.repaint(S.painted.spec)
        end
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
  local g = { view = view, names = names or T.BROWSER_NAMES, firstRow = 0, page = 0 }
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
-- The clicks from entry `from` on, as "x,y" joined by spaces.
function T.clicksSince(from)
  local out = {}
  for i = (from or 0) + 1, #S.clicks do out[#out + 1] = T.clickAt(i) end
  return table.concat(out, " ")
end
-- The scenario's time run 150 ms at a time until `done()` says so, at most `ms`.
function T.runUntil(done, ms)
  for _ = 1, math.floor((ms or 12000) / 150) do
    if done() then return true end
    T.run(150)
  end
  return done()
end
-- Whether the browser model's last click was on `what`.
function T.lastClicked(what)
  local c = S.browser and S.browser.clicked
  return c ~= nil and c[#c] == what
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
-- The names of the text reads asked from entry `from` of S.recognized on, counted: an unnamed
-- region (the preset's name) as "the name".
function T.readNames(from)
  local named = {}
  for i = (from or 0) + 1, #S.recognized do
    local n = S.recognized[i].name or "the name"
    named[n] = (named[n] or 0) + 1
  end
  return named
end
-- Nothing below the browser's header read as text since entry `from` of S.recognized, by
-- RECTANGLE, whatever a read is named: no region meets the browser below the header — from the
-- bottom of its caption's and name's regions down to the lists' bottoms, at 80 % from (98, 84) —
-- and every region that meets the browser at all is the caption's or the name's.
function T.nothingBelowTheHeader(from)
  local function at(x, y) return 98 + 1.6 * x, 84 + 1.6 * y end
  local bx1, by1 = at(EX.BROWSER[1], EX.BROWSER[2])
  local bx2 = at(EX.BROWSER[3], 0)
  local _, hy = at(0, math.max(EX.VIEW_TEXT[4], EX.NAME_TEXT[4]))
  local _, ly = at(0, math.max(EX.AREA[4], EX.CATEGORY_TEXT[4], EX.PRESET_TEXT[4], EX.STAR_COLUMN[4]))
  hy = math.ceil(hy)
  local bad = {}
  for i = (from or 0) + 1, #S.recognized do
    local r = S.recognized[i]
    local across = r[3] > bx1 and r[1] < bx2
    local below = across and r[4] > hy and r[2] < ly
    local inHeader = across and r[4] > by1 and r[2] < hy
    if below or (inHeader and r.name ~= "expansion view" and r.name ~= "expansion name") then
      bad[#bad + 1] = string.format("%s %d,%d-%d,%d", tostring(r.name or "the name"), r[1], r[2], r[3], r[4])
    end
  end
  T.ok(#bad == 0, "nothing below the header read as text: " .. table.concat(bad, "; "))
end
-- The state module and the browser's actions, as the module loaded them.
function T.St() return S.included["modules/vps-avenger/src/state.luau"][1] end
-- Back to Avenger with the page key, Avenger in front again.
function T.back(W)
  S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
  T.pageKey()
  T.event()
end
"##;

fn scenario(s: &str) -> String {
    format!("{BROWSER}\n{s}")
}

// ---------------------------------------------------------------------------------------------
// Through src/act.luau: the view, Close and FAV.
// ---------------------------------------------------------------------------------------------

/// Expansion View: the field at (15, 50) — (122, 164) — clicked with the LEFT button, the view list
/// seen painted inside Pp.VIEW_LIST, List's row at (27, 60) — (141, 180) — chosen in it, and 8 per
/// Row's at (27, 154.5) — (141, 331) — another time. This Avenger draws an icon where the view's
/// label was (`Expansions/Avenger 100 closed.png`): the key posts the view stored, the setting's
/// default, and a view chosen is stored as set once the list takes it — the tool's AVGExpansionView —
/// and the grid confirmed for it in the window, as the tool's action sets AVGExpansionOpen to 0.
/// On an Avenger that draws the label, the view read as the one asked: nothing clicked, "unchanged".
/// A popup that never takes its row: the field clicked again.
#[test]
fn the_view_list_is_opened_by_its_field_and_the_view_chosen_is_stored_as_set() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("8 per Row", nil, { caption = false }))
        T.ok(T.st().expansion_view == "8 per Row" and T.st().expansion_view_from == "stored"
          and T.st().expansion_open == false, "the key: the view stored, " .. tostring(T.st().expansion_view_from))
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
        T.ok(T.st().expansion_view == "List" and T.st().expansion_view_from == "set" and T.st().expansion_open == false
          and T.st().selected_expansion == "" and T.lastPost().kind == "restoreFocus",
          "posted: " .. tostring(T.st().expansion_view))
        T.ok(S.settings.expansionView == "List" and T.count("the view stored: List, set") == 1, "stored: "
          .. tostring(S.settings.expansionView))
        local c = T.St().gridConfirmation(W.id)
        T.ok(c and c.view == "List" and c.zoom == 80 and c.how == "set", "the grid confirmed for List: " .. T.jsonEncode(c or {}))
        T.ok(T.count("[menu] ") > 0 and T.count("drawn: armed on 'Expansion View'") >= 1, T.dump())
        T.event()
        item = choose("8 per Row", { 8, 52, 80, 166 }, "8 per Row")
        T.ok(item == "141,331", "8 per Row's row: " .. item)
        T.ok(T.st().expansion_view == "8 per Row" and T.st().expansion_view_from == "set"
          and S.settings.expansionView == "8 per Row", "set: " .. tostring(T.st().expansion_view))
        -- The setting changed in the settings dialog: the view is the setting's, as stored, no longer
        -- said as set — posted at once, and read so again.
        T.event()
        T.setting("expansionView", "4 per Row")
        T.ok(T.st().expansion_view == "4 per Row" and T.st().expansion_view_from == "stored",
          "changed in the dialog: " .. tostring(T.st().expansion_view) .. ", " .. tostring(T.st().expansion_view_from))
        T.setting("expansionView", "8 per Row")
        T.ok(T.st().expansion_view == "8 per Row" and T.st().expansion_view_from == "stored",
          "back in the dialog: " .. tostring(T.st().expansion_view) .. ", " .. tostring(T.st().expansion_view_from))
        -- An Avenger that draws the view's label, reading the one asked: nothing clicked.
        S.browser.grid.caption = nil
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        local clicks = #S.clicks
        T.tripFrom({ action = "set_expansion_view", args = { view = "8 per Row" }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Expansion View: unchanged, Expansion View 8 per Row", T.err())
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
        T.ok(T.count("its popup was left open (refused) — clicking its opener again to close it") == 1, T.dump())
        T.ok(T.clickAt() == "122,164", "the field again: " .. T.clickAt())
        T.repaint({})
        T.settle(4)
        T.ok(T.err() == "Expansion View: the menu did not take the choice", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// An open expansion — its name in the header — refuses a view with nothing clicked: the same point
/// would close it. The grid — the header blank, no title in its band — refuses Close with nothing
/// clicked: that click would open the view list. Where nothing tells — something drawn in the header
/// that no text is read in (`none`), no title in the band, the categories' scrollbar not dark — no
/// view is chosen, and Close is clicked as the tool clicks it (the maintainer's decision of
/// 2026-10-24): the expansion that was open closes, the grid is read after it, and no view list is
/// read where it would be drawn.
#[test]
fn no_view_is_chosen_and_no_expansion_closed_where_the_header_says_otherwise() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        T.ok(T.st().expansion_open == true and T.st().selected_expansion == "Synthwave"
          and T.st().selected_expansion_from_database == nil, "the key: Synthwave open, named by the header")
        local clicks = #S.clicks
        T.tripFrom({ action = "set_expansion_view", args = { view = "List" }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Expansion View: an expansion is open in VPS Avenger; press Close Expansion "
          .. "to close it first", T.err())
        -- Every browser action through src/act.luau logs the gate's line, with its decision.
        T.ok(T.count("gate 'Expansion View': window " .. tostring(W.id) .. " at 98,84, zoom 80 % (factor 1.60); view ") == 1
          and T.count("state open 'Synthwave' from header (the header reads 'Synthwave' — Synthwave open (") >= 1
          and T.count("; nothing clicked — an expansion is open in VPS Avenger; press Close Expansion to close it first") == 1,
          T.dump())
        T.ok(T.count("gate 'the page key': window " .. tostring(W.id) .. " at 98,84, zoom 80 % (factor 1.60); view ") >= 1
          and T.count("; no click — the state posted to the page") >= 1, "the page key's gate: " .. T.dump())
        T.event()
        S.browser = T.grid("8 per Row", nil, { caption = false })
        T.repaint({})
        T.tripFrom({ action = "close_expansion", args = {}, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Close Expansion: no expansion is open in VPS Avenger", T.err())
        T.ok(T.st().expansion_view == "8 per Row" and T.st().expansion_view_from == "stored"
          and T.st().expansion_open == false, "the grid posted")
        -- Nothing tells.
        T.event()
        S.browser = { mode = "open", open = T.openOf("Synthwave", { header = false, titleDrawn = false }),
          grid = { view = "8 per Row", names = T.BROWSER_NAMES, firstRow = 0, page = 0, caption = false } }
        T.repaint({})
        local UNREAD = "whether an expansion is open in VPS Avenger cannot be read now"
        T.tripFrom({ action = "set_expansion_view", args = { view = "List" }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Expansion View: " .. UNREAD, T.err())
        T.ok(T.count("the header reads no name (none); title band 0.0 % lit, list bar ") >= 1
          and T.count(" lit — no title, the bar not dark — not taken as closed") >= 1, T.dump())
        T.event()
        T.tripFrom({ action = "close_expansion", args = {}, focused = true }, W)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "122,164" and S.browser.mode == "grid",
          "clicked as the tool clicks it: " .. T.clickAt())
        T.settle(4)
        T.ok(#S.clicks == clicks + 1 and T.st().expansion_open == false and T.st().expansion_view == "8 per Row"
          and T.lastPost().id == "expansion-search-results", "closed, the grid read: " .. tostring(T.st().expansion_open))
        T.ok(T.count("'Close Expansion': nothing tells whether an expansion is open — clicked as the tool clicks it") == 1
          and T.count("'Close Expansion': the view list by its pixels — the browser right of the list's place changed as "
          .. "well — the browser itself, no list painted over it") == 1, T.dump())
        T.ok(T.count("gate 'Close Expansion': window " .. tostring(W.id) .. " at 98,84, zoom 80 % (factor 1.60); view ") >= 2
          and T.count("; go on, clicked at its point as the tool clicks it — nothing tells") == 1
          and T.count("state not read from nothing (") >= 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A10 part 3 (the maintainer's decision of 2026-10-24): where nothing tells whether an expansion is
/// open and the grid is shown — the header holds something no text is read in, and the categories'
/// scrollbar's place is lit, as a view whose tiles reach there might — Close is clicked as the tool
/// clicks it, and that click opens the view list: told by its PIXELS (the maintainer's decision of
/// 2026-10-25: nothing below the header is read as text) — a filled change of a list's size over its
/// place, against its place's profile before the click, and nothing changed right of it — it is
/// closed again by its field and seen gone after a change wait. The page is told no expansion was
/// open, and gets the grid in the view stored. A list that does not close is said so, still there at
/// the change wait's bound. No text read reaches below the header.
#[test]
fn close_where_nothing_tells_closes_the_view_list_its_click_opened() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("8 per Row", nil, { caption = false, header = false, barLit = true,
          viewList = true }))
        T.ok(T.st().expansion_open == nil, "nothing tells at the key: " .. tostring(T.st().expansion_open))
        local clicks, reads = #S.clicks, #S.recognized
        T.tripFrom({ action = "close_expansion", args = {}, focused = true }, W)
        T.settle(6)
        T.ok(#S.clicks == clicks + 2 and T.clickAt(clicks + 1) == "122,164" and T.clickAt() == "122,164"
          and not S.browser.listOpen and table.concat(S.browser.clicked, ",") == "view,view",
          "Close, the list it opened, its field again: " .. (#S.clicks - clicks))
        -- The popup over (8, 52)-(80, 166) at 80 %: 115 by 182 pixels from (111, 167).
        T.ok(T.count("'Close Expansion': the view list by its pixels — a change of ") == 1
          and T.count(" in the list's place, ") == 1 and T.count(", and nothing right of it — the view list") == 1
          and T.count("the click opened the view list — no expansion was open; its field clicked again to close it") == 1
          and T.count("'Close Expansion': after its field's click, nothing changed in the list's place") == 1
          and T.count("the view list has gone; the grid in the view 8 per Row (stored)") == 1, T.dump())
        T.nothingBelowTheHeader(reads)
        T.ok(T.err() == "Close Expansion: no expansion was open; the view list the click opened is closed again", T.err())
        T.ok(T.st().expansion_open == false and T.st().expansion_view == "8 per Row" and T.st().expansion_view_from == "stored",
          "the grid posted: " .. tostring(T.st().expansion_open))
        -- A list its field does not close.
        T.event()
        S.browser.grid.listStuck = true
        T.tripFrom({ action = "close_expansion", args = {}, focused = true }, W)
        T.settle(3)
        T.run(2400)
        T.ok(#S.clicks == clicks + 4 and S.browser.listOpen, "the field clicked once more, the list still open")
        T.ok(T.err() == "Close Expansion: the click opened the view list, and it is still open", T.err())
        T.nothingBelowTheHeader(reads)
        -- The rule itself, on profiles: a list's size over its place and nothing right of it is the
        -- list; one too small, or the browser right of it changed as well, is not; profiles of
        -- another place are not compared.
        local Br = S.included["modules/vps-avenger/src/read/browser.luau"][1]
        local function axis(n, v, from, to, w)
          local a = { mean = {}, r = {}, g = {}, b = {} }
          for i = 1, n do
            local x = (i >= from and i <= to) and w or v
            a.mean[i], a.r[i], a.g[i], a.b[i] = x, x, x, x
          end
          return a
        end
        local function prof(x, w, h, c1, c2, r1, r2, lit)
          return { x = x, y = 0, w = w, h = h, columns = axis(w, 40, c1 or 0, c2 or -1, lit or 70),
            rows = axis(h, 40, r1 or 0, r2 or -1, lit or 70) }
        end
        local pl = { k = 1.6 }
        local before = { list = prof(0, 168, 210), rest = prof(176, 176, 269) }
        local seen = Br.listSeen(before, { list = prof(0, 168, 210, 5, 120, 13, 195), rest = prof(176, 176, 269) }, pl)
        T.ok(seen == "list", "a list's size: " .. tostring(seen))
        seen = Br.listSeen(before, { list = prof(0, 168, 210, 5, 25, 13, 195), rest = prof(176, 176, 269) }, pl)
        T.ok(seen == "none", "21 columns, narrower than 32: " .. tostring(seen))
        seen = Br.listSeen(before, { list = prof(0, 168, 210, 5, 120, 13, 195), rest = prof(176, 176, 269, 1, 3, 1, 3) }, pl)
        T.ok(seen == "rest", "the browser right of it changed too: " .. tostring(seen))
        seen = Br.listSeen(before, { list = prof(0, 168, 210, 5, 120, 13, 195), rest = prof(176, 176, 269, nil, nil, 1, 3) }, pl)
        T.ok(seen == "rest", "its rows alone, as a few tiles change them: " .. tostring(seen))
        seen = Br.listSeen(before, { list = prof(0, 168, 210, 5, 120, 13, 195, 44), rest = prof(176, 176, 269) }, pl)
        T.ok(seen == "none", "a change of 4, under the fill's 6: " .. tostring(seen))
        T.ok(Br.listSeen(before, { list = prof(2, 168, 210), rest = prof(176, 176, 269) }, pl) == nil
          and Br.listSeen(nil, before, pl) == nil, "another place, or none before: not compared")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Close at (15, 50) — (122, 164) — where the header reads the expansion open: the header read again
/// blank over a title band with no title, and the open expansion is gone from the state — its
/// lists, FAV, the category; the focus on the expansions' list, the tool's. Still open after: the
/// error.
#[test]
fn close_reads_the_header_again_and_forgets_the_open_expansion() {
    run_mac(&scenario(r##"
        local o = T.openOf("Synthwave", { fav = true })
        local _, _, W = T.browserPage({ mode = "open", open = o, grid = { view = "8 per Row", names = T.BROWSER_NAMES,
          firstRow = 0, page = 0, caption = false } })
        local st = T.st()
        T.ok(st.expansion_favorite_view == true and st.expansion_categories ~= nil, "the key read FAV and the lists")
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
/// clicked and read after, and with FAV on the stars of the rows this session's clicks placed — Bass,
/// chosen first — are posted by the preset's place. No expansion open: refused.
#[test]
fn the_favorites_view_is_read_before_and_after_its_click() {
    run_mac(&scenario(r##"
        local o = T.openOf("Synthwave", { stars = { ["X17 Bass Line 01"] = true } })
        local _, _, W = T.browserPage({ mode = "open", open = o })
        T.ok(T.st().expansion_favorite_view == false, "FAV read OFF at the key")
        local clicks = #S.clicks
        T.tripFrom({ action = "toggle_expansion_favorite_view", args = { enabled = false }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "The favorites view: unchanged, FAV OFF", T.err())
        T.event()
        T.tripFrom({ action = "select_expansion_category", args = { index = 1, total = 4 }, focused = true }, W)
        T.settle(4)
        T.ok(S.browser.open.category == "Bass" and T.st().selected_preset_category == "Bass", "Bass chosen")
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
// The browser's state: the header, the picture test, the user's word.
// ---------------------------------------------------------------------------------------------

/// The third Mac test's failure (its log 3200–3204, 3237–3240): the grid in 8 per Row, the header
/// holding something no text is read in (`none`), the caption an icon. The picture test tells the
/// grid — no title in the band, the categories' scrollbar's place dark — so Open goes on, by its
/// place: Cinematic Pads, the roster's fourth, on a grid of one page, clicked at the tool's point
/// (23 + 3 × 27.5, 68) with no scrollbar click, and the header's name after the click confirms it.
/// The gate's line names the window, where Avenger is, the zoom, the view and how it came to be,
/// the name crop's status, size and place, the picture test, the grid's confirmation and the state
/// and its source; the path's line the place and the click. Nothing below the header is read as
/// text, and the wheel is never turned.
#[test]
fn the_grid_whose_header_reads_none_opens_by_place_where_the_picture_says_grid() {
    run_mac(&scenario(r##"
        local opens = { ["Cinematic Pads"] = T.openOf("Cinematic Pads") }
        local _, _, W = T.browserPage(T.grid("8 per Row", nil, { caption = false, header = false, opens = opens }))
        T.ok(T.st().expansion_open == false and T.st().expansion_view == "8 per Row", "the key: the grid, by the picture")
        T.ok(T.count("[avenger] [read] expansion: the header reads no name (none); title band 0.0 % lit, list bar 0.0 % lit "
          .. "— the grid: no expansion open, the view 8 per Row stored") == 1, T.dump())
        local reads = #S.recognized
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Cinematic Pads"), page_start = 0,
          visible_index = 3 }, focused = true }, W)
        T.settle(4)
        T.ok(#S.clicks == 1 and T.clickAt() == T.at80(105.5, 68) and S.browser.clicked[1] == "Cinematic Pads",
          "the tool's point: " .. T.clickAt())
        T.ok(#S.scrolls == 0, "the wheel never turned")
        local st = T.st()
        T.ok(st.expansion_open == true and st.selected_expansion == "Cinematic Pads" and st.selected_expansion_from_database == nil
          and T.lastPost().id == "open-expansion-name", "open, by its header: " .. tostring(st.selected_expansion))
        T.ok(#st.expansion_categories == 12, "its lists, the database's")
        T.ok(T.count("gate 'Opening an expansion': window " .. tostring(W.id) .. " at 98,84, zoom 80 % (factor 1.60); view "
          .. "8 per Row (as stored); name crop none ") == 1, T.dump())
        -- The page key's gate line and Open's.
        T.ok(T.count("; picture: title band 0.0 % lit, list bar 0.0 % lit — the grid; grid confirmed no; state the grid from "
          .. "picture (") == 2 and T.count("; no click — the state posted to the page") == 1
          and T.count("; go on, by its place") == 1, T.dump())
        T.ok(T.count("'Opening an expansion': path place 4 of 20 in the list chosen (page 1 of 1, tile 4, 8 per Row), "
          .. "clicked (" .. T.at80(105.5, 68) .. "), the header confirms it") == 1, T.dump())
        T.nothingBelowTheHeader(reads)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// An expansion open whose name the header does not read — something drawn there, no text
/// recognised — is told by its title band, lit: open, which one not read, never the grid. Open and
/// Expansion View are refused with nothing clicked, and a confirmation of the grid held in the
/// window is forgotten. A few light pixels in the band — neither a title nor none — and the header
/// blank: not taken as closed either.
#[test]
fn a_lit_title_band_is_an_open_expansion_whose_name_is_not_read() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage(T.grid("8 per Row", nil, { caption = false }))
        T.pageMsg({ action = "confirm_expansion_view", args = { view = "8 per Row" }, focused = true })
        T.ok(T.St().gridConfirmation(W.id) ~= nil, "confirmed")
        T.event()
        S.browser = { mode = "open", open = T.openOf("Synthwave", { header = false }) }
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        local st = T.st()
        T.ok(st.expansion_open == true and st.selected_expansion == "" and st.expansion_categories == nil,
          "open, which one not read: " .. tostring(st.expansion_open))
        T.ok(T.count("the grid confirmed is forgotten: the title band is lit") == 1 and T.St().gridConfirmation(W.id) == nil,
          T.dump())
        T.ok(T.count("the header reads no name (none); title band ") >= 1 and T.count("— an expansion's title: an expansion "
          .. "open — which one cannot be read") >= 1, T.dump())
        local clicks = #S.clicks
        T.tripFrom({ action = "open_expansion", args = { index = 0 }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Opening an expansion: an expansion is open in VPS Avenger; press Close "
          .. "Expansion to close it first", T.err())
        T.event()
        T.tripFrom({ action = "set_expansion_view", args = { view = "List" }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and string.find(T.err(), "an expansion is open", 1, true) ~= nil, T.err())
        -- A few light pixels in the band over the grid, the header blank: neither.
        T.event()
        S.browser = T.grid("8 per Row", nil, { caption = false, titleLit = true })
        T.repaint({})
        T.tripFrom({ action = "open_expansion", args = { index = 0 }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Opening an expansion: whether an expansion is open in VPS Avenger cannot be "
          .. "read now", T.err())
        T.ok(T.count("— neither — not taken as closed") >= 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Where the reads do not settle it — the header `none`, the categories' scrollbar's place lit —
/// Open clicks nothing; Confirm Expansion View, the user's word, confirms the grid in this window for
/// the view and the zoom read there (the tool's action_confirm_expansion_view, AVGExpansionOpen = 0),
/// and Open goes on, by its place, the state "from confirmed". The tile's click forgets the
/// confirmation. Its scope, by the state's own functions: another window, another view, another
/// zoom confirm nothing, and a mismatch forgets it; a view changed in the settings dialog makes the
/// stored view another, and Open is refused again.
#[test]
fn a_grid_the_reads_do_not_settle_is_taken_on_the_users_word_for_its_window_view_and_zoom() {
    run_mac(&scenario(r##"
        local opens = { ["Dark Techno"] = T.openOf("Dark Techno") }
        local _, _, W = T.browserPage(T.grid("8 per Row", nil, { caption = false, header = false, barLit = true, opens = opens }))
        T.ok(T.st().expansion_open == nil, "the key: not read")
        local clicks = #S.clicks
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.err() == "Opening an expansion: whether an expansion is open in VPS Avenger cannot be "
          .. "read now", T.err())
        -- The page key's gate line and Open's.
        T.ok(T.count("gate 'Opening an expansion': ") == 1 and T.count("state not read from nothing (") == 2
          and T.count("gate 'the page key': ") == 1 and T.count("nothing clicked — whether an expansion is open") == 1,
          T.dump())
        -- The user's word.
        T.event()
        local from = #S.pageOrder
        T.pageMsg({ action = "confirm_expansion_view", args = { view = "8 per Row" }, focused = true })
        T.ok(#S.clicks == clicks and not string.find(T.pageSince(from), "focus", 1, true), "no trip, no click")
        T.ok(T.st().expansion_open == false and T.st().expansion_view_from == "confirmed", "the grid confirmed posted")
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
        T.settle(4)
        T.ok(T.clickAt() == T.at80(133, 68) and T.st().selected_expansion == "Dark Techno", "by its place: " .. T.clickAt())
        T.ok(T.count("state the grid from confirmed (") == 1 and T.count("the grid confirmed here for 8 per Row at 80 %") >= 1,
          T.dump())
        T.ok(T.count("the grid confirmed is forgotten: Open clicks a tile") == 1 and T.St().gridConfirmation(W.id) == nil, T.dump())
        -- Its scope.
        local St = T.St()
        St.confirmGrid(W.id, "8 per Row", 80, "confirmed")
        T.ok(St.gridConfirmed(W.id + 1, "8 per Row", 80) == nil and St.gridConfirmation(W.id) ~= nil, "another window")
        T.ok(St.gridConfirmed(W.id, "8 per Row", 100) == nil and St.gridConfirmation(W.id) == nil, "another zoom forgets it")
        St.confirmGrid(W.id, "8 per Row", 80, "confirmed")
        T.ok(St.gridConfirmed(W.id, "List", 80) == nil and St.gridConfirmation(W.id) == nil, "another view forgets it")
        St.confirmGrid(W.id, "8 per Row", 80, "confirmed")
        T.ok(St.gridConfirmed(W.id, "8 per Row", 80) == "confirmed", "the same window, view and zoom")
        -- The user's word settles a header nothing is read in, never one a text is read in: an
        -- expansion opened in Avenger's own browser whose name the header misreads stays open,
        -- whatever the title band's test says — and the text, which contradicts the grid, forgets
        -- the word; with the grid's picture it is not read, and the word kept.
        local Br = S.included["modules/vps-avenger/src/read/browser.luau"][1]
        local scope = { window = W.id, zoom = 80 }
        local garbled = { ["expansion name"] = { status = "text", text = "Qxv Zzrk" } }
        for _, look in ipairs({ { verdict = "between", words = "between" }, { verdict = "dark", words = "dark" }, false }) do
          St.confirmGrid(W.id, "8 per Row", 80, "confirmed")
          local st2 = Br.state(garbled, look or nil, scope)
          T.ok(st2 and st2.open == true and st2.from == "header", tostring(look and look.verdict) .. ": a text read is an "
            .. "expansion open, whatever was confirmed: " .. T.jsonEncode(st2 or {}))
          T.ok(St.gridConfirmation(W.id) == nil, "and the confirmation forgotten")
        end
        T.ok(T.count("the grid confirmed is forgotten: the header reads a text, 'Qxv Zzrk'") == 3, T.dump())
        St.confirmGrid(W.id, "8 per Row", 80, "confirmed")
        T.ok(Br.state(garbled, { verdict = "grid", words = "grid" }, scope) == nil, "a text and the grid's picture: not read")
        local none = { ["expansion name"] = { status = "none", text = "" } }
        local blank = { ["expansion name"] = { status = "blank", text = "" } }
        T.ok(Br.state(none, { verdict = "between", words = "between" }, scope).from == "confirmed"
          and Br.state(blank, nil, scope).from == "confirmed", "nothing read: the user's word")
        T.ok(St.gridConfirmed(W.id, "8 per Row", 80) == "confirmed", "kept by what settles nothing")
        -- The user's own Browse Presets in Avenger's browser forgets it with the places
        -- (actions.luau's handler, Bw.forgetPlaces); Confirm Expansion View confirms after forgetting.
        local Bw = S.included["modules/vps-avenger/src/actions/browser.luau"][1]
        Bw.forgetPlaces(W.id, "Browse Presets hands the keyboard to VPS Avenger's browser")
        T.ok(St.gridConfirmation(W.id) == nil and T.count("the grid confirmed is forgotten: Browse Presets hands the "
          .. "keyboard to VPS Avenger's browser") == 1, "Browse Presets forgets the confirmation: " .. T.dump())
        T.ok(Bw.confirmView({ view = "8 per Row" }, W.id, 80) ~= nil and St.gridConfirmed(W.id, "8 per Row", 80) == "confirmed",
          "Confirm Expansion View's own confirmation stands")
        local A = T.repoText("modules/vps-avenger/src/actions.luau")
        local h = string.find(A, "A.HANDLERS.browse_expansion_presets = function(o)", 1, true)
        T.ok(h and string.find(A, "Bw.forgetPlaces(p and p.id, ", h, true) ~= nil, "Browse Presets' handler forgets the places")
        -- The view changed in the dialog: the view stored is another, and the confirmation does not hold.
        T.event()
        S.browser.mode = "grid"
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        T.setting("expansionView", "List")
        clicks = #S.clicks
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
        T.settle()
        T.ok(#S.clicks == clicks and T.count("the grid confirmed for 8 per Row at 80 % is forgotten: the view is List at 80 % "
          .. "now") >= 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Confirm Expansion View as the tool's: no expansion open, none selected, whatever was read before —
/// an expansion read open in the window has its lists, FAV and category gone from the state; nothing
/// clicked, nothing pictured. Without a window the page was shown beside: refused.
#[test]
fn confirm_expansion_view_posts_the_grid_with_no_expansion_open_as_the_tool() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave", { fav = true }) })
        T.ok(T.st().expansion_open == true and T.st().expansion_categories ~= nil, "Synthwave read open")
        local snaps, clicks = S.snaps, #S.clicks
        T.pageMsg({ action = "confirm_expansion_view", args = { view = "List" }, focused = true })
        local st = T.st()
        T.ok(st.expansion_open == false and st.selected_expansion == "" and st.expansion_view == "List"
          and st.expansion_view_from == "confirmed" and st.expansion_categories == nil and st.expansion_favorite_view == nil,
          "the grid posted: " .. tostring(st.expansion_open))
        T.ok(S.snaps == snaps and #S.clicks == clicks, "nothing pictured or clicked")
        local Bw = S.included["modules/vps-avenger/src/actions/browser.luau"][1]
        local set, why = Bw.confirmView({ view = "List" }, nil, 80)
        T.ok(set == nil and why == "the page was not opened from VPS Avenger; press the page key there", tostring(why))
    "##));
}

/// The picture test's thresholds (read/expansion.luau, E.look, measured on the second Mac test's 22
/// pictures of the grid and 3 of an open expansion): a title band at 0.5 % lit or more is an
/// expansion's title; at 0.2 % or less none, and with the list bar at 10 % or less the grid, above
/// it "no title, the bar not dark"; between, neither; the band not read, no verdict. The name's crop
/// ends above the grid's first row of tiles at every zoom from 50 to 200 %, and holds the title's
/// glyphs, from 45 to 51.5, and the title band.
#[test]
fn the_picture_tests_thresholds_and_the_name_crop_above_the_grid() {
    run_mac(&scenario(r##"
        T.browserPage(T.grid("8 per Row"))
        local E = S.included["modules/vps-avenger/src/read/expansion.luau"][1]
        local pl = { rect = function(_, r) return { r[1], r[2], r[3], r[4] } end }
        local function g(v) return v and { share = function() return v end } or nil end
        local function verdict(title, bar) return E.look(pl, { ["title band"] = g(title), ["list bar"] = g(bar) }).verdict end
        T.ok(verdict(0.074, 0.365) == "open" and verdict(0.005, 0) == "open", "a title")
        T.ok(verdict(0.0049, 0) == "between" and verdict(0.0021, 0) == "between", "neither")
        T.ok(verdict(0.002, 0.10) == "grid" and verdict(0, 0.05) == "grid", "the grid")
        T.ok(verdict(0.002, 0.11) == "dark" and verdict(0, nil) == "dark", "no title, the bar not dark or not read")
        T.ok(verdict(nil, 0) == nil, "the band not read")
        T.ok(E.look(pl, { ["title band"] = g(0.03), ["list bar"] = g(0.3) }).words
          == "title band 3.0 % lit, list bar 30.0 % lit — an expansion's title", "the words")
        local N, A, B = EX.NAME_TEXT, EX.AREA, EX.TITLE_BAND
        T.ok(N[2] <= 45 and N[4] >= 52 and N[4] < A[2] and B[1] >= N[1] and B[2] >= N[2] and B[3] <= N[3] and B[4] <= N[4],
          "the crop over the glyphs and the band, above the area")
        for z = 50, 200, 5 do
          local k = z / 50
          T.ok(math.floor(N[4] * k + 0.5) < math.floor(A[2] * k + 0.5), z .. " %: the crop ends above the tiles")
        end
    "##));
}

// ---------------------------------------------------------------------------------------------
// Opening an expansion by its place.
// ---------------------------------------------------------------------------------------------

/// A10 part 6: by its place, as the tool opens it — its index in the list chosen, the view stored
/// and the tool's page tables. All twenty on one page, which does not scroll: no click on the
/// scrollbar, the tile clicked at the tool's point for its place, and the header's name after the
/// click confirms it. Where the header names another, that one is closed again and said so, and the
/// grid's page is not kept. Where the title band is lit after the click and the header reads no
/// name, the one at the place clicked is posted open — by its name alone, as everything the
/// database gives since 2026-10-26, the answer saying the header did not read it — and the reads
/// after it keep it by that place until one names another.
#[test]
fn an_expansion_is_opened_by_its_place_confirmed_by_the_header() {
    run_mac(&scenario(r##"
        local opens = { ["Cinematic Pads"] = T.openOf("Cinematic Pads"), ["Dark Techno"] = T.openOf("Synthwave"),
          ["Deep House"] = T.openOf("Deep House", { header = false }) }
        local _, _, W = T.browserPage(T.grid("8 per Row", nil, { caption = false, opens = opens }))
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Cinematic Pads") }, focused = true }, W)
        T.settle(4)
        T.ok(#S.clicks == 1 and T.clickAt() == T.at80(105.5, 68), "the tool's point: " .. T.clickAt())
        T.ok(T.count("Cinematic Pads placed by its place 4 of 20 in the list chosen: page 1 of 1, tile 4, in the view 8 per "
          .. "Row; 0 click(s) on the scrollbar first") == 1, T.dump())
        T.ok(T.st().expansion_open == true and T.st().selected_expansion == "Cinematic Pads"
          and T.st().selected_expansion_from_database == nil and T.lastPost().id == "open-expansion-name",
          "open, by its header: " .. tostring(T.st().selected_expansion))
        -- The place of another: the header names Synthwave, closed again.
        T.event()
        S.browser.mode = "grid"
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
        T.settle(6)
        T.ok(#S.clicks == 3 and T.clickAt(2) == T.at80(133, 68) and T.clickAt() == "122,164", "Close after it: " .. T.clickAt())
        T.ok(S.browser.mode == "grid" and T.err() == "Opening an expansion: VPS Avenger opened Synthwave where Dark Techno lies "
          .. "by its place in the list chosen; it was closed again", T.err())
        T.ok(T.count("clicked (" .. T.at80(133, 68) .. "), Synthwave in the header, closed again") == 1, T.dump())
        T.ok(T.st().expansion_open == false, "the grid posted")
        -- Its title lit, its name not read: the one at the place clicked, unmarked, and kept by the key.
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Deep House") }, focused = true }, W)
        T.settle(4)
        T.ok(T.st().expansion_open == true and T.st().selected_expansion == "Deep House"
          and T.st().selected_expansion_from_database == nil, "by its place: " .. tostring(T.st().selected_expansion))
        T.ok(T.err() == "Opening an expansion: VPS Avenger shows an expansion open where Deep House lies by its place in the "
          .. "list chosen; its name cannot be read in the header now", T.err())
        T.event()
        T.back(W)
        T.openPage()
        T.event()
        T.ok(T.st().selected_expansion == "Deep House" and T.st().selected_expansion_from_database == nil,
          "the key keeps it by its place: " .. tostring(T.st().selected_expansion))
        T.ok(T.count("— Deep House, the one opened here by its place in the list chosen") >= 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    // A grid of five pages, the List view, whose every tile opens Synthwave: Dark Techno by its place,
    // the header names Synthwave, closed again — and the grid's page this session placed it at is
    // forgotten, so the next Open takes the grid to its top first again rather than click the same
    // tile where the grid may not be what was placed.
    run_mac(&scenario(r##"
        local opens = {}
        for _, n in ipairs(T.BROWSER_NAMES) do opens[n] = T.openOf("Synthwave") end
        local _, _, W = T.browserPage(T.grid("List", nil, { opens = opens }))
        for round = 1, 2 do
          T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Dark Techno") }, focused = true }, W)
          T.run(4000)
          T.settle(10)
          T.ok(T.err() == "Opening an expansion: VPS Avenger opened Synthwave where Dark Techno lies by its place in the "
            .. "list chosen; it was closed again", round .. ": " .. T.err())
          T.ok(T.count("(the grid's page not known: its top first)") == round, round .. ", its top first: " .. T.dump())
          T.ok(T.count("the place is not confirmed — the grid's page is forgotten") == round, T.dump())
          T.event()
          T.repaint({})
          T.back(W)
          T.openPage()
          T.event()
        end
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The tool's tables (src/tables/pages.luau), pinned by its numbers: an expansion's page, its place
/// on the page and the tile's point in each view the tool measured, and the clicks on the scrollbar
/// that take the grid there; a category's row and the end of its list that shows it; and a preset's
/// page, row and correction by its category's size — measured (15, 30, 136) and estimated (14 is one
/// page, 70 the estimate) — the rows a page shows, and the clicks on the presets' scrollbar.
#[test]
fn the_tools_tables_place_an_expansion_a_category_and_a_preset_and_their_scrollbars_clicks() {
    run_mac(&scenario(r##"
        T.avengerAt80("Init Preset")
        local Pg = S.included["modules/vps-avenger/src/tables/pages.luau"][1]
        local function placed(view, index, total)
          local p = Pg.place(view, index, total)
          local at = p and Pg.cell(view, p)
          return p, at and string.format("%.3f,%.3f", at[1], at[2]) or "none"
        end
        local function ys(list)
          local out = {}
          for _, c in ipairs(list) do out[#out + 1] = string.format("%g", c[2]) end
          return table.concat(out, " ")
        end
        -- 8 per Row, 129 and more: three pages from 0, 64 and 96, shown from 0, 32 and 72.
        local p, at = placed("8 per Row", 70, 129)
        T.ok(p.page == 1 and p.start == 64 and p.visible == 38 and p.count == 3 and at == "188.000,148.000",
          "8 per Row 129, 70: " .. at)
        T.ok(ys(Pg.scroll("8 per Row", 0, 1, 3)) == "158" and ys(Pg.scroll("8 per Row", 2, 1, 3)) == "57 57 57 158"
          and ys(Pg.scroll("8 per Row", 2, 0, 3)) == "57 57 57" and ys(Pg.scroll("8 per Row", 0, 2, 3)) == "222 222 222",
          "8 per Row 129's clicks")
        -- 8 per Row, the tool's 124: two pages of 64, the second short of four; two clicks at the bottom.
        p, at = placed("8 per Row", 100, 124)
        T.ok(p.page == 1 and p.visible == 36 and p.shown == 60 and at == "133.000,148.000", "8 per Row 124, 100: " .. at)
        T.ok(ys(Pg.scroll("8 per Row", 0, 1, 2)) == "204 204" and ys(Pg.scroll("8 per Row", 1, 0, 2)) == "76 76",
          "8 per Row 124's clicks")
        -- List: overlapping pages of 5, 4, 5, 4, 5, 5, 4; the last page at 170 and 206.
        p, at = placed("List", 7, 20)
        T.ok(p.page == 1 and p.visible == 2 and p.count == 5 and at == "117.500,161.000", "List 20, 7: " .. at)
        p, at = placed("List", 19, 20)
        T.ok(p.page == 4 and p.visible == 1 and at == "117.500,206.000", "List 20, 19: " .. at)
        T.ok(ys(Pg.scroll("List", 0, 1, 5)) == "99.75" and ys(Pg.scroll("List", 1, 0, 5)) == "58", "List's clicks")
        -- 4 per Row over nine pages, 5 over six, 6 over five: their rows by page.
        p, at = placed("4 per Row", 50, 124)
        T.ok(p.page == 3 and p.visible == 6 and at == "144.375,127.000", "4 per Row 124, 50: " .. at)
        T.ok(ys(Pg.scroll("4 per Row", 0, 3, 9)) == "122", "4 per Row's click")
        p, at = placed("5 per Row", 112, 124)
        T.ok(p.page == 5 and p.visible == 2 and at == "117.500,113.000", "5 per Row 124, 112: " .. at)
        p, at = placed("6 per Row", 60, 124)
        T.ok(p.page == 2 and p.visible == 6 and at == "27.917,103.000", "6 per Row 124, 60: " .. at)
        T.ok(ys(Pg.scroll("6 per Row", 1, 2, 5)) == "162" and ys(Pg.scroll("6 per Row", 3, 2, 5)) == "143",
          "6 per Row's page 3, from before and after")
        -- 3 per Row: pages of nine, the sixth of every eight of six; its rows moved 7 a page.
        p, at = placed("3 per Row", 50, 124)
        T.ok(p.page == 5 and p.start == 45 and p.visible == 5 and at == "189.167,175.000", "3 per Row 124, 50: " .. at)
        T.ok(ys(Pg.scroll("3 per Row", 0, 5, 14)) == "122", "3 per Row's click")
        -- 7 per Row: pages of 49, three for 124.
        p, at = placed("7 per Row", 60, 124)
        T.ok(p.page == 1 and p.visible == 11 and p.count == 3, "7 per Row 124, 60")
        T.ok(ys(Pg.scroll("7 per Row", 0, 1, 3)) == "214 214 214 137" and ys(Pg.scroll("7 per Row", 2, 1, 3)) == "137",
          "7 per Row's middle page")
        -- One page, too short to fill it: at the area's top. Outside the list, or no view: none.
        p, at = placed("8 per Row", 3, 20)
        T.ok(p.count == 1 and at == "105.500,68.000", "one page: " .. at)
        T.ok(Pg.place("8 per Row", 20, 20) == nil and Pg.place("9 per Row", 1, 20) == nil, "outside the list, or no view")
        -- The categories (expansion_category_pos): the first nine at the list's top, the rest at its
        -- bottom, the last on the ninth row; a list of nine or fewer not scrolled.
        local c = Pg.category(1, 12)
        T.ok(c.at == "top" and c.row == 1 and c.point[1] == 42 and c.point[2] == 86, "Bass of twelve")
        c = Pg.category(10, 12)
        T.ok(c.at == "bottom" and c.row == 7 and c.point[2] == 206, "the eleventh of twelve: row 8")
        c = Pg.category(11, 12)
        T.ok(c.at == "bottom" and c.row == 8 and c.point[2] == 226, "the last: the ninth row")
        c = Pg.category(9, 10)
        T.ok(c.at == "bottom" and c.row == 8, "the tenth of ten")
        c = Pg.category(3, 4)
        T.ok(c.at == nil and c.row == 3 and c.point[2] == 126, "four: not scrolled")
        T.ok(Pg.category(4, 4) == nil and Pg.category(-1, 4) == nil, "outside the list")
        T.ok(ys(Pg.categoryScroll("top")) == "66 66" and ys(Pg.categoryScroll("bottom")) == "218 218", "the two clicks")
        -- The presets (_preset_scroll_target_offset): the first fourteen on page 0; thirty, measured.
        local q = Pg.presetPlace(13, 30)
        T.ok(q.page == 0 and q.row == 13 and q.adjust == 0, "the fourteenth of thirty")
        q = Pg.presetPlace(20, 30)
        T.ok(q.page == 1 and q.first == 14 and q.row == 13 and q.adjust == -5 and not q.final, "the 21st of thirty")
        q = Pg.presetPlace(29, 30)
        T.ok(q.page == 3 and q.first == 29 and q.row == 13 and q.adjust == 0 and q.final, "the last of thirty")
        q = Pg.presetPlace(14, 15)
        T.ok(q.page == 1 and q.row == 13 and q.final, "the last of fifteen")
        q = Pg.presetPlace(130, 136)
        T.ok(q.page == 10 and q.first == 127 and q.row == 8 and q.final, "136, measured: the 131st")
        q = Pg.presetPlace(13, 14)
        T.ok(q.page == 0 and q.row == 13, "fourteen: one page")
        local est = Pg.presetPages(70)
        T.ok(#est == 7 and est[2][1] == 14 and est[2][2] == 4 and est[7][4] == true, "70, the estimate: " .. T.jsonEncode(est))
        local rows, adjust = Pg.presetRows(1, 30)
        T.ok(rows[0] == 7 and rows[7] == 14 and rows[13] == 20 and adjust == -5, "page 1 of thirty: rows 0 to 13 show 8th to 21st")
        rows = Pg.presetRows(0, 2)
        T.ok(rows[0] == 0 and rows[1] == 1 and rows[2] == nil, "two presets")
        -- The presets' scrollbar (_ensure_expansion_preset_visible): 65 or fewer back to the top, three
        -- clicks or more, then down a click a page; more than 65 a click a page, by way of the top
        -- from the third page on.
        T.ok(ys(Pg.presetScroll({ page = 0, first = 0 }, Pg.presetPlace(20, 30), 30)) == "58 58 58 227", "30: page 1 from the top")
        T.ok(ys(Pg.presetScroll({ page = 1, first = 14 }, Pg.presetPlace(29, 30), 30)) == "58 58 58 227 227 227",
          "30: page 3 from page 1")
        T.ok(ys(Pg.presetScroll({ page = 0, first = 0 }, Pg.presetPlace(3, 30), 30)) == "", "30: page 0 from the top, none")
        T.ok(ys(Pg.presetScroll({ page = 2, first = 26 }, Pg.presetPlace(5, 136), 136)) == "58 58", "136: back from the third")
        T.ok(ys(Pg.presetScroll({ page = 1, first = 14 }, Pg.presetPlace(60, 136), 136)) == "227 227 227", "136: on down")
        local point, star = Pg.presetPoint(13, -5)
        T.ok(point[1] == 142 and point[2] == 217.5 and star[1] == 220 and star[2] == 217.5, "a row's point and its star's")
    "##));
}

/// On the shipped database (T.SHIPPED): the four lists the tool offers, from the index's `rosters`
/// (DB.roster) — Factory Standard 3, Complete the tool's 124, Nyhdroe 129 and Joyman 63 — one of
/// Joyman's marked as having no presets in the database, and no other list marking any.
#[test]
fn the_four_lists_on_the_shipped_database_are_the_tools() {
    run_mac(&scenario(r##"
        local names = {}
        for i, x in ipairs(T.jsonDecode(T.SHIPPED["data/index.json"]).expansions) do names[i] = x.name end
        T.browserPage(T.grid("8 per Row", names), { files = T.SHIPPED, roster = "Factory Standard" })
        local function counted()
          local st = T.st()
          local none = 0
          for _, v in ipairs(st.expansions_without_data or {}) do if v then none += 1 end end
          return #st.expansions, none
        end
        local n, none = counted()
        T.ok(n == 3 and none == 0, "Factory Standard: " .. n .. ", " .. none .. " without data")
        for _, c in ipairs({ { "Complete", 124, 0 }, { "Nyhdroe", 129, 0 }, { "Joyman", 63, 1 }, { "Factory Standard", 3, 0 } }) do
          T.setting("roster", c[1])
          n, none = counted()
          T.ok(n == c[2] and none == c[3], string.format("%s: %d, %d without data", c[1], n, none))
        end
    "##));
}

/// On the shipped database (T.SHIPPED), the list "Nyhdroe" — one of the tool's personal rosters, all
/// 129 expansions — and Quantum State, its 103rd, between Psy 2 and Retrowave 80s, in 8 per Row:
/// page 3 of 3, shown from the 73rd, its tile the 31st, at (23 + 6 × 27.5, 68 + 3 × 20). The grid
/// stands on its second page, which no click of this session placed: its top first, by the tool's
/// three clicks at 57 — the first moves it, the second does not, and the third is not made — then
/// its three at 222 for the last page, the last of which moves nothing. The tile clicked, the header
/// reads Quantum State, its lists from its file posted. "Looking for …" said once. Preset info on one
/// of its presets says where it is, its oscillator and its macros; the log counts the database.
#[test]
fn an_expansion_on_a_later_page_is_reached_by_the_tools_scrollbar_clicks_on_the_shipped_database() {
    run_mac(&scenario(r##"
        local model = T.grid("8 per Row", {}, { caption = false, pageRows = { 0, 4, 9 }, page = 1, firstRow = 4,
          opens = { ["Quantum State"] = T.openOf("Quantum State", nil, T.SHIPPED) } })
        local _, _, W = T.browserPage(model, { files = T.SHIPPED, roster = "Nyhdroe" })
        local list = T.st().expansions
        S.browser.grid.names = list
        T.repaint({})
        local i = T.indexIn("Quantum State")
        T.ok(#list == 129 and i == 102 and list[i] == "Psy 2" and list[i + 2] == "Retrowave 80s",
          "Nyhdroe: " .. #list .. " expansions, Quantum State at " .. i)
        local said = #S.speech
        T.tripFrom({ action = "open_expansion", args = { index = i, page_start = 96, visible_index = 30 }, focused = true }, W)
        T.run(9000)
        T.settle(6)
        T.ok(T.clicksSince(0) == table.concat({ T.at80(228, 57), T.at80(228, 57), T.at80(228, 222), T.at80(228, 222),
          T.at80(228, 222), T.at80(188, 128) }, " "), "the scrollbar, then the tile: " .. T.clicksSince(0))
        T.ok(T.count("Quantum State placed by its place 103 of 129 in the list chosen: page 3 of 3, tile 31, in the view 8 per "
          .. "Row; 6 click(s) on the scrollbar first (the grid's page not known: its top first)") == 1, T.dump())
        T.ok(T.count("the grid's scrollbar clicked at 57.0 — the list moved") == 1
          and T.count("the grid's scrollbar clicked at 57.0 — the list stood still") == 1
          and T.count("the grid's scrollbar clicked at 222.0 — the list moved") == 2
          and T.count("the grid's scrollbar clicked at 222.0 — the list stood still") == 1, T.dump())
        T.ok(S.browser.clicked[#S.browser.clicked] == "Quantum State", "clicked: " .. tostring(S.browser.clicked[#S.browser.clicked]))
        T.ok(T.said(said + 1) == "Looking for Quantum State in VPS Avenger's list by its place.", T.said(said + 1))
        local st = T.st()
        T.ok(st.expansion_open == true and st.selected_expansion == "Quantum State" and st.selected_expansion_from_database == nil,
          "open, named by its header: " .. tostring(st.selected_expansion))
        T.ok(table.concat(st.expansion_categories, "|") == "Arp|Bass|Bells|Chords|Drums|Effects|Leads|Organ|Pads|Piano & Keys|Plucked",
          "its lists: " .. table.concat(st.expansion_categories or {}, "|"))
        T.ok(#S.scrolls == 0, "the wheel never turned")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
    run_mac(&scenario(r##"
        T.presets(T.SHIPPED)
        local header = T.avengerAt80("AR Analog Pop (Avenger)")
        T.ok(T.info(header, "AR Analog Pop (Avenger)") == "AR Analog Pop (Avenger). Expansion Quantum State, category Arp, "
          .. "1 of 42. 1 oscillator: OSC 1. Macros: Envelope, Saturation, Space FX; buttons: FM, RATE", T.said())
        T.ok(T.count("[avenger] 'Preset info': read 'AR Analog Pop (Avenger)' — Quantum State 1 of 197 (exactly)") == 1, T.dump())
        T.ok(T.count("[avenger] the preset database: 129 expansions, 18446 presets, exported 2026-09-26 18:57") == 1, T.dump())
    "##));
}

/// A give-up during a scrollbar click's change wait — the page key, another application's window,
/// the page's close box — clicks nothing more, claims no open expansion, and holds no picture once
/// the wait has answered.
#[test]
fn a_give_up_mid_scroll_clicks_nothing_more() {
    for end in ["key", "another application", "page closed"] {
        let s = r##"
            local ending = "@END@"
            local model = T.grid("8 per Row", {}, { caption = false, pageRows = { 0, 4, 9 }, trackDead = true })
            local _, _, W = T.browserPage(model, { files = T.SHIPPED, roster = "Nyhdroe" })
            S.browser.grid.names = T.st().expansions
            T.repaint({})
            T.tripFrom({ action = "open_expansion", args = { index = 120 }, focused = true }, W)
            T.tick(2)
            T.ok(#S.clicks == 1, "the first scrollbar click out: " .. #S.clicks)
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
            T.ok(#S.clicks == clicks, ending .. ": nothing more: " .. #S.clicks)
            T.ok(T.lastPost("state").state.expansion_open == false, ending .. ": no open expansion claimed")
            T.ok(T.held() == 0, ending .. ": no picture held: " .. T.held())
        "##
        .replace("@END@", end);
        run_mac(&scenario(&s));
    }
}

// ---------------------------------------------------------------------------------------------
// An open expansion: its categories, its presets, the stars.
// ---------------------------------------------------------------------------------------------

/// Cinematic Pads open, its twelve categories nine at a time. Bass, the second: the list of
/// categories to its top first, where this session's clicks did not leave it — two clicks at (12, 66)
/// — (117, 190), the second not made where the first moved nothing — then its row, (42, 86) — (165,
/// 222); the page's category. Plucks, the eleventh: the list to its bottom, (12, 218) — (117, 433),
/// then the eighth row, (42, 206) — (165, 414), where the bottom end shows it. Arp again: the top
/// first. A click that changes nothing: the note, the category not known to be shown. Nothing below
/// the header read as text, and no wheel.
#[test]
fn a_category_is_clicked_at_its_place_its_list_at_the_end_that_shows_it() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Cinematic Pads") })
        T.ok(T.st().selected_expansion == "Cinematic Pads", "the key: Cinematic Pads open")
        local reads = #S.recognized
        local function choose(name)
          local cats = T.st().expansion_categories
          T.tripFrom({ action = "select_expansion_category", args = { index = table.find(cats, name) - 1, total = #cats },
            focused = true }, W)
          T.run(4400)
          T.settle(4)
        end
        choose("Bass")
        T.ok(T.clicksSince(0) == "117,190 165,222", "the top first, then the row: " .. T.clicksSince(0))
        T.ok(S.browser.open.category == "Bass" and T.st().selected_preset_category == "Bass" and T.err() == "",
          "Bass: " .. T.err())
        T.ok(T.count("'Choosing a category': the list of categories to its top first, for Bass, the 2 of 12") == 1
          and T.count("'Choosing a category': clicking Bass at (165,222), the tool's row 2 of the categories") == 1, T.dump())
        local n = #S.clicks
        T.event()
        choose("Plucks")
        T.ok(T.clicksSince(n) == "117,433 117,433 165,414", "the bottom, then the eighth row: " .. T.clicksSince(n))
        T.ok(S.browser.open.firstCategory == 3 and S.browser.open.category == "Plucks"
          and T.st().selected_preset_category == "Plucks", "Plucks: " .. tostring(S.browser.open.category))
        n = #S.clicks
        T.event()
        choose("Arp")
        T.ok(T.clicksSince(n) == "117,190 117,190 165,190", "the top again, then the first row: " .. T.clicksSince(n))
        T.ok(S.browser.open.category == "Arp", "Arp")
        -- A click that selects nothing.
        n = #S.clicks
        T.event()
        S.browser.open.categoryDead = true
        choose("Bass")
        T.ok(T.clicksSince(n) == "165,222" and T.err() == "Choosing a category: Bass clicked; nothing VPS Avenger shows "
          .. "changed, and its presets cannot be read", T.clicksSince(n) .. " " .. T.err())
        T.nothingBelowTheHeader(reads)
        T.ok(#S.scrolls == 0, "the wheel never turned")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Select is a preview (the avenger_control developer's report of 2026-10-26, on 0.13.0): Avenger
/// lets the preset be heard and loads none — the header's name stays — so of the page's "unchanged,
/// <name>" after it he wrote "Das ist auch nicht nötig. Das kannst du rausnehmen." Synthwave open,
/// X17 Arp Line 01 loaded: X17 Arp Line 02 selected — its category Arp clicked first, (42, 66) —
/// (165, 190), where this session's clicks did not leave it shown, then its row, (142, 72.5) — (325,
/// 200), one click; the presets' change waited for (Avenger draws the row selected), then the strip
/// and the macros read again with their text, as the tool's action_select_expansion_preset
/// refreshes them. No name read before or after the click, no name watched, nothing said — no
/// "unchanged" — and nothing forgotten: the preset and its place stay the ones loaded, the
/// database's position and where the lists stand with them, so the next row's Select clicks only
/// its row. The row selected already: nothing changes in the list, the wait runs to its bound, and
/// still nothing is said. With FAV on, the stars read off the picture the wait answered, as the tool
/// reads the row's star. And a Select where the name is not known — after a Load whose name was not
/// read — says no error either.
#[test]
fn a_preset_selected_is_a_preview_with_no_name_watched_and_nothing_forgotten() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave", { selectLit = true }) },
          { words = { preset = "X17 Arp Line 01" } })
        T.ok(T.st().preset == "X17 Arp Line 01" and T.st().loaded_expansion_preset_index == 0, "the key placed the preset")
        local function row(action, i)
          return { action = action, args = { preset_index = i, visible_index = i, category_preset_index = i, category_count = 2,
            category_index = 0, category_total = 4, keep_preset_category = "Arp" }, focused = true }
        end
        -- The name's reads after a click (main.luau's watch, `vps-avenger name`), counted.
        local function watched()
          local n = 0
          for _, r in ipairs(S.reads) do if r.key == "vps-avenger name" then n += 1 end end
          return n
        end
        local function unchangedPage(what)
          local st = T.st()
          T.ok(st.preset == "X17 Arp Line 01" and st.expansion == "Synthwave" and st.loaded_expansion_preset_index == 0
            and st.selected_preset_category == "Arp" and T.err() == "", what .. ": " .. tostring(st.preset) .. " "
            .. tostring(st.loaded_expansion_preset_index) .. " '" .. tostring(T.err()) .. "'")
        end
        local reads, w0, forgotten = #S.recognized, watched(), T.count("is forgotten")
        T.tripFrom(row("select_expansion_preset", 1), W)
        T.run(1200)
        T.ok(T.clicksSince(0) == "165,190 325,200" and S.browser.clicked[#S.browser.clicked] == "preset X17 Arp Line 02",
          "its category, then the row, one click: " .. T.clicksSince(0))
        unchangedPage("the preset loaded stays")
        T.ok(watched() == w0 and T.readNames(reads)["the name"] == nil, "no name read: " .. (watched() - w0))
        local named = T.readNames(reads)
        T.ok(named["osc 1"] == 1 and named["macro tabs"] == 1, "the strip and the macros read with their text")
        T.ok(T.count("is forgotten") == forgotten, "nothing forgotten: " .. T.dump())
        T.ok(T.count("[avenger] [browser] 'Selecting a preset': clicking X17 Arp Line 02 at (325,200), the tool's row 2 of "
          .. "page 1 of Arp") == 1 and T.count("'Selecting a preset': the presets' list changed after the click") == 1, T.dump())
        -- The next row: Arp is left shown, so its row alone.
        T.event()
        local clicks, snaps = #S.clicks, S.snaps
        T.tripFrom(row("select_expansion_preset", 0), W)
        T.run(1200)
        T.ok(T.clicksSince(clicks) == "325,180", "its row alone: " .. T.clicksSince(clicks))
        local pictures = S.snaps - snaps
        unchangedPage("again")
        -- The row selected already: the wait to its bound, and nothing said.
        T.event()
        clicks = #S.clicks
        T.tripFrom(row("select_expansion_preset", 0), W)
        T.run(1200)
        T.ok(T.count("'Selecting a preset': nothing in the presets' list changed after the click") == 0, "not before the bound")
        T.run(1500)
        T.ok(T.clicksSince(clicks) == "325,180" and T.count("'Selecting a preset': nothing in the presets' list changed "
          .. "after the click") == 1, T.clicksSince(clicks) .. " " .. T.dump())
        unchangedPage("the same row")
        T.ok(watched() == w0 and T.count("is forgotten") == forgotten, "still no name watched, nothing forgotten")
        -- FAV on, X17 Arp Line 02 a favorite: the stars read off the picture the wait answered.
        T.event()
        S.browser.open.fav = true
        S.browser.open.stars["X17 Arp Line 02"] = true
        T.repaint({})
        snaps = S.snaps
        T.tripFrom(row("select_expansion_preset", 1), W)
        T.run(2400)
        local states = T.st().favorite_preset_states or {}
        T.ok(states["1"] == true and states["0"] == false, "the stars: " .. T.jsonEncode(states))
        T.ok(S.snaps - snaps == pictures, "no picture of its own for the stars: " .. (S.snaps - snaps) .. " against "
          .. pictures)
        unchangedPage("FAV on")
        -- A Load whose name is not read: the preset posted as not read. A Select then says nothing of it.
        T.event()
        T.tripFrom(row("load_expansion_preset", 1), W)
        T.tick(2)
        T.nameReads(nil)
        T.ok(T.err() == "Loading a preset: the preset name cannot be read now" and T.st().preset == nil, T.err())
        T.event()
        local lists = T.count("where the lists stand is forgotten")
        T.tripFrom(row("select_expansion_preset", 1), W)
        -- The row the Load's double-click drew selected: the wait to its bound.
        T.run(2400)
        T.ok(T.err() == "" and T.st().preset == nil and T.count("where the lists stand is forgotten") == lists,
          "no error from the name: '" .. tostring(T.err()) .. "'")
        T.nothingBelowTheHeader(reads)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Load, as it was: Synthwave open, X17 Arp Line 02 loaded — Arp clicked first, (42, 66) — (165,
/// 190), where this session's clicks did not leave it shown, nothing scrolled (two presets, page 0),
/// then its row double-clicked, (142, 72.5) — (325, 200), both clicks of one gesture; the name
/// watched as after ◀ and ▶, the strip and the macros read after it, and the name the one asked:
/// confirmed, posted with the database's expansion and its place. The first: Arp is left shown, so
/// only the row, at (325, 180). The same name again: "unchanged". Another name than the one asked:
/// said, and where the lists stand forgotten, so the next row's Load clicks its category again.
/// Nothing read: the error, the preset posted as not read — never the database's name for the row.
#[test]
fn a_preset_is_loaded_at_its_place_and_confirmed_by_its_name() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") }, { words = { preset = "X17 Arp Line 01" } })
        T.ok(T.st().loaded_expansion_preset_index == 0, "the key placed the preset")
        local function row(action, i, keep)
          return { action = action, args = { preset_index = i, visible_index = i, category_preset_index = i, category_count = 2,
            category_index = 0, category_total = 4, keep_preset_category = keep or "Arp" }, focused = true }
        end
        local reads = #S.recognized
        local snaps = S.snaps
        T.tripFrom(row("load_expansion_preset", 1), W)
        T.tick(3)
        T.ok(T.clicksSince(0) == "165,190 325,200 325,200" and S.clicks[#S.clicks].double == 2
          and S.browser.clicked[#S.browser.clicked] == "preset X17 Arp Line 02", "its category, then the row: "
          .. T.clicksSince(0))
        T.nameReads("X17 Arp Line 02")
        local st = T.st()
        T.ok(st.preset == "X17 Arp Line 02" and st.expansion == "Synthwave"
          and st.loaded_expansion_preset_index == 1 and st.selected_preset_category == "Arp" and T.err() == "", tostring(st.preset))
        T.ok(S.snaps >= snaps + 3, "the browser, the strip and the macros pictured")
        T.ok(T.count("[avenger] [browser] 'Loading a preset': double-clicking X17 Arp Line 02 at (325,200), the tool's row 2 "
          .. "of page 1 of Arp") == 1 and T.count("'Loading a preset': the name read is X17 Arp Line 02 — confirmed") == 1,
          T.dump())
        -- The first: Arp left shown — only the row, both clicks of one double-click.
        T.event()
        local clicks = #S.clicks
        T.tripFrom(row("load_expansion_preset", 0), W)
        T.tick(2)
        T.ok(#S.clicks == clicks + 2 and T.clickAt(clicks + 1) == "325,180" and T.clickAt(clicks + 2) == "325,180"
          and S.clicks[clicks + 2].double == 2, "a double-click: " .. T.clicksSince(clicks))
        T.nameReads("X17 Arp Line 01")
        T.ok(T.st().preset == "X17 Arp Line 01" and T.st().loaded_expansion_preset_index == 0, tostring(T.st().preset))
        -- The same name after the double-click.
        T.event()
        T.tripFrom(row("load_expansion_preset", 0), W)
        T.tick(2)
        T.nameReads("X17 Arp Line 01")
        T.ok(T.err() == "Loading a preset: unchanged, X17 Arp Line 01", T.err())
        -- Another name: said, and the lists' place forgotten.
        T.event()
        T.tripFrom(row("load_expansion_preset", 1), W)
        T.tick(3)
        T.nameReads("X17 Bass Line 01")
        T.ok(T.err() == "Loading a preset: VPS Avenger shows X17 Bass Line 01 where X17 Arp Line 02 lies by its place in Arp",
          T.err())
        T.ok(T.count("where the lists stand is forgotten: the preset read after the click is X17 Bass Line 01, not X17 Arp "
          .. "Line 02") == 1, T.dump())
        clicks = #S.clicks
        T.event()
        T.tripFrom(row("load_expansion_preset", 1), W)
        T.tick(3)
        T.ok(T.clicksSince(clicks) == "165,190 325,200 325,200", "its category again: " .. T.clicksSince(clicks))
        T.nameReads("X17 Arp Line 02")
        -- Nothing read: never the database's name.
        T.event()
        T.tripFrom(row("load_expansion_preset", 1), W)
        T.tick(2)
        T.nameReads(nil)
        T.ok(T.err() == "Loading a preset: the preset name cannot be read now" and T.st().preset == nil, T.err())
        -- A preset the page cannot have meant: refused with no trip.
        T.pageMsg({ action = "mark_expansion_preset_favorite", args = { preset_index = 99 }, focused = true })
        T.ok(T.err() == "Mark as favorite: no preset was named", T.err())
        -- Browse Presets hands Avenger's browser to the user: where its lists stand is not known after it.
        T.event()
        T.tripFrom({ action = "browse_expansion_presets", args = {}, focused = true }, W)
        T.ok(T.count("where the lists stand is forgotten: Browse Presets hands the keyboard to VPS Avenger's browser") == 1,
          T.dump())
        T.nothingBelowTheHeader(reads)
    "##));
}

/// Cinematic Pads' Strings, thirty presets, the tool's measured pages for thirty: its 21st, page 1,
/// drawn on the fourteenth row 5 points high. Strings clicked first — the list of categories to its
/// bottom, (12, 218) — (117, 433) — then the ninth row, (42, 226) — (165, 446); its presets at their
/// top after it, so the tool's three clicks at the presets' scrollbar's top, (228, 58) — (463, 177),
/// the first moving nothing and the others not made, and one at its bottom, (228, 227) — (463, 447);
/// the row at (142, 217.5) — (325, 432). Then its 30th, page 3: from page 1, the top three times —
/// once moving, then not — and the bottom three times, the row at (142, 222.5) — (325, 440). A Select
/// is a preview: the preset loaded stays, and nothing is said.
#[test]
fn a_preset_far_down_its_category_is_reached_by_the_tools_scrollbar_clicks() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Cinematic Pads") },
          { words = { preset = "X4 Strings Line 01" } })
        local preset = T.st().preset
        T.ok(preset == "X4 Strings Line 01", "the key read the preset: " .. tostring(preset))
        local function pick(within)
          local st = T.st()
          local k = 0
          for i, c in ipairs(st.expansion_preset_categories) do
            if c == "Strings" then
              k += 1
              if k == within then return i - 1 end
            end
          end
        end
        T.tripFrom({ action = "select_expansion_preset", args = { preset_index = pick(21), keep_preset_category = "Strings" },
          focused = true }, W)
        T.runUntil(function() return T.lastClicked("preset X4 Strings Line 21") end)
        T.ok(T.clicksSince(0) == "117,433 117,433 165,446 463,177 463,447 325,432", "the clicks: " .. T.clicksSince(0))
        T.ok(S.browser.open.presetPage == 1 and S.browser.clicked[#S.browser.clicked] == "preset X4 Strings Line 21",
          "the 21st: " .. tostring(S.browser.clicked[#S.browser.clicked]))
        T.ok(T.count("X4 Strings Line 21 is the 21 of 30 in Strings — page 2, row 14; 4 click(s) on the presets' scrollbar")
          == 1, T.dump())
        T.run(2400)
        T.ok(T.st().preset == preset and T.err() == "", T.err())
        local n = #S.clicks
        T.event()
        T.tripFrom({ action = "select_expansion_preset", args = { preset_index = pick(30), keep_preset_category = "Strings" },
          focused = true }, W)
        T.runUntil(function() return T.lastClicked("preset X4 Strings Line 30") end)
        T.ok(T.clicksSince(n) == "463,177 463,177 463,447 463,447 463,447 325,440", "from page 1: " .. T.clicksSince(n))
        T.ok(S.browser.open.presetPage == 3 and S.browser.clicked[#S.browser.clicked] == "preset X4 Strings Line 30",
          "the 30th: " .. tostring(S.browser.clicked[#S.browser.clicked]))
        T.run(2400)
        T.ok(T.st().preset == preset and T.err() == "", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The page shows every category; the preset asked is X17 Bass Line 01, in Bass: Bass clicked first,
/// (165, 222), and then the row, (325, 180). The page's category comes back "__all__", the preset
/// loaded the one it was (a Select is a preview). Then X17 Arp Line 01: the category this session's
/// clicks left shown is Bass, so Arp is clicked first.
#[test]
fn a_preset_of_another_category_has_its_category_clicked_first() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") },
          { words = { preset = "X17 Arp Line 01" } })
        T.tripFrom({ action = "select_expansion_preset", args = { preset_index = 2, keep_preset_category = "__all__" },
          focused = true }, W)
        T.tick(3)
        T.ok(T.clicksSince(0) == "165,222 325,180", "Bass, then the row: " .. T.clicksSince(0))
        T.run(2400)
        local st = T.st()
        T.ok(st.selected_preset_category == "__all__" and st.preset == "X17 Arp Line 01" and st.loaded_expansion_preset_index == 0
          and T.err() == "", tostring(st.selected_preset_category) .. " " .. tostring(st.preset) .. " '" .. T.err() .. "'")
        local n = #S.clicks
        T.event()
        T.tripFrom({ action = "select_expansion_preset", args = { preset_index = 0, keep_preset_category = "__all__" },
          focused = true }, W)
        T.tick(3)
        T.ok(T.clicksSince(n) == "165,190 325,180", "Arp, then the row: " .. T.clicksSince(n))
        T.run(2400)
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The page knows Synthwave open, and the header reads Dark Techno: choosing a category, selecting
/// a preset and marking one are each refused with nothing clicked, and the expansion Avenger shows
/// is posted as the one open, read.
#[test]
fn another_expansion_shown_refuses_its_rows_and_is_posted_as_read() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave", { fav = true }) })
        local clicks = #S.clicks
        for _, m in ipairs({
          { action = "select_expansion_category", args = { index = 1, total = 4 } },
          { action = "select_expansion_preset", args = { preset_index = 0 } },
          { action = "mark_expansion_preset_favorite", args = { preset_index = 0 } },
        }) do
          -- The page knows Synthwave open; then Avenger shows Dark Techno.
          S.browser.open = T.openOf("Synthwave", { fav = true })
          T.repaint({})
          T.back(W)
          T.openPage()
          T.event()
          T.ok(T.st().selected_expansion == "Synthwave", "the page knows Synthwave")
          S.browser.open = T.openOf("Dark Techno", { fav = true })
          T.repaint({})
          m.focused = true
          T.tripFrom(m, W)
          T.settle()
          T.ok(#S.clicks == clicks and string.find(T.err(), "VPS Avenger shows another expansion, Dark Techno", 1, true) ~= nil,
            m.action .. ": " .. T.err())
          T.ok(T.st().selected_expansion == "Dark Techno" and T.st().selected_expansion_from_database == nil, "posted as read")
          T.event()
        end
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Mark as favorite: refused with FAV read off. With FAV on, Arp clicked first — this session's
/// clicks did not leave it shown — then the star at (220, the row's 60) — (450, 180) — clicked, a
/// change wait on the star's box from the picture before, and the star read after: a favorite,
/// posted by the preset's place. A star that does not change: the note.
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
        T.tick(3)
        T.ok(T.clicksSince(clicks) == "165,190 450,180" and S.browser.clicked[#S.browser.clicked] == "star X17 Arp Line 01",
          T.clicksSince(clicks))
        T.settle(4)
        local w = waits[#waits]
        T.ok(#waits == 2 and w.change.from ~= nil and T.region(w.change.watch[1]) == "442,172,458,188",
          "the star's box: " .. #waits .. " " .. (w and T.region(w.change.watch[1]) or "none"))
        T.ok(T.st().favorite_preset_states["0"] == true and T.lastPost().kind == "restoreFocus",
          T.jsonEncode(T.st().favorite_preset_states or {}))
        T.event()
        S.browser.open.starDead = true
        clicks = #S.clicks
        T.tripFrom({ action = "mark_expansion_preset_favorite", args = { preset_index = 1 }, focused = true }, W)
        T.run(2400)
        T.ok(T.clicksSince(clicks) == "450,200", "Arp left shown: the star only: " .. T.clicksSince(clicks))
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
          -- The star column FAV is read by at 80 %, (442, 173)-(458, 450), and nothing else this trip
          -- asks right of x 440.
          if opts.region[1] >= 440 then return nil, T.E("failed", "the scenario's: not answered") end
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
        T.tick(3)
        T.ok(S.browser.clicked[#S.browser.clicked] == "star X17 Arp Line 01", tostring(S.browser.clicked[#S.browser.clicked]))
        stop = S.now + 3000
        again()
        T.run(3600)
        T.ok(T.count("the picture changed and the star did not") >= 3, T.dump())
        T.ok(T.err() == "Mark as favorite: unchanged, not a favorite", T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A favourite's star lit the tool's blue, not bright: Arp chosen, then FAV turned on, X17 Arp Line
/// 02's blue star reads as a favourite, and X17 Arp Line 01's unlit one does not.
#[test]
fn a_star_lit_blue_is_a_favorite() {
    run_mac(&scenario(r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave", { blueStars = { ["X17 Arp Line 02"] = true } }) })
        T.tripFrom({ action = "select_expansion_category", args = { index = 0, total = 4 }, focused = true }, W)
        T.settle(4)
        T.event()
        T.tripFrom({ action = "toggle_expansion_favorite_view", args = { enabled = true }, focused = true }, W)
        T.settle(4)
        local st = T.st()
        T.ok(st.expansion_favorite_view == true and st.favorite_preset_states["1"] == true
          and st.favorite_preset_states["0"] == false, T.jsonEncode(st.favorite_preset_states or {}))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// FAV told by the star column (read/expansion.luau, E.favorite), as the second Mac test's pictures
/// draw it (`Expansions/Avenger, 100, Open African House,Fav Stars On, Marked Presets 1,4,8,9.png`
/// and `… Fav Stars Off.png`): with FAV on and no preset marked, every row's dim star says ON; with
/// it off, the selected row lit all over — its star's box full, no star — says OFF, and so it does
/// where the lit row reaches into the next row's box (Avenger's corner placed a unit or more high,
/// about a tenth of that box): a star is looked for in its box's middle. With FAV on, the first of
/// the category's two rows lit, the second's star says ON; with a star marked, ON. The grid shown:
/// FAV not read at all.
#[test]
fn fav_is_told_by_the_stars_drawn_never_by_a_lit_row() {
    for (open, fav) in [
        ("{ fav = true }", "true"),
        ("{ fav = false, lit = 1 }", "false"),
        ("{ fav = false, lit = 1, litReach = 9 }", "false"),
        ("{ fav = true, lit = 1, litReach = 9 }", "true"),
        (r#"{ fav = true, lit = 1, stars = { ["X17 Arp Line 02"] = true } }"#, "true"),
    ] {
        run_mac(&scenario(&r##"
            T.browserPage({ mode = "open", open = T.openOf("Synthwave", @OPEN@) })
            local st = T.st()
            T.ok(st.expansion_open == true and st.expansion_favorite_view == @FAV@,
              "FAV: " .. tostring(st.expansion_favorite_view) .. " " .. T.dump())
            T.ok(T.count("[avenger] [read] expansion: FAV — the star column ") == 1, T.dump())
        "##.replace("@OPEN@", open).replace("@FAV@", fav)));
    }
    run_mac(&scenario(r##"
        T.browserPage(T.grid("8 per Row"))
        T.ok(T.st().expansion_favorite_view == nil and T.count("expansion: FAV not read — no expansion reads open") == 1,
          T.dump())
    "##));
}

/// All: no click — the name read and placed in the expansion open, the page's category "__all__",
/// the focus on All.
#[test]
fn all_reads_the_name_and_clicks_nothing() {
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
          { "X1 Arp Line 01", "the preset loaded is in another expansion, Acid Breaks" },
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

/// Correct open Expansion clicks nothing and reads the name and ONE picture of the browser — its
/// header as text, its picture test — with no expansion expected. The page had Synthwave (opened by
/// its place), and the header reads Dark Techno: corrected to it, read, not marked, with its lists
/// and the preset placed in it, the page's category cleared. The same again: "unchanged". The grid:
/// none open, posted with the view. Open with no name read (its title lit): which one cannot be
/// read, every value of the browser's but the open flag posted as not read. Nothing settled:
/// "cannot be told now", and every value posted as not read — never the one the page had. The
/// tool's read of the area's words, twice: nothing below the header is read.
#[test]
fn correct_open_expansion_reads_the_header_once_and_clicks_nothing() {
    run_mac(&scenario(r##"
        local model = T.grid("8 per Row", nil, { caption = false, opens = { Synthwave = T.openOf("Synthwave") } })
        local _, _, W = T.browserPage(model)
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Synthwave") }, focused = true }, W)
        T.settle(4)
        T.ok(T.st().selected_expansion == "Synthwave", "Synthwave opened")
        local function align(open, grid)
          T.event()
          if grid then
            S.browser.mode, S.browser.grid = "grid", grid
          else
            S.browser.mode, S.browser.open = "open", open
          end
          T.repaint({})
          S.nameText = "X5 Arp Line 01"
          local clicks, reads, snaps = #S.clicks, #S.recognized, S.snaps
          T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
          T.settle()
          T.ok(#S.clicks == clicks and #S.scrolls == 0, "nothing clicked or scrolled")
          local named = T.readNames(reads)
          T.ok(named["expansion view"] == 1 and named["expansion name"] == 1 and named["the name"] == 1, T.jsonEncode(named))
          T.nothingBelowTheHeader(reads)
          T.ok(S.snaps == snaps + 1, "one picture: " .. (S.snaps - snaps))
        end
        align(T.openOf("Dark Techno"))
        local st = T.st()
        T.ok(st.expansion_open == true and st.selected_expansion == "Dark Techno" and st.selected_expansion_from_database == nil,
          "Dark Techno, read: " .. tostring(st.selected_expansion))
        T.ok(table.concat(st.expansion_categories, "|") == "Arp|Bass|Leads|Pads" and st.preset == "X5 Arp Line 01"
          and st.loaded_expansion_preset_index == 0 and st.selected_preset_category == nil, "its lists, the preset placed")
        T.ok(T.err() == "Correct open Expansion: corrected to Dark Techno" and T.lastPost("error").focus == true, T.err())
        T.ok(T.count("'Correct open Expansion': the page had Synthwave; the header reads Dark Techno") == 1, T.dump())
        align(T.openOf("Dark Techno"))
        T.ok(T.err() == "Correct open Expansion: unchanged, Dark Techno", T.err())
        align(nil, { view = "8 per Row", names = T.BROWSER_NAMES, firstRow = 0, page = 0, caption = false })
        T.ok(T.err() == "Correct open Expansion: no expansion is open in VPS Avenger" and T.st().expansion_open == false
          and T.st().expansion_view == "8 per Row", T.err())
        align(T.openOf("Synthwave", { header = false }))
        T.ok(T.err() == "Correct open Expansion: VPS Avenger shows an expansion open; which one cannot be read in its header now"
          and T.st().expansion_open == true and T.st().selected_expansion == "" and T.st().expansion_categories == nil, T.err())
        align(nil, { view = "8 per Row", names = T.BROWSER_NAMES, firstRow = 0, page = 0, caption = false, header = false,
          barLit = true })
        T.ok(T.err() == "Correct open Expansion: whether an expansion is open in VPS Avenger, and which, cannot be told now"
          and T.st().expansion_open == nil and T.st().selected_expansion == nil, T.err())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The bound on Correct open Expansion's Luau work, the runtime's trip around it included: 5,590 turns
/// when measured on 2026-10-25, with the expansion named by its header's name — the header read, the
/// picture test, the state, FAV and the state posted.
const ALIGN_TURNS: u64 = 5600;

/// One press is one picture and one text read of it, answered in the same turn as the arrival —
/// no second read later, which the tool made 0.45 s on where its first settled nothing — and the
/// Luau work of the whole trip stays far under a loop over the browser picture's 92,536 pixels at
/// 80 %.
#[test]
fn correct_open_expansion_reads_once_and_waits_for_no_time() {
    let s = r##"
        local _, _, W = T.browserPage({ mode = "open", open = T.openOf("Synthwave") })
        S.browser.open = T.openOf("Dark Techno")
        T.repaint({})
        T.event()
        local snaps, t0 = S.snaps, T.interrupts()
        T.tripFrom({ action = "align_open_expansion", args = {}, focused = true }, W)
        local spent = T.interrupts() - t0
        T.ok(T.err() == "Correct open Expansion: corrected to Dark Techno", "answered at the arrival: " .. T.err())
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

/// The turns of Luau's loops and calls the browser's gate may spend (T.interrupts): the header's name
/// looked up among a catalog of 129 expansions — a poor read taken by similarity, the names' list
/// built the first time — the picture test's two grids and the state: 12,579 turns when measured on
/// 2026-10-25, GATE_TURNS, plus a margin; far fewer than the pixels of the browser's picture at 80 %
/// (92,536 of them), so no loop over pixels runs in Luau; the same read again, its name looked up
/// already, at most half of it.
const GATE_TURNS: u64 = 12600;

#[test]
fn the_browsers_gate_stays_small_on_the_event_loop() {
    let s = r##"
        local A = { "Amber", "Bright", "Cold", "Dusty", "Electric", "Frozen", "Golden", "Hollow", "Iron", "Jade", "Lunar",
          "Misty", "Neon" }
        local N = { "Keys", "Leads", "Pads", "Plucks", "Drums", "Bass", "Strings", "Voices", "Worlds", "Waves", "Bells" }
        local names = {}
        for i = 1, 129 do names[i] = string.format("%s %s %d", A[(i * 7) % 13 + 1], N[(i * 5) % 11 + 1], i) end
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
        local header = T.avengerAt80("Init Preset")
        S.browser = { mode = "open", open = { name = names[100], header = string.gsub(names[100], "e", "c", 1), categories = { "Pads" },
          presets = { Pads = { "C100 Pad Line 01" } }, category = "Pads", firstCategory = 0, presetPage = 0, fav = false, stars = {} } }
        T.paintAvenger(98, 84, 1.6, {})
        local Br = S.included["modules/vps-avenger/src/read/browser.luau"][1]
        local P = S.included["modules/vps-avenger/src/read/picture.luau"][1]
        local pl = P.placer(header:origin(), 1.6, 80)
        local reads = T.browserReads(98, 84, 1.6)
        local spent, again, st = T.handled(function()
          local snap = T.host.screen.snapshot({ region = { 106, 148, 471, 468 } })
          local t0 = T.interrupts()
          local first = Br.state(reads, Br.look(snap, pl), {})
          local n = T.interrupts() - t0
          t0 = T.interrupts()
          Br.state(reads, Br.look(snap, pl), {})
          local m = T.interrupts() - t0
          snap:release()
          return n, m, first
        end)
        T.ok(st and st.open == true and st.name == names[100], "the name by similarity: " .. tostring(st and st.name))
        T.ok(spent <= @BOUND@ and spent < 92536, string.format("the gate spent %d turns; it may spend %d", spent, @BOUND@))
        T.ok(again <= spent / 2, string.format("again, %d turns of the first's %d", again, spent))
        print(string.format("GATE TURNS: %d turns, %d again", spent, again))
    "##
    .replace("@BOUND@", &(GATE_TURNS + 3000).to_string());
    run_mac(&scenario(&s));
}

/// In a calibrating run, an Open writes `browser-grid-<view>-<zoom>pct.png` once per view and zoom,
/// marked at the tile it clicks, and the expansion opened `browser-open-<zoom>pct.png`, marked at FAV;
/// a second Open in the same view and zoom writes none.
#[test]
fn a_calibrating_run_writes_the_browsers_pictures_once_per_view_and_zoom() {
    run_mac(&scenario(r##"
        local model = T.grid("8 per Row", nil, { caption = false, opens = { ["Dark Techno"] = T.openOf("Dark Techno", { fav = true }),
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
        T.ok(#got == 2 and got[1].name == "browser-grid-8-per-row-80pct.png" and got[1].marks == 1
          and got[2].name == "browser-open-80pct.png" and got[2].marks == 1, T.jsonEncode(got))
        T.ok(T.count("[avenger] [calibrate] browser-grid-8-per-row-80pct picture") == 1, T.dump())
        T.event()
        S.browser.mode = "grid"
        T.repaint({})
        T.back(W)
        T.openPage()
        T.event()
        T.tripFrom({ action = "open_expansion", args = { index = T.indexIn("Acid Breaks") }, focused = true }, W)
        T.settle(4)
        -- The key's own picture of the browser (read/open.luau, Open.PICTURES) at the open between.
        local after = pictures()
        T.ok(#after == 3 and after[3].name == "browser-80pct.png", "the same view and zoom again: none of the "
          .. "actions' — " .. T.jsonEncode(after))
    "##));
}
