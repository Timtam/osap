//! Minimal UI Automation: does a window's UIA subtree contain an element with a
//! given Name + ControlType? Used to confirm a plugin's identity where a window-
//! class match alone is ambiguous — e.g. sforzando exposes a Pane (ControlType
//! 50033) named "PlogueXMLGUI", which ReaHotkey also keys on.

use std::cell::RefCell;

use windows::core::{BSTR, VARIANT};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, TreeScope_Subtree, UIA_ControlTypePropertyId, UIA_NamePropertyId,
};

thread_local! {
    // UIA interfaces are thread-affine; cache per thread, created on first use.
    static AUTOMATION: RefCell<Option<IUIAutomation>> = RefCell::new(None);
}

/// True if `hwnd`'s UIA subtree contains an element whose Name == `name` and
/// ControlType == `control_type`. Never panics; any failure or no-match → false.
pub fn uia_find(hwnd: isize, name: &str, control_type: i32) -> bool {
    AUTOMATION.with(|cell| unsafe {
        // The app's main thread already RoInitialize's COM (MTA) for WinRT OCR;
        // this is a harmless S_FALSE there and initializes the MTA otherwise.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            match CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
                Ok(a) => *borrow = Some(a),
                Err(_) => return false,
            }
        }
        let automation = borrow.as_ref().unwrap();

        let element = match automation.ElementFromHandle(HWND(hwnd as *mut _)) {
            Ok(e) => e,
            Err(_) => return false,
        };

        // Name == `name` (VT_BSTR) AND ControlType == `control_type` (VT_I4). The
        // VARIANTs own their data and free it on drop (no manual SysFreeString).
        let v_name: VARIANT = BSTR::from(name).into();
        let cond_name = match automation.CreatePropertyCondition(UIA_NamePropertyId, &v_name) {
            Ok(c) => c,
            Err(_) => return false,
        };
        let v_ctype: VARIANT = control_type.into();
        let cond_ctype =
            match automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &v_ctype) {
                Ok(c) => c,
                Err(_) => return false,
            };
        let cond = match automation.CreateAndCondition(&cond_name, &cond_ctype) {
            Ok(c) => c,
            Err(_) => return false,
        };

        // FindFirst is on the element; TreeScope_Subtree includes the element
        // itself. A no-match comes back as Err in windows-rs (null → Err), so
        // is_ok() == "found".
        element.FindFirst(TreeScope_Subtree, &cond).is_ok()
    })
}
