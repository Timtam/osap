//! Minimal UI Automation: does a window's UIA subtree contain an element with a
//! given Name + ControlType? Used to confirm a plugin's identity where a window-
//! class match alone is ambiguous — e.g. sforzando exposes a Pane (ControlType
//! 50033) named "PlogueXMLGUI", which ReaHotkey also keys on.

use std::cell::RefCell;

use windows::core::{BSTR, VARIANT};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
    TreeScope_Descendants, TreeScope_Subtree, UIA_ControlTypePropertyId, UIA_NamePropertyId,
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

/// Dev/diagnostic: walk `hwnd`'s UIA subtree in the RAW view and return the
/// "interesting" elements — those with a non-empty Name, plus Qt window panes —
/// as (depth, Name, ClassName, ControlType). Bounded (≤600 nodes, ≤16 deep). Used
/// to discover the Name/ClassName/ControlType to key a plugin's identity on, e.g.
/// a Kontakt rendered inside Komplete Kontrol's own Qt window.
pub fn uia_dump(hwnd: isize) -> Vec<(i32, String, String, i32)> {
    let mut out: Vec<(i32, String, String, i32)> = Vec::new();
    AUTOMATION.with(|cell| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            *borrow =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .ok();
        }
        let automation = match borrow.as_ref() {
            Some(a) => a,
            None => return,
        };
        let root = match automation.ElementFromHandle(HWND(hwnd as *mut _)) {
            Ok(e) => e,
            Err(_) => return,
        };
        // FindAll(Subtree, TrueCondition) — a flat list of all descendants, and
        // unlike the raw TreeWalker it crosses UIA fragment boundaries (e.g. into a
        // Kontakt rendered inside Komplete Kontrol). No depth; capped to stay bounded.
        let cond = match automation.CreateTrueCondition() {
            Ok(c) => c,
            Err(_) => return,
        };
        let arr = match root.FindAll(TreeScope_Subtree, &cond) {
            Ok(a) => a,
            Err(_) => return,
        };
        let len = arr.Length().unwrap_or(0).min(2000);
        for i in 0..len {
            if let Ok(el) = arr.GetElement(i) {
                let name = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                let class = el.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
                let ctype = el.CurrentControlType().map(|t| t.0).unwrap_or(0);
                if !name.is_empty() || !class.is_empty() {
                    out.push((0, name, class, ctype));
                }
            }
        }
    });
    out
}

/// ReaHotkey's FindElement(ClassName) + WalkTree(path).Click port: find the first
/// element in `hwnd`'s raw subtree whose ClassName CONTAINS `class_substr` and whose
/// ControlType == `ctype`, then navigate ReaHotkey-WalkTree-style — `child` (>=1) =
/// the nth child (first child + child-1 next siblings), then `sibling` raw-view
/// siblings (negative = previous, positive = next) — and return that element's
/// bounding-rect centre (to click it), or None if not found / off-screen. Closes
/// KK's library browser (FileTypeSelector, child=0 sibling=-1) and Kontakt's
/// What's-New dialog (WhatsNewScreen, child=2 sibling=0).
pub fn uia_class_nav_point(
    hwnd: isize,
    class_substr: &str,
    ctype: i32,
    child: i32,
    sibling: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            *borrow =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .ok();
        }
        let automation = borrow.as_ref()?;
        let root = automation.ElementFromHandle(HWND(hwnd as *mut _)).ok()?;
        let walker = automation.RawViewWalker().ok()?;
        let mut el = find_by_class(&walker, &root, class_substr, ctype, 0)?;
        // WalkTree `n` (nth child): first child, then n-1 next siblings.
        if child >= 1 {
            el = walker.GetFirstChildElement(&el).ok()?;
            let mut k = child - 1;
            while k > 0 {
                el = walker.GetNextSiblingElement(&el).ok()?;
                k -= 1;
            }
        }
        // WalkTree `-n` / `+n` (nth previous / next sibling).
        let mut n = sibling;
        while n < 0 {
            el = walker.GetPreviousSiblingElement(&el).ok()?;
            n += 1;
        }
        while n > 0 {
            el = walker.GetNextSiblingElement(&el).ok()?;
            n -= 1;
        }
        let r = el.CurrentBoundingRectangle().ok()?;
        if r.right <= r.left || r.bottom <= r.top {
            return None;
        }
        Some(((r.left + r.right) / 2, (r.top + r.bottom) / 2))
    })
}

/// First element (depth-first, raw view) whose ControlType == `ctype` and whose
/// ClassName contains `class_substr`. The raw TreeWalker descends INTO a Qt plugin's
/// QML content (KK's library browser), which the condition-based FindAll does not —
/// it stops at the QuickWindow. Returns an owned (AddRef'd) handle.
unsafe fn find_by_class(
    walker: &IUIAutomationTreeWalker,
    el: &IUIAutomationElement,
    class_substr: &str,
    ctype: i32,
    depth: i32,
) -> Option<IUIAutomationElement> {
    if depth > 25 {
        return None;
    }
    let class = el.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
    let t = el.CurrentControlType().map(|t| t.0).unwrap_or(0);
    if t == ctype && class.contains(class_substr) {
        return Some(el.clone());
    }
    let mut child = match walker.GetFirstChildElement(el) {
        Ok(c) => c,
        Err(_) => return None,
    };
    loop {
        if let Some(found) = find_by_class(walker, &child, class_substr, ctype, depth + 1) {
            return Some(found);
        }
        child = match walker.GetNextSiblingElement(&child) {
            Ok(n) => n,
            Err(_) => return None,
        };
    }
}

/// Tab pass-through for a standalone plugin window (e.g. Kontakt N.exe). Such a window
/// does NOT move keyboard focus on Tab itself, so ReaHotkey — and this — drive it via
/// UIA. Like ReaHotkey's standalone pass-through (its MainElement = window.ElementFromPath
/// (1)), the Tab ring is scoped to the window's FIRST CHILD — the content area — so the
/// window frame and the menu bar, which sit OUTSIDE that content subtree, drop out of the
/// ring on their own (no control-type blocklist needed). Within that scope: enumerate the
/// visible keyboard-focusable descendants in control-view order, find the one focused now,
/// and `SetFocus` the NEXT (`direction` >= 0) / PREVIOUS one, wrapping at the ends. Any
/// candidate that does not actually take focus (a container that redirects it, an item
/// that ignores SetFocus) is skipped — verified by reading focus straight back. Re-
/// enumerated every step, so it tracks a tree that shifts as the user navigates. Returns
/// the newly focused element's (Name, ControlType, 1-based index, count) to announce, or
/// None if the scope has no focusable descendant that accepts focus.
pub fn uia_focus_step(hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let mut borrow = cell.borrow_mut();
        if borrow.is_none() {
            *borrow =
                CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .ok();
        }
        let automation = borrow.as_ref()?;
        let root = automation.ElementFromHandle(HWND(hwnd as *mut _)).ok()?;

        // Scope to the window's content area — its first child in the control view, i.e.
        // ReaHotkey's ElementFromPath(1). This structurally drops the window frame and the
        // menu bar (siblings of the content, not descendants of it) from the ring. Fall
        // back to the window itself if it has no child.
        let scope = automation
            .ControlViewWalker()
            .ok()
            .and_then(|w| w.GetFirstChildElement(&root).ok())
            .unwrap_or_else(|| root.clone());

        // The scope's VISIBLE, keyboard-focusable descendants in control-view order — the
        // same set NVDA would Tab through. TrueCondition + Rust-side filter avoids a bool
        // VARIANT and matches uia_dump's proven path.
        let cond = automation.CreateTrueCondition().ok()?;
        let arr = scope.FindAll(TreeScope_Descendants, &cond).ok()?;
        let len = arr.Length().unwrap_or(0);
        let mut items: Vec<IUIAutomationElement> = Vec::new();
        for i in 0..len {
            if let Ok(el) = arr.GetElement(i) {
                if !el.CurrentIsKeyboardFocusable().map(|b| b.as_bool()).unwrap_or(false) {
                    continue;
                }
                if el.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(false) {
                    continue; // collapsed panel / hidden tab — not a real stop
                }
                items.push(el);
            }
        }
        if items.is_empty() {
            return None;
        }
        let count = items.len() as i32;

        // Index of the element focused now, or -1 if focus is outside the ring.
        let focused = automation.GetFocusedElement().ok();
        let mut cur: i32 = -1;
        if let Some(f) = focused.as_ref() {
            for (i, el) in items.iter().enumerate() {
                if automation.CompareElements(f, el).map(|b| b.as_bool()).unwrap_or(false) {
                    cur = i as i32;
                    break;
                }
            }
        }

        // Step in `direction`, wrapping at the ends, and skip any candidate that does not
        // actually take focus (read focus back to confirm). Bounded by `count`, so it tries
        // each element at most once before giving up.
        let mut idx = cur;
        for _ in 0..count {
            idx = if idx < 0 {
                if direction >= 0 { 0 } else { count - 1 }
            } else if direction >= 0 {
                (idx + 1) % count
            } else {
                (idx - 1 + count) % count
            };
            let el = &items[idx as usize];
            if el.SetFocus().is_ok() {
                let landed = automation
                    .GetFocusedElement()
                    .ok()
                    .map(|f| {
                        automation.CompareElements(&f, el).map(|b| b.as_bool()).unwrap_or(false)
                    })
                    .unwrap_or(false);
                if landed {
                    let name = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                    let ctype = el.CurrentControlType().map(|t| t.0).unwrap_or(0);
                    return Some((name, ctype, idx + 1, count));
                }
            }
        }
        None
    })
}

