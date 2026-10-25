-- The expansion grid's pages by position: where an expansion lies in VPS Avenger's grid by its place
-- in the list chosen, for opening one whose name is not read there (src/actions/browser.luau, the
-- second Mac test's A10, part 6).
--
-- SOURCE: the avenger_control project's actions.py — _expansion_list_page_starts,
-- _expansion_8_page_starts and _expansion_8_visible_start, _expansion_6/5/4/3_page_starts and
-- their counts, _expansion_page_for_index and _click_expansion_page_scroll — and its
-- clicker/header_map.py — expansion_view_shape, expansion_cell_pos, expansion_list_cell_pos,
-- expansion_scroll_pos, expansion_list_scroll_pos and the scrollbar's EXPANSION_*_SCROLL_* points —
-- taken over with its developer's permission (NOTICE, 2026-10-12): at 50 % from Avenger's corner,
-- as every table of this module. The numbers are the project's, measured on its rosters: the
-- 8 per Row layout for 129 expansions and more, three pages; 6, 5 and 4 per Row for five, six and
-- nine pages. A list of another size takes the project's general rule (its pages one view's worth
-- apart, the scrollbar placed by the page's share of them), which it did not measure. Nothing here
-- is taken for done: the expansion clicked is confirmed by the header's name after the click.
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

return Pg
