// VPS Avenger's page: the script of the avenger_control project's panel (its panel/pages.py),
// taken over with its developer's permission (NOTICE). It is the tool's, copied as it is — its
// renderers, value phrasing, disclosures, focus tracking and the Enter/Space handler, which are
// its VoiceOver tuning — and what the port changed is said where it changed:
//   * the transport: the module's messages through `platform` (docs/api/gui.md), where the tool
//     had WebKit's message handler and its Windows build an HTTP post; the page starts empty and
//     asks for its state ("ready"), and sends no action that is not on ACTIONS;
//   * a value that was not read is said "not read", where the tool showed a value it assumed —
//     a state comes whole, so a value missing from it was not read — and what the preset
//     database gave rather than the screen is said with ", from the database";
//   * the Windows-only block made the page's Settings section, on both systems: Correct open
//     Expansion and the module's settings, shown as the module posts them (applySettings);
//   * left out: the OSC rename dialog no control opened, and the focus retries on timers;
//   * the defects of the port's plan: the spelling, the disclosures a re-render closed, and the
//     central tabs this page cannot reach yet, which now say so;
//   * Back and Next say they are unavailable where VPS Avenger steps no preset (on Init Preset);
//   * DRUM SQ claims nothing at a click: no pattern SELECTED and no LENGTH before Avenger is read,
//     and its PASTE items offered from the module's word that a COPY was taken in Avenger's menu;
//   * the expansions listed are the database's roster, said so, one it has no presets for marked,
//     and the open expansion's heading says ", from the database" where only its place in the
//     database's list named it.
// Text goes into the page as text (textContent), never as HTML.
(() => {
  'use strict';

  const $ = (id) => document.getElementById(id);

  // Every action this page sends the module, by the tool's names, and the port's own: the
  // Settings section's two. perform() sends no other, and says so in the
  // module's log — a name not here is this page's own mistake. The module answers each one; a test
  // of the repository holds this list against the tool's and the module's handlers, and every name
  // this script and the markup send against this list.
  const ACTIONS = [
    'prev_preset', 'next_preset', 'start_rename', 'pick_load_preset', 'pick_save_preset_as',
    'pick_initialize_preset', 'cancel_initialize_preset', 'undo', 'redo_menu', 'set_zoom',
    'set_expansion_view', 'open_expansion', 'close_expansion', 'browse_expansion_presets',
    'select_all_expansion_presets', 'select_expansion_category', 'go_to_loaded_preset_category',
    'select_expansion_preset', 'load_expansion_preset', 'mark_expansion_preset_favorite',
    'toggle_expansion_favorite_view', 'select_osc_tab', 'add_osc', 'select_drums_tab',
    'select_trig_tab', 'toggle_osc_mute', 'toggle_drums_mute', 'toggle_trig_mute',
    'toggle_osc_solo', 'toggle_drums_solo', 'toggle_trig_solo', 'osc_menu_item', 'drums_menu_item',
    'trig_menu_item', 'rename_osc', 'select_macro_tab', 'add_macro_tab', 'delete_macro_tab',
    'rename_macro', 'macro_amount', 'reset_macro_amount', 'toggle_macro_button',
    'macro_reset_mode', 'macro_button_menu_item', 'osc_control', 'reset_osc_control',
    'osc_filter_control', 'toggle_drum_pattern', 'select_drum_mode', 'previous_drumkit',
    'next_drumkit', 'select_drumkit_part', 'drumkit_column', 'toggle_drumkit_button',
    'drumkit_control', 'reset_drumkit_control', 'drumkit_menu_item', 'select_central_tab',
    'sample_browser_focus', 'select_sample_editor_tab', 'sample_editor_field',
    'sample_editor_play_mode', 'sample_editor_panel_button', 'toggle_sample_editor_enable',
    'sample_editor_grid', 'sample_editor_eq_type', 'sample_editor_loop_stretch',
    'sample_editor_control', 'reset_sample_editor_control', 'central_drum_sq_sequence_menu_item',
    'central_drum_sq_pattern_player', 'central_drum_sq_auto_follow',
    'select_central_drum_sq_pattern', 'toggle_central_drum_sq_pattern',
    'central_drum_sq_pattern_menu_item', 'central_drum_sq_control',
    'reset_central_drum_sq_control', 'central_drum_sq_length', 'central_drum_sq_speed',
    'align_open_expansion', 'set_setting', 'reset_settings',
    'confirm_osc_count', 'confirm_expansion_view'
  ];
  // To the module; nothing when the page is opened in a browser, to be looked at.
  const post = (value) => {
    if (window.platform) window.platform.post(value);
  };
  // What goes wrong in the page, for the module's log: a page has no console to look in. From
  // here on only — a script that fails to load or to parse reports nothing, this one included.
  const problem = (text) => post({ kind: 'problem', text: String(text) });
  window.addEventListener('error', (event) => {
    problem(`${event.message} (${event.filename}:${event.lineno})`);
  });
  document.addEventListener('securitypolicyviolation', (event) => {
    problem(`the policy refused ${event.blockedURI || 'inline code'} (${event.effectiveDirective})`);
  });
  // A value as it came: null when it was not read. A flag is never false for "not read".
  const readFlag = (v) => (v == null ? null : !!v);
  const readInt = (v) => {
    const n = parseInt(v, 10);
    return Number.isFinite(n) ? n : null;
  };
  const readText = (v) => (v == null ? null : String(v));
  // The twelve drum slots' list as the module posts it: a JSON list, or an object keyed by each
  // slot's number ("12") where few slots were read and slot 11 or 12 among them, which a JSON list
  // cannot carry; each entry through `read`, a hole as null. Neither, or an object with no slot (an
  // empty list, which JSON writes as {}): null.
  const readSlots = (v, read) => {
    if (Array.isArray(v)) return v.map(read);
    if (!v || typeof v !== 'object') return null;
    const out = [];
    Object.keys(v).forEach((key) => {
      const i = parseInt(key, 10);
      if (String(i) === key && i >= 1 && i <= 12) out[i - 1] = v[key];
    });
    return out.length ? Array.from(out, (x) => read(x == null ? null : x)) : null;
  };
  const ALL_PRESET_CATEGORY = '__all__';
  let currentExpansions = [];
  // The list is a roster of the preset database's.
  let currentExpansionsFromDatabase = false;
  // A roster's expansion the database has no presets for (the tool keeps it in its list).
  let currentExpansionsWithoutData = [];
  // The port's: the open expansion named by its place in the database's list, its name not read
  // in Avenger's header.
  let currentSelectedFromDatabase = false;
  // An open expansion's categories and presets are the preset database's lists.
  let currentExpansionListsFromDatabase = false;
  let currentExpansionView = null;
  let currentZoom = null;
  let currentExpansionOpen = null;
  let expansionSearchOpen = true;
  let expansionSearchQuery = '';
  let currentExpansionCategories = [];
  let currentExpansionPresets = [];
  let currentExpansionPresetCategories = [];
  let currentSelectedExpansion = '';
  let selectedPresetCategory = '';
  let loadedExpansionPresetIndex = -1;
  let presetSearchQuery = '';
  let expansionFavoriteView = null;
  let favoritePresetStates = {};
  // Nothing is known before the module's first state: every value is "not read" until then.
  let oscPanel = {};
  let macroPanel = {};
  let contentPanelOpen = true;
  let oscillatorPanelOpen = false;
  let centralTabsOpen = false;
  // The central tab read, or the one the user chose on this page since; null: not read.
  let centralPanelTab = null;
  let centralDrumSqPattern = null;
  let centralDrumSqAutoFollow = null;
  let centralDrumSqPatternChain = null;
  let centralDrumSqPatternCopied = false;
  let centralDrumSqPatternStates = { b: null, c: null, d: null };
  let centralDrumSqLengthSteps = null;
  // LENGTH is never read: the length shown is the one the page's LENGTH set, marked so.
  let centralDrumSqLengthSet = false;
  let macroPanelOpen = true;
  let openOscMenuId = '';
  let openMacroMenuId = '';
  let openDrumkitMenuId = '';
  let openDrumkitSubmenuId = '';
  // The other disclosures a re-render builds again — Play Mode, Grid, Type, Loop-Stretch and
  // DRUM SQ's — by their items' ids, open as the user left them (the tool's closed them).
  const openDisclosures = new Set();
  const zoomMenuOptions = [
    50, 55, 60, 65, 70, 75, 80, 85, 90, 95, 100,
    110, 120, 130, 140, 150, 160, 170, 180, 190, 200
  ];
  const expansionViewOptions = [
    'List', '1 per Row', '2 per Row', '3 per Row', '4 per Row',
    '5 per Row', '6 per Row', '7 per Row', '8 per Row'
  ];
  let pendingCategoryFocus = '';
  const unknownOpenExpansionText = (
    'An expansion is open. Please press the Close Expansion button to close it, then open a new expansion.'
  );
  // Whether the view read in Avenger stands in the search's way: a view not read does not.
  const searchNeedsView = () => currentExpansionView != null && currentExpansionView !== '8 per Row';

  // A disclosure the page builds again stays as the user left it.
  function keepDisclosure(button, menu) {
    const open = openDisclosures.has(menu.id);
    menu.hidden = !open;
    button.setAttribute('aria-expanded', open ? 'true' : 'false');
  }

  function renderExpansionSearch() {
    const results = $('expansion-search-results');
    if (!results) return;
    updateExpansionSearchResetButton();
    results.textContent = '';
    if (searchNeedsView()) {
      const message = document.createElement('p');
      message.id = 'expansion-view-required-message';
      message.textContent = (
        'Expansion Search requires 8 per Row. If VPS Avenger is not set ' +
        'to 8 per Row, use the Expansion View menu.'
      );
      results.removeAttribute('aria-label');
      results.appendChild(message);
      return;
    }
    const q = expansionSearchQuery.trim().toLowerCase();
    const shown = currentExpansions
      .map((name, index) => ({ name, index }))
      .filter((item) => !q || item.name.toLowerCase().includes(q));
    results.setAttribute(
      'aria-label',
      `Expansions, ${shown.length} result${shown.length === 1 ? '' : 's'}` +
        (currentExpansionsFromDatabase ? ', from the database' : '')
    );
    shown.forEach((item) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.id = `expansion-search-button-${item.index}`;
      btn.textContent = item.name;
      if (currentExpansionsWithoutData[item.index] === true) {
        btn.setAttribute('aria-label', `${item.name}, no presets in the database`);
      }
      btn.dataset.action = 'open_expansion';
      btn.dataset.index = String(item.index);
      btn.addEventListener('click', () => {
        const largeRoster = currentExpansions.length >= 129;
        const pageStart = largeRoster
          ? (item.index < 64 ? 0 : (item.index < 96 ? 64 : 96))
          : Math.floor(item.index / 64) * 64;
        const visibleStart = largeRoster
          ? (item.index < 64 ? 0 : (item.index < 96 ? 32 : 72))
          : pageStart;
        trackFocus(btn);
        perform('open_expansion', {
          index: item.index,
          page_start: pageStart,
          visible_index: item.index - visibleStart
        });
      });
      results.appendChild(btn);
    });
  }

  function updateExpansionSearchVisibility() {
    const viewTrigger = $('expansion-view-menu-trigger');
    const searchInput = $('expansion-search-input');
    const resetButton = $('reset-expansion-search-button');
    const searchSection = $('expansion-search-section');
    expansionSearchOpen = true;
    if (viewTrigger) viewTrigger.hidden = false;
    if (searchInput) {
      searchInput.hidden = (
        currentExpansionOpen || searchNeedsView()
      );
    }
    updateExpansionSearchResetButton();
    if (resetButton) {
      resetButton.hidden = (
        currentExpansionOpen ||
        searchNeedsView() ||
        !expansionSearchQuery.trim()
      );
    }
    if (searchSection) {
      searchSection.hidden = currentExpansionOpen;
    }
    renderExpansionSearch();
  }

  function updateExpansionSearchResetButton() {
    const resetButton = $('reset-expansion-search-button');
    if (!resetButton) return;
    resetButton.hidden = (
      currentExpansionOpen ||
      searchNeedsView() ||
      !expansionSearchQuery.trim()
    );
  }

  function renderExpansions() {
    updateExpansionSearchVisibility();
  }

  // The port's: Back and Next where VPS Avenger steps no preset — on Init Preset, its blank
  // preset, from which the tool clicked and waited for a change that never came. The module says
  // why in the state (`preset_steps_unavailable`, from the name read); the buttons are then
  // aria-disabled, with the reason in their help, the title, which is also their description.
  // Never `disabled`: they stay in the Tab order and focusable, and a press still goes to the
  // module, which sends nothing to Avenger and answers in the error region.
  const presetSteps = ['prev-preset-button', 'next-preset-button']
    .map((id) => $(id))
    .filter(Boolean)
    .map((button) => ({ button, help: button.title }));
  function updatePresetSteps(reason) {
    presetSteps.forEach(({ button, help }) => {
      button.setAttribute('aria-disabled', reason ? 'true' : 'false');
      button.title = reason ? `${help}, unavailable: ${reason}.` : help;
    });
  }

  // --- module → page: the state, whole ---
  function applyState(s) {
    if (!s) return;
    const age = $('state-age');
    if (age) {
      // Answered from earlier reads (the page reloaded itself): when they were made.
      const seconds = readInt(s.read_age_seconds);
      age.hidden = seconds == null;
      age.textContent = seconds == null ? '' : (
        `Read ${seconds < 120 ? `${seconds} seconds` : `${Math.round(seconds / 60)} minutes`} ago: ` +
        'this page shows what was read then.'
      );
    }
    if (s.preset == null) {
      if ($('preset-name').textContent !== 'Preset name, not read') {
        $('preset-name').textContent = 'Preset name, not read';
      }
    } else {
      // Render the preset name AND the loaded expansion in the H2 so
      // VO announces both on every change (aria-live + atomic on the
      // heading). The tool's format:
      //     "LD Focus Square, (XP: Acid Breaks)"
      // No expansion known (Init Preset, custom preset, just renamed,
      // or anchor was cleared by Menu/Undo/Redo) → just the bare name,
      // no trailing parenthetical. We use textContent so any commas
      // or parens in the name itself are rendered literally — no HTML
      // interpretation, no markup injection.
      const exp = (s.expansion || '').trim();
      // The expansion is the preset database's, never read off Avenger: the module says so.
      const mark = s.expansion_from_database ? ', from the database' : '';
      const text = exp ? (s.preset + ', (XP: ' + exp + mark + ')') : s.preset;
      if (text !== $('preset-name').textContent) {
        $('preset-name').textContent = text;
      }
    }
    updatePresetSteps(readText(s.preset_steps_unavailable));
    currentZoom = readInt(s.zoom);
    const zoomTrigger = $('zoom-menu-trigger');
    if (zoomTrigger) {
      zoomTrigger.textContent = currentZoom == null ? 'Size, not read' : `Size ${String(currentZoom)} %`;
      zoomTrigger.setAttribute('aria-label', currentZoom == null ? 'Size, not read' : `Size ${String(currentZoom)}%`);
    }
    updateButtonMenuSelection('zoom-menu-items', currentZoom);
    // The state comes whole: a list or a value missing from it was not read, and nothing of an
    // earlier state is kept in its place.
    currentExpansions = Array.isArray(s.expansions) ? s.expansions : [];
    currentExpansionsFromDatabase = Array.isArray(s.expansions) && !!s.expansions_from_database;
    currentExpansionsWithoutData = Array.isArray(s.expansions_without_data)
      ? s.expansions_without_data.map((v) => v === true)
      : [];
    currentExpansionView = readText(s.expansion_view);
    // A view not read — VPS Avenger draws no label for it — says how it came to be: set from this
    // page, confirmed here, or as stored.
    const viewFrom = readText(s.expansion_view_from);
    const viewHow = viewFrom === 'set' ? ', set' : (viewFrom === 'confirmed' ? ', confirmed' : (viewFrom ? ', as stored' : ''));
    const viewTrigger = $('expansion-view-menu-trigger');
    if (viewTrigger) {
      const words = currentExpansionView == null ? 'Expansion View, not read' : `Expansion View ${currentExpansionView}${viewHow}`;
      viewTrigger.textContent = words;
      viewTrigger.setAttribute('aria-label', words);
    }
    updateButtonMenuSelection('expansion-view-menu-items', currentExpansionView);
    updateButtonMenuSelection('settings-expansion-view-items', currentExpansionView);
    renderExpansions();
    {
      // Not read: shown as the expansions' list is, the view saying it was not read.
      currentExpansionOpen = readFlag(s.expansion_open);
      const open = currentExpansionOpen === true;
      const viewRow = $('expansion-view-row');
      const openSection = $('open-expansion-section');
      if (viewRow) viewRow.hidden = open;
      if (openSection) openSection.hidden = !open;
      updateExpansionSearchVisibility();
    }
    {
      const selected = String(s.selected_expansion || '').trim();
      if (selected !== currentSelectedExpansion) {
        selectedPresetCategory = currentExpansionCategories[0] || '';
        loadedExpansionPresetIndex = -1;
        presetSearchQuery = '';
        const presetSearch = $('preset-search-input');
        if (presetSearch) presetSearch.value = '';
        currentSelectedExpansion = selected;
      }
      currentSelectedFromDatabase = !!selected && s.selected_expansion_from_database === true;
      const name = $('open-expansion-name');
      if (name) {
        name.textContent = (
          selected
            ? (currentSelectedFromDatabase ? `${selected}, from the database` : selected)
            : (currentExpansionOpen ? unknownOpenExpansionText : '')
        );
      }
    }
    if (s.reset_browser_searches) {
      expansionSearchQuery = '';
      presetSearchQuery = '';
      const expansionSearch = $('expansion-search-input');
      const presetSearch = $('preset-search-input');
      if (expansionSearch) expansionSearch.value = '';
      if (presetSearch) presetSearch.value = '';
      renderExpansionSearch();
      renderOpenExpansionDetails();
    }
    {
      currentExpansionCategories = Array.isArray(s.expansion_categories) ? s.expansion_categories : [];
      currentExpansionListsFromDatabase = Array.isArray(s.expansion_categories) && !!s.expansion_lists_from_database;
      if (
        currentExpansionCategories.length &&
        selectedPresetCategory !== ALL_PRESET_CATEGORY &&
        !currentExpansionCategories.includes(selectedPresetCategory)
      ) {
        selectedPresetCategory = currentExpansionCategories[0];
      }
      renderOpenExpansionDetails();
    }
    if (s.selected_preset_category != null) {
      const category = String(s.selected_preset_category || '').trim();
      if (
        category === ALL_PRESET_CATEGORY ||
        (category && currentExpansionCategories.includes(category))
      ) {
        selectedPresetCategory = category;
      }
      renderOpenExpansionDetails();
    }
    currentExpansionPresets = Array.isArray(s.expansion_presets) ? s.expansion_presets : [];
    currentExpansionPresetCategories = Array.isArray(s.expansion_preset_categories)
      ? s.expansion_preset_categories
      : [];
    {
      const idx = readInt(s.loaded_expansion_preset_index);
      loadedExpansionPresetIndex = idx == null ? -1 : idx;
    }
    expansionFavoriteView = readFlag(s.expansion_favorite_view);
    favoritePresetStates = {};
    if (s.favorite_preset_states && typeof s.favorite_preset_states === 'object') {
      Object.keys(s.favorite_preset_states).forEach((key) => {
        favoritePresetStates[String(key)] = !!s.favorite_preset_states[key];
      });
    }
    renderOpenExpansionDetails();
    // A panel missing from the state was not read: it is taken as one with nothing read in it.
    if (!(s.osc_panel && typeof s.osc_panel === 'object')) s.osc_panel = {};
    if (!(s.macro_panel && typeof s.macro_panel === 'object')) s.macro_panel = {};
    if (s.osc_panel && typeof s.osc_panel === 'object') {
      const oscCount = readInt(s.osc_panel.count);
      oscPanel = {
        count: oscCount,
        names: Array.isArray(s.osc_panel.names) ? s.osc_panel.names : [],
        names_from_database: Array.isArray(s.osc_panel.names_from_database)
          ? s.osc_panel.names_from_database.map((v) => v === true)
          : [],
        // Where a count the dots did not give came from: 'confirmed' or 'database'.
        count_from: s.osc_panel.count_from ? String(s.osc_panel.count_from) : null,
        copied: s.osc_panel.copied ? String(s.osc_panel.copied) : '',
        selected_type: s.osc_panel.selected_type ? String(s.osc_panel.selected_type) : null,
        selected_index: readInt(s.osc_panel.selected_index),
        selected_label: s.osc_panel.selected_label ? String(s.osc_panel.selected_label) : '',
        mute_states: Array.isArray(s.osc_panel.mute_states) ? s.osc_panel.mute_states.map(readFlag) : [],
        solo_states: Array.isArray(s.osc_panel.solo_states) ? s.osc_panel.solo_states.map(readFlag) : [],
        volume_values: Array.isArray(s.osc_panel.volume_values) ? s.osc_panel.volume_values : [],
        pan_values: Array.isArray(s.osc_panel.pan_values) ? s.osc_panel.pan_values : [],
        transpose_values: Array.isArray(s.osc_panel.transpose_values) ? s.osc_panel.transpose_values : [],
        fine_tune_values: Array.isArray(s.osc_panel.fine_tune_values) ? s.osc_panel.fine_tune_values : [],
        random_pitch_values: Array.isArray(s.osc_panel.random_pitch_values) ? s.osc_panel.random_pitch_values : [],
        drums_mute: readFlag(s.osc_panel.drums_mute),
        drums_solo: readFlag(s.osc_panel.drums_solo),
        drum_pattern: readFlag(s.osc_panel.drum_pattern),
        drum_lock: readFlag(s.osc_panel.drum_lock),
        drum_mode: s.osc_panel.drum_mode ? String(s.osc_panel.drum_mode) : null,
        drumkit_part_selected: s.osc_panel.drumkit_part_selected ? String(s.osc_panel.drumkit_part_selected) : '',
        // The names read, holes where one was not: never the factory names in their place.
        drumkit_part_labels: readSlots(s.osc_panel.drumkit_part_labels, (label) => String(label || '')) || [],
        drumkit_mute_states: readSlots(s.osc_panel.drumkit_mute_states, readFlag) || [],
        drumkit_solo_states: readSlots(s.osc_panel.drumkit_solo_states, readFlag) || [],
        drumkit_lock_states: readSlots(s.osc_panel.drumkit_lock_states, readFlag) || [],
        drumkit_loop_states: readSlots(s.osc_panel.drumkit_loop_states, readFlag) || [],
        sample_editor_tab: s.osc_panel.sample_editor_tab ? String(s.osc_panel.sample_editor_tab) : null,
        sample_editor_view: s.osc_panel.sample_editor_view ? String(s.osc_panel.sample_editor_view) : 'editor',
        sample_editor_amp_states: readSlots(s.osc_panel.sample_editor_amp_states, readFlag) || [],
        sample_editor_pitch_states: readSlots(s.osc_panel.sample_editor_pitch_states, readFlag) || [],
        sample_editor_eq_states: readSlots(s.osc_panel.sample_editor_eq_states, readFlag) || [],
        central_panel_tab: s.osc_panel.central_panel_tab ? String(s.osc_panel.central_panel_tab) : null,
        drum_sq_auto_follow: readFlag(s.osc_panel.drum_sq_auto_follow),
        drum_sq_pattern_chain: readFlag(s.osc_panel.drum_sq_pattern_chain),
        drum_sq_pattern_selected: s.osc_panel.drum_sq_pattern_selected ? String(s.osc_panel.drum_sq_pattern_selected) : null,
        drum_sq_pattern_states: Array.isArray(s.osc_panel.drum_sq_pattern_states) ? s.osc_panel.drum_sq_pattern_states.map(readFlag).slice(0, 3) : [],
        drum_sq_length_steps: readInt(s.osc_panel.drum_sq_length_steps),
        drum_sq_length_set: s.osc_panel.drum_sq_length_set === true,
        drum_sq_pattern_copied: readFlag(s.osc_panel.drum_sq_pattern_copied),
        trig_mute: readFlag(s.osc_panel.trig_mute),
        trig_solo: readFlag(s.osc_panel.trig_solo),
        // The tabs whose Solo reads on: every other tab's Mute is said as silenced by them.
        solo_by: Array.isArray(s.osc_panel.solo_by) ? s.osc_panel.solo_by.map(String) : []
      };
      // The tab read, or none: it is read only with the Drums tab selected.
      centralPanelTab = ['editor', 'arp', 'drum_sq', 'step_sq'].includes(oscPanel.central_panel_tab)
        ? oscPanel.central_panel_tab
        : null;
      centralDrumSqAutoFollow = oscPanel.drum_sq_auto_follow;
      centralDrumSqPatternChain = oscPanel.drum_sq_pattern_chain;
      centralDrumSqPattern = ['a', 'b', 'c', 'd'].includes(oscPanel.drum_sq_pattern_selected)
        ? oscPanel.drum_sq_pattern_selected
        : null;
      const drumSqPatternStates = Array.isArray(oscPanel.drum_sq_pattern_states)
        ? oscPanel.drum_sq_pattern_states
        : [];
      centralDrumSqPatternStates = {
        b: readFlag(drumSqPatternStates[0]),
        c: readFlag(drumSqPatternStates[1]),
        d: readFlag(drumSqPatternStates[2])
      };
      centralDrumSqLengthSet = oscPanel.drum_sq_length_set;
      setCentralDrumSqLengthSteps(oscPanel.drum_sq_length_steps);
      // PASTE follows the module's word that a COPY of this session was taken in Avenger's menu,
      // never the page's own click.
      centralDrumSqPatternCopied = oscPanel.drum_sq_pattern_copied === true;
      renderOscPanel();
    }
    if (s.macro_panel && typeof s.macro_panel === 'object') {
      const readCount = readInt(s.macro_panel.count);
      const macroCount = readCount == null ? null : Math.max(1, Math.min(4, readCount));
      const selectedMacro = readInt(s.macro_panel.selected_index);
      macroPanel = {
        count: macroCount,
        tabs: Array.isArray(s.macro_panel.tabs)
          ? s.macro_panel.tabs.map((tab) => Array.isArray(tab) ? tab.map(readText) : [])
          : [],
        selected_index: selectedMacro == null ? null : Math.max(1, Math.min(4, selectedMacro)),
        selected_labels: Array.isArray(s.macro_panel.selected_labels)
          ? s.macro_panel.selected_labels.map(readText)
          : [],
        labels_from_database: Array.isArray(s.macro_panel.labels_from_database)
          ? s.macro_panel.labels_from_database.map((v) => v === true)
          : [],
        button_states: Array.isArray(s.macro_panel.button_states)
          ? s.macro_panel.button_states.map(readFlag)
          : [],
        amount_values: Array.isArray(s.macro_panel.amount_values)
          ? s.macro_panel.amount_values.map(readInt)
          : []
      };
      renderMacroPanel();
    }
    // (redo_list intentionally ignored — the Redo button now opens
    // Avenger's own native dropdown, which is accessible. We don't
    // mirror the entries inside the panel anymore.)
  }

  // `focus`: the error came while the page was away, and the page comes back on it.
  function showError(msg, focus) {
    $('error-text').textContent = msg || '';
    $('error-section').hidden = !msg;
    if (msg && focus) {
      trackFocus($('error-text'));
      focusTarget($('error-text'));
    }
  }

  function renderOpenExpansionDetails() {
    // By the expansion the module named, not by the heading's text, which may carry a mark.
    const hasKnownExpansion = currentSelectedExpansion !== '';
    const categories = $('open-expansion-categories');
    const fromDatabase = currentExpansionListsFromDatabase ? ', from the database' : '';
    const caption = $('open-expansion-presets-table') && $('open-expansion-presets-table').caption;
    if (caption) caption.textContent = `Presets${fromDatabase}`;
    if (categories) {
      categories.setAttribute('aria-label', `Categories${fromDatabase}`);
      categories.hidden = !hasKnownExpansion;
      categories.textContent = '';
      if (hasKnownExpansion) {
        const allBtn = document.createElement('button');
        allBtn.type = 'button';
        allBtn.textContent = 'All';
        allBtn.id = 'expansion-category-all';
        allBtn.title = 'Show all presets in this expansion';
        allBtn.setAttribute(
          'aria-pressed',
          selectedPresetCategory === ALL_PRESET_CATEGORY ? 'true' : 'false'
        );
        allBtn.addEventListener('click', () => {
          selectedPresetCategory = ALL_PRESET_CATEGORY;
          presetSearchQuery = '';
          const presetSearch = $('preset-search-input');
          if (presetSearch) presetSearch.value = '';
          pendingCategoryFocus = ALL_PRESET_CATEGORY;
          renderOpenExpansionDetails();
          perform('select_all_expansion_presets');
        });
        categories.appendChild(allBtn);
      }
      if (hasKnownExpansion) currentExpansionCategories.forEach((name, index) => {
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.textContent = name;
        btn.id = `expansion-category-${index}`;
        btn.title = `Select category ${name}`;
        btn.setAttribute(
          'aria-pressed',
          selectedPresetCategory === name ? 'true' : 'false'
        );
        btn.addEventListener('click', () => {
          perform('select_expansion_category', {
            index: index,
            total: currentExpansionCategories.length
          });
          selectedPresetCategory = name;
          presetSearchQuery = '';
          const presetSearch = $('preset-search-input');
          if (presetSearch) presetSearch.value = '';
          pendingCategoryFocus = name;
          renderOpenExpansionDetails();
        });
        categories.appendChild(btn);
      });
      if (pendingCategoryFocus) {
        const focusName = pendingCategoryFocus;
        pendingCategoryFocus = '';
        requestAnimationFrame(() => {
          const buttons = Array.from(categories.querySelectorAll('button'));
          const target = focusName === ALL_PRESET_CATEGORY
            ? $('expansion-category-all')
            : buttons.find((button) => button.textContent === focusName);
          if (target) {
            trackFocus(target);
            focusTarget(target, false);
          }
        });
      }
    }
    const table = $('open-expansion-presets-table');
    const presetSearch = $('preset-search-input');
    const resetPresetSearch = $('reset-preset-search-button');
    const goToLoadedCategory = $('go-to-loaded-preset-category-button');
    const favoriteToggle = $('favorite-view-toggle');
    if (favoriteToggle && expansionFavoriteView == null) {
      favoriteToggle.removeAttribute('aria-pressed');
      favoriteToggle.textContent = 'FAV, not read';
      favoriteToggle.setAttribute('aria-label', 'FAV view, not read');
    } else if (favoriteToggle) {
      favoriteToggle.setAttribute(
        'aria-pressed',
        expansionFavoriteView ? 'true' : 'false'
      );
      favoriteToggle.textContent = expansionFavoriteView ? 'FAV ON' : 'FAV OFF';
      favoriteToggle.setAttribute(
        'aria-label',
        expansionFavoriteView ? 'FAV view on' : 'FAV view off'
      );
    }
    if (presetSearch) {
      presetSearch.hidden = !hasKnownExpansion || currentExpansionPresets.length === 0;
    }
    if (resetPresetSearch) {
      resetPresetSearch.hidden = (
        !hasKnownExpansion ||
        currentExpansionPresets.length === 0 ||
        !presetSearchQuery.trim()
      );
    }
    if (goToLoadedCategory) {
      goToLoadedCategory.hidden = (
        !hasKnownExpansion ||
        selectedPresetCategory !== ALL_PRESET_CATEGORY ||
        loadedExpansionPresetIndex < 0
      );
    }
    if (table) {
      table.hidden = !hasKnownExpansion || currentExpansionPresets.length === 0;
    }
    const body = $('open-expansion-presets');
    if (body) {
      body.textContent = '';
      let visiblePresetRow = 0;
      const showingAllPresets = selectedPresetCategory === ALL_PRESET_CATEGORY;
      const categoryCounts = {};
      currentExpansionPresetCategories.forEach((category) => {
        if (!category) return;
        categoryCounts[category] = (categoryCounts[category] || 0) + 1;
      });
      const categorySeenCounts = {};
      const selectedCategoryPresetCount = currentExpansionPresetCategories
        .filter((category) => category === selectedPresetCategory).length;
      const q = presetSearchQuery.trim().toLowerCase();
      let categoryPresetIndex = 0;
      currentExpansionPresets.forEach((name, index) => {
        const category = currentExpansionPresetCategories[index] || '';
        const realCategoryPresetIndex = categorySeenCounts[category] || 0;
        if (category) {
          categorySeenCounts[category] = realCategoryPresetIndex + 1;
        }
        if (!showingAllPresets && category !== selectedPresetCategory) {
          return;
        }
        const row = showingAllPresets ? realCategoryPresetIndex : categoryPresetIndex;
        if (!showingAllPresets) categoryPresetIndex += 1;
        if (q && !name.toLowerCase().includes(q)) {
          return;
        }
        const realCategoryIndex = currentExpansionCategories.indexOf(category);
        const realCategoryCount = categoryCounts[category] || selectedCategoryPresetCount;
        const keepPresetCategory = showingAllPresets ? ALL_PRESET_CATEGORY : selectedPresetCategory;
        visiblePresetRow += 1;
        const tr = document.createElement('tr');
        const nameCell = document.createElement('td');
        const loadCell = document.createElement('td');
        const favoriteCell = document.createElement('td');
        const nameButton = document.createElement('button');
        const loadButton = document.createElement('button');
        const favoriteButton = document.createElement('button');
        const isLoadedPreset = index === loadedExpansionPresetIndex;
        nameButton.type = 'button';
        nameButton.textContent = isLoadedPreset ? `${name} SELECTED` : name;
        nameButton.id = `expansion-preset-${index}`;
        nameButton.setAttribute('aria-pressed', isLoadedPreset ? 'true' : 'false');
        nameButton.setAttribute(
          'aria-label',
          isLoadedPreset ? `${name}, SELECTED` : name
        );
        nameButton.addEventListener('click', () => {
          trackFocus(nameButton);
          perform('select_expansion_preset', {
            visible_index: row,
            preset_index: index,
            category_preset_index: row,
            category_count: realCategoryCount,
            category_index: realCategoryIndex,
            category_total: currentExpansionCategories.length,
            keep_preset_category: keepPresetCategory
          });
        });
        loadButton.type = 'button';
        loadButton.textContent = 'Load';
        loadButton.id = `load-expansion-preset-${index}`;
        loadButton.addEventListener('click', () => {
          trackFocus(loadButton);
          perform('load_expansion_preset', {
            visible_index: row,
            preset_index: index,
            category_preset_index: row,
            category_count: realCategoryCount,
            category_index: realCategoryIndex,
            category_total: currentExpansionCategories.length,
            keep_preset_category: keepPresetCategory
          });
        });
        if (expansionFavoriteView) {
          const favState = favoritePresetStates[String(index)];
          favoriteButton.type = 'button';
          favoriteButton.textContent = favState ? 'Favorite' : 'Mark as favorite';
          favoriteButton.id = `favorite-expansion-preset-${index}`;
          if (favState != null) {
            favoriteButton.setAttribute(
              'aria-pressed',
              favState ? 'true' : 'false'
            );
          }
          favoriteButton.addEventListener('click', () => {
            trackFocus(favoriteButton);
            perform('mark_expansion_preset_favorite', {
              visible_index: row,
              preset_index: index,
              category_preset_index: row,
              category_count: realCategoryCount,
              category_index: realCategoryIndex,
              category_total: currentExpansionCategories.length,
              keep_preset_category: keepPresetCategory
            });
          });
          favoriteCell.appendChild(favoriteButton);
        }
        nameCell.appendChild(nameButton);
        loadCell.appendChild(loadButton);
        tr.appendChild(nameCell);
        tr.appendChild(loadCell);
        if (expansionFavoriteView) {
          tr.appendChild(favoriteCell);
        }
        body.appendChild(tr);
      });
    }
  }

  function oscColumns() {
    const known = oscPanel.count != null;
    const count = known ? Math.max(1, Math.min(8, parseInt(oscPanel.count, 10) || 1)) : 0;
    const names = Array.isArray(oscPanel.names) ? oscPanel.names.slice(0, count) : [];
    while (names.length < count) names.push(null);
    const fromDatabase = Array.isArray(oscPanel.names_from_database) ? oscPanel.names_from_database : [];
    const cols = names.map((name, i) => ({
      type: 'osc',
      index: i + 1,
      label: String(name || `OSC ${i + 1}`),
      // The tab's own words: a name not read says so, and one the database's spelling replaced
      // is marked.
      heading: name == null
        ? `OSC ${i + 1}, name not read`
        : `${String(name)}${fromDatabase[i] === true ? ', from the database' : ''}`
    }));
    if (!known) cols.push({ type: 'unread', label: 'OSC tabs, not read' });
    // A count the dots did not give is said as where it came from, never as read.
    if (known && oscPanel.count_from) {
      cols.push({
        type: 'unread',
        label: `${count} OSC tab${count === 1 ? '' : 's'}, ${oscPanel.count_from === 'confirmed' ? 'as confirmed' : 'from the database'}`
      });
    }
    if (known && count < 8) cols.push({ type: 'add', label: 'Add OSC' });
    cols.push({ type: 'drums', label: 'Drums' });
    cols.push({ type: 'trig', label: 'Trig' });
    return cols;
  }

  function macroColumns() {
    if (macroPanel.count == null) return [{ type: 'unread', label: 'Macro tabs, not read' }];
    const count = Math.max(1, Math.min(4, parseInt(macroPanel.count, 10) || 1));
    const cols = [];
    for (let i = 1; i <= count; i += 1) {
      cols.push({ type: 'macro', index: i, label: `Macro ${i}` });
    }
    if (count < 4) cols.push({ type: 'add', label: 'Add Macro Tab' });
    if (count > 1) cols.push({ type: 'delete-last', label: 'Delete Last Tab' });
    return cols;
  }

  function makeCellButton(label, action, args, id) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.textContent = label;
    if (id) btn.id = id;
    btn.addEventListener('click', () => {
      trackFocus(btn);
      perform(action, args || {});
    });
    return btn;
  }

  function isSelectedMacroCol(col) {
    return col.type === 'macro'
      && macroPanel.selected_index != null
      && parseInt(macroPanel.selected_index, 10) === col.index;
  }

  function macroSlotName(tabIndex, slot) {
    if (slot <= 3) return `Macro ${(tabIndex - 1) * 3 + slot}`;
    return `MacroBtn ${(tabIndex - 1) * 2 + slot - 3}`;
  }

  function displayMacroLabel(label, tabIndex, slot) {
    const slotName = macroSlotName(tabIndex, slot);
    if (label == null) return `${slotName}, not read`;
    const raw = String(label || '').trim();
    if (!raw || /^-+$/.test(raw) || /\bMacro(?:Btn)?\b/i.test(raw)) {
      return `${slotName}: Unassigned`;
    }
    return raw;
  }

  // The five slots' words: `label` for the slot itself, ", not read" where its name was not read,
  // and `name` for its controls' words, the slot's plain name there ("MacroBtn 1 ON").
  function macroTabLabels(index) {
    const tabIndex = Math.max(1, Math.min(4, parseInt(index, 10) || 1));
    const tabs = Array.isArray(macroPanel.tabs) ? macroPanel.tabs : [];
    const labels = Array.isArray(tabs[tabIndex - 1]) ? tabs[tabIndex - 1].slice(0, 5) : [];
    while (labels.length < 5) labels.push(null);
    return labels.map((label, i) => ({
      label: displayMacroLabel(label, tabIndex, i + 1),
      name: label == null ? macroSlotName(tabIndex, i + 1) : displayMacroLabel(label, tabIndex, i + 1)
    }));
  }

  const macroResetModeEntries = [
    {
      label: 'Normal Mode',
      mode: 'normal',
      help: 'If you Reset this Macro, the slider will be set to 0.'
    },
    {
      label: 'Snap At Center',
      mode: 'snap',
      help: 'If you Reset this Macro, the slider will be snapped at 50%.'
    }
  ];

  const macroButtonMenuEntries = [
    {
      label: 'Macro Button Special Switch',
      item: 'special',
      help: 'If this is on, you can set only one Button active.'
    },
    {
      label: 'Trigger Mode',
      item: 'trigger',
      help: 'Set whether the Button toggles on and off by Click or on by Hold and off by Release.'
    }
  ];

  function macroControlTitle(controlLabel, macroLabel) {
    if (controlLabel === 'Rename') return `Rename ${macroLabel}`;
    if (controlLabel === 'Menu') return `Show reset mode options for ${macroLabel}`;
    if (controlLabel === 'Amount') return `Adjust the amount of ${macroLabel}`;
    if (controlLabel === 'Reset Amount') return `Reset the amount of ${macroLabel}`;
    if (controlLabel === 'On/Off') return `Turn ${macroLabel} on or off`;
    return controlLabel;
  }

  function macroControlButtonText(controlLabel, macroLabel, slotIndex) {
    if (controlLabel === 'Amount') {
      const slot = parseInt(slotIndex, 10) || 1;
      const value = Array.isArray(macroPanel.amount_values)
        ? readInt(macroPanel.amount_values[slot - 1])
        : null;
      return value != null && value >= 0
        ? `${macroLabel} Amount ${value}%`
        : `${macroLabel} Amount, not read`;
    }
    if (controlLabel === 'On/Off') {
      const slot = parseInt(slotIndex, 10) || 4;
      const stateIndex = Math.max(0, Math.min(1, slot - 4));
      const on = Array.isArray(macroPanel.button_states)
        ? readFlag(macroPanel.button_states[stateIndex])
        : null;
      return on == null ? `${macroLabel}, not read` : `${macroLabel} ${on ? 'ON' : 'OFF'}`;
    }
    return controlLabel;
  }

  function oscMenuEntries(col) {
    if (col.type === 'osc') {
      const entries = ['Copy'];
      if (oscPanel.copied) entries.push('Paste');
      entries.push('Initialize');
      if ((parseInt(oscPanel.count, 10) || 1) > 1) entries.push('Delete');
      return entries;
    }
    if (col.type === 'drums' || col.type === 'trig') return ['Initialize'];
    return [];
  }

  function performOscMenuChoice(col, choice) {
    if (col.type === 'osc') {
      perform('osc_menu_item', { index: col.index, item: choice });
    } else if (col.type === 'drums') {
      perform('drums_menu_item', { item: choice });
    } else if (col.type === 'trig') {
      perform('trig_menu_item', { item: choice });
    }
  }

  function oscMenuIdForCol(col) {
    return col.type === 'osc'
      ? `osc-menu-items-${col.index}`
      : `${col.type}-menu-items`;
  }

  function oscMenuButtonIdForCol(col) {
    return col.type === 'osc' ? `osc-menu-${col.index}` : `${col.type}-menu`;
  }

  function isSelectedOscCol(col) {
    if (oscPanel.selected_type == null) return false;
    const selectedType = String(oscPanel.selected_type);
    if (col.type !== selectedType) return false;
    if (col.type !== 'osc') return true;
    const selectedIndex = readInt(oscPanel.selected_index);
    return selectedIndex != null && parseInt(col.index, 10) === selectedIndex;
  }

  function markSelectedTabButton(btn, label, selected) {
    if (!selected) return btn;
    btn.classList.add('selected-tab');
    btn.textContent = `✓ ${label}`;
    btn.setAttribute('aria-label', `${label}, SELECTED`);
    btn.setAttribute('aria-current', 'true');
    return btn;
  }

  function oscButtonState(col, kind) {
    if (col.type === 'osc') {
      const states = kind === 'mute'
        ? oscPanel.mute_states
        : oscPanel.solo_states;
      return Array.isArray(states) ? readFlag(states[(parseInt(col.index, 10) || 1) - 1]) : null;
    }
    if (col.type === 'drums') {
      return readFlag(kind === 'mute' ? oscPanel.drums_mute : oscPanel.drums_solo);
    }
    if (col.type === 'trig') {
      return readFlag(kind === 'mute' ? oscPanel.trig_mute : oscPanel.trig_solo);
    }
    return null;
  }

  // Avenger lights every other tab's Mute while a Solo is on, as a Mute pressed: under a Solo a
  // Mute is said as silenced by it, never ON (src/read/strip.luau, Strip.silencedBy).
  function silencedWords(col) {
    const by = Array.isArray(oscPanel.solo_by) ? oscPanel.solo_by : [];
    if (by.length === 0 || oscButtonState(col, 'solo') === true) return null;
    if (by.length === 1) return `silenced by ${by[0]}'s Solo`;
    return `silenced by the Solos of ${by.slice(0, -1).join(', ')} and ${by[by.length - 1]}`;
  }

  function makeOscStateButton(col, kind, action, args, id) {
    const title = kind === 'mute' ? 'Mute' : 'Solo';
    const on = oscButtonState(col, kind);
    const silenced = kind === 'mute' ? silencedWords(col) : null;
    const label = silenced
      ? `${title}, ${silenced}`
      : (on == null ? `${title}, not read` : `${title} ${on ? 'ON' : 'OFF'}`);
    return makeCellButton(label, action, args, id);
  }

  function renderOscPanel() {
    const head = $('osc-panel-head');
    const body = $('osc-panel-body');
    if (!head || !body) return;
    head.textContent = '';
    body.textContent = '';
    const cols = oscColumns();
    cols.forEach((col) => {
      const th = document.createElement('th');
      th.scope = 'col';
      if (col.type === 'osc') {
        const btn = makeCellButton(
          col.heading,
          'select_osc_tab',
          { index: col.index },
          `osc-tab-${col.index}`
        );
        th.appendChild(markSelectedTabButton(
          btn,
          col.heading,
          isSelectedOscCol(col)
        ));
      } else if (col.type === 'add') {
        th.appendChild(makeCellButton('Add OSC', 'add_osc', {}, 'osc-add-head'));
      } else if (col.type === 'drums') {
        const btn = makeCellButton('Drums', 'select_drums_tab', {}, 'drums-tab');
        th.appendChild(markSelectedTabButton(btn, 'Drums', isSelectedOscCol(col)));
      } else if (col.type === 'trig') {
        const btn = makeCellButton('Trig', 'select_trig_tab', {}, 'trig-tab');
        th.appendChild(markSelectedTabButton(btn, 'Trig', isSelectedOscCol(col)));
      } else {
        th.textContent = col.label;
      }
      head.appendChild(th);
    });

    const rows = [
      ['Mute', (col) => {
        if (col.type === 'unread') return null;
        if (col.type === 'osc') return makeOscStateButton(col, 'mute', 'toggle_osc_mute', { index: col.index }, `osc-mute-${col.index}`);
        if (col.type === 'add') return makeCellButton('+', 'add_osc', {}, 'osc-add-mute');
        if (col.type === 'drums') return makeOscStateButton(col, 'mute', 'toggle_drums_mute', {}, 'drums-mute');
        return makeOscStateButton(col, 'mute', 'toggle_trig_mute', {}, 'trig-mute');
      }],
      ['Solo', (col) => {
        if (col.type === 'unread') return null;
        if (col.type === 'osc') return makeOscStateButton(col, 'solo', 'toggle_osc_solo', { index: col.index }, `osc-solo-${col.index}`);
        if (col.type === 'add') return makeCellButton('+', 'add_osc', {}, 'osc-add-solo');
        if (col.type === 'drums') return makeOscStateButton(col, 'solo', 'toggle_drums_solo', {}, 'drums-solo');
        return makeOscStateButton(col, 'solo', 'toggle_trig_solo', {}, 'trig-solo');
      }],
      ['Menu', (col) => {
        if (col.type === 'unread') return null;
        if (col.type === 'add') return makeCellButton('+', 'add_osc', {}, 'osc-add-menu');
        const cellId = oscMenuIdForCol(col);
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.textContent = 'Menu';
        btn.id = oscMenuButtonIdForCol(col);
        btn.setAttribute('aria-expanded', 'false');
        btn.setAttribute('aria-controls', cellId);
        btn.title = `Show or hide ${col.label} menu actions`;
        const menu = document.createElement('div');
        menu.id = cellId;
        menu.className = 'osc-menu-items';
        menu.setAttribute('role', 'group');
        menu.setAttribute('tabindex', '0');
        menu.setAttribute('aria-label', `${col.label} Menu`);
        menu.title = `${col.label} menu actions`;
        const menuOpen = openOscMenuId === cellId;
        menu.hidden = !menuOpen;
        btn.setAttribute('aria-expanded', menuOpen ? 'true' : 'false');
        oscMenuEntries(col).forEach((entry) => {
          const itemBtn = document.createElement('button');
          itemBtn.type = 'button';
          itemBtn.textContent = entry;
          itemBtn.id = col.type === 'osc'
            ? `osc-menu-${col.index}-${entry.toLowerCase()}`
            : `${col.type}-menu-${entry.toLowerCase()}`;
          itemBtn.addEventListener('click', () => {
            openOscMenuId = '';
            btn.setAttribute('aria-expanded', 'false');
            menu.hidden = true;
            trackFocus(btn);
            performOscMenuChoice(col, entry);
          });
          menu.appendChild(itemBtn);
        });
        btn.addEventListener('click', () => {
          trackFocus(btn);
          const open = btn.getAttribute('aria-expanded') === 'true';
          openOscMenuId = open ? '' : cellId;
          btn.setAttribute('aria-expanded', open ? 'false' : 'true');
          menu.hidden = open;
          if (!open) {
            trackFocus(menu);
            requestAnimationFrame(() => focusElementById(cellId));
          }
        });
        const frag = document.createDocumentFragment();
        frag.appendChild(btn);
        frag.appendChild(menu);
        return frag;
      }],
      ['Rename', (col) => {
        if (col.type === 'osc') {
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.textContent = 'Rename';
          btn.id = `osc-rename-${col.index}`;
          btn.addEventListener('click', () => {
            trackFocus(btn);
            perform('rename_osc', { index: col.index });
          });
          return btn;
        }
        if (col.type === 'add') return makeCellButton('+', 'add_osc', {}, 'osc-add-rename');
        return document.createTextNode('');
      }],
    ];

    rows.forEach(([rowName, maker]) => {
      const tr = document.createElement('tr');
      cols.forEach((col) => {
        const td = document.createElement('td');
        const node = maker(col);
        if (node) td.appendChild(node);
        tr.appendChild(td);
      });
      body.appendChild(tr);
    });
    renderOscDetailGroup();
    renderCentralPanel();
  }

  function renderMacroPanel() {
    const head = $('macro-panel-head');
    const body = $('macro-panel-body');
    if (!head || !body) return;
    head.textContent = '';
    body.textContent = '';
    const cols = macroColumns();
    cols.forEach((col) => {
      const th = document.createElement('th');
      th.scope = 'col';
      if (col.type === 'macro') {
        const btn = makeCellButton(
          col.label,
          'select_macro_tab',
          { index: col.index },
          `macro-tab-${col.index}`
        );
        th.appendChild(markSelectedTabButton(
          btn,
          col.label,
          isSelectedMacroCol(col)
        ));
      } else if (col.type === 'add') {
        const addBtn = makeCellButton('Add Macro Tab', 'add_macro_tab', {}, 'macro-add-tab');
        addBtn.title = 'Add a new Macro tab to the current preset.';
        th.appendChild(addBtn);
      } else if (col.type === 'delete-last') {
        const deleteBtn = makeCellButton(
          'Delete Last Tab',
          'delete_macro_tab',
          {},
          'macro-delete-last'
        );
        deleteBtn.title = 'Delete the last Macro tab of the current preset.';
        th.appendChild(deleteBtn);
      } else {
        th.textContent = col.label;
      }
      head.appendChild(th);
    });

    renderMacroDetailGroup();
  }

  function renderSampleEditorControls() {
    const tabs = $('sample-editor-tabs');
    let detail = $('sample-editor-detail-group');
    let detailItems = $('sample-editor-detail-items');
    if (!detail) {
      detail = document.createElement('div');
      detail.id = 'sample-editor-detail-group';
      detail.className = 'osc-detail-control-group';
      detail.setAttribute('role', 'group');
      detail.setAttribute('tabindex', '0');
      detail.setAttribute('aria-label', 'Sample');
      detailItems = document.createElement('div');
      detailItems.id = 'sample-editor-detail-items';
      detail.appendChild(detailItems);
    }
    const editorAreaGroup = $('editor-area-group');
    const sampleEditorGroup = $('sample-editor-group');
    const sampleBrowserGroup = $('sample-browser-group');
    const sampleBrowserItems = $('sample-browser-items');
    const items = $('sample-editor-volume-items');
    const sampleControlsGroup = $('sample-editor-volume-group');
    const sampleEditorView = String(oscPanel.sample_editor_view || 'editor');
    const showSampleBrowser = sampleEditorView === 'browser';
    if (editorAreaGroup) editorAreaGroup.hidden = showSampleBrowser;
    if (sampleEditorGroup) sampleEditorGroup.hidden = showSampleBrowser;
    if (sampleBrowserGroup) sampleBrowserGroup.hidden = !showSampleBrowser;
    if (sampleControlsGroup) sampleControlsGroup.hidden = showSampleBrowser;
    if (sampleBrowserItems) {
      sampleBrowserItems.textContent = '';
      [
        'Crash',
        'Open Hi-Hat',
        'Ride',
        'Closed Hi-Hat 2',
        'Percussion 2',
        'Closed Hi-Hat 1',
        'Percussion 1',
        'Snare 2',
        'Clap',
        'Snare 1',
        'Rimshot',
        'Bassdrum',
        'Drum Loops'
      ].forEach((label, index) => {
        const row = document.createElement('div');
        row.className = 'macro-slot-controls';
        const cell = document.createElement('div');
        cell.className = 'macro-control-cell wide';
        const btn = makeCellButton(
          label,
          'sample_browser_focus',
          { index: index + 1, name: label },
          `sample-browser-item-${index + 1}`
        );
        btn.title = 'Use the Avenger browser. To return to Avenger Control, press Return.';
        cell.appendChild(btn);
        row.appendChild(cell);
        sampleBrowserItems.appendChild(row);
      });
    }
    if (showSampleBrowser) {
      if (tabs) tabs.textContent = '';
      if (detailItems) detailItems.textContent = '';
      if (items) items.textContent = '';
      return;
    }
    const currentTab = ['sample', 'amp', 'pitch', 'eq'].includes(String(oscPanel.sample_editor_tab || 'sample'))
      ? String(oscPanel.sample_editor_tab || 'sample')
      : 'sample';
    const slotRead = readInt(oscPanel.drumkit_part_selected);
    const selectedSlot = slotRead == null ? null : Math.max(1, Math.min(12, slotRead));
    const selectedSampleName = (
      selectedSlot != null
      && Array.isArray(oscPanel.drumkit_part_labels)
      && oscPanel.drumkit_part_labels[selectedSlot - 1]
    ) ? String(oscPanel.drumkit_part_labels[selectedSlot - 1]) : 'selected';
    const tabLabels = [
      ['sample', 'Sample'],
      ['amp', 'Amp'],
      ['pitch', 'Pitch'],
      ['eq', 'EQ']
    ];
    const envelopeLabel = currentTab === 'amp' ? 'Amp Envelope' : 'Pitch Envelope';
    function buildSampleEditorDetail(label) {
      if (!detail || !detailItems) return null;
      detail.setAttribute('aria-label', label);
      detailItems.textContent = '';
      if (currentTab === 'sample') {
        const row = document.createElement('div');
        row.className = 'macro-slot-controls';
        [
          ['start', 'Start'],
          ['end', 'End']
        ].forEach(([key, buttonLabel]) => {
          const cell = document.createElement('div');
          cell.className = 'macro-control-cell';
          const btn = makeCellButton(
            buttonLabel,
            'sample_editor_field',
            { field: key },
            `sample-editor-sample-${key}`
          );
          btn.title = (
            key === 'start'
              ? `Set Start Point of the ${selectedSampleName} Sample. Press Return to return to Avenger Control.`
              : `Set End Point of the ${selectedSampleName} Sample. Press Return to return to Avenger Control.`
          );
          cell.appendChild(btn);
          row.appendChild(cell);
        });
        detailItems.appendChild(row);

        const playRow = document.createElement('div');
        playRow.className = 'macro-slot-controls';
        const playCell = document.createElement('div');
        playCell.className = 'macro-control-cell';
        const playButton = document.createElement('button');
        const playMenuId = 'sample-editor-sample-play-mode-items';
        playButton.type = 'button';
        playButton.textContent = 'Play Mode';
        playButton.id = 'sample-editor-sample-play-mode';
        playButton.setAttribute('aria-expanded', 'false');
        playButton.setAttribute('aria-controls', playMenuId);
        playButton.title = `Set Play Mode for the ${selectedSampleName} Sample.`;
        const playMenu = document.createElement('div');
        playMenu.id = playMenuId;
        playMenu.className = 'osc-menu-items';
        playMenu.setAttribute('role', 'group');
        playMenu.setAttribute('tabindex', '0');
        playMenu.setAttribute('aria-label', 'Play Mode');
        playMenu.title = `Play Mode options for the ${selectedSampleName} Sample.`;
        keepDisclosure(playButton, playMenu);
        [
          ['monophone', 'Monophone'],
          ['polyphone', 'Polyphone']
        ].forEach(([key, itemLabel]) => {
          const itemButton = document.createElement('button');
          itemButton.type = 'button';
          itemButton.textContent = itemLabel;
          itemButton.id = `${playMenuId}-${key}`;
          itemButton.addEventListener('click', () => {
            openDisclosures.delete(playMenuId);
            playButton.setAttribute('aria-expanded', 'false');
            playMenu.hidden = true;
            trackFocus(playButton);
            perform('sample_editor_play_mode', { item: key });
          });
          playMenu.appendChild(itemButton);
        });
        playButton.addEventListener('click', () => {
          trackFocus(playButton);
          const open = playButton.getAttribute('aria-expanded') === 'true';
          if (open) openDisclosures.delete(playMenuId); else openDisclosures.add(playMenuId);
          playButton.setAttribute('aria-expanded', open ? 'false' : 'true');
          playMenu.hidden = open;
          if (!open) {
            trackFocus(playMenu);
            requestAnimationFrame(() => focusElementById(playMenuId));
          }
        });
        playCell.appendChild(playButton);
        playCell.appendChild(playMenu);
        playRow.appendChild(playCell);

        const gainCell = document.createElement('div');
        gainCell.className = 'macro-control-cell';
        const gainButton = makeCellButton(
          'Gain',
          'sample_editor_field',
          { field: 'gain' },
          'sample-editor-sample-gain'
        );
        gainButton.title = `Set Gain of the ${selectedSampleName} Sample. Press Return to return to Avenger Control.`;
        gainCell.appendChild(gainButton);
        playRow.appendChild(gainCell);
        detailItems.appendChild(playRow);
      } else if (currentTab === 'amp' || currentTab === 'pitch') {
        const row = document.createElement('div');
        row.className = 'macro-slot-controls';
        [
          ['menu', 'Menu'],
          ['preset', 'Preset'],
          ['back', 'Back'],
          ['next', 'Next']
        ].forEach(([key, buttonLabel]) => {
          const cell = document.createElement('div');
          cell.className = 'macro-control-cell';
          const btn = makeCellButton(
            buttonLabel,
            'sample_editor_panel_button',
            { button: key },
            `sample-editor-${currentTab}-${key}`
          );
          if (key === 'menu') {
            btn.title = `Load, Save and delete ${envelopeLabel} Presets. Press Return to return to Avenger Control.`;
          } else if (key === 'preset') {
            btn.title = `Select ${envelopeLabel} Presets. Press Return to return to Avenger Control.`;
          } else if (key === 'back') {
            btn.title = `Previous ${envelopeLabel} Preset.`;
          } else if (key === 'next') {
            btn.title = `Next ${envelopeLabel} Preset.`;
          } else {
            btn.title = `${buttonLabel} ${envelopeLabel}.`;
          }
          cell.appendChild(btn);
          row.appendChild(cell);
        });
        detailItems.appendChild(row);

        const enableStates = currentTab === 'amp'
          ? (Array.isArray(oscPanel.sample_editor_amp_states) ? oscPanel.sample_editor_amp_states : [])
          : (Array.isArray(oscPanel.sample_editor_pitch_states) ? oscPanel.sample_editor_pitch_states : []);
        const isEnabled = selectedSlot == null ? null : readFlag(enableStates[selectedSlot - 1]);
        const enableRow = document.createElement('div');
        enableRow.className = 'macro-slot-controls';
        const enableCell = document.createElement('div');
        enableCell.className = 'macro-control-cell';
        const enableBtn = makeCellButton(
          isEnabled == null ? `${label}, not read` : `${label} ${isEnabled ? 'ON' : 'OFF'}`,
          'toggle_sample_editor_enable',
          { tab: currentTab },
          `sample-editor-${currentTab}-enable`
        );
        enableBtn.title = `Turn ${envelopeLabel} on or off for ${selectedSampleName}.`;
        enableCell.appendChild(enableBtn);
        enableRow.appendChild(enableCell);
        detailItems.appendChild(enableRow);

        const gridRow = document.createElement('div');
        gridRow.className = 'macro-slot-controls';
        const gridCell = document.createElement('div');
        gridCell.className = 'macro-control-cell';
        const gridButton = document.createElement('button');
        const gridMenuId = `sample-editor-${currentTab}-grid-items`;
        gridButton.type = 'button';
        gridButton.textContent = 'Grid';
        gridButton.id = `sample-editor-${currentTab}-grid`;
        gridButton.setAttribute('aria-expanded', 'false');
        gridButton.setAttribute('aria-controls', gridMenuId);
        gridButton.title = `Show or hide Grid options for the ${envelopeLabel}.`;
        const gridMenu = document.createElement('div');
        gridMenu.id = gridMenuId;
        gridMenu.className = 'osc-menu-items';
        gridMenu.setAttribute('role', 'group');
        gridMenu.setAttribute('tabindex', '0');
        gridMenu.setAttribute('aria-label', `${label} Grid`);
        gridMenu.title = `${envelopeLabel} Grid options.`;
        keepDisclosure(gridButton, gridMenu);
        [
          ['1/16', '1/16'],
          ['1/8', '1/8'],
          ['1/4', '1/4'],
          ['1/16T', '1/16T'],
          ['1/8T', '1/8T'],
          ['1/4T', '1/4T']
        ].forEach(([key, itemLabel]) => {
          const itemButton = document.createElement('button');
          itemButton.type = 'button';
          itemButton.textContent = itemLabel;
          itemButton.id = `${gridMenuId}-${key.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
          itemButton.addEventListener('click', () => {
            openDisclosures.delete(gridMenuId);
            gridButton.setAttribute('aria-expanded', 'false');
            gridMenu.hidden = true;
            trackFocus(gridButton);
            perform('sample_editor_grid', { item: key });
          });
          gridMenu.appendChild(itemButton);
        });
        gridButton.addEventListener('click', () => {
          trackFocus(gridButton);
          const open = gridButton.getAttribute('aria-expanded') === 'true';
          if (open) openDisclosures.delete(gridMenuId); else openDisclosures.add(gridMenuId);
          gridButton.setAttribute('aria-expanded', open ? 'false' : 'true');
          gridMenu.hidden = open;
          if (!open) {
            trackFocus(gridMenu);
            requestAnimationFrame(() => focusElementById(gridMenuId));
          }
        });
        gridCell.appendChild(gridButton);
        gridCell.appendChild(gridMenu);
        gridRow.appendChild(gridCell);

        [
          ['speed', 'Speed'],
          [currentTab === 'amp' ? 'strength' : 'range', currentTab === 'amp' ? 'Strength' : 'Range']
        ].forEach(([key, buttonLabel]) => {
          const cell = document.createElement('div');
          cell.className = 'macro-control-cell';
          const btn = makeCellButton(
            buttonLabel,
            'sample_editor_field',
            { field: key },
            `sample-editor-${currentTab}-${key}`
          );
          btn.title = `Edit the ${buttonLabel} of the ${envelopeLabel} for the ${selectedSampleName} Sample. Press Return to return to Avenger Control.`;
          cell.appendChild(btn);
          gridRow.appendChild(cell);
        });
        detailItems.appendChild(gridRow);
      } else if (currentTab === 'eq') {
        const eqStates = Array.isArray(oscPanel.sample_editor_eq_states)
          ? oscPanel.sample_editor_eq_states
          : [];
        const isEnabled = selectedSlot == null ? null : readFlag(eqStates[selectedSlot - 1]);
        const enableRow = document.createElement('div');
        enableRow.className = 'macro-slot-controls';
        const enableCell = document.createElement('div');
        enableCell.className = 'macro-control-cell';
        const enableBtn = makeCellButton(
          isEnabled == null ? 'EQ, not read' : `EQ ${isEnabled ? 'ON' : 'OFF'}`,
          'toggle_sample_editor_enable',
          { tab: 'eq' },
          'sample-editor-eq-enable'
        );
        enableBtn.title = `Turn EQ on or off for ${selectedSampleName}.`;
        enableCell.appendChild(enableBtn);
        enableRow.appendChild(enableCell);
        detailItems.appendChild(enableRow);

        const eqTypeItems = [
          ['peak', 'Peak'],
          ['low_shelf', 'Low-Shelf'],
          ['high_shelf', 'High-Shelf'],
          ['low_pass', 'Low-Pass'],
          ['high_pass', 'High-Pass']
        ];
        function appendEqBand(band) {
          const group = document.createElement('div');
          group.className = 'osc-detail-control-group';
          group.setAttribute('role', 'group');
          group.setAttribute('tabindex', '0');
          group.setAttribute('aria-label', `Band ${band}`);

          const row = document.createElement('div');
          row.className = 'macro-slot-controls';
          [
            [`eq${band}_freq`, 'Frequency'],
            [`eq${band}_gain`, 'Gain'],
            [`eq${band}_q`, 'Q-Factor']
          ].forEach(([key, buttonLabel]) => {
            const cell = document.createElement('div');
            cell.className = 'macro-control-cell';
            const btn = makeCellButton(
              buttonLabel,
              'sample_editor_field',
              { field: key },
              `sample-editor-eq-band-${band}-${key.split('_')[1]}`
            );
            btn.title = `Set ${buttonLabel} for EQ Band ${band} of the ${selectedSampleName} Sample. Press Return to return to Avenger Control.`;
            cell.appendChild(btn);
            row.appendChild(cell);
          });

          const typeCell = document.createElement('div');
          typeCell.className = 'macro-control-cell';
          const typeButton = document.createElement('button');
          const typeMenuId = `sample-editor-eq-band-${band}-type-items`;
          typeButton.type = 'button';
          typeButton.textContent = 'Type';
          typeButton.id = `sample-editor-eq-band-${band}-type`;
          typeButton.setAttribute('aria-expanded', 'false');
          typeButton.setAttribute('aria-controls', typeMenuId);
          typeButton.title = `Show or hide Type options for EQ Band ${band}.`;
          const typeMenu = document.createElement('div');
          typeMenu.id = typeMenuId;
          typeMenu.className = 'osc-menu-items';
          typeMenu.setAttribute('role', 'group');
          typeMenu.setAttribute('tabindex', '0');
          typeMenu.setAttribute('aria-label', `EQ Band ${band} Type`);
          typeMenu.title = `Type options for EQ Band ${band}.`;
          keepDisclosure(typeButton, typeMenu);
          eqTypeItems.forEach(([key, itemLabel]) => {
            const itemButton = document.createElement('button');
            itemButton.type = 'button';
            itemButton.textContent = itemLabel;
            itemButton.id = `${typeMenuId}-${key.replace(/_/g, '-')}`;
            itemButton.addEventListener('click', () => {
              openDisclosures.delete(typeMenuId);
              typeButton.setAttribute('aria-expanded', 'false');
              typeMenu.hidden = true;
              trackFocus(typeButton);
              perform('sample_editor_eq_type', { band, item: key });
            });
            typeMenu.appendChild(itemButton);
          });
          typeButton.addEventListener('click', () => {
            trackFocus(typeButton);
            const open = typeButton.getAttribute('aria-expanded') === 'true';
            if (open) openDisclosures.delete(typeMenuId); else openDisclosures.add(typeMenuId);
            typeButton.setAttribute('aria-expanded', open ? 'false' : 'true');
            typeMenu.hidden = open;
            if (!open) {
              trackFocus(typeMenu);
              requestAnimationFrame(() => focusElementById(typeMenuId));
            }
          });
          typeCell.appendChild(typeButton);
          typeCell.appendChild(typeMenu);
          row.appendChild(typeCell);
          group.appendChild(row);
          detailItems.appendChild(group);
        }
        appendEqBand(1);
        appendEqBand(2);
      }
      return detail;
    }
    if (!showSampleBrowser && tabs) {
      tabs.textContent = '';
      const tabRow = document.createElement('div');
      tabRow.className = 'macro-slot-controls drums-three-controls';
      tabLabels.forEach(([key, label], index) => {
        const cell = document.createElement('div');
        cell.className = 'macro-control-cell';
        cell.style.gridColumn = key === 'eq' ? '6' : String(index + 1);
        cell.style.gridRow = '1';
        const btn = makeCellButton(
          `${label}${oscPanel.sample_editor_tab === key ? ' SELECTED' : ''}`,
          'select_sample_editor_tab',
          { tab: key },
          `sample-editor-tab-${key}`
        );
        if (key === 'sample') {
          btn.title = `Edit the ${selectedSampleName} Sample.`;
        } else if (key === 'amp') {
          btn.title = `Edit the Amp Envelope for the ${selectedSampleName} Sample.`;
        } else if (key === 'pitch') {
          btn.title = `Edit the Pitch Envelope for the ${selectedSampleName} Sample.`;
        } else {
          btn.title = `Edit the EQ for the ${selectedSampleName} Sample.`;
        }
        cell.appendChild(btn);
        tabRow.appendChild(cell);
      });
      tabs.appendChild(tabRow);
    }
    if (!showSampleBrowser) {
      const currentLabel = tabLabels.find(([key]) => key === currentTab)?.[1] || 'Sample';
      buildSampleEditorDetail(currentLabel);
    }
    if (!items) return;
    items.textContent = '';
    const loopStretchMenuId = 'sample-editor-loop-stretch-items';
    const loopStretchRow = document.createElement('div');
    loopStretchRow.className = 'macro-slot-controls';
    const loopStretchCell = document.createElement('div');
    loopStretchCell.className = 'macro-control-cell';
    const loopStretchButton = document.createElement('button');
    loopStretchButton.type = 'button';
    loopStretchButton.textContent = 'Loop-Stretch';
    loopStretchButton.id = 'sample-editor-loop-stretch';
    loopStretchButton.setAttribute('aria-expanded', 'false');
    loopStretchButton.setAttribute('aria-controls', loopStretchMenuId);
    loopStretchButton.title = 'Show or hide Loop-Stretch modes for the selected drum slot.';
    const loopStretchMenu = document.createElement('div');
    loopStretchMenu.id = loopStretchMenuId;
    loopStretchMenu.className = 'osc-menu-items';
    loopStretchMenu.setAttribute('role', 'group');
    loopStretchMenu.setAttribute('tabindex', '0');
    loopStretchMenu.setAttribute('aria-label', 'Loop-Stretch');
    loopStretchMenu.title = 'Loop-Stretch modes for the selected drum slot.';
    keepDisclosure(loopStretchButton, loopStretchMenu);
    [
      ['off', 'Off'],
      ['turntable', 'Turntable'],
      ['stretch', 'Stretch']
    ].forEach(([key, label]) => {
      const itemButton = document.createElement('button');
      itemButton.type = 'button';
      itemButton.textContent = label;
      itemButton.id = `sample-editor-loop-stretch-${key}`;
      itemButton.addEventListener('click', () => {
        openDisclosures.delete(loopStretchMenuId);
        loopStretchButton.setAttribute('aria-expanded', 'false');
        loopStretchMenu.hidden = true;
        trackFocus(loopStretchButton);
        perform('sample_editor_loop_stretch', { item: key });
      });
      loopStretchMenu.appendChild(itemButton);
    });
    loopStretchButton.addEventListener('click', () => {
      trackFocus(loopStretchButton);
      const open = loopStretchButton.getAttribute('aria-expanded') === 'true';
      if (open) openDisclosures.delete(loopStretchMenuId); else openDisclosures.add(loopStretchMenuId);
      loopStretchButton.setAttribute('aria-expanded', open ? 'false' : 'true');
      loopStretchMenu.hidden = open;
      if (!open) {
        trackFocus(loopStretchMenu);
        requestAnimationFrame(() => focusElementById(loopStretchMenuId));
      }
    });
    loopStretchCell.appendChild(loopStretchButton);
    loopStretchCell.appendChild(loopStretchMenu);
    loopStretchRow.appendChild(loopStretchCell);
    items.appendChild(loopStretchRow);

    const controls = [
      ['volume', 'Volume'],
      ['pitch', 'Pitch'],
      ['pan', 'Pan'],
      ['spike', 'Spike'],
      ['delay', 'Delay'],
      ['lowpass', 'Lowpass'],
      ['highpass', 'Highpass'],
      ['rate', 'Rate']
    ];
    controls.forEach(([key, label]) => {
      const row = document.createElement('div');
      row.className = 'macro-slot-controls';

      const controlCell = document.createElement('div');
      controlCell.className = 'macro-control-cell';
      const controlButton = makeCellButton(
        label,
        'sample_editor_control',
        { control: key },
        `sample-editor-${key}`
      );
      controlButton.title = `Adjust ${label}. Press Page Up or Down, then Return to confirm.`;
      controlCell.appendChild(controlButton);
      row.appendChild(controlCell);

      const resetCell = document.createElement('div');
      resetCell.className = 'macro-control-cell';
      const resetButton = makeCellButton(
        `Reset ${label}`,
        'reset_sample_editor_control',
        { control: key },
        `sample-editor-reset-${key}`
      );
      resetButton.title = `Reset ${label}.`;
      resetCell.appendChild(resetButton);
      row.appendChild(resetCell);

      items.appendChild(row);
    });
  }

  function centralTabDefinitions() {
    return [
      ['fx', 'FX'],
      ['send_rack', 'SEND RACK'],
      ['master_fx', 'MASTER FX'],
      ['editor', 'EDITOR'],
      ['arp', 'ARP'],
      ['drum_sq', 'DRUM SQ'],
      ['step_sq', 'STEP SQ'],
      ['pitch', 'PITCH'],
      ['mod_env', 'MOD ENV'],
      ['mixer', 'MIXER'],
      ['zones', 'ZONES'],
      ['sys', 'SYS'],
      ['shaper_1', 'SHAPER 1']
    ];
  }

  // `text`: what the placeholder says, where it is not that the tab is out of this page's reach.
  function renderCentralPlaceholder(label, text) {
    const content = $('central-tab-content-group');
    const placeholder = $('central-placeholder-group');
    const items = $('central-placeholder-items');
    const editorGroup = $('editor-area-group');
    const sampleBrowserGroup = $('sample-browser-group');
    const sampleControlsGroup = $('sample-editor-volume-group');
    if (content) content.setAttribute('aria-label', label);
    if (editorGroup) editorGroup.hidden = true;
    if (sampleBrowserGroup) sampleBrowserGroup.hidden = true;
    if (sampleControlsGroup) sampleControlsGroup.hidden = true;
    if (!placeholder || !items) return;
    placeholder.hidden = false;
    placeholder.setAttribute('aria-label', label);
    items.textContent = '';
    const note = document.createElement('p');
    note.id = `central-placeholder-${centralPanelTab || 'unread'}-note`;
    note.className = 'dialog-message';
    note.setAttribute('tabindex', '0');
    note.textContent = text || `${label} is not reachable from this page yet.`;
    items.appendChild(note);
  }

  function renderCentralDrumSq() {
    const content = $('central-tab-content-group');
    const placeholder = $('central-placeholder-group');
    const items = $('central-placeholder-items');
    const editorGroup = $('editor-area-group');
    const sampleBrowserGroup = $('sample-browser-group');
    const sampleControlsGroup = $('sample-editor-volume-group');
    if (content) content.setAttribute('aria-label', 'DRUM SQ');
    if (editorGroup) editorGroup.hidden = true;
    if (sampleBrowserGroup) sampleBrowserGroup.hidden = true;
    if (sampleControlsGroup) sampleControlsGroup.hidden = true;
    if (!placeholder || !items) return;
    placeholder.hidden = false;
    placeholder.setAttribute('aria-label', 'DRUM SQ');
    items.textContent = '';

    const sequenceMenuRow = document.createElement('div');
    sequenceMenuRow.className = 'macro-slot-controls';
    const sequenceMenuCell = document.createElement('div');
    sequenceMenuCell.className = 'macro-control-cell';
    const sequenceMenuButton = document.createElement('button');
    const sequenceMenuId = 'central-drum-sq-sequence-menu-items';
    sequenceMenuButton.type = 'button';
    sequenceMenuButton.textContent = 'SEQUENCE MENU';
    sequenceMenuButton.id = 'central-drum-sq-sequence-menu';
    sequenceMenuButton.setAttribute('aria-expanded', 'false');
    sequenceMenuButton.setAttribute('aria-controls', sequenceMenuId);
    sequenceMenuButton.title = 'Show or hide DRUM SQ sequence actions.';
    const sequenceMenu = document.createElement('div');
    sequenceMenu.id = sequenceMenuId;
    sequenceMenu.className = 'osc-menu-items';
    sequenceMenu.setAttribute('role', 'group');
    sequenceMenu.setAttribute('tabindex', '0');
    sequenceMenu.setAttribute('aria-label', 'DRUM SQ SEQUENCE MENU');
    sequenceMenu.title = 'DRUM SQ sequence actions.';
    keepDisclosure(sequenceMenuButton, sequenceMenu);
    [
      ['save_sequence_as', 'SAVE SEQUENCE AS'],
      ['import_midi', 'IMPORT MIDI'],
      ['export_to_midi', 'EXPORT TO MIDI']
    ].forEach(([key, label]) => {
      const itemButton = document.createElement('button');
      itemButton.type = 'button';
      itemButton.textContent = label;
      itemButton.id = `${sequenceMenuId}-${key.replace(/_/g, '-')}`;
      itemButton.addEventListener('click', () => {
        openDisclosures.delete(sequenceMenuId);
        sequenceMenuButton.setAttribute('aria-expanded', 'false');
        sequenceMenu.hidden = true;
        trackFocus(sequenceMenuButton);
        perform('central_drum_sq_sequence_menu_item', { item: key });
      });
      sequenceMenu.appendChild(itemButton);
    });
    sequenceMenuButton.addEventListener('click', () => {
      trackFocus(sequenceMenuButton);
      const open = sequenceMenuButton.getAttribute('aria-expanded') === 'true';
      if (open) openDisclosures.delete(sequenceMenuId); else openDisclosures.add(sequenceMenuId);
      sequenceMenuButton.setAttribute('aria-expanded', open ? 'false' : 'true');
      sequenceMenu.hidden = open;
      if (!open) {
        trackFocus(sequenceMenu);
        requestAnimationFrame(() => focusElementById(sequenceMenuId));
      }
    });
    sequenceMenuCell.appendChild(sequenceMenuButton);
    sequenceMenuCell.appendChild(sequenceMenu);
    sequenceMenuRow.appendChild(sequenceMenuCell);
    items.appendChild(sequenceMenuRow);

    const patternsGroup = document.createElement('div');
    patternsGroup.className = 'osc-detail-control-group';
    patternsGroup.setAttribute('role', 'group');
    patternsGroup.setAttribute('tabindex', '0');
    patternsGroup.setAttribute('aria-label', 'PATTERNS');

    const patternPlayerRow = document.createElement('div');
    patternPlayerRow.className = 'macro-slot-controls';
    const patternPlayerCell = document.createElement('div');
    patternPlayerCell.className = 'macro-control-cell wide';
    const activePatterns = ['A']
      .concat(['b', 'c', 'd']
        .filter((key) => centralDrumSqPatternStates[key])
        .map((key) => key.toUpperCase()));
    const patternPlayerButton = makeCellButton(
      centralDrumSqPatternChain == null
        ? 'PATTERN CHAIN, not read'
        : (centralDrumSqPatternChain
          ? `PATTERN CHAIN (${activePatterns.join(', ')})`
          : 'PATTERN CHAIN OFF'),
      'central_drum_sq_pattern_player',
      {},
      'central-drum-sq-pattern-player'
    );
    patternPlayerButton.title = centralDrumSqPatternChain == null
      ? 'Turn the pattern chain on or off.'
      : (centralDrumSqPatternChain
        ? 'All active patterns play in ascending order.'
        : 'Only the selected pattern plays.');
    patternPlayerCell.appendChild(patternPlayerButton);
    patternPlayerRow.appendChild(patternPlayerCell);
    patternsGroup.appendChild(patternPlayerRow);

    const autoFollowRow = document.createElement('div');
    autoFollowRow.className = 'macro-slot-controls';
    const autoFollowCell = document.createElement('div');
    autoFollowCell.className = 'macro-control-cell wide';
    const autoFollowWords = centralDrumSqAutoFollow == null
      ? 'AUTO FOLLOW, not read'
      : `AUTO FOLLOW ${centralDrumSqAutoFollow ? 'ON' : 'OFF'}`;
    const autoFollowButton = makeCellButton(
      autoFollowWords,
      'central_drum_sq_auto_follow',
      {},
      'central-drum-sq-auto-follow'
    );
    autoFollowButton.textContent = autoFollowWords;
    autoFollowButton.title = 'Auto Follow for Drum SQ patterns.';
    autoFollowCell.appendChild(autoFollowButton);
    autoFollowRow.appendChild(autoFollowCell);
    patternsGroup.appendChild(autoFollowRow);

    const patternRow = document.createElement('div');
    patternRow.className = 'macro-slot-controls';
    function appendPatternSelectButton(key) {
      const label = key.toUpperCase();
      const cell = document.createElement('div');
      cell.className = 'macro-control-cell';
      const btn = makeCellButton(
        `${label}${centralDrumSqPattern === key ? ' SELECTED' : ''}`,
        'select_central_drum_sq_pattern',
        { pattern: key },
        `central-drum-sq-pattern-${key}`
      );
      btn.title = `Select Pattern ${label}.`;
      // SELECTED only from what is read: the state the action brings back renders it.
      cell.appendChild(btn);
      patternRow.appendChild(cell);
    }
    function appendPatternToggleButton(key) {
      const label = key.toUpperCase();
      const cell = document.createElement('div');
      cell.className = 'macro-control-cell';
      const state = centralDrumSqPatternStates[key];
      const btn = makeCellButton(
        state == null ? `${label}, not read` : `${label} ${state ? 'ON' : 'OFF'}`,
        'toggle_central_drum_sq_pattern',
        { pattern: key },
        `central-drum-sq-pattern-toggle-${key}`
      );
      btn.title = `Turn Pattern ${label} on or off.`;
      cell.appendChild(btn);
      patternRow.appendChild(cell);
    }
    appendPatternSelectButton('a');
    ['b', 'c', 'd'].forEach((key) => {
      appendPatternToggleButton(key);
      if (centralDrumSqPatternStates[key]) {
        appendPatternSelectButton(key);
      }
    });

    const patternMenuCell = document.createElement('div');
    patternMenuCell.className = 'macro-control-cell wide';
    const patternMenuButton = document.createElement('button');
    const patternMenuId = 'central-drum-sq-pattern-menu-items';
    // The selected pattern, or none where it was not read.
    const selectedPatternLabel = centralDrumSqPattern ? centralDrumSqPattern.toUpperCase() : '';
    const patternWords = selectedPatternLabel ? `PATTERN ${selectedPatternLabel}` : 'PATTERN';
    const patternHelp = selectedPatternLabel ? `Pattern ${selectedPatternLabel}` : 'the selected pattern';
    patternMenuButton.type = 'button';
    patternMenuButton.textContent = `${patternWords} MENU`;
    patternMenuButton.id = 'central-drum-sq-pattern-menu';
    patternMenuButton.setAttribute('aria-expanded', 'false');
    patternMenuButton.setAttribute('aria-controls', patternMenuId);
    patternMenuButton.title = `Show or hide ${patternHelp} actions.`;
    const patternMenu = document.createElement('div');
    patternMenu.id = patternMenuId;
    patternMenu.className = 'osc-menu-items';
    patternMenu.setAttribute('role', 'group');
    patternMenu.setAttribute('tabindex', '0');
    patternMenu.setAttribute('aria-label', `${patternWords} MENU`);
    patternMenu.title = `${patternHelp} actions.`;
    keepDisclosure(patternMenuButton, patternMenu);
    [
      ['copy', 'COPY', true],
      ['paste', 'PASTE', centralDrumSqPatternCopied],
      ['copy_first_half', 'COPY FIRST HALF', true],
      ['copy_second_half', 'COPY SECOND HALF', true],
      ['paste_to_first_half', 'PASTE TO FIRST HALF', centralDrumSqPatternCopied],
      ['paste_to_second_half', 'PASTE TO SECOND HALF', centralDrumSqPatternCopied],
      ['clear', 'CLEAR', true]
    ].forEach(([key, label, enabled]) => {
      const itemButton = document.createElement('button');
      itemButton.type = 'button';
      itemButton.textContent = label;
      itemButton.id = `${patternMenuId}-${key.replace(/_/g, '-')}`;
      itemButton.disabled = !enabled;
      itemButton.title = enabled ? `${label} ${patternHelp}.` : `${label} is unavailable until a pattern has been copied.`;
      itemButton.addEventListener('click', () => {
        if (!enabled) return;
        openDisclosures.delete(patternMenuId);
        patternMenuButton.setAttribute('aria-expanded', 'false');
        patternMenu.hidden = true;
        trackFocus(patternMenuButton);
        perform('central_drum_sq_pattern_menu_item', { item: key });
        renderCentralDrumSq();
        requestAnimationFrame(() => focusElementById('central-drum-sq-pattern-menu'));
      });
      patternMenu.appendChild(itemButton);
    });
    patternMenuButton.addEventListener('click', () => {
      trackFocus(patternMenuButton);
      const open = patternMenuButton.getAttribute('aria-expanded') === 'true';
      if (open) openDisclosures.delete(patternMenuId); else openDisclosures.add(patternMenuId);
      patternMenuButton.setAttribute('aria-expanded', open ? 'false' : 'true');
      patternMenu.hidden = open;
      if (!open) {
        trackFocus(patternMenu);
        requestAnimationFrame(() => focusElementById(patternMenuId));
      }
    });
    patternMenuCell.appendChild(patternMenuButton);
    patternMenuCell.appendChild(patternMenu);
    patternRow.appendChild(patternMenuCell);
    patternsGroup.appendChild(patternRow);
    items.appendChild(patternsGroup);

    function appendDrumSqControlRow(controls) {
      const row = document.createElement('div');
      row.className = 'macro-slot-controls';
      controls.forEach((control) => {
        const [key, label, action, args, id, help] = control;
        const cell = document.createElement('div');
        cell.className = 'macro-control-cell';
        const btn = makeCellButton(label, action, args, id);
        btn.title = help;
        cell.appendChild(btn);
        row.appendChild(cell);
      });
      items.appendChild(row);
    }

    appendDrumSqControlRow([
      [
        'vel_amount',
        'VEL AMOUNT',
        'central_drum_sq_control',
        { control: 'vel_amount' },
        'central-drum-sq-vel-amount',
        'Adjust VEL AMOUNT. Press Page Up or Down, then Return to confirm.'
      ],
      [
        'reset_vel_amount',
        'RESET VEL AMOUNT',
        'reset_central_drum_sq_control',
        { control: 'vel_amount' },
        'central-drum-sq-reset-vel-amount',
        'Reset VEL AMOUNT.'
      ]
    ]);

    const lengthRow = document.createElement('div');
    lengthRow.className = 'macro-slot-controls';
    const lengthCell = document.createElement('div');
    lengthCell.className = 'macro-control-cell wide';
    const lengthButton = document.createElement('button');
    const lengthMenuId = 'central-drum-sq-length-items';
    const lengthSteps = centralDrumSqLengthSteps == null ? null : normalizeDrumSqLengthSteps(centralDrumSqLengthSteps);
    lengthButton.type = 'button';
    lengthButton.textContent = centralDrumSqLengthWords(lengthSteps);
    lengthButton.id = 'central-drum-sq-length';
    lengthButton.setAttribute('aria-expanded', 'false');
    lengthButton.setAttribute('aria-controls', lengthMenuId);
    lengthButton.title = 'Set the pattern length.';
    const lengthMenu = document.createElement('div');
    lengthMenu.id = lengthMenuId;
    lengthMenu.className = 'osc-menu-items';
    lengthMenu.setAttribute('role', 'group');
    lengthMenu.setAttribute('tabindex', '0');
    lengthMenu.setAttribute('aria-label', 'PATTERN LENGTH');
    lengthMenu.title = 'Pattern length.';
    keepDisclosure(lengthButton, lengthMenu);
    for (let beats = 1; beats <= 16; beats += 1) {
      const steps = beats * 4;
      const itemButton = document.createElement('button');
      itemButton.type = 'button';
      const label = centralDrumSqLengthLabel(steps);
      const selected = steps === lengthSteps;
      itemButton.textContent = label;
      itemButton.id = `${lengthMenuId}-${steps}`;
      itemButton.dataset.lengthSteps = String(steps);
      itemButton.dataset.baseLabel = label;
      itemButton.dataset.menuOption = String(steps);
      itemButton.setAttribute('aria-label', label);
      itemButton.setAttribute('aria-pressed', selected ? 'true' : 'false');
      itemButton.title = label;
      if (selected) {
        itemButton.setAttribute('aria-current', 'true');
      }
      itemButton.addEventListener('click', () => {
        lengthMenuButtonSafeClose();
        // The LENGTH shown is the one read: the state the action brings back sets it.
        trackFocus(lengthButton);
        perform('central_drum_sq_length', { steps: steps });
        requestAnimationFrame(() => focusElementById('central-drum-sq-length'));
      });
      lengthMenu.appendChild(itemButton);
    }
    updateCentralDrumSqLengthMenuSelection(lengthSteps);
    function lengthMenuButtonSafeClose() {
      openDisclosures.delete(lengthMenuId);
      lengthButton.setAttribute('aria-expanded', 'false');
      lengthMenu.hidden = true;
    }
    lengthButton.addEventListener('click', () => {
      trackFocus(lengthButton);
      const open = lengthButton.getAttribute('aria-expanded') === 'true';
      if (open) openDisclosures.delete(lengthMenuId); else openDisclosures.add(lengthMenuId);
      lengthButton.setAttribute('aria-expanded', open ? 'false' : 'true');
      lengthMenu.hidden = open;
      if (!open) {
        trackFocus(lengthMenu);
        requestAnimationFrame(() => {
          // On the current length, or on the list where the length was not read.
          const targetId = centralDrumSqLengthSteps == null
            ? lengthMenuId
            : `${lengthMenuId}-${normalizeDrumSqLengthSteps(centralDrumSqLengthSteps)}`;
          focusElementById(targetId);
        });
      }
    });
    lengthCell.appendChild(lengthButton);
    lengthCell.appendChild(lengthMenu);
    lengthRow.appendChild(lengthCell);
    items.appendChild(lengthRow);

    appendDrumSqControlRow([
      [
        'shuffle',
        'SHUFFLE',
        'central_drum_sq_control',
        { control: 'shuffle' },
        'central-drum-sq-shuffle',
        'Adjust SHUFFLE. Press Page Up or Down, then Return to confirm.'
      ],
      [
        'reset_shuffle',
        'RESET SHUFFLE',
        'reset_central_drum_sq_control',
        { control: 'shuffle' },
        'central-drum-sq-reset-shuffle',
        'Reset SHUFFLE.'
      ]
    ]);

    const speedRow = document.createElement('div');
    speedRow.className = 'macro-slot-controls';
    const speedCell = document.createElement('div');
    speedCell.className = 'macro-control-cell';
    const speedButton = document.createElement('button');
    const speedMenuId = 'central-drum-sq-speed-items';
    speedButton.type = 'button';
    speedButton.textContent = 'SPEED';
    speedButton.id = 'central-drum-sq-speed';
    speedButton.setAttribute('aria-expanded', 'false');
    speedButton.setAttribute('aria-controls', speedMenuId);
    speedButton.title = 'Show or hide DRUM SQ Speed options.';
    const speedMenu = document.createElement('div');
    speedMenu.id = speedMenuId;
    speedMenu.className = 'osc-menu-items';
    speedMenu.setAttribute('role', 'group');
    speedMenu.setAttribute('tabindex', '0');
    speedMenu.setAttribute('aria-label', 'DRUM SQ SPEED');
    speedMenu.title = 'DRUM SQ Speed options.';
    keepDisclosure(speedButton, speedMenu);
    [
      ['1/16', '1/16'],
      ['1/8', '1/8'],
      ['1/16T', '1/16T'],
      ['1/8T', '1/8T'],
      ['1/32', '1/32'],
      ['1/4', '1/4'],
      ['1/32T', '1/32T'],
      ['1/4T', '1/4T']
    ].forEach(([key, label]) => {
      const itemButton = document.createElement('button');
      itemButton.type = 'button';
      itemButton.textContent = label;
      itemButton.id = `${speedMenuId}-${key.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
      itemButton.addEventListener('click', () => {
        openDisclosures.delete(speedMenuId);
        speedButton.setAttribute('aria-expanded', 'false');
        speedMenu.hidden = true;
        trackFocus(speedButton);
        perform('central_drum_sq_speed', { item: key });
      });
      speedMenu.appendChild(itemButton);
    });
    speedButton.addEventListener('click', () => {
      trackFocus(speedButton);
      const open = speedButton.getAttribute('aria-expanded') === 'true';
      if (open) openDisclosures.delete(speedMenuId); else openDisclosures.add(speedMenuId);
      speedButton.setAttribute('aria-expanded', open ? 'false' : 'true');
      speedMenu.hidden = open;
      if (!open) {
        trackFocus(speedMenu);
        requestAnimationFrame(() => focusElementById(speedMenuId));
      }
    });
    speedCell.appendChild(speedButton);
    speedCell.appendChild(speedMenu);
    speedRow.appendChild(speedCell);
    items.appendChild(speedRow);
  }

  function renderCentralPanel() {
    const rowHost = $('central-tabs-row');
    const content = $('central-tab-content-group');
    const placeholder = $('central-placeholder-group');
    const defs = centralTabDefinitions();
    if (centralPanelTab != null && !defs.some(([key]) => key === centralPanelTab)) {
      centralPanelTab = null;
    }
    if (rowHost) {
      rowHost.textContent = '';
      const row = document.createElement('div');
      row.className = 'macro-slot-controls';
      const clickableTabs = new Set(['editor', 'arp', 'drum_sq', 'step_sq']);
      defs.forEach(([key, label]) => {
        const cell = document.createElement('div');
        cell.className = 'macro-control-cell';
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.id = `central-tab-${key}`;
        btn.textContent = `${label}${centralPanelTab === key ? ' SELECTED' : ''}`;
        btn.title = `Show ${label}.`;
        btn.addEventListener('click', () => {
          centralPanelTab = key;
          trackFocus(btn);
          renderCentralPanel();
          requestAnimationFrame(() => focusElementById(`central-tab-${key}`));
          if (clickableTabs.has(key)) {
            perform('select_central_tab', { tab: key });
          }
        });
        cell.appendChild(btn);
        row.appendChild(cell);
      });
      rowHost.appendChild(row);
    }

    if (centralPanelTab == null) {
      renderCentralPlaceholder(
        'Central tab, not read',
        'The central tab is read only while the Drums tab is selected.'
      );
      return;
    }
    const currentLabel = defs.find(([key]) => key === centralPanelTab)?.[1] || 'EDITOR';
    if (centralPanelTab === 'drum_sq') {
      renderCentralDrumSq();
      return;
    }
    if (centralPanelTab !== 'editor') {
      renderCentralPlaceholder(currentLabel);
      return;
    }

    if (content) content.setAttribute('aria-label', 'Editor');
    if (placeholder) placeholder.hidden = true;
    if (String(oscPanel.selected_type || 'osc') === 'drums') {
      renderSampleEditorControls();
    } else {
      renderCentralPlaceholder('Editor');
    }
  }

  function renderOscDetailGroup() {
    const detail = $('osc-tab-detail-group');
    const items = $('osc-tab-detail-items');
    if (!detail) return;
    let label = oscPanel.selected_label || '';
    if (!label) {
      if (oscPanel.selected_type === 'drums') label = 'Drums';
      else if (oscPanel.selected_type === 'trig') label = 'Trig';
      else if (oscPanel.selected_type == null) label = 'Selected tab, not read';
      else {
        const idx = Math.max(1, parseInt(oscPanel.selected_index, 10) || 1);
        label = (Array.isArray(oscPanel.names) && oscPanel.names[idx - 1])
          ? oscPanel.names[idx - 1]
          : `OSC ${idx}`;
      }
    }
    detail.setAttribute('aria-label', label);
    if (!items) return;
    items.textContent = '';
    if (oscPanel.selected_type === 'drums') {
      const heading = document.createElement('h3');
      heading.textContent = 'Drumkits & Sequences';
      heading.setAttribute('tabindex', '0');
      items.appendChild(heading);

      const group = document.createElement('div');
      group.className = 'osc-detail-control-group';
      group.setAttribute('role', 'group');
      group.setAttribute('tabindex', '0');
      group.setAttribute('aria-label', 'Browser');

      function appendControlRow(parent, controls, rowClass) {
        const row = document.createElement('div');
        row.className = 'macro-slot-controls';
        if (rowClass) row.classList.add(rowClass);
        controls.forEach(([buttonId, buttonLabel, action, args, help]) => {
          const cell = document.createElement('div');
          cell.className = 'macro-control-cell';
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.textContent = buttonLabel;
          btn.id = buttonId;
          btn.title = help;
          if (action) {
            btn.addEventListener('click', () => {
              trackFocus(btn);
              perform(action, args);
            });
          }
          cell.appendChild(btn);
          row.appendChild(cell);
        });
        parent.appendChild(row);
      }

      function appendDrumsRow(controls, rowClass) {
        appendControlRow(group, controls, rowClass);
      }

      appendDrumsRow([
        [
          'drum-pattern',
          oscPanel.drum_pattern == null ? 'Pattern, not read' : `Pattern ${oscPanel.drum_pattern ? 'ON' : 'OFF'}`,
          'toggle_drum_pattern',
          {},
          'Turn the drum pattern on or off.'
        ]
      ]);
      const drumMode = oscPanel.drum_mode == null ? null : String(oscPanel.drum_mode);
      const drumBackHelp = oscPanel.drum_lock
        ? 'Select the previous drumkit with its corresponding sequence.'
        : (drumMode === 'sq' ? 'Select the previous sequence.' : 'Select the previous drumkit.');
      const drumNextHelp = oscPanel.drum_lock
        ? 'Select the next drumkit with its corresponding sequence.'
        : (drumMode === 'sq' ? 'Select the next sequence.' : 'Select the next drumkit.');
      appendDrumsRow([
        [
          'drum-mode-drum',
          `DRUMKIT${drumMode === 'drum' ? ' SELECTED' : ''}`,
          'select_drum_mode',
          { mode: 'drum' },
          'Select drumkit mode.'
        ],
        [
          'drum-mode-lock',
          oscPanel.drum_lock == null ? 'LOCK, not read' : (oscPanel.drum_lock ? 'LOCKED' : 'UNLOCKED'),
          'select_drum_mode',
          { mode: 'lock' },
          'Lock drumkit and sequence changes.'
        ],
        [
          'drum-mode-sq',
          `SEQUENCE${drumMode === 'sq' ? ' SELECTED' : ''}`,
          'select_drum_mode',
          { mode: 'sq' },
          'Select sequence mode.'
        ]
      ], 'drums-three-controls');
      // Named for what they step — never just Back and Next, the preset's words: the sequence in
      // SEQUENCE mode without the lock, the drumkit otherwise.
      const stepsSequence = !oscPanel.drum_lock && drumMode === 'sq';
      appendDrumsRow([
        ['drums-kit-back', stepsSequence ? 'Previous sequence' : 'Previous drumkit', 'previous_drumkit', {}, drumBackHelp],
        ['drums-kit-next', stepsSequence ? 'Next sequence' : 'Next drumkit', 'next_drumkit', {}, drumNextHelp]
      ]);
      items.appendChild(group);

      const drumkitLabels = [
        'Crash',
        'Open Hi-Hat',
        'Ride',
        'Closed Hi-Hat 2',
        'Percussion 2',
        'Closed Hi-Hat 1',
        'Percussion 1',
        'Snare 2',
        'Clap',
        'Snare 1',
        'Rimshot',
        'Bassdrum'
      ];
      // A slot whose name was not read goes by its role in the kit, and its row says the name was
      // not read: the factory names are never shown as this kit's.
      const drumkitNamesRead = drumkitLabels.map((_, index) => Array.isArray(oscPanel.drumkit_part_labels)
        && String(oscPanel.drumkit_part_labels[index] || '').trim() !== '');
      const drumkitCurrentLabels = drumkitLabels.map((role, index) => (drumkitNamesRead[index]
        ? String(oscPanel.drumkit_part_labels[index]).trim()
        : role));
      const drumkitColumns = [
        ['browse', 'Browse'],
        ['back', 'Back'],
        ['next', 'Next'],
        ['mute', 'Mute'],
        ['solo', 'Solo'],
        ['lock', 'Lock'],
        ['loop', 'Loop'],
        ['volume', 'Volume'],
        ['reset_volume', 'Reset Volume'],
        ['pan', 'Pan'],
        ['reset_pan', 'Reset Pan'],
        ['pitch', 'Pitch'],
        ['reset_pitch', 'Reset Pitch'],
        ['menu', 'Menu'],
        ['edit_sample', 'Edit Sample']
      ];
      function drumkitState(kind, index) {
        const key = `drumkit_${kind}_states`;
        const states = Array.isArray(oscPanel[key]) ? oscPanel[key] : [];
        return readFlag(states[index]);
      }
      const drumkitMenuEntries = [
        ['lock_all', 'Lock All'],
        ['unlock_all', 'Unlock All'],
        ['save_drumkit_as', 'Save Drumkit as'],
        ['copy_sample_to', 'Copy Sample to, Sample Only'],
        ['swap_sample_with', 'Swap sample with, Sample Only'],
        ['copy_slot_to', 'Copy Slot to, With Settings'],
        ['swap_slot_with', 'Swap slot with, With Settings'],
        ['remove_sample', 'Remove Sample']
      ];
      const drumkitSubmenuItems = [
        'Slot B (Crash)',
        'Slot A# (Open HH)',
        'Slot A (Ride)',
        'Slot G# (Closed HH2)',
        'Slot G (Perc.2)',
        'Slot F# (Closed HH1)',
        'Slot F (Perc.1)',
        'Slot E (Snare 2)',
        'Slot D# (Clap)',
        'Slot D (Snare 1)',
        'Slot C# (Rimshot)',
        'Slot C (Bassdrum)'
      ];
      const drumkitSubmenuActions = new Set([
        'copy_sample_to',
        'swap_sample_with',
        'copy_slot_to',
        'swap_slot_with'
      ]);
      function makeDrumkitMenu(slot, slotLabel) {
        const menuId = `drumkit-menu-items-${slot}`;
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.textContent = 'Menu';
        btn.id = `drumkit-menu-${slot}`;
        btn.setAttribute('aria-expanded', 'false');
        btn.setAttribute('aria-controls', menuId);
        btn.title = `Show or hide ${slotLabel} menu actions`;
        const menu = document.createElement('div');
        menu.id = menuId;
        menu.className = 'osc-menu-items';
        menu.setAttribute('role', 'group');
        menu.setAttribute('tabindex', '0');
        menu.setAttribute('aria-label', `${slotLabel} Menu`);
        menu.title = `${slotLabel} menu actions`;
        const menuOpen = openDrumkitMenuId === menuId;
        menu.hidden = !menuOpen;
        btn.setAttribute('aria-expanded', menuOpen ? 'true' : 'false');
        drumkitMenuEntries.forEach(([item, label]) => {
          const itemBtn = document.createElement('button');
          itemBtn.type = 'button';
          itemBtn.textContent = label;
          itemBtn.id = `${menuId}-${item}`;
          if (drumkitSubmenuActions.has(item)) {
            const submenuId = `${menuId}-${item}-items`;
            itemBtn.setAttribute('aria-expanded', 'false');
            itemBtn.setAttribute('aria-controls', submenuId);
            itemBtn.title = `Show or hide ${label} targets`;
            const submenu = document.createElement('div');
            submenu.id = submenuId;
            submenu.className = 'osc-menu-items';
            submenu.setAttribute('role', 'group');
            submenu.setAttribute('tabindex', '0');
            submenu.setAttribute('aria-label', `${label} Targets`);
            const submenuOpen = openDrumkitSubmenuId === submenuId;
            submenu.hidden = !submenuOpen;
            itemBtn.setAttribute('aria-expanded', submenuOpen ? 'true' : 'false');
            drumkitSubmenuItems.forEach((targetLabel, targetIndex) => {
              const targetSlot = targetIndex + 1;
              if (targetSlot === slot) return;
              const targetBtn = document.createElement('button');
              targetBtn.type = 'button';
              targetBtn.textContent = targetLabel;
              targetBtn.id = `${submenuId}-${targetSlot}`;
              targetBtn.addEventListener('click', () => {
                openDrumkitMenuId = '';
                openDrumkitSubmenuId = '';
                btn.setAttribute('aria-expanded', 'false');
                itemBtn.setAttribute('aria-expanded', 'false');
                menu.hidden = true;
                submenu.hidden = true;
                trackFocus(btn);
                perform('drumkit_menu_item', {
                  index: slot,
                  item: item,
                  target_index: targetSlot
                });
              });
              submenu.appendChild(targetBtn);
            });
            itemBtn.addEventListener('click', () => {
              trackFocus(itemBtn);
              const open = itemBtn.getAttribute('aria-expanded') === 'true';
              openDrumkitSubmenuId = open ? '' : submenuId;
              itemBtn.setAttribute('aria-expanded', open ? 'false' : 'true');
              submenu.hidden = open;
              if (!open) {
                trackFocus(submenu);
                requestAnimationFrame(() => focusElementById(submenuId));
              }
            });
            menu.appendChild(itemBtn);
            menu.appendChild(submenu);
            return;
          }
          itemBtn.addEventListener('click', () => {
            openDrumkitMenuId = '';
            openDrumkitSubmenuId = '';
            btn.setAttribute('aria-expanded', 'false');
            menu.hidden = true;
            trackFocus(btn);
            perform('drumkit_menu_item', { index: slot, item: item });
          });
          menu.appendChild(itemBtn);
        });
        btn.addEventListener('click', () => {
          trackFocus(btn);
          const open = btn.getAttribute('aria-expanded') === 'true';
          openDrumkitMenuId = open ? '' : menuId;
          openDrumkitSubmenuId = '';
          btn.setAttribute('aria-expanded', open ? 'false' : 'true');
          menu.hidden = open;
          if (!open) {
            trackFocus(menu);
            requestAnimationFrame(() => focusElementById(menuId));
          }
        });
        const frag = document.createDocumentFragment();
        frag.appendChild(btn);
        frag.appendChild(menu);
        return frag;
      }
      function makeDrumkitCell(kind, label, index, slotLabel) {
        const slot = index + 1;
        if (kind === 'mute' || kind === 'solo' || kind === 'lock' || kind === 'loop') {
          const title = label;
          const on = drumkitState(kind, index);
          return makeCellButton(
            on == null ? `${title}, not read` : `${title} ${on ? 'ON' : 'OFF'}`,
            'toggle_drumkit_button',
            { index: slot, button: kind },
            `drumkit-${kind}-${slot}`
          );
        }
        if (kind === 'volume' || kind === 'pan' || kind === 'pitch') {
          return makeCellButton(
            label,
            'drumkit_control',
            { index: slot, control: kind },
            `drumkit-${kind}-${slot}`
          );
        }
        if (kind === 'reset_volume' || kind === 'reset_pan' || kind === 'reset_pitch') {
          const control = kind.replace('reset_', '');
          return makeCellButton(
            label,
            'reset_drumkit_control',
            { index: slot, control: control },
            `drumkit-reset-${control}-${slot}`
          );
        }
        if (kind === 'menu') {
          return makeDrumkitMenu(slot, slotLabel);
        }
        if (kind === 'edit_sample') {
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.textContent = label;
          btn.id = `drumkit-edit-sample-${slot}`;
          btn.addEventListener('click', () => {
            centralPanelTab = 'editor';
            trackFocus(btn);
            renderCentralPanel();
            perform('drumkit_column', { index: slot, column: kind });
          });
          return btn;
        }
        return makeCellButton(
          label,
          'drumkit_column',
          { index: slot, column: kind },
          `drumkit-${kind.replace('_', '-')}-${slot}`
        );
      }
      const kitTable = document.createElement('table');
      kitTable.className = 'osc-table';
      kitTable.setAttribute('aria-label', 'Drumkit');
      const kitBody = document.createElement('tbody');
      drumkitCurrentLabels.forEach((label, index) => {
        const selected = String(index + 1) === String(oscPanel.drumkit_part_selected || '');
        const tr = document.createElement('tr');
        const th = document.createElement('th');
        th.scope = 'row';
        th.appendChild(makeCellButton(
          `${label}${drumkitNamesRead[index] ? '' : ', name not read'}${selected ? ' SELECTED' : ''}`,
          'select_drumkit_part',
          { index: index + 1 },
          `drumkit-part-${index + 1}`
        ));
        tr.appendChild(th);
        drumkitColumns.forEach(([kind, colLabel]) => {
          const td = document.createElement('td');
          td.appendChild(makeDrumkitCell(kind, colLabel, index, label));
          tr.appendChild(td);
        });
        kitBody.appendChild(tr);
      });
      kitTable.appendChild(kitBody);
      items.appendChild(kitTable);
      return;
    }
    if (oscPanel.selected_type !== 'osc') return;
    const selectedIndex = Math.max(1, parseInt(oscPanel.selected_index, 10));
    const volumeValues = Array.isArray(oscPanel.volume_values) ? oscPanel.volume_values : [];
    const panValues = Array.isArray(oscPanel.pan_values) ? oscPanel.pan_values : [];
    const transposeValues = Array.isArray(oscPanel.transpose_values) ? oscPanel.transpose_values : [];
    const fineTuneValues = Array.isArray(oscPanel.fine_tune_values) ? oscPanel.fine_tune_values : [];
    const randomPitchValues = Array.isArray(oscPanel.random_pitch_values) ? oscPanel.random_pitch_values : [];
    const volume = parseInt(volumeValues[selectedIndex - 1], 10);
    const pan = parseInt(panValues[selectedIndex - 1], 10);
    const transpose = parseInt(transposeValues[selectedIndex - 1], 10);
    const fineTune = parseInt(fineTuneValues[selectedIndex - 1], 10);
    const randomPitch = parseInt(randomPitchValues[selectedIndex - 1], 10);
    const volumeLabel = Number.isFinite(volume) ? `Volume ${volume}%` : 'Volume, not read';
    let panLabel = 'Pan, not read';
    if (Number.isFinite(pan)) {
      if (pan === 0) panLabel = 'Pan Center';
      else if (pan < 0) panLabel = `Pan ${Math.abs(pan)}% Left`;
      else panLabel = `Pan ${pan}% Right`;
    }
    let transposeText = 'Transpose, not read';
    if (Number.isFinite(transpose)) {
      if (transpose === 0) {
        transposeText = 'Transpose About 0';
      } else if (transpose % 12 === 0) {
        const octaves = transpose / 12;
        transposeText = `Transpose About ${octaves > 0 ? '+' : ''}${octaves} Oct`;
      } else {
        const approxOctaves = Math.round(transpose / 12);
        transposeText = `Transpose About ${approxOctaves > 0 ? '+' : ''}${approxOctaves} Oct`;
      }
    }
    let fineTuneText = 'Fine Tune, not read';
    if (Number.isFinite(fineTune)) {
      fineTuneText = fineTune === 0
        ? 'Fine Tune 0'
        : `Fine Tune ${fineTune > 0 ? '+' : ''}${fineTune} Cent`;
    }
    let randomPitchText = 'Random/Alternate Pitch, not read';
    if (Number.isFinite(randomPitch)) {
      if (randomPitch === 0) {
        randomPitchText = 'Random/Alternate Pitch Off';
      } else if (randomPitch < 0) {
        randomPitchText = `Random/Alternate Pitch Random ${Math.abs(randomPitch)}%`;
      } else {
        randomPitchText = `Random/Alternate Pitch Alternate ${randomPitch}%`;
      }
    }
    const mainHeading = document.createElement('h3');
    mainHeading.textContent = 'Main Controls';
    mainHeading.setAttribute('tabindex', '0');
    items.appendChild(mainHeading);

    function appendControlGroup(groupLabel, controls) {
      const group = document.createElement('div');
      group.className = 'osc-detail-control-group';
      group.setAttribute('role', 'group');
      group.setAttribute('tabindex', '0');
      group.setAttribute('aria-label', groupLabel);
      const row = document.createElement('div');
      row.className = 'macro-slot-controls';
      controls.forEach(([buttonId, buttonLabel, action, args, help]) => {
        const cell = document.createElement('div');
        cell.className = 'macro-control-cell';
        const btn = document.createElement('button');
        btn.type = 'button';
        btn.textContent = buttonLabel;
        btn.id = buttonId;
        btn.title = help;
        btn.addEventListener('click', () => {
          trackFocus(btn);
          perform(action, args);
        });
        cell.appendChild(btn);
        row.appendChild(cell);
      });
      group.appendChild(row);
      items.appendChild(group);
    }

    const pitchControls = [
      ['osc-volume', volumeLabel, 'osc_control', { control: 'volume' }, `Adjust Volume for ${label}. Press Page Up or Down, then Return to confirm.`],
      ['osc-reset-volume', 'Reset Volume', 'reset_osc_control', { control: 'volume' }, `Reset Volume for ${label}.`],
      ['osc-pan', panLabel, 'osc_control', { control: 'pan' }, `Adjust Pan for ${label}. Press Page Up or Down, then Return to confirm.`],
      ['osc-reset-pan', 'Reset Pan', 'reset_osc_control', { control: 'pan' }, `Reset Pan for ${label}.`],
      ['osc-transpose', transposeText, 'osc_control', { control: 'transpose' }, `Adjust Transpose for ${label}. Press Page Up or Down, then Return to confirm.`],
      ['osc-reset-transpose', 'Reset Transpose', 'reset_osc_control', { control: 'transpose' }, `Reset Transpose for ${label}.`],
      ['osc-fine-tune', fineTuneText, 'osc_control', { control: 'fine_tune' }, `Adjust Fine Tune for ${label}. Press Page Up or Down, then Return to confirm.`],
      ['osc-reset-fine-tune', 'Reset Fine Tune', 'reset_osc_control', { control: 'fine_tune' }, `Reset Fine Tune for ${label}.`],
      ['osc-random-pitch', randomPitchText, 'osc_control', { control: 'random_pitch' }, `Adjust Random/Alternate Pitch for ${label}. Random pitch changes each new note by up to plus or minus one semitone. Alternate pitch switches each new note up and down by up to plus or minus one semitone. Press Page Up or Down, then Return to confirm.`],
      ['osc-reset-random-pitch', 'Reset Random/Alternate Pitch', 'reset_osc_control', { control: 'random_pitch' }, `Reset Random/Alternate Pitch for ${label}.`]
    ];
    appendControlGroup('Volume, Pan & Pitch', pitchControls);

    appendControlGroup('2-Band-Filter', [
      ['osc-low-pass', 'Low-Pass', 'osc_filter_control', { control: 'low_pass' }, `Adjust Low-Pass for ${label}. Page Up moves left, Page Down moves right. Press Return to confirm.`],
      ['osc-high-pass', 'High-Pass', 'osc_filter_control', { control: 'high_pass' }, `Adjust High-Pass for ${label}. Page Up moves left, Page Down moves right. Press Return to confirm.`]
    ]);
  }

  function renderMacroDetailGroup() {
    const detail = $('macro-tab-detail-group');
    const items = $('macro-tab-detail-items');
    if (!detail) return;
    if (macroPanel.selected_index == null) {
      detail.setAttribute('aria-label', 'Macro tab, not read');
      if (items) items.textContent = '';
      return;
    }
    const idx = Math.max(1, Math.min(4, parseInt(macroPanel.selected_index, 10) || 1));
    const labels = macroTabLabels(idx);
    // The shown tab's names the database's spelling replaced, marked on their slots.
    const marked = Array.isArray(macroPanel.labels_from_database) ? macroPanel.labels_from_database : [];
    detail.setAttribute('aria-label', `Macro ${idx}`);
    if (items) {
      items.textContent = '';
      [
        { className: 'macros', start: 0, end: 3 },
        { className: 'buttons', start: 3, end: 5 }
      ].forEach((rowDef) => {
        const row = document.createElement('div');
        row.className = `macro-detail-row ${rowDef.className}`;
        labels.slice(rowDef.start, rowDef.end).forEach((words, labelOffset) => {
          const slotIndex = rowDef.start + labelOffset + 1;
          const label = words.name;
          const slot = document.createElement('div');
          slot.className = 'macro-slot';
          slot.setAttribute('role', 'group');
          slot.setAttribute('tabindex', '0');
          slot.setAttribute(
            'aria-label',
            marked[slotIndex - 1] === true ? `${words.label}, from the database` : words.label
          );
          const isMacroSlider = slotIndex <= 3;
          const isMacroButton = slotIndex >= 4;
          if (isMacroSlider || isMacroButton) {
            const controls = document.createElement('div');
            controls.className = 'macro-slot-controls';
            const controlLabels = isMacroSlider
              ? ['Amount', 'Reset Amount', 'Rename', 'Menu']
              : ['On/Off', 'Rename', 'Menu'];
            controlLabels.forEach((controlLabel) => {
              const cell = document.createElement('div');
              cell.className = 'macro-control-cell';
              if (controlLabel === 'On/Off') cell.classList.add('wide');
              const btn = document.createElement('button');
              btn.type = 'button';
              btn.textContent = macroControlButtonText(controlLabel, label, slotIndex);
              btn.id = `macro-${idx}-slot-${slotIndex}-${controlLabel.toLowerCase().replace(/\//g, '-').replace(/\s+/g, '-')}`;
              btn.title = isMacroButton && controlLabel === 'Menu'
                ? `Show menu options for ${label}`
                : macroControlTitle(controlLabel, label);
              if (controlLabel === 'Menu') {
                const menuId = isMacroSlider
                  ? `macro-${idx}-slot-${slotIndex}-reset-mode-menu`
                  : `macro-${idx}-slot-${slotIndex}-button-menu`;
                btn.setAttribute('aria-expanded', openMacroMenuId === menuId ? 'true' : 'false');
                btn.setAttribute('aria-controls', menuId);
              }
              btn.addEventListener('click', () => {
                trackFocus(btn);
                if (controlLabel === 'Rename') {
                  perform('rename_macro', { index: slotIndex });
                } else if (controlLabel === 'Menu') {
                  const menuId = isMacroSlider
                    ? `macro-${idx}-slot-${slotIndex}-reset-mode-menu`
                    : `macro-${idx}-slot-${slotIndex}-button-menu`;
                  openMacroMenuId = openMacroMenuId === menuId ? '' : menuId;
                  renderMacroDetailGroup();
                  if (openMacroMenuId) {
                    requestAnimationFrame(() => focusElementById(menuId));
                  } else {
                    requestAnimationFrame(() => focusElementById(btn.id));
                  }
                } else if (controlLabel === 'Amount') {
                  perform('macro_amount', { index: slotIndex });
                } else if (controlLabel === 'Reset Amount') {
                  perform('reset_macro_amount', { index: slotIndex });
                } else if (controlLabel === 'On/Off') {
                  perform('toggle_macro_button', { index: slotIndex - 3 });
                }
              });
              cell.appendChild(btn);
              if (controlLabel === 'Menu') {
                const menuId = isMacroSlider
                  ? `macro-${idx}-slot-${slotIndex}-reset-mode-menu`
                  : `macro-${idx}-slot-${slotIndex}-button-menu`;
                const menu = document.createElement('div');
                menu.id = menuId;
                menu.className = 'osc-menu-items';
                menu.setAttribute('role', 'group');
                menu.setAttribute('tabindex', '0');
                menu.setAttribute(
                  'aria-label',
                  isMacroSlider ? `${label} Reset Mode` : `${label} Menu`
                );
                menu.hidden = openMacroMenuId !== menuId;
                const entries = isMacroSlider
                  ? macroResetModeEntries
                  : macroButtonMenuEntries;
                entries.forEach((entry) => {
                  const itemBtn = document.createElement('button');
                  itemBtn.type = 'button';
                  itemBtn.textContent = entry.label;
                  itemBtn.title = entry.help;
                  itemBtn.id = `${menuId}-${entry.mode || entry.item}`;
                  itemBtn.addEventListener('click', () => {
                    openMacroMenuId = '';
                    trackFocus(itemBtn);
                    if (isMacroSlider) {
                      perform('macro_reset_mode', {
                        index: slotIndex,
                        mode: entry.mode
                      });
                    } else {
                      perform('macro_button_menu_item', {
                        index: slotIndex - 3,
                        item: entry.item
                      });
                    }
                  });
                  menu.appendChild(itemBtn);
                });
                cell.appendChild(menu);
              }
              controls.appendChild(cell);
            });
            slot.appendChild(controls);
          }
          row.appendChild(slot);
        });
        items.appendChild(row);
      });
    }
  }

  // --- page → module ---
  // An action and its arguments, by the tool's names. `focused` is what the page knows of its
  // own keyboard: the module acts only while its page has the keyboard, and logs where the
  // page's word and the host's disagree.
  function perform(action, args) {
    if (!ACTIONS.includes(action)) {
      problem(`perform: '${action}' is not one of this page's actions`);
      return;
    }
    post({ action: action, args: args || {}, focused: document.hasFocus() });
  }

  // --- Focus tracking -------------------------------------------------
  // When the module brings the plug-in forward for a click, the WebView is no longer
  // key and AppKit doesn't reliably preserve DOM focus across the
  // orderOut/order-front round-trip — VoiceOver sometimes resumes on
  // the window title or the first heading instead of the button the
  // user just pressed. So we track every user-triggered focus target
  // ourselves, and the module asks for restoreFocus() before
  // bringing the page back. Covers data-action buttons AND the zoom
  // selector; falls back to the preset name if the tracked element is
  // gone (e.g. redo list items that are re-rendered each time).
  let lastFocusedEl = null;
  let lastFocusedId = '';
  function trackFocus(el) {
    lastFocusedEl = el;
    lastFocusedId = (el && el.id) ? el.id : '';
  }
  // Once: the tool's retries on timers are not ported — the module's focus comes before the page
  // is shown, and the host puts the keyboard back on the last element as the page takes it.
  function focusTarget(target) {
    if (!target) return false;
    try {
      if (document.activeElement &&
          typeof document.activeElement.blur === 'function' &&
          document.activeElement !== document.body) {
        document.activeElement.blur();
      }
    } catch (_) { /* ignore */ }
    try { target.focus(); } catch (_) {}
    return true;
  }
  function focusElementById(id) {
    return focusTarget($(id));
  }

  function menuOptionId(itemsId, option) {
    return `${itemsId}-${String(option).toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;
  }

  function updateButtonMenuSelection(itemsId, selectedOption) {
    const items = $(itemsId);
    if (!items) return;
    Array.from(items.querySelectorAll('button[data-menu-option]')).forEach((btn) => {
      const selected = String(btn.dataset.menuOption) === String(selectedOption);
      const base = btn.dataset.baseLabel || btn.textContent || '';
      btn.textContent = selected ? `✓ ${base}` : base;
      btn.setAttribute('aria-label', selected ? `${base}, SELECTED` : base);
      if (selected) {
        btn.setAttribute('aria-current', 'true');
      } else {
        btn.removeAttribute('aria-current');
      }
    });
  }

  function normalizeDrumSqLengthSteps(value) {
    const raw = Number(value || 32);
    const rounded = Math.round(raw / 4) * 4;
    return Math.max(4, Math.min(64, Number.isFinite(rounded) ? rounded : 32));
  }

  function centralDrumSqLengthLabel(steps) {
    const safeSteps = normalizeDrumSqLengthSteps(steps);
    const beats = Math.max(1, Math.min(16, safeSteps / 4));
    return `${beats} BEAT${beats === 1 ? '' : 'S'} = ${safeSteps} STEPS`;
  }

  function updateCentralDrumSqLengthMenuSelection(steps) {
    const safeSteps = steps == null ? null : normalizeDrumSqLengthSteps(steps);
    const menu = $('central-drum-sq-length-items');
    if (!menu) return;
    Array.from(menu.querySelectorAll('button[data-menu-option]')).forEach((button) => {
      const selected = String(button.dataset.menuOption) === String(safeSteps);
      const base = button.dataset.baseLabel || button.textContent || '';
      button.textContent = base;
      button.setAttribute('aria-label', base);
      button.title = base;
      button.setAttribute('aria-pressed', selected ? 'true' : 'false');
      if (selected) {
        button.setAttribute('aria-current', 'true');
      } else {
        button.removeAttribute('aria-current');
      }
    });
  }

  // The length button's words: "not read" for a length not read, and "set, not read" for the one
  // the page's LENGTH set — LENGTH is a knob with no length drawn, which nothing reads.
  function centralDrumSqLengthWords(steps) {
    if (steps == null) return 'LENGTH, not read';
    const beats = Math.max(1, Math.min(16, steps / 4));
    return `LENGTH ${beats} BEAT${beats === 1 ? '' : 'S'}, ${steps} STEPS${centralDrumSqLengthSet ? ', set, not read' : ''}`;
  }

  function setCentralDrumSqLengthSteps(value) {
    const steps = value == null ? null : normalizeDrumSqLengthSteps(value);
    centralDrumSqLengthSteps = steps;
    const trigger = $('central-drum-sq-length');
    if (trigger) {
      trigger.textContent = centralDrumSqLengthWords(steps);
      trigger.setAttribute('aria-label', centralDrumSqLengthWords(steps));
    }
    updateCentralDrumSqLengthMenuSelection(steps);
    return steps;
  }

  function selectedButtonMenuItem(itemsId) {
    return document.querySelector(`#${itemsId} button[aria-current="true"]`);
  }

  function setupButtonMenu(triggerId, itemsId, options, actionName, argName, labelForOption, currentValueGetter) {
    const trigger = $(triggerId);
    const items = $(itemsId);
    if (!trigger || !items) return;
    options.forEach((option) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      const label = labelForOption(option);
      btn.textContent = label;
      btn.dataset.baseLabel = label;
      btn.dataset.menuOption = String(option);
      btn.id = menuOptionId(itemsId, option);
      btn.addEventListener('click', () => {
        trigger.setAttribute('aria-expanded', 'false');
        items.hidden = true;
        trackFocus(trigger);
        requestAnimationFrame(() => focusElementById(triggerId));
        const args = {};
        args[argName] = option;
        perform(actionName, args);
      });
      items.appendChild(btn);
    });
    trigger.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      const open = trigger.getAttribute('aria-expanded') === 'true';
      trigger.setAttribute('aria-expanded', open ? 'false' : 'true');
      items.hidden = open;
      trackFocus(trigger);
      if (!open) {
        requestAnimationFrame(() => {
          const selected = selectedButtonMenuItem(itemsId);
          focusElementById(selected ? selected.id : itemsId);
        });
      }
    });
    if (typeof currentValueGetter === 'function') {
      updateButtonMenuSelection(itemsId, currentValueGetter());
    }
  }

  function setPanelSectionOpen(triggerId, groupId, open) {
    const trigger = $(triggerId);
    const group = $(groupId);
    if (!trigger || !group) return;
    trigger.setAttribute('aria-expanded', open ? 'true' : 'false');
    group.hidden = !open;
  }

  function setContentPanelOpen(open) {
    contentPanelOpen = !!open;
    setPanelSectionOpen('content-browser-toggle', 'content-browser-group', contentPanelOpen);
  }

  function setOscillatorPanelOpen(open) {
    oscillatorPanelOpen = !!open;
    setPanelSectionOpen('osc-panel-toggle', 'osc-panel-group', oscillatorPanelOpen);
  }

  function setCentralTabsOpen(open) {
    centralTabsOpen = !!open;
    setPanelSectionOpen('central-tabs-toggle', 'central-panel-group', centralTabsOpen);
  }

  function setMacroPanelOpen(open) {
    macroPanelOpen = !!open;
    setPanelSectionOpen('macro-panel-toggle', 'macro-panel-group', macroPanelOpen);
  }

  const contentBrowserToggle = $('content-browser-toggle');
  if (contentBrowserToggle) {
    contentBrowserToggle.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      trackFocus(contentBrowserToggle);
      setContentPanelOpen(!contentPanelOpen);
    });
    setContentPanelOpen(true);
  }

  const oscPanelToggle = $('osc-panel-toggle');
  if (oscPanelToggle) {
    oscPanelToggle.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      trackFocus(oscPanelToggle);
      setOscillatorPanelOpen(!oscillatorPanelOpen);
    });
    setOscillatorPanelOpen(false);
  }

  const macroPanelToggle = $('macro-panel-toggle');
  if (macroPanelToggle) {
    macroPanelToggle.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      trackFocus(macroPanelToggle);
      setMacroPanelOpen(!macroPanelOpen);
    });
    setMacroPanelOpen(true);
  }

  const centralTabsToggle = $('central-tabs-toggle');
  if (centralTabsToggle) {
    centralTabsToggle.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      trackFocus(centralTabsToggle);
      setCentralTabsOpen(!centralTabsOpen);
    });
    setCentralTabsOpen(false);
  }

  // Plain data-action buttons (close, preset nav, etc.)
  document.querySelectorAll('[data-action]').forEach(btn => {
    btn.addEventListener('click', () => {
      trackFocus(btn);
      perform(btn.dataset.action);
    });
  });

  // --- Preset menu disclosure ---
  // The Menu trigger toggles aria-expanded + the [hidden] flag on the
  // sibling #preset-menu-items region. We do this manually instead of
  // using <details>/<summary> because VoiceOver-on-WebKit (a) failed
  // to collapse the details on a second activation of the summary,
  // and (b) read the entire details body inline whenever VO landed
  // on the open summary. With the explicit button pattern, VO only
  // ever announces the trigger's own state ("Menu, button, expanded
  // / collapsed") — the inner buttons are reached by navigation, one
  // at a time, like every other button in the panel.
  //
  // Menu entries close the menu and return focus to the Menu trigger.
  // Initialize Preset still routes through the confirmation alertdialog
  // installed below.
  const menuTrigger = $('preset-menu-trigger');
  const menuItems = $('preset-menu-items');
  function setMenuOpen(open) {
    if (!menuTrigger || !menuItems) return;
    menuTrigger.setAttribute(
      'aria-expanded', open ? 'true' : 'false'
    );
    menuItems.hidden = !open;
  }
  function closePresetMenuToTrigger() {
    setMenuOpen(false);
    if (menuTrigger) {
      trackFocus(menuTrigger);
      requestAnimationFrame(() => focusElementById('preset-menu-trigger'));
    }
  }
  function closePresetMenuOnly() {
    setMenuOpen(false);
  }
  if (menuTrigger) {
    menuTrigger.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      const open = menuTrigger.getAttribute('aria-expanded') === 'true';
      setMenuOpen(!open);
      trackFocus(menuTrigger);
      if (!open) {
        requestAnimationFrame(() => focusElementById('preset-menu-items'));
      }
    });
  }
  ['load-preset-button', 'save-preset-as-button'].forEach((id) => {
    const item = $(id);
    if (item) {
      item.addEventListener('click', () => closePresetMenuToTrigger());
    }
  });

  // --- Initialize-Preset confirmation dialog ---
  // The "Initialize Preset" button in the preset menu does NOT fire
  // pick_initialize_preset directly. It shows a panel-internal
  // alertdialog with Yes/No buttons that DO fire the actions:
  // pick_initialize_preset chooses Initialize in Avenger's MENU and
  // answers Avenger's own warning box where it is read;
  // cancel_initialize_preset clicks nothing in Avenger.
  //
  // We move VO focus to the warning text when the dialog opens, so
  // the destructive action has to be read before the Yes/No buttons.
  // Either Yes or No closes the dialog and returns focus to the
  // "Initialize Preset" trigger so the user can re-open the menu
  // (or step back into the panel) without losing their place.
  const initDialog = $('confirm-init-dialog');
  const initTrigger = $('initialize-preset-trigger');
  const initTitle = $('confirm-init-title');
  const initWarning = $('confirm-init-warning');
  const initYes = $('confirm-init-yes');
  const initNo = $('confirm-init-no');

  function openInitDialog() {
    if (!initDialog) return;
    initDialog.hidden = false;
    // Defer the focus move so AppKit / VO see the dialog as visible
    // before we land focus inside it. Without the rAF the dialog can
    // still be display:none (style hasn't applied yet) when focus()
    // fires and VO ignores the focusin event.
    requestAnimationFrame(() => {
      try { initTitle.focus(); } catch (_) {}
      trackFocus(initTitle);
    });
  }

  function closeInitDialog(returnFocus) {
    if (!initDialog) return;
    initDialog.hidden = true;
    if (returnFocus && initTrigger) {
      try { initTrigger.focus(); } catch (_) {}
      trackFocus(initTrigger);
    }
  }

  if (initTrigger) {
    initTrigger.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      closePresetMenuOnly();
      openInitDialog();
    });
  }
  // Yes/No close the dialog locally; the data-action click handler
  // installed above ALSO fires (it's the same DOM button), so the
  // module hears pick_initialize_preset / cancel_initialize_preset
  // and we just have to deal with the visual side here.
  if (initYes) {
    initYes.addEventListener('click', () => closeInitDialog(true));
  }
  if (initNo) {
    initNo.addEventListener('click', () => closeInitDialog(true));
  }

  // --- The Settings section ---
  // The tool's Windows-only block made general: its disclosure handler, under the section's ids,
  // and the controls of the module's settings. Each control sends its change and shows nothing of
  // it by itself: what the controls show is what the module posts (applySettings), after every
  // change — this section's or its settings dialog's — and at every ready.
  const settingsTrigger = $('settings-trigger');
  const settingsItems = $('settings-menu-items');
  if (settingsTrigger && settingsItems) {
    settingsTrigger.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      const open = settingsTrigger.getAttribute('aria-expanded') === 'true';
      settingsTrigger.setAttribute('aria-expanded', open ? 'false' : 'true');
      settingsItems.hidden = open;
      trackFocus(settingsTrigger);
      if (!open) {
        requestAnimationFrame(() => focusElementById('settings-menu-items'));
      }
    });
  }

  // Expansions List: a disclosure as the Size menu's, its entries the lists the module offers.
  const rosterTrigger = $('settings-roster-trigger');
  const rosterItems = $('settings-roster-items');
  let settingsRosters = [];
  let settingsRoster = null;
  function buildRosterItems() {
    if (!rosterItems) return;
    rosterItems.textContent = '';
    settingsRosters.forEach((option) => {
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.textContent = option;
      btn.dataset.baseLabel = option;
      btn.dataset.menuOption = option;
      btn.id = menuOptionId('settings-roster-items', option);
      btn.addEventListener('click', () => {
        rosterTrigger.setAttribute('aria-expanded', 'false');
        rosterItems.hidden = true;
        trackFocus(rosterTrigger);
        requestAnimationFrame(() => focusElementById('settings-roster-trigger'));
        perform('set_setting', { key: 'roster', value: option });
      });
      rosterItems.appendChild(btn);
    });
  }
  if (rosterTrigger && rosterItems) {
    rosterTrigger.addEventListener('click', (event) => {
      event.preventDefault();
      event.stopPropagation();
      const open = rosterTrigger.getAttribute('aria-expanded') === 'true';
      rosterTrigger.setAttribute('aria-expanded', open ? 'false' : 'true');
      rosterItems.hidden = open;
      trackFocus(rosterTrigger);
      if (!open) {
        requestAnimationFrame(() => {
          const selected = selectedButtonMenuItem('settings-roster-items');
          focusElementById(selected ? selected.id : 'settings-roster-items');
        });
      }
    });
  }

  // The field whose value was sent last: the settings message that answers it writes that field
  // even while it has the keyboard — Return sends from the field and leaves it there, and the
  // `change` Return fires is the only one — so a value refused shows the one in effect again.
  // Typing in the field again before the answer keeps what is typed.
  let settingsSentFrom = null;

  // The page key, written as a key spec (Change Page Key, or Return in the field), and its default.
  const keyInput = $('settings-page-key-input');
  const keyChange = $('settings-page-key-change');
  const keyDefault = $('settings-page-key-default');
  function sendPageKey() {
    settingsSentFrom = keyInput;
    perform('set_setting', { key: 'pageKey', value: keyInput ? keyInput.value.trim() : '' });
  }
  if (keyChange) {
    keyChange.addEventListener('click', () => {
      trackFocus(keyChange);
      sendPageKey();
    });
  }
  if (keyInput) {
    keyInput.addEventListener('focus', () => trackFocus(keyInput));
    keyInput.addEventListener('input', () => {
      if (settingsSentFrom === keyInput) settingsSentFrom = null;
    });
    keyInput.addEventListener('keydown', (e) => {
      if (e.key !== 'Enter') return;
      e.preventDefault();
      sendPageKey();
    });
  }
  if (keyDefault) {
    keyDefault.addEventListener('click', () => {
      trackFocus(keyDefault);
      perform('reset_settings', { key: 'pageKey' });
    });
  }

  // The two switches: each sends the other of what the module last posted.
  const soundsToggle = $('settings-sounds');
  const leaveDawToggle = $('settings-leave-daw');
  [[soundsToggle, 'sounds'], [leaveDawToggle, 'pageLeavesDawActive']].forEach(([toggle, setting]) => {
    if (!toggle) return;
    toggle.addEventListener('click', () => {
      trackFocus(toggle);
      perform('set_setting', { key: setting, value: toggle.getAttribute('aria-pressed') !== 'true' });
    });
  });

  // DRUM SQ's pace, sent as written once the field is left or Return is pressed in it: a number
  // where it is one, the text otherwise, for the module to say why it is none.
  const cadenceInput = $('settings-drum-sq-cadence');
  if (cadenceInput) {
    cadenceInput.addEventListener('focus', () => trackFocus(cadenceInput));
    cadenceInput.addEventListener('input', () => {
      if (settingsSentFrom === cadenceInput) settingsSentFrom = null;
    });
    cadenceInput.addEventListener('change', () => {
      const text = cadenceInput.value.trim();
      const n = Number(text);
      settingsSentFrom = cadenceInput;
      perform('set_setting', { key: 'drumSqCadence', value: text !== '' && Number.isFinite(n) ? n : text });
    });
  }

  // Reset Saved Settings: the page's own alertdialog first, as Initialize Preset's; Reset sends
  // (its data-action, above), Cancel sends nothing; either closes it and goes back to the trigger.
  const resetTrigger = $('settings-reset-trigger');
  const resetDialog = $('confirm-reset-dialog');
  const resetTitle = $('confirm-reset-title');
  function closeResetDialog() {
    if (!resetDialog) return;
    resetDialog.hidden = true;
    if (resetTrigger) {
      try { resetTrigger.focus(); } catch (_) {}
      trackFocus(resetTrigger);
    }
  }
  if (resetTrigger && resetDialog) {
    resetTrigger.addEventListener('click', (e) => {
      e.preventDefault();
      e.stopPropagation();
      resetDialog.hidden = false;
      requestAnimationFrame(() => {
        try { resetTitle.focus(); } catch (_) {}
        trackFocus(resetTitle);
      });
    });
  }
  ['confirm-reset-yes', 'confirm-reset-no'].forEach((id) => {
    const btn = $(id);
    if (btn) btn.addEventListener('click', () => closeResetDialog());
  });

  // A settings message: every control set from it, and nothing sent from here — no action, no
  // event — so a settings message never comes back as a change. The two fields are left alone
  // while they have the keyboard — what the user is typing stays — except the one whose value was
  // just sent, which shows the value in effect: the module answers every set_setting with this
  // message, at once.
  function applySettings(m) {
    const sentFrom = settingsSentFrom;
    settingsSentFrom = null;
    const values = (m && m.values) || {};
    const rosters = Array.isArray(m && m.rosters) ? m.rosters.map(String) : [];
    if (rosters.join('\n') !== settingsRosters.join('\n')) {
      settingsRosters = rosters;
      buildRosterItems();
    }
    settingsRoster = values.roster == null ? null : String(values.roster);
    updateButtonMenuSelection('settings-roster-items', settingsRoster);
    if (rosterTrigger) {
      rosterTrigger.textContent = settingsRoster ? `Expansions List, ${settingsRoster}` : 'Expansions List';
    }
    if (keyInput && values.pageKey != null) {
      if (document.activeElement !== keyInput || sentFrom === keyInput) keyInput.value = String(values.pageKey);
    }
    const keyWords = m && m.keyWords ? String(m.keyWords) : '';
    if (keyChange && keyWords) {
      keyChange.title = `Change the key that opens and closes Avenger Control. Current: ${keyWords}.`;
    }
    if (keyDefault && m && m.defaultKeyWords) {
      keyDefault.title = `Set the page key back to ${m.defaultKeyWords}.`;
    }
    if (soundsToggle) soundsToggle.setAttribute('aria-pressed', values.sounds === true ? 'true' : 'false');
    if (leaveDawToggle) leaveDawToggle.setAttribute('aria-pressed', values.pageLeavesDawActive === true ? 'true' : 'false');
    if (cadenceInput && values.drumSqCadence != null) {
      if (document.activeElement !== cadenceInput || sentFrom === cadenceInput) {
        cadenceInput.value = String(values.drumSqCadence);
      }
    }
  }

  // --- Header/content disclosure menus ---
  setupButtonMenu(
    'zoom-menu-trigger',
    'zoom-menu-items',
    zoomMenuOptions,
    'set_zoom',
    'percent',
    (value) => `${value} %`,
    () => currentZoom
  );
  setupButtonMenu(
    'expansion-view-menu-trigger',
    'expansion-view-menu-items',
    expansionViewOptions,
    'set_expansion_view',
    'view',
    (value) => String(value),
    () => currentExpansionView
  );
  // The Settings section's Confirm OSC Count and Confirm Expansion View (the tool's): what the module
  // posts back shows what was confirmed.
  setupButtonMenu(
    'settings-osc-count-trigger',
    'settings-osc-count-items',
    [1, 2, 3, 4, 5, 6, 7, 8],
    'confirm_osc_count',
    'count',
    (n) => `${n} OSC${n === 1 ? '' : 's'}`,
    () => null
  );
  setupButtonMenu(
    'settings-expansion-view-trigger',
    'settings-expansion-view-items',
    expansionViewOptions,
    'confirm_expansion_view',
    'view',
    (value) => String(value),
    () => currentExpansionView
  );
  const expansionSearchInput = $('expansion-search-input');
  const resetExpansionSearchButton = $('reset-expansion-search-button');
  const presetSearchInput = $('preset-search-input');
  const resetPresetSearchButton = $('reset-preset-search-button');
  const favoriteViewToggle = $('favorite-view-toggle');
  if (expansionSearchInput) {
    expansionSearchInput.addEventListener('input', (e) => {
      expansionSearchQuery = e.target.value || '';
      renderExpansionSearch();
    });
    expansionSearchInput.addEventListener('focus', (e) => {
      trackFocus(e.target);
      if (e.target.value) {
        requestAnimationFrame(() => {
          try {
            e.target.select();
          } catch (_err) {}
        });
      }
    });
  }
  if (resetExpansionSearchButton && expansionSearchInput) {
    resetExpansionSearchButton.addEventListener('click', () => {
      expansionSearchQuery = '';
      expansionSearchInput.value = '';
      trackFocus(expansionSearchInput);
      renderExpansionSearch();
      requestAnimationFrame(() => {
        focusTarget(expansionSearchInput, false);
      });
    });
  }
  if (presetSearchInput) {
    presetSearchInput.addEventListener('input', (e) => {
      presetSearchQuery = e.target.value || '';
      renderOpenExpansionDetails();
    });
    presetSearchInput.addEventListener('focus', (e) => {
      trackFocus(e.target);
      if (e.target.value) {
        requestAnimationFrame(() => {
          try {
            e.target.select();
          } catch (_err) {}
        });
      }
    });
  }
  if (resetPresetSearchButton && presetSearchInput) {
    resetPresetSearchButton.addEventListener('click', () => {
      presetSearchQuery = '';
      presetSearchInput.value = '';
      trackFocus(presetSearchInput);
      renderOpenExpansionDetails();
      requestAnimationFrame(() => {
        focusTarget(presetSearchInput, false);
      });
    });
  }
  if (favoriteViewToggle) {
    favoriteViewToggle.addEventListener('click', () => {
      trackFocus(favoriteViewToggle);
      // The view wanted: the other one, or null where the view was not read.
      perform('toggle_expansion_favorite_view', {
        enabled: expansionFavoriteView == null ? null : !expansionFavoriteView
      });
    });
  }

  // --- Suppress AppKit beep on Enter / Space over buttons -----------
  // WKWebView hands keydown events to the DOM, but if AppKit doesn't
  // see the event consumed by something in its own responder chain
  // it plays the system "funk" beep. Buttons activate via Enter/Space
  // through WebKit's default action — the click handler fires, but
  // the keydown still bubbles up to AppKit unconsumed. We swallow the
  // event ourselves and call click() explicitly so the action still
  // runs and no beep is produced. (Logic's own Enter shortcut still
  // works — Logic only sees keypresses when it's frontmost.)
  document.addEventListener('keydown', (e) => {
    const t = e.target;
    if (!t || t.tagName !== 'BUTTON') return;
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      e.stopPropagation();
      t.click();
    }
  });

  // Restore focus to whatever the user last interacted with. Falls back
  // to the preset name if the tracked element is no longer in the DOM
  // (to the browser's group, for something in the browser).
  function restoreFocus() {
    const el = lastFocusedId ? $(lastFocusedId) : lastFocusedEl;
    const contentFallback = $('content-browser-group');
    const presetFallback = $('preset-name');
    const fallback = (
      el && typeof el.closest === 'function' &&
      el.closest('#content-browser-group')
    ) ? contentFallback : presetFallback;
    const target = (el && document.contains(el) && !el.disabled &&
                    el.offsetParent !== null) ? el : fallback;
    return focusTarget(target);
  }

  // --- module → page ---
  // Every message is data: the state, whole; an error for the error region; where the focus goes;
  // that it goes back where it was; or the settings. Then the page asks for its state and its
  // settings: it starts with nothing.
  if (window.platform) {
    window.platform.onmessage = (message) => {
      if (!message || typeof message !== 'object') return;
      if (message.kind === 'state') applyState(message.state || {});
      else if (message.kind === 'error') showError(message.text, message.focus === true);
      else if (message.kind === 'focus') focusElementById(message.id);
      else if (message.kind === 'restoreFocus') restoreFocus();
      else if (message.kind === 'settings') applySettings(message);
    };
    post({ action: 'ready' });
  } else {
    // Opened in a browser, to be looked at: no module, and nothing read.
    applyState({ osc_panel: {}, macro_panel: {} });
  }
})();
