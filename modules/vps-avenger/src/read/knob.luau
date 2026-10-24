-- The knob reader: a knob's value from where its bright pointer points, read off the page's
-- picture. The three macro knobs' amounts, and the selected oscillator's Volume, Pan, Transpose,
-- Fine Tune and Random or Alternate Pitch — all of them off the picture the page key took
-- (read/open.luau), or one knob off a small picture of its own once the page's amount has turned it
-- (K.read, src/amount.luau).
--
-- SOURCE: the avenger_control project's knob readers (its clicker/__init__.py: the macro amount's
-- centre of brightness, the Level knob's ray, Transpose's and Fine Tune's end stops, and Pan's and
-- Random Pitch's extra pixels beside the pointer), taken over with its developer's permission
-- (NOTICE): its arithmetic and its numbers as it has them, worked in its own pixels.
--
-- WHAT CHANGED. The project looked at every pixel of a square round the knob, in a loop. This
-- looks at every pixel of the ring too — the project's own reading on every one of the
-- `Sliders/…` pictures needs every pixel, measured from the knob's middle to the pixel's CORNER as
-- its `dx = xx − size / 2` is: from the pixel's middle, every Pan to the left reads 0, and one point
-- in four reads every Pan and Random as 0 — but without a loop over them: a grid of the rectangle
-- round the rings read together (host.screen.cells, on the image worker) with the project's own
-- test of a bright pixel says which cells hold one, and only the pixels of those cells are asked
-- for their colours (host.screen.pixels on the snapshot, answered at once) and tested again one by
-- one. So the work is the pointers' pixels, not the rings': every pixel of the three macro rings
-- would be about 27,000 points at 100 % and 108,000 at 200 %. The rings read together share their
-- grid — the three macro knobs one, the oscillator's three another — so the page's key waits for
-- two grids, not six. A ring the picture does not hold whole is not read: the project's counts are
-- of the whole ring, and a window reaching below the screen cuts the macro knobs' rings. A pixel's
-- place is turned into the project's pixels (read/picture.luau, `t`), so its numbers — the ring's
-- radii, the ray's length and width,
-- the counts — apply as they are. The search for the pointer's angle scores the same 271 angles
-- with the same weights, but adds each pixel only to the angles whose ray passes near it, rather
-- than every pixel to every angle. Python's round, which rounds a half to the even neighbour, is
-- kept. A knob with fewer bright pixels than the project's 8 is not read — Random Pitch too, which
-- the project read as 0 % from no pixels at all. The third knob's ring is read once for Fine Tune
-- and Random Pitch both, where the project captured and read it twice.

local P = host.include("src/read/picture.luau")
local OSC = host.include("src/tables/osc.luau")
local M = host.include("src/tables/macros.luau")

local K = {}

-- Fewer bright pixels than this (the project's), and the knob is not read.
local BRIGHT_MIN = 8

local round = P.round

-- The project's test of a ring's bright pixel, (r + g + b) / 3 above `lumaMin` and the channels
-- less than `satMax` apart, as a predicate for the grids of the square round the ring.
local function brightTest(lumaMin, satMax)
  local d = satMax - 1
  return host.screen.predicate(string.format("r + g + b >= %d and r - g <= %d and g - r <= %d and r - b <= %d "
    .. "and b - r <= %d and g - b <= %d and b - g <= %d", 3 * lumaMin + 1, d, d, d, d, d, d))
end
local BRIGHT = { [120] = brightTest(120, 75), [125] = brightTest(125, 75) }

-- The bright pixels of the rings round table points `ps`, each between the project's radii `minR`
-- and `maxR` (its pixels at 50 %, grown with its scale), off ONE grid of the rectangle round them
-- all: for ring i, `out[i]` = { pts, info } — pts { dx, dy, luma } in its pixels from the knob's
-- middle to the pixel's corner, for every pixel of the ring brighter than `lumaMin` (120 or 125)
-- whose channels are less than `satMax` (75) apart; info what was looked at, { pixels (the ring's),
-- asked (the colours asked, of the grid's cells that hold a bright pixel), area (the project's
-- pixels a pixel is), cx, cy, rmax (in the picture's pixels) } — or { nil, why }. A ring whose
-- square reaches past snapshot `snap` is not read, and is left out of the grid.
local function rings(pl, snap, ps, minR, maxR, lumaMin, satMax)
  local out, placed, around = {}, {}, nil
  for i, p in ipairs(ps) do
    local ex, ey = pl:exact(p[1], p[2])
    local rmax = maxR * pl.s / pl.t
    local sq = { math.floor(ex - rmax), math.floor(ey - rmax), math.floor(ex + rmax) + 1, math.floor(ey + rmax) + 1 }
    if sq[1] < snap.x or sq[2] < snap.y or sq[3] > snap.x + snap.w or sq[4] > snap.y + snap.h then
      out[i] = { nil, "the ring is not wholly in the picture" }
    else
      placed[i] = { ex = ex, ey = ey, rmin = minR * pl.s / pl.t, rmax = rmax }
      around = around and { math.min(around[1], sq[1]), math.min(around[2], sq[2]), math.max(around[3], sq[3]),
        math.max(around[4], sq[4]) } or sq
    end
  end
  if not around then return out end
  -- Their bright pixels, off one grid of the rectangle round them (P.litCells, P.lit).
  local req = P.litCells("rings", around, BRIGHT[lumaMin] or brightTest(lumaMin, satMax))
  local ok, answer, why = pcall(host.screen.cells, { region = req.rect, cols = req.cols, rows = req.rows,
    predicate = req.pred, snapshot = snap })
  local pixels, asked = nil, nil
  if not ok then
    why = tostring(answer)
  elseif answer then
    pixels, asked = P.lit(P.grid(req, answer), snap, function(c)
      return (c.r + c.g + c.b) / 3 > lumaMin and math.max(c.r, c.g, c.b) - math.min(c.r, c.g, c.b) < satMax
    end)
    if not pixels then why = asked end
  end
  for i, at in pairs(placed) do
    if not pixels then
      out[i] = { nil, tostring(why) }
    else
      -- Those inside the ring, measured from the knob's middle to each pixel's corner.
      local r1, r2 = at.rmin * at.rmin, at.rmax * at.rmax
      local pts = {}
      for _, q in ipairs(pixels) do
        local dx, dy = q[1] - at.ex, q[2] - at.ey
        local d = dx * dx + dy * dy
        if d >= r1 and d <= r2 then
          local c = q[3]
          pts[#pts + 1] = { dx * pl.t, dy * pl.t, (c.r + c.g + c.b) / 3 }
        end
      end
      out[i] = { pts, { pixels = math.floor(math.pi * (r2 - r1) + 0.5), asked = asked, area = pl.t * pl.t,
        cx = math.floor(at.ex + 0.5), cy = math.floor(at.ey + 0.5), rmax = at.rmax } }
    end
  end
  return out
end

local function seen(pts, info)
  return string.format("%d bright of the ring's %d pixels (%d asked; the project's %.0f pixels)", #pts, info.pixels,
    info.asked, #pts * info.area)
end

-- Where a knob read at angle `deg` (the project's: 0 is right, 90 down) points, for the picture.
local function tip(info, deg)
  local a = math.rad(deg)
  return { x = math.floor(info.cx + math.cos(a) * info.rmax + 0.5), y = math.floor(info.cy + math.sin(a) * info.rmax + 0.5) }
end

-- ── The macro knobs ──────────────────────────────────────────────────────────

-- The rings of macro knobs `indices` (1 to 3), read together (`rings`): each { pts, info } or
-- { nil, why }, by its place in `indices`.
local function macroRings(pl, snap, indices)
  local ps = {}
  for i, index in ipairs(indices) do ps[i] = { M.FIELDS["macro" .. index][1], M.AMOUNT_Y } end
  return rings(pl, snap, ps, M.AMOUNT_DETECT_MIN_R, M.AMOUNT_DETECT_MAX_R, 120, 75)
end

-- A macro knob's amount off its ring `r` ({ pts, info } or { nil, why }): 0 to 100 in steps of 10,
-- or nil; and the log's words. The centre of the ring's bright pixels, those near the rim counting
-- a third as much, is where the pointer is; the knob runs 270° clockwise from 135° (down and left).
local function macroOf(pl, r, marks)
  local s = pl.s
  local pts, info = r[1], r[2]
  if not pts then return nil, "not read: " .. tostring(info) end
  marks[#marks + 1] = { x = info.cx, y = info.cy }
  local maxR = M.AMOUNT_DETECT_MAX_R * s
  local sx, sy, sw = 0, 0, 0
  for _, q in ipairs(pts) do
    local d = math.sqrt(q[1] * q[1] + q[2] * q[2])
    local w = (q[3] - 110) * (d < maxR * 0.86 and 1 or 0.35)
    sx += q[1] * w
    sy += q[2] * w
    sw += w
  end
  if sw <= 0 or #pts * info.area < BRIGHT_MIN then
    return nil, seen(pts, info) .. " — too few"
  end
  local ax, ay = sx / sw, sy / sw
  local angle = math.deg(math.atan2(ay, ax))
  local delta = (angle - 135) % 360
  if delta > 315 then
    delta = 0
  elseif delta > 270 then
    delta = 270
  end
  local amount = math.clamp(delta / 270 * 100, 0, 100)
  local step = math.max(1, M.AMOUNT_DETECT_STEP)
  local rounded = math.clamp(round(amount / step) * step, 0, 100)
  marks[#marks + 1] = tip(info, angle)
  return rounded, string.format("%s, centre of brightness (%.1f, %.1f), angle %.1f°, %.1f° round — %d %%",
    seen(pts, info), ax, ay, angle, delta, rounded)
end

-- Macro knob `index` (1 to 3) alone: its amount and the log's words (macroOf).
function K.macro(pl, snap, index, marks)
  return macroOf(pl, macroRings(pl, snap, { index })[1], marks)
end

-- The three macro knobs off one grid: their amounts by index, each nil where it was not read, and
-- the log's words by index.
function K.macros(pl, snap, marks)
  local rs = macroRings(pl, snap, { 1, 2, 3 })
  local amounts, words = {}, {}
  for i = 1, 3 do amounts[i], words[i] = macroOf(pl, rs[i], marks) end
  return amounts, words
end

-- ── The oscillator's knobs ───────────────────────────────────────────────────

-- The direction of each of the 271 angles, from 135° (delta 0) clockwise: worked out once.
local UX, UY = table.create(271, 0), table.create(271, 0)
for delta = 0, 270 do
  local deg = (135 + delta) % 360
  local rad = math.rad(deg <= 180 and deg or deg - 360)
  UX[delta + 1], UY[delta + 1] = math.cos(rad), math.sin(rad)
end

-- The angle the pointer of the knob whose bright points are `pts` points at: of the 271 whole
-- degrees from 135° clockwise to 45°, the one whose ray, from 4·s to 31·s out and 2.2·s either
-- side, gathers the most brightness, weighted down with the distance from the ray (to none at
-- 2.5·s). The first of equals, and 135° when nothing scores: the project's. Its degree (as the
-- project's, 0 to 359) and score.
local function rayOf(pts, s)
  local lo, hi, tol, fall = 4 * s, 31 * s, 2.2 * s, 2.5 * s
  local score = table.create(271, 0)
  for _, q in ipairs(pts) do
    local dx, dy, l = q[1], q[2], q[3]
    local rho = math.sqrt(dx * dx + dy * dy)
    -- A point nearer the middle than the ray begins is on no ray; one farther than its width from
    -- the middle is near rays within asin(width / distance) of its own angle only.
    if rho >= lo then
      local c = (math.deg(math.atan2(dy, dx)) - 135) % 360
      local spread = math.deg(math.asin(math.min(1, tol / rho)))
      local w = l - 120
      -- The degrees round the point's own, as indices into the 271 angles: those past 270 (the
      -- gap at the bottom) left out, and a range that runs past 359 taken on from 0. Each turn is
      -- written out without calls: this loop runs over every bright pixel of the pointer.
      local from, to = math.floor(c - spread), math.ceil(c + spread)
      for pass = 1, 2 do
        local a, b = from, to
        if pass == 2 then a, b = from - 360, to - 360 end
        if a < 0 then a = 0 end
        if b > 270 then b = 270 end
        for d = a + 1, b + 1 do
          local ux, uy = UX[d], UY[d]
          local proj = dx * ux + dy * uy
          if proj >= lo and proj <= hi then
            local perp = dx * uy - dy * ux
            if perp < 0 then perp = -perp end
            if perp <= tol then
              local near = 1 - perp / fall
              if near > 0 then score[d] += w * near end
            end
          end
        end
      end
    end
  end
  local best, bestDeg = -1, 270
  for delta = 0, 270 do
    if score[delta + 1] > best then
      best = score[delta + 1]
      bestDeg = (135 + delta) % 360
    end
  end
  return bestDeg, best
end

-- Pan's (or Random Pitch's) extra pixels beside a pointer along unit vector (ux, uy): those 3·s to
-- 32·s along it and 2.5·s to 13·s beside it, at least 2·s left or right of the middle, weighed by
-- their brightness over 120 on either side. From -100 (all left) to 100 in steps of 25, by how
-- many bright pixels the knob has more than a bare pointer's — or 0 for too few beside it.
local function sideValue(pts, area, s, ux, uy)
  local vx, vy = -uy, ux
  local left, right, n = 0, 0, 0
  for _, q in ipairs(pts) do
    local dx, dy, l = q[1], q[2], q[3]
    local proj = dx * ux + dy * uy
    if proj >= 3 * s and proj <= 32 * s then
      local side = math.abs(dx * vx + dy * vy)
      if side >= 2.5 * s and side <= 13 * s and math.abs(dx) >= 2 * s then
        local w = math.max(0, l - 120)
        if dx < 0 then left += w else right += w end
        n += 1
      end
    end
  end
  local dir = right > left and 1 or (left > right and -1 or 0)
  local raw = 0
  if dir ~= 0 and n * area >= 4 then
    local norm = #pts * area / math.max(0.01, s * s)
    if OSC.PAN_FULL_PIXEL_DELTA > 0 then
      raw = dir * math.clamp((norm - OSC.PAN_BASE_PIXELS) / OSC.PAN_FULL_PIXEL_DELTA * 100, 0, 100)
    end
  end
  local value = math.clamp(round(raw / OSC.PAN_STEP) * OSC.PAN_STEP, -100, 100)
  if math.abs(value) < OSC.PAN_STEP then value = 0 end
  return value, string.format("%d beside it (the project's %.0f), left %.0f, right %.0f, raw %.1f", n, n * area,
    left, right, raw)
end

-- The middles of the oscillator's three knobs, for the calibration picture: where the reader
-- looks, whether or not an oscillator is selected.
function K.oscMiddles(pl, marks)
  for _, key in ipairs({ "volume", "transpose", "fine_tune" }) do
    local p = OSC.CONTROLS[key]
    local x, y = pl:point(p[1], p[2])
    marks[#marks + 1] = { x = x, y = y }
  end
end

-- The rings of the oscillator's knobs `keys` (OSC.CONTROLS' names), read together (`rings`): each
-- { pts, info } or { nil, why }, by its place in `keys`.
local function oscRings(pl, snap, keys)
  local ps = {}
  for i, key in ipairs(keys) do ps[i] = OSC.CONTROLS[key] end
  return rings(pl, snap, ps, OSC.DETECT_MIN_R, OSC.DETECT_MAX_R, 125, 75)
end

-- Why a ring is not read, in the log's words: too few bright pixels, or no points at all.
local function tooFew(pts, info)
  return pts and (seen(pts, info) .. " — too few") or ("not read: " .. tostring(info))
end

-- The pointer of a knob read as Transpose is, its end stops at 5° and 265° round for -48 and +48
-- semitones: its raw semitones and its words; `marks` gets where it points.
local function stops(pts, info, s, marks)
  local deg = rayOf(pts, s)
  local delta = (deg - 135) % 360
  if delta > 270 then delta = 270 end
  marks[#marks + 1] = tip(info, deg <= 180 and deg or deg - 360)
  return ((delta - 5) / 260) * 96 - 48, string.format("%s, pointer at %d° (%d° round)", seen(pts, info), deg, delta)
end

-- Level, at (346, 68), off its ring `r` ({ pts, info } or { nil, why }): volume from the pointer's
-- angle, pan from the pixels beside it — into `out` (each nil where it was not read). Its words.
local function level(pl, r, out, marks)
  local s = pl.s
  local pts, info = r[1], r[2]
  if not (pts and #pts * info.area >= BRIGHT_MIN) then return "Level " .. tooFew(pts, info) end
  local deg = rayOf(pts, s)
  local delta = (deg - 135) % 360
  if delta > 270 then delta = 270 end
  local raw = delta / 270 * 200
  out.volume = math.clamp(round(raw / OSC.VOLUME_STEP) * OSC.VOLUME_STEP, 0, 200)
  local angle = deg <= 180 and deg or deg - 360
  local pan, pw = sideValue(pts, info.area, s, math.cos(math.rad(angle)), math.sin(math.rad(angle)))
  out.pan = pan
  marks[#marks + 1] = tip(info, angle)
  return string.format("Level %s, pointer at %d° (%d° round), volume %d %%; pan: %s — %d",
    seen(pts, info), deg, delta, out.volume, pw, pan)
end

-- Transpose, at (346, 114), off its ring `r`: its semitones, rounded to 12, into `out.transpose`.
-- Its words.
local function transposeKnob(pl, r, out, marks)
  local pts, info = r[1], r[2]
  if not (pts and #pts * info.area >= BRIGHT_MIN) then return "Transpose " .. tooFew(pts, info) end
  local raw, tw = stops(pts, info, pl.s, marks)
  local st = round(raw / OSC.TRANSPOSE_STEP) * OSC.TRANSPOSE_STEP
  if st >= -1 and st <= 1 then
    st = 0
  elseif st <= -47 then
    st = -48
  elseif st >= 47 then
    st = 48
  end
  out.transpose = math.clamp(st, -48, 48)
  return string.format("Transpose %s, %.1f semitones — %d", tw, raw, out.transpose)
end

-- The third knob, at (346, 160), read ONCE for both of its values: Fine Tune, read as Transpose
-- is, its semitones taken as cents; and Random or Alternate Pitch, the pixels beside its pointer
-- taken as pointing up — left Random, right Alternate. The project's reading, which holds while
-- Fine Tune is centred: a Fine Tune pointer turned away from the top is itself counted beside the
-- upright one, and read as Random or Alternate Pitch (TODO.md, "VPS Avenger, the page"). The
-- project captured and read this knob twice, the same square both times. Off its ring `r`, into
-- `out.fineTune` and `out.randomPitch`; its words.
local function fineKnob(pl, r, out, marks)
  local pts, info = r[1], r[2]
  if not (pts and #pts * info.area >= BRIGHT_MIN) then
    local why = tooFew(pts, info)
    return "Fine Tune " .. why .. "; Random Pitch " .. why
  end
  local fraw, fw = stops(pts, info, pl.s, marks)
  local cents = fraw / 48 * 100
  local c = round(cents / OSC.FINE_TUNE_STEP) * OSC.FINE_TUNE_STEP
  if c > -OSC.FINE_TUNE_STEP and c < OSC.FINE_TUNE_STEP then c = 0 end
  out.fineTune = math.clamp(c, -100, 100)
  local value, rw = sideValue(pts, info.area, pl.s, 0, -1)
  out.randomPitch = value
  return string.format("Fine Tune %s, %.1f cents — %d; Random Pitch %s; %s — %d", fw, cents, out.fineTune,
    seen(pts, info), rw, value)
end

-- The selected oscillator's main controls, into `out`: volume (0 to 200 %), pan (-100 to 100),
-- transpose (semitones, -48 to 48), fineTune (cents, -100 to 100) and randomPitch (-100 to 100),
-- each nil where it was not read — three rings, one a knob, off one grid. The log's words; `marks`
-- gets where each pointer was read to.
function K.osc(pl, snap, out, marks)
  local rs = oscRings(pl, snap, { "volume", "transpose", "fine_tune" })
  return table.concat({ level(pl, rs[1], out, marks), transposeKnob(pl, rs[2], out, marks),
    fineKnob(pl, rs[3], out, marks) }, "; ")
end

-- ── One knob, off a picture of its own ───────────────────────────────────────
-- What the page's amount reads after its Return (src/amount.luau): ONE knob off ONE small picture
-- that holds its ring — a few hundred points at most, as above, and the same arithmetic.

-- The picture each knob is read from: a box this big round its middle, in table points at 50 %.
-- The rings reach 28 and 16 of them from the middle at every zoom and on every display (the
-- project's radii of 28 and 16 of its pixels, one to a table point at 50 %), with room round them.
K.BOX = { macro = { 60, 60 }, osc = { 36, 36 } }

-- Knob `which`'s middle, a table point at 50 %, and the box round it (K.BOX) as a table rectangle:
-- "macro1" to "macro3", or a control of the oscillator's (OSC.CONTROLS: volume, pan, transpose,
-- fine_tune, random_pitch).
function K.place(which)
  local field = M.FIELDS[which]
  local p, size
  if field and string.match(which, "^macro%d$") then
    p, size = { field[1], M.AMOUNT_Y }, K.BOX.macro
  else
    p, size = OSC.CONTROLS[which], K.BOX.osc
  end
  if not p then error("no knob " .. tostring(which), 2) end
  return p, { p[1] - size[1] / 2, p[2] - size[2] / 2, p[1] + size[1] / 2, p[2] + size[2] / 2 }
end

-- Knob `which` (K.place's) read off snapshot `snap`, which holds its ring: the fields read, each nil
-- where it was not — { amount } for a macro knob, { volume, pan } for Level (Volume or Pan),
-- { transpose }, and { fineTune, randomPitch } for the third knob (Fine Tune or Random Pitch) —
-- and the log's words; `marks` gets where its pointer was read to.
function K.read(pl, snap, which, marks)
  local i = string.match(which, "^macro(%d)$")
  if i then
    local amount, words = K.macro(pl, snap, tonumber(i), marks)
    return { amount = amount }, words
  end
  local out = {}
  if which == "volume" or which == "pan" then return out, level(pl, oscRings(pl, snap, { "volume" })[1], out, marks) end
  if which == "transpose" then return out, transposeKnob(pl, oscRings(pl, snap, { "transpose" })[1], out, marks) end
  if which == "fine_tune" or which == "random_pitch" then
    return out, fineKnob(pl, oscRings(pl, snap, { "fine_tune" })[1], out, marks)
  end
  error("no knob " .. tostring(which), 2)
end

return K
