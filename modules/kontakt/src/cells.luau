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
-- identity check alone would match KK too. And `not inOwnDialog`: one of Kontakt's own
-- dialogs is a plugin host by our REAPER criteria, and the Qt window that draws it passes
-- both the class test and the identity test — see detect.inOwnDialog for what that cost.
local function bareOfVersion(vIdx)
  return function(c)
    return not D.isKK(c)
      and not D.inOwnDialog()
      and D.variantIndex(c) == vIdx
      and not D.kkPresent()
  end
end

-- A Komplete Kontrol hosting a Kontakt of this version. The overlay anchors on the KK
-- CONTAINER (that is the control the host window exposes), and reaches the Kontakt content
-- through the dynamic frame below.
local function kkHostingVersion(vIdx)
  return function(c)
    if not D.isKK(c) or D.inOwnDialog() then return false end
    local kc = D.findNestedKontakt(c)
    return kc ~= nil and D.variantIndex(kc) == vIdx
  end
end

-- The Kontakt STANDALONE application window (Kontakt N.exe on its own): a top-level
-- NINormalWindow titled "Kontakt N". Mirrors ReaHotkey's Standalone.Register criteria.
-- Unlike a KK-nested Kontakt (a UIA leaf), a standalone Kontakt is fully UIA-accessible.
--
-- On macOS, for Kontakt 7 only, and from a measurement rather than by analogy. The third Mac
-- session's probe recorded the window whole: title exactly "Kontakt 7", role AXWindow with
-- subrole AXDialog (not AXStandardWindow, which is why a matcher copied from any other
-- application's window would have missed it), executable "Kontakt 7". The title is matched
-- EXACTLY there, because on this platform every dialog Kontakt opens is a window of the same
-- process, and a prefix would let one titled "Kontakt 7 …" claim the header. The subrole is
-- left out on purpose: one measurement of it is not enough to rule the other one out, and the
-- exact title already says which window this is.
--
-- Kontakt 8 gets the same block, by analogy and not by measurement — no Kontakt 8 has been
-- seen on a Mac. What makes the analogy cheap to be wrong about: the title is matched exactly,
-- so nothing but a window called "Kontakt 8" is claimed, and everything its standalone header
-- carries acts by NAME ("Kontakt File Menu", then Kontakt's own focus ring), so a Kontakt 8
-- that publishes different names refuses the press out loud rather than clicking elsewhere.
-- The next session's student Mac is new enough that Kontakt 8 is the likelier of the two, and
-- without this block it would have been a session with no overlay on it at all.
local function standaloneWindow(name)
  return {
    title = { pattern = "^" .. name },
    app = { exe = { contains = "Kontakt" } },
    windows = { class = { prefix = "NINormalWindow" } },
    macos = { title = { exact = name }, axRole = "AXWindow" },
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
  -- BOTH versions keep the instrument / multi / snapshot controls in one view only, and
  -- each answers "which view am I in?" differently — so the question belongs to the cell,
  -- which knows its version from the moment the window opened.
  --
  --   Kontakt 8  modern ↔ classic, read from a pixel of its own top bar. It also offers a
  --              toggle: F10 on Windows, the only way back from the modern view without
  --              reading a menu that carries no accessibility information; on a Mac, where
  --              F10 is not Kontakt's key, the entry in that menu, read by OCR (header.luau).
  --   Kontakt 7  single ↔ rack, read from UIA (detect.isRackView). No toggle: F10 is a
  --              Kontakt 8 command, and offering it here pressed a key that does nothing.
  --
  -- Sharing Kontakt 8's pixel probe across both is what hid every one of Kontakt 7's
  -- header controls: it measures a bar Kontakt 7 does not have.
  local isK8 = v.name == "Kontakt 8"
  local common = {
    version = v.name,
    geometry = G,
    header = geo.HEADER[v.name],
    uiaFileMenu = geo.UIA_FILE_MENU[v.name],
    hasViewToggle = isK8,
    -- `o` is passed through so a probe that could not read can ask the overlay to look again.
    -- Without it a refused reading waits for the next question, and after activation there may
    -- not be one: the arbiter picks an overlay from the wrong answer and then stops asking.
    inClassic = isK8 and function(o) return D.isClassicView(o:origin(), o) == true end
      or function() return D.isRackView() end,
    inModern = isK8 and function(o) return D.isClassicView(o:origin(), o) ~= true end
      or function() return false end,
    -- `inClassic` covers BOTH the instrument rack and the instrument editor, because the
    -- status bar and the resize grip are the same in each. The rack's own controls are not:
    -- their coordinates address a rack that the editor has replaced, so they take `inRack`.
    inEdit = isK8 and function(o)
        return D.isClassicView(o:origin(), o) == true and D.isEditView(o:origin(), o) == true
      end or function() return false end,
    inRack = isK8 and function(o)
        return D.isClassicView(o:origin(), o) == true and D.isEditView(o:origin(), o) ~= true
      end or function() return D.isRackView() end,
  }

  CELLS[#CELLS + 1] = setmetatable({
    key = v.name .. " in a DAW",
    where = "in-daw",
    binding = O.embedded({
      hosts = HOSTS,
      -- On Windows the plugin is a Qt control of the host window. On macOS it is nothing
      -- the host publishes — see detect.macosPanel — so the panel is found from Kontakt's own
      -- file-menu button, at the offset the geometry table already carries. Measured for
      -- Kontakt 7 (FILE, to 2 px). Kontakt 8's is the KONTAKT wordmark, published on Windows
      -- only by a STANDALONE Kontakt 8 as "Kontakt File Menu" — embedded there it is a leaf —
      -- so whether a Mac's Kontakt 8 publishes it inside REAPER is exactly what the next
      -- session finds out: if not, the log says "publishes no button named Kontakt File Menu"
      -- once and the cell stays inert, as before. If it does, the corner is the wordmark's
      -- centre minus {86,19}, which was authored as a click point rather than measured as a
      -- centre, and the anchor line in the log prints the corner that came out.
      control = {
        windows = D.QT_PLUGIN,
        linux = D.QT_PLUGIN,
        macos = D.macosPanel(v.name, geo.UIA_FILE_MENU[v.name], G.fileMenu),
      },
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

  local standalone = {
    key = v.name .. " standalone",
    where = "standalone",
    binding = O.window(standaloneWindow(v.name), { slot = STANDALONE_SLOT, menus = true }),
    frame = D.constantFrame(ox, oy),
  }
  -- On macOS the rack question has no honest answer, so the controls that depend on it are
  -- not offered. Kontakt 7's `isRackView` asks whether a VIEW button exists inside the plugin,
  -- and the probe of a standalone Kontakt 7 on a Mac shows VIEW published while the whole
  -- window is given over to the preset browser — so it answers "rack" always, and the six
  -- rack controls (both instrument arrows, the snapshot dropdown and its arrows, the
  -- instrument editor) would be offered with right-anchored coordinates that land in the
  -- browser's search field. ReaHotkey tells the two apart by SHOP's previous sibling, which
  -- the raw walker cannot step to. Own keys, so they shadow `common` for this cell only; the
  -- header's file menu, LIBRARY, VIEW and SHOP act by name and need no such question.
  if host.os.is("macos") then
    standalone.inRack = function() return false end
    standalone.inClassic = function() return false end
    standalone.inEdit = function() return false end
    -- And Kontakt 8's view toggle, whose `when` is this. The classic/modern question is one
    -- pixel measured on Windows, and on a Mac it answers "modern" whenever that pixel is not the
    -- classic grey — so "Switch to classic view" would be offered, and a press would flip a view
    -- nobody can hear, on a window no Kontakt 8 has been measured on.
    standalone.inModern = function() return false end
  end
  CELLS[#CELLS + 1] = setmetatable(standalone, { __index = common })
end

return {
  SLOT = SLOT,
  STANDALONE_SLOT = STANDALONE_SLOT,
  HOSTS = HOSTS,
  all = CELLS,
}
