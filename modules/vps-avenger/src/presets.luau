-- The preset database, as this overlay uses it — when it is installed.
--
-- WHAT IT IS. com.platform.vps-avenger-presets: the catalog of VPS Avenger's expansions — the
-- commercial ones, as the catalog lists them, with their categories, presets, oscillator tab
-- names and macro names — from the avenger_control project's database, given by its developer
-- without restriction. An OPTIONAL dependency: without it `DB.present` is false, nothing here is
-- asked, and the module is stage 1 exactly.
--
-- WHAT IT MAY DO (the maintainer's decision of 2026-09-28). This module works from the screen,
-- and may orient itself on the database as the project's tool does. Whatever it says that was not
-- read off the screen is MARKED: ", from the database" after the value — or before a list of
-- names, where a trailing marker would sound like one more name. So a user always hears which
-- words Avenger showed and which the database supplied.
--
-- SNAPPING. A name read off the screen is looked up in the catalog, as the project's tool looks
-- it up (its preset_db.find_match), in this order:
--   1. the same text, with runs of spaces as one and the acute accent ´ (which five catalog names
--      use as an apostrophe), ` ‘ and ’ all read as ' — a recogniser reads that accent as ';
--   2. the same, ignoring case (ASCII, and the capitals of Latin-1: "PL Üsch");
--   3. the closest name by Ratcliff/Obershelp similarity (Python's difflib ratio, which the tool
--      uses) of at least THRESHOLD, the tool's 0.72, first among the presets of the expansion the
--      module last knew Avenger in; then among all of them, where the closest also has to be
--      closer than every other name by MARGIN;
-- never "Init Preset" (Avenger's blank preset, in no expansion), and never a name that begins
-- with the closest catalog name and goes on ("BA Deep Sub mine"): the tool's rule, because that is
-- how people name their own variant of a catalog preset. 1 and 2 are what was READ, said in the
-- catalog's spelling (the same words) and not marked; 3 is the database's reading of a poor read,
-- and is marked.
--
-- Two differences from the tool, both against saying a catalog name the screen did not show. The
-- tool compares raw text, case and all; this compares the folded text, which takes a poor read
-- for its name more often and a name the catalog does not have for another more often too. The
-- tool looks in its own store of the user's presets first; this module has none. So a name the
-- catalog does not have — the user's own preset, an expansion released after the export — was
-- taken for another catalog name by the whole-catalog search in 71 % of 279 names tried (catalog
-- names left out one expansion at a time, 2026-09-28), and MARGIN brings that to 29 %, while reads
-- of a catalog name with one to three errors are still found in 92 % (98 without it) and none of
-- them for the wrong name (3 without it). In the known expansion, where the catalog's order makes
-- a poor read's name likely, THRESHOLD alone decides, as in the tool. And after Load preset or
-- Save preset as, where Avenger most likely shows the user's own file, a read is looked up by 1
-- and 2 only until a step shows another name (DB.forget; the tool's skip_db_snap).
--
-- WHERE. A name the catalog has once is one position: an expansion and a place in its list. 597
-- names are in several expansions (one in 26), and 19 differ from another only in case: such a
-- name is placed in the expansion the module last knew Avenger in when it is there once, and
-- otherwise not placed at all — the Preset info then names the expansions it is in.
--
-- THE ANCHOR is the position of the preset Avenger shows, as far as this module knows: set by
-- every name read that is placed (the Preset info's read, and the reads before and after ◀ and
-- ▶), cleared by a name read that is not in the catalog, and moved by a step as below. Forgotten
-- (DB.forget) whenever the preset may have changed some other way: a MENU item chosen, the redo
-- list opened, the overlay leaving the front. Not after Undo, which is the runtime's hotspot and
-- tells this module nothing (TODO.md). A read that fails keeps the anchor, and that is where a
-- stale one shows — an Undo's, or the database's N taken for the position after a step whose
-- read-back failed (below): a step whose name before the click cannot be read takes its N from
-- the anchor, and Preset info whose read fails speaks from it. Everything taken from it is
-- marked, and the next name read replaces it.
--
-- THE NEIGHBOUR RULE for ◀ and ▶ (main.luau reads the names; this decides what is said). Before
-- the click the name is read and placed as above. N is the preset one place before or after the
-- anchor in its expansion's list — the list's own order, category block after category block,
-- which is NOT alphabetical — or none: without an anchor; at the first or last preset of an
-- expansion, where Avenger goes on into whichever expansion comes next in the user's own library,
-- which the catalog cannot know (the anchor goes as well, unless the name is read unchanged); and
-- for a press that overtakes the step before it while that one's name has not yet been seen to
-- change, when the name read is the one that step started from, or none — it may be the screen
-- from before the earlier click, and a neighbour of it would be one preset behind (the anchor
-- goes). After the click the name is read back exactly as without the database (main.luau:
-- O:watch, no timer deciding anything), and then:
--   * nothing read by the end of the watch — with N: "<N>, from the database", and the anchor
--     moves to N (the click was sent, and Avenger moves one preset per click, as the tool walks
--     its position; the next name read confirms or replaces it); without N: as without the
--     database;
--   * a name read that is N — the same words; or, when the name CHANGED on screen, the read is no
--     catalog name as it stands, and N is at least as close to it as any other name the lookup
--     found: N in the catalog's spelling, marked only when it took similarity; the anchor moves
--     to N. A read that is another catalog name as it stands ("LD Lead 3" where N is "LD Lead
--     1") is that name, however similar the two, and so is a read closer to another name;
--   * a name read that is not N: the read-back name — in the catalog's spelling when it is a
--     catalog name as it stands, otherwise AS READ, because the database has just been wrong
--     about this step and its reading of a poor read would make both halves its own — then
--     ", unchanged" when it is the name from before the click, then "; <N>, from the database";
--     the screen wins, so the anchor is where the read name is placed (unchanged: where it was),
--     and none for a name that changed and is in the catalog only by similarity;
--   * a name read with no N: as without the database, the name snapped (marked when by
--     similarity).
-- A step another press has overtaken is not decided here at all (main.luau).
--
-- COST, all of it on the event loop. Nothing is read until the first ◀, ▶ or Preset info press
-- after the module loads — for ◀ and ▶, once the click is sent. That press reads, decodes and
-- indexes the index (0.3 MB, 17,571 names), once, and the log says how long it took; an
-- expansion's file (5 to 110 KB) is read at the first Preset info in it. Measured in a debug test
-- build against the real data on 2026-09-28: the index 30 to 50 ms to read and decode and about
-- 100 to index, the largest file 25, a first Preset info 150 to 190 in all; a name the catalog
-- has, under a millisecond; one found by similarity in the anchor's expansion, a few; one
-- compared with the whole catalog — a name it does not have, the user's own preset — 150 to 300
-- ms for ten made-up names, of which MARGIN's extra comparisons are about a tenth. The release
-- build that ships is not measured (TODO.md). The last SNAPS_MAX answers are kept, so a name is
-- compared with the catalog once, not before and after every step.

local P = host.tryRequire("com.platform.vps-avenger-presets")

local DB = {}

-- Whether the database is installed. main.luau adds the Preset info control only then.
DB.present = P ~= nil

DB.MARK = "from the database"

-- The similarity a read has to reach to be taken for a catalog name: the project's tool's
-- DEFAULT_MATCH_THRESHOLD, chosen by experiment there to take "AR Bond Pess Side" for "AR Band
-- Pass Slide" and not two different names for one another.
local THRESHOLD = 0.72

-- How much closer than every other name the closest has to be in the search over the whole
-- catalog (SNAPPING): "LD Lead 4", as like "LD Lead 1" as "LD Lead 2", is none of them.
local MARGIN = 0.05

-- At most this many expansions are named for a name that is in several.
local LIST_MAX = 5

-- A position is one number: the expansion's place in the index × SPAN + the preset's place in
-- its list (the longest list is 978). A number rather than a table, because the catalog has
-- 17,571 of them.
local SPAN = 65536
local function E(p) return p // SPAN end
local function I(p) return p % SPAN end

local cat = nil -- the index and its lookups, once built
local catWhy = nil -- why there is none, once that is known
local anchor = nil -- the position of the preset Avenger shows, as far as this module knows
local from = nil -- the position the latest step started from, or nil (DB.before)
local ownName = false -- after Load or Save as: a read is looked up as the same words only

-- ── Text ─────────────────────────────────────────────────────────────────────

-- The form names are compared in, and said in: runs of spaces as one space, none at either end
-- (50 catalog names end in a space or hold two), and ´ ` ‘ ’ as '. A name with none of that — most
-- of them — comes back as it is after four searches.
local function tidy(s)
  s = s or ""
  if not string.find(s, "[%c`\194\226]") and not string.find(s, "  ", 1, true)
    and string.byte(s, 1) ~= 32 and string.byte(s, -1) ~= 32 then
    return s
  end
  local t = string.gsub(s, "\194\180", "'")
  t = string.gsub(t, "\226\128[\152\153]", "'")
  t = string.gsub(t, "`", "'")
  t = string.gsub(t, "%s+", " ")
  t = string.gsub(t, "^ ", "")
  return (string.gsub(t, " $", ""))
end

-- A tidy name in small letters: ASCII, and Latin-1's capitals À to Þ (two bytes in UTF-8, the
-- second 0x80 to 0x9E; not ×, 0x97).
local function folded(t)
  local f = string.lower(t)
  if not string.find(f, "\195", 1, true) then return f end
  return (string.gsub(f, "\195([\128-\158])", function(c)
    local b = string.byte(c)
    if b == 151 then return nil end
    return "\195" .. string.char(b + 32)
  end))
end

local function fold(s) return folded(tidy(s)) end

-- "A, B and C", or the first LIST_MAX and how many more.
local function listOf(names)
  local shown = {}
  for k = 1, math.min(#names, LIST_MAX) do shown[k] = names[k] end
  if #names > LIST_MAX then
    return table.concat(shown, ", ") .. string.format(" and %d more", #names - LIST_MAX)
  end
  if #shown == 1 then return shown[1] end
  return table.concat(shown, ", ", 1, #shown - 1) .. " and " .. shown[#shown]
end

-- ── Similarity ───────────────────────────────────────────────────────────────
-- Ratcliff/Obershelp, as difflib.SequenceMatcher(None, a, b).ratio() computes it for strings as
-- short as preset names (under 200 characters, so difflib's junk heuristic is off): the longest
-- common block, earliest in `a` and then in `b` among equals, then the same on either side of it;
-- 2 × the characters matched / both lengths. Over bytes, so a letter outside ASCII counts twice.
-- The buffers are reused: a comparison makes no tables.

local bufA, bufB, rowA, rowB = {}, {}, {}, {}

local function fill(buf, s)
  local n = #s
  for k = 1, n do buf[k] = string.byte(s, k) end
  return n
end

local function longest(a, b, alo, ahi, blo, bhi)
  local bi, bj, bk = alo, blo, 0
  local prev, cur = rowA, rowB
  for j = blo - 1, bhi do prev[j] = 0 end
  for i = alo, ahi do
    local ai = a[i]
    cur[blo - 1] = 0
    for j = blo, bhi do
      if ai == b[j] then
        local k = prev[j - 1] + 1
        cur[j] = k
        if k > bk then bi, bj, bk = i - k + 1, j - k + 1, k end
      else
        cur[j] = 0
      end
    end
    prev, cur = cur, prev
  end
  return bi, bj, bk
end

local function matched(a, b, alo, ahi, blo, bhi)
  if alo > ahi or blo > bhi then return 0 end
  local i, j, k = longest(a, b, alo, ahi, blo, bhi)
  if k == 0 then return 0 end
  return k + matched(a, b, alo, i - 1, blo, j - 1) + matched(a, b, i + k, ahi, j + k, bhi)
end

local function ratio(x, y)
  local n = #x + #y
  if n == 0 then return 1 end
  return 2 * matched(bufA, bufB, 1, fill(bufA, x), 1, fill(bufB, y)) / n
end

-- The closest of folded names `list` to folded read `f`, at THRESHOLD or above: the name and its
-- ratio; the first of equals wins. Third, the closest ratio of any OTHER name it compared: with
-- `margin`, every name within `margin` of the closest is compared, so the caller can see whether
-- the closest stands out (0: nothing else came near). Two bounds come first, as difflib's
-- get_close_matches uses them — the lengths alone, and the letters the two have in common in any
-- order — and neither is ever below the ratio, so they only save work: a name is compared when it
-- could be the closest, or with `margin`, within `margin` of it.
local avail = {}
local function closest(f, list, margin)
  local qn = #f
  if qn == 0 or not list then return nil end
  margin = margin or 0
  local qh = {}
  for k = 1, qn do
    local c = string.byte(f, k)
    qh[c] = (qh[c] or 0) + 1
  end
  fill(bufA, f)
  local best, bestR, second = nil, THRESHOLD, 0
  for _, s in ipairs(list) do
    local cn = #s
    local need = bestR - margin
    if 2 * math.min(qn, cn) / (qn + cn) >= need then
      local m = 0
      for k = 1, cn do
        local c = string.byte(s, k)
        bufB[k] = c
        local left = avail[c]
        if left == nil then left = qh[c] or 0 end
        avail[c] = left - 1
        if left > 0 then m += 1 end
      end
      for k = 1, cn do avail[bufB[k]] = nil end
      if 2 * m / (qn + cn) >= need then
        local r = 2 * matched(bufA, bufB, 1, qn, 1, cn) / (qn + cn)
        if r >= THRESHOLD and (best == nil or r > bestR) then
          if best and bestR > second then second = bestR end
          best, bestR = s, r
        elseif r > second then
          second = r
        end
      end
    end
  end
  return best, bestR, second
end

-- ── The catalog ──────────────────────────────────────────────────────────────

-- The index and its lookups, built at the first call: `exact` (tidy name -> positions), `byFold`
-- (folded name -> positions), `uniq` (every folded name once, in catalog order) and `folds` (per
-- expansion, its folded names in its list's order). nil and why when the database cannot be read;
-- then never asked again until the module is reloaded, and ◀ and ▶ work without it.
local function catalog()
  if cat then return cat end
  if catWhy then return nil, catWhy end
  if not P then
    catWhy = "not installed"
    return nil, catWhy
  end
  local t0 = host.now()
  local ok, idx, why = pcall(P.index)
  if not ok then idx, why = nil, tostring(idx) end
  if idx then
    for _, x in ipairs(idx.expansions) do
      if #x.presets >= SPAN then
        idx, why = nil, string.format("%s lists %d presets, more than a position holds", x.name, #x.presets)
        break
      end
    end
  end
  if not idx then
    catWhy = tostring(why or "no index")
    host.log.info("[avenger] the preset database cannot be read: " .. catWhy
      .. " — Previous and Next work without it, and Preset info says so")
    return nil, catWhy
  end
  local exact, byFold, uniq, folds = {}, {}, {}, {}
  local n = 0
  for e, x in ipairs(idx.expansions) do
    local own = table.create(#x.presets)
    for i, name in ipairs(x.presets) do
      local p = e * SPAN + i
      local t = tidy(name)
      local f = folded(t)
      own[i] = f
      local l = exact[t]
      if l then l[#l + 1] = p else exact[t] = { p } end
      local g = byFold[f]
      if g then
        g[#g + 1] = p
      else
        byFold[f] = { p }
        uniq[#uniq + 1] = f
      end
      n += 1
    end
    folds[e] = own
  end
  cat = { idx = idx, exact = exact, byFold = byFold, uniq = uniq, folds = folds }
  host.log.info(string.format("[avenger] the preset database: %d expansions, %d presets, exported %s — "
    .. "read and indexed in %d ms", #idx.expansions, n, tostring(idx.exported), host.now() - t0))
  return cat
end

local function nameAt(p) return cat.idx.expansions[E(p)].presets[I(p)] end
local function expansionOf(p) return cat.idx.expansions[E(p)].name end

local function where(p)
  if not p then return "no position" end
  return string.format("%s %d of %d", expansionOf(p), I(p), #cat.idx.expansions[E(p)].presets)
end

-- Read text `text` in the catalog (see SNAPPING): { positions, how = "exact" | "folded" |
-- "fuzzy", ratio } — or nil, why not, and for a name of its own the ratio of the catalog name it
-- begins with (the neighbour rule weighs N against it). Found by similarity in the anchor's
-- expansion, the positions are that expansion's only.
local function lookUp(t, f)
  local ps = cat.exact[t]
  if ps then return { positions = ps, how = "exact", ratio = 1 } end
  ps = cat.byFold[f]
  if ps then return { positions = ps, how = "folded", ratio = 1 } end
  if ownName then
    return nil, "not in the catalog as it stands — after Load or Save as, not looked for by similarity"
  end
  local best, r, second, within = nil, nil, nil, nil
  if anchor then
    best, r = closest(f, cat.folds[E(anchor)])
    within = best and E(anchor)
  end
  if not best then best, r, second = closest(f, cat.uniq, MARGIN) end
  if not best then return nil, "not in the catalog" end
  ps = cat.byFold[best]
  if within then
    local only = {}
    for _, p in ipairs(ps) do
      if E(p) == within then only[#only + 1] = p end
    end
    ps = only
  end
  if #f > #best and string.sub(f, 1, #best) == best then
    return nil, string.format("a name of its own that begins with '%s'", tidy(nameAt(ps[1]))), r
  end
  if second and r - second < MARGIN then
    return nil, string.format("not in the catalog: '%s' (%.2f) is no closer than another name (%.2f)",
      tidy(nameAt(ps[1])), r, second)
  end
  return { positions = ps, how = "fuzzy", ratio = r }
end

-- The last answers, by text, by the anchor's expansion and by whether similarity is looked for
-- (the things besides the text that the answer depends on): a step's name read after the click is
-- the next step's name read before it, and a name the catalog does not have — the user's own
-- preset — would otherwise be compared with every catalog name twice per step. At most
-- SNAPS_MAX, all dropped when that is reached.
local SNAPS_MAX = 64
local snaps, snapCount = {}, 0

local function snap(text)
  local t = tidy(text)
  if t == "" then return nil, "nothing read" end
  local f = folded(t)
  if f == "init preset" or f == "init" then return nil, "Avenger's blank preset" end
  local key = t .. "\0" .. (anchor and E(anchor) or 0) .. (ownName and "\0own" or "")
  local kept = snaps[key]
  if kept then return kept.res, kept.why, kept.near end
  local res, why, near = lookUp(t, f)
  if snapCount >= SNAPS_MAX then snaps, snapCount = {}, 0 end
  snaps[key], snapCount = { res = res, why = why, near = near }, snapCount + 1
  return res, why, near
end

-- The one position of snap result `res`: the anchor itself when the name is there (Avenger has
-- not moved), else its one place in the anchor's expansion, else its one place at all; nil when
-- that leaves more than one.
local function pick(res)
  local ps = res.positions
  if anchor then
    local inside = nil
    for _, p in ipairs(ps) do
      if p == anchor then return p end
      if E(p) == E(anchor) then
        if inside == nil then inside = p else inside = false end
      end
    end
    if inside then return inside end
  end
  if #ps == 1 then return ps[1] end
  return nil
end

-- A snap result as it is said: the catalog's spelling at `p` (or its first place), marked when
-- it was found by similarity.
local function spoken(res, p)
  local name = tidy(nameAt(p or res.positions[1]))
  if res.how == "fuzzy" then return name .. ", " .. DB.MARK end
  return name
end

local function how(res)
  if res.how == "fuzzy" then return string.format("by similarity %.2f", res.ratio) end
  return res.how == "exact" and "exactly" or "ignoring case"
end

-- ── What the overlay asks ────────────────────────────────────────────────────

-- The preset may have changed in a way this module does not read: the anchor goes. `own`: true
-- when Avenger most likely shows the user's own file now (Load preset, Save preset as — SNAPPING),
-- false when it does not (Initialize preset), nil to leave that as it was (the redo list, the
-- overlay leaving the front, Save preset, which keeps the name).
function DB.forget(why, own)
  if anchor and cat then
    host.log.info(string.format("[avenger] the database's position (%s) is forgotten: %s", where(anchor), why))
  end
  anchor = nil
  from = nil
  if own ~= nil then ownName = own end
end

-- A step of ◀ (dir -1) or ▶ (dir 1): `read` is the name read just before its click, or nil;
-- `overtaking` whether the step before this one is still waiting for its name to change
-- (main.luau). Places the name, and says which neighbour the database expects. nil when the
-- database cannot be used — the step is then the stage-1 step; otherwise the step's record for
-- DB.after. main.luau calls it once the click is sent, so that the first use's index load never
-- holds the click back; it depends on nothing the click changes.
function DB.before(label, read, dir, overtaking)
  if not catalog() then return nil end
  if read then
    local res, why = snap(read)
    anchor = res and pick(res) or nil
    host.log.info(string.format("[avenger] '%s': read '%s' before the click — %s (%s)", label, read,
      res and (anchor and where(anchor) or "in the catalog more than once, not placed") or "not placed",
      res and how(res) or tostring(why)))
  end
  -- The step before has not seen its name change yet, so the screen read now may still be the
  -- one from before that step's click: when it places the preset where that step started — or
  -- nothing, or that step's start was not known — its neighbour would be one preset behind.
  if overtaking and anchor and (from == nil or anchor == from) then
    host.log.info(string.format("[avenger] '%s': the step before has not settled, and the name may still be "
      .. "the one from before its click — no neighbour", label))
    anchor = nil
  end
  from = anchor
  local n = nil
  if anchor then
    local i = I(anchor) + dir
    if i >= 1 and i <= #cat.idx.expansions[E(anchor)].presets then n = anchor + dir end
  end
  host.log.info(string.format("[avenger] '%s': the database's neighbour: %s", label,
    n and string.format("'%s' (%s)", tidy(nameAt(n)), where(n))
      or (anchor and string.format("none, %s is at the %s of its expansion", tidy(nameAt(anchor)),
        dir < 0 and "start" or "end") or "none, the position is not known")))
  return { label = label, n = n, edge = anchor ~= nil and n == nil }
end

-- After the step: `now` is the name the watch ended on (nil: nothing read), `unchanged` whether
-- it is the name from before the click. What to say, by THE NEIGHBOUR RULE above — or nil to say
-- what the module says without the database (no neighbour, and nothing read).
function DB.after(step, now, unchanged)
  if not (step and cat) then return nil end
  local label, n = step.label, step.n
  -- From an expansion's first or last preset the click leads into an expansion the catalog
  -- cannot name, so the old one is neither the position nor where to look for the new name
  -- first, unless the name has not changed.
  if step.edge and not unchanged then anchor = nil end
  -- A name that changed on screen is no longer the one Load or Save as brought.
  if now ~= nil and not unchanged then ownName = false end
  local nName = n and tidy(nameAt(n))
  if now == nil then
    if not n then return nil end
    anchor = n
    host.log.info(string.format("[avenger] '%s': nothing read after the click — the database's '%s' said, "
      .. "marked", label, nName))
    return nName .. ", " .. DB.MARK
  end
  local res, why, near = snap(now)
  if n then
    -- The neighbour, when the read is its name as it stands; or, when the name changed and the
    -- read is no catalog name as it stands, when it is similar enough to the neighbour's and no
    -- other name the lookup found is closer — the database's expectation settles a poor read. A
    -- read that IS another catalog name ("LD Lead 3" for the expected "LD Lead 1") is that name,
    -- however similar, and one closer to another name ("LD Laed 3") is not the neighbour either.
    -- A name read unchanged is the screen saying the preset did not move.
    local f, fn = fold(now), folded(nName)
    local r = nil
    if f == fn then
      r = 1
    elseif not unchanged and not (res and res.how ~= "fuzzy")
      and not (#f > #fn and string.sub(f, 1, #fn) == fn) then
      r = ratio(f, fn)
      local closer = res and res.ratio or near
      if r < THRESHOLD or (closer and r < closer) then r = nil end
    end
    if r then
      anchor = n
      host.log.info(string.format("[avenger] '%s': read '%s', the database's neighbour %s", label, now,
        r == 1 and "exactly" or string.format("by similarity %.2f", r)))
      return r == 1 and nName or (nName .. ", " .. DB.MARK)
    end
  end
  local p = res and pick(res) or nil
  local said = res and spoken(res, p) or tidy(now)
  -- Disagreeing with the neighbour, a name the catalog has only by similarity is said as read:
  -- the database was just wrong about this step, and the screen's words are what is left. And it
  -- places nothing, unless it is the name from before the click, which stays where it was.
  local asRead = n ~= nil and res ~= nil and res.how == "fuzzy"
  if asRead then
    said = tidy(now)
    if not unchanged then p = nil end
  end
  if unchanged then said = said .. ", unchanged" end
  anchor = p
  host.log.info(string.format("[avenger] '%s': read '%s'%s — %s (%s%s)%s", label, now,
    unchanged and ", as before the click" or "", p and where(p) or "not placed",
    res and how(res) or tostring(why), asRead and ", said as read" or "",
    n and string.format("; not the database's neighbour '%s', which is said as well", nName) or ""))
  if n then said = said .. "; " .. nName .. ", " .. DB.MARK end
  return said
end

-- The oscillators and macros of preset `i` of expansion file `x`, as said.
local function oscText(x, i)
  local names = {}
  for k, o in ipairs(P.oscillators(x, i)) do names[k] = o.name end
  return string.format("%d oscillator%s, %s: %s", #names, #names == 1 and "" or "s", DB.MARK,
    table.concat(names, ", "))
end

local function macroText(x, i)
  local tabs = P.macros(x, i)
  local function one(tab)
    local knobs, buttons = {}, {}
    for k, m in ipairs(tab.knobs) do knobs[k] = m.name end
    for k, m in ipairs(tab.buttons) do buttons[k] = m.name end
    return table.concat(knobs, ", ") .. "; buttons: " .. table.concat(buttons, ", ")
  end
  if #tabs == 1 then return "Macros, " .. DB.MARK .. ": " .. one(tabs[1]) end
  local parts = { string.format("%d macro tabs, %s", #tabs, DB.MARK) }
  for t, tab in ipairs(tabs) do parts[#parts + 1] = string.format("Tab %d: %s", t, one(tab)) end
  return table.concat(parts, ". ")
end

-- The Preset info: `read` is the name read just now, or nil. Sentences, ". " between them:
--   the name — as read and snapped (marked when by similarity); "<read>, not in the database";
--     or, nothing read, "the preset name cannot be read now; <anchor's name>, from the database";
--   "Expansion E, category C, k of n, from the database";
--   "m oscillators, from the database: …" — a tab not named is said as Avenger shows it, OSC n;
--   "Macros, from the database: three knobs; buttons: two", or per tab when there are several.
-- A name in several places the anchor cannot tell apart names its expansions instead: "<name>, in
-- 3 expansions, from the database: A, B and C" — the marker before the list, as everywhere. Placing
-- the name sets the anchor, as every read does.
function DB.info(read)
  if not catalog() then return "the preset database cannot be read" end
  local p, first
  if read then
    local res, why = snap(read)
    if not res then
      anchor = nil
      host.log.info(string.format("[avenger] 'Preset info': read '%s' — %s", read, tostring(why)))
      return tidy(read) .. ", not in the database"
    end
    p = pick(res)
    anchor = p
    first = spoken(res, p)
    host.log.info(string.format("[avenger] 'Preset info': read '%s' — %s (%s)", read,
      p and where(p) or "in the catalog more than once, not placed", how(res)))
    if not p then
      local names, seen = {}, {}
      for _, q in ipairs(res.positions) do
        local e = expansionOf(q)
        if not seen[e] then
          seen[e] = true
          names[#names + 1] = e
        end
      end
      if #names == 1 then
        return string.format("%s, %d times in %s, %s", first, #res.positions, names[1], DB.MARK)
      end
      return string.format("%s, in %d expansions, %s: %s", first, #names, DB.MARK, listOf(names))
    end
  else
    if not anchor then return "the preset name cannot be read now" end
    p = anchor
    first = string.format("the preset name cannot be read now; %s, %s", tidy(nameAt(p)), DB.MARK)
    host.log.info(string.format("[avenger] 'Preset info': nothing read — the position kept, %s", where(p)))
  end
  local parts = { first }
  local e = expansionOf(p)
  local ok, x, bad = pcall(P.expansion, e)
  if not ok then x, bad = nil, tostring(x) end
  if not x then
    host.log.info(string.format("[avenger] 'Preset info': %s's file cannot be read: %s", e, tostring(bad)))
    parts[#parts + 1] = string.format("Expansion %s, %s; its categories, oscillators and macros cannot be "
      .. "read from it", e, DB.MARK)
    return table.concat(parts, ". ")
  end
  local cname, k, count = P.category(x, I(p))
  if cname then
    parts[#parts + 1] = string.format("Expansion %s, category %s, %d of %d, %s", e, cname, k, count, DB.MARK)
  else
    parts[#parts + 1] = string.format("Expansion %s, %s", e, DB.MARK)
  end
  parts[#parts + 1] = oscText(x, I(p))
  parts[#parts + 1] = macroText(x, I(p))
  return table.concat(parts, ". ")
end

return DB
