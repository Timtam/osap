//! Minimal UI Automation: does a window's UIA subtree contain an element with a
//! given Name + ControlType? Used to confirm a plugin's identity where a window-
//! class match alone is ambiguous — e.g. sforzando exposes a Pane (ControlType
//! 50033) named "PlogueXMLGUI", which ReaHotkey also keys on.
//!
//! **Asked on one thread.** Everything here runs on the host's accessibility thread
//! (`element.rs`), which enters the multithreaded COM apartment once ([`thread_init`]) and asks
//! one question at a time through [`run`]; nothing here runs on the event loop any more. The
//! automation object and its timeouts are that thread's ([`automation`]).
//!
//! **Not known is not "not there".** A call the program did not answer — it did not answer within
//! [`UIA_TIMEOUT_MS`], it runs with higher rights, it went away — is noted for the question in
//! progress ([`noted`]), and a question that found nothing after such a call is answered "not
//! known" with the reason ([`settle`]), never "no". A walk stops at the first such call: a program
//! that did not answer one question is not asked the next one of the same walk. So does a question
//! that could not be put at all — the automation object not made, a condition it could not build
//! ([`reached`], [`local`]): it is "not known" too, `element::NO_INTERFACE`, as a press is told.

use std::cell::{Cell, RefCell};

use super::{DumpNode, ElementAnswer, ElementAsk, ElementFailure, ElementOutcome};
use crate::errors::{common, element, HostError};
use windows::core::{Interface, BSTR, VARIANT};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Accessibility::{
    IUIAutomation2,
    CUIAutomation8,
    CUIAutomation, IUIAutomation, IUIAutomationCondition, IUIAutomationElement,
    IUIAutomationInvokePattern, IUIAutomationLegacyIAccessiblePattern, IUIAutomationTogglePattern,
    IUIAutomationTreeWalker,
    TreeScope_Descendants, TreeScope_Subtree, UIA_BoundingRectanglePropertyId,
    UIA_ClassNamePropertyId, UIA_ControlTypePropertyId, UIA_NamePropertyId, UIA_PATTERN_ID,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};

thread_local! {
    // UIA interfaces are thread-affine; cache per thread, created on first use.
    static AUTOMATION: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
    // Whether this thread's automation object leaves the keyboard focus where it is when it acts
    // (`automation`): an action is made only when it does.
    static ACTS_IN_PLACE: Cell<bool> = const { Cell::new(false) };
    // The first failure of the question in progress — a call the program did not answer, or one of
    // the automation object's own (`reached`, `local`) — what a module is told, and whether it was
    // the timeout. Cleared by `run` before each question and taken after it (`settle`).
    static FAILED: RefCell<Option<(HostError, bool)>> = const { RefCell::new(None) };
    // The first bound a walk of the question in progress stopped at before the end of the tree —
    // its node budget or its time — as a module is told it. Cleared by `run` before each question
    // and taken after it (`settle`): a search it cut short that found nothing is not known, never
    // "not there".
    static CUT: RefCell<Option<HostError>> = const { RefCell::new(None) };
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
///
/// The questions are asked on a thread of their own now, so a call that runs into this bound no
/// longer holds the keyboard; it still bounds how long that thread, and an event loop that waits
/// for its answer, can be kept by one program.
pub(crate) const UIA_TIMEOUT_MS: u32 = 1000;

/// `UIA_E_TIMEOUT`: the program did not answer within the bound `automation` sets.
const TIMEOUT: u32 = 0x8013_1505;

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
    /// The nodes it began with, for the words of a cut ([`cut_by_nodes`]).
    of: i32,
    until: std::time::Instant,
    /// True once either bound stopped it, so the caller can say the dump is partial instead
    /// of letting a truncated tree read as a complete one.
    stopped: bool,
}

impl Budget {
    fn new(nodes: i32) -> Self {
        Self { nodes, of: nodes, until: std::time::Instant::now() + WALK_DEADLINE, stopped: false }
    }

    /// Spends one node — a walk asks for it as it reaches the node, so a walk whose tree has no
    /// more nodes than the budget never runs out. False when there is nothing left to spend: the
    /// walk stopped before the end of the tree, which is noted for the question in progress
    /// ([`CUT`]).
    ///
    /// The clock is read every 64 nodes rather than every one: `Instant::now` is a syscall on
    /// some platforms, and a walk that measured itself more often than it worked would be
    /// its own problem. 64 nodes is well under a second even at the timeout above.
    fn spend(&mut self) -> bool {
        if self.nodes <= 0 {
            self.cut(cut_by_nodes(self.of));
            return false;
        }
        self.nodes -= 1;
        if self.nodes % 64 == 0 && std::time::Instant::now() >= self.until {
            self.cut(cut_by_time());
            return false;
        }
        true
    }

    fn cut(&mut self, why: HostError) {
        self.stopped = true;
        note_cut(why);
    }
}

/// What a module is told of a search that stopped at its node budget of `of` before the end of the
/// tree.
fn cut_by_nodes(of: i32) -> HostError {
    HostError::with(&element::TOO_MANY_NODES, format!("the tree holds more than the {of} elements a search reads"))
}

/// What a module is told of a search that stopped at [`WALK_DEADLINE`] before the end of the tree.
fn cut_by_time() -> HostError {
    HostError::with(&element::TOO_SLOW, format!("the search took longer than {} s", WALK_DEADLINE.as_secs()))
}

/// A walk of the question in progress stopped at a bound before the end of the tree: noted, the
/// first one only ([`CUT`]).
fn note_cut(why: HostError) {
    CUT.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(why);
        }
    });
}

/// The accessibility thread's start (`element.rs`): it enters the multithreaded COM apartment, once,
/// for its whole life — where Microsoft wants a UI Automation client, and an apartment whose calls
/// wait on no message loop of the thread's own. The automation object is made at the first
/// question (`automation`). A thread that could not enter it says so once; its questions then fail
/// as UI Automation answers them there.
pub fn thread_init() {
    // SAFETY: no pointers; this thread entering its apartment, once.
    let entered = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if entered.is_err() {
        crate::logging::line(
            "uia",
            &format!("the accessibility thread could not enter the multithreaded COM apartment (0x{:08x})", entered.0 as u32),
        );
    }
}

/// The thread's automation object, created on first use with its timeouts set.
///
/// One place rather than nine. Every function here used to open its own with the same six
/// lines, which is how the timeouts came to be missing everywhere at once: there was no
/// single place to put them.
///
/// The thread's COM apartment is entered once, by [`thread_init`], and not here: entered at every
/// call, it was never left, and the apartment's count of entries only grew (the system audit's
/// P3 in TODO.md).
fn automation(cell: &RefCell<Option<IUIAutomation>>) -> std::cell::RefMut<'_, Option<IUIAutomation>> {
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
                    // An action (`element_press`) leaves the keyboard focus where it is. UIA's
                    // default moves it to the element first, which takes the keyboard from the
                    // window in front — a page beside the plug-in — and is heard. Set where the
                    // object is made, before anything acts; nothing before the press acted.
                    let in_place = a2.SetAutoSetFocus(false).is_ok();
                    ACTS_IN_PLACE.with(|c| c.set(in_place));
                    crate::logging::line(
                        "uia",
                        &format!(
                            "accessibility call timeout set to {UIA_TIMEOUT_MS} ms on the accessibility \
                             thread (the default is about two minutes); {}",
                            if in_place {
                                "an action leaves the keyboard focus where it is"
                            } else {
                                "an action could not be kept from moving the keyboard focus, so none is made"
                            }
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

/// Whether a call's failure `code` means the program did not answer the question at all — it did
/// not answer in time, it runs with higher rights, it went away — rather than that it answered
/// "nothing": a search that found nothing (which windows-rs hands back as an error without a code
/// of the program's), an element that went away while it was walked, a provider that cannot do
/// what was asked. Only the first kind makes a question "not known".
fn unanswered(code: u32) -> bool {
    matches!(code, TIMEOUT | 0x8007_0005 | 0x8001_0108 | 0x8007_06BA | 0x8007_06BE)
}

/// `r`'s value, or `None` — and when the call failed because the program did not answer
/// ([`unanswered`]), that is noted for the question in progress, the first such failure only.
fn noted<T>(r: windows::core::Result<T>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            let code = e.code().0 as u32;
            if e.code().is_err() && unanswered(code) {
                note_failed(failure(code as i32), code == TIMEOUT);
            }
            None
        }
    }
}

/// The thread's automation object as [`automation`] made it, or `None` — noted for the question in
/// progress, since without it nothing can be asked, which is not "nothing there"
/// ([`element::NO_INTERFACE`]).
fn reached(made: &Option<IUIAutomation>) -> Option<&IUIAutomation> {
    if made.is_none() {
        note_failed(HostError::of(&element::NO_INTERFACE), false);
    }
    made.as_ref()
}

/// `r`'s value, or `None` — a call of the automation object's own, which asks no program: a
/// condition, a walker. One that failed is noted for the question in progress as
/// [`element::NO_INTERFACE`]: the question could not be put as asked.
fn local<T>(r: windows::core::Result<T>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(_) => {
            note_failed(HostError::of(&element::NO_INTERFACE), false);
            None
        }
    }
}

/// A failure of the question in progress, and whether it was the timeout: the first one only.
fn note_failed(why: HostError, timeout: bool) {
    FAILED.with(|f| {
        let mut f = f.borrow_mut();
        if f.is_none() {
            *f = Some((why, timeout));
        }
    });
}

/// Whether a call of the question in progress went unanswered: a walk stops there, and a list of
/// names is not searched on.
fn stopped() -> bool {
    FAILED.with(|f| f.borrow().is_some())
}

/// What a question's answer becomes once it is known whether a call of it went unanswered
/// (`failed`), or a walk of it stopped at its bound before the end of the tree (`cut`): an answer
/// that found something stands — what was found is there; one that found nothing is "not known",
/// with the reason, since the part not answered or not walked may have held it; a dump keeps what
/// it found and says why it is not all. A program that did not answer is said before a bound: it
/// is what stopped the walk. Only that makes the questions behind this one go unasked
/// (`unanswered`, element.rs); a tree too large or too slow to walk answered every call.
fn settle(answer: ElementAnswer, failed: Option<(HostError, bool)>, cut: Option<HostError>) -> ElementOutcome {
    answer.settled(failed.or(cut.map(|why| (why, false))))
}

/// One question, on the accessibility thread: the [`super::ElementRunner`]'s `run`. A window that is
/// gone holds nothing, and every read answers that; a press says so (`element_press`).
pub fn run(ask: &ElementAsk) -> ElementOutcome {
    if let ElementAsk::Press { hwnd, name, ctype } = ask {
        return element_press(*hwnd, name, *ctype);
    }
    let hwnd = ask.hwnd();
    // SAFETY: a local question about a handle, which a stale one answers no to.
    if hwnd == 0 || unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(hwnd as _) } == 0 {
        return ask.nothing();
    }
    FAILED.with(|f| *f.borrow_mut() = None);
    CUT.with(|c| *c.borrow_mut() = None);
    let answer = match ask {
        ElementAsk::Find { hwnd, name, ctype } => ElementAnswer::Found(element_find(*hwnd, name, *ctype)),
        ElementAsk::FindAny { hwnd, names, types } => ElementAnswer::Index(element_find_any(*hwnd, names, types)),
        ElementAsk::Locate { hwnd, name, ctype } => ElementAnswer::Point(element_locate(*hwnd, name, *ctype)),
        ElementAsk::LocateVia { hwnd, via_name, via_type, name, ctype } => {
            ElementAnswer::Point(element_locate_via(*hwnd, via_name, *via_type, name, *ctype))
        }
        ElementAsk::PluginLocate { hwnd, container, name, ctype } => {
            ElementAnswer::Point(element_plugin_locate(*hwnd, container, name, *ctype))
        }
        ElementAsk::StateProbe { hwnd, container, name, ctype } => {
            ElementAnswer::State(element_state_probe(*hwnd, container, name, *ctype))
        }
        ElementAsk::Dump { hwnd } => ElementAnswer::Nodes(element_dump(*hwnd), None),
        ElementAsk::RawDump { hwnd } => ElementAnswer::Nodes(element_raw_dump(*hwnd), None),
        ElementAsk::ClassNavPoint { hwnd, class, ctype, child, sibling } => {
            ElementAnswer::Point(element_class_nav_point(*hwnd, class, *ctype, *child, *sibling))
        }
        ElementAsk::FocusStep { hwnd, direction } => ElementAnswer::Step(element_focus_step(*hwnd, *direction)),
        ElementAsk::FocusPut { hwnd } => ElementAnswer::Put(element_focus_put(*hwnd)),
        // Not here: OSARA's F6 is the way back into a plug-in on Windows (element.md).
        ElementAsk::FocusWithin { .. } => ElementAnswer::Within(None),
        ElementAsk::Press { .. } => unreachable!("a press is answered above"),
    };
    settle(answer, FAILED.with(|f| f.borrow_mut().take()), CUT.with(|c| c.borrow_mut().take()))
}

/// Finds the first element in `hwnd`'s UIA subtree whose Name == `name` and
/// ControlType == `control_type`. Never panics; any failure / no-match → None.
fn find_element(hwnd: isize, name: &str, control_type: i32) -> Option<IUIAutomationElement> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = reached(&borrow)?;

        let element = noted(automation.ElementFromHandle(HWND(hwnd as *mut _)))?;

        // ControlType == `control_type` (VT_I4), AND Name == `name` (VT_BSTR) when
        // a name is given; an empty name means "any element of this type" (e.g.
        // detecting an open Menu). VARIANTs free their data on drop.
        let v_ctype: VARIANT = control_type.into();
        let cond_ctype = local(automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &v_ctype))?;
        let cond = if name.is_empty() {
            cond_ctype
        } else {
            let v_name: VARIANT = BSTR::from(name).into();
            let cond_name = local(automation.CreatePropertyCondition(UIA_NamePropertyId, &v_name))?;
            local(automation.CreateAndCondition(&cond_name, &cond_ctype))?
        };

        // FindFirst is on the element; TreeScope_Subtree includes the element
        // itself. A no-match comes back as Err in windows-rs (null → Err).
        noted(element.FindFirst(TreeScope_Subtree, &cond))
    })
}

/// True if `hwnd`'s UIA subtree contains an element with that Name + ControlType.
fn element_find(hwnd: isize, name: &str, control_type: i32) -> bool {
    find_element(hwnd, name, control_type).is_some()
}

/// "Is any of these names present as any of these control types?" — the shape almost
/// every plugin-identity check takes ("Kontakt 8" as Window OR Pane). Answers it with a
/// SINGLE tree traversal built from one OR-condition, instead of one full subtree walk
/// per name×type pair; the walk dominates the cost, and identity is re-checked on every
/// detection pass. Returns the 1-based index of the matching NAME (so the caller learns
/// WHICH one matched, e.g. the plugin's version), or None.
///
/// An empty name matches any element of the given types. A name the program did not answer
/// for ends the search: the names after it are not asked.
fn element_find_any(hwnd: isize, names: &[String], types: &[i32]) -> Option<usize> {
    if names.is_empty() || types.is_empty() {
        return None;
    }
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = reached(&borrow)?;
        let element = noted(automation.ElementFromHandle(HWND(hwnd as *mut _)))?;

        // OR over the control types, ANDed with the name when one is given. One
        // traversal per NAME (a name is what we must tell apart in the result); the
        // types — the part that multiplied the walks — collapse into the condition.
        // Folded pairwise rather than via CreateOrConditionFromArray, which wants a
        // SAFEARRAY; the type list is a handful of entries, so the shape is irrelevant.
        let mut cond_type: Option<IUIAutomationCondition> = None;
        for t in types {
            let v: VARIANT = (*t).into();
            let c = local(automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &v))?;
            cond_type = Some(match cond_type {
                None => c,
                Some(prev) => local(automation.CreateOrCondition(&prev, &c))?,
            });
        }
        let cond_type = cond_type?;

        for (i, name) in names.iter().enumerate() {
            let cond = if name.is_empty() {
                cond_type.clone()
            } else {
                let v: VARIANT = BSTR::from(name.as_str()).into();
                let cond_name = local(automation.CreatePropertyCondition(UIA_NamePropertyId, &v))?;
                // A name that cannot be asked ends the search, as one the program did not answer
                // for does: the names after it are not asked, and none found is not known.
                local(automation.CreateAndCondition(&cond_name, &cond_type))?
            };
            if noted(element.FindFirst(TreeScope_Subtree, &cond)).is_some() {
                return Some(i + 1);
            }
            if stopped() {
                return None;
            }
        }
        None
    })
}

/// The screen-pixel centre of that element's bounding rectangle (to click it), or
/// None if not found / it has no on-screen rect.
fn element_locate(hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)> {
    let element = find_element(hwnd, name, control_type)?;
    unsafe {
        let r = noted(element.CurrentBoundingRectangle())?;
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
fn element_locate_via(
    hwnd: isize,
    via_name: &str,
    via_type: i32,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = reached(&borrow)?;
        let root = noted(automation.ElementFromHandle(HWND(hwnd as *mut _)))?;

        // Build a Name(+Type) condition; an empty name means "any of this type".
        let make_cond = |nm: &str, ct: i32| -> Option<IUIAutomationCondition> {
            let v_ct: VARIANT = ct.into();
            let cond_ct = local(automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &v_ct))?;
            if nm.is_empty() {
                Some(cond_ct)
            } else {
                let v_nm: VARIANT = BSTR::from(nm).into();
                let cond_nm = local(automation.CreatePropertyCondition(UIA_NamePropertyId, &v_nm))?;
                local(automation.CreateAndCondition(&cond_nm, &cond_ct))
            }
        };

        // Find the container (the plugin's identity pane), then search WITHIN it —
        // this crosses the hosted-fragment boundary that a search from `root` does not.
        let cond_via = make_cond(via_name, via_type)?;
        let container = noted(root.FindFirst(TreeScope_Subtree, &cond_via))?;
        let cond = make_cond(name, control_type)?;
        let el = noted(container.FindFirst(TreeScope_Subtree, &cond))?;
        let r = noted(el.CurrentBoundingRectangle())?;
        if r.right <= r.left || r.bottom <= r.top {
            return None;
        }
        Some(((r.left + r.right) / 2, (r.top + r.bottom) / 2))
    })
}

/// What `host.element.press` is told for an accessibility call that failed with `code` — and every
/// other call here that the program did not answer ([`noted`]).
pub(super) fn failure(code: i32) -> HostError {
    match code as u32 {
        0x8007_0005 => HostError::of(&element::DENIED),
        // UIA_E_TIMEOUT: the program did not answer within the bound `automation` sets.
        TIMEOUT => HostError::with(&element::TIMEOUT, format!("the program did not answer within {UIA_TIMEOUT_MS} ms")),
        // UIA_E_ELEMENTNOTAVAILABLE.
        0x8004_0201 => HostError::of(&element::ELEMENT_GONE),
        // UIA_E_ELEMENTNOTENABLED.
        0x8004_0200 => HostError::of(&element::NOT_ENABLED),
        // UIA_E_INVALIDOPERATION, UIA_E_NOTSUPPORTED.
        0x8013_1509 | 0x8004_0204 => HostError::of(&element::CANNOT_PRESS),
        // RPC_E_DISCONNECTED, RPC_S_SERVER_UNAVAILABLE, RPC_S_CALL_FAILED: its process went.
        0x8001_0108 | 0x8007_06BA | 0x8007_06BE => HostError::of(&element::PROGRAM_GONE),
        other => HostError::with(&element::NOT_TAKEN, format!("the program did not take the action (0x{other:08x})")),
    }
}

/// `host.element.press`: the first element of `hwnd`'s tree with that Name and ControlType — as
/// `element_find` finds it, an empty name being any of the type — asked to perform its action:
/// Invoke, else Toggle, else LegacyIAccessible's default action, the first the element offers.
/// The keyboard focus stays where it is (`automation`), so the window in front keeps it.
///
/// Every step is a call into the other program, each bounded by `UIA_TIMEOUT_MS`; a provider that
/// acts before it answers (a Win32 button whose action opens a dialog, JUCE's handler) can take
/// the whole bound. On the accessibility thread, as every question here; the event loop waits for
/// it only when the module handed over no callback (`element.rs`). `Ok` says the program accepted
/// the action, not that it did what the module wanted. One log line either way.
fn element_press(hwnd: isize, name: &str, control_type: i32) -> ElementOutcome {
    // SAFETY: a local question about a handle, which a stale one answers no to.
    if hwnd == 0 || unsafe { windows_sys::Win32::UI::WindowsAndMessaging::IsWindow(hwnd as _) } == 0 {
        return Err(ElementFailure { why: HostError::of(&common::WINDOW_GONE), unanswered: false });
    }
    let pressed = AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let Some(automation) = borrow.as_ref() else {
            return Err((HostError::of(&element::NO_INTERFACE), false));
        };
        if !ACTS_IN_PLACE.with(|c| c.get()) {
            return Err((HostError::of(&element::MOVES_FOCUS), false));
        }
        let fail = |e: windows::core::Error| (failure(e.code().0), e.code().0 as u32 == TIMEOUT);
        let root = automation.ElementFromHandle(HWND(hwnd as *mut _)).map_err(fail)?;
        let v_ctype: VARIANT = control_type.into();
        let mut cond = automation.CreatePropertyCondition(UIA_ControlTypePropertyId, &v_ctype).map_err(fail)?;
        if !name.is_empty() {
            let v_name: VARIANT = BSTR::from(name).into();
            let cond_name = automation.CreatePropertyCondition(UIA_NamePropertyId, &v_name).map_err(fail)?;
            cond = automation.CreateAndCondition(&cond_name, &cond).map_err(fail)?;
        }
        // A search that finds nothing answers no element, which windows-rs turns into an error
        // without a failure code of the program's: that one is "not found".
        let el = match root.FindFirst(TreeScope_Subtree, &cond) {
            Ok(el) => el,
            Err(e) if e.code().is_err() && e.code().0 as u32 != 0x8000_4003 => return Err(fail(e)),
            Err(_) => return Err((HostError::of(&element::NO_ELEMENT), false)),
        };
        // Invoke (10000), else Toggle (10015), else LegacyIAccessible's default action (10018) — the
        // last only when the element names one.
        if let Some(p) = el.GetCurrentPattern(UIA_PATTERN_ID(10000)).ok().and_then(|u| u.cast::<IUIAutomationInvokePattern>().ok()) {
            return p.Invoke().map(|()| "Invoke").map_err(fail);
        }
        if let Some(p) = el.GetCurrentPattern(UIA_PATTERN_ID(10015)).ok().and_then(|u| u.cast::<IUIAutomationTogglePattern>().ok()) {
            return p.Toggle().map(|()| "Toggle").map_err(fail);
        }
        if let Some(p) = el
            .GetCurrentPattern(UIA_PATTERN_ID(10018))
            .ok()
            .and_then(|u| u.cast::<IUIAutomationLegacyIAccessiblePattern>().ok())
        {
            let action = p.CurrentDefaultAction().map(|b| b.to_string()).unwrap_or_default();
            if !action.trim().is_empty() {
                return p.DoDefaultAction().map(|()| "its default action").map_err(fail);
            }
        }
        Err((HostError::of(&element::CANNOT_PRESS), false))
    });
    let what = if name.is_empty() { format!("the first element of type {control_type}") } else { format!("'{name}' (type {control_type})") };
    match &pressed {
        Ok(how) => crate::logging::line("uia", &format!("pressed {what} in window {hwnd} by {how}, the keyboard focus left where it was")),
        Err((why, _)) => crate::logging::line("uia", &format!("could not press {what} in window {hwnd}: {why}")),
    }
    pressed.map(|_| ElementAnswer::Pressed).map_err(|(why, unanswered)| ElementFailure { why, unanswered })
}

/// An element's on-screen rectangle, or all zeros when it has none.
///
/// Part of a dump because a dump exists to let somebody author coordinates for a machine
/// they cannot look at, and "there is a slider" without "and it is here" is half an answer.
fn rect_of(el: &IUIAutomationElement) -> (i32, i32, i32, i32) {
    match noted(unsafe { el.CurrentBoundingRectangle() }) {
        Some(r) => (r.left, r.top, r.right - r.left, r.bottom - r.top),
        None => (0, 0, 0, 0),
    }
}

/// The most elements `element_dump` lists.
const DUMP_CAP: i32 = 2000;

/// Dev/diagnostic: walk `hwnd`'s UIA subtree in the RAW view and return the
/// "interesting" elements — those with a non-empty Name, plus Qt window panes —
/// as (depth, Name, ClassName, ControlType). Bounded (≤600 nodes, ≤16 deep). Used
/// to discover the Name/ClassName/ControlType to key a plugin's identity on, e.g.
/// a Kontakt rendered inside Komplete Kontrol's own Qt window.
fn element_dump(hwnd: isize) -> Vec<DumpNode> {
    let mut out: Vec<DumpNode> = Vec::new();
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let Some(automation) = reached(&borrow) else { return };
        let root = match noted(automation.ElementFromHandle(HWND(hwnd as *mut _))) {
            Some(e) => e,
            None => return,
        };
        // FindAll(Subtree, TrueCondition) — a flat list of all descendants, and
        // unlike the raw TreeWalker it crosses UIA fragment boundaries (e.g. into a
        // Kontakt rendered inside Komplete Kontrol). No depth; capped to stay bounded.
        let Some(cond) = local(automation.CreateTrueCondition()) else { return };
        let arr = match noted(root.FindAll(TreeScope_Subtree, &cond)) {
            Some(a) => a,
            None => return,
        };
        let all = arr.Length().unwrap_or(0);
        if all > DUMP_CAP {
            // The rest of the tree is not listed: the dump says so (`settle`).
            note_cut(HostError::with(&element::DUMP_CUT, format!("the tree holds {all} elements, more than the {DUMP_CAP} a dump lists")));
        }
        for i in 0..all.min(DUMP_CAP) {
            if stopped() {
                return;
            }
            if let Ok(el) = arr.GetElement(i) {
                let name = noted(el.CurrentName()).map(|b| b.to_string()).unwrap_or_default();
                let class = noted(el.CurrentClassName()).map(|b| b.to_string()).unwrap_or_default();
                let ctype = noted(el.CurrentControlType()).map(|t| t.0).unwrap_or(0);
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
fn element_class_nav_point(
    hwnd: isize,
    class_substr: &str,
    ctype: i32,
    child: i32,
    sibling: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = reached(&borrow)?;
        let root = noted(automation.ElementFromHandle(HWND(hwnd as *mut _)))?;
        let walker = local(automation.RawViewWalker())?;
        let mut el = find_by_class(&walker, &root, class_substr, ctype, 0)?;
        // WalkTree `n` (nth child): first child, then n-1 next siblings.
        if child >= 1 {
            el = noted(walker.GetFirstChildElement(&el))?;
            let mut k = child - 1;
            while k > 0 {
                el = noted(walker.GetNextSiblingElement(&el))?;
                k -= 1;
            }
        }
        // WalkTree `-n` / `+n` (nth previous / next sibling).
        let mut n = sibling;
        while n < 0 {
            el = noted(walker.GetPreviousSiblingElement(&el))?;
            n += 1;
        }
        while n > 0 {
            el = noted(walker.GetNextSiblingElement(&el))?;
            n -= 1;
        }
        let r = noted(el.CurrentBoundingRectangle())?;
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
/// It also stops at a call the program did not answer ([`stopped`]). A walk its budget stops says
/// so for the question in progress (`Budget::spend`) — only when there is a node it did not reach:
/// the next sibling is asked for before the budget is.
unsafe fn raw_walk(
    walker: &IUIAutomationTreeWalker,
    el: &IUIAutomationElement,
    depth: i32,
    budget: &mut Budget,
    visit: &mut impl FnMut(&IUIAutomationElement, i32) -> bool,
) -> bool {
    if depth > 40 || stopped() || !budget.spend() {
        return true;
    }
    if !visit(el, depth) {
        return false;
    }
    let mut child = match noted(walker.GetFirstChildElement(el)) {
        Some(c) => c,
        None => return true,
    };
    loop {
        if !raw_walk(walker, &child, depth + 1, budget, visit) {
            return false;
        }
        if budget.stopped || stopped() {
            return true;
        }
        child = match noted(walker.GetNextSiblingElement(&child)) {
            Some(n) => n,
            None => return true,
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

fn element_raw_dump(hwnd: isize) -> Vec<DumpNode> {
    let mut out: Vec<DumpNode> = Vec::new();
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let Some(automation) = reached(&borrow) else { return };
        let root = match noted(automation.ElementFromHandle(HWND(hwnd as *mut _))) {
            Some(e) => e,
            None => return,
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
            noted(root.BuildUpdatedCache(&req))
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
        // A program that did not answer the one request is not walked a question at a time.
        if stopped() {
            return;
        }
        // The walk below is the answer now: what bound the cached one stopped at is not its.
        CUT.with(|c| *c.borrow_mut() = None);

        // The way it was done before, kept as the answer for whatever the cache cannot do.
        // Slow, and correct on every provider this project has ever met.
        let Some(walker) = local(automation.RawViewWalker()) else { return };
        let mut budget = Budget::new(4000);
        raw_walk(&walker, &root, 0, &mut budget, &mut |el, depth| {
            let name = noted(el.CurrentName()).map(|b| b.to_string()).unwrap_or_default();
            let class = noted(el.CurrentClassName()).map(|b| b.to_string()).unwrap_or_default();
            let ctype = noted(el.CurrentControlType()).map(|t| t.0).unwrap_or(0);
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

/// Every element of `root`'s raw tree that IS the plug-in — ReaHotkey's CheckElement: named
/// `container_name`, a Window or a Pane — in raw-tree order, the `ni::qt::QuickWindow` ones first
/// (ReaHotkey's Criteria order: the Qt scene root, which has the content, before the QWindowIcon
/// host).
unsafe fn containers_of(
    walker: &IUIAutomationTreeWalker,
    root: &IUIAutomationElement,
    container_name: &str,
) -> Vec<IUIAutomationElement> {
    let mut containers: Vec<(bool, IUIAutomationElement)> = Vec::new();
    let mut budget = Budget::new(4000);
    raw_walk(walker, root, 0, &mut budget, &mut |el, _| {
        let t = noted(el.CurrentControlType()).map(|t| t.0).unwrap_or(0);
        if t == 50032 || t == 50033 {
            let n = noted(el.CurrentName()).map(|b| b.to_string()).unwrap_or_default();
            if n == container_name {
                let class = noted(el.CurrentClassName()).map(|b| b.to_string()).unwrap_or_default();
                containers.push((class.contains("QuickWindow"), el.clone()));
            }
        }
        true
    });
    containers.sort_by_key(|(is_quick, _)| !*is_quick);
    containers.into_iter().map(|(_, el)| el).collect()
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
fn element_plugin_locate(
    hwnd: isize,
    container_name: &str,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = reached(&borrow)?;
        let root = noted(automation.ElementFromHandle(HWND(hwnd as *mut _)))?;
        let walker = local(automation.RawViewWalker())?;

        for container in &containers_of(&walker, &root, container_name) {
            let mut hit: Option<(i32, i32)> = None;
            let mut budget = Budget::new(4000);
            raw_walk(&walker, container, 0, &mut budget, &mut |el, _| {
                let t = noted(el.CurrentControlType()).map(|t| t.0).unwrap_or(0);
                if t != control_type {
                    return true;
                }
                if !named(name, || noted(el.CurrentName()).map(|b| b.to_string()).unwrap_or_default()) {
                    return true;
                }
                if let Some(r) = noted(el.CurrentBoundingRectangle()) {
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
            if stopped() {
                return None;
            }
        }
        None
    })
}

/// Whether an element's Name is the one asked for, read only when it has to be: an empty `wanted`
/// takes any element of the type, as everywhere else in host.element. `pluginLocate`'s walk and
/// `stateProbe`'s both ask it; `stateProbe`'s compared the Name exactly until 2026-10-24, so an
/// empty name took only an element without one.
fn named(wanted: &str, name: impl FnOnce() -> String) -> bool {
    wanted.is_empty() || name() == wanted
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
fn element_state_probe(
    hwnd: isize,
    container_name: &str,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = reached(&borrow)?;
        let root = noted(automation.ElementFromHandle(HWND(hwnd as *mut _)))?;
        let walker = local(automation.RawViewWalker())?;

        for container in &containers_of(&walker, &root, container_name) {
            let mut found: Option<(i32, i32)> = None;
            let mut budget = Budget::new(4000);
            raw_walk(&walker, container, 0, &mut budget, &mut |el, _| {
                let t = noted(el.CurrentControlType()).map(|t| t.0).unwrap_or(0);
                if t != control_type {
                    return true;
                }
                if !named(name, || noted(el.CurrentName()).map(|b| b.to_string()).unwrap_or_default()) {
                    return true;
                }
                // Toggle first (UIA_TogglePatternId = 10015); -1 when the pattern is absent,
                // which is a different answer from "off" and has to stay distinguishable.
                let toggle = el
                    .GetCurrentPattern(UIA_PATTERN_ID(10015))
                    .ok()
                    .and_then(|unk| unk.cast::<IUIAutomationTogglePattern>().ok())
                    .and_then(|tp| noted(tp.CurrentToggleState()))
                    .map(|v| v.0)
                    .unwrap_or(-1);
                // Then LegacyIAccessible (10018) state bits, where a control that behaves like a
                // checkbox without declaring itself one usually keeps it: 0x10 CHECKED,
                // 0x08 PRESSED, 0x04 FOCUSED, 0x100000 FOCUSABLE.
                let legacy = el
                    .GetCurrentPattern(UIA_PATTERN_ID(10018))
                    .ok()
                    .and_then(|unk| unk.cast::<IUIAutomationLegacyIAccessiblePattern>().ok())
                    .and_then(|lp| noted(lp.CurrentState()))
                    .unwrap_or(0) as i32;
                found = Some((toggle, legacy));
                false
            });
            if found.is_some() {
                return found;
            }
            if stopped() {
                return None;
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
    if depth > 25 || stopped() {
        return None;
    }
    let class = noted(el.CurrentClassName()).map(|b| b.to_string()).unwrap_or_default();
    let t = noted(el.CurrentControlType()).map(|t| t.0).unwrap_or(0);
    if t == ctype && class.contains(class_substr) {
        return Some(el.clone());
    }
    let mut child = noted(walker.GetFirstChildElement(el))?;
    loop {
        if let Some(found) = find_by_class(walker, &child, class_substr, ctype, depth + 1) {
            return Some(found);
        }
        if stopped() {
            return None;
        }
        child = noted(walker.GetNextSiblingElement(&child))?;
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
/// the newly focused element's Name, ControlType, 1-based index, the count and its
/// bounding rectangle, or None if the scope has no focusable descendant that accepts focus.
fn element_focus_step(hwnd: isize, direction: i32) -> Option<super::FocusStep> {
    AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = reached(&borrow)?;
        let root = noted(automation.ElementFromHandle(HWND(hwnd as *mut _)))?;

        // Scope to the window's content area — its first child in the control view, i.e.
        // ReaHotkey's ElementFromPath(1). This structurally drops the window frame and the
        // menu bar (siblings of the content, not descendants of it) from the ring. Fall
        // back to the window itself if it has no child.
        let scope = automation
            .ControlViewWalker()
            .ok()
            .and_then(|w| noted(w.GetFirstChildElement(&root)))
            .unwrap_or_else(|| root.clone());
        if stopped() {
            return None;
        }

        // The scope's VISIBLE, keyboard-focusable descendants in control-view order — the
        // same set NVDA would Tab through. TrueCondition + Rust-side filter avoids a bool
        // VARIANT and matches element_dump's proven path.
        let cond = local(automation.CreateTrueCondition())?;
        let arr = noted(scope.FindAll(TreeScope_Descendants, &cond))?;
        let len = arr.Length().unwrap_or(0);
        let mut items: Vec<IUIAutomationElement> = Vec::new();
        for i in 0..len {
            if stopped() {
                return None;
            }
            if let Ok(el) = arr.GetElement(i) {
                if !noted(el.CurrentIsKeyboardFocusable()).map(|b| b.as_bool()).unwrap_or(false) {
                    continue;
                }
                if noted(el.CurrentIsOffscreen()).map(|b| b.as_bool()).unwrap_or(false) {
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
        let focused = noted(automation.GetFocusedElement());
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
        // each element at most once before giving up — or before the program stops answering.
        let mut idx = cur;
        for _ in 0..count {
            if stopped() {
                return None;
            }
            idx = if idx < 0 {
                if direction >= 0 { 0 } else { count - 1 }
            } else if direction >= 0 {
                (idx + 1) % count
            } else {
                (idx - 1 + count) % count
            };
            let el = &items[idx as usize];
            if noted(el.SetFocus()).is_some() {
                let landed = noted(automation.GetFocusedElement())
                    .map(|f| {
                        automation.CompareElements(&f, el).map(|b| b.as_bool()).unwrap_or(false)
                    })
                    .unwrap_or(false);
                if landed {
                    let name = noted(el.CurrentName()).map(|b| b.to_string()).unwrap_or_default();
                    let ctype = noted(el.CurrentControlType()).map(|t| t.0).unwrap_or(0);
                    // Read once, AFTER the focus landed and was read back, so it has whatever the
                    // provider did to the element while it took the focus — a list scrolled to
                    // it inside SetFocus is where it is now, not where the enumeration found it.
                    // A scroll the plug-in animates afterwards is not waited for: nothing here
                    // waits on a clock, and the element is reported where it is at this read.
                    // One more cross-process property read per step. Physical pixels, the space
                    // every capture here uses: the application is per-monitor DPI aware (its
                    // manifest), and UIA hands such a client unscaled coordinates.
                    let bounds = noted(el.CurrentBoundingRectangle()).and_then(|r| {
                        // Subtracted as floats: a provider's nonsense edges cannot overflow here.
                        super::FocusBounds::new(
                            r.left as f64,
                            r.top as f64,
                            r.right as f64 - r.left as f64,
                            r.bottom as f64 - r.top as f64,
                        )
                    });
                    return Some(super::FocusStep { name, ctype, index: idx + 1, count, bounds });
                }
            }
        }
        None
    })
}

/// A landing's put (HW1, `focus_landing.rs`): UI Automation's SetFocus on window `hwnd` — a
/// plug-in's child control, as a rule — asked here, on the accessibility thread, never on the event
/// loop, and read back here: whether the keyboard is on the window, or a window inside it, afterwards
/// (`GetGUIThreadInfo` of its thread, local). For a window with no provider of its own — a JUCE
/// plug-in without accessibility — UI Automation's window proxy sets the focus itself, which may
/// attach input to the window's thread for the moment it takes; this code never does. A program that
/// does not answer within the UIA timeouts is "not known", with why. Unverified for such a window on
/// Windows: the read-back says whether it took (TODO.md, HW1).
fn element_focus_put(hwnd: isize) -> bool {
    let took = AUTOMATION.with(|cell| unsafe {
        let borrow = automation(cell);
        let automation = reached(&borrow)?;
        let el = noted(automation.ElementFromHandle(HWND(hwnd as *mut _)))?;
        noted(el.SetFocus())
    });
    if took.is_none() {
        return false;
    }
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GetWindowThreadProcessId, IsChild, GUITHREADINFO};
    // SAFETY: plain queries about a handle; a stale one answers no.
    unsafe {
        let tid = GetWindowThreadProcessId(hwnd as _, std::ptr::null_mut());
        let mut gti: GUITHREADINFO = std::mem::zeroed();
        gti.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if tid == 0 || GetGUIThreadInfo(tid, &mut gti) == 0 || gti.hwndFocus.is_null() {
            return false;
        }
        gti.hwndFocus as isize == hwnd || IsChild(hwnd as _, gti.hwndFocus) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An empty name takes any element of the type, its Name not even read; any other is compared
    /// exactly. `stateProbe`'s walk and `pluginLocate`'s both ask so: until 2026-10-24 `stateProbe`
    /// compared the Name exactly, and an empty one took only an element without a name.
    #[test]
    fn an_empty_name_takes_any_element_of_the_type_in_every_walk() {
        let read = std::cell::Cell::new(0);
        let r = &read;
        let name = move |n: &'static str| {
            move || {
                r.set(r.get() + 1);
                n.to_string()
            }
        };
        assert!(named("", name("Keyboard")));
        assert!(named("", name("")));
        assert_eq!(read.get(), 0, "an empty name reads no Name");
        assert!(named("Keyboard", name("Keyboard")));
        assert!(!named("Keyboard", name("keyboard")), "compared exactly");
        assert!(!named("Keyboard", name("")));
        const SRC: &str = include_str!("uia.rs");
        for f in ["fn element_state_probe(", "fn element_plugin_locate("] {
            let body = &SRC[SRC.find(f).unwrap()..];
            let body = &body[..body.find("\n}\n").unwrap()];
            assert!(body.contains("if !named(name, || noted(el.CurrentName())"), "{f}");
            assert!(!body.contains("n != name"), "{f} compares the name itself");
        }
    }

    /// An action that failed is said in the words a module is told, under its code: rights, time,
    /// the element, the program; anything else with its number.
    #[test]
    fn a_failed_press_says_why_in_words() {
        use crate::errors::Code;
        for (code, reason, said) in [
            (0x8007_0005_u32, &element::DENIED, "the program runs with higher rights"),
            (0x8013_1505, &element::TIMEOUT, "the program did not answer within 1000 ms"),
            (0x8004_0201, &element::ELEMENT_GONE, "the element is gone"),
            (0x8004_0200, &element::NOT_ENABLED, "the element is not enabled"),
            (0x8013_1509, &element::CANNOT_PRESS, "the element cannot be pressed"),
            (0x8004_0204, &element::CANNOT_PRESS, "the element cannot be pressed"),
            (0x8007_06BA, &element::PROGRAM_GONE, "the program is gone"),
            (0x8000_4005, &element::NOT_TAKEN, "the program did not take the action (0x80004005)"),
        ] {
            let e = failure(code as i32);
            assert_eq!((e.reason(), e.message()), (reason, said), "{code:08x}");
        }
        let codes: Vec<Code> = [0x8007_0005_u32, 0x8013_1505, 0x8004_0201, 0x8004_0200, 0x8013_1509, 0x8007_06BA, 0x8000_4005]
            .iter()
            .map(|c| failure(*c as i32).code())
            .collect();
        assert_eq!(codes, [Code::Denied, Code::Timeout, Code::Gone, Code::Refused, Code::Refused, Code::Gone, Code::Failed]);
    }

    /// The focus stays where it is: the automation object of every thread is made with UIA's
    /// automatic focus off, and a press is refused on one where that could not be done.
    #[test]
    fn an_action_leaves_the_focus_where_it_is() {
        const SRC: &str = include_str!("uia.rs");
        let made = &SRC[SRC.find("fn automation(").unwrap()..SRC.find("fn unanswered(").unwrap()];
        assert!(made.contains("a2.SetAutoSetFocus(false).is_ok()") && made.contains("ACTS_IN_PLACE.with(|c| c.set(in_place));"));
        let press = &SRC[SRC.find("fn element_press(").unwrap()..SRC.find("/// An element's on-screen rectangle").unwrap()];
        assert!(press.find("ACTS_IN_PLACE.with(|c| c.get())").unwrap() < press.find("ElementFromHandle").unwrap());
        // Invoke first, Toggle second, the default action last.
        let (i, t, d) = (press.find("Invoke()").unwrap(), press.find("Toggle()").unwrap(), press.find("DoDefaultAction()").unwrap());
        assert!(i < t && t < d);
        assert!(!press.contains("SetFocus("), "a press never moves the focus itself");
    }

    /// Only a program that did not answer makes a question not known: the timeout, higher rights,
    /// a program that went away. A search that found nothing, an element that went away while it
    /// was walked, a provider that cannot do it, are answers.
    #[test]
    fn only_a_program_that_did_not_answer_makes_a_question_not_known() {
        for code in [0x8013_1505_u32, 0x8007_0005, 0x8001_0108, 0x8007_06BA, 0x8007_06BE] {
            assert!(unanswered(code), "{code:08x}");
        }
        for code in [0x8000_4003_u32, 0x8004_0201, 0x8013_1509, 0x8004_0204, 0x8000_4005, 0x8007_0057] {
            assert!(!unanswered(code), "{code:08x}");
        }
        // The first such call is the one said, and whether it was the timeout.
        FAILED.with(|f| *f.borrow_mut() = None);
        assert!(!stopped());
        let found: windows::core::Result<u32> = Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x8000_4003_u32 as i32)));
        assert_eq!(noted(found), None);
        assert!(!stopped(), "nothing found is an answer");
        let late: windows::core::Result<u32> = Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x8013_1505_u32 as i32)));
        assert_eq!(noted(late), None);
        let denied: windows::core::Result<u32> = Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x8007_0005_u32 as i32)));
        assert_eq!(noted(denied), None);
        assert!(stopped());
        assert_eq!(FAILED.with(|f| f.borrow_mut().take()), Some((HostError::of(&element::TIMEOUT), true)));
        assert_eq!(noted::<u32>(Ok(7)), Some(7));
    }

    /// A question that cannot be put — no automation object, or a condition it cannot build — is
    /// not known, `failed`, as a press is told: every question goes through `reached` and builds its
    /// conditions and walkers through `local`, and only a press keeps its own words for the object.
    #[test]
    fn a_question_that_cannot_be_put_is_not_known() {
        FAILED.with(|f| *f.borrow_mut() = None);
        assert!(reached(&None).is_none());
        assert!(stopped(), "a walk stops there");
        assert_eq!(FAILED.with(|f| f.borrow_mut().take()), Some((HostError::of(&element::NO_INTERFACE), false)));
        let refused: windows::core::Result<u32> = Err(windows::core::Error::from_hresult(windows::core::HRESULT(0x8007_000E_u32 as i32)));
        assert_eq!(local(refused), None);
        assert_eq!(FAILED.with(|f| f.borrow_mut().take()), Some((HostError::of(&element::NO_INTERFACE), false)), "not the timeout: nothing behind it goes unasked");
        assert_eq!(local::<u32>(Ok(3)), Some(3));
        assert!(!stopped());
        assert_eq!(
            settle(ElementAnswer::Found(false), Some((HostError::of(&element::NO_INTERFACE), false)), None),
            Err(ElementFailure { why: HostError::of(&element::NO_INTERFACE), unanswered: false })
        );
        const SRC: &str = include_str!("uia.rs");
        let code = &SRC[..SRC.find("\n#[cfg(test)]\nmod tests {").unwrap()];
        // The press alone takes the object as it is, to say why in its own way.
        assert_eq!(code.matches("borrow.as_ref()").count(), 1);
        let press = &code[code.find("fn element_press(").unwrap()..code.find("/// An element's on-screen rectangle").unwrap()];
        assert!(press.contains("let Some(automation) = borrow.as_ref() else {\n            return Err((HostError::of(&element::NO_INTERFACE), false));"));
        assert_eq!(code.matches("reached(&borrow)").count(), 10, "every function that asks, but the press");
        // No condition, walker or element the object makes is dropped as nothing found; the cache
        // request alone falls back, to the walk.
        let press_at = code.find("fn element_press(").unwrap()..code.find("/// An element's on-screen rectangle").unwrap();
        for made in ["CreatePropertyCondition", "CreateAndCondition", "CreateOrCondition", "CreateTrueCondition", "RawViewWalker()"] {
            for (at, _) in code.match_indices(made) {
                let line = code[..at].rsplit('\n').next().unwrap().to_string() + &code[at..at + code[at..].find('\n').unwrap()];
                assert!(!line.contains(".ok()"), "{line}");
                // Made through `local` — the press aside, which says why in its own words — so a
                // condition not made is noted, and never skipped past (`Err(_) => continue`).
                if !press_at.contains(&at) && !line.trim_start().starts_with("//") {
                    assert!(code[..at].ends_with("local(automation."), "{line}");
                }
            }
        }
        // findAny: a name whose condition cannot be built ends the search, as an unanswered one
        // does — the names after it are not asked.
        let any = crate::ocr_wiring_tests::body(code, "fn element_find_any(");
        assert!(any.contains(
            "let cond_name = local(automation.CreatePropertyCondition(UIA_NamePropertyId, &v))?;\n                // A name that cannot be asked ends the search"
        ));
        assert!(any.contains("local(automation.CreateAndCondition(&cond_name, &cond_type))?\n            };"));
        assert!(!any.contains("continue"), "no name is skipped past");
    }

    /// What was found stands whatever else went unanswered; nothing found after a call that went
    /// unanswered is not known, never no; a dump keeps what it found and says why it is not all.
    #[test]
    fn nothing_found_after_an_unanswered_call_is_not_known() {
        let late = || Some((HostError::of(&element::TIMEOUT), true));
        assert_eq!(settle(ElementAnswer::Found(true), late(), None), Ok(ElementAnswer::Found(true)));
        assert_eq!(settle(ElementAnswer::Index(Some(2)), late(), None), Ok(ElementAnswer::Index(Some(2))));
        assert_eq!(settle(ElementAnswer::Point(Some((5, 6))), late(), None), Ok(ElementAnswer::Point(Some((5, 6)))));
        for nothing in [
            ElementAnswer::Found(false),
            ElementAnswer::Index(None),
            ElementAnswer::Point(None),
            ElementAnswer::State(None),
            ElementAnswer::Step(None),
            ElementAnswer::Within(None),
        ] {
            assert_eq!(settle(nothing.clone(), None, None), Ok(nothing.clone()), "an answer when every call was answered");
            assert_eq!(
                settle(nothing.clone(), late(), None),
                Err(ElementFailure { why: HostError::of(&element::TIMEOUT), unanswered: true }),
                "{nothing:?}"
            );
        }
        let denied = Some((HostError::of(&element::DENIED), false));
        assert_eq!(
            settle(ElementAnswer::Found(false), denied.clone(), None),
            Err(ElementFailure { why: HostError::of(&element::DENIED), unanswered: false })
        );
        let node = DumpNode { depth: 0, name: "FILE".into(), class: String::new(), ctype: 50000, x: 1, y: 2, w: 3, h: 4 };
        assert_eq!(
            settle(ElementAnswer::Nodes(vec![node.clone()], None), late(), None),
            Ok(ElementAnswer::Nodes(vec![node.clone()], Some(HostError::of(&element::TIMEOUT))))
        );
        assert_eq!(settle(ElementAnswer::Nodes(vec![node.clone()], None), None, None), Ok(ElementAnswer::Nodes(vec![node], None)));
    }

    /// A walk its bound stopped before the end of the tree — its node budget, or its time — is not
    /// "not there" either: nothing found is not known, with the bound, and the questions behind it
    /// are still asked (it answered every call); what was found stands; a dump says why it is not
    /// all; a program that did not answer is said before the bound. Reached with a node to spare,
    /// the budget notes nothing.
    #[test]
    fn a_walk_cut_short_by_its_bound_is_not_known() {
        let cut = || Some(cut_by_nodes(4000));
        assert_eq!(cut().unwrap(), HostError::of(&element::TOO_MANY_NODES));
        assert_eq!(cut().unwrap().message(), "the tree holds more than the 4000 elements a search reads");
        assert_eq!(cut_by_time(), HostError::of(&element::TOO_SLOW));
        assert_eq!(cut_by_time().message(), "the search took longer than 5 s");
        assert_eq!(
            settle(ElementAnswer::Point(None), None, cut()),
            Err(ElementFailure { why: HostError::of(&element::TOO_MANY_NODES), unanswered: false })
        );
        assert_eq!(settle(ElementAnswer::State(Some((1, 16))), None, cut()), Ok(ElementAnswer::State(Some((1, 16)))));
        assert_eq!(
            settle(ElementAnswer::Nodes(Vec::new(), None), None, Some(cut_by_time())),
            Ok(ElementAnswer::Nodes(Vec::new(), Some(HostError::of(&element::TOO_SLOW))))
        );
        let late = Some((HostError::of(&element::TIMEOUT), true));
        assert_eq!(
            settle(ElementAnswer::Point(None), late, cut()),
            Err(ElementFailure { why: HostError::of(&element::TIMEOUT), unanswered: true })
        );
        // The budget — the walks' 4000, whose words the message is — the last node is spent without
        // a word; asked for one more, it says so, once.
        CUT.with(|c| *c.borrow_mut() = None);
        let mut b = Budget::new(4000);
        assert!((0..4000).all(|_| b.spend()));
        assert!(CUT.with(|c| c.borrow().is_none()) && !b.stopped, "a tree of exactly the budget is all walked");
        assert!(!b.spend() && b.stopped);
        assert!(!b.spend());
        assert_eq!(CUT.with(|c| c.borrow_mut().take()), Some(HostError::of(&element::TOO_MANY_NODES)));
    }
}
