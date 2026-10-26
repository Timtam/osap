-- VPS Avenger's page: the avenger_control project's panel, as an HTML page of this module's own
-- (ui/, host.gui.webview) beside Avenger — the maintainer's decisions of 2026-10-12 — on the
-- overlay runtime's page support (O:addPage, docs/api/overlay.md), which holds the header
-- overlay's place while the page is in front and makes every action in Avenger a trip:
--
--   the key       the tool's chord, ⌃⇧⌘F1 on a Mac (Ctrl+Shift+Alt+F1 on Windows), or the
--                 `pageKey` setting: pressed on an identified Avenger it reads Avenger off one
--                 picture (read/open.luau), its questions asked at once with callbacks, and shows
--                 the page beside Avenger once they are in (the runtime's `openLater`), with Open.wav;
--                 pressed in the page the runtime brings Avenger's window forward first and hides
--                 the page after, with Close.wav; a press whose reads show no page says why,
--                 unless the user moved on
--   the gate      anywhere else the key says why it opens nothing, as the tool's sentence does
--   the state     only what was read (state.luau): the preset's name, the zoom, the browser, the
--                 oscillator strip and the selected oscillator's knobs, the central tab and DRUM
--                 SQ with its length, the drumkit table, the sample editor's tab, view and switch,
--                 the macros; what the preset database gives, shown as it is; everything else "not
--                 read"; ◀ and ▶ unavailable where the preset read is Init Preset; and PASTE of
--                 DRUM SQ's pattern menu offered once a COPY of this session was taken
--   the actions   the header's, the oscillator strip's, the macros', the Drums tab's, the sample
--                 editor's, DRUM SQ's and the expansion browser's are trips into Avenger
--                 (actions.luau, act.luau, actions/browser.luau), the renames, Browse Presets, a
--                 drum slot's Browse, a row of the sample browser, a field of the sample editor and
--                 its envelope's Menu and Preset trips that hand the keyboard to Avenger
--                 (handover.luau), and the knobs turned with a press held through a hand-over of the
--                 keys (amount.luau); Initialize's No clicks nothing, as it is meant to, and Go to
--                 Category is the database's, with no trip, and Correct open Expansion a trip with
--                 no click; a name the page sends that is none of these is answered "<label> is not
--                 available in this module yet"
--   the list      the expansions the page lists: the preset database's roster (state.luau,
--                 S.expansions); an expansion opened from the page is the one at its place in
--                 the list last posted, which is where it is clicked in Avenger's grid
--                 (src/actions/browser.luau, B.byPlace)
--   the watcher   while the page is hidden, DRUM SQ's selected pattern said as AUTO FOLLOW moves it
--                 (watch.luau), paced by the `drumSqCadence` setting
--   the settings  six (P.SETTINGS), which the module's settings dialog shows and the page's own
--                 Settings section — the tool's Windows-only block made general — writes with
--                 set_setting and reset_settings, no trip; whichever wrote, the settings are posted
--                 to the page after the change, and at every `ready`, so the two always agree
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
-- Six, in the module's settings dialog (the plan's §2.9): the page key, the expansions list, the
-- sounds, the Mac page mode, how often DRUM SQ's watcher takes its picture (watch.luau), and the
-- Expansion View VPS Avenger shows where it draws no label to read — set from the page, or
-- confirmed there (read/browser.luau, Br.view), as the tool kept its AVGExpansionView.
local KEY_DEFAULT = host.os.pick({
  macos = "Ctrl+Shift+Win+F1", -- Command is Ctrl and Control is Win in a key spec on a Mac
  windows = "Ctrl+Shift+Alt+F1",
  linux = "Ctrl+Shift+Alt+F1",
})
-- The tool's rosters of expansions, exactly as it offers them (its preset_db.py, ROSTER_KEY_OPTIONS,
-- each label without "Expansions "), its default first: "Factory Standard", the ones every owner of
-- Avenger has; "Complete", the tool's catalog; and the two personal rosters the tool offers, by the
-- names it gives them — the maintainer's decision of 2026-10-24, an exception to the rule that no
-- person is named, for exactly these two (DB.roster).
P.ROSTERS = { "Factory Standard", "Complete", "Nyhdroe", "Joyman" }
local X = host.include("src/tables/expansion.luau")

local function onOff(v) return v and "on" or "off" end
local function whole(v) return type(v) == "number" and v == math.floor(v) end

-- The six, in the dialog's order, from one table: `opts` what host.settings.define is given (the
-- dialog's label, its bounds or choices), `words` the page's name for it, which its answers begin
-- with, `say(v)` a value in words, and `check(v)` why `v` cannot be stored, or nil — the page's
-- messages are checked with it before anything is set, and a stored value at load: `define` hands
-- back a value of the right kind unchecked (docs/api/settings.md), and a settings.toml written by
-- hand can hold a list the module does not have or a pace of 5000. The page key's own checks are
-- its change's (P.start): the platform's, the tool's rule, and the registration itself.
P.SETTINGS = {
  { key = "roster", default = P.ROSTERS[1], words = "Expansions List",
    opts = { label = "Expansions list (from the preset database)", oneOf = P.ROSTERS },
    say = tostring,
    check = function(v)
      if v == nil then return "no expansions list was named" end
      if type(v) ~= "string" or not table.find(P.ROSTERS, v) then
        return tostring(v) .. " is not one of the expansions lists"
      end
      return nil
    end },
  { key = "pageKey", default = KEY_DEFAULT, words = "Page Key",
    opts = { label = "Key that shows the Avenger Control page and goes back to the plug-in" },
    check = function(v)
      if type(v) ~= "string" or v == "" then return "no key was written" end
      return nil
    end },
  { key = "sounds", default = true, words = "Sounds",
    opts = { label = "Sounds when the page opens and closes, and at a knob's limit" },
    say = onOff,
    check = function(v) return type(v) ~= "boolean" and "on or off was not sent" or nil end },
  { key = "pageLeavesDawActive", default = false, words = "Leave the DAW Active",
    opts = { label = "On a Mac, the page leaves the DAW the active application (experimental)" },
    say = onOff,
    check = function(v) return type(v) ~= "boolean" and "on or off was not sent" or nil end },
  { key = "drumSqCadence", default = 250, words = "DRUM SQ Auto Follow",
    opts = { label = "Drum SQ auto follow: milliseconds between reads (100 to 1000)", min = 100, max = 1000 },
    say = function(v) return tostring(v) .. " milliseconds" end,
    -- host.settings.set holds the bounds, not the whole number, which the watcher needs
    -- (watch.luau, cadenceOf).
    check = function(v)
      if v == nil then return "no number of milliseconds was sent" end
      if not (whole(v) and v >= 100 and v <= 1000) then
        return tostring(v) .. " is no whole number of milliseconds from 100 to 1000"
      end
      return nil
    end },
  { key = "expansionView", default = "8 per Row", words = "Expansion View",
    opts = { label = "Expansion View VPS Avenger shows (where it draws no label to read)", oneOf = X.VIEW_OPTIONS },
    say = tostring,
    check = function(v)
      if v == nil then return "no view was named" end
      if type(v) ~= "string" or not table.find(X.VIEW_OPTIONS, v) then return tostring(v) .. " is not one of VPS Avenger's views" end
      return nil
    end },
}
local byKey = {}
for _, s in ipairs(P.SETTINGS) do byKey[s.key] = s end
-- Reset's order: the Mac mode last, since it describes the page again (O:pageAgain) and the page the
-- reset came from is closed with it.
local RESET_ORDER = { "roster", "expansionView", "sounds", "drumSqCadence", "pageKey", "pageLeavesDawActive" }

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
  align_open_expansion = "Correct open Expansion",
  confirm_osc_count = "Confirm OSC Count",
  confirm_expansion_view = "Confirm Expansion View",
  set_setting = "Settings",
  reset_settings = "Reset Saved Settings",
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

-- The actions this module has a handler for: the trips (actions.luau), and
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
-- an answer nobody chose. Every action of the page's has a handler: the port is closed (the plan's
-- slice 10), and this stays, empty, for the next edit of the page.
P.NOT_BUILT = {}

-- ── The page ─────────────────────────────────────────────────────────────────
local E = nil -- what main.luau hands over (P.start)
local O = host.require("com.platform.overlay")
local S = host.include("src/state.luau")
local Reads = host.include("src/read/open.luau")
local Br = host.include("src/read/browser.luau")
local Watch = host.include("src/watch.luau")

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
-- This page object's document is there to be told the settings: it has loaded, or sent its own
-- `ready`. A page described again is told them by its own `ready`, never before it.
local listening = false
-- A change the page asked for (set_setting, reset_settings), while it is made: the settings'
-- callbacks run inside the module's own host.settings.set (docs/api/settings.md, "When it runs"),
-- and with this they leave what they would say to the handler — `said`, a sentence to speak;
-- `refused`, why the page key was refused; `rebuilt`, the Mac mode the page was described again in
-- — which answers once, after the set. nil for the dialog's changes: those speak and post at once.
local asking = nil

-- A message to the page. page:post writes it as host.json.encode does, and raises for what JSON
-- cannot hold; it answers false and an error value for a page it could not hand it to (gone with
-- the module's last load, or its engine refused it: docs/api/gui.md). Either is logged, and the
-- key's show or the trip's answer goes on without it rather than failing whole.
local function post(value)
  local page = E.header.page
  if not page then return end
  local ok, posted, err = pcall(function() return page:post(value) end)
  if not ok or posted == false then
    host.log.info(string.format("[avenger] [page] a '%s' message could not be posted: %s", tostring(value.kind),
      tostring(ok and err or posted)))
  end
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

-- Key spec `s` in this platform's words, or as written where it has none.
local function describeKey(s)
  local ok, words = pcall(host.keys.describe, s)
  return ok and words or tostring(s)
end

local function keyWords()
  return describeKey(key)
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

-- The list of expansions the page is posted: the roster (S.expansions) — and the names kept, as the
-- list last posted, which an index the page sends with open_expansion refers to.
local lastShown = nil
local function shownExpansions()
  local shown = S.expansions(roster())
  -- A roster's expansion the database has no data for stays in its list, as the tool keeps it, and
  -- is marked so (DB.roster).
  if shown then
    local have = {}
    for _, n in ipairs(E.DB.expansionNames() or {}) do have[n] = true end
    shown.noData = {}
    for i, n in ipairs(shown.names) do
      if not have[n] then shown.noData[i] = true end
    end
  end
  lastShown = shown and shown.names or nil
  return shown
end

-- The state of window `id` posted as a press or a trip read it: this page object has its reads.
local function postState(id)
  post({ kind = "state", state = S.snapshot(id, shownExpansions()) })
  given = S.at(id)
end

-- page:show's error value (docs/api/gui.md), in the page's words.
local function notShown(err)
  if err ~= nil and err.code == host.errors.REFUSED then
    return "The system did not bring the page to the front; if it does not come by itself, the key brings it."
  end
  return "The page did not come to the front: " .. tostring(err) .. "."
end

-- In the error region when the picture was cut at the screen's edge (read/open.luau): the parts
-- that leave it, and the way to bring them back (Open.offScreen) — or, where none of the page's
-- parts is named, that part of Avenger does.
local function cutWords(zoom, parts)
  if parts then
    return string.upper(string.sub(parts, 1, 1)) .. string.sub(parts, 2) .. ". What lies past the edge was not read."
  end
  return (zoom and ("At " .. zoom .. " percent, part") or "Part")
    .. " of VPS Avenger lies past the edge of the screen: what lies there was not read."
end

-- The key on an identified Avenger — the runtime's `open`, in the key's handler, answered later
-- (O:addPage's `openLater`, overlay runtime 0.14): Avenger read off one picture (read/open.luau),
-- taken while Avenger is unobscured, its questions and its first text read asked at once with
-- callbacks (the second Mac test's A19: the page key took about a second); then, once the overlay
-- is still where the key was pressed, the state, the error region and the focus, and `answer()` —
-- the runtime shows the page then. `answer(false)` where no page comes of it, with why said —
-- except when the user moved on, and is somewhere else by then. A press while another's read is out
-- reads afresh: the runtime shows the latest press's answer alone, and the first text read of the
-- earlier one, under the same key, is superseded (docs/api/ocr.md); nothing of a press is kept that
-- a disable, which drops what is out, could leave waiting.
local function open(o, answer)
  local t0 = host.now()
  local win = host.window.active()
  local origin = o:origin()
  if not (win and origin) then
    host.log.info("[avenger] [page] the key, with no window in front to read: nothing opened")
    say(notHereWords())
    return false
  end
  local mark = o:here()
  local before = S.values(win.id)
  local told = false
  local function finish(v)
    if told then return end
    told = true
    answer(v)
  end
  local ok, err = pcall(Reads.read, o, { zoom = E.zoomOf(origin), mark = mark,
    expected = before and before.selectedExpansion or nil,
    window = win.id, preset = E.ring.lastName() }, function(values, why, words)
    if not values then
      host.log.info("[avenger] [page] not shown: " .. tostring(why))
      if words then say(words) end
      return finish(false)
    end
    local fine, gone = o:stillHere(mark)
    if not fine then
      host.log.info("[avenger] [page] not shown: the user moved on while Avenger was read — " .. tostring(gone))
      return finish(false)
    end
    S.read(win.id, values)
    -- The latest name read, for the ring's ◀ and ▶ as well (main.luau, "◀ and ▶ on Init Preset").
    if values.preset then E.ring.noteName(values.preset) end
    host.log.info(string.format("[avenger] [page] read for the page: the preset %s, the zoom %s",
      values.preset and ("'" .. values.preset .. "'") or "not read", values.zoom and (values.zoom .. " %") or "not read"))
    plugin, keyAt = win, t0
    postState(win.id)
    post({ kind = "error", text = values.cut and cutWords(values.zoom, values.offScreen) or "" })
    if shownOnce then
      post({ kind = "restoreFocus" })
    else
      post({ kind = "focus", id = "preset-name" })
    end
    finish(nil)
  end)
  if not ok then
    host.log.info("[avenger] [page] not shown: the reads could not be made — " .. tostring(err))
    say(Reads.FAILED)
    finish(false)
  end
end

-- Every show the runtime made at the key: Open.wav and the time from the key — and, beside a
-- `true`, the error value of a page shown over the plug-in's window, in front all the same (a
-- partial answer, docs/api/gui.md) — or why not, said.
local function onShown(_, ok, err)
  if ok then
    shownOnce = true
    sound("Open")
    host.log.info(string.format("[avenger] [page] shown beside '%s', %d ms after the key%s",
      tostring(plugin and plugin.title), host.now() - (keyAt or host.now()), err and (" (" .. err .. ")") or ""))
    return
  end
  host.log.info("[avenger] [page] not shown: " .. tostring(err))
  say(notShown(err))
end

-- The way back (the key in the page, the runtime's): Avenger's window asked forward first, the page
-- hidden after — and kept, with the reason in its error region, when the window would not come.
-- The runtime asks for the window even when the host no longer lists it: a DAW may hide its
-- plug-in windows while another application is active (docs/api/gui.md, "DAWs" — Logic's floating
-- ones are reported to), and the focus brings the DAW forward, which shows them again. Only a
-- window that refused and is not listed is taken as closed.
local function onBack(_, ok, err)
  if ok then
    -- Whether the DAW hid it while the page was in front is a Mac session's question (TODO.md, MW6).
    if plugin and not current(plugin) then
      host.log.info(string.format("[avenger] [page] '%s' (id=%s) is not listed while the page is in front; "
        .. "its focus was accepted", tostring(plugin.title), tostring(plugin.id)))
    end
    sound("Close")
    return
  end
  if err.code == host.errors.GONE or not current(plugin) then
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
  post({ kind = "state", state = S.snapshot(id, shownExpansions(), { age = at ~= nil and at ~= given }) })
end

-- The settings as the page's Settings section shows them (ui/panel.js, applySettings): the six
-- values as stored, the expansions lists to choose from, and the key in effect and the default
-- key in this platform's words. About 200 bytes.
local function settingsMessage()
  local values = {}
  for _, s in ipairs(P.SETTINGS) do values[s.key] = host.settings.get(s.key) end
  return { kind = "settings", values = values, rosters = P.ROSTERS, keyWords = keyWords(),
    defaultKeyWords = describeKey(KEY_DEFAULT) }
end

-- The end of every one of the six settings' callbacks: the settings to the page, so that what the
-- dialog changed shows there — once its document is there to be told; and while the page's own
-- change is made, once, after it (`asking`).
local function postSettings()
  if asking then return end
  if listening then post(settingsMessage()) end
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
    listening = true
    post(settingsMessage())
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
  ready, listening = true, true
  if reloaded then
    given = nil
    if S.at(plugin and plugin.id) then answerReady() end
  end
end

local function onClose(_, how)
  if how == "failed" then
    ready, given, listening = false, nil, false
    host.log.info("[avenger] [page] the page failed; it is built again the next time it is needed")
    return
  end
  sound("Close")
end

-- The page described, for the runtime (O:addPage, O:pageAgain): its three events are the
-- runtime's, which runs this module's hooks after its own part.
local function describe(on)
  local leaves = host.settings.get("pageLeavesDawActive") == true
  ready, shownOnce, given, listening = false, false, nil, false
  return host.gui.webview({
    file = "ui/index.html",
    title = "Avenger Control",
    activates = not leaves,
    onMessage = on.message,
    onReady = on.ready,
    onClose = on.close,
  })
end

-- ── The page key ─────────────────────────────────────────────────────────────

-- Whether key spec `s` has no modifier: the whole of it is the key its last part names. The tool's
-- own rule (its Change Panel Shortcut took a key only with a modifier held): a bare F2 as a global
-- hotkey would take F2 from every application while the module runs. String work only
-- (host.keys.normalize), with no platform's spelling of a modifier in it.
local function bare(s)
  local last = string.match(s, "([^+]*)$")
  local ok, all, alone = pcall(function() return host.keys.normalize(s), host.keys.normalize(last) end)
  return ok and all ~= nil and all == alone
end

-- Why `s` cannot be the page key on this platform, or nil when it can: the reasons host.keys.check
-- gives where it says the key cannot work here; a modifier tap, a capture that is never a hotkey
-- (docs/api/keys.md); and a key with no modifier ("no modifier", the tool's rule). Codes, joined.
local function refusal(s)
  local ok, c = pcall(host.keys.check, s)
  if not ok then return tostring(c) end
  local reasons = c.reasons or {}
  if not c.ok then return table.concat(reasons, ", ") end
  if table.find(reasons, "tap") then return "tap" end
  if bare(s) then return "no modifier" end
  return nil
end

-- A page key's refusal — refusal's codes, joined — in the page's words.
local KEY_WHY = {
  parse = "it is not a key",
  reserved = "the system keeps it",
  no_keycode = "this system has no such key",
  voiceover = "Control with Option is the screen reader's",
  tap = "a modifier alone is no key",
  ["no modifier"] = "it needs at least one modifier key",
}

local function trim(s) return (string.gsub(s, "^%s*(.-)%s*$", "%1")) end

-- `because`: refusal's codes, or O:pageKey's error value (docs/api/overlay.md): `clash` for a
-- control's hotkey, any other code a refusal of the system's or another program's.
local function whyWords(because)
  if type(because) ~= "string" then
    if because.code == O.errors.CLASH then return "a control of the ring has it" end
    return "the system refused it: " .. because
  end
  local out = {}
  for part in string.gmatch(because, "[^,]+") do
    local w = KEY_WHY[trim(part)]
    if not w then return because end
    out[#out + 1] = w
  end
  return #out > 0 and table.concat(out, "; ") or because
end

-- ── The page's Settings section ──────────────────────────────────────────────
-- set_setting and reset_settings: no trip — nothing in Avenger is asked — and, as every message of
-- the page's, only while the page has the keyboard (onMessage). Each is answered once: the
-- settings, so a control whose value was refused shows the one in effect again, then the error
-- region, cleared or with the answer, the focus left where it is (actions.luau's refusals do so).

local function answer(text)
  post(settingsMessage())
  post({ kind = "error", text = text or "" })
end

-- Setting `s` set to `value` for the page: what its callbacks left in `asking`, and why the set
-- raised, if it did (the page's checks come first, so only a host's refusal of its own).
local function change(s, value)
  asking = {}
  local ok, err = pcall(host.settings.set, s.key, value)
  local left = asking
  asking = nil
  if not ok then
    host.log.info(string.format("[avenger] [page] %s could not be set to %s: %s", s.key, tostring(value),
      tostring(err)))
    left.failed = true
  end
  return left
end

-- The page that asked was described again (the Mac mode, O:pageAgain): it is gone, and the new
-- one is told by its own `ready` — so the answer is spoken, after Close.wav, as for any page that
-- goes (page:close calls no onClose for the module's own close, docs/api/gui.md). The same words
-- on both systems, and none about what the mode does: on Windows `activates = false` changes
-- nothing (docs/api/gui.md), and the page is described again there all the same.
local function modeWords(leaves)
  return byKey.pageLeavesDawActive.words .. " is " .. onOff(leaves) .. " now. Press " .. keyWords()
    .. " to open the page again."
end

local function gone(first, leaves)
  sound("Close")
  say((first and (first .. " ") or "") .. modeWords(leaves))
end

local function valueWords(s, v)
  if s.key == "pageKey" then return describeKey(v) end
  return s.say(v)
end

-- Whether `a` is the value `b` stored: the same key, for the page key, however it is written.
local function same(s, a, b)
  if s.key == "pageKey" then
    local ok, na, nb = pcall(function() return host.keys.normalize(a), host.keys.normalize(b) end)
    if ok and na ~= nil and na == nb then return true end
  end
  return a == b
end

local function noSetting(label, name)
  local why = (type(name) ~= "string" or name == "") and "no setting was named"
    or (name .. " is no setting of this page")
  host.log.info(string.format("[avenger] [page] '%s' from the page: %s", label, tostring(why)))
  answer(label .. ": " .. why)
end

-- set_setting {key, value}: one of the six, checked (P.SETTINGS), stored where it is not the value
-- already stored — the callbacks then do what a change does — and answered.
P.HANDLERS.set_setting = function(_, args)
  local s = type(args.key) == "string" and byKey[args.key] or nil
  if not s then return noSetting(P.LABELS.set_setting, args.key) end
  local value = args.value
  if s.key == "pageKey" and type(value) == "string" then value = trim(value) end
  local bad = s.check(value)
  if bad then
    host.log.info(string.format("[avenger] [page] setting %s from the page refused: %s", s.key, bad))
    return answer(s.words .. ": " .. bad)
  end
  local old = host.settings.get(s.key)
  if same(s, value, old) then return answer(s.words .. ": unchanged, " .. valueWords(s, old)) end
  host.log.info(string.format("[avenger] [page] setting %s from the page: %s -> %s", s.key, tostring(old),
    tostring(value)))
  local left = change(s, value)
  if left.rebuilt ~= nil then return gone(nil, left.rebuilt) end
  if left.said then say(left.said) end
  local text = nil
  if left.refused then
    text = string.format("%s: %s cannot be the page key (%s); it stays %s", s.words, value, left.refused, keyWords())
  elseif left.failed then
    text = s.words .. ": " .. valueWords(s, value) .. " cannot be stored now"
  end
  answer(text)
end

-- reset_settings {key?}: with a key, that one setting back at its default (the page key's Reset to
-- Default) as set_setting would set it; without, every one, in RESET_ORDER.
P.HANDLERS.reset_settings = function(_, args)
  local label = P.LABELS.reset_settings
  if args.key ~= nil then
    local s = type(args.key) == "string" and byKey[args.key] or nil
    if not s then return noSetting(label, args.key) end
    return P.HANDLERS.set_setting(nil, { key = s.key, value = s.default })
  end
  local refused, rebuilt = nil, nil
  for _, k in ipairs(RESET_ORDER) do
    local s = byKey[k]
    host.log.info(string.format("[avenger] [page] reset: %s %s -> %s", k, tostring(host.settings.get(k)),
      tostring(s.default)))
    local left = change(s, s.default)
    refused = left.refused or refused
    if left.rebuilt ~= nil then rebuilt = left.rebuilt end
  end
  local text = "every setting is back at its default"
  if refused then text = text .. " but the page key, which stays " .. keyWords() .. " (" .. refused .. ")" end
  if rebuilt ~= nil then return gone(string.upper(string.sub(text, 1, 1)) .. string.sub(text, 2) .. ".", rebuilt) end
  answer(label .. ": " .. text)
end

-- ── What main.luau calls ─────────────────────────────────────────────────────

-- At load, before the header overlay is bound (O:addPage asks it then): the settings, the key and
-- the page. `env`: { header = the header overlay, zoomOf = (origin) -> the zoom read or nil, DB =
-- presets.luau, ring = main.luau's own clicks, reads and choices, for the trips (actions.luau) }.
function P.start(env)
  E = env
  -- The six, and a stored value outside its schema set back to its default (P.SETTINGS); the page
  -- key's stored value is checked as a key, below.
  for _, s in ipairs(P.SETTINGS) do
    local v = host.settings.define(s.key, s.default, s.opts)
    local bad = s.key ~= "pageKey" and s.check(v) or nil
    if bad then
      host.log.info(string.format("[avenger] [page] the stored %s setting: %s — %s instead", s.key, bad,
        tostring(s.default)))
      host.settings.set(s.key, s.default)
    end
  end
  local stored = host.settings.get("pageKey")

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
    openLater = true,
    onMessage = onMessage,
    onReady = onReady,
    onClose = onClose,
    onShown = onShown,
    onBack = onBack,
  }
  env.header:addPage(spec)

  -- Every one of the six settings' callbacks ends with the settings posted (postSettings), the
  -- dialog's OK — which sets every setting, changed or not — included.
  host.settings.onChange("pageKey", function(new)
    if new == key then return postSettings() end
    local because = refusal(new)
    if not because then
      local fine, err = env.header:pageKey(new)
      if not fine then because = err end
    end
    if not because then
      key = new
      -- What the key says elsewhere names the key: the runtime reads `notHere` at every press.
      spec.notHere = notHereWords()
      host.log.info("[avenger] [page] the page key is now " .. tostring(new))
      local words = "Page key changed to " .. keyWords() .. "."
      if asking then asking.said = words else say(words) end
      return postSettings()
    end
    -- Refused: the key in effect stays, and the setting says it again — spoken for the dialog's,
    -- in the error region for the page's (P.HANDLERS.set_setting).
    host.log.info(string.format("[avenger] [page] '%s' cannot be the page key (%s); it stays %s",
      tostring(new), tostring(because), tostring(key)))
    if asking then
      asking.refused = whyWords(because)
    else
      say(string.format("%s cannot be the page key; it stays %s.", tostring(new), keyWords()))
    end
    if key then host.settings.set("pageKey", key) end
    postSettings()
  end)
  host.settings.onChange("pageLeavesDawActive", function(new, old)
    if new ~= old then
      -- Fixed for the life of a page object: the page is taken down and described again, and the
      -- new one is told the settings by its own `ready`.
      env.header:pageAgain()
      host.log.info("[avenger] [page] the page is described again: activates = " .. tostring(not new))
      if asking then asking.rebuilt = new end
    end
    postSettings()
  end)
  host.settings.onChange("roster", function(new, old)
    if new ~= old and ready then answerReady() end
    postSettings()
  end)

  A.start({
    header = env.header,
    DB = env.DB,
    ring = env.ring,
    zoomOf = env.zoomOf,
    LABELS = P.LABELS,
    post = post,
    plugin = function() return plugin end,
    shown = function() return lastShown end,
    postState = postState,
    closedWords = closedWords,
    sound = sound,
  })
  -- DRUM SQ's watcher, once its cadence is defined: on the header overlay, quiet while the page is
  -- shown.
  Watch.start({
    header = env.header,
    page = function() return env.header.page end,
    ringBusy = env.ring.ringBusy,
    ringValues = env.ring.ringValues,
    ringSeen = env.ring.ringSeen,
    S = S,
  })
  -- After the watcher's own, which arms it at the new pace: the settings posted once it has.
  host.settings.onChange("sounds", postSettings)
  host.settings.onChange("drumSqCadence", postSettings)
  -- The view stored, posted with the state where the grid reads in a view not read — said as set or
  -- confirmed only where the page's own action stored it (Br.settingChanged).
  host.settings.onChange("expansionView", function(new, old)
    Br.settingChanged(new)
    local id = plugin and plugin.id
    local v = S.values(id)
    if v and v.expansionOpen == false and v.expansionViewFrom ~= nil then
      local view, how = Br.view()
      S.set(id, { expansionView = view, expansionViewFrom = how })
    end
    if new ~= old and ready then answerReady() end
    postSettings()
  end)
  host.log.info("[avenger] [page] " .. keyWords() .. " shows the Avenger Control page beside an identified VPS Avenger")
end

return P
