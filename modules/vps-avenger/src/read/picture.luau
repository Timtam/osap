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
-- THE PROJECT'S PIXELS. Its captures were a Retina display's backing pixels, two for every point
-- each way (its ocr.py), and its counts and sizes are in those pixels at the zoom of the moment.
-- A picture here is one pixel per point on a Mac (docs/api/screen.md, "Reading a snapshot"), and
-- one physical pixel on Windows, where a display scaled to 150 % draws Avenger 1.5 times as
-- large. So a count is measured as a SHARE of its region, and turned into the count the project
-- would have seen there by the region's size in the project's pixels — and the log says both, so
-- that a tester's run says whether the project's numbers hold on his display (TODO.md, GM7).

local G = host.include("src/geometry.luau")

local P = {}

-- The project's pixels per point, each way: a Retina display's backing pixels.
P.RETINA = 2

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
  -- The project's pixels per pixel of the picture, each way: two on a Mac at any zoom; on
  -- Windows two over the display's scaling, which the factor holds besides the zoom.
  pl.t = P.RETINA * pl.s / k
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

-- A grid of one row over `rect`, one cell per pixel column (at most 256): a run of cells that
-- pass is something the test finds, as wide as the run.
function P.columns(name, rect, pred)
  if not rect then return nil end
  local w = rect[3] - rect[1]
  return { name = name, kind = "cells", rect = rect, cols = math.min(256, w), rows = 1, pred = pred }
end

-- A column profile of `rect`.
function P.profile(name, rect)
  if not rect then return nil end
  return { name = name, kind = "profile", rect = rect }
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

-- Cell (col, row), from 1: 0 to 255.
function Grid:at(c, r)
  local i = ((r - 1) * self.cols + c - 1) * 2 + 1
  local hi, lo = string.byte(self.hex, i, i + 1)
  return HEX[hi] * 16 + HEX[lo]
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

-- The runs of a one-row grid: { from, to, pixels } for every run of cells whose share is above
-- 0, `from` and `to` in screen pixels, `pixels` the pixels that pass in it.
function Grid:runs()
  local out, cur = {}, nil
  local h = self.rect[4] - self.rect[2]
  for col = 1, self.cols do
    local v = self:at(col, 1)
    local xs = self.xs[col]
    if v > 0 then
      if not cur then
        cur = { from = self.rect[1] + xs[1], to = self.rect[1] + xs[2], pixels = 0 }
        out[#out + 1] = cur
      end
      cur.to = self.rect[1] + xs[2]
      cur.pixels += v / 255 * (xs[2] - xs[1]) * h
    else
      cur = nil
    end
  end
  return out
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

-- Points on a regular grid over the ring between radius `rmin` and `rmax` round screen point
-- (cx, cy), at most about `max` of them: the points, and the stride. A point stands for stride²
-- pixels of the ring.
function P.ring(cx, cy, rmin, rmax, max)
  local area = math.pi * (rmax * rmax - rmin * rmin)
  local stride = math.max(1, math.ceil(math.sqrt(area / max)))
  local n = math.ceil(rmax / stride)
  local out = {}
  for j = -n, n do
    for i = -n, n do
      local dx, dy = i * stride, j * stride
      local d = math.sqrt(dx * dx + dy * dy)
      if d >= rmin and d <= rmax then out[#out + 1] = { cx + dx, cy + dy } end
    end
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
