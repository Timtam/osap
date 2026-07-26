-- WHERE a Kontakt can be, as a list of CELLS. Each cell is one place a Kontakt runs, and
-- gets its OWN overlay: {Kontakt 7, Kontakt 8} × {bare in a DAW, nested in Komplete
-- Kontrol, standalone}.
--
-- Why cells rather than one overlay that works out where it is: the answer never changes
-- while the window exists. A window IS a Kontakt 7 in a DAW, for as long as it is around.
-- That makes it the arbiter's business — decide once, then the overlay's control set,
-- coordinates and announcements are all fixed. The alternative (one overlay asking at
-- runtime) needs a version lookup, a `geom(o, key)` indirection, `when` predicates on
-- half the controls, and coordinates that are functions instead of numbers — all of it
-- to re-derive per keystroke something that was settled when the window opened.
--
-- Use a `when` predicate for the other kind of question: one whose answer changes WHILE
-- the overlay is active (which tab is at the front, whether the browser is open, whether
-- a library is loaded). The arbiter cannot re-elect on those without tearing the overlay
-- down and rebuilding it, which a screen-reader user hears.

local O = host.require("com.platform.overlay")
local daw = host.require("com.platform.daw-hosts")
-- OPTIONAL dependency: Komplete Kontrol, if it's installed. Present → its exports (incl.
-- the standalone-window matcher) are imported into this VM; absent → nil, and Kontakt-in-
-- standalone-KK detection is simply off (bare Kontakt in a DAW never needs KK).
local kk = host.tryRequire("com.platform.komplete-kontrol")
local D = host.include("src/detect.luau")
local geo = host.include("src/geometry.luau")

-- The embedded overlays compete on one slot, the standalone ones on another: a DAW with an
-- embedded Kontakt and a standalone Kontakt can be open at the same time, so those two
-- must be able to be active independently. Within a slot, only one cell can ever match.
local SLOT = "com.platform.kontakt"
local STANDALONE_SLOT = "com.platform.kontakt-standalone"

-- Hosts = the DAWs (from daw-hosts) PLUS, when Komplete Kontrol is installed, KK's own
-- standalone window: KK running on its own hosts a Kontakt exactly like a DAW does
-- (verified live — the KK NINormalWindow has the SAME two Qt children as DAW-embedded KK:
-- the NI-versioned KK container control and, inside it, the plain-Qt nested Kontakt). We
-- reuse KK's OWN window matcher rather than treating KK as a DAW or duplicating its
-- identity; if KK isn't installed, O.hosts omits it.
local HOSTS = O.hosts(daw.all, kk and kk.standaloneWindow)

-- A bare Kontakt of this version, with no Komplete Kontrol wrapping it. The `not isKK`
-- matters: a KK container reports the nested "Kontakt N" name in its own subtree, so an
-- identity check alone would match KK too.
local function bareOfVersion(vIdx)
  return function(c)
    return not D.isKK(c) and D.variantIndex(c) == vIdx and not D.kkPresent()
  end
end

-- A Komplete Kontrol hosting a Kontakt of this version. The overlay anchors on the KK
-- CONTAINER (that is the control the host window exposes), and reaches the Kontakt content
-- through the dynamic frame below.
local function kkHostingVersion(vIdx)
  return function(c)
    if not D.isKK(c) then return false end
    local kc = D.findNestedKontakt(c)
    return kc ~= nil and D.variantIndex(kc) == vIdx
  end
end

-- The Kontakt STANDALONE application window (Kontakt N.exe on its own): a top-level
-- NINormalWindow titled "Kontakt N". Mirrors ReaHotkey's Standalone.Register criteria.
-- Unlike a KK-nested Kontakt (a UIA leaf), a standalone Kontakt is fully UIA-accessible.
local function standaloneWindow(name)
  return {
    title = { pattern = "^" .. name },
    app = { exe = { contains = "Kontakt" } },
    windows = { class = { prefix = "NINormalWindow" } },
  }
end

-- Build the cells. `where` drives what a cell contains:
--   in-daw      the Kontakt header: file-menu actions, the arrows, K7's extra buttons
--   in-kk       Komplete Kontrol's chrome only — under KK you load instruments through
--               KK's browser, so Kontakt's own file menu is not what you want there
--   standalone  the file menu plus K7's extras, driven by UIA (which works there), and a
--               Tab pass-through of Kontakt's native focus ring
local CELLS = {}
for vIdx, v in ipairs(D.VARIANTS) do
  local G = geo.GEOMETRY[v.name]
  local ox, oy = v.offset[1], v.offset[2]
  local common = { version = v.name, geometry = G, uiaFileMenu = geo.UIA_FILE_MENU[v.name] }

  CELLS[#CELLS + 1] = setmetatable({
    key = v.name .. " in a DAW",
    where = "in-daw",
    binding = O.embedded({
      hosts = HOSTS,
      control = D.QT_PLUGIN,
      identify = bareOfVersion(vIdx),
      -- Relational (it asks whether a KK is also present), so it must NOT be cached per
      -- HWND: a Kontakt's handle is stable while a KK is loaded around it.
      cacheIdentity = false,
    }, { slot = SLOT, menus = true }),
    frame = D.constantFrame(ox, oy),
  }, { __index = common })

  -- Only when Komplete Kontrol is actually loaded: without it a Kontakt cannot BE inside
  -- one, and the cell would be a claim that can never match — plus its controls are KK's
  -- own exported part, which does not exist then.
  if kk then
    CELLS[#CELLS + 1] = setmetatable({
      key = v.name .. " in Komplete Kontrol",
      where = "in-kk",
      binding = O.embedded({
        hosts = HOSTS,
        control = D.QT_PLUGIN,
        identify = kkHostingVersion(vIdx),
        cacheIdentity = false,
      }, { slot = SLOT, menus = true }),
      frame = D.kkContentFrame(ox, oy),
    }, { __index = common })
  end

  CELLS[#CELLS + 1] = setmetatable({
    key = v.name .. " standalone",
    where = "standalone",
    binding = O.window(standaloneWindow(v.name), { slot = STANDALONE_SLOT, menus = true }),
    frame = D.constantFrame(ox, oy),
  }, { __index = common })
end

return {
  SLOT = SLOT,
  STANDALONE_SLOT = STANDALONE_SLOT,
  HOSTS = HOSTS,
  all = CELLS,
}
