-- The central tab selected, read off the page's picture: of the four the avenger_control project
-- has points for — EDITOR, ARP, DRUM SQ and STEP SQ — the one whose bottom strip is darkest, when
-- it is clearly the darkest.
--
-- SOURCE: the project's detect_central_tab (its clicker/__init__.py), taken over with its
-- developer's permission (NOTICE): its box round each tab's point, its darkest-strip rule and its
-- margin, as a column profile of one picture (read/picture.luau). The project asks it only while
-- the Drums tab is selected, and so does this module (read/open.luau).

local P = host.include("src/read/picture.luau")
local C = host.include("src/tables/centre.luau")

local Z = {}

-- The darkest tab has to be this much darker than the next, or no tab is taken as selected.
local MARGIN = 4

-- The row of the four tabs: from the first tab's box's left to the last's right.
local function row()
  local half = C.TAB_SIZE[1] / 2
  local y = C.TABS.editor[2]
  return { C.TABS.editor[1] - half, y - C.TAB_SIZE[2] / 2, C.TABS.step_sq[1] + half, y + C.TAB_SIZE[2] / 2 }
end
Z.ROW = row()

function Z.ask(pl, out)
  local r = pl:rect(Z.ROW)
  if not r then return end
  local rows = P.bottomRows(pl, r[4] - r[2])
  out[#out + 1] = P.profile("centre band", { r[1], r[4] - rows, r[3], r[4] })
end

-- The tab selected into `v.central` — "editor", "arp", "drum_sq" or "step_sq" — or nothing.
function Z.decide(pl, got, v, log, marks)
  local prof = got["centre band"]
  if not prof then
    log("centre: not read (" .. tostring(got.why["centre band"]) .. ")")
    return
  end
  local ranked, words = {}, {}
  for _, key in ipairs(C.TAB_ORDER) do
    local p = C.TABS[key]
    local x1 = pl:point(p[1] - C.TAB_SIZE[1] / 2, p[2])
    local x2 = pl:point(p[1] + C.TAB_SIZE[1] / 2, p[2])
    local m = P.bandMean(prof, x1, x2)
    if m then ranked[#ranked + 1] = { key = key, mean = m } end
    words[#words + 1] = string.format("%s %s", key, m and string.format("%.1f", m) or "not read")
    local mx, my = pl:point(p[1], p[2])
    marks[#marks + 1] = { x = mx, y = my }
  end
  table.sort(ranked, function(a, b) return a.mean < b.mean end)
  local margin = nil
  if #ranked > 0 then
    margin = (#ranked > 1 and ranked[2].mean or 999) - ranked[1].mean
    if margin >= MARGIN then v.central = ranked[1].key end
  end
  log(string.format("centre: the tabs' bottom strips %s; the darkest %s by %s — %s", table.concat(words, ", "),
    ranked[1] and ranked[1].key or "none", margin and string.format("%.1f", margin) or "nothing",
    v.central or ("none taken, under " .. MARGIN)))
end

return Z
