//! Minimal UI Automation: does a window's UIA subtree contain an element with a
//! given Name + ControlType? Used to confirm a plugin's identity where a window-
//! class match alone is ambiguous — e.g. sforzando exposes a Pane (ControlType
//! 50033) named "PlogueXMLGUI", which ReaHotkey also keys on.

use std::cell::RefCell;

use windows::core::{BSTR, VARIANT};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, TreeScope_Subtree,
    UIA_ControlTypePropertyId, UIA_NamePropertyId,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};

thread_local! {
    // UIA interfaces are thread-affine; cache per thread, created on first use.
    static AUTOMATION: RefCell<Option<IUIAutomation>> = RefCell::new(None);
}

/// Finds the first element in `hwnd`'s UIA subtree whose Name == `name` and
/// ControlType == `control_type`. Never panics; any failure / no-match → None.
fn find_element(hwnd: isize, name: &str, control_type: i32) -> Option<IUIAutomationElement> {
    AUTOMATION.with(|cell| unsafe {
        // The app's main thread already RoInitialize's COM (MTA) for WinRT OCR;
        // this is a harmless S_FALSE there and initializes the MTA otherwise.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            *borrow =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .ok();
        }
        let automation = borrow.as_ref()?;

        let element = automation.ElementFromHandle(HWND(hwnd as *mut _)).ok()?;

        // ControlType == `control_type` (VT_I4), AND Name == `name` (VT_BSTR) when
        // a name is given; an empty name means "any element of this type" (e.g.
        // detecting an open Menu). VARIANTs free their data on drop.
        let v_ctype: VARIANT = control_type.into();
        let cond_ctype = automation
            .CreatePropertyCondition(UIA_ControlTypePropertyId, &v_ctype)
            .ok()?;
        let cond = if name.is_empty() {
            cond_ctype
        } else {
            let v_name: VARIANT = BSTR::from(name).into();
            let cond_name = automation.CreatePropertyCondition(UIA_NamePropertyId, &v_name).ok()?;
            automation.CreateAndCondition(&cond_name, &cond_ctype).ok()?
        };

        // FindFirst is on the element; TreeScope_Subtree includes the element
        // itself. A no-match comes back as Err in windows-rs (null → Err).
        element.FindFirst(TreeScope_Subtree, &cond).ok()
    })
}

/// True if `hwnd`'s UIA subtree contains an element with that Name + ControlType.
pub fn uia_find(hwnd: isize, name: &str, control_type: i32) -> bool {
    find_element(hwnd, name, control_type).is_some()
}

/// The screen-pixel centre of that element's bounding rectangle (to click it), or
/// None if not found / it has no on-screen rect.
pub fn uia_locate(hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)> {
    let element = find_element(hwnd, name, control_type)?;
    unsafe {
        let r = element.CurrentBoundingRectangle().ok()?;
        if r.right <= r.left || r.bottom <= r.top {
            return None; // collapsed / off-screen
        }
        Some(((r.left + r.right) / 2, (r.top + r.bottom) / 2))
    }
}
