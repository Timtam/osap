-- The overlay runtime's own reasons: every error value the runtime answers a module with is made
-- from one of these (main.luau's `fail`), as every one of the host's is made from a Reason of
-- crates/host/src/errors.rs. One reason is a code, a message — `…` standing for each part filled in
-- — and the docs entries that give it; tools/api-index.py reads them from here and holds each
-- entry's Errors table to them, in both directions. An error the runtime only passes on — a
-- capture's, a page's, a focus's — is the host's, and its reasons are not listed here: main.luau
-- hands each on through `handOn(entries, from, err)`, which names the entry and the host entry it
-- came from, and the tool reads those calls and holds each entry's Errors table to them, a row
-- linking to each host entry and to no other.
--
-- `codes` are the runtime's eight codes, which O.errors holds beside the host's (docs/api/errors.md,
-- "The overlay runtime's codes"); every reason's code is one of them or one of host.errors's.
--
-- Each reason is written on its own, `name = { code = "…", message = "…", entries = { … } },`, which
-- is the one form the tool reads.
return {
  codes = { "not_in_front", "moved_on", "unplaced", "closed", "no_menu", "no_submenu", "unfit_submenu", "clash" },

  reasons = {
    -- ── Placing: O:toScreen, O:toScreenRect, and what the capture methods place ──
    no_origin = {
      code = "unplaced",
      message = "the overlay has no origin now",
      entries = {
        "o-toscreen", "o-toscreenrect", "o-calibrationshot", "o-calibrationverify",
        "o-capturecontrol", "o-captureregion", "o-capturefull", "o-captureall",
      },
    },
    scale_raised = {
      code = "unplaced",
      message = "its scale function raised: …",
      entries = { "o-toscreen", "o-toscreenrect", "o-capturecontrol", "o-captureregion", "o-captureall" },
    },
    scale_nil = {
      code = "unplaced",
      message = "its scale function answered nil",
      entries = { "o-toscreen", "o-toscreenrect", "o-capturecontrol", "o-captureregion", "o-captureall" },
    },
    scale_bad = {
      code = "unplaced",
      message = "its scale function answered …, not a factor above 0",
      entries = { "o-toscreen", "o-toscreenrect", "o-capturecontrol", "o-captureregion", "o-captureall" },
    },
    empty_on_screen = { code = "outside", message = "empty on screen", entries = { "o-toscreenrect" } },
    region_raised = {
      code = "unplaced",
      message = "its … function raised: …",
      entries = { "o-capturecontrol" },
    },
    region_nil = {
      code = "unplaced",
      message = "its … function answered nil",
      entries = { "o-capturecontrol" },
    },
    region_bad = {
      code = "unplaced",
      message = "its … function answered …, not {x1, y1, x2, y2}",
      entries = { "o-capturecontrol" },
    },

    -- ── O:stillHere, in the order it asks; the first three also where a press waited ──
    no_longer_active = {
      code = "not_in_front",
      message = "the overlay is no longer active",
      entries = { "o-stillhere", "o-choosemenuitem", "trip-aftermenu" },
    },
    came_back = {
      code = "moved_on",
      message = "the overlay left the front and came back since",
      entries = { "o-stillhere", "o-choosemenuitem", "trip-aftermenu" },
    },
    other_window = {
      code = "moved_on",
      message = "the overlay is on another window now",
      entries = { "o-stillhere", "o-choosemenuitem", "trip-aftermenu" },
    },
    -- The runtime's own question (an OCR button's click), which no module can ask: it reaches none.
    control_gone = { code = "gone", message = "the control is not in the overlay any more", entries = {} },
    focus_moved = { code = "moved_on", message = "the focus has moved on", entries = { "o-stillhere" } },
    later_key = {
      code = "moved_on",
      message = "a later key reached the overlay",
      entries = { "o-stillhere" },
    },
    later_said = {
      code = "moved_on",
      message = "a later announcement was made",
      entries = { "o-stillhere" },
    },
    later_spoken = {
      code = "moved_on",
      message = "a later announcement was spoken",
      entries = { "o-stillhere" },
    },
    menu_open = {
      code = "moved_on",
      message = "a menu is open over the overlay",
      entries = { "o-stillhere" },
    },
    menu_since = {
      code = "moved_on",
      message = "a menu opened over the overlay since",
      entries = { "o-stillhere" },
    },

    -- ── The check every click of the overlay gets ──
    outside_origin = {
      code = "outside",
      message = "'…' resolves to (…,…), outside its origin's window (…,…) …x… — not clicking",
      entries = { "o-choosemenuitem" },
    },
    covered_point = {
      code = "covered",
      message = "'…' at (…,…) is inside our window's frame but another window is drawn there, or the window is gone — not clicking",
      entries = { "o-choosemenuitem" },
    },

    -- ── O:chooseMenuItem, at the call ──
    pick_not_in_front = { code = "not_in_front", message = "the overlay is not in front", entries = { "o-choosemenuitem" } },
    pick_busy = {
      code = "busy",
      message = "pressed again while its menu is still being waited for — the opener is not clicked again; the first press stands",
      entries = { "o-choosemenuitem" },
    },
    opener_unplaced = {
      code = "unplaced",
      message = "'…' cannot be placed now — …; nothing clicked",
      entries = { "o-choosemenuitem" },
    },

    -- ── O:chooseMenuItem, later: onDone ──
    pick_no_menu = {
      code = "no_menu",
      message = "no menu test saw a menu within … ms of the press",
      entries = { "o-choosemenuitem" },
    },
    pick_closed = { code = "closed", message = "the menu closed before a test placed the item", entries = { "o-choosemenuitem" } },
    pick_outside = {
      code = "no_menu",
      message = "the item, at …, is outside the menu that was seen",
      entries = { "o-choosemenuitem" },
    },
    pick_outside_closed = {
      code = "closed",
      message = "the item, at …, is outside the menu that was seen",
      entries = { "o-choosemenuitem" },
    },
    pick_other = {
      code = "no_menu",
      message = "its menuItem function took no window seen for its menu (the last at …)",
      entries = { "o-choosemenuitem" },
    },
    pick_other_closed = {
      code = "closed",
      message = "its menuItem function took no window seen for its menu (the last at …)",
      entries = { "o-choosemenuitem" },
    },
    pick_nowhere = {
      code = "no_menu",
      message = "test '…' sees a menu but does not say where it is, and no test that does saw it",
      entries = { "o-choosemenuitem" },
    },
    pick_nowhere_closed = {
      code = "closed",
      message = "test '…' sees a menu but does not say where it is, and no test that does saw it",
      entries = { "o-choosemenuitem" },
    },
    item_unplaced = { code = "unplaced", message = "the item cannot be placed now: …", entries = { "o-choosemenuitem" } },
    item_covered = {
      code = "covered",
      message = "(…,…) is inside the menu's window id …, but another window is drawn there, or that window is gone",
      entries = { "o-choosemenuitem" },
    },
    item_covered_listed = {
      code = "covered",
      message = "(…,…) is inside the menu's window … of the window list, but another window is drawn there, or that window is gone",
      entries = { "o-choosemenuitem" },
    },
    untaken = {
      code = "refused",
      message = "the menu was still there after … click(s) on the item at (…,…)",
      entries = { "o-choosemenuitem" },
    },
    untaken_submenu = {
      code = "refused",
      message = "the submenu has closed and its parent menu is still there after … click(s) on the item at (…,…)",
      entries = { "o-choosemenuitem" },
    },
    no_submenu = {
      code = "no_submenu",
      message = "no submenu was seen within … ms of the item's click",
      entries = { "o-choosemenuitem" },
    },
    no_submenu_closed = {
      code = "no_submenu",
      message = "the menu closed after the item's click, and no submenu was seen",
      entries = { "o-choosemenuitem" },
    },
    unfit_submenu = {
      code = "unfit_submenu",
      message = "its submenu's menuItem function took no popup seen after the item's click (the last at …)",
      entries = { "o-choosemenuitem" },
    },
    unfit_submenu_outside = {
      code = "unfit_submenu",
      message = "the submenu's item, at …, is outside the submenu that was seen",
      entries = { "o-choosemenuitem" },
    },
    unfollowed = {
      code = "not_ready",
      message = "the bound came while its test was getting ready to follow its submenu; nothing was clicked in it",
      entries = { "o-choosemenuitem" },
    },
    unfollowed_closed = {
      code = "closed",
      message = "the menu closed while its test was getting ready to follow its submenu; nothing was clicked in it",
      entries = { "o-choosemenuitem" },
    },
    moved_press = { code = "moved_on", message = "another press came first", entries = { "o-choosemenuitem" } },
    moved_key = {
      code = "moved_on",
      message = "one of the overlay's own keys came first, with no menu open",
      entries = { "o-choosemenuitem" },
    },
    moved_left = { code = "moved_on", message = "the overlay left the front first", entries = { "o-choosemenuitem" } },

    -- ── A trip: trip:afterMenu, O:inPlugin, O:pageBack ──
    menu_overlay_away = {
      code = "not_in_front",
      message = "'…' is not in front",
      entries = { "trip-aftermenu" },
    },
    trip_busy = { code = "busy", message = "a trip of the page is under way", entries = { "o-inplugin", "o-pageback" } },
    page_not_in_front = {
      code = "not_in_front",
      message = "the page does not have the keyboard",
      entries = { "o-inplugin", "o-pageback" },
    },
    nothing_held = {
      code = "not_found",
      message = "no place is held: the page came forward another way",
      entries = { "o-inplugin", "o-pageback" },
    },
    window_gone = {
      code = "gone",
      message = "the plug-in's window is gone",
      entries = { "o-inplugin", "o-pageback", "o-addpage", "trip-handover" },
    },
    not_brought = {
      code = "refused",
      message = "the plug-in's window was not brought forward",
      entries = { "o-inplugin", "o-pageback", "o-addpage" },
    },

    -- ── A trip given up: onGiveUp's error, and a hand-over's onEnd's ──
    another_application = {
      code = "moved_on",
      message = "a window of another application came to the front, or one of this application's that is not the page",
      entries = { "o-inplugin", "trip-handover" },
    },
    page_key = { code = "cancelled", message = "the page key gave the trip up", entries = { "o-inplugin", "trip-handover" } },
    page_closed = { code = "closed", message = "the page was closed", entries = { "o-inplugin", "trip-handover" } },
    not_come = {
      code = "not_in_front",
      message = "the plug-in's window did not come within … ms",
      entries = { "o-inplugin" },
    },
    no_keyboard = {
      code = "not_in_front",
      message = "the plug-in's window does not have the keyboard",
      entries = { "o-inplugin", "trip-handover" },
    },
    outranked = {
      code = "taken",
      message = "another module's overlay holds the plug-in's window",
      entries = { "o-inplugin", "trip-handover" },
    },
    hook_failed = { code = "failed", message = "… failed", entries = { "o-inplugin", "trip-handover" } },

    -- ── trip:handOver, at the call ──
    keys_held = {
      code = "busy",
      message = "trip '…' of this module holds the module's keys",
      entries = { "trip-handover" },
    },
    capture_raised = { code = "failed", message = "capturing … raised: …", entries = { "trip-handover", "o-holdkeys" } },

    -- ── O:holdKeys, at the call ──
    hold_not_in_front = { code = "not_in_front", message = "the overlay is not in front", entries = { "o-holdkeys" } },

    -- ── O:pageKey ──
    key_parse = { code = "invalid", message = "… is not a key spec", entries = { "o-pagekey" } },
    key_tap = {
      code = "invalid",
      message = "… is a modifier tap, which only host.keys.capture can watch",
      entries = { "o-pagekey" },
    },
    key_reserved = { code = "taken", message = "… is kept by the system", entries = { "o-pagekey" } },
    key_no_keycode = {
      code = "not_supported",
      message = "… has no key code of its own on this platform",
      entries = { "o-pagekey" },
    },
    key_clash = {
      code = "clash",
      message = "… is the hotkey of … of overlay '…'",
      entries = { "o-pagekey" },
    },
    register_raised = { code = "failed", message = "registering … raised: …", entries = { "o-pagekey" } },

    -- ── The calibration and capture methods ──
    not_active = {
      code = "not_in_front",
      message = "the overlay is not active",
      entries = { "o-calibrationcrop", "o-capturecontrol", "o-captureregion", "o-capturefull", "o-captureall" },
    },
    no_focus = {
      code = "not_found",
      message = "no control is focused",
      entries = { "o-calibrationcrop", "o-calibrationverify" },
    },
    no_position = {
      code = "unplaced",
      message = "the focused control has no position to crop around",
      entries = { "o-calibrationcrop" },
    },
    no_control = {
      code = "not_found",
      message = "no control labelled '…' with a region",
      entries = { "o-capturecontrol" },
    },
    no_crop = {
      code = "not_found",
      message = "no crop of '…' was written: … is not there",
      entries = { "o-calibrationverify" },
    },
  },
}
