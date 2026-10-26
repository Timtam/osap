//! VPS Avenger's renames and Browse Presets from the page (the plan's slice 6,
//! `modules/vps-avenger/src/handover.luau` and `src/read/editor.luau`), on the overlay runtime's
//! hand-over (`trip:handOver`), loaded for real against the scripted host of
//! `overlay_avenger_tests.rs`, whose painted picture of Avenger now shows Avenger's own text editor
//! over a name field (`editor`) and whose change waits keep their options.
//!
//! What the plan says is pinned here. A rename double-clicks the field where the tool does — an OSC
//! tab that is not the one selected selected first, its change seen, so that its selection is no
//! editor — sees the editor open in a change wait from the field's picture — a box over a tenth of
//! it, standing still — and only then hands the keyboard over, with nothing said; an editor never
//! seen open is said, and nothing is handed over; a hand-over refused is said, and nothing held. Return or Escape go through to Avenger and end it once the field's
//! edges read closed again — waited for when Avenger repaints late, said when it is still open — and
//! the name is then READ off a picture: the preset's (kept with the database's place when it is the
//! same as the name read first, or after Escape; the user's own otherwise), the OSC tab's with the
//! strip, the macro field's with the macro panel, each landing the page's focus on its Rename button;
//! a name not read back is said, never posted as the one before. Browse Presets clicks
//! nothing, says the tool's sentence, holds Return alone and reads the name until it settles. The
//! page key and another window end a rename or a browse with the preset posted as not read. The
//! module sends, types and records no key, and no picture is held at the end of any of it.

use super::overlay_avenger_tests::{run, run_mac};

/// Avenger at 80 % in REAPER's floating window on a Mac (or on Windows, set up by the scenario),
/// painted as `spec` says, the page open in front; and the reads a rename makes.
const OPEN: &str = r##"
local S = T.S
function T.pageOn(spec, words)
  local header, box, W = T.avengerAt80("Init Preset")
  T.pageReady()
  T.paintAvenger(98, 84, 1.6, spec)
  T.words(words or {})
  T.openPage()
  T.event()
  return header, box, W
end
-- What the text reads say from here on: the preset's name where it is read on the event loop
-- (`nameAt`, its region at 80 % unless given), the field renamed where it is read back off a picture
-- (its entry's name), and the named regions of a part's read from `words`.
function T.reads(name, renamed, words, nameAt)
  S.words = words or {}
  S.ocr = function(g, n)
    if n == "the field renamed" then return renamed or "" end
    if n then return S.words[n] or "" end
    return T.region(g) == (nameAt or "152,95,440,119") and (name or "") or ""
  end
end
function T.settle(n) T.tick(n or 3) end
-- The picture as painted, with Avenger's editor open over table rectangle `field` (at 50 %), or not.
function T.editor(spec, field)
  local s = table.clone(spec)
  s.editor = field
  T.repaint(s)
end
-- The rename's start: the page's message, the double-click's second click, and the editor painted
-- over `field`, seen open: the hand-over.
function T.renameOpen(m, W, spec, field)
  T.tripFrom(m, W)
  T.tick()
  T.editor(spec, field)
  T.tick()
end
-- Return or Escape typed into Avenger's editor, which closes it (repainted as `spec`): the key
-- through to it, the tick that sees it, and the turns of the end, the read and the back step.
function T.commit(key, spec)
  if spec then T.repaint(spec) end
  local how = T.typed(key)
  T.settle(4)
  return how
end
function T.posted()
  local out = {}
  for _, p in ipairs(T.page().posts) do out[#out + 1] = p end
  return out
end
"##;

fn scenario(s: &str) -> String {
    format!("{OPEN}\n{s}")
}

// ---------------------------------------------------------------------------------------------
// The preset's name.
// ---------------------------------------------------------------------------------------------

/// Renaming the preset: the name read first, then the field's picture, then two clicks at the tool's
/// point (123, 14) through the frame and the factor; the change wait from that picture, settling,
/// over a tenth of it; the editor painted, the keyboard handed over — Return and Escape held, the
/// flag on — with nothing said. Return typed and the editor gone: its edges read closed, the name
/// read back — "My Bass", the user's own, with no expansion — and posted, then no error, then the
/// focus on the Rename button; the page back.
#[test]
fn rename_preset_double_clicks_the_name_sees_the_editor_hands_over_and_reads_the_new_name() {
    run_mac(&scenario(r##"
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
        T.reads("LD Lead 1", "My Bass")
        local clicks, said, posts = #S.clicks, #S.speech, #T.page().posts
        local orderFrom = #S.order
        T.tripFrom({ action = "start_rename", args = {}, focused = true }, W)
        T.ok(#S.clicks == clicks + 1, "the first click at once: " .. (#S.clicks - clicks))
        T.tick()
        -- (123, 14) at 1.6 from Avenger's corner (98, 84): 98 + 196.8 -> 295, 84 + 22.4 -> 106.
        T.ok(#S.clicks == clicks + 2 and T.clickAt(clicks + 1) == "295,106" and T.clickAt(clicks + 2) == "295,106",
          "two clicks on the name: " .. T.clickAt())
        local order = table.concat(S.order, ",", orderFrom + 1)
        T.ok(string.find(order, "snapshot(cb)", 1, true) ~= nil, "the field's picture before the clicks: " .. order)
        T.ok(#S.changeWaits == 1, "the change wait: " .. #S.changeWaits)
        local cw = S.changeWaits[1]
        -- The field (34, 7)-(214, 22) grown by 2: (149, 92)-(444, 122), 295 x 30, a tenth 885.
        T.ok(T.region(cw.region) == "149,92,444,122" and cw.settle == 50 and cw.minPixels == 885 and cw.tolerance == 16
          and cw.timeout == 2000, T.region(cw.region) .. " " .. cw.settle .. " " .. cw.minPixels)
        T.ok(not S.holding["Return"], "nothing handed over before the editor is seen")
        T.editor(PLAIN, { 34, 7, 214, 22 })
        T.tick()
        T.ok(S.holding["Return"] and S.holding["Escape"] and T.open() == true and S.scopedTo == W.id, "handed over")
        T.ok(#S.speech == said, "nothing said: " .. T.said(said + 1))
        -- Tab does nothing in Avenger's editor (Act.SWALLOWED): taken while Return and Escape go
        -- through, the rename's sentence once, interrupting.
        T.ok(S.menuTaken["Tab"] and S.menuTaken["Shift+Tab"], "Tab and Shift+Tab swallowed")
        T.ok(T.typed("Tab") == "taken" and T.typed("Shift+Tab") == "taken" and T.typed("Tab") == "taken", "taken")
        T.ok(#S.speech == said + 1 and T.lastSaid() == "Return keeps the name, Escape cancels."
          and S.speech[#S.speech].interrupt == true, T.said(said + 1))
        said = #S.speech
        T.ok(T.count("[avenger] [editor] 'preset': open — changed true, settled true") == 1, T.dump())
        T.ok(T.count("[avenger] [rename] 'preset': the keyboard after the double-click: ") == 1, T.dump())
        T.ok(#T.page().posts == posts, "nothing posted while the user types")
        T.ok(T.held() == 0, "no picture held while the user types: " .. T.held())
        T.ok(T.commit("Return", PLAIN) == "passed", "Return through to Avenger")
        T.ok(T.count("[avenger] [editor] 'preset': closed — the edges ") == 1, T.dump())
        T.ok(T.count("[avenger] [rename] 'preset': Return — read 'My Bass', the user's own") == 1, T.dump())
        local p = T.page().posts
        T.ok(p[#p - 2].kind == "state" and p[#p - 2].state.preset == "My Bass" and p[#p - 2].state.expansion == nil,
          tostring(p[#p - 2].state and p[#p - 2].state.preset))
        T.ok(p[#p - 1].kind == "error" and p[#p - 1].text == "" and p[#p].kind == "focus" and p[#p].id == "rename-preset-button",
          tostring(p[#p].id))
        T.ok(S.front == T.PAGE_WIN and not S.holding["Return"] and T.open() == false, "the page back, the keys back")
        T.ok(not S.menuTaken["Tab"] and not S.menuTaken["Shift+Tab"], "Tab and Shift+Tab given back")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Escape cancels: the old name is read again and kept, with the database's place, however the read
/// back differs from it — "PD Wurm Pud", not roughly the same; and a name read back that is roughly
/// the one Avenger showed at the rename (the tool's `_names_roughly_same`) is kept the same way —
/// the name read first, "AR Deep Arp", not the page's last one. A name of the user's own has no
/// expansion.
#[test]
fn rename_preset_with_escape_or_the_same_name_keeps_the_database_place() {
    for case in ["escape", "same", "own"] {
        let s = r##"
            local case = "@CASE@"
            local PLAIN = { osc = 1, selected = "osc", index = 1 }
            T.presets()
            local _, _, W = T.pageOn(PLAIN, { preset = "PD Warm Pad" })
            T.ok(T.lastPost("state").state.expansion == "Alpha", "placed at the key")
            local back = ({ escape = "PD Wurm Pud", same = "AR deep arp", own = "PD Warm Pad 2 (mine)" })[case]
            -- What Avenger shows at the rename: the page's last name, or one the user chose in Avenger since.
            local shown = case == "same" and "AR Deep Arp" or "PD Warm Pad"
            T.reads(shown, back)
            T.renameOpen({ action = "start_rename", args = {}, focused = true }, W, PLAIN, { 34, 7, 214, 22 })
            T.ok(S.holding["Escape"], "handed over")
            T.ok(T.count("is forgotten: renaming from the page") == 1, "the database's position forgotten: " .. T.dump())
            T.commit(case == "escape" and "Escape" or "Return", PLAIN)
            local st = T.lastPost("state").state
            if case == "own" then
              T.ok(st.preset == back and st.expansion == nil, case .. ": " .. tostring(st.preset) .. " " .. tostring(st.expansion))
              T.ok(T.count("the user's own") == 1, T.dump())
            else
              T.ok(st.preset == back and st.expansion == "Alpha",
                case .. ": " .. tostring(st.preset) .. " " .. tostring(st.expansion))
              T.ok(T.count("the name before ('" .. shown .. "') kept") == 1, T.dump())
            end
            T.ok(T.held() == 0, "every picture released: " .. T.held())
        "##;
        run_mac(&scenario(&s.replace("@CASE@", case)));
    }
}

/// An editor never seen open — nothing repainted after the double-click: the change wait runs to its
/// bound, and the error region says so; no key is ever handed over, no flag is ever set.
#[test]
fn an_editor_never_seen_open_is_said_and_nothing_is_handed_over() {
    run_mac(&scenario(r##"
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
        T.reads("LD Lead 1", "My Bass")
        local flags = #S.menuOpen
        T.tripFrom({ action = "start_rename", args = {}, focused = true }, W)
        T.tick()
        T.run(1800)
        T.ok(#S.changeWaits == 1, "still looking at 1.8 s")
        T.run(450)
        T.ok(T.lastPost("error").text == "Renaming the preset: VPS Avenger's editor did not open; nothing was renamed"
          and T.lastPost("error").focus == true, tostring(T.lastPost("error").text))
        T.ok(T.count("[avenger] [editor] 'preset': not open — changed false") == 1, T.dump())
        for i = flags + 1, #S.menuOpen do T.ok(S.menuOpen[i] ~= true, "no flag set") end
        T.ok(T.count("hands the keyboard over") == 0 and S.front == T.PAGE_WIN, "nothing handed over; the page back")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// A change too small to be the editor — a hover outline along the field's top, under a tenth of its
/// picture — is no editor, in a scripted host that counts the changed pixels as the host does.
#[test]
fn a_change_too_small_is_no_editor() {
    run_mac(&scenario(r##"
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
        T.reads("LD Lead 1", "My Bass")
        S.honourMinPixels = true
        T.tripFrom({ action = "start_rename", args = {}, focused = true }, W)
        T.tick()
        local hover = table.clone(PLAIN)
        hover.lasting = { 34, 7, 214, 8 }
        T.repaint(hover)
        T.run(2200)
        T.ok(T.lastPost("error").text == "Renaming the preset: VPS Avenger's editor did not open; nothing was renamed",
          tostring(T.lastPost("error").text))
        T.ok(not S.holding["Return"] and T.held() == 0, "nothing handed over, nothing held")
        -- The editor itself is over the share: a second rename sees it.
        T.event()
        T.tripFrom({ action = "start_rename", args = {}, focused = true }, W)
        T.tick()
        T.editor(PLAIN, { 34, 7, 214, 22 })
        T.tick()
        T.ok(S.holding["Return"], "the editor is over the share")
        T.commit("Return", PLAIN)
        T.ok(T.lastPost("state").state.preset == "My Bass", "read back")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The editor still open after Return — Avenger did not take it: the field reads open, the repaint is
/// waited for to the bound, and then said, behind the screen reader; the keyboard stays Avenger's. A
/// second Return with the editor gone ends it.
#[test]
fn the_editor_still_open_after_return_keeps_the_hand_over_and_says_so() {
    run_mac(&scenario(r##"
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
        T.reads("LD Lead 1", "My Bass")
        T.renameOpen({ action = "start_rename", args = {}, focused = true }, W, PLAIN, { 34, 7, 214, 22 })
        T.ok(T.commit("Return") == "passed", "through")
        T.ok(T.count("[avenger] [editor] 'preset': open — the edges ") == 1, T.dump())
        T.run(2200)
        T.ok(T.lastSaid() == "VPS Avenger's editor is still open: Return keeps the name, Escape cancels."
          and S.speech[#S.speech].interrupt == false, tostring(T.lastSaid()))
        T.ok(S.holding["Return"] and T.open() == true and S.front ~= T.PAGE_WIN, "the keyboard stays Avenger's")
        T.ok(T.held() == 0, "nothing held while the user types on: " .. T.held())
        T.ok(T.commit("Return", PLAIN) == "passed", "the second Return")
        T.ok(T.lastPost("state").state.preset == "My Bass" and S.front == T.PAGE_WIN, "read back, the page back")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Avenger repainting late after Return: the first picture still shows the editor, the change wait
/// from it sees the repaint, and the field reads closed — not taken for open.
#[test]
fn a_slow_repaint_after_return_is_waited_for_not_taken_for_open() {
    run_mac(&scenario(r##"
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
        T.reads("LD Lead 1", "My Bass")
        T.renameOpen({ action = "start_rename", args = {}, focused = true }, W, PLAIN, { 34, 7, 214, 22 })
        T.typed("Return")
        T.tick()
        T.runDue()
        T.ok(T.count("[avenger] [editor] 'preset': open — the edges ") == 1 and #S.changeWaits == 1, T.dump())
        T.repaint(PLAIN)
        T.settle(4)
        T.ok(T.count("[avenger] [editor] 'preset': closed — the edges ") == 1, T.dump())
        T.ok(T.lastPost("state").state.preset == "My Bass" and S.front == T.PAGE_WIN, "read back")
        T.ok(T.count("is still open") == 0, "nothing said")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The page key while renaming — also while the end is looked at, the editor's repaint waited for —
/// the trip given up, the keys back, the page shown with the preset posted as not read and the page
/// key's words; another application's window while renaming: the same, the page not shown over it.
/// Nothing held either way.
#[test]
fn the_page_key_or_another_window_while_renaming_posts_the_name_as_not_read() {
    for how in ["key", "other", "key while its end is looked at"] {
        let s = r##"
            local how = "@HOW@"
            local PLAIN = { osc = 1, selected = "osc", index = 1 }
            local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
            T.reads("LD Lead 1", "My Bass")
            T.renameOpen({ action = "start_rename", args = {}, focused = true }, W, PLAIN, { 34, 7, 214, 22 })
            T.ok(S.holding["Return"], "handed over")
            if how == "other" then
              S.listed = { W, T.OTHER }
              T.show(T.OTHER)
            else
              if how ~= "key" then
                -- Return through, the editor still painted: its repaint is waited for, a picture held.
                T.typed("Return")
                T.tick()
                T.runDue()
                T.ok(#S.changeWaits == 1 and T.held() == 1, "the repaint waited for: " .. T.held())
              end
              T.pageKey()
            end
            T.settle(2)
            if how ~= "key" and how ~= "other" then T.run(2200) end
            T.ok(not S.holding["Return"] and not S.menuTaken["Tab"] and not S.menuTaken["Shift+Tab"] and T.open() ~= true,
              how .. ": the keys back, Tab and Shift+Tab among them")
            T.ok(T.lastPost("state").state.preset == nil, how .. ": the preset posted as not read")
            local want = how ~= "other" and "Renaming the preset: stopped by the page key; what VPS Avenger did then was not read"
              or "Renaming the preset: another window came to the front; nothing more was done"
            T.ok(T.lastPost("error").text == want, tostring(T.lastPost("error").text))
            T.ok((S.front == T.PAGE_WIN) == (how ~= "other"), how .. ": the page shown only at the key")
            T.ok(T.held() == 0, how .. ": every picture released: " .. T.held())
        "##;
        run_mac(&scenario(&s.replace("@HOW@", how)));
    }
}

/// The keyboard not handed over — a Mac without the Accessibility grant, a capture refused: a rename
/// whose editor is open says so, Browse Presets says nothing of pressing Return; no flag is set, no
/// key is held, and the preset is posted as not read.
#[test]
fn a_hand_over_refused_is_said_and_nothing_is_held() {
    for action in ["start_rename", "browse_expansion_presets"] {
        let s = r##"
            local action = "@ACTION@"
            local PLAIN = { osc = 1, selected = "osc", index = 1 }
            local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
            T.reads("LD Lead 1", "My Bass")
            local flags, said = #S.menuOpen, #S.speech
            S.captureRaises = { Return = "the event tap could not be created (Accessibility)" }
            if action == "start_rename" then
              T.renameOpen({ action = action, args = {}, focused = true }, W, PLAIN, { 34, 7, 214, 22 })
            else
              T.tripFrom({ action = action, args = {}, focused = true }, W)
            end
            T.settle(2)
            local want = action == "start_rename"
              and "Renaming the preset: the keyboard cannot be handed to VPS Avenger now; its editor is still open"
              or "Browse Presets: the keyboard cannot be handed to VPS Avenger now"
            T.ok(T.lastPost("error").text == want, tostring(T.lastPost("error").text))
            T.ok(T.count("the keyboard is not handed over — Return is not captured: the event tap could not be created "
              .. "(Accessibility)") == 1, T.dump())
            for i = flags + 1, #S.menuOpen do T.ok(S.menuOpen[i] ~= true, "no flag set") end
            T.ok(next(S.holding) == nil or (not S.holding["Return"] and not S.holding["Escape"]), "no key held")
            for i = said + 1, #S.speech do
              T.ok(not string.find(S.speech[i].text, "press Return", 1, true), "no Return promised: " .. S.speech[i].text)
            end
            T.ok(T.lastPost("state").state.preset == nil, "the preset posted as not read")
            T.ok(S.front == T.PAGE_WIN and T.held() == 0, "the page back, nothing held: " .. T.held())
            S.captureRaises = nil
        "##;
        run_mac(&scenario(&s.replace("@ACTION@", action)));
    }
}

/// A name that cannot be read back — the field read as nothing after Return — is said for the
/// preset, an OSC tab and a macro field alike; the preset is posted as not read, never as the name
/// before.
#[test]
fn a_name_not_read_back_is_said_and_never_posted_as_the_name_before() {
    for kind in ["preset", "osc", "macro"] {
        let s = r##"
            local kind = "@KIND@"
            local SPEC = { osc = 3, selected = "osc", index = 2, macroShown = 1 }
            local WORDS = { preset = "LD Lead 1", ["osc 1"] = "Bass", ["osc 2"] = "Keys", ["osc 3"] = "Pad",
              ["macro tabs"] = "MACRO 1", ["macro macro1"] = "Cut" }
            local _, _, W = T.pageOn(SPEC, WORDS)
            T.reads("LD Lead 1", "", WORDS)
            local m = ({ preset = { action = "start_rename", args = {} },
              osc = { action = "rename_osc", args = { index = 2 } },
              macro = { action = "rename_macro", args = { index = 1 } } })[kind]
            m.focused = true
            local field = ({ preset = { 34, 7, 214, 22 }, osc = { 341.4, 28, 438.6, 42 }, macro = { 711, 443, 761, 455 } })[kind]
            T.renameOpen(m, W, SPEC, field)
            T.ok(S.holding["Return"], "handed over")
            T.commit("Return", SPEC)
            local label = ({ preset = "Renaming the preset", osc = "Renaming an oscillator", macro = "Renaming a macro" })[kind]
            T.ok(T.lastPost("error").text == label .. ": the new name cannot be read now", tostring(T.lastPost("error").text))
            local st = T.lastPost("state").state
            if kind == "preset" then
              T.ok(st.preset == nil, "the preset posted as not read: " .. tostring(st.preset))
              T.ok(T.count("[avenger] [rename] 'preset': Return — nothing read") == 1, T.dump())
            elseif kind == "osc" then
              T.ok(st.osc_panel.names[2] == "Keys", "the tab's name as read before: " .. tostring(st.osc_panel.names[2]))
              T.ok(T.count("[avenger] [rename] 'osc 2': Return — nothing read") == 1, T.dump())
            else
              T.ok(T.count("[avenger] [rename] 'macro macro1': Return — nothing read") == 1, T.dump())
            end
            T.ok(S.front == T.PAGE_WIN and T.held() == 0, "the page back, nothing held: " .. T.held())
        "##;
        run_mac(&scenario(&s.replace("@KIND@", kind)));
    }
}

// ---------------------------------------------------------------------------------------------
// An oscillator tab and a macro field.
// ---------------------------------------------------------------------------------------------

/// Renaming OSC 2 of 3 with OSC 1 selected: the strip read first, then OSC 2 selected where the
/// count puts it — one click, which Avenger answers with the selection at once — and its change
/// seen and the selection read; only then the field's picture and the double-click on the tab, the
/// editor painted over its name, and after Return the strip and the tab's name read off one picture:
/// "Lead Saw", not snapped, the other tabs as last read; the focus on the tab's Rename button.
#[test]
fn rename_osc_double_clicks_the_tab_reads_its_name_unsnapped_and_focuses_its_rename_button() {
    run_mac(&scenario(r##"
        local SPEC = { osc = 3, selected = "osc", index = 1 }
        local SEL2 = { osc = 3, selected = "osc", index = 2 }
        local _, _, W = T.pageOn(SPEC, { ["osc 1"] = "Bass", ["osc 2"] = "OSC 2", ["osc 3"] = "Pad" })
        T.ok(T.lastPost("state").state.osc_panel.names[2] == "OSC 2", "read at the key")
        T.reads("Init Preset", "Lead Saw")
        local clicks, selected = #S.clicks, nil
        S.onClick = function(n)
          if n == clicks + 1 then
            T.repaint(SEL2)
            selected = S.picture
          end
        end
        -- Tab 2 of 3: (341.3, 28)-(438.7, 42); its point (389.5, 35) -> 98 + 623.2 -> 721, 84 + 56 -> 140.
        T.tripFrom({ action = "rename_osc", args = { index = 2 }, focused = true }, W)
        T.ok(#S.clicks == clicks + 1 and T.clickAt() == "721,140" and S.clicks[#S.clicks].button == nil,
          "one click first: " .. T.clickAt())
        T.ok(T.count("[avenger] [rename] 'osc 2': the strip reads OSC 1 selected — OSC 2 is selected first") == 1, T.dump())
        T.ok(T.count("[avenger] [act] 'Renaming OSC 2': changed — OSC 2 selected") == 1, "its change seen: " .. T.dump())
        T.tick(2)
        T.ok(#S.clicks == clicks + 3 and T.clickAt(clicks + 2) == "721,140" and T.clickAt() == "721,140", T.clickAt())
        T.ok(#S.changeWaits == 1 and S.changeWaits[1].base == selected, "the open wait from a picture with the selection")
        T.editor(SEL2, { 341.4, 28, 438.6, 42 })
        T.tick()
        T.ok(S.holding["Return"], "handed over")
        T.commit("Return", SEL2)
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.names[1] == "Bass" and o.names[2] == "Lead Saw" and o.names[3] == "Pad" and o.names_from_database == nil,
          tostring(o.names[2]))
        T.ok(o.selected_index == 2 and o.count == 3, "the strip read again: " .. tostring(o.selected_index))
        T.ok(T.lastPost().kind == "focus" and T.lastPost().id == "osc-rename-2", tostring(T.lastPost().id))
        T.ok(T.count("[avenger] [rename] 'osc 2': Return — read 'Lead Saw'") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The ring names a tab as the page last read it (the review of 2026-10-26): on AR Band Pass Slide,
/// whose tabs the database names Arp, OSC 2 and Lead, the key reads the first as Pluck, and OSC 2
/// is renamed Wobble from the page. Back in Avenger the Oscillator stepper says Wobble, and Pluck a
/// step to the left. A rename is an edit of the preset's tabs, so the third, its name not read, is
/// OSC 3 — not the database's Lead. ▶ to AR Deep Arp forgets the edit, and ◀ back to AR Band Pass
/// Slide loads it from its file again: the names read before are the preset before's, and the
/// stepper says the database's Arp.
#[test]
fn the_rings_stepper_says_a_tab_as_the_page_read_it_and_none_of_the_databases_after_a_rename() {
    run_mac(&scenario(r##"
        T.presets()
        local header, _, W = T.avengerAt80("AR Band Pass Slide")
        local SEL1 = { osc = 3, selected = "osc", index = 1 }
        local SEL2 = { osc = 3, selected = "osc", index = 2 }
        local SEL3 = { osc = 3, selected = "osc", index = 3 }
        local i = nil
        for k, c in ipairs(header.controls) do if c.label == "Oscillator" then i = k end end
        T.paintAvenger(98, 84, 1.6, SEL2)
        T.words({ preset = "AR Band Pass Slide", ["osc 1"] = "Pluck" })
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, OSC 2, slider", T.said())
        T.pageReady()
        T.openPage()
        T.event()
        T.ok(T.lastPost("state").state.osc_panel.names[1] == "Pluck", "read at the key")
        T.reads("AR Band Pass Slide", "Wobble", { preset = "AR Band Pass Slide", ["osc 1"] = "Pluck" })
        T.renameOpen({ action = "rename_osc", args = { index = 2 }, focused = true }, W, SEL2, { 341.4, 28, 438.6, 42 })
        T.ok(S.holding["Return"], "handed over")
        T.commit("Return", SEL2)
        local o = T.lastPost("state").state.osc_panel
        T.ok(o.names[1] == "Pluck" and o.names[2] == "Wobble" and o.names[3] == nil, tostring(o.names[2]))
        T.event()
        S.focusAtOnce, S.focusLands = true, function() S.front, S.chain = W, { W } end
        T.pageKey()
        T.event()
        T.run(400)
        T.ok(header.active and S.front == W, "the header again")
        T.ok(T.lastSaid() == "Oscillator, Wobble, slider", "the arrival: " .. T.said())
        T.stepAt(header, i, -1)
        T.repaint(SEL1)
        T.tick(4)
        T.ok(string.find(T.lastSaid() or "", "Pluck", 1, true) ~= nil, T.said())
        T.stepAt(header, i, 1)
        T.repaint(SEL2)
        T.tick(4)
        T.stepAt(header, i, 1)
        T.repaint(SEL3)
        T.tick(4)
        T.ok(string.find(T.lastSaid() or "", "OSC 3", 1, true) ~= nil and string.find(T.lastSaid(), "Lead", 1, true) == nil,
          T.said())
        T.repaint({ osc = 1, selected = "osc", index = 1 })
        T.ok(T.step(header, 3, "AR Band Pass Slide", "AR Deep Arp") == "AR Deep Arp", T.said())
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, One Osc, slider", T.said())
        T.repaint(SEL1)
        T.step(header, 2, "AR Deep Arp", "AR Band Pass Slide")
        header.focus = i - 1
        T.tab()
        T.ok(T.lastSaid() == "Oscillator, Arp, slider", "the file's names again: " .. T.said())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// The selection is no editor: renaming OSC 2 with OSC 1 selected, Avenger painting the selection
/// at the first click, the field's picture is taken once the selection has been seen, so the
/// double-click's open wait sees no change — an editor that never opens is said, with the strip as
/// read, and nothing is handed over. The tab now selected is double-clicked at once, with no click
/// of its own.
#[test]
fn rename_osc_takes_no_selection_for_the_editor_and_double_clicks_a_selected_tab_at_once() {
    run_mac(&scenario(r##"
        local SPEC = { osc = 3, selected = "osc", index = 1 }
        local SEL2 = { osc = 3, selected = "osc", index = 2 }
        local _, _, W = T.pageOn(SPEC, { ["osc 1"] = "Bass", ["osc 2"] = "OSC 2", ["osc 3"] = "Pad" })
        T.reads("Init Preset", "Lead Saw")
        S.honourMinPixels = true
        local clicks, flags = #S.clicks, #S.menuOpen
        S.onClick = function(n) if n == clicks + 1 then T.repaint(SEL2) end end
        T.tripFrom({ action = "rename_osc", args = { index = 2 }, focused = true }, W)
        T.tick(3)
        T.ok(#S.clicks == clicks + 3, "selected, then double-clicked: " .. (#S.clicks - clicks))
        T.run(2200)
        T.ok(T.lastPost("error").text == "Renaming an oscillator: VPS Avenger's editor did not open; nothing was renamed",
          tostring(T.lastPost("error").text))
        T.ok(T.count("[avenger] [editor] 'osc': not open — changed false") == 1, T.dump())
        T.ok(T.lastPost("state").state.osc_panel.selected_index == 2, "the strip as read after the selection")
        for i = flags + 1, #S.menuOpen do T.ok(S.menuOpen[i] ~= true, "no flag set") end
        T.ok(T.count("hands the keyboard over") == 0 and not S.holding["Return"] and T.held() == 0,
          "nothing handed over, nothing held: " .. T.held())
        T.event()
        S.onClick = nil
        clicks = #S.clicks
        T.renameOpen({ action = "rename_osc", args = { index = 2 }, focused = true }, W, SEL2, { 341.4, 28, 438.6, 42 })
        T.ok(#S.clicks == clicks + 2 and S.holding["Return"], "the double-click alone, handed over: " .. (#S.clicks - clicks))
        T.ok(T.count("is selected first") == 1, "selected first only the first time")
        T.commit("Return", SEL2)
        T.ok(T.lastPost("state").state.osc_panel.names[2] == "Lead Saw", "read back")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// An OSC tab past the count, or a strip whose dots read no count, is refused with nothing clicked;
/// no index at all is refused at once, with no trip.
#[test]
fn rename_osc_refuses_a_tab_past_the_count_or_with_no_count_and_clicks_nothing() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 3, selected = "osc", index = 1 })
        local clicks, focus = #S.clicks, #S.focusCalls
        T.tripFrom({ action = "rename_osc", args = { index = 4 }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text
          == "Renaming an oscillator: OSC 4 is not there; VPS Avenger shows 3 oscillators", tostring(T.lastPost("error").text))
        T.event()
        T.paintAvenger(98, 84, 1.6, {})
        T.tripFrom({ action = "rename_osc", args = { index = 1 }, focused = true }, W)
        T.settle(2)
        T.ok(#S.clicks == clicks and T.lastPost("error").text == "Renaming an oscillator: the oscillator tabs cannot be "
          .. "counted now", tostring(T.lastPost("error").text))
        T.event()
        focus = #S.focusCalls
        T.pageMsg({ action = "rename_osc", args = {}, focused = true })
        T.ok(#S.focusCalls == focus and T.lastPost("error").text == "Renaming an oscillator: no oscillator tab was named",
          tostring(T.lastPost("error").text))
        T.pageMsg({ action = "rename_macro", args = { index = 6 }, focused = true })
        T.ok(#S.focusCalls == focus and T.lastPost("error").text == "Renaming a macro: no macro field was named",
          tostring(T.lastPost("error").text))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Renaming macro field 4 — the first button's, `btn1` — with macro tab 2 shown: the panel read
/// with its words first, the field double-clicked at the tool's point, and after Return its name read
/// back: macro 4 "Hold On", the others as read; the focus on its Rename button of tab 2.
#[test]
fn rename_macro_double_clicks_the_field_reads_the_label_and_focuses_its_rename_button() {
    run_mac(&scenario(r##"
        local SPEC = { osc = 1, selected = "osc", index = 1, macroShown = 2 }
        local WORDS = { ["macro tabs"] = "MACRO 1 MACRO 2", ["macro macro1"] = "Cut", ["macro macro2"] = "Res",
          ["macro macro3"] = "Env", ["macro btn1"] = "Hold", ["macro btn2"] = "Gate" }
        local _, _, W = T.pageOn(SPEC, WORDS)
        T.reads("Init Preset", "Hold On", WORDS)
        local clicks = #S.clicks
        -- btn1's field point (761, 511): 98 + 1217.6 -> 1316, 84 + 817.6 -> 902.
        T.renameOpen({ action = "rename_macro", args = { index = 4 }, focused = true }, W, SPEC, { 736, 505, 786, 517 })
        T.ok(#S.clicks == clicks + 2 and T.clickAt() == "1316,902", T.clickAt())
        T.ok(S.holding["Return"], "handed over")
        T.commit("Return", SPEC)
        local m = T.lastPost("state").state.macro_panel
        T.ok(m.selected_index == 2 and m.count == 2, "the panel read again: " .. tostring(m.selected_index))
        T.ok(m.selected_labels[1] == "Cut" and m.selected_labels[4] == "Hold On" and m.selected_labels[5] == "Gate",
          tostring(m.selected_labels[4]))
        T.ok(T.lastPost().kind == "focus" and T.lastPost().id == "macro-2-slot-4-rename", tostring(T.lastPost().id))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// With the macro tab shown not read — two tabs, neither strip darker — the field is renamed all the
/// same, and the page restores its own focus.
#[test]
fn rename_macro_with_the_tab_shown_not_read_leaves_the_focus_to_the_page() {
    run_mac(&scenario(r##"
        local SPEC = { osc = 1, selected = "osc", index = 1 }
        local WORDS = { ["macro tabs"] = "MACRO 1 MACRO 2", ["macro macro1"] = "Cut" }
        local _, _, W = T.pageOn(SPEC, WORDS)
        T.reads("Init Preset", "Sweep", WORDS)
        T.renameOpen({ action = "rename_macro", args = { index = 1 }, focused = true }, W, SPEC, { 711, 443, 761, 455 })
        T.ok(S.holding["Return"], "handed over")
        T.commit("Return", SPEC)
        T.ok(T.lastPost().kind == "restoreFocus" and T.lastPost("error").text == "", tostring(T.lastPost().kind))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

// ---------------------------------------------------------------------------------------------
// Browse Presets.
// ---------------------------------------------------------------------------------------------

/// Browse Presets: no click, the tool's sentence behind the screen reader, only Return held; Return
/// through to Avenger's browser, then the name read until two readings agree, posted with the
/// database's place; the focus on the Browse Presets button.
#[test]
fn browse_presets_says_the_tools_sentence_holds_only_return_clicks_nothing_and_reads_the_name_after() {
    run_mac(&scenario(r##"
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        T.presets()
        local _, _, W = T.pageOn(PLAIN, { preset = "LD Palm´s Lead 1" })
        local clicks = #S.clicks
        T.tripFrom({ action = "browse_expansion_presets", args = {}, focused = true }, W)
        T.ok(#S.clicks == clicks, "no click")
        T.ok(S.holding["Return"] and not S.holding["Escape"] and T.open() == true, "Return alone, let through")
        T.ok(T.lastSaid() == "Browse presets in Avenger. To return to Avenger Control, press Return."
          and S.speech[#S.speech].interrupt == false, tostring(T.lastSaid()))
        T.answer(T.lastRead("vps-avenger browse"), T.nameRead("LD Palm´s Lead 1"))
        T.ok(T.typed("Escape") == "passed", "Escape is Avenger's browser's")
        -- Tab does nothing in Avenger's browser (Act.SWALLOWED): taken, the browser's sentence once.
        T.ok(S.menuTaken["Tab"] and S.menuTaken["Shift+Tab"], "Tab and Shift+Tab swallowed")
        local sw = #S.speech
        T.ok(T.typed("Tab") == "taken" and T.typed("Shift+Tab") == "taken", "taken")
        T.ok(#S.speech == sw + 1 and T.lastSaid() == "To return to Avenger Control, press Return."
          and S.speech[#S.speech].interrupt == true, T.said(sw + 1))
        T.tick()
        T.ok(S.holding["Return"], "and no end")
        T.typed("Return")
        T.tick()
        T.runDue()
        T.nameReads("AR Deep Arp", 1200)
        local st = T.lastPost("state").state
        T.ok(st.preset == "AR Deep Arp" and st.expansion == "Alpha",
          tostring(st.preset) .. " " .. tostring(st.expansion))
        T.ok(T.lastPost().kind == "focus" and T.lastPost().id == "browse-expansion-presets-button", tostring(T.lastPost().id))
        T.ok(T.count("[avenger] [browse] the name at the start 'LD Palm´s Lead 1', after Return 'AR Deep Arp' (read ") == 1,
          T.dump())
        T.ok(S.front == T.PAGE_WIN and not S.holding["Return"] and not S.menuTaken["Tab"], "the page back, the keys back")
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// Browse Presets ended by the page key or another window: the preset posted as not read, the keys
/// back.
#[test]
fn browse_presets_ended_by_the_page_key_or_another_window_posts_the_name_as_not_read() {
    for how in ["key", "other"] {
        let s = r##"
            local how = "@HOW@"
            local _, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1 }, { preset = "LD Lead 1" })
            T.tripFrom({ action = "browse_expansion_presets", args = {}, focused = true }, W)
            T.ok(S.holding["Return"], "handed over")
            if how == "key" then
              T.pageKey()
            else
              S.listed = { W, T.OTHER }
              T.show(T.OTHER)
            end
            T.settle(2)
            T.ok(not S.holding["Return"] and not S.menuTaken["Tab"] and not S.menuTaken["Shift+Tab"] and T.open() ~= true,
              how .. ": the keys back, Tab and Shift+Tab among them")
            T.ok(T.lastPost("state").state.preset == nil, how .. ": the preset posted as not read")
            T.ok(T.starts(T.lastPost("error").text, "Browse Presets: "), tostring(T.lastPost("error").text))
        "##;
        run_mac(&scenario(&s.replace("@HOW@", how)));
    }
}

/// Browse Presets with Avenger's window leaving the front between the act's check and the hand-over
/// (while the name's read is asked): handOver answers `not_in_front`, having given the trip up itself,
/// so the module ends nothing — the page hears the give-up's words, and no "the keyboard was not
/// handed over" follows on a trip already over.
#[test]
fn browse_presets_refused_for_the_front_leaves_the_words_to_the_give_up() {
    run_mac(&scenario(r##"
        local _, _, W = T.pageOn({ osc = 1, selected = "osc", index = 1 }, { preset = "LD Lead 1" })
        local recognize = T.host.ocr.recognize
        rawset(T.host.ocr, "recognize", function(r, opts, cb)
          if type(opts) == "table" and opts.key == "vps-avenger browse" then
            S.listed, S.front, S.chain = { W, T.OTHER }, T.OTHER, { T.OTHER }
          end
          return recognize(r, opts, cb)
        end)
        T.tripFrom({ action = "browse_expansion_presets", args = {}, focused = true }, W)
        T.settle(2)
        T.ok(not S.holding["Return"], "nothing handed over")
        T.ok(T.count("done after the trip ended") == 0, T.dump())
        T.ok(T.lastPost("error").text == "Browse Presets: another window came to the front; nothing more was done",
          tostring(T.lastPost("error").text))
    "##));
}

// ---------------------------------------------------------------------------------------------
// No key of the module's own; the pictures for calibration; Windows.
// ---------------------------------------------------------------------------------------------

/// The module sends, types and captures no key of its own while renaming or browsing: nothing is
/// sent, and every capture made from the message on is the runtime's — Return and Escape, Return
/// alone for the browser — and the two it swallows for the module's `swallow`, Tab and Shift+Tab,
/// taken in a menu.
#[test]
fn the_module_sends_types_and_captures_no_key_while_renaming() {
    run_mac(&scenario(r##"
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
        T.reads("LD Lead 1", "My Bass")
        local captured = {}
        local capture = T.host.keys.capture
        rawset(T.host.keys, "capture", function(spec, fn, opts)
          captured[#captured + 1] = spec .. ((opts and opts.menu == "taken") and " (taken)" or "")
          return capture(spec, fn, opts)
        end)
        local sent = #S.sent
        T.renameOpen({ action = "start_rename", args = {}, focused = true }, W, PLAIN, { 34, 7, 214, 22 })
        T.commit("Return", PLAIN)
        T.event()
        T.tripFrom({ action = "browse_expansion_presets", args = {}, focused = true }, W)
        T.typed("Return")
        T.tick()
        T.runDue()
        T.nameReads("LD Lead 2", 1200)
        T.ok(#S.sent == sent, "nothing sent: " .. table.concat(S.sent, ","))
        T.ok(table.concat(captured, ",") == "Return,Escape,Tab (taken),Shift+Tab (taken),Return,Tab (taken),Shift+Tab (taken)",
          table.concat(captured, ","))
        T.ok(S.front == T.PAGE_WIN and T.held() == 0, "back, nothing held")
    "##));
}

/// A calibrating run writes the editor's three pictures — before the double-click, open, and at the
/// end — once per kind of field and zoom in a session.
#[test]
fn a_calibrating_run_saves_the_editors_three_pictures_once_per_kind_and_zoom() {
    run_mac(&scenario(r##"
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        T.calibrate(true)
        local _, _, W = T.pageOn(PLAIN, { preset = "LD Lead 1" })
        T.reads("LD Lead 1", "My Bass")
        local function editorShots()
          local out = {}
          for _, m in ipairs(S.marked) do
            local name = string.match(m.path, "(editor%-[%w%-]+)%.png$")
            if name then out[#out + 1] = name end
          end
          return table.concat(out, ",")
        end
        for _ = 1, 2 do
          T.renameOpen({ action = "start_rename", args = {}, focused = true }, W, PLAIN, { 34, 7, 214, 22 })
          T.commit("Return", PLAIN)
          T.event()
        end
        T.ok(editorShots() == "editor-preset-before-80pct,editor-preset-open-80pct,editor-preset-end-80pct", editorShots())
        T.ok(T.count("[avenger] [calibrate] the editor's open picture of the preset field -> ") == 1, T.dump())
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}

/// On Windows, Avenger's own child window in REAPER's FX window at 150 % display scaling: the name
/// double-clicked through the factor 2.4, the scope on the FX window, and the name read back.
#[test]
fn rename_preset_in_reaper_on_windows() {
    run(&scenario(r##"
        local j = T.onWindows(80, 1.5)
        local header = T.avenger()
        T.answer(T.lastRead("vps-avenger header"), T.header(248, 60, 2.4, 80))
        T.event()
        T.ok(header.active, "identified")
        T.run(400)
        T.pageReady()
        local PLAIN = { osc = 1, selected = "osc", index = 1 }
        T.paintAvenger(248, 60, 2.4, PLAIN)
        T.words({ preset = "LD Lead 1" })
        T.openPage()
        T.event()
        -- The name's region (34, 7)-(214, 22) at 2.4 from (248, 60): (330, 77)-(762, 113).
        T.reads("LD Lead 1", "My Bass", nil, "330,77,762,113")
        S.focusAtOnce = false
        S.focusLands = function() S.front, S.chain = T.WFX, { j, T.WFX } end
        T.pageMsg({ action = "start_rename", args = {}, focused = true })
        T.land()
        T.runDue()
        T.tick()
        -- (123, 14) at 2.4: 248 + 295.2 -> 543, 60 + 33.6 -> 94.
        T.ok(T.clickAt() == "543,94", "the name: " .. T.clickAt())
        T.editor(PLAIN, { 34, 7, 214, 22 })
        T.tick()
        T.ok(S.holding["Return"] and S.scopedTo == T.WFX.id, "handed over, the scope on the FX window: " .. tostring(S.scopedTo))
        T.commit("Return", PLAIN)
        T.ok(T.lastPost("state").state.preset == "My Bass", tostring(T.lastPost("state").state.preset))
        T.ok(T.held() == 0, "every picture released: " .. T.held())
    "##));
}
