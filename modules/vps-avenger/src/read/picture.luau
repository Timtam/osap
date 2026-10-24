-- The page's reads of ONE picture of Avenger: where a table's rectangle lies in it, what to ask
-- of it, and the arithmetic over the answers.
--
-- WHY THIS SHAPE. The avenger_control project measures Avenger by looping over every pixel of a
-- small capture in Python, one capture per question (its clicker/__init__.py). Here the loop over
-- pixels never runs in Luau, which would hold the event loop and every module with it
-- (docs/module-runtime-and-lifecycle.md): a picture is taken once (read/open.luau), and each
-- question is asked of it as
--   * a grid of cells (host.screen.cells with a predicate, on the image worker): for every cell
--     the share of its pixels that pass a colour test. Cells about one table unit across, so a
--     button's share is the cells it covers, weighted by how much of each it covers;
--   * a column profile (host.screen.profile): the mean colour of every pixel column of a band, so
--     a tab's mean brightness is the mean of its columns — the darkest strip;
--   * a few hundred points (host.screen.pixels of the picture, answered at once), where the
--     project's own arithmetic is wanted point by point: the knobs, and the means of the pixels a
--     test lets through.
-- Luau then works on the answers only: grids of at most 4096 cells, profiles of one value per
-- pixel column, and lists of points.
--
-- THE PROJECT'S PIXELS. Its counts and sizes are in the pixels of its own captures at the zoom of
-- the moment, and those were one pixel per point: its knob constants are counts in such pictures
-- (a bare pointer, PAN_BASE_PIXELS × s², is the 50 bright pixels of
-- `Sliders/Pan/5 Avenger Pan C.png` at 100 %), whatever the backing scale of the display it ran
-- on. A picture here is one pixel per point on a Mac too, at any backing scale (docs/api/screen.md,
-- "Reading a snapshot"), and one physical pixel on Windows, where a display scaled to 150 % draws
-- Avenger 1.5 times as large. So a count is measured as a SHARE of its region, and turned into the
-- count the project would have seen there by the region's size in the project's pixels — and the
-- log says both, so that a tester's run says whether the project's numbers hold on his display
-- (TODO.md, GM7).

local G = host.include("src/geometry.luau")

local P = {}

-- ── Placing ──────────────────────────────────────────────────────────────────
-- A placer: the overlay's origin and factor as they were when the picture was taken, so a read
-- made after a wait still places where the picture was taken. `zoom` is the zoom read off
-- Avenger's header, or nil when it is not known (an unread zoom field on Windows).

local Placer = {}
Placer.__index = Placer

function P.placer(origin, k, zoom)
  local pl = setmetatable({ origin = origin, k = k, zoom = zoom }, Placer)
  -- The project's scale, the zoom over 50: its sizes grow with it. Where the zoom is not known,
  -- the factor is the best there is.
  pl.s = zoom and zoom / G.AUTHORED_ZOOM or k
  -- The project's pixels per pixel of the picture, each way: one on a Mac at any zoom and backing
  -- scale; on Windows one over the display's scaling, which the factor holds besides the zoom.
  pl.t = pl.s / k
  return pl
end

-- Table point (x, y) on screen: the runtime's own formula (geometry.luau, G.place).
function Placer:point(x, y)
  return G.place(self.origin, self.k, x, y)
end

-- Table point (x, y) on screen before it is rounded to a pixel: where a knob's middle really is,
-- which a pixel's middle (its corner + 0.5) is measured from.
function Placer:exact(x, y)
  local fx, fy = G.frameOf(self.origin)
  return self.origin.client.x + fx + self.k * x, self.origin.client.y + fy + self.k * y
end

-- Table rectangle `r` on screen, in whole pixels, its corners placed like points; nil when it is
-- empty there.
function Placer:rect(r)
  local x1, y1 = self:point(r[1], r[2])
  local x2, y2 = self:point(r[3], r[4])
  if x2 <= x1 or y2 <= y1 then return nil end
  return { x1, y1, x2, y2 }
end

-- A table box `size` = { w, h } round table point `p`, on screen.
function Placer:box(p, size)
  return self:rect({ p[1] - size[1] / 2, p[2] - size[2] / 2, p[1] + size[1] / 2, p[2] + size[2] / 2 })
end

-- How many of the project's pixels a rectangle of the picture held there.
function Placer:toolPixels(rect)
  return (rect[3] - rect[1]) * (rect[4] - rect[2]) * self.t * self.t
end

-- ── What is asked ────────────────────────────────────────────────────────────
-- Requests are data: read/open.luau sends every one of them at once, against the picture, and
-- hands each answer back by its name.

-- A grid of `pred` over screen rectangle `rect`, its cells about one table unit across: at most
-- 256 a side and 4096 in all, as host.screen.cells takes them. `rows`, when given, is how many
-- rows: one for a row of boxes as tall as the grid, which needs none. nil for no rectangle.
function P.cells(name, rect, pred, k, rows)
  if not rect then return nil end
  local w, h = rect[3] - rect[1], rect[4] - rect[2]
  local cols = math.clamp(math.floor(w / k + 0.5), 1, math.min(256, w))
  rows = rows or math.clamp(math.floor(h / k + 0.5), 1, math.min(256, h))
  -- Too many: both sides shrunk in proportion until they fit.
  while cols * rows > 4096 do
    local f = math.sqrt(4096 / (cols * rows))
    cols, rows = math.max(1, math.floor(cols * f)), math.max(1, math.floor(rows * f))
  end
  return { name = name, kind = "cells", rect = rect, cols = cols, rows = rows, pred = pred }
end

-- A grid of `pred` over screen rectangle `rect` whose cells are c by c pixels, for the smallest c
-- that keeps it within the grids' limits (256 a side, 4096 in all): the one grid P.lit reads every
-- pixel that passes off. nil for no rectangle.
function P.litCells(name, rect, pred)
  if not rect then return nil end
  local w, h = rect[3] - rect[1], rect[4] - rect[2]
  local c = 1
  while math.ceil(w / c) > 256 or math.ceil(h / c) > 256 or math.ceil(w / c) * math.ceil(h / c) > 4096 do c += 1 end
  return { name = name, kind = "cells", rect = rect, cols = math.ceil(w / c), rows = math.ceil(h / c), pred = pred }
end

-- The runs of pixels `pixels` (P.lit's) along their rows: { y, x1, x2 }, x2 past the run's last
-- pixel, in order of their rows and their starts — sorted as numbers, which the sort compares
-- itself.
function P.runsOf(pixels)
  local x0, y0 = math.huge, math.huge
  for _, q in ipairs(pixels) do
    if q[1] < x0 then x0 = q[1] end
    if q[2] < y0 then y0 = q[2] end
  end
  local keys = table.create(#pixels)
  for i, q in ipairs(pixels) do keys[i] = (q[2] - y0) * 1048576 + (q[1] - x0) end
  table.sort(keys)
  local runs = {}
  for _, key in ipairs(keys) do
    local y, x = key // 1048576 + y0, key % 1048576 + x0
    local last = runs[#runs]
    if last and last[1] == y and last[3] == x then
      last[3] = x + 1
    elseif not (last and last[1] == y and last[3] > x) then
      runs[#runs + 1] = { y, x, x + 1 }
    end
  end
  return runs
end

-- The 8-connected parts of `runs` (P.runsOf's, in its order), each { pixels, x1, y1, x2, y2 (past
-- its last pixel each way), runs = its runs }: runs of neighbouring rows that overlap or meet at a
-- corner are one. One step a pair of runs on neighbouring rows.
function P.components(runs)
  local parent = table.create(#runs, 0)
  for i = 1, #runs do parent[i] = i end
  local function root(i)
    while parent[i] ~= i do
      parent[i] = parent[parent[i]]
      i = parent[i]
    end
    return i
  end
  -- The runs of the row before, from `prevFrom` to `prevTo`, swept left to right with the row's own:
  -- one that ends before a run begins touches none after it either.
  local prevFrom, prevTo, rowFrom = 1, 0, 1
  for i, r in ipairs(runs) do
    if i > 1 and r[1] ~= runs[i - 1][1] then
      if runs[i - 1][1] == r[1] - 1 then prevFrom, prevTo = rowFrom, i - 1 else prevFrom, prevTo = 1, 0 end
      rowFrom = i
    end
    while prevFrom <= prevTo and runs[prevFrom][3] < r[2] do prevFrom += 1 end
    local j = prevFrom
    while j <= prevTo and runs[j][2] <= r[3] do
      local a, b = root(i), root(j)
      if a ~= b then parent[a] = b end
      j += 1
    end
  end
  local byRoot, out = {}, {}
  for i, r in ipairs(runs) do
    local k = root(i)
    local c = byRoot[k]
    if not c then
      c = { pixels = 0, x1 = r[2], y1 = r[1], x2 = r[3], y2 = r[1] + 1, runs = {} }
      byRoot[k] = c
      out[#out + 1] = c
    end
    c.pixels += r[3] - r[2]
    c.x1, c.x2 = math.min(c.x1, r[2]), math.max(c.x2, r[3])
    c.y1, c.y2 = math.min(c.y1, r[1]), math.max(c.y2, r[1] + 1)
    c.runs[#c.runs + 1] = r
  end
  table.sort(out, function(a, b) return a.x1 < b.x1 end)
  return out
end

-- A column profile of `rect` — of its rows as well with `axes` "both" (the drumkit's name column,
-- read/drums.luau).
function P.profile(name, rect, axes)
  if not rect then return nil end
  return { name = name, kind = "profile", rect = rect, axes = axes }
end

-- ── The answers ──────────────────────────────────────────────────────────────

-- The blocks of an axis `len` pixels long cut into `n`, as host.screen.cells cuts it: block s
-- (from 0) starts at floor(s·len/n) and ends before max(start + 1, floor((s + 1)·len/n)).
local function blocks(len, n)
  local out = table.create(n)
  for s = 0, n - 1 do
    local a = (s * len) // n
    out[s + 1] = { a, math.max(a + 1, ((s + 1) * len) // n) }
  end
  return out
end

local Grid = {}
Grid.__index = Grid

-- A grid read: `answer` as host.screen.cells gives it, for request `req` — and `req.view`, the
-- picture's rectangle, where the request was cut to it (read/open.luau).
function P.grid(req, answer)
  return setmetatable({ rect = req.rect, cols = req.cols, rows = req.rows, hex = answer.cells, view = req.view,
    xs = blocks(req.rect[3] - req.rect[1], req.cols), ys = blocks(req.rect[4] - req.rect[2], req.rows) }, Grid)
end

-- A hex digit's value, by its byte: the grid's answer is two lower-case digits a cell.
local HEX = {}
for i = 0, 9 do HEX[48 + i] = i end
for i = 0, 5 do
  HEX[97 + i] = 10 + i
  HEX[65 + i] = 10 + i
end

-- The blocks of `n` over `len` that pixels a to b (exclusive) may touch: block s holds pixel x
-- only when s is floor(x·n/len) or one either side of it.
local function span(a, b, len, n)
  return math.max(1, (math.max(0, a) * n) // len), math.min(n, (math.max(0, b - 1) * n) // len + 2)
end

-- Screen rectangle `r` in the grid: the share of its pixels that pass (0 to 1), each cell it
-- covers counted by how much of it it covers; how many pixels that is; and whether any cell it
-- covers has a pixel that passes. nil when it lies outside the grid, or reaches past the edge of
-- the picture (`view`): a box the screen shows only part of is not read. Only the cells it covers
-- are looked at.
function Grid:share(r)
  local v = self.view
  if v and (r[1] < v[1] or r[2] < v[2] or r[3] > v[3] or r[4] > v[4]) then return nil end
  local gx, gy = self.rect[1], self.rect[2]
  local x1, y1, x2, y2 = r[1] - gx, r[2] - gy, r[3] - gx, r[4] - gy
  local w, h = self.rect[3] - gx, self.rect[4] - gy
  local c1, c2 = span(x1, x2, w, self.cols)
  local r1, r2 = span(y1, y2, h, self.rows)
  -- Inline, the decoding included: this is the loop over the cells.
  local hex, cols, xs, ys = self.hex, self.cols, self.xs, self.ys
  local sum, area, any = 0, 0, false
  for row = r1, r2 do
    local yb = ys[row]
    local oy = math.min(yb[2], y2) - math.max(yb[1], y1)
    if oy > 0 then
      local base = (row - 1) * cols
      for col = c1, c2 do
        local xb = xs[col]
        local ox = math.min(xb[2], x2) - math.max(xb[1], x1)
        if ox > 0 then
          local i = (base + col - 1) * 2 + 1
          local hi, lo = string.byte(hex, i, i + 1)
          local v = HEX[hi] * 16 + HEX[lo]
          sum += v / 255 * ox * oy
          area += ox * oy
          if v > 0 then any = true end
        end
      end
    end
  end
  if area == 0 then return nil end
  return sum / area, area, any
end

-- The hex of each share a cell of `n` pixels can have, n from `totals` (a set), as host.screen.cells
-- writes it — round(255 × passing / n), halves to even — but none.
local function shares(totals)
  local seen, out = {}, {}
  for n in pairs(totals) do
    for k = 1, n do
      local v = string.format("%02x", P.round(255 * k / n))
      if not seen[v] then seen[v], out[#out + 1] = true, v end
    end
  end
  return out
end

-- EVERY PIXEL that passes, of grid `g` (P.litCells'): the cells that hold one are found by the
-- string library's plain search of the answer for each share a cell can have — a step a cell found,
-- whatever the grid's size — and only their pixels are asked for their colours (host.screen.pixels
-- of snapshot `snap`, answered at once) and tested by `test(colour)`, the grid's predicate written
-- out in Luau. The pixels, { x, y, colour } in screen pixels in no order, and how many colours were
-- asked; or nil and why the colours were not read.
function P.lit(g, snap, test)
  local hex, cols = g.hex, g.cols
  -- The cells' sizes: the blocks are of at most two widths and two heights.
  local wx, wy, totals = {}, {}, {}
  for _, b in ipairs(g.xs) do wx[b[2] - b[1]] = true end
  for _, b in ipairs(g.ys) do wy[b[2] - b[1]] = true end
  for a in pairs(wx) do
    for b in pairs(wy) do totals[a * b] = true end
  end
  local found, cells = {}, {}
  for _, v in ipairs(shares(totals)) do
    local at = 1
    while true do
      local i = string.find(hex, v, at, true)
      if not i then break end
      -- A match that starts on a cell's second digit spans two cells, and is none of them.
      if i % 2 == 1 and not found[i] then
        found[i] = true
        cells[#cells + 1] = (i - 1) // 2
      end
      at = i + 1
    end
  end
  table.sort(cells)
  local pts = {}
  for _, cell in ipairs(cells) do
    local xb, yb = g.xs[cell % cols + 1], g.ys[cell // cols + 1]
    for y = g.rect[2] + yb[1], g.rect[2] + yb[2] - 1 do
      for x = g.rect[1] + xb[1], g.rect[1] + xb[2] - 1 do pts[#pts + 1] = { x, y } end
    end
  end
  local out = {}
  for from = 1, #pts, 4096 do
    local part = table.move(pts, from, math.min(#pts, from + 4095), 1, {})
    local ok, colours, why = pcall(host.screen.pixels, part, { snapshot = snap })
    if not ok then return nil, tostring(colours) end
    if type(colours) ~= "table" then return nil, tostring(why) end
    for k, c in ipairs(colours) do
      if test(c) then
        local q = part[k]
        out[#out + 1] = { q[1], q[2], c }
      end
    end
  end
  return out, #pts
end

-- A column profile's mean brightness between screen x1 and x2, as the project counts it — the
-- mean of (r + g + b) / 3 — over the band it was taken of, and how many columns that is; nil
-- outside it. Pixel columns are whole, so this is the mean over exactly those pixels.
function P.bandMean(prof, x1, x2)
  local cols = prof and prof.columns
  if not cols then return nil end
  local from = math.max(x1, prof.x) - prof.x + 1
  local to = math.min(x2, prof.x + prof.w) - prof.x
  if to < from then return nil end
  local sum = 0
  for i = from, to do sum += (cols.r[i] + cols.g[i] + cols.b[i]) / 3 end
  return sum / (to - from + 1), to - from + 1
end

-- The same over a profile's ROWS between screen y1 and y2 (a profile asked with `axes` "both"),
-- and how many rows that is; nil outside it.
function P.rowMean(prof, y1, y2)
  local rows = prof and prof.rows
  if not rows then return nil end
  local from = math.max(y1, prof.y) - prof.y + 1
  local to = math.min(y2, prof.y + prof.h) - prof.y
  if to < from then return nil end
  local sum = 0
  for i = from, to do sum += (rows.r[i] + rows.g[i] + rows.b[i]) / 3 end
  return sum / (to - from + 1), to - from + 1
end

-- The bottom rows of a box `boxH` pixels tall, as the project reads a tab's selection: its own
-- rule, three rows at most and a third of the box (two when the box is under six), in its pixels;
-- in the picture's, at least one.
function P.bottomRows(pl, boxH)
  local h = boxH * pl.t
  local rows = h >= 6 and math.min(3, h // 3) or 2
  return math.max(1, math.floor(math.max(1, rows) / pl.t + 0.5))
end

-- ── Points ───────────────────────────────────────────────────────────────────

-- Points on a regular grid over screen rectangle `r`, at most `max` of them: every pixel where
-- that many fit, every second (third, …) each way otherwise. The points, and the stride.
function P.spread(r, max)
  local w, h = r[3] - r[1], r[4] - r[2]
  local stride = math.max(1, math.ceil(math.sqrt(w * h / max)))
  local out = {}
  for y = r[2] + stride // 2, r[4] - 1, stride do
    for x = r[1] + stride // 2, r[3] - 1, stride do out[#out + 1] = { x, y } end
  end
  return out, stride
end

-- ── Numbers ──────────────────────────────────────────────────────────────────

-- Python's round(), which the project's numbers went through: to the nearest whole number, a
-- half to the even one (2.5 is 2, 3.5 is 4).
function P.round(x)
  local f = math.floor(x)
  local d = x - f
  if d > 0.5 then return f + 1 end
  if d < 0.5 then return f end
  return f % 2 == 0 and f or f + 1
end

-- ── Words ────────────────────────────────────────────────────────────────────

-- A share as the log says it, in per cent.
function P.pct(share)
  if share == nil then return "not read" end
  return string.format("%.1f %%", share * 100)
end

return P
