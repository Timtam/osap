-- VPS Avenger's own editor over one of its name fields — the preset's name, an OSC tab's, a macro
-- knob's or button's — as the renames see it (the plan's slice 6, src/handover.luau). A double-click
-- on the field opens it: Avenger puts a text editor in the field's place with the old name
-- selected, and Return keeps what was typed there, Escape the old name — the avenger_control tool's
-- account (its actions.py), and JUCE's Label with its editor on a double-click. Avenger publishes no
-- element for it, so it is seen only in pictures of the field:
--
--   OPEN     a change wait on the field's PICTURE — the field and a margin round it, where an
--            outline at the editor's edge lies — from the picture before the double-click: a change
--            over a tenth of it that stands still for SETTLE ms is the editor. A hover outline, the
--            caret and a flash are not: a thin change is under the share, and a change that undoes
--            itself is no change (docs/api/screen.md, `settle`).
--   CLOSED   the field's EDGES — its top two rows and its bottom two, its left two columns and its
--            right two, each a line's mean colour in one profile of the field — against the same lines
--            before the double-click and with the editor open: nearer the picture before by MARGIN_D
--            is closed, nearer the open one by as much is open, and between the two unsure. The text
--            lies in none of them; the editor's background, outline and selection reach them.
--
-- Every number here is a starting value nobody has measured on an Avenger (TODO.md, "VPS Avenger",
-- U3). Each decision is logged with its numbers, and a calibrating run saves the field's three
-- pictures — before the double-click, the editor open, and at the end — once per kind of field and
-- zoom in a session. Nothing here clicks; every read is of a picture the caller took.

local O = host.require("com.platform.overlay")
local Open = host.include("src/read/open.luau")

local Ed = {}

-- The picture: the field grown by this much each way, in table units at 50 %.
Ed.MARGIN = 2
-- How long the editor's picture stands still before it counts: two pictures on a Mac, whose change
-- wait photographs every 33 ms.
Ed.SETTLE = 50
-- The share of the picture's pixels a change has to cover to be the editor's box.
Ed.OPEN_SHARE = 0.10
-- A channel this far apart is a changed pixel: the host's default.
Ed.TOLERANCE = 16
-- How long the editor is looked for: the longest one change wait may wait (docs/api/screen.md). A
-- bound on looking, not a guess: an editor that opens is answered when it opens.
Ed.WITHIN = 2000
-- How much nearer one reference the edges have to read than the other to be taken for it.
Ed.MARGIN_D = 6
-- The rows and columns read at each edge.
Ed.LINES = 2

local log = function(s) host.log.info("[avenger] [editor] " .. s) end

-- Field `rect` (a table rectangle at 50 %) with placer `pl`: { field, picture } on screen — the field,
-- and the field grown by MARGIN — each cut as a part's picture is (read/open.luau); nil when either
-- is not on the screen.
function Ed.place(pl, rect)
  local m = Ed.MARGIN
  local field = Open.region(pl, rect)
  local picture = Open.region(pl, { rect[1] - m, rect[2] - m, rect[3] + m, rect[4] + m })
  if not (field and picture) then return nil end
  return { field = field, picture = picture }
end

-- The change wait that sees the editor come — `from` the picture before the double-click — or, after
-- an end key, go (from a picture that still shows it): on the field's picture, a tenth of it.
function Ed.wait(from, at)
  local p = at.picture
  return { from = from, watch = { p }, timeout = Ed.WITHIN, settle = Ed.SETTLE, tolerance = Ed.TOLERANCE,
    minPixels = math.max(4, math.ceil(Ed.OPEN_SHARE * (p[3] - p[1]) * (p[4] - p[2]))) }
end

-- Whether a change wait's answer is the editor open: a picture that changed and stood still.
function Ed.isOpen(snap, info)
  return snap ~= nil and type(info) == "table" and info.changed == true and info.settled == true
end

-- The open decision, in the log: `name` the field's, with what the wait answered.
function Ed.logOpen(name, open, info, why)
  local i = type(info) == "table" and info or {}
  log(string.format("'%s': %s — changed %s, settled %s, after %s ms in %s picture(s)%s", name,
    open and "open" or "not open", tostring(i.changed), tostring(i.settled), tostring(i.waited), tostring(i.frames),
    why and ("; " .. tostring(why)) or ""))
end

-- The field's edges in snapshot `snap`, read in a handler where the profile waits: a list of
-- { r, g, b } — the top rows, the bottom rows, the left columns, the right columns — with `mean`,
-- the whole field's; or nil and why.
function Ed.edges(snap, at)
  local ok, p, why = pcall(host.screen.profile, { region = at.field, axes = "both", snapshot = snap })
  if not ok then return nil, tostring(p) end
  if not p then return nil, tostring(why) end
  local rows, cols = p.rows, p.columns
  local nr, nc, k = #rows.r, #cols.r, Ed.LINES
  if nr < 2 * k or nc < 2 * k then return nil, string.format("the field is %dx%d, too small for its edges", nc, nr) end
  local out = {}
  local function add(axis, i) out[#out + 1] = { axis.r[i], axis.g[i], axis.b[i] } end
  for i = 1, k do add(rows, i) end
  for i = nr - k + 1, nr do add(rows, i) end
  for i = 1, k do add(cols, i) end
  for i = nc - k + 1, nc do add(cols, i) end
  local m = { 0, 0, 0 }
  for i = 1, nr do
    m[1] += rows.r[i]
    m[2] += rows.g[i]
    m[3] += rows.b[i]
  end
  out.mean = { m[1] / nr, m[2] / nr, m[3] / nr }
  return out
end

-- How far apart two lists of colours are: the mean over them of |Δr| + |Δg| + |Δb|.
local function apart(a, b)
  local d = 0
  for i, x in ipairs(a) do
    local y = b[i]
    d += math.abs(x[1] - y[1]) + math.abs(x[2] - y[2]) + math.abs(x[3] - y[3])
  end
  return d / math.max(1, #a)
end

-- Whether the field's edges `now` read "closed", "open" or "unsure", against `before` and `open`
-- (Ed.edges of the pictures before the double-click and with the editor open); logged with the
-- distances, and the whole field's beside them for calibration.
function Ed.classify(name, now, before, open)
  local db, dop = apart(now, before), apart(now, open)
  local verdict = "unsure"
  if db + Ed.MARGIN_D < dop then
    verdict = "closed"
  elseif dop + Ed.MARGIN_D < db then
    verdict = "open"
  end
  log(string.format("'%s': %s — the edges %.1f from the picture before, %.1f from the editor open (margin %d); "
    .. "the whole field %.1f and %.1f", name, verdict, db, dop, Ed.MARGIN_D, apart({ now.mean }, { before.mean }),
    apart({ now.mean }, { open.mean })))
  return verdict
end

-- The pictures written this session, by name.
local shots = {}

-- In a calibrating run, the picture `stage` ("before", "open", "end") of field kind `kind` at
-- `zoom`, once per session: the field's picture, its corners marked. With a callback: the picture is
-- written on the image worker, and nothing waits for it.
function Ed.shot(kind, stage, snap, at, zoom)
  if not (snap and O.calibrating()) then return end
  local name = string.format("editor-%s-%s-%dpct", kind, stage, zoom or 0)
  if shots[name] then return end
  shots[name] = true
  local path = host.path("calibration/" .. name .. ".png")
  local f = at.field
  local ok, err = pcall(host.screen.saveMarked, path, { snapshot = snap, region = at.picture,
    marks = { { x = f[1], y = f[2] }, { x = f[3] - 1, y = f[4] - 1 } } }, function(saved, why)
      host.log.info(string.format("[avenger] [calibrate] the editor's %s picture of the %s field -> %s (%s)", stage,
        kind, path, saved and "true" or ("false: " .. tostring(why))))
    end)
  if not ok then host.log.info("[avenger] [calibrate] the editor's picture not asked for: " .. tostring(err)) end
end

return Ed
