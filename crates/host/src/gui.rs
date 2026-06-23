//! wxDragon GUI host: a **tray-resident module manager**. wxWidgets owns the
//! main message loop; our OS events are pumped from a wx `Timer` tick
//! (event-loop coexistence, see `docs/architecture-feasibility-study.md` §11.6).
//!
//! The window lists loaded modules with enable/disable checkboxes and lives in
//! the system tray: it starts hidden (modules are needed far more often than
//! their management), closing it (X) hides it back to the tray, and only the
//! tray "Quit" actually exits. Double-clicking the tray icon reopens it.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use wxdragon::prelude::*;

use crate::settings;

const MENU_SHOW: i32 = 1001;
const MENU_QUIT: i32 = 1002;

/// One module setting, as the manager window needs it to render + edit a control.
#[derive(Clone)]
pub struct SettingDesc {
    pub key: String,
    pub label: String,
    pub kind: settings::Kind,
    pub value: settings::Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub choices: Option<Vec<String>>,
}

/// One module as the manager needs it to render + manage a row.
pub struct ModuleInfo {
    pub name: String,
    pub version: String,
    pub id: String,
    /// This module's index in the manager's module list — passed back to the
    /// toggle / settings callbacks.
    pub module_idx: usize,
    pub enabled: bool,
    /// Ids this module depends on — used to block uninstalling a module that
    /// another currently-loaded module still needs.
    pub dependencies: Vec<String>,
    pub settings: Vec<SettingDesc>,
}

/// One built row in the Installed list: its tree item plus what the handlers
/// need — the module index (toggle/settings callbacks), id + name (messages),
/// its settings, and the ids it depends on (to block removing a needed module).
struct Row {
    item: TreeItemId,
    module_idx: usize,
    id: String,
    name: String,
    settings: Vec<SettingDesc>,
    dependencies: Vec<String>,
}

impl Row {
    fn new(item: TreeItemId, info: &ModuleInfo) -> Self {
        Row {
            item,
            module_idx: info.module_idx,
            id: info.id.clone(),
            name: info.name.clone(),
            settings: info.settings.clone(),
            dependencies: info.dependencies.clone(),
        }
    }
}

/// Runs the tray-resident module manager. `on_toggle(idx, enabled)` fires when
/// the user checks/unchecks a module; `pump` drains OS events each tick. Blocks
/// until the user chooses Quit.
pub fn run_gui(
    modules: Vec<ModuleInfo>,
    on_toggle: impl FnMut(usize, bool) + 'static,
    on_set: impl FnMut(usize, String, settings::Value) + 'static,
    on_install: impl Fn(std::path::PathBuf) -> Result<(String, Vec<ModuleInfo>), String> + 'static,
    on_remove: impl Fn(usize) + 'static,
    mut pump: impl FnMut() + 'static,
    mut drain_errors: impl FnMut() -> Vec<(String, String)> + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    wxdragon::main(move |app| {
        // The window is a background manager: hiding/closing it must not quit the
        // app — only the tray "Quit" does.
        app.set_exit_on_frame_delete(false);

        let frame = Frame::builder()
            .with_title("Automation Platform — Modules")
            .with_size(Size::new(560, 470))
            .build();

        let panel = Panel::builder(&frame).build();
        let sizer = BoxSizer::builder(Orientation::Vertical).build();
        let notebook = Notebook::builder(&panel).build();

        // ===== "Installed" tab =====
        let installed = Panel::builder(&notebook).build();
        let is = BoxSizer::builder(Orientation::Vertical).build();
        let heading = StaticText::builder(&installed)
            .with_label("Installed modules — uncheck to disable, check to enable:")
            .build();
        is.add(&heading, 0, SizerFlag::All, 12);

        // A native tree control with TVS_CHECKBOXES: real OS checkboxes that
        // expose the proper toggle state to the screen reader (UIA). Must be
        // enabled *before* items are inserted. Every module is listed (libraries
        // too — they just can't be removed while something needs them). The root
        // is kept so a row can be appended live when a module is hot-loaded.
        let list = TreeCtrl::builder(&installed)
            .with_style(TreeCtrlStyle::HideRoot | TreeCtrlStyle::Single | TreeCtrlStyle::NoLines)
            .build();
        let hwnd = list.get_handle();
        native_checkboxes::enable(hwnd);
        let root = list.add_root("Modules", None, None);
        let mut rows: Vec<Row> = Vec::new();
        if let Some(root) = &root {
            for m in modules.iter() {
                let label = format!("{}  v{}   ({})", m.name, m.version, m.id);
                if let Some(item) = list.append_item(root, &label, None, None) {
                    native_checkboxes::set(hwnd, &item, m.enabled);
                    rows.push(Row::new(item, m));
                }
            }
        }
        is.add(&list, 1, SizerFlag::All | SizerFlag::Expand, 12);

        let inst_buttons = BoxSizer::builder(Orientation::Horizontal).build();
        let settings_btn = Button::builder(&installed).with_label("Settings…").build();
        let uninstall_btn = Button::builder(&installed).with_label("Uninstall").build();
        inst_buttons.add(&settings_btn, 0, SizerFlag::All, 6);
        inst_buttons.add(&uninstall_btn, 0, SizerFlag::All, 6);
        is.add_sizer(&inst_buttons, 0, SizerFlag::All, 6);
        installed.set_sizer(is, true);
        notebook.add_page(&installed, "Installed", true, None);

        // ===== "Browse" + "Updates" tabs (wired up in the next step) =====
        let browse = Panel::builder(&notebook).build();
        let bs = BoxSizer::builder(Orientation::Vertical).build();
        bs.add(
            &StaticText::builder(&browse)
                .with_label("Search the module registry (public GitHub repos):")
                .build(),
            0,
            SizerFlag::Left | SizerFlag::Top,
            12,
        );
        let search = TextCtrl::builder(&browse).build();
        search.set_name("Search modules");
        bs.add(&search, 0, SizerFlag::All | SizerFlag::Expand, 12);
        let search_btn = Button::builder(&browse).with_label("Search").build();
        bs.add(&search_btn, 0, SizerFlag::All, 6);
        let browse_list = ListBox::builder(&browse).build();
        bs.add(&browse_list, 1, SizerFlag::All | SizerFlag::Expand, 12);
        let install_btn = Button::builder(&browse).with_label("Install selected").build();
        bs.add(&install_btn, 0, SizerFlag::All, 6);
        let browse_status = StaticText::builder(&browse).with_label("").build();
        bs.add(&browse_status, 0, SizerFlag::All, 6);
        browse.set_sizer(bs, true);
        notebook.add_page(&browse, "Browse", false, None);

        let updates = Panel::builder(&notebook).build();
        let us = BoxSizer::builder(Orientation::Vertical).build();
        let check_btn = Button::builder(&updates).with_label("Check for updates").build();
        us.add(&check_btn, 0, SizerFlag::All, 12);
        let updates_list = ListBox::builder(&updates).build();
        us.add(&updates_list, 1, SizerFlag::All | SizerFlag::Expand, 12);
        let update_btn = Button::builder(&updates).with_label("Update selected").build();
        us.add(&update_btn, 0, SizerFlag::All, 6);
        let updates_status = StaticText::builder(&updates).with_label("").build();
        us.add(&updates_status, 0, SizerFlag::All, 6);
        updates.set_sizer(us, true);
        notebook.add_page(&updates, "Updates", false, None);

        sizer.add(&notebook, 1, SizerFlag::All | SizerFlag::Expand, 0);
        let hint = StaticText::builder(&panel)
            .with_label("Closing this window hides it to the tray; modules keep running. Quit from the tray icon.")
            .build();
        sizer.add(&hint, 0, SizerFlag::All, 12);
        panel.set_sizer(sizer, true);

        // Detect native checkbox toggles (mouse click on the box, or Space on the
        // focused row). The native control flips state on the *down* event, so by
        // the *up* event the new state is in place; diff each row against the
        // last-known and report via the real module index.
        let states = Rc::new(RefCell::new(
            rows.iter().map(|r| modules[r.module_idx].enabled).collect::<Vec<bool>>(),
        ));
        let rows = Rc::new(RefCell::new(rows));
        let on_toggle: Rc<RefCell<Box<dyn FnMut(usize, bool)>>> =
            Rc::new(RefCell::new(Box::new(on_toggle)));
        settings_btn.enable(false); // refreshed on selection (mouse-up / key-up)
        {
            let (rows, states, on_toggle) = (rows.clone(), states.clone(), on_toggle.clone());
            list.on_mouse_left_up(move |e| {
                sync_checks(hwnd, &rows.borrow(), &states, &on_toggle);
                refresh_settings_btn(&list, &rows.borrow(), &settings_btn);
                e.skip(true);
            });
        }
        {
            let (rows, states, on_toggle) = (rows.clone(), states.clone(), on_toggle.clone());
            list.on_key_up(move |e| {
                sync_checks(hwnd, &rows.borrow(), &states, &on_toggle);
                refresh_settings_btn(&list, &rows.borrow(), &settings_btn);
                e.skip(true);
            });
        }

        // "Settings…" opens a per-module dialog with native controls.
        let on_set: Rc<RefCell<Box<dyn FnMut(usize, String, settings::Value)>>> =
            Rc::new(RefCell::new(Box::new(on_set)));
        {
            let (rows, on_set) = (rows.clone(), on_set.clone());
            settings_btn.on_click(move |_| {
                let Some(sel) = list.get_selection() else {
                    return;
                };
                let found = {
                    let rb = rows.borrow();
                    rb.iter()
                        .find(|r| native_checkboxes::same(&r.item, &sel))
                        .map(|r| (r.module_idx, r.settings.clone()))
                };
                let Some((module_idx, settings)) = found else {
                    return;
                };
                if settings.is_empty() {
                    modal_message(&frame, "Settings", "This module has no settings.", false);
                    return;
                }
                open_settings_dialog(&frame, module_idx, &settings, &on_set);
            });
        }

        // "Uninstall" removes the selected module's files AND disables it in the
        // running app immediately (revoking its hotkeys/keys/triggers) + drops its
        // row — no restart needed. A module another currently-loaded module
        // depends on can't be removed.
        {
            let (rows, states, settings_btn) = (rows.clone(), states.clone(), settings_btn);
            uninstall_btn.on_click(move |_| {
                let Some(sel) = list.get_selection() else {
                    return;
                };
                let found = {
                    let rb = rows.borrow();
                    let graph: Vec<(String, Vec<String>)> =
                        rb.iter().map(|x| (x.id.clone(), x.dependencies.clone())).collect();
                    rb.iter().find(|r| native_checkboxes::same(&r.item, &sel)).map(|r| {
                        // Block if any loaded module *transitively* depends on this one —
                        // removing it would break them (not just its direct dependents).
                        let needed_by: Vec<String> =
                            crate::registry::transitive_dependents(&r.id, &graph)
                                .iter()
                                .filter_map(|did| rb.iter().find(|x| &x.id == did))
                                .map(|x| format!("{} ({})", x.name, x.id))
                                .collect();
                        (r.id.clone(), r.module_idx, needed_by)
                    })
                };
                let Some((id, module_idx, needed_by)) = found else {
                    return;
                };
                if !needed_by.is_empty() {
                    modal_message(
                        &frame,
                        "Can't uninstall",
                        &format!(
                            "\u{201c}{id}\u{201d} is required by:\n\u{2022} {}\n\nRemove those \
                             modules first.",
                            needed_by.join("\n\u{2022} ")
                        ),
                        false,
                    );
                    return;
                }
                if !modal_message(
                    &frame,
                    "Uninstall module",
                    &format!(
                        "Remove \u{201c}{id}\u{201d}? It is disabled immediately and won't load \
                         after a restart."
                    ),
                    true,
                ) {
                    return;
                }
                let msg = match crate::registry::uninstall(&id) {
                    Ok(true) => {
                        on_remove(module_idx); // revoke its hotkeys/keys/triggers now
                        // Dependencies only this module pulled in, now needed by nothing
                        // else (cascading) — offer to remove them too. Computed from the
                        // graph that still includes the module being removed.
                        let orphan_ids: Vec<String> = {
                            let rb = rows.borrow();
                            let graph: Vec<(String, Vec<String>)> =
                                rb.iter().map(|x| (x.id.clone(), x.dependencies.clone())).collect();
                            crate::registry::orphaned_by(std::slice::from_ref(&id), &graph)
                                .into_iter()
                                .filter(|oid| rb.iter().any(|x| &x.id == oid))
                                .collect()
                        };
                        let pos = rows.borrow().iter().position(|r| r.id == id);
                        if let Some(pos) = pos {
                            let removed = rows.borrow_mut().remove(pos);
                            states.borrow_mut().remove(pos);
                            list.delete(&removed.item);
                        }
                        settings_btn.enable(false);
                        if !orphan_ids.is_empty()
                            && modal_message(
                                &frame,
                                "Remove unused dependencies?",
                                &format!(
                                    "These were only needed by \u{201c}{id}\u{201d} and are now \
                                     unused:\n\u{2022} {}\n\nRemove them too?",
                                    orphan_ids.join("\n\u{2022} ")
                                ),
                                true,
                            )
                        {
                            for oid in &orphan_ids {
                                let _ = crate::registry::uninstall(oid);
                                let pos = rows.borrow().iter().position(|r| &r.id == oid);
                                if let Some(p) = pos {
                                    let removed = rows.borrow_mut().remove(p);
                                    states.borrow_mut().remove(p);
                                    on_remove(removed.module_idx);
                                    list.delete(&removed.item);
                                }
                            }
                            format!(
                                "Uninstalled \u{201c}{id}\u{201d} and {} now-unused dependenc{}.",
                                orphan_ids.len(),
                                if orphan_ids.len() == 1 { "y" } else { "ies" }
                            )
                        } else {
                            format!("Uninstalled and disabled \u{201c}{id}\u{201d}.")
                        }
                    }
                    Ok(false) => format!("\u{201c}{id}\u{201d} has no installed files to remove."),
                    Err(e) => format!("Could not uninstall {id}: {e}"),
                };
                modal_message(&frame, "Uninstall", &msg, false);
            });
        }

        // ----- Browse / Updates: background HTTP (GitHub) delivered to the GUI
        // thread via a shared inbox drained on the timer tick (wxdragon has no
        // CallAfter). Threads only move owned data; controls stay on this thread.
        enum Job {
            Browse(Vec<crate::registry::RemoteModule>),
            BrowseStatus(String),
            Updates(Vec<(String, String)>), // (module id, repo)
            Installed(std::path::PathBuf),  // hot-load a freshly installed module
            Done(String),                   // a modal result message
        }
        let inbox: Arc<Mutex<Vec<Job>>> = Arc::new(Mutex::new(Vec::new()));
        let browse_results: Rc<RefCell<Vec<crate::registry::RemoteModule>>> =
            Rc::new(RefCell::new(Vec::new()));
        let update_results: Rc<RefCell<Vec<(String, String)>>> = Rc::new(RefCell::new(Vec::new()));
        // True while an install OR update is running: the two are mutually
        // exclusive (both write the same modules dir), so both buttons are
        // disabled together and only re-enabled when the one in flight reports back.
        let busy = Rc::new(Cell::new(false));

        // Search.
        {
            let (inbox, browse_status) = (inbox.clone(), browse_status);
            search_btn.on_click(move |_| {
                let query = search.get_value();
                browse_status.set_label("Searching…");
                let inbox = inbox.clone();
                std::thread::spawn(move || {
                    let job = match crate::registry::search(query.trim()) {
                        Ok(v) => Job::Browse(v),
                        Err(e) => Job::BrowseStatus(format!("Search failed: {e}")),
                    };
                    inbox.lock().unwrap().push(job);
                });
            });
        }

        // Install selected (review capabilities first).
        {
            let (browse_results, browse_status, inbox, busy) =
                (browse_results.clone(), browse_status, inbox.clone(), busy.clone());
            install_btn.on_click(move |_| {
                if busy.get() {
                    return;
                }
                let Some(row) = browse_list.get_selection() else {
                    return;
                };
                let (full_name, branch) = {
                    let results = browse_results.borrow();
                    let Some(rm) = results.get(row as usize) else {
                        return;
                    };
                    (rm.full_name.clone(), rm.default_branch.clone())
                };
                if crate::registry::installed().iter().any(|m| {
                    m.source.as_ref().map(|s| s.repo.as_str()) == Some(full_name.as_str())
                }) {
                    modal_message(
                        &frame,
                        "Already installed",
                        &format!(
                            "\u{201c}{full_name}\u{201d} is already installed. Use the Updates \
                             tab to upgrade it, or uninstall it first."
                        ),
                        false,
                    );
                    return;
                }
                // Lock install + update for the whole flow (manifest fetch, the
                // confirm modal, the download) so neither can be started again.
                busy.set(true);
                install_btn.enable(false);
                update_btn.enable(false);
                browse_status.set_label("Fetching manifest…");
                let manifest = match crate::registry::fetch_manifest(&full_name, &branch) {
                    Ok(m) => m,
                    Err(e) => {
                        browse_status.set_label(&format!("Failed: {e}"));
                        busy.set(false);
                        update_btn.enable(true);
                        refresh_install_btn(
                            &browse_list,
                            &browse_results.borrow(),
                            false,
                            &install_btn,
                        );
                        return;
                    }
                };
                browse_status.set_label("");
                let caps = &manifest.capabilities.require;
                let caps_str = if caps.is_empty() {
                    "(none)".to_string()
                } else {
                    caps.join("\n\u{2022} ")
                };
                let msg = format!(
                    "\u{201c}{}\u{201d} v{} ({})\n\nRequested capabilities:\n\u{2022} {}\n\nInstall this module?",
                    manifest.name, manifest.version, manifest.id, caps_str
                );
                if !modal_message(&frame, "Review capabilities", &msg, true) {
                    busy.set(false);
                    update_btn.enable(true);
                    refresh_install_btn(
                        &browse_list,
                        &browse_results.borrow(),
                        false,
                        &install_btn,
                    );
                    return;
                }
                browse_status.set_label(&format!("Installing {full_name}…"));
                let inbox = inbox.clone();
                std::thread::spawn(move || {
                    // Always deliver a result, even on an unexpected panic, so the
                    // buttons can never get stuck disabled.
                    let job = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // Install the whole dependency tree, not just this repo; the
                        // hot-load resolves the now-installed deps as siblings.
                        match crate::registry::install_tree(&full_name) {
                            Ok(_) => {
                                let repo = full_name.rsplit('/').next().unwrap_or(&full_name);
                                Job::Installed(crate::registry::modules_dir().join(repo))
                            }
                            Err(e) => Job::Done(format!("Install failed: {e}")),
                        }
                    }))
                    .unwrap_or_else(|_| Job::Done("Install failed (internal error).".to_string()));
                    inbox.lock().unwrap().push(job);
                });
            });
        }

        // Grey the Install button unless a not-yet-installed result is selected.
        install_btn.enable(false);
        {
            let (browse_results, busy) = (browse_results.clone(), busy.clone());
            browse_list.on_selection_changed(move |_| {
                refresh_install_btn(&browse_list, &browse_results.borrow(), busy.get(), &install_btn);
            });
        }

        // Check for updates.
        {
            let (inbox, updates_status) = (inbox.clone(), updates_status);
            check_btn.on_click(move |_| {
                updates_status.set_label("Checking…");
                let inbox = inbox.clone();
                std::thread::spawn(move || {
                    let mut updatable = Vec::new();
                    for m in crate::registry::installed() {
                        if crate::registry::update_available(&m).is_some() {
                            if let Some(src) = &m.source {
                                updatable.push((m.id.clone(), src.repo.clone()));
                            }
                        }
                    }
                    inbox.lock().unwrap().push(Job::Updates(updatable));
                });
            });
        }

        // Update selected.
        {
            let (update_results, updates_status, inbox, busy) =
                (update_results.clone(), updates_status, inbox.clone(), busy.clone());
            update_btn.on_click(move |_| {
                if busy.get() {
                    return;
                }
                let Some(row) = updates_list.get_selection() else {
                    return;
                };
                let Some((id, repo)) = update_results.borrow().get(row as usize).cloned() else {
                    return;
                };
                busy.set(true);
                install_btn.enable(false);
                update_btn.enable(false);
                updates_status.set_label(&format!("Updating {id}…"));
                let inbox = inbox.clone();
                std::thread::spawn(move || {
                    let job = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        match crate::registry::install(&repo) {
                            Ok(m) => {
                                // Modules that inherit this one hold a copy of its code
                                // and reload it on the next start (the cascade is via
                                // restart for now — no live hot-reload yet).
                                let graph: Vec<(String, Vec<String>)> = crate::registry::installed()
                                    .into_iter()
                                    .map(|x| (x.id, x.dependencies))
                                    .collect();
                                let deps = crate::registry::transitive_dependents(&m.id, &graph);
                                let extra = if deps.is_empty() {
                                    String::new()
                                } else {
                                    format!(" Dependents reload with it: {}.", deps.join(", "))
                                };
                                Job::Done(format!("Updated {}. Restart to apply.{extra}", m.id))
                            }
                            Err(e) => Job::Done(format!("Update failed: {e}")),
                        }
                    }))
                    .unwrap_or_else(|_| Job::Done("Update failed (internal error).".to_string()));
                    inbox.lock().unwrap().push(job);
                });
            });
        }

        // Close (X) hides to tray instead of quitting.
        frame.on_close(move |event| {
            if let WindowEventData::General(e) = &event {
                e.veto();
            }
            frame.show(false);
        });

        // System tray icon + right-click menu (Show / Quit).
        let taskbar = TaskBarIcon::builder().build();
        if let Some(icon) = make_icon() {
            taskbar.set_icon(&icon, "Automation Platform");
        }
        let mut menu = Menu::builder()
            .append_item(MENU_SHOW, "Show module manager", "Show the module window")
            .append_separator()
            .append_item(MENU_QUIT, "Quit", "Quit Automation Platform")
            .build();
        taskbar.set_popup_menu(&mut menu);
        std::mem::forget(menu); // the tray icon owns it for the app's lifetime

        taskbar.on_menu(move |event| match event.get_id() {
            MENU_SHOW => {
                frame.show(true);
                frame.centre();
            }
            MENU_QUIT => app.exit_main_loop(),
            _ => {}
        });

        #[cfg(any(target_os = "windows", target_os = "linux"))]
        taskbar.on_left_double_click(move |_| {
            frame.show(true);
            frame.centre();
        });

        let _ = taskbar.show_balloon(
            "Automation Platform",
            "Running in the system tray. Double-click the tray icon to manage modules.",
            0,
            0,
            None,
        );
        std::mem::forget(taskbar); // keep the icon + its handlers alive

        // wxWidgets owns the loop now, so this recurring tick is how our OS events
        // (hotkeys / captured keys / foreground changes) reach Luau. The init
        // closure returns *before* the loop runs, so the timer must outlive it —
        // leak it for the app's lifetime (its Drop would stop the wxTimer).
        let timer = Timer::new(&frame);
        {
            let inbox = inbox.clone();
            let (rows, states, busy) = (rows.clone(), states.clone(), busy.clone());
            // Re-entrancy guard: the modal dialogs below run a NESTED wx event loop,
            // during which this continuous timer keeps firing and re-enters on_tick.
            // Bail on re-entry so we don't pump module callbacks — or stack further
            // dialogs — behind an already-open modal.
            let in_tick = Rc::new(std::cell::Cell::new(false));
            timer.on_tick(move |_event| {
                if in_tick.replace(true) {
                    return;
                }
                struct Reset<'a>(&'a std::cell::Cell<bool>);
                impl Drop for Reset<'_> {
                    fn drop(&mut self) {
                        self.0.set(false);
                    }
                }
                let _reset = Reset(&in_tick);
                pump();
                // Surface any module callback failures collected during pump() in an
                // accessible dialog (deduped + queued host-side), so a faulting module
                // is visible (and isolated) rather than silently logged or a crash.
                for (title, msg) in drain_errors() {
                    modal_message(&frame, &title, &msg, false);
                }
                // Drain background-job results and apply them on the GUI thread.
                let jobs: Vec<Job> = std::mem::take(&mut *inbox.lock().unwrap());
                for job in jobs {
                    match job {
                        Job::Browse(v) => {
                            browse_list.clear();
                            let installed: std::collections::HashSet<String> =
                                crate::registry::installed()
                                    .into_iter()
                                    .filter_map(|m| m.source.map(|s| s.repo))
                                    .collect();
                            for r in &v {
                                let desc = if r.description.is_empty() {
                                    String::new()
                                } else {
                                    format!(" — {}", r.description)
                                };
                                let tag = if installed.contains(&r.full_name) {
                                    "  — installed"
                                } else {
                                    ""
                                };
                                browse_list.append(&format!(
                                    "{}{}  ({} stars){}",
                                    r.full_name, desc, r.stars, tag
                                ));
                            }
                            browse_status.set_label(&format!("{} result(s).", v.len()));
                            *browse_results.borrow_mut() = v;
                            refresh_install_btn(
                                &browse_list,
                                &browse_results.borrow(),
                                busy.get(),
                                &install_btn,
                            );
                        }
                        Job::BrowseStatus(s) => browse_status.set_label(&s),
                        Job::Updates(v) => {
                            updates_list.clear();
                            for (id, repo) in &v {
                                updates_list.append(&format!("{id}  ({repo})"));
                            }
                            updates_status.set_label(&format!("{} update(s) available.", v.len()));
                            *update_results.borrow_mut() = v;
                        }
                        Job::Installed(dir) => {
                            // Re-enable + clear busy up front, so even a panic in
                            // on_install can't leave the buttons stuck disabled.
                            busy.set(false);
                            update_btn.enable(true);
                            refresh_install_btn(
                                &browse_list,
                                &browse_results.borrow(),
                                false,
                                &install_btn,
                            );
                            let msg = match on_install(dir) {
                                Ok((id, infos)) if !infos.is_empty() => {
                                    // List every newly loaded module (a hot-load can
                                    // pull in not-yet-loaded dependencies), so each
                                    // is immediately toggleable / removable.
                                    if let Some(root) = &root {
                                        for info in &infos {
                                            let label = format!(
                                                "{}  v{}   ({})",
                                                info.name, info.version, info.id
                                            );
                                            if let Some(item) =
                                                list.append_item(root, &label, None, None)
                                            {
                                                native_checkboxes::set(hwnd, &item, info.enabled);
                                                states.borrow_mut().push(info.enabled);
                                                rows.borrow_mut().push(Row::new(item, info));
                                            }
                                        }
                                    }
                                    format!(
                                        "Installed and loaded \u{201c}{id}\u{201d} — it's running \
                                         now and listed below."
                                    )
                                }
                                Ok((id, _)) => format!(
                                    "Installed \u{201c}{id}\u{201d} to disk, but a copy is already \
                                     running. Restart to apply the update."
                                ),
                                Err(e) => format!(
                                    "Installed, but loading failed:\n{e}\n\nRestart the app to retry."
                                ),
                            };
                            modal_message(&frame, "Installed", &msg, false);
                        }
                        Job::Done(msg) => {
                            busy.set(false);
                            update_btn.enable(true);
                            refresh_install_btn(
                                &browse_list,
                                &browse_results.borrow(),
                                false,
                                &install_btn,
                            );
                            modal_message(&frame, "Modules", &msg, false);
                        }
                    }
                }
            });
        }
        timer.start(15, false);
        std::mem::forget(timer);

        // Window starts hidden: the manager lives in the background.
    })
}

/// Generates a simple 32×32 RGBA tray icon (a filled accent dot on transparency).
fn make_icon() -> Option<Bitmap> {
    let size: u32 = 32;
    let mut data = vec![0u8; (size * size * 4) as usize];
    let c = (size as f32 - 1.0) / 2.0;
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            if (dx * dx + dy * dy).sqrt() <= c {
                let i = ((y * size + x) * 4) as usize;
                data[i] = 0x2E;
                data[i + 1] = 0xA8;
                data[i + 2] = 0x9F;
                data[i + 3] = 0xFF;
            }
        }
    }
    Bitmap::from_rgba(&data, size, size)
}

/// Formats a numeric setting value for a text field (no trailing `.0` for ints).
fn number_to_string(v: &settings::Value) -> String {
    match v {
        settings::Value::Int(i) => i.to_string(),
        settings::Value::Float(f) => f.to_string(),
        _ => "0".to_string(),
    }
}

/// A modal message dialog whose body text is screen-reader-accessible: the
/// message lives in a focused, read-only multiline text control. (A bare
/// StaticText isn't focusable, so a wxMessageDialog's body is only reachable by
/// object navigation — this is read aloud on open.) Returns whether the user
/// confirmed (Yes); an OK-only dialog always returns true.
fn modal_message(parent: &Frame, title: &str, message: &str, yes_no: bool) -> bool {
    let dialog = Dialog::builder(parent, title).build();
    let panel = Panel::builder(&dialog).build();
    let sizer = BoxSizer::builder(Orientation::Vertical).build();

    let text = TextCtrl::builder(&panel)
        .with_style(TextCtrlStyle::MultiLine | TextCtrlStyle::ReadOnly)
        .build();
    text.set_value(message);
    text.set_name(title);
    // Size to the content (~62 chars per 440px line) so a longer message isn't
    // crammed into a narrow, heavily-wrapped box.
    let lines: usize = message
        .lines()
        .map(|l| (l.chars().count().saturating_sub(1) / 62) + 1)
        .sum::<usize>()
        .max(1);
    text.set_min_size(Size::new(440, ((lines as i32) * 20 + 36).clamp(70, 380)));
    sizer.add(&text, 1, SizerFlag::All | SizerFlag::Expand, 12);

    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    if yes_no {
        let yes = Button::builder(&panel).with_label("Yes").build();
        let no = Button::builder(&panel).with_label("No").build();
        {
            let d = dialog;
            yes.on_click(move |_| d.end_modal(ID_YES));
        }
        {
            let d = dialog;
            no.on_click(move |_| d.end_modal(ID_NO));
        }
        buttons.add(&yes, 0, SizerFlag::All, 6);
        buttons.add(&no, 0, SizerFlag::All, 6);
    } else {
        let ok = Button::builder(&panel).with_label("OK").build();
        {
            let d = dialog;
            ok.on_click(move |_| d.end_modal(ID_OK));
        }
        buttons.add(&ok, 0, SizerFlag::All, 6);
    }
    sizer.add_sizer(&buttons, 0, SizerFlag::AlignRight | SizerFlag::All, 6);

    panel.set_sizer(sizer, true);
    let dlg_sizer = BoxSizer::builder(Orientation::Vertical).build();
    dlg_sizer.add(&panel, 1, SizerFlag::Expand, 0);
    dialog.set_sizer_and_fit(dlg_sizer, true);

    text.set_focus(); // read the message aloud when the dialog opens
    let res = dialog.show_modal();
    dialog.destroy();
    if yes_no {
        res == ID_YES
    } else {
        true
    }
}

/// Opens a modal settings dialog for one module, building a native control per
/// setting (checkbox / number field / dropdown / text). On OK, applies each via `on_set`.
fn open_settings_dialog(
    parent: &Frame,
    idx: usize,
    descs: &[SettingDesc],
    on_set: &RefCell<Box<dyn FnMut(usize, String, settings::Value)>>,
) {
    enum Ctl {
        Bool(CheckBox),
        Num(TextCtrl, bool, Option<f64>, Option<f64>), // (field, integral, min, max)
        Choice(Choice, Vec<String>),
        Text(TextCtrl),
    }

    let dialog = Dialog::builder(parent, "Module settings").build();
    // Controls go on a Panel (not the bare Dialog): standard wxWidgets practice,
    // and it gives the screen reader correct control labeling + Tab navigation.
    let panel = Panel::builder(&dialog).build();
    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    let mut controls: Vec<(String, Ctl)> = Vec::with_capacity(descs.len());

    for d in descs {
        match d.kind {
            settings::Kind::Bool => {
                // Label it exactly like the other rows: a leading StaticText is
                // what the screen reader reliably reads as the control's name here
                // (the checkbox's own label was not announced).
                let lbl = StaticText::builder(&panel).with_label(&d.label).build();
                sizer.add(&lbl, 0, SizerFlag::Left | SizerFlag::Top, 8);
                let cb = CheckBox::builder(&panel).build();
                cb.set_name(&d.label);
                cb.set_value(matches!(d.value, settings::Value::Bool(true)));
                sizer.add(&cb, 0, SizerFlag::All, 8);
                controls.push((d.key.clone(), Ctl::Bool(cb)));
            }
            settings::Kind::Number => {
                // A labeled text field — a composite spinner doesn't expose an
                // accessible name to the screen reader; the range is enforced on OK.
                let lbl = StaticText::builder(&panel).with_label(&d.label).build();
                sizer.add(&lbl, 0, SizerFlag::Left | SizerFlag::Top, 8);
                let tc = TextCtrl::builder(&panel).build();
                tc.set_name(&d.label); // SetLabel() asserts on a TextCtrl; rely on
                                       // the preceding StaticText + the window name
                tc.set_value(&number_to_string(&d.value));
                sizer.add(&tc, 0, SizerFlag::All | SizerFlag::Expand, 8);
                let integral = matches!(d.value, settings::Value::Int(_));
                controls.push((d.key.clone(), Ctl::Num(tc, integral, d.min, d.max)));
            }
            settings::Kind::Str => {
                let lbl = StaticText::builder(&panel).with_label(&d.label).build();
                sizer.add(&lbl, 0, SizerFlag::Left | SizerFlag::Top, 8);
                if let Some(choices) = &d.choices {
                    let ch = Choice::builder(&panel).build();
                    ch.set_name(&d.label);
                    let mut sel = 0u32;
                    for (i, c) in choices.iter().enumerate() {
                        ch.append(c);
                        if d.value.as_str() == Some(c.as_str()) {
                            sel = i as u32;
                        }
                    }
                    ch.set_selection(sel);
                    sizer.add(&ch, 0, SizerFlag::All, 8);
                    controls.push((d.key.clone(), Ctl::Choice(ch, choices.clone())));
                } else {
                    let tc = TextCtrl::builder(&panel).build();
                    tc.set_name(&d.label);
                    tc.set_value(d.value.as_str().unwrap_or(""));
                    sizer.add(&tc, 0, SizerFlag::All | SizerFlag::Expand, 8);
                    controls.push((d.key.clone(), Ctl::Text(tc)));
                }
            }
        }
    }

    // OK / Cancel — end the modal with the standard ids.
    let buttons = BoxSizer::builder(Orientation::Horizontal).build();
    let ok = Button::builder(&panel).with_label("OK").build();
    let cancel = Button::builder(&panel).with_label("Cancel").build();
    {
        let d = dialog;
        ok.on_click(move |_| d.end_modal(ID_OK));
    }
    {
        let d = dialog;
        cancel.on_click(move |_| d.end_modal(ID_CANCEL));
    }
    buttons.add(&ok, 0, SizerFlag::All, 6);
    buttons.add(&cancel, 0, SizerFlag::All, 6);
    sizer.add_sizer(&buttons, 0, SizerFlag::AlignRight | SizerFlag::All, 6);

    panel.set_sizer(sizer, true);
    let dlg_sizer = BoxSizer::builder(Orientation::Vertical).build();
    dlg_sizer.add(&panel, 1, SizerFlag::Expand, 0);
    dialog.set_sizer_and_fit(dlg_sizer, true);

    if dialog.show_modal() == ID_OK {
        let mut apply = on_set.borrow_mut();
        for (key, ctl) in &controls {
            let value: Option<settings::Value> = match ctl {
                Ctl::Bool(c) => Some(settings::Value::Bool(c.get_value())),
                Ctl::Num(c, integral, min, max) => {
                    c.get_value().trim().parse::<f64>().ok().map(|mut n| {
                        if let Some(mn) = min {
                            n = n.max(*mn);
                        }
                        if let Some(mx) = max {
                            n = n.min(*mx);
                        }
                        if *integral {
                            settings::Value::Int(n as i64)
                        } else {
                            settings::Value::Float(n)
                        }
                    })
                }
                Ctl::Choice(c, choices) => {
                    let i = c.get_selection().unwrap_or(0) as usize;
                    Some(settings::Value::Str(choices.get(i).cloned().unwrap_or_default()))
                }
                Ctl::Text(c) => Some(settings::Value::Str(c.get_value())),
            };
            if let Some(v) = value {
                apply(idx, key.clone(), v);
            }
        }
    }
    dialog.destroy();
}

/// Whether a repo (owner/repo) is already installed in the modules directory.
fn repo_installed(full_name: &str) -> bool {
    crate::registry::installed()
        .iter()
        .any(|m| m.source.as_ref().map(|s| s.repo.as_str()) == Some(full_name))
}

/// Enables the Install button only when a not-yet-installed result is selected
/// and no install/update is in flight.
fn refresh_install_btn(
    list: &ListBox,
    results: &[crate::registry::RemoteModule],
    busy: bool,
    btn: &Button,
) {
    let enabled = !busy
        && list
            .get_selection()
            .and_then(|row| results.get(row as usize))
            .map(|rm| !repo_installed(&rm.full_name))
            .unwrap_or(false);
    btn.enable(enabled);
}

/// Enables the Settings button only when the selected row's module actually has
/// settings (and a row is selected at all) — so the user can't open an empty
/// settings dialog.
fn refresh_settings_btn(list: &TreeCtrl, rows: &[Row], settings_btn: &Button) {
    let has_settings = list
        .get_selection()
        .and_then(|sel| rows.iter().find(|r| native_checkboxes::same(&r.item, &sel)))
        .map(|r| !r.settings.is_empty())
        .unwrap_or(false);
    settings_btn.enable(has_settings);
}

/// Reads each row's native check state, reporting any that changed since the
/// last call via `on_toggle`. Called after every mouse-up / key-up on the tree.
fn sync_checks(
    hwnd: *mut c_void,
    rows: &[Row],
    states: &RefCell<Vec<bool>>,
    on_toggle: &RefCell<Box<dyn FnMut(usize, bool)>>,
) {
    let mut states = states.borrow_mut();
    let mut cb = on_toggle.borrow_mut();
    for (i, row) in rows.iter().enumerate() {
        let now = native_checkboxes::get(hwnd, &row.item);
        if states.get(i).copied() != Some(now) {
            if let Some(slot) = states.get_mut(i) {
                *slot = now;
            }
            cb(row.module_idx, now); // report the real module index
        }
    }
}

/// Native Windows checkbox support for `wxTreeCtrl` (`TVS_CHECKBOXES`). wxdragon
/// doesn't expose it, so we drive the underlying `SysTreeView32` directly — this
/// gives real, UIA-exposed checkboxes (screen-reader-correct) rather than the
/// generic/owner-drawn lists wxWidgets offers cross-platform.
#[cfg(windows)]
mod native_checkboxes {
    use std::ffi::c_void;

    use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SendMessageW, SetWindowLongPtrW, GWL_STYLE,
    };
    use wxdragon::widgets::treectrl::TreeItemId;

    const TVS_CHECKBOXES: isize = 0x0100;
    const TV_FIRST: u32 = 0x1100;
    const TVM_SETITEMW: u32 = TV_FIRST + 63;
    const TVM_GETITEMW: u32 = TV_FIRST + 62;
    const TVIF_HANDLE: u32 = 0x0010;
    const TVIF_STATE: u32 = 0x0008;
    const TVIS_STATEIMAGEMASK: u32 = 0xF000;

    /// Layout-compatible mirror of `TVITEMW` from `<commctrl.h>`.
    #[repr(C)]
    struct Tvitemw {
        mask: u32,
        h_item: *mut c_void,
        state: u32,
        state_mask: u32,
        text: *mut u16,
        text_max: i32,
        image: i32,
        selected_image: i32,
        children: i32,
        l_param: isize,
    }

    /// Reads the native `HTREEITEM` out of a wxdragon `TreeItemId`. Relies on
    /// `TreeItemId` being a single-field `{ ptr: *mut wxd_TreeItemId_t }`, and
    /// `wxd_TreeItemId_t*` being a reinterpret of `wxTreeItemId*` whose only
    /// member is `void* m_pItem`. Stable on wxMSW.
    fn htreeitem(item: &TreeItemId) -> *mut c_void {
        let inner: *mut c_void = unsafe { std::mem::transmute_copy(item) };
        if inner.is_null() {
            return std::ptr::null_mut();
        }
        unsafe { *(inner as *const *mut c_void) }
    }

    /// OR-in `TVS_CHECKBOXES`. Must be called before items are inserted.
    pub fn enable(hwnd: *mut c_void) {
        let hwnd = hwnd as HWND;
        if hwnd.is_null() {
            return;
        }
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
            if (style & TVS_CHECKBOXES) == 0 {
                SetWindowLongPtrW(hwnd, GWL_STYLE, style | TVS_CHECKBOXES);
            }
        }
    }

    /// Sets the checkbox state. State-image index is 1-based: 1 = unchecked,
    /// 2 = checked.
    pub fn set(hwnd: *mut c_void, item: &TreeItemId, checked: bool) {
        let hwnd = hwnd as HWND;
        let h_item = htreeitem(item);
        if hwnd.is_null() || h_item.is_null() {
            return;
        }
        let mut tvi = Tvitemw {
            mask: TVIF_HANDLE | TVIF_STATE,
            h_item,
            state: (if checked { 2u32 } else { 1u32 }) << 12,
            state_mask: TVIS_STATEIMAGEMASK,
            text: std::ptr::null_mut(),
            text_max: 0,
            image: 0,
            selected_image: 0,
            children: 0,
            l_param: 0,
        };
        unsafe {
            SendMessageW(hwnd, TVM_SETITEMW, 0 as WPARAM, &mut tvi as *mut _ as LPARAM);
        }
    }

    /// Reads the checkbox state (state-image index 2 == checked).
    pub fn get(hwnd: *mut c_void, item: &TreeItemId) -> bool {
        let hwnd = hwnd as HWND;
        let h_item = htreeitem(item);
        if hwnd.is_null() || h_item.is_null() {
            return false;
        }
        let mut tvi = Tvitemw {
            mask: TVIF_HANDLE | TVIF_STATE,
            h_item,
            state: 0,
            state_mask: TVIS_STATEIMAGEMASK,
            text: std::ptr::null_mut(),
            text_max: 0,
            image: 0,
            selected_image: 0,
            children: 0,
            l_param: 0,
        };
        unsafe {
            SendMessageW(hwnd, TVM_GETITEMW, 0 as WPARAM, &mut tvi as *mut _ as LPARAM);
        }
        ((tvi.state & TVIS_STATEIMAGEMASK) >> 12) == 2
    }

    /// True if two `TreeItemId`s refer to the same native tree node.
    pub fn same(a: &TreeItemId, b: &TreeItemId) -> bool {
        let ha = htreeitem(a);
        !ha.is_null() && ha == htreeitem(b)
    }
}

/// Non-Windows stub: a native TVS_CHECKBOXES equivalent (macOS/GTK) comes later.
#[cfg(not(windows))]
mod native_checkboxes {
    use std::ffi::c_void;
    use wxdragon::widgets::treectrl::TreeItemId;

    pub fn enable(_hwnd: *mut c_void) {}
    pub fn set(_hwnd: *mut c_void, _item: &TreeItemId, _checked: bool) {}
    pub fn get(_hwnd: *mut c_void, _item: &TreeItemId) -> bool {
        false
    }
    pub fn same(_a: &TreeItemId, _b: &TreeItemId) -> bool {
        false
    }
}
