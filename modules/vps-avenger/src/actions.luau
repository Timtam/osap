-- The page's header actions in Avenger (the plan's slice 4): each a trip of the overlay runtime's
-- (O:inPlugin, docs/api/overlay.md) — Avenger's window brought forward, the click and the read made
-- in it, and the page brought back with what was read — through stage 1's own clicks, reads and
-- choices (main.luau, `ring`) and its tables (geometry.luau), so the page clicks where the ring
-- clicks:
--
--   prev_preset, next_preset   ◀ or ▶, then the name watched until it changes (O:watch), and
--                              "unchanged" said when it has not by the watch's bound; on Init
--                              Preset nothing is sent, and the error region says why
--   pick_load_preset,          MENU's item chosen in Avenger's own menu (O:chooseMenuItem), then
--   pick_save_preset_as        the file dialog that comes next: the trip waits for it, and for
--                              Avenger in front again (trip:afterDialog), then watches the name
--   pick_initialize_preset     MENU's Initialize, then Avenger's warning box READ and YES clicked
--                              where it was read (dialog.luau, D.answer), then everything read
--                              again, as at the key (read/open.luau)
--   undo                       UNDO, then the name watched
--   redo_menu                  UNDO right-clicked, as the ring does: the trip holds while the menu
--                              tests see the redo list, which the user chooses in, and goes on
--                              once they see it closed (trip:afterMenu); then the name watched
--   set_zoom                   the zoom list's entry for the size asked — any entry, where the
--                              ring's stepper takes the one beside — then the zoom field read
--
-- and the oscillator strip's and the macros' (the plan's slice 5), each through src/act.luau with
-- its spec from src/actions/osc.luau or src/actions/macros.luau, which the ring's controls share:
-- the part read, the click, its change watched for, the part read again (`viaAct` below);
--
-- and the renames and Browse Presets (the plan's slice 6), hand-overs of the keyboard to Avenger's
-- own editor and browser (src/handover.luau):
--
--   start_rename               the preset's name double-clicked, Avenger's editor seen open, the
--   rename_osc {index}         user's typing Avenger's until Return or Escape and the field seen
--   rename_macro {index}       closed again; then the name READ off a picture — never recorded
--                              from the keys
--   browse_expansion_presets   the tool's sentence, Return let through to Avenger's browser, then
--                              the name read until it settles
--
-- and the knobs turned (the plan's slice 7), each a press held through a hand-over of the keys
-- (src/amount.luau): the knob pressed and held, the tool's sentence said; PageUp and PageDown one
-- move each, captured — none of them reaches Avenger or the DAW; Return lets go and the knob is
-- READ, Escape lets go and reads nothing:
--
--   macro_amount {index}       a macro knob's amount
--   osc_control {control}      the selected oscillator's Volume, Pan, Transpose, Fine Tune or Random
--                              Pitch — nothing pressed on the Drums or the Trig tab
--   osc_filter_control {control}   Low-Pass or High-Pass, along x: no reader, Return claims nothing
--
-- and the Drums tab (the plan's slice 8, its drums), each through src/act.luau with its spec from
-- src/actions/drums.luau — nothing clicked unless the strip reads the Drums tab selected — but two:
-- a slot's knob turned (drumkit_control, src/amount.luau, Am.drum) and a slot's Browse (a hand-over
-- of the keyboard to Avenger's sample browser, src/handover.luau, Hv.browseSlot):
--
--   toggle_drum_pattern, select_drum_mode {mode}, previous_drumkit, next_drumkit,
--   select_drumkit_part {index}, toggle_drumkit_button {index, button},
--   drumkit_menu_item {index, item, target_index?}, drumkit_column {index, column},
--   drumkit_control {index, control}, reset_drumkit_control {index, control},
--   select_central_tab {tab}
--
-- and the sample editor and DRUM SQ (the plan's slice 8, its other half), each through src/act.luau
-- with its spec from src/actions/sample.luau or src/actions/drumsq.luau — nothing clicked in the
-- sample editor unless the Drums tab, EDITOR and the editor's own tab read shown, nothing in DRUM SQ
-- unless it reads as the central tab — but these: the knobs turned (sample_editor_control,
-- central_drum_sq_control: src/amount.luau, Am.sample and Am.drumSq), the hand-overs of the keyboard
-- to Avenger (sample_editor_field, sample_browser_focus, and sample_editor_panel_button's Menu and
-- Preset: src/handover.luau), and LENGTH, a double-click and a drag of its own
-- (src/actions/drumsq.luau, Sq.length):
--
--   select_sample_editor_tab {tab}, toggle_sample_editor_enable {tab}, sample_editor_play_mode {item},
--   sample_editor_grid {item}, sample_editor_eq_type {band, item}, sample_editor_loop_stretch {item},
--   sample_editor_panel_button {button}, sample_editor_field {field}, sample_browser_focus {index},
--   sample_editor_control {control}, reset_sample_editor_control {control},
--   central_drum_sq_pattern_player, central_drum_sq_auto_follow, select_central_drum_sq_pattern
--   {pattern}, toggle_central_drum_sq_pattern {pattern}, central_drum_sq_pattern_menu_item {item},
--   central_drum_sq_speed {item}, central_drum_sq_sequence_menu_item {item}, central_drum_sq_length
--   {steps}, central_drum_sq_control {control}, reset_central_drum_sq_control {control}
--
-- Nothing is taken for done: every answer is what was read after the click, and a read that did
-- not come is said as that — after a click or choice that may have changed the preset, with the
-- preset posted as not read (go). After every trip the page gets its state — the preset heading
-- from the name read — then the error, or the focus back where the user was; the words `after`
-- returns are said only when the page does not come back. Errors are sentences of the module's
-- own, in the error region; the log has the rest, and no raised message reaches the page.
--
-- The trip's own checks are the runtime's: one at a time, only from the page with the keyboard,
-- and `trip:inFront()` after every wait, before a click or a read — false has ended the trip.

local G = host.include("src/geometry.luau")
local H = host.include("src/header.luau")
local D = host.include("src/dialog.luau")
local S = host.include("src/state.luau")
local Reads = host.include("src/read/open.luau")
local Act = host.include("src/act.luau")
local Osc = host.include("src/actions/osc.luau")
local Macros = host.include("src/actions/macros.luau")
local Drums = host.include("src/actions/drums.luau")
local Sa = host.include("src/actions/sample.luau")
local Sq = host.include("src/actions/drumsq.luau")
local C = host.include("src/tables/centre.luau")
local Hv = host.include("src/handover.luau")
local Am = host.include("src/amount.luau")

local A = {}

local E = nil -- page.luau's (A.start)
local R = nil -- main.luau's ring helpers

local log = function(s) host.log.info("[avenger] [page] " .. s) end

-- Why a trip ended without its work done, in the page's words.
local GIVEN_UP = {
  key = "stopped by the page key; what VPS Avenger did then was not read",
  ["another application"] = "another window came to the front; nothing more was done",
  ["page closed"] = "the page was closed; nothing more was done",
  ["not in front"] = "VPS Avenger's window did not come to the front; nothing more was done",
  outranked = "another overlay took VPS Avenger's window; nothing more was done",
  failed = "something went wrong in this module; nothing more was done",
}

-- Why a choice in Avenger's menu was not made (O:chooseMenuItem's `why`), in the page's words.
local notChosen = Act.notChosen

-- What the page is told after a trip, and the words for when it does not come back: the state —
-- what the trip read written over what was read before (`set`, `values`, `part`) — then the error,
-- or the focus: on the element the action names (`focus`, the tool's), or back where the user was;
-- an error with a `focus` is a refusal at a limit, the focus put there and the error said beside.
-- `result`: { set = { key -> value }?, values = all of them?, part = { keys, values }? (a part read
-- again, src/act.luau), facts = { key -> value }? (state.luau's), error = a reason?, sentence = a
-- whole sentence?, note = a reason?, focus = an element's id?, said = what was read, in words? }.
local function report(label, result)
  local w = E.plugin()
  local id = w and w.id
  if id and result.values then S.read(id, result.values) end
  if id and result.set then S.set(id, result.set) end
  if id and result.part then S.part(id, result.part.keys, result.part.values) end
  if id and result.facts then
    for k, v in pairs(result.facts) do S.note(id, k, v) end
  end
  E.postState(id)
  local text = result.sentence
    or (result.error and (label .. ": " .. result.error))
    or (result.note and (label .. ": " .. result.note))
  if text then
    if result.focus then
      -- A refusal at a limit (src/act.luau): the focus where the tool leaves it — the eighth tab,
      -- the fourth macro tab, the only one — and then the sentence in the error region, a live
      -- region, which says it without taking the focus there.
      E.post({ kind = "focus", id = result.focus })
      E.post({ kind = "error", text = text })
    else
      E.post({ kind = "error", text = text, focus = true })
    end
    return text
  end
  E.post({ kind = "error", text = "" })
  if result.focus then
    E.post({ kind = "focus", id = result.focus })
  else
    E.post({ kind = "restoreFocus" })
  end
  if result.said then return label .. ": " .. result.said end
  local name = result.set and result.set.preset
  return name and (label .. ": " .. name) or nil
end

-- The name read after a step, with where the preset database has it (marked so in the page), or
-- the expansion cleared where it does not.
local function named(name)
  local expansion = false
  if name and E.DB.present then
    local ok, p = pcall(E.DB.place, name)
    if ok and p then expansion = p.expansion elseif not ok then log("the preset database failed: " .. tostring(p)) end
  end
  return { preset = name or false, expansion = expansion }
end

-- Action `action` as a trip from overlay `o`: `act(o, trip, forget)` in Avenger,
-- `trip:done(result)` its end (`report`'s result). `forget(why, own)` where the trip's click or
-- choice may have changed the preset: the database's position is forgotten (DB.forget's `why` and
-- `own`), and the ring's latest name read (main.luau, "◀ and ▶ on Init Preset"); and from then on
-- a trip that ends without the name read after it — a watch that read nothing, a give-up — posts
-- the preset as not read, so the page claims nothing of a preset it has not seen since.
local function go(o, action, act)
  local label = E.LABELS[action]
  local touched = false
  local function forget(why, own)
    touched = true
    E.DB.forget(why, own)
    R.unseen()
  end
  -- Whatever the trip does, the ring's reads of this stay are not Avenger's any more (ring.luau).
  R.forgetRing()
  local function back(result)
    if touched and not result.values and not (result.set and result.set.preset ~= nil) then
      result.set = result.set or {}
      result.set.preset, result.set.expansion = false, false
    end
    return report(label, result)
  end
  local started, why = o:inPlugin({
    label = label,
    act = function(w, trip) return act(w, trip, forget) end,
    after = function(_, result) return back(result) end,
    onGiveUp = function(_, why2)
      if why2 == "gone" then return back({ sentence = E.closedWords() }) end
      return back({ error = GIVEN_UP[why2] or ("nothing more was done (" .. tostring(why2) .. ")") })
    end,
  })
  -- "busy" and "not in front" are the runtime's to log: a message while a trip is out, or one that
  -- came after the user went elsewhere.
  if started or why == "busy" or why == "not in front" then return end
  if why == "gone" then
    E.post({ kind = "error", text = E.closedWords() })
  elseif why == "nothing held" then
    E.post({ kind = "error", text = label .. ": the page was not opened from VPS Avenger; press the page key there" })
  else
    E.post({ kind = "error", text = label .. ": VPS Avenger's window would not come to the front" })
  end
end

-- The name before the click — read while the act waits — or the last one read; nil for none.
local function nameBefore(o)
  return R.nameNow(o) or R.lastName()
end

-- The name watched after a click, then the trip done with it: `unchanged` says a name that has not
-- changed by the watch's bound ("unchanged" in the page), as ◀ and ▶ say it. A name that changed
-- counts once it has settled (main.luau, watchName): one that never did is "cannot be read now",
-- never posted as the preset.
local function thenName(o, trip, was, unchanged, label)
  R.watchName(o, was, function(now, before, changed)
    if now == nil then return trip:done({ error = "the preset name cannot be read now" }) end
    local note = nil
    if unchanged and not changed and before ~= nil and now == before then note = "unchanged, " .. now end
    trip:done({ set = named(now), note = note })
  end, label)
end

-- ◀ and ▶, and UNDO: a click, then the name. ◀ and ▶ (`steps`) only where the preset read is not
-- Init Preset, which VPS Avenger steps no preset from (main.luau, "◀ and ▶ on Init Preset"): where
-- the page's state has it, nothing is sent to Avenger — no trip — and the error region says why;
-- where the name read in Avenger before the click is it (or, read as nothing, the latest one read
-- was), nothing is clicked, and the page comes back with that name and why.
local function click(action, ringLabel, at, unchanged, steps)
  return function(o)
    if steps then
      local w = E.plugin()
      local why = S.stepsUnavailable(w and w.id)
      if why then
        log(string.format("'%s': the page's preset is Init Preset — nothing sent to VPS Avenger", ringLabel))
        E.post({ kind = "error", text = E.LABELS[action] .. ": " .. why })
        return
      end
    end
    go(o, action, function(w, trip, forget)
      local was = nameBefore(w)
      if not trip:inFront() then return end
      if steps and R.atInit() then
        log(string.format("'%s': the name read in VPS Avenger is Init Preset — nothing clicked", ringLabel))
        return trip:done({ set = named(was), error = S.AT_INIT })
      end
      local ok, said = R.clickAt(w, ringLabel, at, nil, true)
      if not ok then return trip:done({ sentence = said }) end
      -- A step or an undo the database's position does not follow from the page.
      forget("the page's " .. ringLabel)
      thenName(w, trip, was, unchanged, ringLabel)
    end)
  end
end

-- MENU's item `item` (`ringLabel` as the ring names it), then `andThen(o, trip, was)` once the
-- choice has been taken, `forget` (go) told first; the popup closed again when a choice was given
-- up with it left open.
local function menuChoice(w, trip, forget, ringLabel, item, andThen)
  local was = nameBefore(w)
  if not trip:inFront() then return end
  local pick = {}
  local opened = w:chooseMenuItem({
    label = ringLabel,
    at = G.MENU,
    menuItem = R.menuItemAt(ringLabel, item, pick),
    onDone = function(m, chosen, why)
      if not chosen then
        R.closeLeftOpen(m, pick, why, ringLabel, G.MENU)
        return trip:done({ error = notChosen(why) })
      end
      -- Every item may load, rename or blank the preset: the database's position is not Avenger's.
      forget(ringLabel .. " chosen from the page", R.OWN_NAME[ringLabel])
      andThen(m, trip, was)
    end,
  })
  if opened then
    H.popup(w:origin())
  elseif trip.live then
    trip:done({ error = "MENU cannot be opened now" })
  end
end

-- Load and Save as: the file dialog Avenger opens is the user's; the trip waits for it and for
-- Avenger in front again, then watches the name. No dialog within the runtime's bound is said, and
-- the name read all the same.
local function fileItem(action, ringLabel, item)
  return function(o)
    go(o, action, function(w, trip, forget)
      menuChoice(w, trip, forget, ringLabel, item, function(m, t, was)
        t:afterDialog(function(o2, t2, seen)
          if not seen then log(string.format("'%s': no file dialog came within the bound", ringLabel)) end
          R.watchName(o2, was, function(now)
            if now == nil then return t2:done({ error = "the preset name cannot be read now" }) end
            t2:done({ set = named(now), note = (not seen) and "no file dialog was seen" or nil })
          end, ringLabel)
        end)
      end)
    end)
  end
end

A.HANDLERS = {
  prev_preset = click("prev_preset", "Previous preset", G.PREV, true, true),
  next_preset = click("next_preset", "Next preset", G.NEXT, true, true),
  undo = click("undo", "Undo", G.UNDO, false),
  pick_load_preset = fileItem("pick_load_preset", "Load preset", G.LOAD),
  pick_save_preset_as = fileItem("pick_save_preset_as", "Save preset as", G.SAVE_AS),
}

-- Initialize: the page's own dialog has asked Yes. Not chosen where Avenger's box would lie off
-- the screen, as the ring refuses it; otherwise MENU's Initialize, Avenger's box read and answered
-- YES where it was read (D.answer), and once it has gone, everything read again in the last read's
-- answer, a handler where the reads wait.
A.HANDLERS.pick_initialize_preset = function(o)
  go(o, "pick_initialize_preset", function(w, trip, forget)
    local r, zoom = R.boxOffScreen(w)
    if r then
      return trip:done({ error = "Avenger's warning box would open off the screen"
        .. (zoom and (" at " .. zoom .. " percent") or "") .. "; zoom out first" })
    end
    menuChoice(w, trip, forget, "Initialize preset", G.INITIALIZE, function(m, t)
      D.answer(m, t, "Initialize preset", function(ok, why)
        if not ok then return t:done({ error = why }) end
        if not t:inFront() then return end
        local origin = m:origin()
        local okR, values, whyNot = pcall(Reads.read, m, { zoom = origin and E.zoomOf(origin), mark = m:here(),
          roster = E.roster() })
        if not (okR and values) then
          log("Initialize Preset: VPS Avenger was not read again — " .. tostring(okR and whyNot or values))
          return t:done({ error = "VPS Avenger cannot be read again now" })
        end
        if values.preset then R.noteName(values.preset) end
        t:done({ values = values })
      end)
    end)
  end)
end

-- The redo list: UNDO right-clicked, as the ring's Redo list does. The menu tests are asked for
-- the list BEFORE the click (it counts as the press), the user chooses in it, and once they see it
-- closed, the name.
A.HANDLERS.redo_menu = function(o)
  go(o, "redo_menu", function(w, trip, forget)
    local was = nameBefore(w)
    if not trip:inFront() then return end
    local ok, why = trip:afterMenu(function(o2, t2, seen)
      if not seen then return t2:done({ error = "the redo list was not seen" }) end
      thenName(o2, t2, was, false)
    end)
    if not ok then return trip:done({ error = "the redo list cannot be opened now (" .. tostring(why) .. ")" }) end
    local clicked, said = R.clickAt(w, "Redo list", G.UNDO, "right", true)
    if not clicked then return trip:done({ sentence = said }) end
    H.popup(w:origin())
    -- What the list redoes is chosen in the list.
    forget("the redo list opened from the page")
  end)
end

-- Size: the zoom list's entry for `args.percent`, wherever it lies in the list, then the zoom
-- field read until it shows a zoom again (header.luau reads it until it has changed, or has shown
-- the same one so often that the step did not take).
A.HANDLERS.set_zoom = function(o, args)
  local to = tonumber(args.percent)
  go(o, "set_zoom", function(w, trip)
    local origin = w:origin()
    local t = to and G.zoomIndex(to)
    if not t then return trip:done({ error = tostring(args.percent) .. " is not one of VPS Avenger's sizes" }) end
    local z = origin and H.zoom(origin)
    if not z then return trip:done({ error = "the size VPS Avenger is at cannot be read now" }) end
    if H.stepping(origin) then return trip:done({ error = "the last size chosen is still being read" }) end
    if z == to then return trip:done({ set = { zoom = z } }) end
    local pick = {}
    local opened = w:chooseMenuItem({
      label = "Zoom",
      at = G.ZOOM_FIELD,
      menuItem = R.zoomEntry(t, z, to, pick),
      onDone = function(m, chosen, why)
        if not chosen then
          if why ~= "busy" then
            H.zoomOver()
            R.closeLeftOpen(m, pick, why, "Zoom", G.ZOOM_FIELD)
          end
          return trip:done({ error = notChosen(why) })
        end
        H.stepped(m:origin() or origin, z)
        m:watch({
          -- The zoom asked first either way, as the ring's stepper asks it: asking is what reads
          -- the field again while the step waits.
          read = function(o3)
            local g = o3:origin()
            local now = H.zoom(g)
            if H.stepping(g) then return nil end
            return now
          end,
          done = function(now) return now ~= nil end,
          within = R.ZOOM_SETTLE,
          onDone = function(now)
            trip:done({ set = { zoom = now }, note = now == z and ("VPS Avenger stays at " .. z .. " percent") or nil })
          end,
          onGiveUp = function() trip:done({ error = "the zoom field cannot be read now" }) end,
        })
      end,
    })
    if opened then
      H.popup(origin, true)
    elseif trip.live then
      trip:done({ error = "the size list cannot be opened now" })
    end
  end)
end

-- ── The oscillator strip and the macros ───────────────────────────────────────
-- Action `action` as a trip through src/act.luau: `build(args, label, known)` its spec (osc.luau,
-- macros.luau), `known` what was last read in Avenger's window, for its words. A builder's refusal
-- — arguments the page cannot have meant — is posted at once, with no trip: nothing in Avenger is
-- needed for it.
local function viaAct(action, build)
  return function(o, args)
    local label = E.LABELS[action]
    local w = E.plugin()
    local spec, why = build(args, label, S.values(w and w.id))
    if not spec then
      log(string.format("'%s' not sent to VPS Avenger: %s", label, tostring(why)))
      E.post({ kind = "error", text = label .. ": " .. tostring(why) })
      return
    end
    go(o, action, function(m, trip)
      Act.run(m, spec, function(result)
        -- nil: the trip's own check ended it, and its give-up says the rest.
        if result then trip:done(result) end
      end, trip)
    end)
  end
end

for _, name in ipairs({ "select_osc_tab", "add_osc", "select_drums_tab", "select_trig_tab", "toggle_osc_mute",
  "toggle_osc_solo", "toggle_drums_mute", "toggle_drums_solo", "toggle_trig_mute", "toggle_trig_solo",
  "osc_menu_item", "drums_menu_item", "trig_menu_item", "reset_osc_control" }) do
  A.HANDLERS[name] = viaAct(name, Osc[name])
end
for _, name in ipairs({ "select_macro_tab", "add_macro_tab", "delete_macro_tab", "toggle_macro_button",
  "macro_reset_mode", "macro_button_menu_item", "reset_macro_amount" }) do
  A.HANDLERS[name] = viaAct(name, Macros[name])
end

-- ── The renames and Browse Presets ───────────────────────────────────────────
-- Each a trip whose act hands the keyboard over (src/handover.luau). An index the page cannot have
-- meant is refused at once, with no trip, as a builder's refusal is (viaAct); an OSC tab past the
-- count is refused in Avenger, once the dots are read.
local function refuse(action, why)
  local label = E.LABELS[action]
  log(string.format("'%s' not sent to VPS Avenger: %s", label, why))
  E.post({ kind = "error", text = label .. ": " .. why })
end

local function whole(v)
  local n = tonumber(v)
  return n and n == math.floor(n) and n or nil
end

A.HANDLERS.start_rename = function(o)
  go(o, "start_rename", function(w, trip, forget) Hv.renamePreset(w, trip, forget) end)
end

A.HANDLERS.rename_osc = function(o, args)
  local i = whole(args.index)
  if not (i and i >= 1) then return refuse("rename_osc", "no oscillator tab was named") end
  go(o, "rename_osc", function(w, trip) Hv.renameOsc(w, trip, i) end)
end

A.HANDLERS.rename_macro = function(o, args)
  local i = whole(args.index)
  if not (i and i >= 1 and i <= 5) then return refuse("rename_macro", "no macro field was named") end
  go(o, "rename_macro", function(w, trip) Hv.renameMacro(w, trip, i) end)
end

A.HANDLERS.browse_expansion_presets = function(o)
  go(o, "browse_expansion_presets", function(w, trip, forget) Hv.browse(w, trip, forget) end)
end

-- ── The knobs turned ─────────────────────────────────────────────────────────
-- Each a trip whose act presses the knob and hands the keys over (src/amount.luau), its spec built
-- at once: a knob or a band the page cannot have meant is refused with no trip, as above.
local function amount(action, build)
  return function(o, args)
    local w = E.plugin()
    local spec, why = build(args, E.LABELS[action], S.values(w and w.id))
    if not spec then return refuse(action, why) end
    go(o, action, function(m, trip) Am.run(m, trip, spec) end)
  end
end
A.HANDLERS.macro_amount = amount("macro_amount", Am.macro)
A.HANDLERS.osc_control = amount("osc_control", Am.osc)
A.HANDLERS.osc_filter_control = amount("osc_filter_control", Am.filter)

-- ── The Drums tab ────────────────────────────────────────────────────────────
for _, name in ipairs({ "toggle_drum_pattern", "select_drum_mode", "previous_drumkit", "next_drumkit",
  "select_drumkit_part", "toggle_drumkit_button", "drumkit_menu_item", "reset_drumkit_control",
  "select_central_tab" }) do
  A.HANDLERS[name] = viaAct(name, Drums[name])
end
A.HANDLERS.drumkit_control = amount("drumkit_control", Am.drum)
-- A slot's Back, Next and Edit Sample through src/act.luau; its Browse a hand-over.
do
  local columns = viaAct("drumkit_column", Drums.drumkit_column)
  A.HANDLERS.drumkit_column = function(o, args)
    local i, column = Drums.columnArgs(args)
    if not i then return refuse("drumkit_column", column) end
    if column ~= "browse" then return columns(o, args) end
    go(o, "drumkit_column", function(w, trip) Hv.browseSlot(w, trip, i, E.LABELS.drumkit_column) end)
  end
end

-- ── The sample editor ────────────────────────────────────────────────────────
for _, name in ipairs({ "select_sample_editor_tab", "toggle_sample_editor_enable", "sample_editor_play_mode",
  "sample_editor_grid", "sample_editor_eq_type", "sample_editor_loop_stretch", "reset_sample_editor_control" }) do
  A.HANDLERS[name] = viaAct(name, Sa[name])
end
A.HANDLERS.sample_editor_control = amount("sample_editor_control", Am.sample)
-- The envelope's Back and Next through src/act.luau; its Menu and Preset a hand-over.
do
  local steps = viaAct("sample_editor_panel_button", Sa.sample_editor_panel_button)
  A.HANDLERS.sample_editor_panel_button = function(o, args)
    local button, why = Sa.panelArg(args)
    if not button then return refuse("sample_editor_panel_button", why) end
    if button == "back" or button == "next" then return steps(o, args) end
    go(o, "sample_editor_panel_button", function(w, trip)
      Hv.browseEnvelope(w, trip, button, E.LABELS.sample_editor_panel_button)
    end)
  end
end
A.HANDLERS.sample_editor_field = function(o, args)
  local field, why = Sa.fieldArg(args)
  if not field then return refuse("sample_editor_field", why) end
  go(o, "sample_editor_field", function(w, trip) Hv.renameSample(w, trip, field, E.LABELS.sample_editor_field) end)
end
-- A row from 1 to 13: the tool clamped any other to them.
A.HANDLERS.sample_browser_focus = function(o, args)
  local i = whole(args.index)
  if not (i and i >= 1 and i <= C.BROWSER_ITEM_COUNT) then
    return refuse("sample_browser_focus", "no row of the sample browser was named")
  end
  go(o, "sample_browser_focus", function(w, trip) Hv.browseSample(w, trip, i, E.LABELS.sample_browser_focus) end)
end

-- ── DRUM SQ ──────────────────────────────────────────────────────────────────
for _, name in ipairs({ "central_drum_sq_pattern_player", "central_drum_sq_auto_follow",
  "select_central_drum_sq_pattern", "toggle_central_drum_sq_pattern", "central_drum_sq_pattern_menu_item",
  "central_drum_sq_speed", "central_drum_sq_sequence_menu_item", "reset_central_drum_sq_control" }) do
  A.HANDLERS[name] = viaAct(name, Sq[name])
end
A.HANDLERS.central_drum_sq_control = amount("central_drum_sq_control", Am.drumSq)
A.HANDLERS.central_drum_sq_length = function(o, args)
  local steps, why = Sq.lengthArg(args)
  if not steps then return refuse("central_drum_sq_length", why) end
  go(o, "central_drum_sq_length", function(w, trip) Sq.length(w, trip, steps, E.LABELS.central_drum_sq_length) end)
end

-- What page.luau hands over: { header, DB, ring, zoomOf, LABELS, post, roster, plugin = () -> the
-- window the page was shown beside, postState = (id) -> (), closedWords = () -> the sentence,
-- sound = (name) -> (), the page's sounds behind its setting }.
function A.start(env)
  E, R = env, env.ring
  Act.start(env.ring)
  Hv.start({ ring = env.ring, named = named, plugin = env.plugin })
  Am.start({ ring = env.ring, plugin = env.plugin, sound = env.sound })
  Sq.start({ ring = env.ring, plugin = env.plugin })
end

return A
