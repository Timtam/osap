-- VPS Avenger's page: the avenger_control project's panel, as an HTML page of this module's own
-- (ui/, host.gui.webview) beside Avenger — the maintainer's decisions of 2026-10-12 — on the
-- overlay runtime's page support (O:addPage, docs/api/overlay.md), which holds the header
-- overlay's place while the page is in front and makes every action in Avenger a trip:
--
--   the key       the tool's chord, ⌃⇧⌘F1 on a Mac (Ctrl+Shift+Alt+F1 on Windows), or the
--                 `pageKey` setting: pressed on an identified Avenger it reads Avenger off one
--                 picture (read/open.luau) and shows the page beside Avenger, with Open.wav;
--                 pressed in the page the runtime brings Avenger's window forward first and hides
--                 the page after, with Close.wav; a press whose reads show no page says why,
--                 unless the user moved on
--   the gate      anywhere else the key says why it opens nothing, as the tool's sentence does
--   the state     only what was read (state.luau): the preset's name, the zoom, the browser, the
--                 oscillator strip and the selected oscillator's knobs, the central tab and DRUM
--                 SQ, the macros; what the preset database gives is marked so; everything else
--                 "not read"
--   the actions   the header's are trips into Avenger (actions.luau); Initialize's No clicks
--                 nothing, as it is meant to; every other action the page sends is answered
--                 "<label> is not available in this module yet" in its error region, until the
--                 stage that builds it (the plan's slices 5 to 10)
--
-- A message counts only while the page has the keyboard (host.window.foreground), so a press that
-- reaches the module after the user went elsewhere does nothing; and one trip at a time, which the
-- runtime keeps (a second message while one is out is dropped, and logged). The page's own word
-- for its keyboard — the `focused` it sends — is logged where the two disagree, which on a Mac
-- with the page described `activates = false` is the question (TODO.md, MW1b). The page is
-- described once, and again only when the `pageLeavesDawActive` setting changes, since that is
-- fixed per page object (O:pageAgain); the runtime builds it the first time Avenger is identified,
-- not on the key.

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

-- The actions this module has a handler for: the header's trips (actions.luau), and
-- `cancel_initialize_preset`, Initialize's No in the page's own dialog, which clicks nothing in
-- Avenger (the maintainer's decision: the tool clicked MENU, Initialize and NO, three clicks that
-- change nothing): the page closes its dialog, and nothing is left to do here but say so in the
-- log.
local A = host.include("src/actions.luau")
P.HANDLERS = table.clone(A.HANDLERS)
P.HANDLERS.cancel_initialize_preset = function()
  host.log.info("[avenger] [page] Initialize Preset: No — the page's dialog closed; nothing clicked in Avenger")
end

-- The actions not built yet, by name: each is answered "<label> is not available in this module
-- yet". Explicit, so that a name the page sends and nobody listed is a test's failure rather than
-- an answer nobody chose.
P.NOT_BUILT = {
  "start_rename", "set_expansion_view",
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

local ready = false -- this page object's document has loaded
local plugin = nil -- the plug-in window the page was last shown beside, as a window table
local shownOnce = false -- the first show lands on the preset's name, later ones where the user was
local key = nil -- the page key in effect
local keyAt = nil -- when the key whose reads the next show answers was pressed (host.now)
local spec = nil -- what O:addPage was given: its `notHere` follows the key (P.start)
-- When the reads were made whose state this page object was last posted by a press: its document
-- has them, or gets them when it loads (posts wait for the load), so `ready` answers them without
-- their age. Forgotten when the page is described again, fails, or reloads itself.
local given = nil

local function post(value)
  local page = E.header.page
  if page then page:post(value) end
end

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
-- which DAWs there are is daw-hosts' to say. The runtime says it (`notHere`), and this module where
-- its own look finds no window in front.
local function notHereWords()
  return "It seems that the VPS Avenger plug-in window is not at the front. Please open the plug-in window, "
    .. "then press " .. keyWords() .. " again."
end

-- What the page says of a plug-in window that is gone: the tool's sentence.
local function closedWords()
  return "The plug-in window was closed. Please open it again, then press " .. keyWords() .. " again."
end

-- The database's list of expansions for the `roster` setting, or nil.
local function roster()
  local names, why = E.DB.roster(host.settings.get("roster"))
  if not names then
    host.log.info("[avenger] [page] no list of expansions from the preset database: " .. tostring(why))
  end
  return names
end

-- The state of window `id` posted as a press or a trip read it: this page object has its reads.
local function postState(id)
  post({ kind = "state", state = S.snapshot(id, roster()) })
  given = S.at(id)
end

local function notShown(why)
  if why == "declined by the system" then
    return "The system did not bring the page to the front; if it does not come by itself, the key brings it."
  end
  return "The page did not come to the front: " .. tostring(why) .. "."
end

-- In the error region when the picture was cut at the screen's edge (read/open.luau).
local function cutWords(zoom)
  return (zoom and ("At " .. zoom .. " percent, part") or "Part")
    .. " of VPS Avenger lies past the edge of the screen: what lies there was not read."
end

-- The key on an identified Avenger — the runtime's `open`, in the key's handler, where the reads
-- wait: Avenger read off one picture (read/open.luau), taken while Avenger is unobscured; then,
-- once the overlay is still where the key was pressed, the state, the error region and the focus.
-- The runtime shows the page after. `false` where no page comes of it, with why said — except when
-- the user moved on, and is somewhere else by then.
local function open(o)
  local t0 = host.now()
  local win = host.window.active()
  local origin = o:origin()
  if not (win and origin) then
    host.log.info("[avenger] [page] the key, with no window in front to read: nothing opened")
    say(notHereWords())
    return false
  end
  local mark = o:here()
  local ok, values, why, words = pcall(Reads.read, o, { zoom = E.zoomOf(origin), mark = mark, roster = roster() })
  if not ok then
    host.log.info("[avenger] [page] not shown: the reads could not be made — " .. tostring(values))
    say(Reads.FAILED)
    return false
  end
  if not values then
    host.log.info("[avenger] [page] not shown: " .. tostring(why))
    if words then say(words) end
    return false
  end
  local fine, gone = o:stillHere(mark)
  if not fine then
    host.log.info("[avenger] [page] not shown: the user moved on while Avenger was read — " .. tostring(gone))
    return false
  end
  S.read(win.id, values)
  host.log.info(string.format("[avenger] [page] read for the page: the preset %s, the zoom %s",
    values.preset and ("'" .. values.preset .. "'") or "not read", values.zoom and (values.zoom .. " %") or "not read"))
  plugin, keyAt = win, t0
  postState(win.id)
  post({ kind = "error", text = values.cut and cutWords(values.zoom) or "" })
  if shownOnce then
    post({ kind = "restoreFocus" })
  else
    post({ kind = "focus", id = "preset-name" })
  end
  return nil
end

-- Every show the runtime made at the key: Open.wav and the time from the key; or why not, said.
local function onShown(_, ok, why)
  if ok then
    shownOnce = true
    sound("Open")
    host.log.info(string.format("[avenger] [page] shown beside '%s', %d ms after the key%s",
      tostring(plugin and plugin.title), host.now() - (keyAt or host.now()), why and (" (" .. why .. ")") or ""))
    return
  end
  host.log.info("[avenger] [page] not shown: " .. tostring(why))
  say(notShown(why))
end

-- The way back (the key in the page, the runtime's): Avenger's window asked forward first, the page
-- hidden after — and kept, with the reason in its error region, when the window would not come.
-- The runtime asks for the window even when the host no longer lists it: a DAW may hide its
-- plug-in windows while another application is active (docs/api/gui.md, "DAWs" — Logic's floating
-- ones are reported to), and the focus brings the DAW forward, which shows them again. Only a
-- window that refused and is not listed is taken as closed.
local function onBack(_, ok, why)
  if ok then
    -- Whether the DAW hid it while the page was in front is a Mac session's question (TODO.md, MW6).
    if plugin and not current(plugin) then
      host.log.info(string.format("[avenger] [page] '%s' (id=%s) is not listed while the page is in front; "
        .. "its focus was accepted", tostring(plugin.title), tostring(plugin.id)))
    end
    sound("Close")
    return
  end
  if why == "gone" or not current(plugin) then
    post({ kind = "error", text = closedWords() })
    return
  end
  post({ kind = "error", text = "VPS Avenger's window would not come to the front." })
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

local function onMessage(o, m)
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
  local page = E.header.page
  local front = page ~= nil and inFront(page.window)
  noteFocus(m.focused, front)
  if not front then
    host.log.info("[avenger] [page] '" .. action .. "' not acted on: the page does not have the keyboard")
    return
  end
  local handler = P.HANDLERS[action]
  if handler then
    handler(o, type(m.args) == "table" and m.args or {})
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
end

local function onClose(_, why)
  if why == "failed" then
    ready, given = false, nil
    host.log.info("[avenger] [page] the page failed; it is built again the next time it is needed")
    return
  end
  sound("Close")
end

-- The page described, for the runtime (O:addPage, O:pageAgain): its three events are the
-- runtime's, which runs this module's hooks after its own part.
local function describe(on)
  local leaves = host.settings.get("pageLeavesDawActive") == true
  ready, shownOnce, given = false, false, nil
  return host.gui.webview({
    file = "ui/index.html",
    title = "Avenger Control",
    activates = not leaves,
    onMessage = on.message,
    onReady = on.ready,
    onClose = on.close,
  })
end

-- Why `s` cannot be the page key on this platform (host.keys.check), or nil when it can.
local function refusal(s)
  local ok, c = pcall(host.keys.check, s)
  if not ok then return tostring(c) end
  if not c.ok then return table.concat(c.reasons or {}, ", ") end
  return nil
end

-- ── What main.luau calls ─────────────────────────────────────────────────────

-- At load, before the header overlay is bound (O:addPage asks it then): the settings, the key and
-- the page. `env`: { header = the header overlay, zoomOf = (origin) -> the zoom read or nil, DB =
-- presets.luau, ring = main.luau's own clicks, reads and choices, for the trips (actions.luau) }.
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

  key = stored
  local why = refusal(stored)
  if why then
    host.log.info(string.format("[avenger] [page] the page key '%s' cannot be used (%s): %s instead",
      tostring(stored), tostring(why), KEY_DEFAULT))
    key = KEY_DEFAULT
    if stored ~= KEY_DEFAULT then host.settings.set("pageKey", KEY_DEFAULT) end
  end
  spec = {
    describe = describe,
    key = key,
    notHere = notHereWords(),
    open = open,
    onMessage = onMessage,
    onReady = onReady,
    onClose = onClose,
    onShown = onShown,
    onBack = onBack,
  }
  env.header:addPage(spec)

  host.settings.onChange("pageKey", function(new)
    if new == key then return end
    local because = refusal(new)
    if not because then
      local fine, whyNot = env.header:pageKey(new)
      if not fine then because = tostring(whyNot) end
    end
    if not because then
      key = new
      -- What the key says elsewhere names the key: the runtime reads `notHere` at every press.
      spec.notHere = notHereWords()
      host.log.info("[avenger] [page] the page key is now " .. tostring(new))
      say("Page key changed to " .. keyWords() .. ".")
      return
    end
    -- Refused: the key in effect stays, and the setting says it again.
    host.log.info(string.format("[avenger] [page] '%s' cannot be the page key (%s); it stays %s",
      tostring(new), because, tostring(key)))
    say(string.format("%s cannot be the page key; it stays %s.", tostring(new), keyWords()))
    if key then host.settings.set("pageKey", key) end
  end)
  host.settings.onChange("pageLeavesDawActive", function(new, old)
    if new == old then return end
    -- Fixed for the life of a page object: the page is taken down and described again.
    env.header:pageAgain()
    host.log.info("[avenger] [page] the page is described again: activates = " .. tostring(not new))
  end)
  host.settings.onChange("roster", function(new, old)
    if new ~= old and ready then answerReady() end
  end)

  A.start({
    header = env.header,
    DB = env.DB,
    ring = env.ring,
    zoomOf = env.zoomOf,
    LABELS = P.LABELS,
    post = post,
    roster = roster,
    plugin = function() return plugin end,
    postState = postState,
    closedWords = closedWords,
  })
  host.log.info("[avenger] [page] " .. keyWords() .. " shows the Avenger Control page beside an identified VPS Avenger")
end

return P
