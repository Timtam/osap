-- The knob reader: a knob's value from where its bright pointer points, read off the page's
-- picture. The three macro knobs' amounts, and the selected oscillator's Volume, Pan, Transpose,
-- Fine Tune and Random or Alternate Pitch.
--
-- SOURCE: the avenger_control project's knob readers (its clicker/__init__.py: the macro amount's
-- centre of brightness, the Level knob's ray, Transpose's and Fine Tune's end stops, and Pan's and
-- Random Pitch's extra pixels beside the pointer), taken over with its developer's permission
-- (NOTICE): its arithmetic and its numbers as it has them, worked in its own pixels.
--
-- WHAT CHANGED. The project looked at every pixel of a square round the knob. This reads the
-- ring it actually uses at a few hundred points of the one picture (host.screen.pixels on the
-- snapshot, answered at once): every pixel where they fit, and every second, third, … each way
-- where they do not, each point then standing for that many pixels. A point's place is turned
-- into the project's pixels (read/picture.luau, `t`), so its numbers — the ring's radii, the
-- ray's length and width, the counts — apply as they are. The search for the pointer's angle
-- scores the same 271 angles with the same weights, but adds each point only to the angles whose
-- ray passes near it, rather than every point to every angle. Python's round, which rounds a half
-- to the even neighbour, is kept. A knob with fewer bright pixels than the project's 8 is not
-- read — Random Pitch too, which the project read as 0 % from no pixels at all.

local P = host.include("src/read/picture.luau")
local OSC = host.include("src/tables/osc.luau")
local M = host.include("src/tables/macros.luau")

local K = {}

-- At most this many points of the picture per knob.
K.POINTS = 300

-- Fewer bright pixels than this (the project's), and the knob is not read.
local BRIGHT_MIN = 8

local round = P.round

-- The bright points of the ring round table point `p`, between the project's radii `minR` and
-- `maxR` (its pixels at 50 %, grown with its scale): { dx, dy, luma } in its pixels from the
-- knob's middle, for every point brighter than `lumaMin` whose channels are less than `satMax`
-- apart. And what was looked at: { points, stride, area (the project's pixels a point stands
-- for), cx, cy, rmax (in the picture's pixels) } — or nil and why.
local function ring(pl, snap, p, minR, maxR, lumaMin, satMax)
  -- From the knob's middle as it lies on screen to each pixel's middle: a pointer one pixel
  -- either side of a middle rounded to a pixel would lean.
  local ex, ey = pl:exact(p[1], p[2])
  local cx, cy = math.floor(ex + 0.5), math.floor(ey + 0.5)
  local rmin, rmax = minR * pl.s / pl.t, maxR * pl.s / pl.t
  local pts, stride = P.ring(cx, cy, rmin, rmax, K.POINTS)
  local ok, colours, why = pcall(host.screen.pixels, pts, { snapshot = snap })
  if not ok then return nil, tostring(colours) end
  if type(colours) ~= "table" then return nil, tostring(why) end
  local out = {}
  for i, c in ipairs(colours) do
    local luma = (c.r + c.g + c.b) / 3
    if luma > lumaMin and math.max(c.r, c.g, c.b) - math.min(c.r, c.g, c.b) < satMax then
      out[#out + 1] = { (pts[i][1] + 0.5 - ex) * pl.t, (pts[i][2] + 0.5 - ey) * pl.t, luma }
    end
  end
  return out, { points = #pts, stride = stride, area = (stride * pl.t) ^ 2, cx = cx, cy = cy, rmax = rmax }
end

local function seen(pts, info)
  return string.format("%d bright of %d points (%s; the project's %.0f pixels)", #pts, info.points,
    info.stride == 1 and "every pixel" or string.format("one in %d pixels each way", info.stride), #pts * info.area)
end

-- Where a knob read at angle `deg` (the project's: 0 is right, 90 down) points, for the picture.
local function tip(info, deg)
  local a = math.rad(deg)
  return { x = math.floor(info.cx + math.cos(a) * info.rmax + 0.5), y = math.floor(info.cy + math.sin(a) * info.rmax + 0.5) }
end

-- ── The macro knobs ──────────────────────────────────────────────────────────

-- Macro knob `index` (1 to 3): its amount, 0 to 100 in steps of 10, or nil; and the log's words.
-- The centre of the ring's bright pixels, those near the rim counting a third as much, is where
-- the pointer is; the knob runs 270° clockwise from 135° (down and left).
function K.macro(pl, snap, index, marks)
  local field = M.FIELDS["macro" .. index]
  local s = pl.s
  local pts, info = ring(pl, snap, { field[1], M.AMOUNT_Y }, M.AMOUNT_DETECT_MIN_R, M.AMOUNT_DETECT_MAX_R, 120, 75)
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
      for d = math.floor(c - spread), math.ceil(c + spread) do
        local delta = d % 360
        if delta <= 270 then
          local ux, uy = UX[delta + 1], UY[delta + 1]
          local proj = dx * ux + dy * uy
          if proj >= lo and proj <= hi then
            local perp = math.abs(dx * uy - dy * ux)
            if perp <= tol then
              score[delta + 1] += (l - 120) * math.max(0, 1 - perp / fall)
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

-- The selected oscillator's main controls, into `out`: volume (0 to 200 %), pan (-100 to 100),
-- transpose (semitones, -48 to 48), fineTune (cents, -100 to 100) and randomPitch (-100 to 100),
-- each nil where it was not read. The log's words; `marks` gets where each pointer was read to.
function K.osc(pl, snap, out, marks)
  local s = pl.s
  local words = {}
  local function knob(p)
    return ring(pl, snap, p, OSC.DETECT_MIN_R, OSC.DETECT_MAX_R, 125, 75)
  end

  -- Level: volume from the pointer's angle, pan from the pixels beside it.
  local pts, info = knob(OSC.CONTROLS.volume)
  if pts and #pts * info.area >= BRIGHT_MIN then
    local deg = rayOf(pts, s)
    local delta = (deg - 135) % 360
    if delta > 270 then delta = 270 end
    local raw = delta / 270 * 200
    out.volume = math.clamp(round(raw / OSC.VOLUME_STEP) * OSC.VOLUME_STEP, 0, 200)
    local angle = deg <= 180 and deg or deg - 360
    local pan, pw = sideValue(pts, info.area, s, math.cos(math.rad(angle)), math.sin(math.rad(angle)))
    out.pan = pan
    marks[#marks + 1] = tip(info, angle)
    words[#words + 1] = string.format("Level %s, pointer at %d° (%d° round), volume %d %%; pan: %s — %d",
      seen(pts, info), deg, delta, out.volume, pw, pan)
  else
    words[#words + 1] = "Level " .. (pts and (seen(pts, info) .. " — too few") or ("not read: " .. tostring(info)))
  end

  -- Transpose: the same ray, its end stops at 5° and 265° round for -48 and +48 semitones.
  local function transposeOf(p)
    local tp, ti = knob(p)
    if not (tp and #tp * ti.area >= BRIGHT_MIN) then
      return nil, tp and (seen(tp, ti) .. " — too few") or ("not read: " .. tostring(ti))
    end
    local deg = rayOf(tp, s)
    local delta = (deg - 135) % 360
    if delta > 270 then delta = 270 end
    marks[#marks + 1] = tip(ti, deg <= 180 and deg or deg - 360)
    return ((delta - 5) / 260) * 96 - 48, string.format("%s, pointer at %d° (%d° round)", seen(tp, ti), deg, delta)
  end
  local raw, tw = transposeOf(OSC.CONTROLS.transpose)
  if raw then
    local st = round(raw / OSC.TRANSPOSE_STEP) * OSC.TRANSPOSE_STEP
    if st >= -1 and st <= 1 then
      st = 0
    elseif st <= -47 then
      st = -48
    elseif st >= 47 then
      st = 48
    end
    out.transpose = math.clamp(st, -48, 48)
    words[#words + 1] = string.format("Transpose %s, %.1f semitones — %d", tw, raw, out.transpose)
  else
    words[#words + 1] = "Transpose " .. tw
  end

  -- Fine Tune: the third knob read as Transpose is, its semitones taken as cents.
  local fraw, fw = transposeOf(OSC.CONTROLS.fine_tune)
  if fraw then
    local cents = fraw / 48 * 100
    local c = round(cents / OSC.FINE_TUNE_STEP) * OSC.FINE_TUNE_STEP
    if c > -OSC.FINE_TUNE_STEP and c < OSC.FINE_TUNE_STEP then c = 0 end
    out.fineTune = math.clamp(c, -100, 100)
    words[#words + 1] = string.format("Fine Tune %s, %.1f cents — %d", fw, cents, out.fineTune)
  else
    words[#words + 1] = "Fine Tune " .. fw
  end

  -- Random or Alternate Pitch: the pixels beside the third knob's pointer, taken as pointing up —
  -- left Random, right Alternate. The project's reading, which holds while Fine Tune is centred:
  -- a Fine Tune pointer turned away from the top is itself counted beside the upright one, and
  -- read as Random or Alternate Pitch (TODO.md, "VPS Avenger, the page").
  local rp, ri = knob(OSC.CONTROLS.random_pitch)
  if rp and #rp * ri.area >= BRIGHT_MIN then
    local value, rw = sideValue(rp, ri.area, s, 0, -1)
    out.randomPitch = value
    words[#words + 1] = string.format("Random Pitch %s; %s — %d", seen(rp, ri), rw, value)
  else
    words[#words + 1] = "Random Pitch " .. (rp and (seen(rp, ri) .. " — too few") or ("not read: " .. tostring(ri)))
  end
  return table.concat(words, "; ")
end

return K
