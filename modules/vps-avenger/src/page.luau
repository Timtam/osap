-- VPS Avenger's page: the avenger_control project's panel, as an HTML page of this module's own
-- (ui/, host.gui.webview) beside Avenger — the maintainer's decisions of 2026-10-12. This is its
-- read-only stage, which clicks nothing:
--
--   the key       the tool's chord, ⌃⇧⌘F1 on a Mac (Ctrl+Shift+Alt+F1 on Windows), or the
--                 `pageKey` setting: pressed on an identified Avenger it reads Avenger off one
--                 picture (read/open.luau) and shows the page beside Avenger, with Open.wav;
--                 pressed in the page it brings Avenger's window forward first and hides the page
--                 after, with Close.wav; pressed while the last press's reads are still out, it
--                 does nothing; a press whose reads show no page says why, unless the user moved on
--   the gate      anywhere else the key says why it opens nothing, as the tool's sentence does
--   the state     only what was read (state.luau): the preset's name, the zoom, the browser, the
--                 oscillator strip and the selected oscillator's knobs, the central tab and DRUM
--                 SQ, the macros; what the preset database gives is marked so; everything else
--                 "not read"
--   the actions   every action the page sends is answered "<label> is not available in this
--                 module yet" in its error region, until the stage that builds it (the plan's
--                 slices 3 to 10); Initialize's No clicks nothing, as it is meant to
--
-- A message counts only while the page has the keyboard (host.window.foreground), so a press that
-- reaches the module after the user went elsewhere does nothing. The page's own word for it — the
-- `focused` it sends — is logged where the two disagree, which on a Mac with the page described
-- `activates = false` is the question (TODO.md, MW1b). The page is described once, and again only
-- when the `pageLeavesDawActive` setting changes, since that is fixed per page object; it is built
-- the first time Avenger is identified, not on the key.

local P = {}

-- ── Settings ─────────────────────────────────────────────────────────────────
-- Five, in the module's settings dialog (the plan's §2.9). Only the first four act in this stage;
-- the Drum SQ watcher that `drumSqCadence` paces comes later.
local KEY_DEFAULT = host.os.pick({
  macos = "Ctrl+Shift+Win+F1", -- Command is Ctrl and Control is Win in a key spec on a Mac
  windows = "Ctrl+Shift+Alt+F1",
  linux = "Ctrl+Shift+Alt+F1",
})
-- The tool's rosters of expansions, its default first ("Factory Standard": the ones every owner of
-- Avenger has, its preset_db.py) — the two that name no person.
P.ROSTERS = { "Factory Standard", "Complete" }

-- ── The actions ──────────────────────────────────────────────────────────────
-- Every action the page sends (ui/panel.js, ACTIONS), by the tool's names, with the words its
-- answer uses. A test holds these against the page's list.
P.LABELS = {
  prev_preset = "Previous preset",
  next_preset = "Next preset",
  start_rename = "Renaming the preset",
  pick_load_preset = "Load Preset",
  pick_save_preset_as = "Save Preset as",
  pick_initialize_preset = "Initialize Preset",
  cancel_initialize_preset = "No",
  undo = "Undo",
  redo_menu = "Redo",
  set_zoom = "Size",
  set_expansion_view = "Expansion View",
  open_expansion = "Opening an expansion",
  close_expansion = "Close Expansion",
  browse_expansion_presets = "Browse Presets",
  select_all_expansion_presets = "All presets",
  select_expansion_category = "Choosing a category",
  go_to_loaded_preset_category = "Go to Category",
  select_expansion_preset = "Selecting a preset",
  load_expansion_preset = "Loading a preset",
  mark_expansion_preset_favorite = "Mark as favorite",
  toggle_expansion_favorite_view = "The favorites view",
  select_osc_tab = "Choosing an oscillator tab",
  add_osc = "Add OSC",
  select_drums_tab = "The Drums tab",
  select_trig_tab = "The Trig tab",
  toggle_osc_mute = "An oscillator's Mute",
  toggle_drums_mute = "The Drums' Mute",
  toggle_trig_mute = "The Trig's Mute",
  toggle_osc_solo = "An oscillator's Solo",
  toggle_drums_solo = "The Drums' Solo",
  toggle_trig_solo = "The Trig's Solo",
  osc_menu_item = "An oscillator's menu",
  drums_menu_item = "The Drums' menu",
  trig_menu_item = "The Trig's menu",
  rename_osc = "Renaming an oscillator",
  select_macro_tab = "Choosing a macro tab",
  add_macro_tab = "Add Macro Tab",
  delete_macro_tab = "Delete Last Tab",
  rename_macro = "Renaming a macro",
  macro_amount = "A macro's amount",
  reset_macro_amount = "Resetting a macro's amount",
  toggle_macro_button = "A macro button",
  macro_reset_mode = "A macro's reset mode",
  macro_button_menu_item = "A macro button's menu",
  osc_control = "An oscillator's control",
  reset_osc_control = "Resetting an oscillator's control",
  osc_filter_control = "The 2-band filter",
  toggle_drum_pattern = "The drum pattern",
  select_drum_mode = "The drum mode",
  previous_drumkit = "The previous drumkit",
  next_drumkit = "The next drumkit",
  select_drumkit_part = "Choosing a drum slot",
  drumkit_column = "A drum slot's Browse, Back, Next or Edit Sample",
  toggle_drumkit_button = "A drum slot's Mute, Solo, Lock or Loop",
  drumkit_control = "A drum slot's Volume, Pan or Pitch",
  reset_drumkit_control = "Resetting a drum slot's control",
  drumkit_menu_item = "A drum slot's menu",
  select_central_tab = "Choosing a central tab",
  sample_browser_focus = "The sample browser",
  select_sample_editor_tab = "Choosing a sample editor tab",
  sample_editor_field = "A sample editor field",
  sample_editor_play_mode = "Play Mode",
  sample_editor_panel_button = "An envelope's Menu, Preset, Back or Next",
  toggle_sample_editor_enable = "Turning the envelope or the EQ on or off",
  sample_editor_grid = "Grid",
  sample_editor_eq_type = "An EQ band's type",
  sample_editor_loop_stretch = "Loop-Stretch",
  sample_editor_control = "A sample control",
  reset_sample_editor_control = "Resetting a sample control",
  central_drum_sq_sequence_menu_item = "The DRUM SQ sequence menu",
  central_drum_sq_pattern_player = "The pattern chain",
  central_drum_sq_auto_follow = "Auto follow",
  select_central_drum_sq_pattern = "Choosing a DRUM SQ pattern",
  toggle_central_drum_sq_pattern = "Turning a DRUM SQ pattern on or off",
  central_drum_sq_pattern_menu_item = "The DRUM SQ pattern menu",
  central_drum_sq_control = "VEL AMOUNT or SHUFFLE",
  reset_central_drum_sq_control = "Resetting VEL AMOUNT or SHUFFLE",
  central_drum_sq_length = "The DRUM SQ length",
  central_drum_sq_speed = "The DRUM SQ speed",
}

-- The actions this module has a handler for. `cancel_initialize_preset` is Initialize's No in the
-- page's own dialog, which clicks nothing in Avenger (the maintainer's decision: the tool clicked
-- MENU, Initialize and NO, three clicks that change nothing): the page closes its dialog, and
-- nothing is left to do here but say so in the log.
P.HANDLERS = {
  cancel_initialize_preset = function()
    host.log.info("[avenger] [page] Initialize Preset: No — the page's dialog closed; nothing clicked in Avenger")
  end,
}

-- The actions not built yet, by name: each is answered "<label> is not available in this module
-- yet". Explicit, so that a name the page sends and nobody listed is a test's failure rather than
-- an answer nobody chose.
P.NOT_BUILT = {
  "prev_preset", "next_preset", "start_rename", "pick_load_preset", "pick_save_preset_as",
  "pick_initialize_preset", "undo", "redo_menu", "set_zoom", "set_expansion_view",
  "open_expansion", "close_expansion", "browse_expansion_presets", "select_all_expansion_presets",
  "select_expansion_category", "go_to_loaded_preset_category", "select_expansion_preset",
  "load_expansion_preset", "mark_expansion_preset_favorite", "toggle_expansion_favorite_view",
  "select_osc_tab", "add_osc", "select_drums_tab", "select_trig_tab", "toggle_osc_mute",
  "toggle_drums_mute", "toggle_trig_mute", "toggle_osc_solo", "toggle_drums_solo",
  "toggle_trig_solo", "osc_menu_item", "drums_menu_item", "trig_menu_item", "rename_osc",
  "select_macro_tab", "add_macro_tab", "delete_macro_tab", "rename_macro", "macro_amount",
  "reset_macro_amount", "toggle_macro_button", "macro_reset_mode", "macro_button_menu_item",
  "osc_control", "reset_osc_control", "osc_filter_control", "toggle_drum_pattern",
  "select_drum_mode", "previous_drumkit", "next_drumkit", "select_drumkit_part", "drumkit_column",
  "toggle_drumkit_button", "drumkit_control", "reset_drumkit_control", "drumkit_menu_item",
  "select_central_tab", "sample_browser_focus", "select_sample_editor_tab", "sample_editor_field",
  "sample_editor_play_mode", "sample_editor_panel_button", "toggle_sample_editor_enable",
  "sample_editor_grid", "sample_editor_eq_type", "sample_editor_loop_stretch",
  "sample_editor_control", "reset_sample_editor_control", "central_drum_sq_sequence_menu_item",
  "central_drum_sq_pattern_player", "central_drum_sq_auto_follow",
  "select_central_drum_sq_pattern", "toggle_central_drum_sq_pattern",
  "central_drum_sq_pattern_menu_item", "central_drum_sq_control", "reset_central_drum_sq_control",
  "central_drum_sq_length", "central_drum_sq_speed",
}

-- ── The page ─────────────────────────────────────────────────────────────────
local E = nil -- what main.luau hands over (P.start)
local S = host.include("src/state.luau")
local Reads = host.include("src/read/open.luau")

local page = nil
local ready = false -- built, and its document has loaded
local prepareFailed = false -- a prepare answered a failure: from then on only the key builds it
local wanted = nil -- a show answered "not ready yet": { win, t0 }, shown once the page has loaded
local plugin = nil -- the plug-in window the page was last shown beside, as a window table
local shownOnce = false -- the first show lands on the preset's name, later ones where the user was
local key, keyId = nil, nil -- the page key in effect, and its registration
-- When the reads were made whose state this page object was last posted by a press: its document
-- has them, or gets them when it loads (posts wait for the load), so `ready` answers them without
-- their age. Forgotten when the page is described again, fails, or reloads itself.
local given = nil

local function inFront(id)
  local f = host.window.foreground()
  return id ~= nil and f ~= nil and f.id == id
end

-- Window `w` as it is now — it may have moved — or nil when it is gone.
local function current(w)
  if not (w and w.app) then return nil end
  for _, x in ipairs(host.window.list({ pids = { w.app.pid } })) do
    if x.id == w.id then return x end
  end
  return nil
end

local function sound(name)
  if host.settings.get("sounds") then host.sound.play("sounds/" .. name .. ".wav") end
end

local function keyWords()
  local ok, words = pcall(host.keys.describe, key)
  return ok and words or tostring(key)
end

local function say(words)
  host.speech.output(words, { interrupt = true })
end

-- The tool's sentence for a key pressed where no Avenger is in front, without the DAWs it named:
-- which DAWs there are is daw-hosts' to say.
local function notInFront()
  say("It seems that the VPS Avenger plug-in window is not at the front. Please open the plug-in window, "
    .. "then press " .. keyWords() .. " again.")
end

local function post(value)
  if page then page:post(value) end
end

-- The database's list of expansions for the `roster` setting, or nil.
local function roster()
  local names, why = E.DB.roster(host.settings.get("roster"))
  if not names then
    host.log.info("[avenger] [page] no list of expansions from the preset database: " .. tostring(why))
  end
  return names
end

local function notShown(why)
  if why == "declined by the system" then
    return "The system did not bring the page to the front; if it does not come by itself, the key brings it."
  end
  return "The page did not come to the front: " .. tostring(why) .. "."
end

-- Shows the page beside `win`, the key pressed at `t0`: what the page is told comes first.
local function show(win, t0)
  local ok, why = page:show({ focus = true, avoid = win.bounds })
  if ok then
    wanted = nil
    shownOnce = true
    sound("Open")
    host.log.info(string.format("[avenger] [page] shown beside '%s', %d ms after the key%s", tostring(win.title),
      host.now() - t0, why and (" (" .. why .. ")") or ""))
  elseif why == "not ready yet" then
    wanted = { win = win, t0 = t0 }
    say("Building the page.")
  else
    wanted = nil
    host.log.info("[avenger] [page] not shown: " .. tostring(why))
    say(notShown(why))
  end
end

-- In the error region when the picture was cut at the screen's edge (read/open.luau).
local function cutWords(zoom)
  return (zoom and ("At " .. zoom .. " percent, part") or "Part")
    .. " of VPS Avenger lies past the edge of the screen: what lies there was not read."
end

-- The key on an identified Avenger: Avenger read off one picture (read/open.luau) — the picture
-- taken in the key's handler while Avenger is unobscured, its questions answered on later turns —
-- then, once the reads are in and the overlay is still where the key was pressed, the state, the
-- focus, and the page. The log has the time from the key to the show. Where no page comes of it,
-- the user hears why — except when they moved on, and are somewhere else by then.
local function open(o)
  local t0 = host.now()
  local win = host.window.active()
  local origin = o:origin()
  if not (win and origin) then
    host.log.info("[avenger] [page] the key, with no window in front to read: nothing opened")
    notInFront()
    return
  end
  local mark = o:here()
  local zoom = E.zoomOf(origin)
  local ok, err = pcall(Reads.read, o, { zoom = zoom, mark = mark, roster = roster() }, function(values, why, words)
    if not values then
      host.log.info("[avenger] [page] not shown: " .. tostring(why))
      if words then say(words) end
      return
    end
    local fine, gone = o:stillHere(mark)
    if not fine then
      host.log.info("[avenger] [page] not shown: the user moved on while Avenger was read — " .. tostring(gone))
      return
    end
    S.read(win.id, values)
    host.log.info(string.format("[avenger] [page] read for the page: the preset %s, the zoom %s",
      values.preset and ("'" .. values.preset .. "'") or "not read", values.zoom and (values.zoom .. " %") or "not read"))
    plugin = win
    post({ kind = "state", state = S.snapshot(win.id, roster()) })
    given = S.at(win.id)
    post({ kind = "error", text = values.cut and cutWords(values.zoom) or "" })
    if shownOnce then
      post({ kind = "restoreFocus" })
    else
      post({ kind = "focus", id = "preset-name" })
    end
    show(win, t0)
  end)
  if not ok then
    host.log.info("[avenger] [page] not shown: the reads could not be made — " .. tostring(err))
    say(Reads.FAILED)
  end
end

-- The key in the page: Avenger's window forward first, the page hidden after — and kept, with the
-- reason in its error region, when the window would not come. The window is asked for even when
-- the host no longer lists it: a DAW may hide its plug-in windows while another application is
-- active (docs/api/gui.md, "DAWs" — Logic's floating ones are reported to), and the focus brings
-- the DAW forward, which shows them again. Only a window that refused and is not listed is taken
-- as closed.
local function back()
  local id = plugin and plugin.id
  if id and host.window.focus(id) then
    -- Whether the DAW hid it while the page was in front is a Mac session's question (TODO.md, MW6).
    if not current(plugin) then
      host.log.info(string.format("[avenger] [page] '%s' (id=%s) is not listed while the page is in front; "
        .. "its focus was accepted", tostring(plugin.title), tostring(id)))
    end
    page:hide()
    sound("Close")
    return
  end
  if not current(plugin) then
    post({ kind = "error", text = "The plug-in window was closed. Please open it again, then press "
      .. keyWords() .. " again." })
    return
  end
  post({ kind = "error", text = "VPS Avenger's window would not come to the front." })
end

local function onKey()
  if page and inFront(page.window) then
    back()
    return
  end
  if Reads.busy() then
    host.log.info("[avenger] [page] the key again while the last press's reads are out: nothing done")
    return
  end
  local o = E.header
  if o.active and o:origin() then
    open(o)
    return
  end
  host.log.info("[avenger] [page] the key, with no VPS Avenger identified in front: nothing opened")
  notInFront()
end

-- The page loaded, or reloaded itself: its state, from the reads already made — with how long ago
-- they were made, unless this page object was posted them by the press that made them.
local function answerReady()
  local id = plugin and plugin.id
  local at = S.at(id)
  post({ kind = "state", state = S.snapshot(id, roster(), { age = at ~= nil and at ~= given }) })
end

-- What the page's `focused` says against the host, logged once per pair: whether a message from
-- the page counts can be answered both ways, and only a Mac's log says whether they agree there.
local focusPairs = {}
local function noteFocus(said, front)
  if type(said) ~= "boolean" or said == front then return end
  local pair = tostring(said) .. "/" .. tostring(front)
  if focusPairs[pair] then return end
  focusPairs[pair] = true
  host.log.info(string.format("[avenger] [page] the page says it %s the keyboard, and the host that its "
    .. "window %s in front", said and "has" or "has not", front and "is" or "is not"))
end

local function onMessage(m)
  if type(m) ~= "table" then return end
  if m.kind == "problem" then
    host.log.info("[avenger] [page] the page reports: " .. tostring(m.text))
    return
  end
  if m.action == "ready" then
    answerReady()
    return
  end
  local action = m.action
  if type(action) ~= "string" then return end
  local front = page ~= nil and inFront(page.window)
  noteFocus(m.focused, front)
  if not front then
    host.log.info("[avenger] [page] '" .. action .. "' not acted on: the page does not have the keyboard")
    return
  end
  local handler = P.HANDLERS[action]
  if handler then
    handler(type(m.args) == "table" and m.args or {})
    return
  end
  local label = P.LABELS[action]
  if not label then
    host.log.info("[avenger] [page] '" .. action .. "' is no action of the page's")
    label = action
  end
  post({ kind = "error", text = label .. " is not available in this module yet" })
end

local function onReady()
  -- Loaded before: the page reloaded itself, and its new document has nothing posted to the old
  -- one. Its own `ready` may have come before this and been answered without the reads' age, so
  -- the state goes again, with it.
  local reloaded = ready
  ready = true
  if reloaded then
    given = nil
    if S.at(plugin and plugin.id) then answerReady() end
  end
  local w = wanted
  if w and inFront(w.win.id) then
    show(current(w.win) or w.win, w.t0)
  else
    wanted = nil
  end
end

local function onClose(why)
  wanted = nil
  if why == "failed" then
    ready, given = false, nil
    host.log.info("[avenger] [page] the page failed; it is built again the next time it is needed")
    return
  end
  sound("Close")
end

local function describe()
  local leaves = host.settings.get("pageLeavesDawActive") == true
  page = host.gui.webview({
    file = "ui/index.html",
    title = "Avenger Control",
    activates = not leaves,
    onMessage = onMessage,
    onReady = onReady,
    onClose = onClose,
  })
  ready, prepareFailed, wanted, shownOnce, given = false, false, nil, false, nil
end

local function prepare()
  local ok, why = page:prepare()
  if not ok and why ~= "not ready yet" then
    prepareFailed = true
    host.log.info("[avenger] [page] the page was not prepared (" .. tostring(why) .. "); the key builds it from now on")
  end
end

-- The page key: `spec` registered in place of the one in effect — or nil and why not, for a spec
-- the platform cannot hold (host.keys.check) or the host refused.
local function claim(spec)
  local ok, c = pcall(host.keys.check, spec)
  if not ok then return nil, tostring(c) end
  if not c.ok then return nil, table.concat(c.reasons or {}, ", ") end
  local fine, id = pcall(host.hotkey.register, spec, onKey)
  if not fine then return nil, tostring(id) end
  if keyId then host.hotkey.unregister(keyId) end
  key, keyId = spec, id
  return true
end

-- ── What main.luau calls ─────────────────────────────────────────────────────

-- At load: the settings, the key and the page. `env`: { header = the header overlay, zoomOf =
-- (origin) -> the zoom read or nil, DB = presets.luau }.
function P.start(env)
  E = env
  host.settings.define("roster", P.ROSTERS[1], {
    label = "Expansions list (from the preset database)", oneOf = P.ROSTERS })
  local stored = host.settings.define("pageKey", KEY_DEFAULT, {
    label = "Key that shows the Avenger Control page and goes back to the plug-in" })
  host.settings.define("sounds", true, { label = "Sounds when the page opens and closes" })
  host.settings.define("pageLeavesDawActive", false, {
    label = "On a Mac, the page leaves the DAW the active application (experimental)" })
  host.settings.define("drumSqCadence", 250, {
    label = "Drum SQ auto follow: milliseconds between reads (100 to 1000)", min = 100, max = 1000 })

  local ok, why = claim(stored)
  if not ok then
    host.log.info(string.format("[avenger] [page] the page key '%s' cannot be used (%s): %s instead",
      tostring(stored), tostring(why), KEY_DEFAULT))
    local fine, because = claim(KEY_DEFAULT)
    if not fine then
      host.log.info("[avenger] [page] the default page key cannot be used either: " .. tostring(because))
    elseif stored ~= KEY_DEFAULT then
      host.settings.set("pageKey", KEY_DEFAULT)
    end
  end
  host.settings.onChange("pageKey", function(new)
    if new == key then return end
    local fine, because = claim(new)
    if fine then
      host.log.info("[avenger] [page] the page key is now " .. tostring(new))
      say("Page key changed to " .. keyWords() .. ".")
      return
    end
    -- Refused: the key in effect stays, and the setting says it again.
    host.log.info(string.format("[avenger] [page] '%s' cannot be the page key (%s); it stays %s",
      tostring(new), tostring(because), tostring(key)))
    say(string.format("%s cannot be the page key; it stays %s.", tostring(new), keyWords()))
    if key then host.settings.set("pageKey", key) end
  end)
  host.settings.onChange("pageLeavesDawActive", function(new, old)
    if new == old then return end
    -- Fixed for the life of a page object: the page is taken down and described again.
    page:close()
    describe()
    host.log.info("[avenger] [page] the page is described again: activates = " .. tostring(not new))
  end)
  host.settings.onChange("roster", function(new, old)
    if new ~= old and ready then answerReady() end
  end)

  describe()
  host.log.info("[avenger] [page] " .. keyWords() .. " shows the Avenger Control page beside an identified VPS Avenger")
end

-- The header overlay came to the front on an identified Avenger: the page is built now, if it is
-- not, so that the key shows it at once. In the arbiter's call, where nothing waits: a prepare
-- only asks for the build.
function P.arrived()
  if page and not ready and not prepareFailed then prepare() end
end

return P
