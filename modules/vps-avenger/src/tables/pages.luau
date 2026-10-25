-- VPS Avenger's expansion browser by position, as the avenger_control project goes through it: where
-- an expansion lies in the grid by its place in the list chosen, where an open expansion's category
-- lies in its list of categories, and where a preset lies in its category's list — for every action
-- of the browser's (src/actions/browser.luau). Since the maintainer's decision of 2026-10-25 nothing
-- in the browser below its header is read as text: the expansions, the categories and the presets
-- come from the preset database (modules/vps-avenger-presets), their places from these tables, and
-- the header's names confirm what a click did.
--
-- SOURCE: the avenger_control project's actions.py — _expansion_list_page_starts,
-- _expansion_8_page_starts and _expansion_8_visible_start, _expansion_6/5/4/3_page_starts and
-- their counts, _expansion_page_for_index, _click_expansion_page_scroll,
-- action_select_expansion_category, _preset_scroll_target_offset with its measured table,
-- _estimated_preset_scroll_specs and _ensure_expansion_preset_visible — and its
-- clicker/header_map.py — expansion_view_shape, expansion_cell_pos, expansion_list_cell_pos,
-- expansion_scroll_pos, expansion_list_scroll_pos, expansion_category_pos, expansion_preset_pos and
-- the scrollbars' EXPANSION_*_SCROLL_* points — taken over with its developer's permission (NOTICE,
-- 2026-10-12): at 50 % from Avenger's corner, as every table of this module. The numbers are the
-- project's, measured on its rosters: the 8 per Row layout for 129 expansions and more, three
-- pages; 6, 5 and 4 per Row for five, six and nine pages; and where the presets of a category of 15
-- to 63, 72, 75 to 77, 96, 100, 101, 109 and 136 lie once its list is scrolled. A list of another
-- size takes the project's general rule (the grid's pages one view's worth apart, the scrollbar
-- placed by the page's share of them; the presets' estimate), which it did not measure. Nothing
-- here is taken for done: the expansion clicked is confirmed by the header's name after the click,
-- a preset selected or loaded by the preset's name read after it.
--
-- Pages are counted from 0, places in the list from 0, as the project counts them.

local X = host.include("src/tables/expansion.luau")

local Pg = {}

-- The scrollbar's x and its points (the project's EXPANSION_SCROLL_*, EXPANSION_8_LARGE_SCROLL_*,
-- EXPANSION_7/6/5/4/3_SCROLL_*, EXPANSION_LIST_SCROLL_*).
Pg.SCROLL_X = 228
local SCROLL = { top = 76, bottom = 204, middleFromTop = 162, middleFromBottom = 118 }
local EIGHT = { top = 57, middle = 158, bottom = 222 }
local SEVEN = { top = 66, middle = 137, bottom = 214 }
local SIX = { 63, 122, { next = 162, back = 143 }, 193, 217 }
local FIVE = { 62, 100, 127, 154, 181, 208 }
local FOUR = { 61, 86, 104, 122, 140, 158, 176, 194, 212 }
local THREE = { first = 58, second = 74, step = 12, last = 218 }
local LIST_SCROLL = { first = 59, last = 222, firstBack = 58 }
-- The List view's rows, by page in a cycle of seven (EXPANSION_LIST_PAGE_Y_CYCLE), and its counts.
local LIST_YS = {
  { 73, 109, 145, 181, 217 },
  { 89, 125, 161, 197 },
  { 68, 104, 140, 176, 212 },
  { 83, 119, 155, 191 },
  { 62, 98, 134, 170, 206 },
  { 78, 114, 150, 186, 222 },
  { 93, 129, 165, 201 },
}
local LIST_COUNTS = { 5, 4, 5, 4, 5, 5, 4 }
-- The rows' y of 4, 5 and 6 per Row by page (expansion_cell_pos).
local FOUR_YS = {
  { 67, 109, 151, 193 }, { 93, 135, 177, 219 }, { 109, 151, 193 }, { 85, 127, 169, 211 }, { 101, 143, 185 },
  { 75, 117, 159, 201 }, { 92, 134, 176, 218 }, { 108, 150, 192 }, { 122, 164, 206 },
}
local FIVE_YS = {
  { 73, 107, 141, 175, 209 }, { 100, 134, 168, 202 }, { 95, 129, 163, 197 }, { 90, 124, 158, 192 },
  { 85, 119, 153, 187, 221 }, { 113, 147, 181, 215 },
}
local SIX_YS = {
  { 70, 98, 126, 154, 182, 210 }, { 112, 140, 168 }, { 75, 103, 131, 159, 187, 215 }, { 120, 148, 176, 204 },
  { 182, 210 },
}

-- The columns and the rows a view shows (expansion_view_shape): List 1 by 5, "n per Row" n by n.
function Pg.shape(view)
  if view == "List" then return 1, 5 end
  local n = tonumber(string.match(tostring(view or ""), "^(%d) per Row$")) or 8
  n = math.clamp(n, 1, 8)
  return n, n
end

local function listStarts(total)
  if total <= 0 then return { 0 } end
  local starts, pos, i = { 0 }, 0, 0
  while pos + LIST_COUNTS[i % 7 + 1] < total do
    pos += LIST_COUNTS[i % 7 + 1]
    starts[#starts + 1] = pos
    i += 1
  end
  return starts
end

local function under(list, total)
  local out = {}
  for _, s in ipairs(list) do
    if s < total then out[#out + 1] = s end
  end
  if #out == 0 then out[1] = 0 end
  return out
end

local function threeCount(page) return page % 8 == 5 and 6 or 9 end

local function threeStarts(total)
  if total <= 0 then return { 0 } end
  local starts, pos, page = { 0 }, 0, 0
  while pos + threeCount(page) < total do
    pos += threeCount(page)
    starts[#starts + 1] = pos
    page += 1
  end
  return starts
end

-- The page starts of `view` for a list of `total`, or nil for a view whose pages are one view's
-- worth apart (1, 2 and 7 per Row).
local function starts(view, total)
  if view == "List" then return listStarts(total) end
  if view == "8 per Row" then
    if total >= 129 then return { 0, 64, 96 } end
    local out = {}
    for s = 0, math.max(1, total) - 1, 64 do out[#out + 1] = s end
    return #out > 0 and out or { 0 }
  end
  if view == "6 per Row" then return under({ 0, 36, 54, 90, 114 }, total) end
  if view == "5 per Row" then return under({ 0, 25, 45, 65, 85, 110 }, total) end
  if view == "4 per Row" then return under({ 0, 16, 32, 44, 60, 72, 88, 104, 116 }, total) end
  if view == "3 per Row" then return threeStarts(total) end
  return nil
end

-- 8 per Row's first expansion shown on `page` (_expansion_8_visible_start).
local function eightShown(page, total)
  if total >= 129 then return ({ 0, 32, 72 })[math.clamp(page, 0, 2) + 1] end
  return math.max(0, page) * 64
end

-- Where the expansion at place `index` (from 0) of a list of `total` lies in view `view`: { page,
-- start = the page's first place, visible = its place among those shown, count = the pages,
-- shown = how many the page shows }, or nil for a place outside the list or a view Avenger lacks.
function Pg.place(view, index, total)
  if not (X.viewMenuPoint(view) and index >= 0 and index < total) then return nil end
  local cols, rows = Pg.shape(view)
  local list = starts(view, total)
  local page, start, visible, count, shown
  if view == "List" then
    page = 0
    for i, s in ipairs(list) do
      local n = LIST_COUNTS[(i - 1) % 7 + 1]
      if s <= index and index < s + n then
        page = i - 1
        break
      end
      if s <= index then page = i - 1 end
    end
    start = list[page + 1]
    visible, count = index - start, #list
    shown = math.min(LIST_COUNTS[page % 7 + 1], total - start)
  elseif list then
    page = 0
    for i, s in ipairs(list) do
      local e = list[i + 1] or total
      if s <= index and index < e then
        page = i - 1
        break
      end
      if s <= index then page = i - 1 end
    end
    start, count = list[page + 1], #list
    if view == "8 per Row" then
      local first = eightShown(page, total)
      visible, shown = index - first, math.max(0, math.min(64, total - first))
    else
      visible = index - start
      shown = math.max(0, (list[page + 2] or total) - start)
    end
  else
    local size = cols * rows
    page = index // size
    start, visible = page * size, index - page * size
    count = math.max(1, (total + size - 1) // size)
    shown = math.min(size, total - start)
  end
  return { page = page, start = start, visible = visible, count = count, shown = shown }
end

-- The scrollbar's point for page `page` of `count` in a grid view (expansion_scroll_pos): the
-- middle of three pages as the project reached it from the top (`dir` 1) or the bottom (-1).
local function scrollAt(page, count, dir)
  if count == 3 and page == 1 then
    if dir > 0 then return SCROLL.middleFromTop end
    if dir < 0 then return SCROLL.middleFromBottom end
  end
  local t = count <= 1 and 0 or page / (count - 1)
  return SCROLL.top + t * (SCROLL.bottom - SCROLL.top)
end

-- The List view's point for page `page` of `count` (expansion_list_scroll_pos).
local function listScrollAt(page, count, dir)
  local y
  if count <= 1 then
    y = LIST_SCROLL.first
  else
    page = math.min(page, count - 1)
    y = LIST_SCROLL.first + page / (count - 1) * (LIST_SCROLL.last - LIST_SCROLL.first)
  end
  if dir < 0 then
    y -= 1
    if page <= 0 then y = LIST_SCROLL.firstBack end
  end
  return y
end

-- The clicks on the scrollbar that take the grid in view `view` from page `from` to page `to` of
-- `count`, the project's (_click_expansion_page_scroll): a list of table points, each clicked in
-- turn — the project's repeated clicks at one point listed as many times.
function Pg.scroll(view, from, to, count)
  local dir = to > from and 1 or -1
  local ys = {}
  local function at(y, n)
    for _ = 1, n or 1 do ys[#ys + 1] = y end
  end
  if view == "List" then
    at(listScrollAt(to, count, dir))
  elseif view == "8 per Row" and count == 3 then
    if to <= 0 then
      at(EIGHT.top, from >= 2 and 3 or 1)
    elseif to == 1 then
      if from >= 2 then at(EIGHT.top, 3) end
      at(EIGHT.middle)
    else
      at(EIGHT.bottom, from <= 0 and 3 or 2)
    end
  elseif view == "3 per Row" then
    at(math.min(to <= 0 and THREE.first or THREE.second + (to - 1) * THREE.step, THREE.last))
  elseif view == "5 per Row" and count == 6 then
    at(FIVE[to + 1] or FIVE[6])
  elseif view == "4 per Row" and count == 9 then
    at(FOUR[to + 1] or FOUR[9])
  elseif view == "6 per Row" and count == 5 then
    local y = SIX[to + 1] or SIX[5]
    if type(y) == "table" then y = dir < 0 and y.back or y.next end
    at(y)
  elseif view == "7 per Row" and count == 3 then
    if to <= 0 then
      at(SEVEN.top, 2)
    elseif to == 1 then
      if from < to then at(SEVEN.bottom, 3) end
      at(SEVEN.middle)
    else
      at(SEVEN.bottom)
    end
  else
    -- The project's two plain clicks at the page's share of the scrollbar (drag_expansion_scroll_page).
    at(scrollAt(to, count, dir), 2)
  end
  local out = {}
  for i, y in ipairs(ys) do out[i] = { Pg.SCROLL_X, y } end
  return out
end

-- Where expansion `p` (Pg.place's) is clicked in view `view`, a table point (expansion_cell_pos,
-- expansion_list_cell_pos), or nil where the project's tables have no row for it.
function Pg.cell(view, p)
  local cols, rows = Pg.shape(view)
  local v = p.visible
  local left, right, top, bottom = X.AREA[1], X.AREA[3], X.AREA[2], X.AREA[4]
  local function column(i) return left + (i % cols + 0.5) * ((right - left) / cols) end
  if view == "List" then
    local ys = (p.page >= p.count - 1 and p.count > 1) and { 170, 206 } or LIST_YS[p.page % 7 + 1]
    local y = ys[v + 1]
    return y and { X.LIST_X, y } or nil
  end
  local byPage = ({ ["4 per Row"] = FOUR_YS, ["5 per Row"] = FIVE_YS, ["6 per Row"] = SIX_YS })[view]
  if view == "3 per Row" then
    local base = { 84, 140, 196 }
    local offset = (p.page * 7) % 56
    local ys
    if offset == 35 then
      ys = { base[1] + offset, base[2] + offset }
    elseif offset > 35 then
      ys = { base[1] + offset - 56, base[2] + offset - 56, base[3] + offset - 56 }
    else
      ys = { base[1] + offset, base[2] + offset, base[3] + offset }
    end
    local y = ys[v // cols + 1]
    return y and { column(v), y } or nil
  end
  if byPage then
    local ys = byPage[math.clamp(p.page, 0, #byPage - 1) + 1]
    local y = ys[v // cols + 1]
    return y and { column(v), y } or nil
  end
  -- The last page, shown short, holds its expansions at the bottom of the area — where the grid
  -- scrolls at all: one page of a list too short to fill it stands at the area's top (the project's
  -- rule took none, its lists being longer).
  if p.count > 1 and p.page >= p.count - 1 and p.shown < cols * rows then
    v += math.max(0, rows - math.ceil(p.shown / cols)) * cols
  end
  local row = v // cols
  if v < 0 or row >= rows then return nil end
  if cols == 8 then return { X.GRID8_X0 + (v % cols) * X.GRID8_X_STEP, X.GRID8_Y0 + row * X.GRID8_Y_STEP } end
  return { column(v), top + (row + 0.5) * ((bottom - top) / rows) }
end

-- ── An open expansion's categories ───────────────────────────────────────────
-- Nine rows from (42, 66), 20 apart; their scrollbar at x 12, clicked twice at its top (66) or its
-- bottom (218) to show the first nine or the last nine (the project's EXPANSION_CATEGORY_*).
Pg.CATEGORY_SCROLL = { x = 12, top = 66, bottom = 218 }
Pg.CATEGORY_SCROLL_CLICKS = 2

-- Where category `index` (from 0) of `total` is clicked, and the end of the list that shows it
-- (expansion_category_pos and action_select_expansion_category): the first nine shown with the list
-- at its top, the rest with it at its bottom, where the last one is the ninth row — { point = a
-- table point, row (from 0), at = "top" | "bottom" | nil }. `at` is nil for a list of nine or
-- fewer, which shows them all and has nothing to scroll; the project clicked its scrollbar all the
-- same. nil for an index outside the list.
function Pg.category(index, total)
  if not (index and total and index >= 0 and index < total) then return nil end
  local rows = X.CATEGORY_VISIBLE_ROWS
  local row, at
  if index < rows then
    row, at = index, "top"
  else
    row, at = rows - 1 - math.max(0, total - 1 - index), "bottom"
  end
  row = math.clamp(row, 0, rows - 1)
  return { point = { X.CATEGORY_X, X.CATEGORY_Y0 + row * X.CATEGORY_Y_STEP }, row = row, at = total > rows and at or nil }
end

-- The clicks that take the list of categories to its end `at` ("top" or "bottom"): the project's
-- two, a list of table points.
function Pg.categoryScroll(at)
  local y = at == "bottom" and Pg.CATEGORY_SCROLL.bottom or Pg.CATEGORY_SCROLL.top
  local out = {}
  for i = 1, Pg.CATEGORY_SCROLL_CLICKS do out[i] = { Pg.CATEGORY_SCROLL.x, y } end
  return out
end

-- ── A category's presets ─────────────────────────────────────────────────────
-- Fourteen rows from (142, 60), 12.5 apart, a row's star at x 220; their scrollbar at x 228, each
-- click at its top (58) or its bottom (227) a page up or down (the project's EXPANSION_PRESET_*).
Pg.PRESET_SCROLL = { x = 228, top = 58, bottom = 227 }

-- Where a category's presets lie once its list is scrolled, measured in the project, by the
-- category's size: for each page, from 0, { the first preset of the category the page brings in
-- (from 0), the row it is drawn in (from 0), the correction of every row's y on that page in table
-- points, true for the last page } — the project's measured_specs, as it wrote them.
local MEASURED = {
  [15] = { { 0, 0, 0 }, { 14, 13, 0, true } },
  [16] = { { 0, 0, 0 }, { 14, 12, 0, true } },
  [17] = { { 0, 0, 0 }, { 14, 12, -5 }, { 16, 13, 0, true } },
  [18] = { { 0, 0, 0 }, { 14, 11, 0 }, { 17, 13, 0, true } },
  [19] = { { 0, 0, 0 }, { 14, 10, 3 }, { 17, 12, 0, true } },
  [20] = { { 0, 0, 0 }, { 14, 10, 0 }, { 18, 12, 0, true } },
  [21] = { { 0, 0, 0 }, { 14, 9, 0 }, { 19, 12, 0, true } },
  [22] = { { 0, 0, 0 }, { 14, 9, 0 }, { 19, 11, 0, true } },
  [23] = { { 0, 0, 0 }, { 14, 9, 0 }, { 19, 10, 0, true } },
  [24] = { { 0, 0, 0 }, { 14, 8, 0 }, { 20, 10, 0, true } },
  [25] = { { 0, 0, 0 }, { 14, 8, 0 }, { 20, 9, 0, true } },
  [26] = { { 0, 0, 0 }, { 14, 8, -5 }, { 20, 8, 0, true } },
  [27] = { { 0, 0, 0 }, { 14, 7, 3 }, { 20, 7, 0, true } },
  [28] = { { 0, 0, 0 }, { 14, 7, 0 }, { 21, 7, 0, true } },
  [29] = { { 0, 0, 0 }, { 14, 7, -3 }, { 21, 7, -6 }, { 28, 13, 0, true } },
  [30] = { { 0, 0, 0 }, { 14, 7, -5 }, { 21, 6, 0 }, { 29, 13, 0, true } },
  [31] = { { 0, 0, 0 }, { 14, 6, 4 }, { 21, 6, -4 }, { 29, 12, 0, true } },
  [32] = { { 0, 0, 0 }, { 14, 6, 0 }, { 22, 6, 0 }, { 30, 12, 0, true } },
  [33] = { { 0, 0, 0 }, { 14, 6, 0 }, { 22, 6, 0 }, { 30, 11, 0, true } },
  [34] = { { 0, 0, 0 }, { 14, 6, 0 }, { 22, 6, -5 }, { 30, 10, 0, true } },
  [35] = { { 0, 0, 0 }, { 14, 6, -3 }, { 22, 5, 0 }, { 31, 10, 0, true } },
  [36] = { { 0, 0, 0 }, { 14, 6, -5 }, { 22, 5, 0 }, { 31, 9, 0, true } },
  [37] = { { 0, 0, 0 }, { 14, 5, 3 }, { 22, 5, -4 }, { 31, 8, 0, true } },
  [38] = { { 0, 0, 0 }, { 14, 5, 0 }, { 23, 5, 3 }, { 31, 7, 0, true } },
  [39] = { { 0, 0, 0 }, { 14, 5, 0 }, { 23, 5, 0 }, { 31, 7, 0, true } },
  [40] = { { 0, 0, 0 }, { 14, 5, 0 }, { 23, 5, 0 }, { 32, 6, 0, true } },
  [41] = { { 0, 0, 0 }, { 14, 5, 0 }, { 23, 5, -5 }, { 32, 5, 0, true } },
  [42] = { { 0, 0, 0 }, { 14, 5, -3 }, { 23, 4, 3 }, { 32, 4, 0, true } },
  [43] = { { 0, 0, 0 }, { 14, 5, -5 }, { 23, 4, 0 }, { 33, 4, 7 }, { 42, 13, 0, true } },
  [44] = { { 0, 0, 0 }, { 14, 5, -6 }, { 23, 4, 0 }, { 33, 4, 3 }, { 42, 12, 0, true } },
  [45] = { { 0, 0, 0 }, { 14, 4, 5 }, { 23, 4, -3 }, { 33, 4, 0 }, { 43, 12, 0, true } },
  [46] = { { 0, 0, 0 }, { 14, 4, 3 }, { 23, 4, -3 }, { 33, 4, 0 }, { 43, 11, 0, true } },
  [47] = { { 0, 0, 0 }, { 14, 4, 0 }, { 23, 4, -2 }, { 33, 4, 0 }, { 43, 10, 0, true } },
  [48] = { { 0, 0, 0 }, { 14, 4, 0 }, { 23, 4, 0 }, { 33, 4, 0 }, { 44, 10, 0, true } },
  [49] = { { 0, 0, 0 }, { 14, 4, 0 }, { 24, 4, 0 }, { 34, 4, 0 }, { 44, 9, 0, true } },
  [50] = { { 0, 0, 0 }, { 14, 4, 0 }, { 24, 4, 0 }, { 34, 4, -3 }, { 44, 8, 0, true } },
  [51] = { { 0, 0, 0 }, { 14, 4, 0 }, { 24, 4, -3 }, { 34, 4, -6 }, { 44, 7, 0, true } },
  [52] = { { 0, 0, 0 }, { 14, 4, 0 }, { 24, 4, -5 }, { 34, 3, 0 }, { 45, 7, 0, true } },
  [53] = { { 0, 0, 0 }, { 14, 4, 0 }, { 24, 3, 3 }, { 34, 3, 0 }, { 45, 6, 0, true } },
  [54] = { { 0, 0, 0 }, { 14, 4, 0 }, { 24, 3, 2 }, { 34, 3, 0 }, { 45, 6, 0, true } },
  [55] = { { 0, 0, 0 }, { 14, 4, -3 }, { 24, 3, 0 }, { 35, 4, -5 }, { 45, 4, 0, true } },
  [56] = { { 0, 0, 0 }, { 14, 4, -4 }, { 24, 3, 0 }, { 35, 3, -3 }, { 45, 3, 0, true } },
  [57] = { { 0, 0, 0 }, { 14, 4, -5 }, { 24, 3, 0 }, { 35, 3, -5 }, { 46, 3, 0, true } },
  [58] = { { 0, 0, 0 }, { 14, 4, -7 }, { 24, 3, 0 }, { 35, 3, 0 }, { 46, 4, -7 }, { 56, 12, 0, true } },
  [59] = { { 0, 0, 0 }, { 14, 4, -7 }, { 24, 3, -5 }, { 35, 3, 0 }, { 46, 3, 0 }, { 57, 12, 0, true } },
  [60] = { { 0, 0, 0 }, { 14, 3, 0 }, { 25, 3, -5 }, { 35, 3, 0 }, { 46, 3, 0 }, { 57, 11, 0, true } },
  [61] = { { 0, 0, 0 }, { 14, 3, 0 }, { 25, 4, -7 }, { 35, 3, -5 }, { 46, 3, -3 }, { 57, 10, 0, true } },
  [62] = { { 0, 0, 0 }, { 14, 3, 0 }, { 25, 3, -3 }, { 35, 3, -6 }, { 46, 3, -5 }, { 57, 9, 0, true } },
  [63] = { { 0, 0, 0 }, { 14, 3, 0 }, { 25, 3, 0 }, { 36, 4, -8 }, { 46, 3, -7 }, { 57, 8, 0, true } },
  [72] = { { 0, 0, 0 }, { 14, 3, -3 }, { 25, 3, -7 }, { 36, 2, 0 }, { 48, 3, -3 }, { 59, 3, -7 },
    { 70, 12, 0, true } },
  [75] = { { 0, 0, 0 }, { 14, 3, -5 }, { 25, 2, 0 }, { 37, 3, -3 }, { 48, 3, -7 }, { 59, 2, 0 },
    { 71, 10, 0, true } },
  [76] = { { 0, 0, 0 }, { 14, 3, -5 }, { 25, 2, 0 }, { 37, 3, -3 }, { 48, 2, 0 }, { 60, 3, -3 },
    { 71, 9, 0, true } },
  [77] = { { 0, 0, 0 }, { 14, 3, -5 }, { 25, 2, 0 }, { 37, 3, -5 }, { 48, 2, 0 }, { 60, 3, -5 },
    { 71, 8, 0, true } },
  [96] = { { 0, 0, 0 }, { 14, 2, 0 }, { 26, 2, 0 }, { 38, 2, 0 }, { 50, 2, 0 }, { 62, 2, 0 }, { 74, 2, 0 },
    { 86, 4, 0, true } },
  [100] = { { 0, 0, 0 }, { 14, 2, 0 }, { 26, 2, 0 }, { 38, 2, 0 }, { 50, 2, 0 }, { 62, 2, -3 }, { 74, 2, -5 },
    { 86, 2, -7 }, { 98, 12, 0, true } },
  [101] = { { 0, 0, 0 }, { 14, 2, 0 }, { 26, 2, 0 }, { 38, 2, 0 }, { 50, 2, -3 }, { 62, 2, -3 }, { 74, 2, -7 },
    { 86, 2, -8 }, { 98, 11, 0, true } },
  [109] = { { 0, 0, 0 }, { 14, 2, 0 }, { 26, 2, -5 }, { 38, 2, -7 }, { 50, 1, 0 }, { 63, 2, 0 }, { 75, 2, -5 },
    { 87, 2, -8 }, { 99, 4, 0, true } },
  [136] = { { 0, 0, 0 }, { 14, 2, -6 }, { 26, 1, 0 }, { 39, 2, -8 }, { 51, 1, -3 }, { 64, 1, 0 }, { 77, 2, -6 },
    { 89, 1, 0 }, { 102, 1, 0 }, { 115, 2, -3 }, { 127, 5, 0, true } },
}

-- The pages of a category of `count` presets: the measured ones, or the project's estimate for a
-- size it did not measure (_estimated_preset_scroll_specs).
function Pg.presetPages(count)
  local m = MEASURED[count]
  if m then return m end
  local visible = X.PRESET_VISIBLE_ROWS
  local specs = { { 0, 0, 0 } }
  local first = 14
  while first < count do
    local remaining = count - first
    if remaining <= visible then
      specs[#specs + 1] = { first, math.max(0, visible - remaining), 0, true }
      break
    end
    local row = count <= 30 and math.max(0, visible - 7) or (count >= 75 and 2 or 4)
    specs[#specs + 1] = { first, row, 0 }
    first += visible - row
  end
  return specs
end

-- Where preset `index` (from 0, its place among its category's) of a category of `count` lies
-- (_preset_scroll_target_offset): { page, first = the page's first preset, row (from 0), adjust =
-- the page's correction of y, final = its last page }. The first fourteen, and every preset of a
-- category of fourteen or fewer, on page 0.
function Pg.presetPlace(index, count)
  local visible = X.PRESET_VISIBLE_ROWS
  index, count = math.max(0, index), math.max(0, count)
  if count <= visible or index < visible then return { page = 0, first = 0, row = index, adjust = 0, final = false } end
  local specs = Pg.presetPages(count)
  local page = 0
  for i, s in ipairs(specs) do
    if index >= s[1] then page = i - 1 else break end
  end
  local s = specs[page + 1]
  return { page = page, first = s[1], row = math.clamp(s[2] + index - s[1], 0, visible - 1), adjust = s[3],
    final = s[4] == true }
end

-- What page `page` of a category of `count` shows: for each row from 0 to 13 the preset drawn
-- there (its place among its category's, from 0), { [row] = place }, and the page's correction of
-- y — as the project places them (`first` on its row, the rows above and below it the presets
-- before and after it).
function Pg.presetRows(page, count)
  local visible = X.PRESET_VISIBLE_ROWS
  local out = {}
  local s = (count > visible and page > 0) and Pg.presetPages(count)[page + 1] or { 0, 0, 0 }
  for r = 0, visible - 1 do
    local i = s[1] + r - s[2]
    if i >= 0 and i < count then out[r] = i end
  end
  return out, s[3]
end

-- The clicks on the presets' scrollbar that take a category of `count` from `from` — { page,
-- first, final } as this session's own clicks left it, page 0 right after the category's click — to
-- `to` (Pg.presetPlace's), the project's (_ensure_expansion_preset_visible): a category of 65 or
-- fewer back to its top — three clicks or more, as many as either page — then down a click a page;
-- a longer one up or down a click a page, by way of its top where it goes back from its third page
-- on or from its last. A list of table points, each clicked in turn.
function Pg.presetScroll(from, to, count)
  local ys = {}
  local function at(y, n)
    for _ = 1, n do ys[#ys + 1] = y end
  end
  local top, bottom = Pg.PRESET_SCROLL.top, Pg.PRESET_SCROLL.bottom
  local page, first, final = from.page or 0, from.first or 0, from.final == true
  if count <= 65 then
    if page ~= 0 or first ~= 0 or to.page > 0 then at(top, math.max(1, page, to.page, 3)) end
    if to.page > 0 then at(bottom, to.page) end
  else
    if to.page < page and (page >= 2 or final) then
      at(top, math.max(1, page))
      page = 0
    end
    if page ~= to.page then at(to.page > page and bottom or top, math.max(1, math.abs(to.page - page))) end
  end
  local out = {}
  for i, y in ipairs(ys) do out[i] = { Pg.PRESET_SCROLL.x, y } end
  return out
end

-- Where the preset on row `row` (from 0) is clicked, and its star, on a page whose rows' y is
-- corrected by `adjust` (expansion_preset_pos): two table points.
function Pg.presetPoint(row, adjust)
  local y = X.PRESET_Y0 + row * X.PRESET_Y_STEP + (adjust or 0)
  return { X.PRESET_X, y }, { X.PRESET_FAVORITE_X, y }
end

return Pg
