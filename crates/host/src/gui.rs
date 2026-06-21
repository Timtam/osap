//! wxDragon GUI host: a **tray-resident module manager**. wxWidgets owns the
//! main message loop; our OS events are pumped from a wx `Timer` tick
//! (event-loop coexistence, see `docs/architecture-feasibility-study.md` §11.6).
//!
//! The window lists loaded modules with enable/disable checkboxes and lives in
//! the system tray: it starts hidden (modules are needed far more often than
//! their management), closing it (X) hides it back to the tray, and only the
//! tray "Quit" actually exits. Double-clicking the tray icon reopens it.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use wxdragon::prelude::*;

const MENU_SHOW: i32 = 1001;
const MENU_QUIT: i32 = 1002;

/// One row in the module-manager list.
pub struct ModuleInfo {
    pub name: String,
    pub version: String,
    pub id: String,
    pub enabled: bool,
}

/// Runs the tray-resident module manager. `on_toggle(idx, enabled)` fires when
/// the user checks/unchecks a module; `pump` drains OS events each tick. Blocks
/// until the user chooses Quit.
pub fn run_gui(
    modules: Vec<ModuleInfo>,
    on_toggle: impl FnMut(usize, bool) + 'static,
    mut pump: impl FnMut() + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    wxdragon::main(move |app| {
        // The window is a background manager: hiding/closing it must not quit the
        // app — only the tray "Quit" does.
        app.set_exit_on_frame_delete(false);

        let frame = Frame::builder()
            .with_title("Automation Platform — Modules")
            .with_size(Size::new(480, 400))
            .build();

        let panel = Panel::builder(&frame).build();
        let sizer = BoxSizer::builder(Orientation::Vertical).build();

        let heading = StaticText::builder(&panel)
            .with_label("Loaded modules — uncheck to disable, check to enable:")
            .build();
        sizer.add(&heading, 0, SizerFlag::All, 12);

        // A native tree control with TVS_CHECKBOXES: real OS checkboxes that
        // expose the proper toggle state to the screen reader (UIA), unlike a
        // generic/owner-drawn list. TVS_CHECKBOXES must be enabled *before*
        // items are inserted.
        let list = TreeCtrl::builder(&panel)
            .with_style(TreeCtrlStyle::HideRoot | TreeCtrlStyle::Single | TreeCtrlStyle::NoLines)
            .build();
        let hwnd = list.get_handle();
        native_checkboxes::enable(hwnd);
        let mut items: Vec<TreeItemId> = Vec::with_capacity(modules.len());
        if let Some(root) = list.add_root("Modules", None, None) {
            for m in &modules {
                let label = format!("{}  v{}   ({})", m.name, m.version, m.id);
                if let Some(item) = list.append_item(&root, &label, None, None) {
                    native_checkboxes::set(hwnd, &item, m.enabled);
                    items.push(item);
                }
            }
        }
        sizer.add(&list, 1, SizerFlag::All | SizerFlag::Expand, 12);

        let hint = StaticText::builder(&panel)
            .with_label(
                "Closing this window hides it to the system tray; modules keep running.\n\
                 Reopen it from the tray icon (double-click), or choose Quit to exit.",
            )
            .build();
        sizer.add(&hint, 0, SizerFlag::All, 12);

        panel.set_sizer(sizer, true);

        // Detect native checkbox toggles (mouse click on the box, or Space on
        // the focused row). The native control flips the state itself — it
        // toggles on button-/key-*down*, so by the *up* event the new state is
        // already in place. We diff every row against the last-known states and
        // report the change(s).
        let states = Rc::new(RefCell::new(
            modules.iter().map(|m| m.enabled).collect::<Vec<bool>>(),
        ));
        let items = Rc::new(items);
        let on_toggle: Rc<RefCell<Box<dyn FnMut(usize, bool)>>> =
            Rc::new(RefCell::new(Box::new(on_toggle)));
        {
            let (items, states, on_toggle) = (items.clone(), states.clone(), on_toggle.clone());
            list.on_mouse_left_up(move |e| {
                sync_checks(hwnd, &items, &states, &on_toggle);
                e.skip(true);
            });
        }
        {
            let (items, states, on_toggle) = (items.clone(), states.clone(), on_toggle.clone());
            list.on_key_up(move |e| {
                sync_checks(hwnd, &items, &states, &on_toggle);
                e.skip(true);
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
        timer.on_tick(move |_event| pump());
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

/// Reads each row's native check state, reporting any that changed since the
/// last call via `on_toggle`. Called after every mouse-up / key-up on the tree.
fn sync_checks(
    hwnd: *mut c_void,
    items: &[TreeItemId],
    states: &RefCell<Vec<bool>>,
    on_toggle: &RefCell<Box<dyn FnMut(usize, bool)>>,
) {
    let mut states = states.borrow_mut();
    let mut cb = on_toggle.borrow_mut();
    for (idx, item) in items.iter().enumerate() {
        let now = native_checkboxes::get(hwnd, item);
        if states.get(idx).copied() != Some(now) {
            if let Some(slot) = states.get_mut(idx) {
                *slot = now;
            }
            cb(idx, now);
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
}
