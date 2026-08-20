//! wxDragon GUI host: a **tray-resident module manager**. wxWidgets owns the
//! main message loop; our OS events are pumped from a wx `Timer` tick
//! (event-loop coexistence, see `docs/architecture-feasibility-study.md` §11.6).
//!
//! The window lists loaded modules with enable/disable checkboxes and lives in
//! the system tray: it starts hidden (modules are needed far more often than
//! their management), closing it (X) hides it back to the tray, and only the
//! tray "Quit" actually exits. Double-clicking the tray icon reopens it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use wxdragon::prelude::*;

use crate::settings;

const MENU_SHOW: i32 = 1001;
const MENU_QUIT: i32 = 1002;
const MENU_HIDE: i32 = 1003;

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

/// One built row in the Installed list: what the handlers need — the module index
/// (toggle/settings callbacks), id + name (messages), its settings, and the ids it depends
/// on (to block removing a needed module).
///
/// A row's identity is its POSITION: `rows[i]` is list item `i`, always. That is all a
/// `wxCheckListBox` offers, and it is enough as long as every change to the list goes
/// through `rebuild_list`, so the two cannot drift apart.
struct Row {
    module_idx: usize,
    id: String,
    name: String,
    /// What the list shows. Held because the control can neither rename one item nor
    /// delete one — the list is rebuilt from these.
    label: String,
    /// The checkbox, as we last set it.
    ///
    /// Not a shadow copy to be diffed against the control — that was the old design, and
    /// one unanswerable reading of it read as "the user just unticked everything". This is
    /// written when the user toggles and read only when the list is rebuilt.
    enabled: bool,
    settings: Vec<SettingDesc>,
    dependencies: Vec<String>,
}

impl Row {
    fn new(info: &ModuleInfo) -> Self {
        Row {
            module_idx: info.module_idx,
            id: info.id.clone(),
            name: info.name.clone(),
            label: row_label(&info.name, &info.version, &info.id),
            enabled: info.enabled,
            settings: info.settings.clone(),
            dependencies: info.dependencies.clone(),
        }
    }
}

/// How a module reads in the list. One place: it is written at first fill, at install and
/// at update, and three copies of a format string drift.
fn row_label(name: &str, version: &str, id: &str) -> String {
    format!("{name}  v{version}   ({id})")
}


/// Puts the manager window in front, from the tray menu or a double-click.
///
/// `show(true)` alone is not enough, and the reason is worth writing down because it looks
/// like it should be. `wxWindowBase::Show` returns early when the window already believes it
/// is shown, and after the user switches to another application with Command-Tab, nothing
/// syncs that belief back from the window system — the frame is still "shown", merely behind
/// everything. So the menu item ran, the window did not move, and for somebody who cannot
/// see the screen the application had simply disappeared. `raise()` is what actually orders
/// it in, and it has to come after `show(true)` because it does nothing to a hidden window.
///
/// On macOS one more thing is needed: ordering a window in does not bring the PROCESS
/// forward, and an agent application that is not frontmost puts its window up behind
/// whatever is. The Dock-icon promotion is separate and optional — see the setting.
/// Puts the manager window away again — the counterpart to `show_manager`.
///
/// It exists because on macOS there was no way to do this at all. An application with no
/// Dock icon has no menu bar, and without a menu bar macOS has no Command-W and no
/// Command-Q: those are menu items, not built-in keystrokes. So a window that opened could
/// not be closed, and for somebody who cannot see the title bar's close button that is a
/// room with no door.
fn hide_manager(frame: &Frame) {
    frame.show(false);
    // Demoted after hiding, never before: the window has to be off the screen before the
    // application stops being one that has windows.
    #[cfg(target_os = "macos")]
    if crate::appcfg::dock_while_open() {
        crate::backend::set_regular(false);
    }
}

fn show_manager(frame: &Frame) {
    #[cfg(target_os = "macos")]
    {
        if crate::appcfg::dock_while_open() {
            crate::backend::set_regular(true);
        }
        crate::backend::activate_self();
    }
    frame.show(true);
    frame.centre();
    frame.raise();
}

/// The Installed list, which is a different control on each platform.
///
/// Not a preference. On Windows a `wxTreeCtrl` is a real `SysTreeView32`, and with
/// `TVS_CHECKBOXES` its checkboxes come from the OS and are described by comctl32's own
/// accessibility server. On macOS the same class is `wxGenericTreeCtrl`: a scrolled window
/// that paints its rows itself, and wxWidgets compiles its whole accessibility layer out for
/// anything but MSW (`include/wx/chkconf.h`). VoiceOver therefore does not read that list
/// badly — it skips the control entirely. There a `wxCheckListBox` is a real `NSTableView`
/// with an `NSButtonCell` switch column.
///
/// Tried and rejected: one `wxCheckListBox` everywhere. It is native on macOS, but on
/// Windows it is an owner-drawn listbox whose checkbox wxWidgets draws and describes itself
/// (`wxCheckListBoxAccessible`, new in 3.3.2), and that shim reports no child count, no item
/// locations and no selected state. Built and tried with NVDA: worse than what was already
/// there. Giving up the platform that works to fix the one that does not is the wrong trade,
/// so both stay.
///
/// What the rest of the window sees is an index. Item `i` is `rows[i]` on both platforms —
/// the one invariant every caller depends on, and the only thing this type must keep true.
#[derive(Clone)]
struct InstalledList {
    inner: Rc<ListInner>,
}

impl InstalledList {
    fn new(parent: &Panel) -> Self {
        InstalledList { inner: Rc::new(ListInner::new(parent)) }
    }

    /// Puts the control in a sizer. Here rather than at the call site, so the widget type
    /// stays inside.
    fn add_to(&self, sizer: &BoxSizer) {
        self.inner.add_to(sizer);
    }

    /// Redraws the list from `rows`, keeping the selection on the same index where it can.
    ///
    /// Wholesale on both platforms. The macOS control can neither rename an item nor delete
    /// one — wxdragon binds append, insert and clear and nothing else — and doing it the
    /// same way on Windows leaves one behaviour to reason about instead of two. A dozen rows
    /// cost nothing.
    ///
    /// Losing your place in a list you cannot see is not a small thing, so the selection
    /// comes back, clamped if the list got shorter; and a freshly built list lands on the
    /// first row rather than on nothing, so there is something to arrow from.
    fn rebuild(&self, rows: &[Row]) {
        self.inner.rebuild(rows);
    }

    fn selection(&self) -> Option<usize> {
        self.inner.selection()
    }

    fn checked(&self, i: usize) -> bool {
        self.inner.checked(i)
    }

    /// Fires when a checkbox may have changed; the handler re-reads every row.
    ///
    /// "May", and every row, because the two platforms know different amounts. The macOS
    /// control raises a real toggle event naming the row; the Windows tree raises nothing at
    /// all, so there the only honest signal is "a key or a button came up, look again".
    /// `apply_toggle` reports only rows that really changed, so both arrive at the same
    /// place and neither can announce a change that did not happen.
    fn on_maybe_toggled(&self, f: impl FnMut() + 'static) {
        self.inner.on_maybe_toggled(Box::new(f));
    }

    fn on_select(&self, f: impl FnMut() + 'static) {
        self.inner.on_select(Box::new(f));
    }
}

type ListCallback = Rc<RefCell<Option<Box<dyn FnMut()>>>>;

#[cfg(windows)]
struct ListInner {
    ctrl: TreeCtrl,
    /// The tree needs a root to hang rows from, even though it is hidden.
    root: Option<TreeItemId>,
    /// Item `i` of the tree, so an index becomes a node and a node becomes an index.
    items: RefCell<Vec<TreeItemId>>,
    hwnd: *mut std::ffi::c_void,
    toggled: ListCallback,
    selected: ListCallback,
}

#[cfg(windows)]
impl ListInner {
    fn new(parent: &Panel) -> Self {
        let ctrl = TreeCtrl::builder(parent)
            .with_style(TreeCtrlStyle::HideRoot | TreeCtrlStyle::Single | TreeCtrlStyle::NoLines)
            .build();
        let hwnd = ctrl.get_handle();
        // Must be on before the first item is inserted.
        native_checkboxes::enable(hwnd);
        let root = ctrl.add_root("Modules", None, None);

        // The tree offers no toggle and no selection event, so both answers come off the
        // same two up-events — and they are bound ONCE here, with the callbacks stored,
        // rather than bound again per registration. The native control flips its box on the
        // DOWN event, so by the up event the new state is already there to be read.
        let toggled: ListCallback = Rc::new(RefCell::new(None));
        let selected: ListCallback = Rc::new(RefCell::new(None));
        let fire = {
            let (t, s) = (toggled.clone(), selected.clone());
            move || {
                if let Some(f) = t.borrow_mut().as_mut() {
                    f();
                }
                if let Some(f) = s.borrow_mut().as_mut() {
                    f();
                }
            }
        };
        let fire = Rc::new(RefCell::new(fire));
        {
            let fire = fire.clone();
            ctrl.on_mouse_left_up(move |e| {
                (fire.borrow_mut())();
                e.skip(true);
            });
        }
        {
            let fire = fire.clone();
            ctrl.on_key_up(move |e| {
                (fire.borrow_mut())();
                e.skip(true);
            });
        }
        ListInner { ctrl, root, items: RefCell::new(Vec::new()), hwnd, toggled, selected }
    }

    fn add_to(&self, sizer: &BoxSizer) {
        sizer.add(&self.ctrl, 1, SizerFlag::All | SizerFlag::Expand, 12);
    }

    fn rebuild(&self, rows: &[Row]) {
        let previous = self.selection();
        {
            let mut items = self.items.borrow_mut();
            for it in items.iter() {
                self.ctrl.delete(it);
            }
            items.clear();
            let Some(root) = &self.root else {
                return;
            };
            for r in rows {
                if let Some(item) = self.ctrl.append_item(root, &r.label, None, None) {
                    native_checkboxes::set(self.hwnd, &item, r.enabled);
                    items.push(item);
                }
            }
        }
        if !rows.is_empty() {
            let i = previous.unwrap_or(0).min(rows.len() - 1);
            if let Some(item) = self.items.borrow().get(i) {
                self.ctrl.select_item(item);
            }
        }
    }

    fn selection(&self) -> Option<usize> {
        let sel = self.ctrl.get_selection()?;
        self.items.borrow().iter().position(|it| native_checkboxes::same(it, &sel))
    }

    fn checked(&self, i: usize) -> bool {
        self.items.borrow().get(i).is_some_and(|it| native_checkboxes::get(self.hwnd, it))
    }

    fn on_maybe_toggled(&self, f: Box<dyn FnMut()>) {
        *self.toggled.borrow_mut() = Some(f);
    }

    fn on_select(&self, f: Box<dyn FnMut()>) {
        *self.selected.borrow_mut() = Some(f);
    }
}

#[cfg(not(windows))]
struct ListInner {
    ctrl: CheckListBox,
    toggled: ListCallback,
}

#[cfg(not(windows))]
impl ListInner {
    fn new(parent: &Panel) -> Self {
        // `Single` explicitly: this widget's `Default` style is literally 0, unlike
        // ListBox's. Never `Sort` — it would reorder items behind us, and a row's position
        // is its identity.
        let ctrl = CheckListBox::builder(parent).with_style(CheckListBoxStyle::Single).build();
        ctrl.set_name("Installed modules");
        let toggled: ListCallback = Rc::new(RefCell::new(None));

        // Space, which this control does not handle itself: wxOSX's wxCheckListBox has an
        // empty event table, where the MSW one maps Space, plus and minus. Without this a
        // VoiceOver user could reach the list, read it, and change nothing. `check()` does
        // not raise the toggle event — wx sends that only from its own input handling — so
        // the handler flips the box and then says so.
        {
            let (t, c) = (toggled.clone(), ctrl);
            ctrl.on_key_down(move |e| {
                const SPACE: i32 = 32;
                if let WindowEventData::Keyboard(k) = &e {
                    if k.get_key_code() == Some(SPACE) {
                        if let Some(i) = c.get_selection() {
                            c.check(i, !c.is_checked(i));
                            if let Some(f) = t.borrow_mut().as_mut() {
                                f();
                            }
                            return;
                        }
                    }
                }
                e.skip(true);
            });
        }
        {
            // A real toggle event: edge-triggered, only on genuine user action, and
            // `check()` cannot re-enter it.
            let t = toggled.clone();
            ctrl.on_toggled(move |_| {
                if let Some(f) = t.borrow_mut().as_mut() {
                    f();
                }
            });
        }
        ListInner { ctrl, toggled }
    }

    fn add_to(&self, sizer: &BoxSizer) {
        sizer.add(&self.ctrl, 1, SizerFlag::All | SizerFlag::Expand, 12);
    }

    fn rebuild(&self, rows: &[Row]) {
        let previous = self.ctrl.get_selection();
        self.ctrl.clear();
        for (i, r) in rows.iter().enumerate() {
            self.ctrl.append(&r.label);
            self.ctrl.check(i as u32, r.enabled);
        }
        if !rows.is_empty() {
            let last = rows.len() as u32 - 1;
            self.ctrl.set_selection(previous.unwrap_or(0).min(last), true);
        }
    }

    fn selection(&self) -> Option<usize> {
        self.ctrl.get_selection().map(|i| i as usize)
    }

    fn checked(&self, i: usize) -> bool {
        self.ctrl.is_checked(i as u32)
    }

    fn on_maybe_toggled(&self, f: Box<dyn FnMut()>) {
        *self.toggled.borrow_mut() = Some(f);
    }

    fn on_select(&self, f: Box<dyn FnMut()>) {
        // Fires on arrow keys too, which the Windows side catches only because a key-up
        // happens to follow.
        let f = RefCell::new(f);
        self.ctrl.on_selected(move |_| (f.borrow_mut())());
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
    on_reload: impl Fn(usize) -> Result<(ModuleInfo, crate::ReloadReport), String> + 'static,
    mut pump: impl FnMut() + 'static,
    mut drain_errors: impl FnMut() -> Vec<(String, String)> + 'static,
    // Speech belongs to the host, which owns the single engine — a second one is an error
    // on macOS, where both of the `tts` crate's backends register the same Objective-C
    // class name. So the GUI is handed the ability to say something rather than the means.
    announce: impl Fn(&str) + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    let on_reload = std::rc::Rc::new(on_reload);
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

        // See `InstalledList` for why this is one control on Windows and another on macOS.
        // Every module is listed, libraries too — they just cannot be removed while
        // something still needs them.
        let list = InstalledList::new(&installed);
        let rows: Vec<Row> = modules.iter().map(Row::new).collect();
        list.rebuild(&rows);
        list.add_to(&is);

        let inst_buttons = BoxSizer::builder(Orientation::Horizontal).build();
        let settings_btn = Button::builder(&installed).with_label("Settings…").build();
        let reload_btn = Button::builder(&installed).with_label("Reload").build();
        let uninstall_btn = Button::builder(&installed).with_label("Uninstall").build();
        inst_buttons.add(&settings_btn, 0, SizerFlag::All, 6);
        inst_buttons.add(&reload_btn, 0, SizerFlag::All, 6);
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

        // ===== "Application settings" tab: the platform's own =====
        //
        // A tab rather than a dialog, and each change applies as it is made. A modal would
        // mean opening it, changing something, confirming, and only then finding out what it
        // did; a tab is somewhere you can be, tick something, hear the result, and untick it.
        // There is no OK button for the same reason there is none on the module checkboxes in
        // the Installed list: the change IS the action.
        //
        // Built from `appcfg::SWITCHES`, so a setting added there appears here without anyone
        // remembering to come back.
        // A ScrolledWindow, not a Panel, and the reason is a control that existed and could
        // not be reached. The window is 560x470; each setting is a label, a checkbox and a
        // paragraph of explanation, so five of them already fill the page and the sixth and
        // seventh — the two that only exist on macOS — were simply cut off the bottom. Not
        // greyed, not announced, not there: the tester could find "Speak through VoiceOver"
        // and no trace of the setting under it.
        //
        // A fixed page that silently truncates is the worst shape for this list, because the
        // list grows. Scrolling also means the screen reader's own focus movement brings a
        // control into view instead of stopping at the edge of what happens to fit.
        let app_tab = ScrolledWindow::builder(&notebook).build();
        // Vertical only: nothing here is wider than the page, and a horizontal scrollbar
        // would be one more thing to land on for no reason.
        app_tab.set_scroll_rate(0, 10);
        let as_ = BoxSizer::builder(Orientation::Vertical).build();
        as_.add(
            &StaticText::builder(&app_tab)
                .with_label(
                    "Settings for the application itself. Each takes effect when its \
                     label says, and is remembered.",
                )
                .build(),
            0,
            SizerFlag::All,
            12,
        );
        for sw in crate::appcfg::SWITCHES.iter().filter(|s| s.applies_here()) {
            // A variable set in the environment forces its setting on and nothing here can
            // undo that until the application is started without it. So the box is shown
            // ticked and disabled, with the reason in its name — rather than a control that
            // would accept being unticked and then quietly stay on.
            let forced = crate::appcfg::forced_by_env(sw.key);
            let label = if forced {
                format!(
                    "{} — forced on by the {} environment variable, so it cannot be changed here",
                    sw.label,
                    crate::appcfg::env_name(sw.key)
                )
            } else {
                sw.label.to_string()
            };
            // The checkbox carries its own label, and there is no StaticText in front of it.
            //
            // There used to be, and the comment said a checkbox's own label is not what the
            // screen reader announces. That was half right and it produced the wrong shape.
            // On WINDOWS the name comes from `set_name`: wxCheckBox installs an accessible
            // that returns the window name, whose default is the literal string "check", and
            // the StaticText never contributed to it. On macOS `set_name` does nothing at all
            // — wxWidgets compiles its accessibility layer out for every port but MSW — and
            // the name of an NSButton is its title, which was empty. So the arrangement gave
            // Windows a label twice over and macOS a labelled paragraph sitting next to a
            // nameless control. Label plus name is one announcement on both.
            let cb = CheckBox::builder(&app_tab).with_label(&label).build();
            cb.set_name(&label);
            cb.set_value(crate::appcfg::get(sw.key));
            if forced {
                cb.enable(false);
            }
            as_.add(&cb, 0, SizerFlag::Left | SizerFlag::All, 8);
            // The explanation under the control rather than in it: it is a sentence, not a
            // name, and a name that long is tiring to hear on every visit.
            as_.add(
                &StaticText::builder(&app_tab).with_label(sw.help).build(),
                0,
                SizerFlag::Left | SizerFlag::Bottom,
                10,
            );
            let key = sw.key;
            cb.on_toggled(move |event| {
                // The box's own new state, from the event — not the inverse of what the
                // setting was. Deriving it by inversion looks equivalent and is not: the two
                // can already disagree (an environment variable forces one on regardless of
                // what is stored), and then every click would flip the wrong way.
                let want = event.is_checked();
                crate::appcfg::set(key, want);
                let now = crate::appcfg::get(key);
                let mut store = settings::Store::load();
                store.set_app_flag(key, now);
                store.save();
                crate::logging::line(
                    "settings",
                    &format!("{key} is now {}", if now { "on" } else { "off" }),
                );
            });
        }
        // `false`: fitting would resize the page to its contents, which is exactly what a
        // scrolled window must not do — the sizer's size becomes the virtual size and the
        // scrollbar covers the difference.
        app_tab.set_sizer(as_, false);
        notebook.add_page(&app_tab, "Application settings", false, None);

        sizer.add(&notebook, 1, SizerFlag::All | SizerFlag::Expand, 0);
        let hint = StaticText::builder(&panel)
            .with_label("Closing this window hides it to the tray; modules keep running. Quit from the tray icon.")
            .build();
        sizer.add(&hint, 0, SizerFlag::All, 12);
        panel.set_sizer(sizer, true);

        // The list arrives with its first row selected (see InstalledList::rebuild), so the
        // buttons start from what is actually selected rather than from "nothing".
        refresh_settings_btn(&list, &rows, &settings_btn);
        reload_btn.enable(list.selection().is_some());
        let rows = Rc::new(RefCell::new(rows));
        let on_toggle: Rc<RefCell<Box<dyn FnMut(usize, bool)>>> =
            Rc::new(RefCell::new(Box::new(on_toggle)));

        {
            let (rows, on_toggle, l) = (rows.clone(), on_toggle.clone(), list.clone());
            list.on_maybe_toggled(move || {
                let n = rows.borrow().len();
                for i in 0..n {
                    apply_toggle(&l, &rows, i, &on_toggle);
                }
            });
        }
        {
            let (rows, l) = (rows.clone(), list.clone());
            list.on_select(move || {
                refresh_settings_btn(&l, &rows.borrow(), &settings_btn);
                reload_btn.enable(l.selection().is_some());
            });
        }

        // "Settings…" opens a per-module dialog with native controls.
        let on_set: Rc<RefCell<Box<dyn FnMut(usize, String, settings::Value)>>> =
            Rc::new(RefCell::new(Box::new(on_set)));
        {
            let (rows, on_set, list) = (rows.clone(), on_set.clone(), list.clone());
            settings_btn.on_click(move |_| {
                let Some(sel) = list.selection() else {
                    return;
                };
                let found = {
                    let rb = rows.borrow();
                    rb.get(sel).map(|r| (r.module_idx, r.settings.clone()))
                };
                let Some((module_idx, settings)) = found else {
                    return;
                };
                if settings.is_empty() {
                    modal_message(&frame, "Settings", "This module has no settings.", false);
                    return;
                }
                {
                    let on_set = on_set.clone();
                    let mut apply = move |key: String, v: settings::Value| {
                        on_set.borrow_mut()(module_idx, key, v);
                    };
                    open_settings_dialog(&frame, "Module settings", &settings, &mut apply);
                }
            });
        }

        // "Reload" rebuilds the selected module's VM in place from its source dir
        // (edit a dev module + reload without restarting the app). Dependents that
        // hold its code keep the old copy until restarted.
        {
            let (rows, list) = (rows.clone(), list.clone());
            let reload_cb = on_reload.clone();
            reload_btn.on_click(move |_| {
                let Some(sel) = list.selection() else {
                    return;
                };
                let idx = { rows.borrow().get(sel).map(|r| r.module_idx) };
                let Some(idx) = idx else {
                    return;
                };
                match reload_cb(idx) {
                    Ok((info, report)) => {
                        // The reload may have changed the schema, the declared
                        // dependencies (which the Uninstall guard reasons over — a
                        // stale set could drop a module a live one now needs), and the
                        // name/version. Refresh all of them on the row + its label.
                        {
                            let mut rb = rows.borrow_mut();
                            if let Some(r) = rb.iter_mut().find(|r| r.module_idx == idx) {
                                r.settings = info.settings.clone();
                                r.dependencies = info.dependencies.clone();
                                r.name = info.name.clone();
                                r.label = row_label(&info.name, &info.version, &info.id);
                            }
                        }
                        list.rebuild(&rows.borrow());
                        // The cascade is the point: a module that depends on this one
                        // carries a COPY of its code, so it was rebuilt too. Name them,
                        // and name any that could not be rebuilt -- those are now
                        // inactive, which the user has to know.
                        let others = report.reloaded.len().saturating_sub(1);
                        let mut msg = if others == 0 {
                            format!("Module \u{201c}{}\u{201d} reloaded.", info.name)
                        } else {
                            format!(
                                "Module \u{201c}{}\u{201d} reloaded, along with {} module(s) that depend on it: {}.",
                                info.name,
                                others,
                                report.reloaded[1..].join(", ")
                            )
                        };
                        if !report.failed.is_empty() {
                            msg.push_str(&format!(
                                "\n\nThese could not be rebuilt and are now inactive: {}. Fix each and press Reload on it.",
                                report
                                    .failed
                                    .iter()
                                    .map(|(id, e)| format!("{id} ({e})"))
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            ));
                        }
                        modal_message(&frame, "Reloaded", &msg, false);
                    }
                    Err(e) => {
                        modal_message(
                            &frame,
                            "Reload failed",
                            &format!(
                                "{e}\n\nThe module is now inactive — its hotkeys, keys and triggers were revoked. Fix the error in its source and press Reload again to restore it."
                            ),
                            false,
                        );
                    }
                }
            });
        }

        // "Uninstall" removes the selected module's files AND disables it in the
        // running app immediately (revoking its hotkeys/keys/triggers) + drops its
        // row — no restart needed. A module another currently-loaded module
        // depends on can't be removed.
        {
            let (rows, list) = (rows.clone(), list.clone());
            uninstall_btn.on_click(move |_| {
                let Some(sel) = list.selection() else {
                    return;
                };
                let found = {
                    let rb = rows.borrow();
                    let graph: Vec<(String, Vec<String>)> =
                        rb.iter().map(|x| (x.id.clone(), x.dependencies.clone())).collect();
                    rb.get(sel).map(|r| {
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
                            rows.borrow_mut().remove(pos);
                            list.rebuild(&rows.borrow());
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
                                    on_remove(removed.module_idx);
                                    list.rebuild(&rows.borrow());
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
            /// An update landed on disk: rebuild the running module + its dependents.
            Updated(String),
            Browse(Vec<crate::registry::RemoteModule>),
            BrowseStatus(String),
            Updates(Vec<(String, String, String)>), // (module id, repo, "vX → vY")
            Installed(std::path::PathBuf),  // hot-load a freshly installed module
            Done(String),                   // a modal result message
        }
        let inbox: Arc<Mutex<Vec<Job>>> = Arc::new(Mutex::new(Vec::new()));
        let browse_results: Rc<RefCell<Vec<crate::registry::RemoteModule>>> =
            Rc::new(RefCell::new(Vec::new()));
        let update_results: Rc<RefCell<Vec<(String, String, String)>>> =
            Rc::new(RefCell::new(Vec::new()));
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
                // The operating-system claim, when the module makes one that excludes this
                // machine. A warning rather than a refusal: the manifest may simply be
                // behind the code, the user may be installing on one machine for another,
                // and refusing an install on the strength of a line in a text file is a
                // stronger claim than that line can carry. The install still happens; the
                // module just will not load here, and the log says so at every start.
                let os = std::env::consts::OS;
                let os_note = if manifest.runs_on(os) {
                    String::new()
                } else {
                    format!(
                        "\n\nNOT FOR THIS SYSTEM\nIt declares support for: {}\nThis machine is: \
                         {os}\n\nIt can be installed, but it will not be loaded here.",
                        manifest.supported_os.join(", ")
                    )
                };
                let msg = format!(
                    "\u{201c}{}\u{201d} v{} ({})\n\nRequested capabilities:\n\u{2022} {}{}\n\nInstall this module?",
                    manifest.name, manifest.version, manifest.id, caps_str, os_note
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
                // Offer the OPTIONAL dependencies (extra features, not required) as an
                // opt-in: declining still installs the module + its required deps.
                let optional_ids: Vec<String> = manifest
                    .optional_dependencies
                    .iter()
                    .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                let accepted_optional: std::collections::HashSet<String> = if optional_ids.is_empty()
                {
                    std::collections::HashSet::new()
                } else {
                    let opt_msg = format!(
                        "\u{201c}{}\u{201d} can also use these optional modules (extra features, not required):\n\u{2022} {}\n\nInstall them too?",
                        manifest.name,
                        optional_ids.join("\n\u{2022} ")
                    );
                    if modal_message(&frame, "Optional dependencies", &opt_msg, true) {
                        optional_ids.into_iter().collect()
                    } else {
                        std::collections::HashSet::new()
                    }
                };
                browse_status.set_label(&format!("Installing {full_name}…"));
                let inbox = inbox.clone();
                std::thread::spawn(move || {
                    // Always deliver a result, even on an unexpected panic, so the
                    // buttons can never get stuck disabled.
                    let job = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        // Install the whole dependency tree, not just this repo; the
                        // hot-load resolves the now-installed deps as siblings.
                        match crate::registry::install_tree(&full_name, &accepted_optional) {
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
                        if let Some(new_version) = crate::registry::update_available(&m) {
                            if let Some(src) = &m.source {
                                updatable.push((
                                    m.id.clone(),
                                    src.repo.clone(),
                                    format!("v{} \u{2192} v{new_version}", m.version),
                                ));
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
                let Some((id, repo, _)) = update_results.borrow().get(row as usize).cloned() else {
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
                            Ok(m) => Job::Updated(m.id),
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
            hide_manager(&frame);
        });

        // Escape, and Command-W where there is a Command key.
        //
        // Bound on the frame, which is where wxWidgets sends a key event that the focused
        // control did not handle. On Windows this is a convenience next to Alt+F4; on macOS
        // it is the only keystroke that closes this window at all, because an application
        // without a Dock icon has no menu bar to put Command-W in. The tray menu carries the
        // same action for the case where this does not reach us.
        frame.on_key_down(move |event| {
            const ESCAPE: i32 = 27;
            const W: i32 = 87;
            if let WindowEventData::Keyboard(k) = &event {
                let code = k.get_key_code();
                // `control_down` is Command on macOS and Control on Windows — wxWidgets maps
                // the two by position, and `RawControlDown` is the one that means the
                // literal Control key. So this is Command-W there and Ctrl+W here, each
                // being the convention of its own platform, from one line.
                if code == Some(ESCAPE) || (code == Some(W) && k.control_down()) {
                    hide_manager(&frame);
                    return;
                }
            }
            event.skip(true);
        });

        // System tray icon + right-click menu (Show / Quit).
        // A status item in the menu bar, not a Dock icon. The default type maps to the
        // Dock on macOS, and the application ships as an agent with no Dock icon, so the
        // menu would have had nowhere to appear. VoiceOver reaches the menu bar extras
        // with VO-M twice.
        #[cfg(target_os = "macos")]
        let taskbar = TaskBarIcon::builder()
            .with_icon_type(wxdragon::widgets::taskbar_icon::TaskBarIconType::CustomStatusItem)
            .build();
        #[cfg(not(target_os = "macos"))]
        let taskbar = TaskBarIcon::builder().build();
        if let Some(icon) = make_icon() {
            taskbar.set_icon(&icon, "Automation Platform");
        }
        // Three items rather than two. "Close the module window" is here because the
        // keyboard route to it is not guaranteed: on macOS this application has no menu bar,
        // so it has no Command-W, and a menu item is the one way out that cannot depend on
        // a keystroke reaching the right window.
        let mut menu = Menu::builder()
            .append_item(MENU_SHOW, "Show module manager", "Show the module window")
            .append_item(MENU_HIDE, "Close the module window", "Put the module window away")
            .append_separator()
            .append_item(MENU_QUIT, "Quit", "Quit Automation Platform")
            .build();
        taskbar.set_popup_menu(&mut menu);
        std::mem::forget(menu); // the tray icon owns it for the app's lifetime

        taskbar.on_menu(move |event| match event.get_id() {
            MENU_SHOW => show_manager(&frame),
            MENU_HIDE => hide_manager(&frame),
            MENU_QUIT => app.exit_main_loop(),
            _ => {}
        });

        #[cfg(any(target_os = "windows", target_os = "linux"))]
        taskbar.on_left_double_click(move |_| show_manager(&frame));

        // The one signal that the application actually started. `show_balloon` is
        // Windows-only — it returns false everywhere else without doing anything — and on
        // macOS there is no Dock icon to notice either, so a user who cannot see the screen
        // would have nothing at all to go on. Speech is the honest channel here: it reaches
        // the person the application is for, on both platforms, and it is the same channel
        // everything else in the product uses.
        #[cfg(target_os = "macos")]
        let hint = "Automation Platform is running in the menu bar. Open its menu to manage modules.";
        #[cfg(not(target_os = "macos"))]
        let hint = "Running in the system tray. Double-click the tray icon to manage modules.";
        if !taskbar.show_balloon("Automation Platform", hint, 0, 0, None) {
            announce(hint);
        }
        std::mem::forget(taskbar); // keep the icon + its handlers alive

        // wxWidgets owns the loop now, so this recurring tick is how our OS events
        // (hotkeys / captured keys / foreground changes) reach Luau. The init
        // closure returns *before* the loop runs, so the timer must outlive it —
        // leak it for the app's lifetime (its Drop would stop the wxTimer).
        let timer = Timer::new(&frame);
        {
            let inbox = inbox.clone();
            let (rows, busy, list) = (rows.clone(), busy.clone(), list.clone());
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
                            for (id, repo, transition) in &v {
                                updates_list.append(&format!("{id}  {transition}  ({repo})"));
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
                                    for info in &infos {
                                        rows.borrow_mut().push(Row::new(info));
                                    }
                                    list.rebuild(&rows.borrow());
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
                        Job::Updated(id) => {
                            busy.set(false);
                            update_btn.enable(true);
                            // The files changed on disk; now rebuild what is RUNNING.
                            // Every module that inherits this one holds a copy of its
                            // code, so the reload cascades to them -- that is what used
                            // to need a restart.
                            let idx =
                                rows.borrow().iter().find(|r| r.id == id).map(|r| r.module_idx);
                            let msg = match idx {
                                None => format!(
                                    "Updated \u{201c}{id}\u{201d} on disk. It is not loaded in this \
                                     session, so there is nothing to reload."
                                ),
                                Some(idx) => match on_reload(idx) {
                                    Ok((info, report)) => {
                                        {
                                            let mut rb = rows.borrow_mut();
                                            if let Some(r) =
                                                rb.iter_mut().find(|r| r.module_idx == idx)
                                            {
                                                r.settings = info.settings.clone();
                                                r.dependencies = info.dependencies.clone();
                                                r.name = info.name.clone();
                                                r.label = row_label(
                                                    &info.name,
                                                    &info.version,
                                                    &info.id,
                                                );
                                            }
                                        }
                                        let others = report.reloaded.len().saturating_sub(1);
                                        let mut m = if others == 0 {
                                            format!(
                                                "Updated \u{201c}{id}\u{201d} and reloaded it \u{2014} running now."
                                            )
                                        } else {
                                            format!(
                                                "Updated \u{201c}{id}\u{201d} and reloaded it, along with {} module(s) that depend on it: {}. All running now.",
                                                others,
                                                report.reloaded[1..].join(", ")
                                            )
                                        };
                                        if !report.failed.is_empty() {
                                            m.push_str(&format!(
                                                "\n\nThese could not be rebuilt and are now inactive: {}.",
                                                report
                                                    .failed
                                                    .iter()
                                                    .map(|(i, e)| format!("{i} ({e})"))
                                                    .collect::<Vec<_>>()
                                                    .join("; ")
                                            ));
                                        }
                                        m
                                    }
                                    Err(e) => format!(
                                        "Updated \u{201c}{id}\u{201d} on disk, but reloading it failed:\n{e}\n\nIt is now inactive; restart the app to retry."
                                    ),
                                },
                            };
                            modal_message(&frame, "Updated", &msg, false);
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
/// setting (checkbox / number field / dropdown / text). On OK, hands each to `apply`.
///
/// Used for a module's own settings and for the application's, because the second thing a
/// screen-reader user learns about a dialog is its shape — and two dialogs that do the same
/// job differently are two things to learn. The caller supplies the title and decides what a
/// changed value means.
fn open_settings_dialog(
    parent: &Frame,
    title: &str,
    descs: &[SettingDesc],
    apply: &mut dyn FnMut(String, settings::Value),
) {
    enum Ctl {
        Bool(CheckBox),
        Num(TextCtrl, bool, Option<f64>, Option<f64>), // (field, integral, min, max)
        Choice(Choice, Vec<String>),
        Text(TextCtrl),
    }

    let dialog = Dialog::builder(parent, title).build();
    // Controls go on a Panel (not the bare Dialog): standard wxWidgets practice,
    // and it gives the screen reader correct control labeling + Tab navigation.
    let panel = Panel::builder(&dialog).build();
    let sizer = BoxSizer::builder(Orientation::Vertical).build();
    let mut controls: Vec<(String, Ctl)> = Vec::with_capacity(descs.len());

    for d in descs {
        match d.kind {
            settings::Kind::Bool => {
                // Label AND name, and no StaticText in front — see the Application
                // settings tab for why the leading StaticText was the wrong shape. The
                // fields below keep theirs: a text field cannot carry a label at all.
                let cb = CheckBox::builder(&panel).with_label(&d.label).build();
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
                apply(key.clone(), v);
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
fn refresh_settings_btn(list: &InstalledList, rows: &[Row], settings_btn: &Button) {
    let has_settings = list
        .selection()
        .and_then(|sel| rows.get(sel))
        .map(|r| !r.settings.is_empty())
        .unwrap_or(false);
    settings_btn.enable(has_settings);
}

/// One row's checkbox changed: record it and tell the host.
///
/// Reads the control rather than assuming, so the Space handler and the platform's own
/// toggle both end up here saying the same thing. Reporting only a real change makes it
/// safe to call twice for one keystroke.
fn apply_toggle(
    list: &InstalledList,
    rows: &RefCell<Vec<Row>>,
    i: usize,
    on_toggle: &RefCell<Box<dyn FnMut(usize, bool)>>,
) {
    let now = list.checked(i);
    let changed = {
        let mut rb = rows.borrow_mut();
        match rb.get_mut(i) {
            Some(r) if r.enabled != now => {
                r.enabled = now;
                Some(r.module_idx)
            }
            _ => None,
        }
    };
    if let Some(module_idx) = changed {
        on_toggle.borrow_mut()(module_idx, now);
    }
}

/// Native Windows checkbox support for `wxTreeCtrl` (`TVS_CHECKBOXES`). wxdragon
/// doesn't expose it, so we drive the underlying `SysTreeView32` directly — this
/// gives real, UIA-exposed checkboxes (screen-reader-correct) rather than the
/// generic/owner-drawn lists wxWidgets offers cross-platform.
///
/// Windows only, and there is no stub for anywhere else: this module used to carry one,
/// along with a `supported()` that answered `false` so that callers would not act on a
/// fabricated reading. Nothing needs that any more — the platforms now use different
/// controls (see `InstalledList`), and each side reads a real one.
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
