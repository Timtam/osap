//! Minimal UI Automation: does a window's UIA subtree contain an element with a
//! given Name + ControlType? Used to confirm a plugin's identity where a window-
//! class match alone is ambiguous — e.g. sforzando exposes a Pane (ControlType
//! 50033) named "PlogueXMLGUI", which ReaHotkey also keys on.

use std::cell::RefCell;

use super::DumpNode;
use windows::core::{Interface, BSTR, VARIANT};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{
    IUIAutomation2,
    CUIAutomation8,
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationLegacyIAccessiblePattern, IUIAutomationTogglePattern, IUIAutomationTreeWalker,
    TreeScope_Descendants, TreeScope_Subtree, UIA_BoundingRectanglePropertyId,
    UIA_ClassNamePropertyId, UIA_ControlTypePropertyId, UIA_NamePropertyId, UIA_PATTERN_ID,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};

thread_local! {
    // UIA interfaces are thread-affine; cache per thread, created on first use.
    static AUTOMATION: RefCell<Option<IUIAutomation>> = RefCell::new(None);
}

/// How long a single cross-process UIA call may take before it is given up on.
///
/// **Every read in this file is synchronous IPC into another application**, and that
/// application answers on its own message pump. One that is busy, mid-repaint, or simply not
/// pumping does not answer at all — and UIA's default is to wait about two minutes for it.
/// On the thread that carries the keyboard, that is not a slow answer, it is a dead
/// application.
///
/// Measured rather than assumed: timing `element_raw_dump` over every top-level window on
/// one desktop, the Chromium-backed ones answered in 65-374 ms for up to 2876 elements,
/// while one wxWidgets window never answered at all — the run stopped in front of it and
/// stayed there. That is the shape this bounds.
///
/// The macOS backend has done exactly this since it was written (`MESSAGING_TIMEOUT` in
/// `macos/ax.rs`, with a comment explaining that an unbounded walk against an unresponsive
/// plugin hangs the thread that carries the keyboard). The same reasoning applies here and
/// nobody had applied it: this platform had no bound at all.
const UIA_TIMEOUT_MS: u32 = 1000;

/// How long a whole tree walk may take before it stops and reports what it has.
///
/// The per-call timeout above bounds ONE question. A walk asks thousands, and an application
/// that answers every one of them slowly is not caught by it: measured after the timeout went
/// in, a WinUI window with an embedded web view took **thirty seconds** to dump — bounded, and
/// still far past anything an event loop can absorb. The node budget does not help either; it
/// bounds nodes, and the cost here is per question rather than per node.
///
/// Five seconds rather than something tighter, because this is a diagnostic: the probe's dump
/// is how a plug-in's tree gets read at all, and a truncated one is worth much less than a
/// slow one. What must not happen is an application that stops answering for half a minute,
/// and a truncated dump says so in its own output rather than looking complete.
const WALK_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// What a bounded walk has left: nodes, and time.
struct Budget {
    nodes: i32,
    until: std::time::Instant,
    /// True once either bound stopped it, so the caller can say the dump is partial instead
    /// of letting a truncated tree read as a complete one.
    stopped: bool,
}

impl Budget {
    fn new(nodes: i32) -> Self {
        Self { nodes, until: std::time::Instant::now() + WALK_DEADLINE, stopped: false }
    }

    /// Spends one node. False when there is nothing left to spend.
    ///
    /// The clock is read every 64 nodes rather than every one: `Instant::now` is a syscall on
    /// some platforms, and a walk that measured itself more often than it worked would be
    /// its own problem. 64 nodes is well under a second even at the timeout above.
    fn spend(&mut self) -> bool {
        if self.nodes <= 0 {
            self.stopped = true;
            return false;
        }
        self.nodes -= 1;
        if self.nodes % 64 == 0 && std::time::Instant::now() >= self.until {
            self.stopped = true;
            return false;
        }
        true
    }
}

/// The thread's automation object, created on first use with its timeouts set.
///
/// One place rather than nine. Every function here used to open its own with the same six
/// lines, which is how the timeouts came to be missing everywhere at once: there was no
/// single place to put them.
fn automation(cell: &RefCell<Option<IUIAutomation>>) -> std::cell::RefMut<'_, Option<IUIAutomation>> {
    // The app's main thread already RoInitialize's COM (MTA) for WinRT OCR;
    // this is a harmless S_FALSE there and initializes the MTA otherwise.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let mut borrow = cell.borrow_mut();
    if borrow.is_none() {
        // `CUIAutomation8` rather than `CUIAutomation`, and the difference is the whole point
        // of this function: the older class does not implement `IUIAutomation2`, so asking it
        // for the timeouts returns E_NOINTERFACE and the bound is silently not applied. That
        // was the first version of this code, and it looked like it worked — the measurement
        // is what said otherwise. `CUIAutomation8` has been present since Windows 8; the
        // fall-back is for anything older, where there is no timeout to set anyway.
        let made = unsafe {
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .or_else(|_| {
                    CoCreateInstance::<_, IUIAutomation>(
                        &CUIAutomation,
                        None,
                        CLSCTX_INPROC_SERVER,
                    )
                })
        }
        .ok();
        if let Some(a) = made.as_ref() {
            // `IUIAutomation2` is where the timeouts live and it is not on every Windows this
            // runs on, so a failure to reach it is not an error — it is the older behaviour,
            // said out loud once rather than left to be discovered by a hang.
            match a.cast::<IUIAutomation2>() {
                Ok(a2) => unsafe {
                    let _ = a2.SetConnectionTimeout(UIA_TIMEOUT_MS);
                    let _ = a2.SetTransactionTimeout(UIA_TIMEOUT_MS);
                    crate::logging::line(
                        "uia",
                        &format!(
                            "accessibility call timeout set to {UIA_TIMEOUT_MS} ms for this \
                             thread (the default is about two minutes)"
                        ),
                    );
                },
                Err(e) => crate::logging::line(
                    "uia",
                    &format!(
                        "IUIAutomation2 unavailable ({e}); calls into an application that \
                         does not answer will wait for the system default"
                    ),
                ),
            }
        }
        *borrow = made;
    }
    borrow
}

/// Finds the first element in `hwnd`'s UIA subtree whose Name == `name` and
/// ControlType == `control_type`. Never panics; any failure / no-match → None.
fn find_element(hwnd: isize, name: &str, control_type: i32) -> Option<IUIAutomationElement> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
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
pub fn element_find(hwnd: isize, name: &str, control_type: i32) -> bool {
    find_element(hwnd, name, control_type).is_some()
}

/// "Is any of these names present as any of these control types?" — the shape almost
/// every plugin-identity check takes ("Kontakt 8" as Window OR Pane). Answers it with a
/// SINGLE tree traversal built from one OR-condition, instead of one full subtree walk
/// per name×type pair; the walk dominates the cost, and identity is re-checked on every
/// detection pass. Returns the 1-based index of the matching NAME (so the caller learns
/// WHICH one matched, e.g. the plugin's version), or None.
///
/// An empty name matches any element of the given types.
pub fn element_find_any(hwnd: isize, names: &[String], types: &[i32]) -> Option<usize> {
    if names.is_empty() || types.is_empty() {
        return None;
    }
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = borrow.as_ref()?;
        let element = automation.ElementFromHandle(HWND(hwnd as *mut _)).ok()?;

        // OR over the control types, ANDed with the name when one is given. One
        // traversal per NAME (a name is what we must tell apart in the result); the
        // types — the part that multiplied the walks — collapse into the condition.
        // Folded pairwise rather than via CreateOrConditionFromArray, which wants a
        // SAFEARRAY; the type list is a handful of entries, so the shape is irrelevant.
        let mut cond_type: Option<IUIAutomationCondition> = None;
        for t in types {
            let v: VARIANT = (*t).into();
            let c = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &v).ok()?;
            cond_type = Some(match cond_type {
                None => c,
                Some(prev) => automation.CreateOrCondition(&prev, &c).ok()?,
            });
        }
        let cond_type = cond_type?;

        for (i, name) in names.iter().enumerate() {
            let cond = if name.is_empty() {
                cond_type.clone()
            } else {
                let v: VARIANT = BSTR::from(name.as_str()).into();
                let cond_name =
                    automation.CreatePropertyCondition(UIA_NamePropertyId, &v).ok()?;
                match automation.CreateAndCondition(&cond_name, &cond_type) {
                    Ok(c) => c,
                    Err(_) => continue,
                }
            };
            if element.FindFirst(TreeScope_Subtree, &cond).is_ok() {
                return Some(i + 1);
            }
        }
        None
    })
}

/// The screen-pixel centre of that element's bounding rectangle (to click it), or
/// None if not found / it has no on-screen rect.
pub fn element_locate(hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)> {
    let element = find_element(hwnd, name, control_type)?;
    unsafe {
        let r = element.CurrentBoundingRectangle().ok()?;
        if r.right <= r.left || r.bottom <= r.top {
            return None; // collapsed / off-screen
        }
        Some(((r.left + r.right) / 2, (r.top + r.bottom) / 2))
    }
}

/// Like `element_locate`, but first descends into a CONTAINER element (`via_name` /
/// `via_type`, e.g. the "Kontakt 8" QuickWindow pane) reachable from `hwnd`, then
/// searches for the target WITHIN that container. A DAW-embedded plugin exposes the
/// container element but hosts its real UI as a nested UIA fragment that a search from
/// the outer `hwnd` does NOT cross — searching from the container element itself does.
/// Ports ReaHotkey's GetPluginUIAElement + `MainElement.FindElement(...)`. Returns the
/// target's on-screen click centre, or None (container or target not found / off-screen).
pub fn element_locate_via(
    hwnd: isize,
    via_name: &str,
    via_type: i32,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = borrow.as_ref()?;
        let root = automation.ElementFromHandle(HWND(hwnd as *mut _)).ok()?;

        // Build a Name(+Type) condition; an empty name means "any of this type".
        let make_cond = |nm: &str, ct: i32| -> Option<IUIAutomationCondition> {
            let v_ct: VARIANT = ct.into();
            let cond_ct = automation
                .CreatePropertyCondition(UIA_ControlTypePropertyId, &v_ct)
                .ok()?;
            if nm.is_empty() {
                Some(cond_ct)
            } else {
                let v_nm: VARIANT = BSTR::from(nm).into();
                let cond_nm = automation
                    .CreatePropertyCondition(UIA_NamePropertyId, &v_nm)
                    .ok()?;
                automation.CreateAndCondition(&cond_nm, &cond_ct).ok()
            }
        };

        // Find the container (the plugin's identity pane), then search WITHIN it —
        // this crosses the hosted-fragment boundary that a search from `root` does not.
        let cond_via = make_cond(via_name, via_type)?;
        let container = root.FindFirst(TreeScope_Subtree, &cond_via).ok()?;
        let cond = make_cond(name, control_type)?;
        let el = container.FindFirst(TreeScope_Subtree, &cond).ok()?;
        let r = el.CurrentBoundingRectangle().ok()?;
        if r.right <= r.left || r.bottom <= r.top {
            return None;
        }
        Some(((r.left + r.right) / 2, (r.top + r.bottom) / 2))
    })
}

/// An element's on-screen rectangle, or all zeros when it has none.
///
/// Part of a dump because a dump exists to let somebody author coordinates for a machine
/// they cannot look at, and "there is a slider" without "and it is here" is half an answer.
fn rect_of(el: &IUIAutomationElement) -> (i32, i32, i32, i32) {
    match unsafe { el.CurrentBoundingRectangle() } {
        Ok(r) => (r.left, r.top, r.right - r.left, r.bottom - r.top),
        Err(_) => (0, 0, 0, 0),
    }
}

/// Dev/diagnostic: walk `hwnd`'s UIA subtree in the RAW view and return the
/// "interesting" elements — those with a non-empty Name, plus Qt window panes —
/// as (depth, Name, ClassName, ControlType). Bounded (≤600 nodes, ≤16 deep). Used
/// to discover the Name/ClassName/ControlType to key a plugin's identity on, e.g.
/// a Kontakt rendered inside Komplete Kontrol's own Qt window.
pub fn element_dump(hwnd: isize) -> Vec<DumpNode> {
    let mut out: Vec<DumpNode> = Vec::new();
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
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
                    let (x, y, w, h) = rect_of(&el);
                    out.push(DumpNode { depth: 0, name, class, ctype, x, y, w, h });
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
pub fn element_class_nav_point(
    hwnd: isize,
    class_substr: &str,
    ctype: i32,
    child: i32,
    sibling: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
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

/// A raw-view depth-first walk, bounded by node budget and depth, calling `visit` for
/// every element. `visit` returning false stops the walk. The RAW walker is the only
/// way into a Qt plugin's QML content: the condition-based FindAll/FindFirst stops at
/// the QuickWindow fragment boundary, so a DAW-embedded Kontakt looks empty to it.
unsafe fn raw_walk(
    walker: &IUIAutomationTreeWalker,
    el: &IUIAutomationElement,
    depth: i32,
    budget: &mut Budget,
    visit: &mut impl FnMut(&IUIAutomationElement, i32) -> bool,
) -> bool {
    if depth > 40 || !budget.spend() {
        return true;
    }
    if !visit(el, depth) {
        return false;
    }
    let mut child = match walker.GetFirstChildElement(el) {
        Ok(c) => c,
        Err(_) => return true,
    };
    loop {
        if !raw_walk(walker, &child, depth + 1, budget, visit) {
            return false;
        }
        if budget.nodes <= 0 || budget.stopped {
            return true;
        }
        child = match walker.GetNextSiblingElement(&child) {
            Ok(n) => n,
            Err(_) => return true,
        };
    }
}

/// Dev/diagnostic counterpart to `element_dump` that uses the RAW TreeWalker instead of a
/// condition-based FindAll, so it crosses into a hosted Qt fragment (a DAW-embedded
/// Kontakt's real UI, which FindAll cannot see). Returns (depth, Name, ClassName,
/// ControlType); bounded by node budget and depth.
/// Walks a subtree that has already been fetched, reading only cached properties.
///
/// Every call in here is local. `GetCachedChildren` returns what the one round trip already
/// brought back, and each `Cached…` accessor reads a property out of that snapshot — which is
/// the entire point, and the difference between a walk that costs milliseconds and one that
/// costs a fifth of a second.
unsafe fn cached_walk(
    el: &IUIAutomationElement,
    depth: i32,
    budget: &mut Budget,
    out: &mut Vec<DumpNode>,
) {
    if depth > 40 || !budget.spend() {
        return;
    }
    let name = el.CachedName().map(|b| b.to_string()).unwrap_or_default();
    let class = el.CachedClassName().map(|b| b.to_string()).unwrap_or_default();
    let ctype = el.CachedControlType().map(|t| t.0).unwrap_or(0);
    if !name.is_empty() || !class.is_empty() {
        let (x, y, w, h) = match el.CachedBoundingRectangle() {
            Ok(r) => (r.left, r.top, r.right - r.left, r.bottom - r.top),
            Err(_) => (0, 0, 0, 0),
        };
        out.push(DumpNode { depth, name, class, ctype, x, y, w, h });
    }
    let Ok(children) = el.GetCachedChildren() else {
        return;
    };
    let n = children.Length().unwrap_or(0);
    for i in 0..n {
        if let Ok(child) = children.GetElement(i) {
            cached_walk(&child, depth + 1, budget, out);
        }
    }
}

pub fn element_raw_dump(hwnd: isize) -> Vec<DumpNode> {
    let mut out: Vec<DumpNode> = Vec::new();
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = match borrow.as_ref() {
            Some(a) => a,
            None => return,
        };
        let root = match automation.ElementFromHandle(HWND(hwnd as *mut _)) {
            Ok(e) => e,
            Err(_) => return,
        };

        // ONE crossing of the process boundary, instead of six per element.
        //
        // This was measured rather than suspected: a module reading a 53-element JUCE window
        // saw 211 ms per walk, and every keystroke that wanted a value paid it. The old path
        // below asks the target process for a name, a class, a control type and a rectangle
        // one property at a time, and the walker asks for each child and each sibling on top
        // — about six round trips per element, at roughly four milliseconds each, which is
        // simply what cross-process UIA costs.
        //
        // A cache request says up front which properties are wanted and over what scope, and
        // `BuildUpdatedCache` fetches the lot in a single call. The tree filter is the RAW
        // view, deliberately: this function exists to see into hosted fragments that the
        // condition-based views stop at — an embedded Kontakt looked like it had no content
        // at all until this walked raw — and a cache request that filtered to the control
        // view would quietly undo that.
        let cached = automation.CreateCacheRequest().ok().and_then(|req| {
            req.AddProperty(UIA_NamePropertyId).ok()?;
            req.AddProperty(UIA_ClassNamePropertyId).ok()?;
            req.AddProperty(UIA_ControlTypePropertyId).ok()?;
            req.AddProperty(UIA_BoundingRectanglePropertyId).ok()?;
            req.SetTreeScope(TreeScope_Subtree).ok()?;
            req.SetTreeFilter(&automation.RawViewCondition().ok()?).ok()?;
            root.BuildUpdatedCache(&req).ok()
        });

        if let Some(cached_root) = cached {
            let mut budget = Budget::new(4000);
            cached_walk(&cached_root, 0, &mut budget, &mut out);
            if !out.is_empty() {
                report_partial(&budget, out.len());
                return;
            }
            // An empty result from a cache that reported success is not proof of an empty
            // window — some providers answer a subtree request with only the root — so it
            // falls through rather than reporting nothing, and the old path settles it.
            crate::logging::trace("uia", || {
                "cached subtree came back empty; walking the raw tree instead".to_string()
            });
        }

        // The way it was done before, kept as the answer for whatever the cache cannot do.
        // Slow, and correct on every provider this project has ever met.
        let walker = match automation.RawViewWalker() {
            Ok(w) => w,
            Err(_) => return,
        };
        let mut budget = Budget::new(4000);
        raw_walk(&walker, &root, 0, &mut budget, &mut |el, depth| {
            let name = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
            let class = el.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
            let ctype = el.CurrentControlType().map(|t| t.0).unwrap_or(0);
            if !name.is_empty() || !class.is_empty() {
                let (x, y, w, h) = rect_of(el);
                out.push(DumpNode { depth, name, class, ctype, x, y, w, h });
            }
            true
        });
        report_partial(&budget, out.len());
    });
    out
}

/// Says so when a dump stopped early, because a truncated tree is indistinguishable from a
/// small one to whoever reads it afterwards — and this output is read by somebody working
/// from a log file on a machine nobody here can touch.
fn report_partial(budget: &Budget, found: usize) {
    if budget.stopped {
        crate::logging::line(
            "uia",
            &format!(
                "the accessibility dump stopped early with {found} element(s) — it ran out \
                 of {} and what is above is PART of the tree, not all of it",
                if budget.nodes <= 0 { "nodes" } else { "time" }
            ),
        );
    }
}

/// ReaHotkey's `GetPluginUIAElement` + `MainElement.FindElement(...)`, ported.
///
/// A DAW-embedded plugin hosts its real UI as a nested UIA fragment. ReaHotkey reaches
/// it by finding the element that IS the plugin — Name == `container_name` (e.g.
/// "Kontakt 8") with ControlType Window (50032) or Pane (50033) — preferring the
/// `ni::qt::QuickWindow` class (the Qt scene root, which has the content) over the
/// `…QWindowIcon` HWND host (a content-empty proxy), and then searching for the target
/// WITHIN it. We do the same, but over the RAW tree walker: the condition-based search
/// stops at the fragment boundary, which is exactly why an embedded Kontakt looked like
/// it had no accessible content at all.
///
/// `name` empty means "any element of this ControlType" (e.g. an open Menu, 50009).
/// Returns the target's on-screen click centre, or None.
pub fn element_plugin_locate(
    hwnd: isize,
    container_name: &str,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = borrow.as_ref()?;
        let root = automation.ElementFromHandle(HWND(hwnd as *mut _)).ok()?;
        let walker = automation.RawViewWalker().ok()?;

        // Every element that IS the plugin (ReaHotkey's CheckElement), in raw-tree order.
        let mut containers: Vec<(bool, IUIAutomationElement)> = Vec::new();
        let mut budget = Budget::new(4000);
        raw_walk(&walker, &root, 0, &mut budget, &mut |el, _| {
            let t = el.CurrentControlType().map(|t| t.0).unwrap_or(0);
            if t == 50032 || t == 50033 {
                let n = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                if n == container_name {
                    let class = el.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
                    // ReaHotkey's Criteria order: ni::qt::QuickWindow before QWindowIcon.
                    containers.push((class.contains("QuickWindow"), el.clone()));
                }
            }
            true
        });
        if containers.is_empty() {
            return None;
        }
        containers.sort_by_key(|(is_quick, _)| !*is_quick);

        for (_, container) in &containers {
            let mut hit: Option<(i32, i32)> = None;
            let mut budget = Budget::new(4000);
            raw_walk(&walker, container, 0, &mut budget, &mut |el, _| {
                let t = el.CurrentControlType().map(|t| t.0).unwrap_or(0);
                if t != control_type {
                    return true;
                }
                if !name.is_empty() {
                    let n = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                    if n != name {
                        return true;
                    }
                }
                if let Ok(r) = el.CurrentBoundingRectangle() {
                    if r.right > r.left && r.bottom > r.top {
                        hit = Some(((r.left + r.right) / 2, (r.top + r.bottom) / 2));
                        return false; // found — stop walking
                    }
                }
                true
            });
            if hit.is_some() {
                return hit;
            }
        }
        None
    })
}

/// What a named element says about its own STATE, as a diagnostic string.
///
/// Written because "UIA has nothing to offer here" was a conclusion drawn from the control
/// TYPE — Kontakt's status-bar toggles come back as plain Buttons — and nothing had ever asked
/// them a state question. Only Name, ClassName and ControlType were ever fetched, so the
/// absence of a toggle state was never observed, only assumed. This asks properly: the Toggle
/// pattern first, then LegacyIAccessible's state bits, which is where a control that behaves
/// like a checkbox without declaring itself one usually keeps it.
///
/// Returns None when the element cannot be found at all, so "not found" and "found, says
/// nothing" stay distinguishable.
pub fn element_state_probe(
    hwnd: isize,
    container_name: &str,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = borrow.as_ref()?;
        let root = automation.ElementFromHandle(HWND(hwnd as *mut _)).ok()?;
        let walker = automation.RawViewWalker().ok()?;

        let mut containers: Vec<(bool, IUIAutomationElement)> = Vec::new();
        let mut budget = Budget::new(4000);
        raw_walk(&walker, &root, 0, &mut budget, &mut |el, _| {
            let t = el.CurrentControlType().map(|t| t.0).unwrap_or(0);
            if t == 50032 || t == 50033 {
                let n = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                if n == container_name {
                    let class = el.CurrentClassName().map(|b| b.to_string()).unwrap_or_default();
                    containers.push((class.contains("QuickWindow"), el.clone()));
                }
            }
            true
        });
        containers.sort_by_key(|(is_quick, _)| !*is_quick);

        for (_, container) in &containers {
            let mut found: Option<(i32, i32)> = None;
            let mut budget = Budget::new(4000);
            raw_walk(&walker, container, 0, &mut budget, &mut |el, _| {
                let t = el.CurrentControlType().map(|t| t.0).unwrap_or(0);
                if t != control_type {
                    return true;
                }
                let n = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
                if n != name {
                    return true;
                }
                // Toggle first (UIA_TogglePatternId = 10015); -1 when the pattern is absent,
                // which is a different answer from "off" and has to stay distinguishable.
                let toggle = el
                    .GetCurrentPattern(UIA_PATTERN_ID(10015))
                    .ok()
                    .and_then(|unk| unk.cast::<IUIAutomationTogglePattern>().ok())
                    .and_then(|tp| tp.CurrentToggleState().ok())
                    .map(|v| v.0)
                    .unwrap_or(-1);
                // Then LegacyIAccessible (10018) state bits, where a control that behaves like a
                // checkbox without declaring itself one usually keeps it: 0x10 CHECKED,
                // 0x08 PRESSED, 0x04 FOCUSED, 0x100000 FOCUSABLE.
                let legacy = el
                    .GetCurrentPattern(UIA_PATTERN_ID(10018))
                    .ok()
                    .and_then(|unk| unk.cast::<IUIAutomationLegacyIAccessiblePattern>().ok())
                    .and_then(|lp| lp.CurrentState().ok())
                    .unwrap_or(0) as i32;
                found = Some((toggle, legacy));
                false
            });
            if found.is_some() {
                return found;
            }
        }
        None
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
pub fn element_focus_step(hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
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
        // VARIANT and matches element_dump's proven path.
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

