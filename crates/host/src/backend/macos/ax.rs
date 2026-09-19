//! The accessibility tree: what windows exist, what is inside them, and where.
//!
//! Everything the Windows backend does with `HWND`s and UI Automation is answered here from
//! one API. The `uia_*` methods on the trait keep their names because that is where they
//! were born, but each one is a QUESTION — is this element present, where would I click it,
//! what does it say about its own state, step the focus — and this module answers the
//! question rather than imitating the mechanism.
//!
//! Three rules hold for every function here, taken from the Windows implementation because
//! the host depends on them: never panic and never propagate an error (a failure is `None`,
//! `false` or empty); bound every traversal by nodes and by depth, since an unbounded walk
//! against an unresponsive plugin hangs the thread that carries the keyboard; and return
//! `None` for an element whose rectangle is collapsed or off-screen, so nothing ever clicks
//! the centre of a zero-size box.
//!
//! Two macOS-specific facts shape the rest.
//!
//! **Every read here is a synchronous IPC call into another process.** A tree walk is not a
//! memory traversal, it is hundreds of round trips, and a plugin that is mid-repaint answers
//! slowly or not at all. So the messaging timeout is turned down from the six-second default
//! to [`MESSAGING_TIMEOUT`] the first time this module touches accessibility, and every
//! element's properties are read in ONE batched call rather than one call per attribute.
//!
//! **There is no window class.** The join key that every module's plugin detection matches
//! on — `"Qt%d+.-QWindowIcon"` on Windows — has to be synthesised, and [`join_class`] is
//! where that decision lives. Read it before writing a macOS module variant.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use core::ffi::c_void;
use core::ptr::NonNull;

use objc2_app_kit::{
    NSAccessibilityChildrenAttribute, NSAccessibilityDescriptionAttribute,
    NSAccessibilityFocusedAttribute, NSAccessibilityFocusedUIElementAttribute,
    NSAccessibilityFocusedWindowAttribute, NSAccessibilityIdentifierAttribute,
    NSAccessibilityMainWindowAttribute, NSAccessibilityMinimizedAttribute,
    NSAccessibilityParentAttribute, NSAccessibilityPositionAttribute, NSAccessibilityRoleAttribute,
    NSAccessibilitySizeAttribute, NSAccessibilitySubroleAttribute, NSAccessibilityTitleAttribute,
    NSAccessibilityValueAttribute, NSAccessibilityWindowsAttribute, NSApplicationActivationOptions,
    NSApplicationActivationPolicy, NSRunningApplication, NSWindow, NSWorkspace,
};
use objc2_application_services::{
    AXCopyMultipleAttributeOptions, AXError, AXUIElement, AXValue, AXValueType,
};
// `Type` is imported for its `retain` method, which is how a borrowed element handed to a
// walk is kept past the end of the walk. Without the trait in scope the compiler suggests
// `retain_count`, which is not the same thing at all.
use objc2_core_foundation::{
    CFArray, CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, CGPoint, CGRect,
    CGSize, Type,
};
use objc2_core_graphics::{
    kCGWindowBounds, kCGWindowLayer, kCGWindowNumber, kCGWindowOwnerPID,
    CGRectMakeWithDictionaryRepresentation, CGWindowListCopyWindowInfo, CGWindowListOption,
};
use objc2_foundation::NSString;

use super::budget::Budget;
use super::front_memory::{Memory, Recall};
use super::handles;
use crate::backend::{ControlInfo, DumpNode, WinInfo};

/// How long any one accessibility call may take before it is abandoned, in seconds.
///
/// The default is about six seconds per call. On a walk of a few hundred elements against a
/// plugin that has stopped answering, that is not a delay, it is a hang — and the thread it
/// hangs is the one carrying speech, the hotkeys and the event tap, which the system
/// switches off if it stops responding. A quarter of a second is far longer than a healthy
/// answer takes (single-digit milliseconds) and short enough that even a whole walk of dead
/// elements stays inside the pump's budget.
///
/// It was a quarter of a second, on the reasoning that a healthy answer takes single-digit
/// milliseconds. True of a healthy answer, and the wrong budget. A tester's log showed
/// sforzando on a 2015 MacBook Air with VoiceOver running never once answering inside it:
/// every read timed out, the application went into the penalty box, the next read was skipped
/// outright, and the overlay never saw a window it could plainly read — the manual probe in the
/// same session dumped sixteen elements out of it without trouble.
///
/// A second is generous for a healthy application and still a bounded stall for a dead one,
/// because the penalty box below is what stops a wedged plugin charging it repeatedly. The two
/// have to be read together: a longer budget needs a longer quarantine.
pub(super) const MESSAGING_TIMEOUT: f32 = 1.0;

/// Node and depth ceilings. Every walk in this file passes through [`walk`] and every one of
/// them is bounded: the trees these run against are written by plugin vendors, and one that
/// is cyclic or merely enormous must cost us a bounded amount rather than the session.
const QUERY_NODES: i32 = 1500;
const QUERY_DEPTH: i32 = 24;
const DUMP_NODES: i32 = 2000;
const DUMP_DEPTH: i32 = 24;
/// The control walk is hot — every focus change asks for it — so it is bounded far more
/// tightly than a query. Plugin surfaces sit near the top of a window's tree; going deeper
/// only collects the individual buttons, which are not surfaces and cost an IPC round trip
/// each.
const CONTROL_NODES: i32 = 600;
const CONTROL_DEPTH: i32 = 8;
const CONTROL_MAX: usize = 256;
const FOCUS_NODES: i32 = 800;
const FOCUS_DEPTH: i32 = 20;
/// Children examined per node. A list with ten thousand rows is a real thing and none of the
/// questions here are answered by the ten-thousandth one.
const CHILDREN_MAX: usize = 256;

/// How long a HOT walk may take: the control walk, the queries, the focus step.
///
/// The node counts above bound how many elements a walk visits. They do not bound how long
/// that takes, and on this platform every element is a cross-process round trip whose own
/// ceiling is [`MESSAGING_TIMEOUT`] — so six hundred nodes is bounded at six hundred seconds,
/// which is not a bound anybody can use. The Windows backend learned exactly this on
/// 2026-09-04: after its per-call timeout went in, one window still took thirty seconds to
/// walk, because a budget that counts nodes cannot see a clock.
///
/// **300 ms is not a guess.** It is the figure this file already names as the point past
/// which macOS stops waiting for the event tap and switches it off — see the pump's own
/// warning line. A walk that crosses it has already cost the user their keyboard, so there is
/// nothing to be gained by letting it finish. Twenty times the pump's 15 ms budget, so it
/// cannot fire on a healthy tree.
const HOT_DEADLINE: Duration = Duration::from_millis(300);

/// And how long the DIAGNOSTIC dump may take, which is a different trade.
///
/// `host.element.rawDump` is how a plug-in's tree gets read at all, by somebody who cannot
/// see it; a truncated dump is worth much less than a slow one, and nothing is waiting on the
/// keyboard while a tester presses the probe key on purpose. Five seconds, the same number and
/// the same reasoning as the Windows side.
///
/// This is the one that matters most right now. The most valuable press of the next session is
/// the probe over a Kontakt or Komplete Kontrol window, and that is by a distance the largest
/// tree this has ever been pointed at: sforzando's whole window came to sixteen nodes. A hang
/// there would cost the answer the session exists for.
const DUMP_DEADLINE: Duration = Duration::from_secs(5);

/// An observation slower than this is worth a line in the log: the host warns at the same
/// threshold, and on this platform the cause is almost always the target app, not us.
const SLOW_MS: u128 = 50;

// ---------------------------------------------------------------------------------------
// Attribute names
//
// The kAX* string constants do not exist in objc2-application-services — the generated
// constant files are empty. AppKit's NSAccessibility* names are the same strings, exported
// as &'static NSString, and NSString is toll-free bridged with CFString, so these cost
// nothing per call and cannot be misspelled.
// ---------------------------------------------------------------------------------------

/// SAFETY: `NSString` and `CFString` are toll-free bridged; the reference keeps its lifetime.
#[inline]
fn ns_to_cf(s: &NSString) -> &CFString {
    unsafe { &*(s as *const NSString as *const CFString) }
}

macro_rules! attr_names {
    ($($fn_name:ident => $static_name:ident),* $(,)?) => {
        $(
            #[inline]
            fn $fn_name() -> &'static CFString {
                ns_to_cf(unsafe { $static_name })
            }
        )*
    };
}

attr_names! {
    a_role => NSAccessibilityRoleAttribute,
    a_subrole => NSAccessibilitySubroleAttribute,
    a_identifier => NSAccessibilityIdentifierAttribute,
    a_title => NSAccessibilityTitleAttribute,
    a_description => NSAccessibilityDescriptionAttribute,
    a_value => NSAccessibilityValueAttribute,
    a_position => NSAccessibilityPositionAttribute,
    a_size => NSAccessibilitySizeAttribute,
    a_children => NSAccessibilityChildrenAttribute,
    a_parent => NSAccessibilityParentAttribute,
    a_windows => NSAccessibilityWindowsAttribute,
    a_focused_window => NSAccessibilityFocusedWindowAttribute,
    a_main_window => NSAccessibilityMainWindowAttribute,
    a_focused_element => NSAccessibilityFocusedUIElementAttribute,
    a_focused => NSAccessibilityFocusedAttribute,
    a_minimized => NSAccessibilityMinimizedAttribute,
}

// ---------------------------------------------------------------------------------------
// Errors, and saying which kind out loud
// ---------------------------------------------------------------------------------------

fn err_name(err: AXError) -> &'static str {
    match err {
        AXError::Success => "success",
        AXError::APIDisabled => "APIDisabled",
        AXError::NotImplemented => "NotImplemented",
        AXError::AttributeUnsupported => "AttributeUnsupported",
        AXError::ActionUnsupported => "ActionUnsupported",
        AXError::NotificationUnsupported => "NotificationUnsupported",
        AXError::CannotComplete => "CannotComplete",
        AXError::InvalidUIElement => "InvalidUIElement",
        AXError::InvalidUIElementObserver => "InvalidUIElementObserver",
        AXError::NoValue => "NoValue",
        AXError::IllegalArgument => "IllegalArgument",
        AXError::Failure => "Failure",
        _ => "other",
    }
}

thread_local! {
    /// Which "this is broken, not merely absent" errors have already been reported. A
    /// missing permission produces one of these on every single call, and a log that
    /// repeats it ten thousand times is a log nobody can read.
    static REPORTED: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
    /// Last time an app-is-busy failure was reported, so a stuck plugin says so
    /// periodically rather than continuously.
    static LAST_BUSY: Cell<Option<Instant>> = const { Cell::new(None) };

    /// Applications that failed to answer, and when it is worth asking them again.
    ///
    /// One unresponsive application used to cost the messaging timeout SEVERAL TIMES per
    /// call, because resolving its frontmost window asks for a focused window, then a main
    /// window, then the window list. Measured on a first macOS session: 699 ms inside one
    /// `active_window`, against a pump budget of 15 ms and a threshold of 300 ms past which
    /// the system switches the key tap off. Remembering that an application is not talking
    /// turns that into one timeout every few seconds instead of three per question.
    static BUSY_UNTIL: RefCell<HashMap<i32, Instant>> = RefCell::new(HashMap::new());

    /// What each application last said when asked which of its windows is in front.
    ///
    /// The rules live in [`super::front_memory`], where they can be tested on a machine with
    /// no accessibility API. This is the per-thread instance, and the only thing here that
    /// knows how to ask whether a process is still running.
    static LAST_FRONT: RefCell<Memory> = RefCell::new(Memory::default());

    /// When a served-from-memory answer was last written to the log, so the note appears
    /// without one line per pump tick.
    static LAST_REMEMBERED: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Records what an application answered, so the next question has a fall-back if it stops.
fn remember_front(pid: i32, handle: isize, info: Option<WinInfo>) {
    LAST_FRONT.with(|m| {
        m.borrow_mut().remember(pid, handle, info, Instant::now(), |p| {
            // The one part of this that cannot be compiled at home, which is why it is passed
            // in rather than reached for: `libc` is a macOS-only dependency here.
            unsafe { libc::kill(p, 0) == 0 }
        })
    });
}

/// Forgets an application's window: it has said it no longer has one, or what is remembered
/// has just been ruled out.
fn forget_front(pid: i32) {
    LAST_FRONT.with(|m| m.borrow_mut().forget(pid));
}

/// Says that an answer came out of memory rather than out of the application.
///
/// At line level rather than trace: a remembered window may since have moved, and a session
/// where an overlay worked from stale geometry has to be explainable afterwards from the log
/// alone — the only thing a remote tester can send. Rate-limited to one line a second,
/// because the question behind it is asked on every pump tick and the quarantine lasts five.
fn note_remembered(pid: i32, age: Duration) {
    let now = Instant::now();
    let due = LAST_REMEMBERED.with(|c| match c.get() {
        Some(prev) if now.duration_since(prev) < Duration::from_secs(1) => false,
        _ => {
            c.set(Some(now));
            true
        }
    });
    if due {
        crate::logging::line(
            "macos",
            &format!(
                "active_window: {} (pid {pid}) is not answering — serving the window it \
                 last described, {} ms ago",
                exe_for_pid(pid),
                age.as_millis()
            ),
        );
    }
}

/// How long an application that failed to answer is left alone.
///
/// Long enough that a wedged plugin cannot dominate the pump, short enough that one that
/// was merely busy redrawing is back in the conversation before the user notices. A guess,
/// and one the log can correct: every skipped question is traced.
///
/// Raised from 1.5s together with the messaging timeout. With a one-second budget a genuinely
/// wedged application would otherwise cost a second of pump every 1.5 — two thirds of the
/// thread that carries speech. Five seconds keeps it to a fifth, while an application that is
/// merely slow now answers on its first attempt instead of being quarantined for missing a
/// deadline it could never have met.
pub(super) const BUSY_PENALTY: Duration = Duration::from_millis(5000);

/// Note that an application is not answering. Also fed by a refused observer subscription
/// (`watch::create_observer`), which asks the same process the same way.
pub(super) fn note_busy(pid: i32) {
    if pid > 0 {
        BUSY_UNTIL.with(|b| b.borrow_mut().insert(pid, Instant::now() + BUSY_PENALTY));
    }
}

/// Is this application still in the penalty box? Expired entries are dropped as they are
/// found, so the map cannot grow beyond the applications that have recently misbehaved.
pub(super) fn is_busy(pid: i32) -> bool {
    BUSY_UNTIL.with(|b| {
        let mut b = b.borrow_mut();
        match b.get(&pid) {
            Some(until) if *until > Instant::now() => true,
            Some(_) => {
                b.remove(&pid);
                false
            }
            None => false,
        }
    })
}

/// Whether to give up on a call into an application that is not answering.
///
/// The quarantine only ever meant anything at the two places that consulted it. Everything
/// else resolved a handle and walked straight in — which is how serving a REMEMBERED window
/// from `active_window` turned into `host.window.controls()` walking six hundred nodes of a
/// wedged application on the very same tick, the stall removed from one function and paid one
/// function later. The overlay runtime asks `active()` and then, if it got a window, asks for
/// its controls; before the memory existed the first answer was `nil` and the second question
/// was never asked at all.
///
/// Traced rather than logged at line level, unlike the same check in `enumerate_windows`:
/// that one is cold, these are asked on every tick, and a line each would bury the log. The
/// once-a-second line from `note_remembered` is what explains the session.
fn skip_busy(pid: i32, what: &str) -> bool {
    if !is_busy(pid) {
        return false;
    }
    crate::logging::trace("macos", || {
        format!(
            "{what}: not asking {} (pid {pid}), it did not answer a moment ago",
            exe_for_pid(pid)
        )
    });
    true
}

/// Classifies a failed accessibility call and logs the ones a person could act on.
///
/// The distinction matters more here than the failure does. `AttributeUnsupported` is
/// ordinary noise on a tree walk — most elements have no `AXIdentifier` — while
/// `APIDisabled` means the Accessibility permission was never granted and *every* call from
/// now on will fail the same way, and `CannotComplete` means the plugin is not answering.
/// A tester who can only send us a log file needs those three to read differently.
fn note_error(err: AXError, what: &str) {
    match err {
        AXError::AttributeUnsupported | AXError::NoValue | AXError::ActionUnsupported => {}
        AXError::APIDisabled | AXError::NotImplemented => {
            let name = err_name(err);
            let first = REPORTED.with(|r| r.borrow_mut().insert(name));
            if first {
                crate::logging::line(
                    "macos",
                    &format!(
                        "accessibility refused ({name}) reading {what} — without the \
                         Accessibility permission every element read comes back empty; grant \
                         it in System Settings > Privacy & Security > Accessibility"
                    ),
                );
            }
        }
        AXError::CannotComplete => {
            let now = Instant::now();
            let due = LAST_BUSY
                .with(|c| c.get())
                .is_none_or(|t| now.duration_since(t).as_secs() >= 5);
            if due {
                LAST_BUSY.with(|c| c.set(Some(now)));
                crate::logging::line(
                    "macos",
                    &format!(
                        "accessibility could not complete reading {what} — the target \
                         application is busy, starting or not responding (it is waited for at \
                         most {MESSAGING_TIMEOUT}s, but one not ready to answer refuses at \
                         once); the answer was dropped"
                    ),
                );
            }
        }
        _ => crate::logging::trace("macos", || {
            format!("ax: {} reading {what}", err_name(err))
        }),
    }
}

// ---------------------------------------------------------------------------------------
// Elements: creating them, and reading one attribute at a time
// ---------------------------------------------------------------------------------------

thread_local! {
    static TIMEOUT_SET: Cell<bool> = const { Cell::new(false) };
    /// Whether a walk has ever run out of budget. One line per session is the right amount:
    /// if it happens once it will happen on every recheck.
    static BUDGET_REPORTED: Cell<bool> = const { Cell::new(false) };
    /// Executable file names by pid. The host's own comment about `active_window` calls
    /// this out: it looks cheap and is not, because it resolves the owning process's name,
    /// and every embedded overlay asks for it on every recheck.
    static EXE_BY_PID: RefCell<HashMap<i32, String>> = RefCell::new(HashMap::new());
    /// Bundle identifiers by pid, cached for the same reason and read on the same path.
    static BUNDLE_BY_PID: RefCell<HashMap<i32, String>> = RefCell::new(HashMap::new());
    /// Window classes whose content rect was found to sit below the frame, so the
    /// derivation is reported once per class rather than once per call.
    static INSET_REPORTED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    /// Frame-to-content edge offsets by window handle — see `content_rect`. Bounded by the
    /// handle table, which sweeps its own dead entries; a stale offset for a handle that is
    /// never asked about again costs four floats.
    static INSET_BY_HANDLE: RefCell<HashMap<isize, (f64, f64, f64, f64)>> =
        RefCell::new(HashMap::new());
    /// UIA control types with no macOS mapping, reported once each.
    static UNMAPPED_REPORTED: RefCell<HashSet<i32>> = RefCell::new(HashSet::new());
}

/// Turns the messaging timeout down, once per process.
///
/// Setting it on the system-wide element sets it for this client as a whole, which is what
/// we want: elements are created all over this file and in the other backend modules, and a
/// timeout that had to be remembered at every creation site would eventually be forgotten
/// at one of them — and one forgotten site is enough to hang the pump.
fn ensure_timeout() {
    if TIMEOUT_SET.with(|c| c.replace(true)) {
        return;
    }
    let sw = unsafe { AXUIElement::new_system_wide() };
    let err = unsafe { sw.set_messaging_timeout(MESSAGING_TIMEOUT) };
    if err == AXError::Success {
        crate::logging::line(
            "macos",
            &format!("accessibility messaging timeout set to {MESSAGING_TIMEOUT}s process-wide"),
        );
    } else {
        crate::logging::line(
            "macos",
            &format!(
                "could not set the accessibility messaging timeout ({}) — calls will use the \
                 ~6s default and a frozen plugin can stall the pump",
                err_name(err)
            ),
        );
    }
}

/// The system-wide element — the root everything else hangs off, and the only element that
/// answers "what has keyboard focus" across applications.
pub(super) fn system_wide() -> CFRetained<AXUIElement> {
    ensure_timeout();
    unsafe { AXUIElement::new_system_wide() }
}

/// The application element for a process id.
pub(super) fn app_element(pid: i32) -> CFRetained<AXUIElement> {
    ensure_timeout();
    let el = unsafe { AXUIElement::new_application(pid) };
    // Also set it here: the process-wide setting above is the belt, this is the braces, and
    // an element created before `ensure_timeout` ever ran would otherwise keep the default.
    let _ = unsafe { el.set_messaging_timeout(MESSAGING_TIMEOUT) };
    el
}

/// One attribute of one element, or `None` if it is absent or the read failed.
///
/// `Ok(None)` from the API (the call succeeded, the value was NULL) and an outright failure
/// both come back as `None` — no caller in this backend distinguishes them, and the failure
/// has already been classified into the log by then.
pub(super) fn attribute(el: &AXUIElement, name: &CFString) -> Option<CFRetained<CFType>> {
    let mut raw: *const CFType = core::ptr::null();
    let err = unsafe { el.copy_attribute_value(name, NonNull::from(&mut raw)) };
    if err != AXError::Success {
        if err == AXError::CannotComplete {
            // Recorded here because this is the funnel every attribute read goes through,
            // and it is the only place that has both the failure and the element to ask
            // which application produced it.
            note_busy(element_pid(el));
        }
        note_error(err, &name.to_string());
        return None;
    }
    // Copy rule: the crate does NOT wrap this out-pointer, so it is ours to own.
    NonNull::new(raw.cast_mut()).map(|p| unsafe { CFRetained::from_raw(p) })
}

/// An attribute read as a string, empty and absent both collapsing to `None`.
pub(super) fn attribute_string(el: &AXUIElement, name: &CFString) -> Option<String> {
    let v = attribute(el, name)?;
    let s = v.downcast_ref::<CFString>()?.to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// An attribute read as another element.
pub(super) fn attribute_element(el: &AXUIElement, name: &CFString) -> Option<CFRetained<AXUIElement>> {
    let v = attribute(el, name)?;
    v.downcast_ref::<AXUIElement>().map(|e| e.retain())
}

/// An element attribute read that tells a FAILED read apart from an absent value.
///
/// `attribute` folds both into `None`, and for every other caller that is right: nobody
/// there needs to know. The focus chain does. "Nothing reports focus" and "the application
/// did not answer within the messaging timeout" are opposite answers to the question a
/// caller is asking — the first means the keyboard is in the window itself, the second
/// means nothing is known — and folding them made a timed-out read look like success.
pub(super) fn attribute_element_checked(
    el: &AXUIElement,
    name: &CFString,
) -> Result<Option<CFRetained<AXUIElement>>, AXError> {
    let mut raw: *const CFType = core::ptr::null();
    let err = unsafe { el.copy_attribute_value(name, NonNull::from(&mut raw)) };
    match err {
        // The same three `note_error` files as "expected absence" rather than failure —
        // classified here the way the funnel already classifies them, not by a second
        // opinion of what counts as absent.
        AXError::Success | AXError::NoValue | AXError::AttributeUnsupported => {
            let v = NonNull::new(raw.cast_mut()).map(|p| unsafe { CFRetained::from_raw(p) });
            Ok(v.and_then(|v: CFRetained<CFType>| v.downcast_ref::<AXUIElement>().map(|e| e.retain())))
        }
        other => {
            if other == AXError::CannotComplete {
                note_busy(element_pid(el));
            }
            note_error(other, &name.to_string());
            Err(other)
        }
    }
}

/// An attribute read as a boolean. Absent is `None`, not `false`: for `AXMinimized` those
/// two mean different things.
fn attribute_bool(el: &AXUIElement, name: &CFString) -> Option<bool> {
    let v = attribute(el, name)?;
    if let Some(b) = v.downcast_ref::<CFBoolean>() {
        return Some(b.as_bool());
    }
    v.downcast_ref::<CFNumber>().and_then(|n| n.as_i64()).map(|n| n != 0)
}

/// The children of an element, capped.
pub(super) fn children(el: &AXUIElement) -> Vec<CFRetained<AXUIElement>> {
    let Some(v) = attribute(el, a_children()) else {
        return Vec::new();
    };
    let Some(arr) = v.downcast_ref::<CFArray>() else {
        return Vec::new();
    };
    // SAFETY: AXChildren is documented as an array of AXUIElementRef.
    let typed: &CFArray<AXUIElement> = unsafe { arr.cast_unchecked::<AXUIElement>() };
    let n = typed.len().min(CHILDREN_MAX);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if let Some(c) = typed.get(i) {
            out.push(c);
        }
    }
    out
}

/// The process that owns an element, or 0 if it could not be asked.
pub(super) fn element_pid(el: &AXUIElement) -> i32 {
    let mut pid: libc::pid_t = 0;
    let err = unsafe { el.pid(NonNull::from(&mut pid)) };
    if err == AXError::Success {
        pid
    } else {
        note_error(err, "pid");
        0
    }
}

fn ax_value_point(v: &AXValue) -> Option<CGPoint> {
    let mut out = CGPoint::new(0.0, 0.0);
    let ptr = NonNull::from(&mut out).cast::<c_void>();
    unsafe { v.value(AXValueType::CGPoint, ptr) }.then_some(out)
}

fn ax_value_size(v: &AXValue) -> Option<CGSize> {
    let mut out = CGSize::new(0.0, 0.0);
    let ptr = NonNull::from(&mut out).cast::<c_void>();
    unsafe { v.value(AXValueType::CGSize, ptr) }.then_some(out)
}

// ---------------------------------------------------------------------------------------
// Reading a whole element in one round trip
// ---------------------------------------------------------------------------------------

/// The seven attributes every walk in this file wants, in the order [`Snap`] unpacks them.
const SNAP_ATTRS: usize = 7;

/// What one element is and where it sits — everything a walk needs to decide about it.
#[derive(Default, Clone)]
pub(super) struct Snap {
    pub role: String,
    pub subrole: String,
    pub identifier: String,
    pub title: String,
    pub description: String,
    pub rect: Option<CGRect>,
}

impl Snap {
    /// The label a module means when it asks for an element "by name".
    ///
    /// UIA's Name property has no single macOS counterpart. `AXTitle` is the usual carrier,
    /// but a great many controls — anything drawn rather than laid out, which is most of a
    /// plugin — leave it empty and put the label in `AXDescription` instead. Both are
    /// accepted, and a module written against a dump will see whichever one is populated.
    fn matches_name(&self, name: &str) -> bool {
        if name.is_empty() {
            return true; // "any element of this type", exactly as on Windows
        }
        self.title == name || self.description == name
    }
}

thread_local! {
    /// The attribute-name array handed to the batched read, built once.
    static SNAP_KEYS: RefCell<Option<CFRetained<CFArray<CFString>>>> = const { RefCell::new(None) };
    static BATCH_FELL_BACK: Cell<bool> = const { Cell::new(false) };
}

fn snap_keys() -> CFRetained<CFArray<CFString>> {
    SNAP_KEYS.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(CFArray::from_objects(&[
                a_role(),
                a_subrole(),
                a_identifier(),
                a_title(),
                a_description(),
                a_position(),
                a_size(),
            ]));
        }
        c.as_ref().expect("just filled").clone()
    })
}

/// Everything about one element, in a single cross-process call where that works.
///
/// Seven separate `copy_attribute_value` calls are seven round trips into another process,
/// and a walk multiplies that by every node it visits — this is the difference between a
/// control walk that costs a few milliseconds and one that costs a few hundred, which on
/// this thread is the difference between working and having the event tap switched off.
/// `AXUIElementCopyMultipleAttributeValues` asks for all seven at once. If it refuses, or
/// answers with a differently-shaped array than it was asked for, the individual reads are
/// still correct and are used instead — once, loudly, so the log says which path is running.
pub(super) fn snapshot(el: &AXUIElement) -> Snap {
    if !BATCH_FELL_BACK.with(|c| c.get()) {
        if let Some(s) = snapshot_batched(el) {
            return s;
        }
    }
    snapshot_individually(el)
}

fn snapshot_batched(el: &AXUIElement) -> Option<Snap> {
    let keys = snap_keys();
    let mut raw: *const CFArray = core::ptr::null();
    // Options 0 (not StopOnError): an element that lacks one attribute still answers for
    // the rest, with an error placeholder in that slot, which downcasts to nothing below.
    let err = unsafe {
        el.copy_multiple_attribute_values(
            keys.as_opaque(),
            AXCopyMultipleAttributeOptions(0),
            NonNull::from(&mut raw),
        )
    };
    if err != AXError::Success {
        // A per-element failure is not a reason to abandon the batched path forever — an
        // element can simply be dead. Only a shape we cannot read is (see below).
        note_error(err, "multiple attributes");
        return None;
    }
    let values: CFRetained<CFArray> = NonNull::new(raw.cast_mut())
        .map(|p| unsafe { CFRetained::from_raw(p) })?;
    // SAFETY: the values array is documented as one CFTypeRef per requested attribute.
    let typed: &CFArray<CFType> = unsafe { values.cast_unchecked::<CFType>() };
    if typed.len() != SNAP_ATTRS {
        BATCH_FELL_BACK.with(|c| c.set(true));
        crate::logging::line(
            "macos",
            &format!(
                "batched attribute read returned {} values for {SNAP_ATTRS} attributes — \
                 falling back to one call per attribute for the rest of this session (slower, \
                 same answers)",
                typed.len()
            ),
        );
        return None;
    }

    let text = |i: usize| -> String {
        typed
            .get(i)
            .and_then(|v| v.downcast_ref::<CFString>().map(|s| s.to_string()))
            .unwrap_or_default()
    };
    let point = typed
        .get(5)
        .and_then(|v| v.downcast_ref::<AXValue>().and_then(ax_value_point));
    let size = typed
        .get(6)
        .and_then(|v| v.downcast_ref::<AXValue>().and_then(ax_value_size));
    Some(Snap {
        role: text(0),
        subrole: text(1),
        identifier: text(2),
        title: text(3),
        description: text(4),
        rect: match (point, size) {
            (Some(p), Some(s)) => Some(CGRect::new(p, s)),
            _ => None,
        },
    })
}

fn snapshot_individually(el: &AXUIElement) -> Snap {
    Snap {
        role: attribute_string(el, a_role()).unwrap_or_default(),
        subrole: attribute_string(el, a_subrole()).unwrap_or_default(),
        identifier: attribute_string(el, a_identifier()).unwrap_or_default(),
        title: attribute_string(el, a_title()).unwrap_or_default(),
        description: attribute_string(el, a_description()).unwrap_or_default(),
        rect: element_rect_cg(el),
    }
}

/// The element's frame, in points, top-left origin — the coordinate space everything on
/// this side of the trait speaks.
///
/// `AXPosition` is already measured from the top-left of the primary display, unlike
/// `NSWindow`/`NSScreen`, so nothing is flipped here. Anything that flips a coordinate does
/// it where it reads it, never later.
fn element_rect_cg(el: &AXUIElement) -> Option<CGRect> {
    let p = attribute(el, a_position())?;
    let s = attribute(el, a_size())?;
    let origin = ax_value_point(p.downcast_ref::<AXValue>()?)?;
    let size = ax_value_size(s.downcast_ref::<AXValue>()?)?;
    Some(CGRect::new(origin, size))
}

/// The element's frame as integer points: `(x, y, w, h)`. For the other backend modules.
// Allowed dead: this and the two below are the accessibility surface the REST of the
// backend reaches through, so they exist whether or not anything in this file calls them.
#[allow(dead_code)]
pub(super) fn element_rect(el: &AXUIElement) -> Option<(i32, i32, i32, i32)> {
    let r = element_rect_cg(el)?;
    Some(rect_i32(r))
}

fn rect_i32(r: CGRect) -> (i32, i32, i32, i32) {
    (
        r.origin.x.round() as i32,
        r.origin.y.round() as i32,
        r.size.width.round() as i32,
        r.size.height.round() as i32,
    )
}

/// The click point of a rectangle: its centre, or `None` if the rectangle is collapsed.
///
/// The `None` matters. A zero-size or negative rectangle is what a hidden control, a
/// collapsed panel and an element that has been asked about after its window closed all
/// report, and its "centre" is a perfectly plausible-looking coordinate somewhere the user
/// did not intend to click.
fn centre(r: CGRect) -> Option<(i32, i32)> {
    if r.size.width <= 0.0 || r.size.height <= 0.0 {
        return None;
    }
    Some((
        (r.origin.x + r.size.width / 2.0).round() as i32,
        (r.origin.y + r.size.height / 2.0).round() as i32,
    ))
}

/// An element's role, e.g. `"AXWindow"`. For the other backend modules.
#[allow(dead_code)]
pub(super) fn role(el: &AXUIElement) -> String {
    attribute_string(el, a_role()).unwrap_or_default()
}

/// **The join key every module's plugin detection matches on**, in the `class` field of
/// `WinInfo` and `ControlInfo`.
///
/// Windows has a window class string and modules match it with Luau patterns —
/// `string.match(c.class, "Qt%d+.-QWindowIcon")` is the gate every embedded overlay passes
/// through. macOS has nothing of the kind, so one is published here, and the format is part
/// of the platform's contract with its modules rather than an implementation detail:
///
/// ```text
/// AXRole/AXSubrole/AXIdentifier
/// ```
///
/// All three parts come straight from the element, both separators are always present, and
/// a missing part is empty — `"AXWindow/AXStandardWindow/"`, `"AXGroup//NI.Kontakt.Main"`,
/// `"AXGroup//"`. The parts were chosen because they are what a plugin actually controls:
/// the role and subrole are what the toolkit reports, and `AXIdentifier` is where Qt puts
/// `objectName`, which is the closest thing to a Windows class name the platform has. None
/// of the three is ever localised, which the title and role *description* both are.
///
/// A module matches it with the same `string.match` it already uses — `"^AXWindow/"`,
/// `"/NI%.Kontakt"` — and `element_dump` prints exactly this string, so an author reading a dump
/// can paste what they see. Never empty: an element with no role at all still yields `"//"`,
/// which is a pattern that can be written and matched rather than a nil that cannot.
pub(super) fn join_class(s: &Snap) -> String {
    format!("{}/{}/{}", s.role, s.subrole, s.identifier)
}

// ---------------------------------------------------------------------------------------
// Bounded traversal
// ---------------------------------------------------------------------------------------

/// Depth-first pre-order walk, bounded by nodes and by depth. What `visit` returns decides
/// what happens next: [`WalkStep::Descend`] goes into the element's children,
/// [`WalkStep::Skip`] moves on to its siblings, [`WalkStep::Stop`] ends the walk.
///
/// Every traversal in this file goes through here, so the bound cannot be forgotten at one
/// call site — and a bound is not a nicety on this platform: these trees are written by
/// plugin vendors, one of them will be cyclic or merely enormous, and the thread that would
/// pay for it is the one carrying the keyboard. Returns `false` if `visit` stopped the walk.
fn walk(
    el: &AXUIElement,
    depth: i32,
    max_depth: i32,
    budget: &mut Budget,
    visit: &mut impl FnMut(&AXUIElement, &Snap, i32) -> WalkStep,
) -> bool {
    if !budget.spend() {
        // Said out loud, once, and not at trace level. A walk that ran out returns exactly
        // what a walk that finished and found nothing returns, so without this line a plugin
        // whose tree is larger than the bound answers "that element is not here" to every
        // question, for ever, and the overlay simply never activates — with nothing anywhere
        // to distinguish it from a plugin we do not support.
        //
        // Which bound ran out is named, because the two want opposite responses: a node
        // budget reached means raise the count for that walk, a deadline reached means the
        // application is answering too slowly to walk at all and no count will help.
        if !BUDGET_REPORTED.with(Cell::get) {
            BUDGET_REPORTED.with(|c| c.set(true));
            crate::logging::line(
                "macos",
                if budget.ran_out_of_time() {
                    "an accessibility walk ran out of TIME and stopped early — the application \
                     is answering too slowly to walk, so any answer from it is 'not found so \
                     far', not 'not there'. A larger node budget would not help this one."
                } else {
                    "an accessibility walk hit its NODE budget and stopped early — any answer \
                     from it is 'not found so far', not 'not there'. If detection is failing \
                     on a large plugin, this is the first thing to look at."
                },
            );
        }
        return true;
    }
    if depth > max_depth {
        return true;
    }
    let snap = snapshot(el);
    match visit(el, &snap, depth) {
        WalkStep::Stop => return false,
        WalkStep::Skip => return true,
        WalkStep::Descend => {}
    }
    for child in children(el) {
        if !walk(&child, depth + 1, max_depth, budget, visit) {
            return false;
        }
    }
    true
}

enum WalkStep {
    /// Keep going, into this element's children.
    Descend,
    /// Keep going, but not into this element's children.
    Skip,
    /// Stop the walk entirely — the answer has been found.
    Stop,
}

// ---------------------------------------------------------------------------------------
// UIA control types onto AX roles
// ---------------------------------------------------------------------------------------

/// The AX roles that mean what a UIA ControlType id means.
///
/// The ids stay UIA ids on every platform — they are what `host.element.type` hands modules and
/// what every shipped module already passes — so the translation happens here, at the edge,
/// and one id maps to a SET because AX splits distinctions UIA does not (a Windows "Button"
/// is an `AXButton`, an `AXMenuButton` or an `AXPopUpButton` depending on what it opens).
///
/// `None` means the id has no honest macOS counterpart. The caller logs it and matches
/// nothing, so an unported module says so in the log instead of quietly finding nothing.
fn roles_for_type(ctype: i32) -> Option<&'static [&'static str]> {
    Some(match ctype {
        50000 => &["AXButton", "AXMenuButton", "AXPopUpButton", "AXCheckBox", "AXRadioButton"][..],
        50002 => &["AXCheckBox"][..],
        50003 => &["AXComboBox", "AXPopUpButton"][..],
        50004 => &["AXTextField", "AXTextArea", "AXComboBox"][..],
        50005 => &["AXLink"][..],
        50006 => &["AXImage"][..],
        50007 => &["AXRow", "AXCell"][..],
        50008 => &["AXList", "AXTable", "AXOutline"][..],
        // Menu is deliberately AXMenu ALONE. Every macOS application has an AXMenuBar at
        // all times, so including it would make "is a menu open?" — which the overlay asks
        // every 150 ms to decide whether to hand the navigation keys back — answer yes
        // forever, and the overlay would never respond to a key again.
        50009 => &["AXMenu"][..],
        50010 => &["AXMenuBar"][..],
        50011 => &["AXMenuItem", "AXMenuBarItem"][..],
        50012 => &["AXProgressIndicator", "AXBusyIndicator"][..],
        50013 => &["AXRadioButton"][..],
        50014 => &["AXScrollBar"][..],
        50015 => &["AXSlider"][..],
        50016 => &["AXIncrementor"][..],
        50018 => &["AXTabGroup"][..],
        // A Cocoa tab is a radio button with subrole AXTabButton; toolkits that draw their
        // own tabs usually settle for a plain button.
        50019 => &["AXRadioButton", "AXButton"][..],
        50020 => &["AXStaticText", "AXTextField"][..],
        50021 => &["AXToolbar"][..],
        50022 => &["AXHelpTag"][..],
        50023 => &["AXOutline"][..],
        50024 => &["AXRow"][..],
        50025 => &["AXUnknown"][..],
        50026 => &["AXGroup", "AXRadioGroup", "AXSplitGroup"][..],
        50032 => &["AXWindow", "AXSheet", "AXDrawer"][..],
        // Pane is the workhorse: it is what a plugin's own surface reports as, and AX has
        // several containers that all mean "a region that holds other things".
        50033 => &[
            "AXGroup",
            "AXUnknown",
            "AXSplitGroup",
            "AXScrollArea",
            "AXLayoutArea",
            "AXWebArea",
        ][..],
        50036 => &["AXTable"][..],
        50038 => &["AXSplitter"][..],
        _ => return None,
    })
}

/// `roles_for_type`, with the "nobody has ported this yet" note in the log.
fn roles_or_log(ctype: i32) -> Option<&'static [&'static str]> {
    match roles_for_type(ctype) {
        Some(r) => Some(r),
        None => {
            let first = UNMAPPED_REPORTED.with(|s| s.borrow_mut().insert(ctype));
            if first {
                crate::logging::line(
                    "macos",
                    &format!(
                        "UIA control type {ctype} has no accessibility role mapped to it on this \
                         platform — every query for it will find nothing. The module asking for \
                         it needs a macOS variant."
                    ),
                );
            }
            None
        }
    }
}

/// The UIA id a module should pass to reach an element of this role. Used by `dump`, so the
/// numbers printed next to an element are the numbers that will match it again.
fn ctype_for_role(role: &str) -> i32 {
    match role {
        "AXWindow" | "AXSheet" | "AXDrawer" => 50032,
        "AXGroup" | "AXUnknown" | "AXSplitGroup" | "AXScrollArea" | "AXLayoutArea" | "AXWebArea" => {
            50033
        }
        "AXButton" | "AXMenuButton" | "AXPopUpButton" => 50000,
        "AXCheckBox" => 50002,
        "AXRadioButton" => 50013,
        "AXComboBox" => 50003,
        "AXTextField" | "AXTextArea" => 50004,
        "AXStaticText" => 50020,
        "AXMenu" => 50009,
        "AXMenuBar" => 50010,
        "AXMenuItem" | "AXMenuBarItem" => 50011,
        "AXTabGroup" => 50018,
        "AXToolbar" => 50021,
        "AXList" => 50008,
        "AXTable" => 50036,
        "AXOutline" => 50023,
        "AXRow" | "AXCell" => 50007,
        "AXSlider" => 50015,
        "AXImage" => 50006,
        "AXScrollBar" => 50014,
        "AXProgressIndicator" | "AXBusyIndicator" => 50012,
        "AXSplitter" => 50038,
        "AXLink" => 50005,
        _ => 0,
    }
}

fn role_matches(snap: &Snap, roles: &[&str]) -> bool {
    roles.iter().any(|r| *r == snap.role)
}

// ---------------------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------------------

/// The executable's file name for a pid — `"Kontakt 8"`, the counterpart of Windows'
/// `"reaper.exe"`.
///
/// Cached, because the host's own note about `active_window` says this is the expensive part
/// of a call that looks cheap and that every embedded overlay makes on every recheck. Only
/// successful answers are cached: a process that has not finished launching has no
/// executable URL yet, and remembering that emptiness would outlast the reason for it.
/// The application's bundle identifier for a pid — `"com.native-instruments.Kontakt8"`.
///
/// The stable identity on this platform, and the one a matcher should prefer when it wants
/// to be exact: the executable inside a bundle is named by the vendor's build system, so it
/// can differ from the product's name and from what the same product's executable is called
/// on Windows. Cached for the same reason `exe_for_pid` is — both are read on every
/// activation, and a process lookup is not free.
fn bundle_id_for_pid(pid: i32) -> String {
    if pid == 0 {
        return String::new();
    }
    if let Some(hit) = BUNDLE_BY_PID.with(|c| c.borrow().get(&pid).cloned()) {
        return hit;
    }
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return String::new();
    };
    let id = app.bundleIdentifier().map(|s| s.to_string()).unwrap_or_default();
    if !id.is_empty() {
        BUNDLE_BY_PID.with(|c| c.borrow_mut().insert(pid, id.clone()));
    }
    id
}

fn exe_for_pid(pid: i32) -> String {
    if pid == 0 {
        return String::new();
    }
    if let Some(hit) = EXE_BY_PID.with(|c| c.borrow().get(&pid).cloned()) {
        return hit;
    }
    let Some(app) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return String::new();
    };
    let name = app
        .executableURL()
        .and_then(|u| u.path())
        .map(|p| p.to_string())
        .and_then(|p| p.rsplit('/').next().map(|s| s.to_string()))
        .or_else(|| app.localizedName().map(|s| s.to_string()))
        .unwrap_or_default();
    if !name.is_empty() {
        EXE_BY_PID.with(|c| c.borrow_mut().insert(pid, name.clone()));
    }
    name
}

/// Where a window's CONTENT starts, which is not always where the window starts.
///
/// Every authored coordinate in every module is measured from here (AutoHotkey's Client
/// mode), so being wrong by the height of a title bar shifts a whole plugin's controls
/// uniformly downwards — the kind of failure that looks like "the overlay clicks slightly
/// off" rather than like a bug.
///
/// macOS has no `GetClientRect`: an `AXWindow`'s position and size describe the whole frame,
/// title bar included, and nothing reports the content rect directly. So:
///
/// - A window with no title bar — every plugin window worth overlaying, since Qt and JUCE
///   draw their own chrome — reports no window subrole or an unknown one, and its content
///   IS its frame. Nothing is probed and nothing can go wrong.
/// - A standard titled window is asked for its children, and if exactly one of them spans
///   the frame's width and starts a plausible title-bar height below its top, that child is
///   the content view and its rectangle is the client rect.
///
/// The probe costs a children read and a rectangle per child, which is too much to pay on
/// every `active_window` call, so the answer is kept as the four EDGE OFFSETS between the
/// frame and the content and re-applied arithmetically afterwards. Offsets rather than the
/// rectangle itself because they survive the window being moved and resized — the title bar
/// keeps its height — which is the same assumption Windows makes when it reports a client
/// rect at all.
///
/// The derivation is reported once per class so the first Mac session can check it against
/// a ruler rather than discovering it through mis-aimed clicks.
fn content_rect(el: &AXUIElement, snap: &Snap, frame: CGRect, hwnd: isize) -> CGRect {
    if let Some(d) = INSET_BY_HANDLE.with(|c| c.borrow().get(&hwnd).copied()) {
        return apply_inset(frame, d);
    }
    let titled = matches!(
        snap.subrole.as_str(),
        "AXStandardWindow" | "AXDialog" | "AXSystemDialog"
    );
    if !titled || frame.size.width <= 0.0 || frame.size.height <= 0.0 {
        INSET_BY_HANDLE.with(|c| c.borrow_mut().insert(hwnd, (0.0, 0.0, 0.0, 0.0)));
        return frame;
    }
    let mut best: Option<CGRect> = None;
    let mut button_centre: Option<f64> = None;
    // Every child, and one batched read each. This used to stop after 24, and the window's
    // own close/zoom/minimise buttons — the measuring instrument below — are the LAST
    // children in every dump this project holds: 11th of 13 for sforzando, 29th of 32 for
    // Kontakt 7 standalone, 41st of 45 for Kontakt inside REAPER's FX window. The cap walked
    // straight past them on both Kontakt windows, silently: content came back equal to the
    // frame, nothing was remembered (a failure never is), and every authored Kontakt
    // coordinate would have landed one title bar too high — on exactly the large plugin
    // windows the port exists for, and only on those, because a small window keeps its
    // buttons inside the first 24. `snapshot` reads subrole and rect in one call, so the
    // cost is one call per child, once per window handle; `children` already caps at 256.
    for child in children(el) {
        let child_snap = snapshot(&child);
        let Some(r) = child_snap.rect else {
            continue;
        };
        // The window's own buttons, which are the measuring instrument when there is no
        // content view to find. See below.
        if matches!(
            child_snap.subrole.as_str(),
            "AXCloseButton" | "AXMinimizeButton" | "AXZoomButton"
        ) {
            let centre = r.origin.y + r.size.height / 2.0 - frame.origin.y;
            if centre > 0.0 && button_centre.is_none_or(|c| centre < c) {
                button_centre = Some(centre);
            }
            continue;
        }
        let inset = r.origin.y - frame.origin.y;
        let spans = r.size.width >= frame.size.width - 2.0;
        let inside = r.origin.x >= frame.origin.x - 1.0
            && r.origin.y + r.size.height <= frame.origin.y + frame.size.height + 1.0;
        // A title bar is tens of points, never hundreds; anything below the top by more
        // than that is a panel inside the content, not the content itself.
        if spans && inside && inset > 0.5 && inset <= 64.0 {
            if best.is_none_or(|b| r.size.height > b.size.height) {
                best = Some(r);
            }
        }
    }

    // A full-width child that stops short of the bottom is a strip across the top of the
    // content, not the content view — its TOP is where the content starts, and the content
    // runs to the frame's bottom edge, which is what the close-button measure below assumes as
    // well. Kontakt 8 standalone on a Mac is the case: its only full-width child is the 38 pt
    // 'Kontakt Header' group, so the client came back 1010x38 of a 1010x675 frame, and every
    // height-dependent read (bottom-anchored probes, the probe's own OCR checks) saw a strip.
    // Extended rather than rejected, because rejecting falls back to the whole frame on a
    // window with no buttons to measure by, and that would move the origin a title bar up. Every
    // other derivation logged so far already reaches the bottom and is unchanged.
    if let Some(r) = best.as_mut() {
        r.size.height = frame.origin.y + frame.size.height - r.origin.y;
    }

    // No content view — and that is the common case, not the exception.
    //
    // Measured on Sforzando standalone, the first real plugin probed on a Mac: its window's
    // direct children are buttons, a slider and labels, with no container among them. So the
    // search above finds nothing and the client rect falls back to the frame, which puts
    // every authored coordinate exactly one title bar too high. That was measured too — the
    // three read-outs the Windows module targets were found by OCR at dy of +32, +31 and
    // +32, with dx of −4, +7 and −5. One constant, vertical only.
    //
    // The window's own close button is what states that constant. It sits vertically centred
    // in the title bar, so twice its centre offset IS the bar's height: 2 × 14 = 28 points
    // on that machine, against the 31-32 the OCR comparison implied — the remainder being
    // the margin an authored region carries around its glyph, not a disagreement.
    //
    // This matters far beyond one plugin: with it, a module's Windows coordinates land on
    // the same controls on macOS, and the port is one module rather than two.
    if best.is_none() {
        if let Some(centre) = button_centre {
            let bar = (centre * 2.0).clamp(8.0, 64.0);
            best = Some(CGRect::new(
                CGPoint::new(frame.origin.x, frame.origin.y + bar),
                CGSize::new(frame.size.width, frame.size.height - bar),
            ));
        }
    }
    // A measurement is remembered; a failure to measure is not.
    //
    // The probe can come up empty for a reason that will not last — a window asked about in
    // the moment between appearing and laying out its content has no qualifying child yet.
    // Caching that answer would fix the client rect at the whole frame for the life of the
    // window, so every coordinate a module authored against the content would sit a title
    // bar too high, permanently, on that window and not on the identical one opened a second
    // later. Falling back to the frame for this call is right; deciding for ever is not.
    if let Some(r) = best {
        let delta = (
            r.origin.x - frame.origin.x,
            r.origin.y - frame.origin.y,
            r.size.width - frame.size.width,
            r.size.height - frame.size.height,
        );
        INSET_BY_HANDLE.with(|c| c.borrow_mut().insert(hwnd, delta));
    }
    match best {
        Some(r) => {
            let class = join_class(snap);
            let first = INSET_REPORTED.with(|s| s.borrow_mut().insert(class.clone()));
            if first {
                crate::logging::line(
                    "macos",
                    &format!(
                        "window class {class}: content starts {:.0}pt below the frame top and is \
                         {:.0}x{:.0}pt of a {:.0}x{:.0}pt frame — module coordinates are measured \
                         from there",
                        r.origin.y - frame.origin.y,
                        r.size.width,
                        r.size.height,
                        frame.size.width,
                        frame.size.height
                    ),
                );
            }
            r
        }
        None => frame,
    }
}

fn apply_inset(frame: CGRect, d: (f64, f64, f64, f64)) -> CGRect {
    CGRect::new(
        CGPoint::new(frame.origin.x + d.0, frame.origin.y + d.1),
        CGSize::new(frame.size.width + d.2, frame.size.height + d.3),
    )
}

/// Builds the host's view of a window from its element.
///
/// `require_title` is the enumerate-versus-active distinction: the window LIST and the
/// activation stream drop untitled windows because they are not matchable yet, while
/// `active_window` keeps them, because a focused window is always relevant — Komplete
/// Kontrol's Save-preset dialog carries no title at all, and dropping it would leave the
/// module with no origin to match against.
fn win_info(el: CFRetained<AXUIElement>, require_title: bool) -> Option<WinInfo> {
    // Local, not a round trip: the element carries its owner. Wanted before the reads rather
    // than after them, so the quarantine can be consulted between them.
    let pid = element_pid(&el);
    let snap = snapshot(&el);
    let title = snap.title.clone();
    if require_title && title.is_empty() {
        return None;
    }
    // The snapshot may have BEEN the read that timed out — it is a batch, and its
    // individual-attribute fall-back is seven more reads after that. Asking this application
    // anything else now pays the messaging timeout a second time inside one `win_info`, which
    // is how a bounded call still costs two seconds. `attribute` has already written the
    // quarantine down by the time we get here; this is the first place that reads it back.
    if skip_busy(pid, "win_info") {
        return None;
    }
    // Minimised is this platform's "not visible": the window still exists and still answers,
    // it is simply not on screen, and its geometry is stale.
    if attribute_bool(&el, a_minimized()) == Some(true) {
        return None;
    }
    let frame = snap.rect?;
    let class = join_class(&snap);
    // Interned before the content rect is worked out, because that answer is remembered per
    // handle: the handle is the only thing that identifies this window across calls.
    let hwnd = handles::intern(el.clone(), pid, 0);
    let client = content_rect(&el, &snap, frame, hwnd);
    let (x, y, w, h) = rect_i32(frame);
    let (cx, cy, cw, ch) = rect_i32(client);
    Some(WinInfo {
        hwnd,
        title,
        class,
        pid: pid as u32,
        exe: exe_for_pid(pid),
        bundle_id: bundle_id_for_pid(pid),
        x,
        y,
        w,
        h,
        client_x: cx,
        client_y: cy,
        client_w: cw,
        client_h: ch,
    })
}

/// Is this window the one a click at the screen point would land in?
///
/// Asked of the window server the way a click asks it: `NSWindow`'s hit-test names the
/// frontmost window that would receive a mouse-down at the point, across every application,
/// and skips windows that let clicks through. That last part is why it is this and not
/// `CGWindowList`, which answers what is DRAWN there: a screen reader's cursor ring is
/// drawn over the very control being operated and lets clicks pass, so the drawn-there
/// answer would have refused essentially every press on the machines this exists for.
///
/// AppKit's screen coordinates start at the bottom-left of the primary display, so y is
/// flipped against that display's height in points; x is shared.
///
/// Three answers, and the third is the one that matters. Ours: `Some(true)`. Somebody
/// else's — named in the log with its owner, so a refused press can be explained —
/// `Some(false)`. And `None` wherever the question could not be put: no pairing between
/// the accessibility window and a `CGWindowID` (see `window_id`), no window at the point,
/// a window of VoiceOver's own that it did not mark click-through, or one of this
/// application's (the announcement window sits over the plugin). A wrong "no" costs the
/// user a press that would have worked, which is exactly what every click did before this
/// existed; a wrong "yes" is the state before it.
///
/// Main thread only, which the pump is; anywhere else it answers `None` rather than
/// touching AppKit from the wrong thread.
pub(super) fn window_owns_point(hwnd: isize, x: i32, y: i32) -> Option<bool> {
    let mtm = objc2::MainThreadMarker::new()?;
    let ours = window_id(hwnd);
    if ours == 0 {
        return None;
    }
    let (_, height) = super::capture::screen_size();
    if height <= 0 {
        return None;
    }
    let point = CGPoint::new(x as f64, (height - y) as f64);
    let hit = NSWindow::windowNumberAtPoint_belowWindowWithWindowNumber(point, 0, mtm);
    if hit <= 0 {
        return None;
    }
    if hit as u32 == ours {
        return Some(true);
    }
    let Some((pid, layer)) = window_owner(hit as u32) else {
        return None;
    };
    if pid == std::process::id() as i32 || is_voiceover(pid) {
        return None;
    }
    // One line per couple of seconds: a hotspot asks before every click, and a window
    // that stays in the way stays in the way for every one of them.
    thread_local! {
        static LAST: Cell<Option<Instant>> = const { Cell::new(None) };
    }
    let due = LAST.with(|l| l.get().is_none_or(|t| t.elapsed().as_secs() >= 2));
    if due {
        LAST.with(|l| l.set(Some(Instant::now())));
        crate::logging::line(
            "macos",
            &format!(
                "the point {x},{y} is under window {hit} of {} (pid {pid}, layer {layer}), not \
                 under window {hwnd} — a click there would land in that application",
                exe_for_pid(pid)
            ),
        );
    }
    Some(false)
}

/// Who owns a window number, and at what level. From the on-screen list; `None` for a
/// number the list does not carry.
fn window_owner(number: u32) -> Option<(i32, i32)> {
    let list = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        0,
    )?;
    // SAFETY: CGWindowListCopyWindowInfo is documented to return an array of dictionaries.
    let typed: &CFArray<CFDictionary> = unsafe { list.cast_unchecked::<CFDictionary>() };
    for i in 0..typed.len().min(512) {
        let Some(dict) = typed.get(i) else { continue };
        if dict_i64(&dict, unsafe { kCGWindowNumber }).unwrap_or(0) as u32 != number {
            continue;
        }
        let pid = dict_i64(&dict, unsafe { kCGWindowOwnerPID }).unwrap_or(0) as i32;
        let layer = dict_i64(&dict, unsafe { kCGWindowLayer }).unwrap_or(0) as i32;
        return Some((pid, layer));
    }
    None
}

/// VoiceOver's process, by bundle id — its cursor ring and caption panel are windows, and
/// a press must never be refused on their account.
fn is_voiceover(pid: i32) -> bool {
    let id = NSString::from_str("com.apple.VoiceOver");
    NSRunningApplication::runningApplicationsWithBundleIdentifier(&id)
        .iter()
        .any(|a| a.processIdentifier() == pid)
}

/// Every on-screen window a process owns, from the window server.
///
/// This is the list the accessibility tree does not have. A popup menu drawn as a window of
/// its own — an `NSMenu` at level 101, or a borderless panel a toolkit puts up for a
/// self-drawn menu — is in here from the moment it appears until it goes, whether or not
/// the application posts a notification about it or exposes it as an `AXMenu`. The overlay
/// runtime's menu watch compares this list against the one it took before clicking the
/// control that opens the menu; a window that is there now and was not then is the menu.
///
/// One system-wide call, no message to the application, so it is safe on the tick that
/// carries the keyboard. Names are deliberately not read: without Screen Recording they are
/// withheld, and nothing here needs them.
pub fn windows_of(pid: u32) -> Vec<crate::backend::WindowSpot> {
    let mut out = Vec::new();
    let Some(list) = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        0,
    ) else {
        return out;
    };
    // SAFETY: CGWindowListCopyWindowInfo is documented to return an array of dictionaries.
    let typed: &CFArray<CFDictionary> = unsafe { list.cast_unchecked::<CFDictionary>() };
    for i in 0..typed.len().min(512) {
        let Some(dict) = typed.get(i) else { continue };
        if dict_i64(&dict, unsafe { kCGWindowOwnerPID }).unwrap_or(-1) != pid as i64 {
            continue;
        }
        let Some(b) = dict_rect(&dict) else { continue };
        out.push(crate::backend::WindowSpot {
            id: dict_i64(&dict, unsafe { kCGWindowNumber }).unwrap_or(0) as u64,
            layer: dict_i64(&dict, unsafe { kCGWindowLayer }).unwrap_or(0) as i32,
            class: String::new(),
            x: b.origin.x as i32,
            y: b.origin.y as i32,
            w: b.size.width as i32,
            h: b.size.height as i32,
        });
    }
    out
}

/// How long one application may take to list its windows when NOBODY asked for that
/// application in particular.
///
/// The process-wide timeout is a second, and the unfiltered listing asks every running
/// application in turn, so one process that never answers costs the whole second on the
/// thread that carries the event tap — measured on the third Mac session at 2.0-2.3 s of a
/// 3 s stall per probe press, four presses out of four, each of them switching the tap off.
/// A quarter second is what the messaging timeout used to be before a slow-but-alive plugin
/// on a 2015 Air needed the full second, and that plugin is exactly the case this bound does
/// NOT touch: an application a matcher names is asked through `enumerate_windows_of`, at the
/// full timeout. Only the applications nobody asked about get the short one.
const UNASKED_APP_TIMEOUT: f32 = 0.25;

/// An application that takes this long to list its windows is named in the log. Not the
/// timeout — a slow answer is the shape of the stall, and a second of it is invisible in a
/// line that only says the whole listing took two.
const SLOW_APP_MS: u128 = 100;

/// Every running application that could own a window, without asking any of them anything.
///
/// Local: the workspace's list, plus the two per-pid lookups that are already cached on this
/// thread. Background-only processes (`Prohibited` activation policy — helpers, agents,
/// XPC services) are left out because they cannot have a window, and asking each of them
/// for `AXWindows` is a cross-process call apiece; there are more of them than of everything
/// else together.
pub fn running_apps() -> Vec<crate::backend::AppInfo> {
    let mut out = Vec::new();
    let apps = NSWorkspace::sharedWorkspace().runningApplications();
    for app in apps.iter().take(512) {
        let pid = app.processIdentifier();
        if pid <= 0 || app.activationPolicy() == NSApplicationActivationPolicy::Prohibited {
            continue;
        }
        out.push(crate::backend::AppInfo {
            pid: pid as u32,
            exe: exe_for_pid(pid),
            bundle_id: bundle_id_for_pid(pid),
        });
    }
    out
}

/// Visible windows with a non-empty title, across every application.
///
/// The cold path: `host.window.list()` with no filter, and `find`/`findAll` for a matcher
/// that names no application. Bounded per application rather than per listing — see
/// [`UNASKED_APP_TIMEOUT`].
///
/// Hidden applications (Command-H) are asked like any other. They were skipped for a day,
/// and that made the two listings disagree: `running_apps` keeps them, so a matcher that
/// names one found its windows while a matcher that did not could not — and a module that
/// finds a hidden DAW's plugin window and focuses it is exactly how that DAW comes back.
pub fn enumerate_windows() -> Vec<WinInfo> {
    let mut pids = Vec::new();
    let apps = NSWorkspace::sharedWorkspace().runningApplications();
    for app in apps.iter().take(512) {
        let pid = app.processIdentifier();
        if pid <= 0 || app.activationPolicy() == NSApplicationActivationPolicy::Prohibited {
            continue;
        }
        pids.push(pid as u32);
    }
    enumerate_windows_in(&pids, UNASKED_APP_TIMEOUT, "enumerate_windows")
}

/// The windows of the applications a matcher named, at the full messaging timeout.
///
/// These are the applications the caller is waiting for an answer about, so a slow one is
/// given the whole second the process-wide setting allows — the 2015 Air's sforzando never
/// answered inside a quarter of one, and a listing that left it out would have made the
/// shortcut say "could not find a plugin window" about a window that was there.
pub fn enumerate_windows_of(pids: &[u32]) -> Vec<WinInfo> {
    enumerate_windows_in(pids, MESSAGING_TIMEOUT, "enumerate_windows_of")
}

fn enumerate_windows_in(pids: &[u32], timeout: f32, what: &str) -> Vec<WinInfo> {
    let t = Instant::now();
    let mut out = Vec::new();
    let mut asked = 0usize;
    for &pid in pids.iter().take(256) {
        let pid = pid as i32;
        // The penalty box applies here too. The read below already RECORDS a timeout
        // (`attribute` marks the application busy on CannotComplete); this loop simply never
        // looked before asking, while `frontmost_window_element` has since the quarantine
        // was introduced.
        //
        // Logged at line level, not trace, because this is what makes a window vanish from
        // `find` for five seconds, and a module that then reports it could not find a window
        // is saying something the log has to be able to explain. Named, not numbered: a bare
        // pid is a number the reader has to resolve themselves, and `exe_for_pid` is already
        // cached on this thread.
        if is_busy(pid) {
            crate::logging::line(
                "macos",
                &format!(
                    "{what}: not asking {} (pid {pid}) for its windows, it did not answer a \
                     moment ago",
                    exe_for_pid(pid)
                ),
            );
            continue;
        }
        let app_el = app_element(pid);
        // Per element, and this element is created for this call and not remembered — the
        // window elements it hands back keep the process-wide timeout, because THOSE are
        // interned and asked again later, and a remembered window that answers in half a
        // second must not read as "did not answer" for the rest of the session.
        let _ = unsafe { app_el.set_messaging_timeout(timeout) };
        let started = Instant::now();
        asked += 1;
        // Not through `attribute`, which on a timeout writes the five-second quarantine
        // that every other path consults. That is right when the bound was the full second
        // — the application really is not answering — and wrong at the short one: a plugin
        // that lists its windows in half a second, asked here because nobody named it,
        // would have been marked busy, and for the next five seconds `active_window` would
        // serve its remembered window, `find` with its name would refuse to ask it, and the
        // observer retry would wait — all for a deadline it was never expected to meet.
        let read = attribute_checked(&app_el, a_windows());
        let v = match read {
            Ok(Some(v)) => v,
            Ok(None) => {
                name_if_slow(pid, started, what, 0);
                continue; // no windows, or a process with no accessibility surface at all
            }
            Err(AXError::CannotComplete) if timeout < MESSAGING_TIMEOUT => {
                crate::logging::line(
                    "macos",
                    &format!(
                        "{what}: {} (pid {pid}) did not list its windows within {} ms — left \
                         out of this listing, not quarantined; a matcher that names it is \
                         asked at the full timeout",
                        exe_for_pid(pid),
                        (timeout * 1000.0) as u32
                    ),
                );
                continue;
            }
            Err(err) => {
                if err == AXError::CannotComplete {
                    note_busy(pid);
                }
                note_error(err, "AXWindows");
                name_if_slow(pid, started, what, 0);
                continue;
            }
        };
        let Some(arr) = v.downcast_ref::<CFArray>() else {
            continue;
        };
        // SAFETY: AXWindows is documented as an array of AXUIElementRef.
        let typed: &CFArray<AXUIElement> = unsafe { arr.cast_unchecked::<AXUIElement>() };
        let mut listed = 0usize;
        for i in 0..typed.len().min(64) {
            if let Some(w) = typed.get(i) {
                if let Some(info) = win_info(w, true) {
                    out.push(info);
                    listed += 1;
                }
            }
        }
        name_if_slow(pid, started, what, listed);
    }
    let ms = t.elapsed().as_millis();
    crate::logging::trace("macos", || {
        format!("{what}: {} window(s) from {asked} application(s) in {ms} ms", out.len())
    });
    if ms > SLOW_MS {
        crate::logging::line(
            "macos",
            &format!(
                "{what} blocked the pump for {ms} ms listing {asked} application(s); any that \
                 took {SLOW_APP_MS} ms or more is named above"
            ),
        );
    }
    out
}

/// An attribute read that reports the failure instead of filing it.
///
/// The funnel (`attribute`) classifies every failure into the log and quarantines the
/// application on a timeout, and for every ordinary caller that is right. The window
/// listing is not ordinary: it asks with a bound of its own, and what a miss at that bound
/// means is the caller's to decide. `Ok(None)` is the API's own "no value" and the two
/// absence errors the funnel treats the same way.
fn attribute_checked(
    el: &AXUIElement,
    name: &CFString,
) -> Result<Option<CFRetained<CFType>>, AXError> {
    let mut raw: *const CFType = core::ptr::null();
    let err = unsafe { el.copy_attribute_value(name, NonNull::from(&mut raw)) };
    match err {
        AXError::Success | AXError::NoValue | AXError::AttributeUnsupported => {
            Ok(NonNull::new(raw.cast_mut()).map(|p| unsafe { CFRetained::from_raw(p) }))
        }
        other => Err(other),
    }
}

/// One line per application that took its time, so a two-second listing says WHICH process
/// the two seconds went to. The stall line alone never did, and the timeout line names an
/// attribute, not a process.
fn name_if_slow(pid: i32, started: Instant, what: &str, listed: usize) {
    let ms = started.elapsed().as_millis();
    if ms >= SLOW_APP_MS {
        crate::logging::line(
            "macos",
            &format!(
                "{what}: {} (pid {pid}) took {ms} ms to list its windows ({listed} listed)",
                exe_for_pid(pid)
            ),
        );
    }
}

/// Which application is in front. Asked of the workspace, which is local, cheap, and still
/// answers when the application itself has stopped talking to us.
fn frontmost_pid() -> Option<i32> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    (pid > 0).then_some(pid)
}

/// The answer to "which window is in front", and how much it is worth.
enum Front {
    /// The application answered. This element is live and may be asked further questions.
    Fresh(CFRetained<AXUIElement>),
    /// It did not answer. Nothing more may be asked of it — every question would pay the
    /// messaging timeout over again — so the caller serves what it remembers, or nothing.
    Silent,
    /// It answered, and the answer is that it has no window. To be believed.
    NoWindow,
}

/// The frontmost window of one application, or why there isn't one.
fn front_window_of(pid: i32) -> Front {
    if is_busy(pid) {
        crate::logging::trace("macos", || {
            format!("skipping pid {pid}: it did not answer a moment ago")
        });
        return Front::Silent;
    }
    // How long the answer actually took, when it came at all.
    //
    // The timeout was set from an assumption about healthy applications and cost a tester his
    // whole session; the replacement is a guess too, and this is what turns the next one into
    // a measurement. Only slow SUCCESSES are reported — a fast answer is not news, and a
    // failure already writes its own line.
    //
    // That "only successes" is load-bearing, and the restructure that introduced this
    // function nearly lost it: the fall-back chain used to end in `?`, which returned before
    // this line, and moving the search into a function that returns `Option` put the log
    // ahead of the test. A read that timed out would then have been written down as
    // "answered after 1002 ms" — the messaging timeout reported as a response time, on the
    // one line whose whole purpose is to measure what a real answer costs. The application
    // not answering must never be logged as it answering.
    let started = Instant::now();
    let app_el = app_element(pid);
    let found = window_after_fallbacks(&app_el, pid);
    let ms = started.elapsed().as_millis();
    match found {
        Some(el) => {
            if ms >= 100 {
                crate::logging::line(
                    "macos",
                    &format!("pid {pid} answered the frontmost-window question after {ms} ms"),
                );
            }
            Front::Fresh(el)
        }
        // Whether the application went quiet is what tells "it has no window" apart from "it
        // did not say". `attribute` quarantines a pid the moment a read comes back
        // `CannotComplete`, so the quarantine IS that answer, already recorded.
        None if is_busy(pid) => Front::Silent,
        None => Front::NoWindow,
    }
}

/// `AXFocusedWindow`, then `AXMainWindow`, then the first of `AXWindows` — and the busy check
/// between them, which is the point of this function existing.
///
/// `AXFocusedWindow` is the window that would receive a keystroke, which is the question the
/// host is really asking. The two fall-backs are there for an application that ANSWERS and
/// has no focused window: a dialog that is foreground before it has settled is exactly the
/// case they were written for.
///
/// They were also being paid by applications that do not answer at all, which is a different
/// thing needing the opposite treatment — and the tester's log shows what it cost. This
/// question stalled his pump **fourteen times**, worst case 2605 ms, the worst of them while
/// he was simply using the plug-in; the shape in the log is `timed out reading
/// AXFocusedWindow after 1s` followed by an answer at 2455 ms. One timeout, then two more
/// charged to an application we had already written down as not answering.
///
/// The check between the reads separates the two exactly, and needs nothing new to do it: an
/// absent value does not set the quarantine and still falls through, a timed-out read does
/// and stops here.
fn window_after_fallbacks(app_el: &AXUIElement, pid: i32) -> Option<CFRetained<AXUIElement>> {
    if let Some(w) = attribute_element(app_el, a_focused_window()) {
        return Some(w);
    }
    if is_busy(pid) {
        return None;
    }
    if let Some(w) = attribute_element(app_el, a_main_window()) {
        return Some(w);
    }
    if is_busy(pid) {
        return None;
    }
    let v = attribute(app_el, a_windows())?;
    let arr = v.downcast_ref::<CFArray>()?;
    // SAFETY: AXWindows is an array of AXUIElementRef.
    let typed: &CFArray<AXUIElement> = unsafe { arr.cast_unchecked::<AXUIElement>() };
    typed.get(0)
}

/// The element of the frontmost application's frontmost window.
///
/// Deliberately without the remembered fall-back the other two callers have: this hands back
/// a LIVE element, and both of its callers immediately snapshot it or walk it upwards. A
/// remembered element would move the timeouts one line down rather than remove them.
pub(super) fn frontmost_window_element() -> Option<(CFRetained<AXUIElement>, i32)> {
    let pid = frontmost_pid()?;
    match front_window_of(pid) {
        Front::Fresh(el) => Some((el, pid)),
        Front::Silent | Front::NoWindow => None,
    }
}

/// The frontmost window. A title is NOT required — an untitled dialog is exactly the case
/// this has to answer for.
pub fn active_window() -> Option<WinInfo> {
    let t = Instant::now();
    let pid = frontmost_pid()?;
    // Whether the answer came from the application or out of memory. Carried into the trace
    // because a remembered answer is fast, and a fast line that does not say so reads as a
    // healthy application — which is the opposite of what it means.
    let mut from_memory = false;
    let info = match front_window_of(pid) {
        Front::Fresh(el) => {
            // Interned BEFORE the snapshot, not after. It costs no call into the application
            // — the handle table is local — and it is the only way to know afterwards WHICH
            // window the application named, in the case where it named one and then stopped
            // describing it. `win_info` interns the same element again and gets the same
            // number back.
            let named = handles::intern(el.clone(), pid, 0);
            match win_info(el, false) {
                Some(w) => {
                    remember_front(pid, w.hwnd, Some(w.clone()));
                    // The key tap holds a NUMBER rather than asking this question itself —
                    // asking it inside the tap callback is the one cross-process call that
                    // must never happen there. That number is told to it by two
                    // notifications, and a notification that arrives while the application
                    // is quarantined tells it nothing, so it can go stale and stay stale:
                    // no further notification is due while the user works inside one plugin.
                    // This is the same fact, resolved fresh, on the tick, for free. It costs
                    // one atomic store and it is what makes the gate self-heal.
                    //
                    // Only on this arm on purpose: a REMEMBERED window is an answer about
                    // geometry, not about which window has the keyboard now, and writing it
                    // here would arm the gate from something nobody has confirmed.
                    super::tap::note_foreground(w.hwnd);
                    Some(w)
                }
                // `win_info` reads a dozen attributes of its own and any of them can be the
                // one that times out, so the same rule applies a second time: the quarantine
                // is what separates "no window" from "no answer".
                None if is_busy(pid) => {
                    let got = remembered_info(pid, Some(named));
                    from_memory = got.is_some();
                    got
                }
                None => {
                    forget_front(pid);
                    None
                }
            }
        }
        // Nothing was named here — the application was not asked at all — so there is no
        // identity to check the memory against, and the memory is the best that exists.
        Front::Silent => {
            let got = remembered_info(pid, None);
            from_memory = got.is_some();
            got
        }
        Front::NoWindow => {
            forget_front(pid);
            None
        }
    };
    let ms = t.elapsed().as_millis();
    crate::logging::trace("macos", || {
        let how = if from_memory { " (remembered — the application is not answering)" } else { "" };
        match &info {
            Some(w) => format!(
                "active_window: id {} '{}' class {} at {},{} {}x{} client {},{} {}x{} in \
                 {ms} ms{how}",
                w.hwnd, w.title, w.class, w.x, w.y, w.w, w.h, w.client_x, w.client_y,
                w.client_w, w.client_h
            ),
            None => format!("active_window: none in {ms} ms{how}"),
        }
    });
    if ms > SLOW_MS {
        crate::logging::line("macos", &format!("active_window blocked the pump for {ms} ms"));
    }
    info
}

/// One window by handle. `require_title` is the enumerate-versus-active distinction above.
pub fn window_info(handle: isize, require_title: bool) -> Option<WinInfo> {
    let entry = handles::get(handle)?;
    win_info(entry.element, require_title)
}

/// The handle of the frontmost window. Used for the key-scope snapshot.
///
/// Returns the interned handle without building a whole `WinInfo`, because the caller wants
/// identity and nothing else — but it does intern, so the number it hands back is the same
/// one `active_window` reports and the comparison at key-press time is meaningful.
///
/// **`None` is not zero.** `Some(0)` means the application answered and has no window;
/// `None` means it did not answer at all, and the two lead to opposite actions in the caller.
/// This is the third time the same conflation has been found in this codebase — it cost the
/// macOS tester his startup announcement as `via_screen_reader`, and it is recorded a second
/// time against `note_foreground` in TODO.md, which is what this returns for. A caller that
/// writes a fabricated 0 into the key tap's idea of what is in front closes the scope gate
/// for EVERY hotkey an overlay owns, and nothing reopens it until the next notification —
/// which, if the user stays inside the plugin, may be a long time. Reasoned from the code
/// rather than observed: the log line that would show it ("a key the overlay had claimed
/// reached the application instead") has not been seen in a tester's log.
pub fn foreground_window_id() -> Option<isize> {
    let Some(pid) = frontmost_pid() else {
        return Some(0);
    };
    match front_window_of(pid) {
        Front::Fresh(el) => {
            let handle = handles::intern(el, pid, 0);
            remember_front(pid, handle, None);
            Some(handle)
        }
        // Deliberately NOT served from memory, unlike `active_window`. Memory can answer
        // "which window is in front", which is what the pump asks on every tick. It cannot
        // answer what every caller of THIS function asks, which is a form of "what just
        // changed": `watch` calls it on an `AXFocusedWindowChanged` notification, where the
        // remembered window is by definition the one that is no longer focused, and on an
        // application activation, where memory of that pid may be minutes old;
        // `tap::set_key_scope` calls it to re-check a pin it already disagrees with, where
        // memory would repeat the number that caused the disagreement and the log would
        // report it as a fresh answer. `set_key_scope` then FREEZES what this returns into
        // the hotkey scope for as long as the overlay is active, so a stale answer here does
        // not expire with the quarantine the way a stale `active_window` does.
        //
        // And a handle handed back here does not stop at the caller. `watch::on_activated`
        // pushes it into the activation queue, and `queue::drain` resolves it on the pump
        // thread with `window_info` — which is how a remembered handle would send the pump
        // straight back into the application that had just failed to answer, paying the
        // timeout in `snapshot` and again in its individual-attribute fall-back. That path
        // is gated now (`win_info` consults the quarantine), so this is belt and braces
        // rather than the only thing standing in the way; the reason above is the one that
        // decides it.
        //
        // Returning 0 is not right either: it means "no window" where the truth is "not
        // known", and the difference is not cosmetic — 0 makes every scoped hotkey stop
        // matching until the next notification arrives, which may be a long time. That is a
        // pre-existing defect this change deliberately does not touch, and it is recorded in
        // TODO.md: the fix is to let `note_foreground` say "unknown", not to guess a window.
        Front::Silent => None,
        Front::NoWindow => {
            forget_front(pid);
            Some(0)
        }
    }
}

/// The snapshot this application last gave, served because it has stopped answering.
///
/// `named` is the window the application just told us is in front, where it got that far
/// before going quiet. A remembered snapshot of a DIFFERENT window is not an old answer, it
/// is a wrong one: what the caller reads out of it is a rectangle, and an overlay's clicks
/// are placed inside that rectangle. A plug-in that opens a dialog is exactly the case —
/// the new window is named immediately and is the likeliest thing to stall while it comes
/// up, and the answer would otherwise be the main window's geometry with the dialog sitting
/// on top of it.
///
/// `None` where nothing is remembered, or where the wrong thing is. Inventing a window is
/// worse than admitting there is none.
fn remembered_info(pid: i32, named: Option<isize>) -> Option<WinInfo> {
    // The borrow ends with the call: the two refusals below take a mutable one.
    let recalled = LAST_FRONT.with(|m| m.borrow().recall(pid, named, Instant::now()));
    match recalled {
        Recall::Serve { info, age } => {
            note_remembered(pid, age);
            Some(info)
        }
        Recall::Mismatch { remembered } => {
            crate::logging::line(
                "macos",
                &format!(
                    "active_window: {} (pid {pid}) named window {} and then stopped \
                     describing it; the one remembered is {remembered} — reporting no active \
                     window rather than the wrong one",
                    exe_for_pid(pid),
                    named.unwrap_or(0)
                ),
            );
            // Dropped as well as declined. Without this, every `Front::Silent` tick for the
            // rest of the quarantine would go on serving the window just ruled out.
            forget_front(pid);
            None
        }
        Recall::TooOld { age } => {
            crate::logging::line(
                "macos",
                &format!(
                    "active_window: {} (pid {pid}) has not described its window for {} s — \
                     that is past what this will stand behind, so it reports no active window \
                     rather than a rectangle nobody has confirmed",
                    exe_for_pid(pid),
                    age.as_secs()
                ),
            );
            // Nothing will make it younger, so it goes: the line above is said once rather
            // than on every tick for as long as the application stays wedged.
            forget_front(pid);
            None
        }
        Recall::Nothing => None,
    }
}

/// Brings a window to the front and gives it the keyboard.
///
/// Two calls, and both are needed. Raising a window inside an application that is not
/// frontmost leaves it exactly where it was, behind whatever is; activating an application
/// without raising the window brings its *other* windows forward instead. Window managers on
/// this platform all do both, in this order.
///
/// The action name is written out rather than taken from a constant: `kAXRaiseAction` is not
/// among the bound accessibility constants, and it is a documented string. The same is true
/// of the two menu notifications this backend already listens for.
///
/// Reports what happened rather than assuming. macOS can decline either half — an
/// application that is not scriptable by us, a window that has gone away between being
/// listed and being asked — and a caller that believes it moved the focus when it did not
/// would tell a blind user they are somewhere they are not.
pub fn focus_window(handle: isize) -> bool {
    let Some(entry) = handles::get(handle) else {
        crate::logging::line("macos", &format!("cannot focus window {handle}: no such handle"));
        return false;
    };
    let raise = CFString::from_static_str("AXRaise");
    // SAFETY: the element is alive for the duration of the call and the action name is a
    // static string; `perform_action` is a plain cross-process call.
    let raised = unsafe { entry.element.perform_action(&raise) } == AXError::Success;

    let activated = NSRunningApplication::runningApplicationWithProcessIdentifier(entry.pid)
        .is_some_and(|app| {
            #[allow(deprecated)]
            let opts = NSApplicationActivationOptions::ActivateAllWindows
                | NSApplicationActivationOptions::ActivateIgnoringOtherApps;
            app.activateWithOptions(opts)
        });

    // A third thing, and the tester's session is why it is here. Raising the window and
    // activating the application both reported success every single time — the log has no
    // refusal in it — and yet Tab still went to the host's own controls. Bringing a window
    // forward is not the same as moving the KEYBOARD into it: the application's focus stays
    // wherever it was, which in REAPER is its FX list, and an overlay that gates on "the
    // keyboard is in the plugin" then correctly declines to do anything. He found the
    // workaround himself: press Shift+Tab once and it starts working.
    //
    // Asking the window to be the focused one is the polite way to say the same thing. It
    // may not be enough — whether an application moves its internal focus in response is the
    // application's business — which is why the chain depth is measured afterwards and
    // reported either way. That number is the difference between "we asked and it worked"
    // and "we asked and this host ignores it", and nobody here can produce it.
    let focused =
        unsafe { entry.element.set_attribute_value(a_focused(), CFBoolean::new(true).as_ref()) }
            == AXError::Success;

    let depth = window_focus_chain().len();
    crate::logging::line(
        "macos",
        &format!(
            "focusing window {handle} (pid {}): raise {}, activation {}, focused-attribute \
             {} — the focus chain is now {depth} deep{}",
            entry.pid,
            if raised { "yes" } else { "REFUSED" },
            if activated { "yes" } else { "REFUSED" },
            if focused { "yes" } else { "REFUSED" },
            // Depth only, and said as depth. The chain is read straight after an
            // activation that is asynchronous, so it can still describe another
            // application — the caller that needs to know checks the last link's identity
            // against the window it asked for, and this log line is not that caller.
            match depth {
                0 => ", and where the keyboard is could not be read",
                1 => ", and the focused element is a window",
                _ => ", so the focus is on something inside whatever is in front",
            }
        ),
    );
    raised || activated
}

/// The `CGWindowID` behind a window handle, or 0 if it could not be paired.
///
/// The two halves of "a window" come from different APIs on this platform and neither can
/// produce the other through anything public: an `AXUIElement` can be read, focused and
/// walked but knows nothing about where its pixels are, and a `CGWindowID` is the opposite.
/// `_AXUIElementGetWindow` pairs them directly and is what every window manager on the
/// platform uses; it is private, so this is the documented fallback — match the window list
/// by owning process and by frame — and it is resolved lazily and remembered, because the
/// window list is a whole-system query and no caller wants to pay for it on a focus change.
pub(super) fn window_id(handle: isize) -> u32 {
    let Some(entry) = handles::get(handle) else {
        return 0;
    };
    if entry.window_id != 0 {
        return entry.window_id;
    }
    let Some(frame) = element_rect_cg(&entry.element) else {
        return 0;
    };
    let Some(list) = CGWindowListCopyWindowInfo(
        CGWindowListOption::OptionOnScreenOnly | CGWindowListOption::ExcludeDesktopElements,
        0,
    ) else {
        crate::logging::line(
            "macos",
            "the window list came back empty — without Screen Recording macOS hides other \
             applications' windows, and capture will return the wallpaper",
        );
        return 0;
    };
    // SAFETY: CGWindowListCopyWindowInfo is documented to return an array of dictionaries.
    let typed: &CFArray<CFDictionary> =
        unsafe { list.cast_unchecked::<CFDictionary>() };
    let mut found = 0u32;
    for i in 0..typed.len().min(512) {
        let Some(dict) = typed.get(i) else { continue };
        let owner = dict_i64(&dict, unsafe { kCGWindowOwnerPID }).unwrap_or(-1);
        if owner as i32 != entry.pid {
            continue;
        }
        let Some(bounds) = dict_rect(&dict) else { continue };
        if (bounds.origin.x - frame.origin.x).abs() <= 2.0
            && (bounds.origin.y - frame.origin.y).abs() <= 2.0
            && (bounds.size.width - frame.size.width).abs() <= 2.0
            && (bounds.size.height - frame.size.height).abs() <= 2.0
        {
            found = dict_i64(&dict, unsafe { kCGWindowNumber }).unwrap_or(0) as u32;
            break;
        }
    }
    if found != 0 {
        // Re-interning the same element only fills the id in; the handle does not change.
        handles::intern(entry.element, entry.pid, found);
        crate::logging::line(
            "macos",
            &format!("window {handle} paired with CGWindowID {found} by frame and owner"),
        );
    } else {
        crate::logging::line(
            "macos",
            &format!(
                "window {handle} (pid {}) has no window-list entry matching its frame — anything \
                 that needs the window id will fall back to the screen region",
                entry.pid
            ),
        );
    }
    found
}

#[allow(dead_code)]
fn dict_i64(dict: &CFDictionary, key: &CFString) -> Option<i64> {
    let ptr: *const c_void = (key as *const CFString).cast();
    let v = unsafe { dict.value(ptr) };
    if v.is_null() {
        return None;
    }
    // SAFETY: the value is borrowed from the dictionary, which outlives this call.
    let num: &CFType = unsafe { &*(v as *const CFType) };
    num.downcast_ref::<CFNumber>()?.as_i64()
}

#[allow(dead_code)]
fn dict_rect(dict: &CFDictionary) -> Option<CGRect> {
    let ptr: *const c_void = (unsafe { kCGWindowBounds } as *const CFString).cast();
    let v = unsafe { dict.value(ptr) };
    if v.is_null() {
        return None;
    }
    // SAFETY: kCGWindowBounds carries a CFDictionary in CGRect's dictionary representation.
    let bounds: &CFDictionary =
        unsafe { &*(v as *const CFDictionary) };
    let mut out = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(0.0, 0.0));
    let ok = unsafe { CGRectMakeWithDictionaryRepresentation(Some(bounds), &mut out) };
    ok.then_some(out)
}

// ---------------------------------------------------------------------------------------
// Structure
// ---------------------------------------------------------------------------------------

fn control_info(el: CFRetained<AXUIElement>, snap: &Snap, pid: i32) -> Option<ControlInfo> {
    let frame = snap.rect?;
    if frame.size.width <= 0.0 || frame.size.height <= 0.0 {
        return None; // an element with no rectangle is not a surface
    }
    let (x, y, w, h) = rect_i32(frame);
    let class = join_class(snap);
    let hwnd = handles::intern(el.clone(), pid, 0);
    // There is no frame-versus-client distinction below the window: an accessibility
    // element's rectangle is its content, with no border and no title bar of its own. The
    // Windows backend reports the same thing for the borderless windows plugins use —
    // `mod.rs` says so in as many words.
    //
    // A WINDOW is the exception, and this function is handed one: `window_focus_chain` ends
    // its chain with the AXWindow itself. Taking the shortcut there reported the frame as the
    // client rect, so the last link of `host.window.focusChain()` and `host.window.active()`
    // described the same window with a `y` about a title bar apart — and the Windows backend
    // does fill a top-level window's client fields from `ClientToScreen`, so the two
    // platforms disagreed as well.
    //
    // Found by an audit looking for something else entirely. It is not a scale fault: 28
    // points at 1.00 and the same 28 points at 2.00. But it is a coordinate wrong by a
    // constant, which is precisely what a points-versus-pixels fault looks like from the
    // outside, and the next session is the one that finally has a Retina display to test on.
    // Leaving a decoy of that shape in place for a session that cannot be repeated is the
    // expensive choice.
    let client = if snap.role == "AXWindow" {
        rect_i32(content_rect(&el, snap, frame, hwnd))
    } else {
        (x, y, w, h)
    };
    Some(ControlInfo {
        hwnd,
        class,
        x,
        y,
        w,
        h,
        client_x: client.0,
        client_y: client.1,
        client_w: client.2,
        client_h: client.3,
    })
}

/// Is this element a plausible plugin SURFACE rather than a control drawn on one?
///
/// `window_controls` answers "what drawable sub-surfaces exist inside this window" — on
/// Windows, the child HWNDs. Individual buttons and labels are not sub-surfaces and there
/// are hundreds of them; including them would multiply the cost of the hottest walk in the
/// backend by an order of magnitude for answers no module reads.
fn is_surface(role: &str) -> bool {
    matches!(
        role,
        "AXGroup"
            | "AXUnknown"
            | "AXSplitGroup"
            | "AXScrollArea"
            | "AXLayoutArea"
            | "AXWebArea"
            | "AXTabGroup"
            | "AXDrawer"
            | "AXSheet"
            | "AXPopover"
            | "AXToolbar"
            | "AXWindow"
    )
}

/// Every descendant surface inside a window, with what it is and where its content starts.
///
/// The counterpart of `EnumChildWindows`: recursive, not just direct children, and visible
/// only. Modules match `class` against a Luau pattern and then read the rectangle — this is
/// how an overlay finds the plugin embedded in a DAW's window, so the answer has to be
/// stable across calls, which is what interning the handles buys.
pub fn window_controls(hwnd: isize) -> Vec<ControlInfo> {
    let t = Instant::now();
    let Some(entry) = handles::get(hwnd) else {
        crate::logging::trace("macos", || format!("window_controls: {hwnd} is not a live handle"));
        return Vec::new();
    };
    let pid = entry.pid;
    // Six hundred nodes into an application that is not answering is the worst shape this
    // file can take: the walk is bounded by nodes and by depth, and by nothing at all in
    // time. This is the question the overlay runtime asks immediately after `active()`.
    if skip_busy(pid, "window_controls") {
        return Vec::new();
    }
    let mut out: Vec<ControlInfo> = Vec::new();
    let mut budget = Budget::new(CONTROL_NODES, HOT_DEADLINE);
    walk(
        &entry.element,
        0,
        CONTROL_DEPTH,
        &mut budget,
        &mut |el, snap, depth| {
            if out.len() >= CONTROL_MAX {
                return WalkStep::Stop;
            }
            // The window itself is the root, not one of its own controls.
            if depth > 0 && is_surface(&snap.role) {
                if let Some(c) = control_info(el.retain(), snap, pid) {
                    out.push(c);
                }
            }
            // A button holds no surfaces, and asking it for its children is a round trip
            // that can only answer "none". An unfamiliar role is descended into anyway:
            // this platform's plugins invent roles, and the whole point of the walk is to
            // reach what they invented.
            if !is_surface(&snap.role) && ctype_for_role(&snap.role) != 0 {
                return WalkStep::Skip;
            }
            WalkStep::Descend
        },
    );
    let ms = t.elapsed().as_millis();
    // Saying whether that count is all of them — at LINE level when it is not, or when the walk
    // was slow, and traced otherwise.
    //
    // The probe prints this number as "surfaces inside it: N" and SPEAKS it — it is the
    // tester's confirmation that the press landed — and the Kontakt window is the first one
    // that will have any. Four bounds can cut it short and none of them was visible: the
    // 256-surface stop returns `WalkStep::Stop` without touching the budget, the depth cut
    // returns silently, `CHILDREN_MAX` clips a long child list, and the node/time bound is
    // announced only by a once-per-session flag that an earlier walk in the same session will
    // already have spent. So a partial count read exactly like a complete one, which is the
    // same fault `dump()` had and was given its own line for.
    //
    // It used to be written on every call, and this runs on every recheck: 2615 of the fifth
    // session's 16839 lines, all but four of them "neither bound" and under 25 ms. A line that
    // says the ordinary thing thousands of times buries the four that matter.
    let bounded =
        budget.ran_out_of_time() || out.len() >= CONTROL_MAX || budget.nodes_left() <= 0;
    let cut = if budget.ran_out_of_time() {
        " — STOPPED ON TIME, so there may be more"
    } else if out.len() >= CONTROL_MAX {
        " — STOPPED AT THE SURFACE LIMIT, so there are almost certainly more"
    } else if budget.nodes_left() <= 0 {
        " — STOPPED ON THE NODE BUDGET, so there may be more"
    } else {
        " (neither bound was reached; anything below depth 8, or past 256 children of one \
         node, is still not looked at)"
    };
    let report = format!(
        "window_controls({hwnd}): {} surface(s), {} node(s) visited, {ms} ms{cut}",
        out.len(),
        CONTROL_NODES - budget.nodes_left()
    );
    if bounded || ms >= 25 {
        crate::logging::line("macos", &report);
    } else {
        crate::logging::trace("macos", || report);
    }
    if ms > SLOW_MS {
        crate::logging::line(
            "macos",
            &format!("window_controls({hwnd}) blocked the pump for {ms} ms"),
        );
    }
    out
}

/// The focused element first, its ancestors after it, the window last.
///
/// The question it answers is whether the keyboard focus is inside a plugin's surface or on
/// the host application's own chrome, so the order is the contract: `focusChain[1]` in Lua
/// is the focused control.
///
/// **Empty means the focus could not be read.** Where the read succeeds and nothing reports
/// focus, the window stands in for it, exactly as the Windows version falls back from
/// `hwndFocus` to the foreground window — a plugin view that is not accessible is exactly
/// that case, and a one-link chain ending in the window is the honest "inside". A read that
/// FAILED (the application did not answer within the messaging timeout) used to take the
/// same fallback and come out identical, so a module could not tell "measured: in the
/// window" from "measured nothing", and would have announced the first for the second. Now
/// it is empty, and the module treats empty as not knowing.
pub fn window_focus_chain() -> Vec<ControlInfo> {
    let t = Instant::now();
    // The quarantine applies here too, and this is the one read that could never record it.
    // The question goes to the SYSTEM-WIDE element, whose pid is 0, so a timeout here marked
    // nobody busy: while REAPER was not answering, every overlay recheck paid the full messaging
    // timeout again — once per epoch, on the thread that carries the event tap, while the
    // library overlays' 500 ms landmark polls turn epochs over constantly with a Kontakt window
    // in front. The frontmost application is asked of the workspace, which is local, and it is
    // the application the system-wide read would have been waiting on. Empty is the honest
    // answer meanwhile: every caller reads an empty chain as "not known".
    let front = frontmost_pid();
    if let Some(p) = front {
        if skip_busy(p, "focus_chain") {
            return Vec::new();
        }
    }
    let sw = system_wide();
    // Where the frontmost window is asked for only when it is needed. This runs on every
    // overlay recheck, and resolving the frontmost application costs a workspace query plus
    // two or three cross-process reads that the usual case — something does have focus —
    // has no use for.
    let (start, pid) = match attribute_element_checked(&sw, a_focused_element()) {
        Ok(Some(f)) => {
            let pid = element_pid(&f);
            (Some(f), pid)
        }
        Ok(None) => match frontmost_window_element() {
            Some((w, p)) => (Some(w), p),
            None => (None, 0),
        },
        Err(err) => {
            if err == AXError::CannotComplete {
                if let Some(p) = front {
                    note_busy(p);
                }
            }
            crate::logging::trace("macos", || {
                "focus chain: the focused element could not be read; answering empty".to_string()
            });
            return Vec::new();
        }
    };
    let Some(start) = start else {
        return Vec::new();
    };

    let mut out: Vec<ControlInfo> = Vec::new();
    let mut cur = start;
    let mut saw_window = false;
    for _ in 0..FOCUS_DEPTH {
        let snap = snapshot(&cur);
        // The application element is the parent of every window and is not part of the
        // chain: the host expects the top-level window last.
        if snap.role == "AXApplication" {
            break;
        }
        let is_window = snap.role == "AXWindow";
        if let Some(c) = control_info(cur.clone(), &snap, pid) {
            out.push(c);
        }
        if is_window {
            saw_window = true;
            break;
        }
        let Some(parent) = attribute_element(&cur, a_parent()) else {
            break;
        };
        cur = parent;
    }
    // A focused element that could not be walked up to its window still has to yield a
    // chain ending in one, or the module's "am I inside this plugin" test has nothing to
    // stand on. The window goes on the end even if it reports no rectangle: what the caller
    // reads here is identity and class, and an empty chain is a worse answer than a chain
    // whose last link has no geometry.
    if !saw_window {
        if let Some((w, p)) = frontmost_window_element() {
            let snap = snapshot(&w);
            let frame = snap.rect.unwrap_or(CGRect::new(
                CGPoint::new(0.0, 0.0),
                CGSize::new(0.0, 0.0),
            ));
            let (x, y, cw, ch) = rect_i32(frame);
            out.push(ControlInfo {
                hwnd: handles::intern(w, p, 0),
                class: join_class(&snap),
                x,
                y,
                w: cw,
                h: ch,
                client_x: x,
                client_y: y,
                client_w: cw,
                client_h: ch,
            });
        }
    }
    let ms = t.elapsed().as_millis();
    crate::logging::trace("macos", || {
        format!(
            "window_focus_chain: {} link(s) in {ms} ms, innermost {}",
            out.len(),
            out.first().map(|c| c.class.as_str()).unwrap_or("-")
        )
    });
    out
}

// ---------------------------------------------------------------------------------------
// The uia_* family
// ---------------------------------------------------------------------------------------

/// Where a query starts: the element behind the handle.
fn root_of(hwnd: isize, what: &str) -> Option<CFRetained<AXUIElement>> {
    match handles::get(hwnd) {
        // One check covering `find`, `find_any`, `locate`, `dump` and `focus_step`, because
        // every one of them starts here and then walks. A module reaches them with a handle
        // it got from `active()`, so a remembered window would otherwise hand each of them
        // an application already known not to be answering.
        Some(e) if skip_busy(e.pid, what) => None,
        Some(e) => Some(e.element),
        None => {
            crate::logging::trace("macos", || format!("{what}: {hwnd} is not a live handle"));
            None
        }
    }
}

/// Does this subtree contain an element with this name and role?
///
/// Presence only. Two shipped callers: a plugin's identity check, and the menu watch asking
/// "is a plugin-drawn menu open anywhere in this tree" every 150 ms — which is why the menu
/// question does not walk the tree at all (see [`menu_open_in_app`]).
pub fn find(hwnd: isize, name: &str, control_type: i32) -> bool {
    let Some(roles) = roles_or_log(control_type) else {
        return false;
    };
    let Some(root) = root_of(hwnd, "find") else {
        return false;
    };
    if name.is_empty() && control_type == 50009 {
        return menu_open_in_app(&root);
    }
    let t = Instant::now();
    let mut hit = false;
    let mut budget = Budget::new(QUERY_NODES, HOT_DEADLINE);
    walk(&root, 0, QUERY_DEPTH, &mut budget, &mut |_, snap, _| {
        if role_matches(snap, roles) && snap.matches_name(name) {
            hit = true;
            WalkStep::Stop
        } else {
            WalkStep::Descend
        }
    });
    let ms = t.elapsed().as_millis();
    crate::logging::trace("macos", || {
        format!("find({hwnd}, '{name}', {control_type}) = {hit} in {ms} ms")
    });
    if ms > SLOW_MS {
        crate::logging::line(
            "macos",
            &format!("uia.find('{name}', {control_type}) blocked the pump for {ms} ms"),
        );
    }
    hit
}

/// "Is a menu open in this application right now?"
///
/// A menu is not part of the window that opened it on this platform: macOS displays one as
/// its own element hanging off the application, so the general subtree walk would answer no
/// however long it spent looking. It is also asked on a 150 ms timer, on the thread that
/// carries the keyboard, so it must not cost a walk at all — the application's own children
/// are a handful of elements and that is as far as this goes.
///
/// Two things it deliberately does NOT do.
///
/// It never descends into `AXMenuBar`. A menu bar item owns its `AXMenu` whether the menu is
/// showing or not, so finding one there would mean "a menu is open" forever — and the
/// consequence of that answer is that the overlay hands every navigation key back to the
/// application and stops responding, which is far worse than missing an open menu.
///
/// It looks one level inside the application's other windows, because a toolkit that draws
/// its own popups (Qt does) puts them in a borderless window rather than in a real menu. If
/// a plugin's menu turns out to sit somewhere else again, this returns false and the overlay
/// keeps the keys — the safe direction — and a `element_dump` of the plugin while its menu is
/// open will show where it actually lives.
fn menu_open_in_app(el: &AXUIElement) -> bool {
    let pid = element_pid(el);
    if pid == 0 {
        return false;
    }
    let app = app_element(pid);
    let mut open = false;
    'outer: for child in children(&app).into_iter().take(16) {
        let snap = snapshot(&child);
        if snap.role == "AXMenu" {
            open = true;
            break;
        }
        if snap.role != "AXWindow" {
            continue;
        }
        for grandchild in children(&child).into_iter().take(8) {
            if role(&grandchild) == "AXMenu" {
                open = true;
                break 'outer;
            }
        }
    }
    crate::logging::trace("macos", || format!("menu open in pid {pid}: {open}"));
    open
}

/// "Is any of these names present in this subtree, as any of these roles? Which name?"
///
/// The most-called accessibility method in the platform: a plugin's identity is re-checked
/// on every detection pass, per candidate control. The Windows version pays one tree
/// traversal per NAME to learn which one matched; here one traversal serves all of them —
/// every node is tested against every name and the LOWEST matching index wins, which is the
/// same answer name-major order would give, for the cost of the cheapest question.
///
/// Returns the 1-based index of the matching name. An empty name matches any element of the
/// given types.
pub fn find_any(hwnd: isize, names: &[String], types: &[i32]) -> Option<usize> {
    if names.is_empty() || types.is_empty() {
        return None;
    }
    let mut roles: Vec<&'static str> = Vec::new();
    for t in types {
        if let Some(r) = roles_or_log(*t) {
            roles.extend_from_slice(r);
        }
    }
    if roles.is_empty() {
        return None;
    }
    let root = root_of(hwnd, "find_any")?;
    let t0 = Instant::now();
    let mut best: Option<usize> = None;
    let mut budget = Budget::new(QUERY_NODES, HOT_DEADLINE);
    walk(&root, 0, QUERY_DEPTH, &mut budget, &mut |_, snap, _| {
        if !role_matches(snap, &roles) {
            return WalkStep::Descend;
        }
        for (i, name) in names.iter().enumerate() {
            if snap.matches_name(name) {
                let idx = i + 1;
                if best.is_none_or(|b| idx < b) {
                    best = Some(idx);
                }
                // Nothing can beat the first name, so there is no reason to keep looking.
                if idx == 1 {
                    return WalkStep::Stop;
                }
                break;
            }
        }
        WalkStep::Descend
    });
    let ms = t0.elapsed().as_millis();
    crate::logging::trace("macos", || {
        format!(
            "find_any({hwnd}, {} name(s), {} type(s)) = {best:?} after {} node(s) in {ms} ms",
            names.len(),
            types.len(),
            QUERY_NODES - budget.nodes_left()
        )
    });
    if ms > SLOW_MS {
        crate::logging::line(
            "macos",
            &format!("uia.findAny blocked the pump for {ms} ms ({} names)", names.len()),
        );
    }
    best
}

/// The first element in the subtree with this name and role.
fn first_match(
    root: &AXUIElement,
    name: &str,
    roles: &[&str],
    depth: i32,
    nodes: i32,
) -> Option<(CFRetained<AXUIElement>, Snap)> {
    let mut found: Option<(CFRetained<AXUIElement>, Snap)> = None;
    // A hot query like the others: it runs on the pump, so it gets the deadline that says
    // the event tap is about to be switched off rather than the diagnostic one.
    let mut budget = Budget::new(nodes, HOT_DEADLINE);
    walk(root, 0, depth, &mut budget, &mut |el, snap, _| {
        if role_matches(snap, roles) && snap.matches_name(name) {
            found = Some((el.retain(), snap.clone()));
            WalkStep::Stop
        } else {
            WalkStep::Descend
        }
    });
    found
}

/// Where do I click the element with this name and role?
pub fn locate(hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)> {
    let roles = roles_or_log(control_type)?;
    let root = root_of(hwnd, "locate")?;
    let (_, snap) = first_match(&root, name, roles, QUERY_DEPTH, QUERY_NODES)?;
    let point = snap.rect.and_then(centre);
    crate::logging::trace("macos", || {
        format!("locate({hwnd}, '{name}', {control_type}) = {point:?}")
    });
    point
}

/// Descend into a named container, then find the target within it.
///
/// No shipped module calls this — `plugin_locate` superseded it — but it is the honest
/// two-level form and costs a few lines over the pieces that already exist.
pub fn locate_via(
    hwnd: isize,
    via_name: &str,
    via_type: i32,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    let via_roles = roles_or_log(via_type)?;
    let roles = roles_or_log(control_type)?;
    let root = root_of(hwnd, "locate_via")?;
    let (container, _) = first_match(&root, via_name, via_roles, QUERY_DEPTH, QUERY_NODES)?;
    let (_, snap) = first_match(&container, name, roles, QUERY_DEPTH, QUERY_NODES)?;
    snap.rect.and_then(centre)
}

/// Every element that IS the plugin: named `container_name`, with a Window or Pane role.
///
/// Ordered so that a container with content comes before one without. On Windows this was
/// the `ni::qt::QuickWindow`-before-`QWindowIcon` preference, because the latter is a
/// content-empty proxy for the real scene; the Qt detail does not survive the crossing, but
/// the shape of the problem does — an AudioUnit view hosted inside a DAW produces proxy
/// elements with empty subtrees next to the one that holds the plugin's actual controls.
fn plugin_containers(root: &AXUIElement, container_name: &str) -> Vec<CFRetained<AXUIElement>> {
    let window_roles = roles_for_type(50032).unwrap_or(&[]);
    let pane_roles = roles_for_type(50033).unwrap_or(&[]);
    let mut found: Vec<(bool, CFRetained<AXUIElement>)> = Vec::new();
    let mut budget = Budget::new(QUERY_NODES, HOT_DEADLINE);
    walk(root, 0, QUERY_DEPTH, &mut budget, &mut |el, snap, _| {
        // An exact name match, not `Snap::matches_name`: an empty container name means "a
        // container with no name", the way it does on Windows, and not "any container".
        if (role_matches(snap, window_roles) || role_matches(snap, pane_roles))
            && (snap.title == container_name || snap.description == container_name)
        {
            let has_content = !children(el).is_empty();
            found.push((has_content, el.retain()));
        }
        WalkStep::Descend
    });
    found.sort_by_key(|(has_content, _)| !*has_content);
    found.into_iter().map(|(_, el)| el).collect()
}

/// Find the element that IS the plugin, then the target inside it, and give me the screen
/// centre to click.
///
/// The primary way every Kontakt header control acts. `name` empty means "any element of
/// this role".
pub fn plugin_locate(
    hwnd: isize,
    container_name: &str,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    let roles = roles_or_log(control_type)?;
    let root = root_of(hwnd, "plugin_locate")?;
    let t = Instant::now();
    let containers = plugin_containers(&root, container_name);
    if containers.is_empty() {
        // No element IS the plugin, so ask the whole window — what `state_probe` below already
        // does, for the same reason: on Windows the container is the fragment boundary the
        // search cannot cross, and macOS has no such boundary.
        //
        // Measured inside REAPER on a Mac: Kontakt 8's groups hang straight off the FX window
        // ('FX: Track 1 "Kontakt 8"'), and nothing in it is called "Kontakt 8". So every header
        // action missed by name — the status-bar toggles said "not found", and the file menu
        // worked only because its fallback point happened to be the button's own centre —
        // while the panel anchor, which looks the same button up on the whole window, found it.
        //
        // Only for a NAME. Empty means "any element of this role", and on a whole DAW window
        // that is the DAW's own first button (REAPER's 'Add'). A container that exists and
        // lacks the target still answers None: that is a real "not here" (Kontakt 7's VIEW
        // button, which decides its rack view), not a missing container.
        if name.is_empty() {
            crate::logging::trace("macos", || {
                format!("plugin_locate({hwnd}): no container named '{container_name}'")
            });
            return None;
        }
        let point = first_match(&root, name, roles, QUERY_DEPTH, QUERY_NODES)
            .and_then(|(_, snap)| snap.rect.and_then(centre));
        let ms = t.elapsed().as_millis();
        crate::logging::trace("macos", || {
            format!(
                "plugin_locate({hwnd}, '{container_name}', '{name}', {control_type}) = {point:?} \
                 on the whole window (no container of that name) in {ms} ms"
            )
        });
        if ms > SLOW_MS {
            crate::logging::line(
                "macos",
                &format!("uia.pluginLocate('{name}') blocked the pump for {ms} ms"),
            );
        }
        return point;
    }
    let mut point = None;
    for container in &containers {
        if let Some((_, snap)) = first_match(container, name, roles, QUERY_DEPTH, QUERY_NODES) {
            if let Some(p) = snap.rect.and_then(centre) {
                point = Some(p);
                break;
            }
        }
    }
    let ms = t.elapsed().as_millis();
    crate::logging::trace("macos", || {
        format!(
            "plugin_locate({hwnd}, '{container_name}', '{name}', {control_type}) = {point:?} \
             across {} container(s) in {ms} ms",
            containers.len()
        )
    });
    if ms > SLOW_MS {
        crate::logging::line(
            "macos",
            &format!("uia.pluginLocate('{name}') blocked the pump for {ms} ms"),
        );
    }
    point
}

/// Everything in the subtree, as `(depth, name, class, controlType)`.
///
/// Diagnostic, and the most valuable thing in this file for the first months of the port:
/// the macOS modules are being written by people who cannot see the screen, against plugins
/// nobody here has run, and a dump is the only way to find out what a plugin's tree
/// actually contains. So `class` is the exact join-key string [`join_class`] publishes —
/// what is printed can be pasted into a module's pattern — and the control type is the UIA
/// id that will match that role again.
pub fn dump(hwnd: isize) -> Vec<DumpNode> {
    let Some(root) = root_of(hwnd, "dump") else {
        // Said at line level, because the caller cannot tell this from a walk that ran and
        // found nothing — and what it does with "nothing" is print the sentence that decides
        // an architecture. `root_of` refuses for two reasons: the handle is not live, or the
        // application is in the five-second busy quarantine, and both of ITS notices are
        // traced rather than logged. So an empty answer arrived with no explanation anywhere,
        // and the probe's own escape clause ("if the dump line above says it was complete")
        // pointed at a line that was never written.
        crate::logging::line(
            "macos",
            &format!(
                "dump({hwnd}): NOT WALKED — the handle is not live, or the application is in \
                 the busy quarantine. This is not an empty tree; it is no answer."
            ),
        );
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut budget = Budget::new(DUMP_NODES, DUMP_DEADLINE);
    walk(&root, 0, DUMP_DEPTH, &mut budget, &mut |_, snap, depth| {
        let name = if snap.title.is_empty() {
            snap.description.clone()
        } else {
            snap.title.clone()
        };
        let class = join_class(snap);
        if !name.is_empty() || !snap.role.is_empty() {
            // The rectangle comes from the same batched read as everything else, so it
            // costs nothing extra — and it is what turns a dump from a list of what a
            // plugin contains into something an overlay can be authored against.
            // No rectangle at all is a legitimate answer for an element the toolkit
            // never places, and zeros say so without pretending otherwise.
            let (x, y, w, h) = snap.rect.map(rect_i32).unwrap_or((0, 0, 0, 0));
            out.push(DumpNode {
                depth,
                name,
                class,
                ctype: ctype_for_role(&snap.role),
                x,
                y,
                w,
                h,
            });
        }
        WalkStep::Descend
    });
    // Whether this is the whole tree, on the dump's OWN line rather than through the
    // once-per-session notice inside `walk`.
    //
    // That notice is a single thread-local flag, so the first walk of the session to run out
    // consumes it — and on a large plug-in the control walk (600 nodes at a 300 ms deadline)
    // will do that before the dump is even asked for. The dump would then be truncated in
    // silence, and the sentence the probe builds from it — "N of M elements publish an
    // AXIdentifier" — would be computed over a fragment and read exactly like a complete
    // answer. That sentence decides whether the nested-overlay design ports or is rebuilt out
    // of image matching, so it is the most expensive wrong answer available here.
    //
    // The Windows side has said this per dump since 2026-09-04 (`report_partial` in `uia.rs`)
    // and its doc gives the reason in one line: a truncated tree is indistinguishable from a
    // small one to whoever reads it afterwards. The platform that had the report was the one
    // that will never be pointed at the tree.
    let whole = if budget.ran_out_of_time() {
        " — STOPPED ON TIME, so this is PART of the tree and not all of it"
    } else if budget.nodes_left() <= 0 {
        " — STOPPED ON THE NODE BUDGET, so this is PART of the tree and not all of it"
    } else {
        // Not "complete". Neither bound was reached, which is a smaller claim: `walk` also
        // returns early at `depth > max_depth` without spending anything, so a tree deeper
        // than DUMP_DEPTH is cut off with both budgets intact. Saying "complete" would assert
        // something this function cannot know.
        " — neither bound was reached (anything below depth 24 is still cut off)"
    };
    crate::logging::line(
        "macos",
        &format!(
            "dump({hwnd}): {} element(s), {} node(s) visited{whole}",
            out.len(),
            DUMP_NODES - budget.nodes_left()
        ),
    );
    out
}

/// What a named element says about its own state, as `(toggle, legacy)`.
///
/// `None` means the element was not found, and stays distinguishable from "found, and it
/// says nothing" — the caller draws a different conclusion from each.
///
/// `toggle` is `0` off, `1` on, `-1` when the element carries no state at all. AX has one
/// place for this, `AXValue`, and a control that does not behave like a switch simply has
/// no value; there is no second opinion to consult, so the `legacy` half of the pair — a
/// Windows MSAA state word — carries only the CHECKED bit, synthesised from the same
/// reading, for the caller that looks at `checked` rather than `toggle`.
pub fn state_probe(
    hwnd: isize,
    container_name: &str,
    name: &str,
    control_type: i32,
) -> Option<(i32, i32)> {
    let roles = roles_or_log(control_type)?;
    let root = root_of(hwnd, "state_probe")?;
    let containers = plugin_containers(&root, container_name);
    let mut element = None;
    for container in &containers {
        if let Some((el, _)) = first_match(container, name, roles, QUERY_DEPTH, QUERY_NODES) {
            element = Some(el);
            break;
        }
    }
    // A container that does not exist should not hide an element that does: Windows searches
    // the container first and only the container, but there the container is the fragment
    // boundary, which macOS does not have.
    let element = match element {
        Some(el) => el,
        None => first_match(&root, name, roles, QUERY_DEPTH, QUERY_NODES)?.0,
    };

    let toggle = match attribute(&element, a_value()) {
        Some(v) => {
            if let Some(b) = v.downcast_ref::<CFBoolean>() {
                i32::from(b.as_bool())
            } else if let Some(n) = v.downcast_ref::<CFNumber>() {
                match n.as_i64() {
                    Some(0) => 0,
                    Some(1) => 1,
                    Some(_) => 2, // anything else is this platform's "indeterminate"
                    None => -1,
                }
            } else {
                -1
            }
        }
        None => -1,
    };
    let legacy = if toggle == 1 { 0x10 } else { 0 };
    crate::logging::trace("macos", || {
        format!("state_probe({hwnd}, '{name}') = toggle {toggle}, legacy {legacy:#x}")
    });
    Some((toggle, legacy))
}

/// Find the first element whose identifier contains a substring, walk to a child and a
/// sibling of it, and give me that element's click centre.
///
/// A port of ReaHotkey's `FindElement(ClassName)` + `WalkTree(path)`, which closes Komplete
/// Kontrol's library browser and dismisses Kontakt's What's-New dialog. There is no class
/// name here: `AXIdentifier` is the nearest thing — Qt populates it from `objectName`,
/// which is where those ReaHotkey class names came from in the first place — with
/// `AXRoleDescription` as a fallback for toolkits that leave the identifier empty.
///
/// `child` is 1-based and 0 means "do not descend"; `sibling` is signed, negative walking
/// backwards, and is counted within the parent's children.
pub fn class_nav_point(
    hwnd: isize,
    class_substr: &str,
    ctype: i32,
    child: i32,
    sibling: i32,
) -> Option<(i32, i32)> {
    let roles = roles_or_log(ctype)?;
    let root = root_of(hwnd, "class_nav_point")?;
    let mut found: Option<CFRetained<AXUIElement>> = None;
    let mut budget = Budget::new(QUERY_NODES, HOT_DEADLINE);
    walk(&root, 0, QUERY_DEPTH, &mut budget, &mut |el, snap, _| {
        if role_matches(snap, roles) && snap.identifier.contains(class_substr) {
            found = Some(el.retain());
            return WalkStep::Stop;
        }
        WalkStep::Descend
    });
    if found.is_none() {
        // Second pass on the DESCRIPTION, which is what a toolkit that sets no identifier
        // usually does carry. A separate pass rather than an OR in the first one, so an
        // exact identifier match always wins over a loose description match.
        //
        // `AXDescription`, not `AXRoleDescription`: the role description is a localised
        // phrase for the KIND of thing ("button", "Schaltfläche"), so matching a vendor's
        // object name against it could only ever succeed by accident, and on a German
        // machine not even that. The batched snapshot has already fetched this one, so the
        // pass costs no extra round trip per element.
        let mut budget = Budget::new(QUERY_NODES, HOT_DEADLINE);
        walk(&root, 0, QUERY_DEPTH, &mut budget, &mut |_el, snap, _| {
            if role_matches(snap, roles) && snap.description.contains(class_substr) {
                found = Some(_el.retain());
                return WalkStep::Stop;
            }
            WalkStep::Descend
        });
    }
    let mut el = found?;

    if child >= 1 {
        let kids = children(&el);
        el = kids.get((child - 1) as usize)?.clone();
    }
    if sibling != 0 {
        let parent = attribute_element(&el, a_parent())?;
        let kids = children(&parent);
        let here = kids.iter().position(|k| **k == *el)?;
        let target = here as i32 + sibling;
        if target < 0 || target as usize >= kids.len() {
            crate::logging::trace("macos", || {
                format!("class_nav_point({hwnd}, '{class_substr}'): sibling {sibling} is off the end")
            });
            return None;
        }
        el = kids[target as usize].clone();
    }
    let point = element_rect_cg(&el).and_then(centre);
    crate::logging::trace("macos", || {
        format!("class_nav_point({hwnd}, '{class_substr}', {ctype}, {child}, {sibling}) = {point:?}")
    });
    point
}

/// Move the plugin's own keyboard focus one step, and say what got focused.
///
/// For a standalone plugin window that does not move focus on Tab by itself. Four things
/// about it are load-bearing, all of them learned on the Windows side:
///
/// 1. **The ring is the whole window, minus the window's own title-bar buttons.** The
///    Windows side scopes to the window's first child (ReaHotkey's `ElementFromPath(1)`),
///    which there is the content area and drops the frame and menu bar for free. On macOS
///    the first child is whatever the application published first: in the probe of a
///    standalone Kontakt 7 it is Kontakt's logo, a button with no children, so a ring
///    scoped to it was empty and the pass-through could never step anywhere. The menu bar
///    is not inside a window on this platform at all; what the frame contributes is the
///    close, minimise, zoom and full-screen buttons, and those are skipped by subrole —
///    a Return on a close button that a Tab had just landed on would close the plugin.
/// 2. **Candidates are re-enumerated on every step**, because a plugin's tree changes shape
///    as the user moves through it and a remembered list would step into elements that no
///    longer exist.
/// 3. **Focus is read back to confirm it landed.** Plenty of elements accept the request and
///    do nothing; those are skipped rather than silently announced.
/// 4. **The index is 1-based**, to match Luau and the Windows backend. The caller
///    (`Overlay:_stepPassThrough`) counts a lap from the distance between successive indices
///    while `count` holds, so what it needs is an index that is stable for an element in an
///    unchanged ring — not any particular base. (It used to wait for the index it entered at to
///    come round again; that ring trapped a tester in Kontakt 8's 90 stops.)
///
/// It also has to raise a real focus event, which setting `AXFocused` does — the screen
/// reader announces the element and the overlay deliberately stays quiet.
pub fn focus_step(hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)> {
    let root = root_of(hwnd, "focus_step")?;
    let t = Instant::now();
    // The whole window — see point 1 above for why not its first child on this platform.
    let scope = root.clone();

    let mut items: Vec<(CFRetained<AXUIElement>, Snap)> = Vec::new();
    let mut budget = Budget::new(FOCUS_NODES, HOT_DEADLINE);
    walk(&scope, 0, FOCUS_DEPTH, &mut budget, &mut |el, snap, depth| {
        if depth == 0 {
            return WalkStep::Descend; // the scope itself is not a stop on the ring
        }
        // The window's own frame buttons, and nothing under them (the zoom button carries a
        // group of its own). Never a stop: landing on one puts a Return one key from closing
        // or minimising the plugin, and none of them is the plugin's.
        if matches!(
            snap.subrole.as_str(),
            "AXCloseButton" | "AXMinimizeButton" | "AXZoomButton" | "AXFullScreenButton"
        ) {
            return WalkStep::Skip;
        }
        // Off-screen and collapsed elements are not real stops: a hidden tab page keeps its
        // whole subtree, and tabbing into it would announce controls the user cannot see.
        let visible = snap.rect.is_some_and(|r| r.size.width > 0.0 && r.size.height > 0.0);
        if visible && is_focusable(el) {
            items.push((el.retain(), snap.clone()));
        }
        WalkStep::Descend
    });
    if items.is_empty() {
        crate::logging::line(
            "macos",
            &format!("focus_step({hwnd}): nothing in this window accepts keyboard focus"),
        );
        return None;
    }
    let count = items.len() as i32;

    let sw = system_wide();
    let focused = attribute_element(&sw, a_focused_element());
    let mut cur: i32 = -1;
    if let Some(f) = focused.as_ref() {
        for (i, (el, _)) in items.iter().enumerate() {
            if **el == **f {
                cur = i as i32;
                break;
            }
        }
    }

    let mut idx = cur;
    for _ in 0..count {
        idx = if idx < 0 {
            if direction >= 0 {
                0
            } else {
                count - 1
            }
        } else if direction >= 0 {
            (idx + 1) % count
        } else {
            (idx - 1 + count) % count
        };
        let (el, snap) = &items[idx as usize];
        let err = unsafe { el.set_attribute_value(a_focused(), CFBoolean::new(true).as_ref()) };
        if err != AXError::Success {
            note_error(err, "AXFocused");
            continue;
        }
        let landed = attribute_element(&sw, a_focused_element()).is_some_and(|f| *f == **el);
        if !landed {
            continue;
        }
        let name = if snap.title.is_empty() {
            snap.description.clone()
        } else {
            snap.title.clone()
        };
        let ctype = ctype_for_role(&snap.role);
        crate::logging::trace("macos", || {
            format!(
                "focus_step({hwnd}, {direction}) = '{name}' type {ctype} at {}/{count} in {} ms",
                idx + 1,
                t.elapsed().as_millis()
            )
        });
        // Once per window, at line level. The failures above are already lines; success was
        // only trace, so a pass-through that WORKED left nothing in the log, and whether a
        // plugin's own controls took the keyboard would rest on the tester's notes alone. The
        // count is the other half of the answer: a ring of three and a ring of thirty are
        // different plugins to navigate.
        thread_local! {
            static RING_REPORTED: RefCell<std::collections::HashSet<isize>> =
                RefCell::new(std::collections::HashSet::new());
        }
        if RING_REPORTED.with(|r| r.borrow_mut().insert(hwnd)) {
            crate::logging::line(
                "macos",
                &format!(
                    "focus_step({hwnd}): the ring has {count} focusable stop(s); the keyboard \
                     is on '{name}' ({}), stop {} — in {} ms",
                    snap.role,
                    idx + 1,
                    t.elapsed().as_millis()
                ),
            );
        }
        return Some((name, ctype, idx + 1, count));
    }
    crate::logging::line(
        "macos",
        &format!("focus_step({hwnd}): none of {count} candidate(s) accepted focus"),
    );
    None
}

/// Give keyboard focus to the first focusable element inside a rectangle of the window.
///
/// The polite half of "put the keyboard back into the plugin". Focusing the WINDOW (which
/// `focus_window` does, three ways) hands the keyboard to whatever that window last had
/// focused — REAPER's FX list, measured five times out of five — and the only thing that
/// moved it on from there was a click into the plugin's panel. That click is right for a
/// plugin that publishes no element at all; for one that does, it is a press on whatever
/// sits at the corner. Setting `AXFocused` on one of the plugin's own elements is what a
/// screen reader's navigation does, raises a real focus event, and presses nothing.
///
/// Walked from the window itself rather than from its first child, unlike `focus_step`:
/// inside REAPER's FX window Kontakt's elements ARE the window's children, and there is no
/// content group to scope to. The rectangle is the scope instead — the caller knows where
/// the panel is — and the first candidate in tree order that takes focus (read back, not
/// assumed) wins, so a text field near the top of a panel is what usually gets it.
pub fn focus_within(hwnd: isize, x: i32, y: i32, w: i32, h: i32) -> Option<(String, i32)> {
    let root = root_of(hwnd, "focus_within")?;
    let t = Instant::now();
    let inside = |r: &CGRect| {
        let cx = r.origin.x + r.size.width / 2.0;
        let cy = r.origin.y + r.size.height / 2.0;
        cx >= x as f64 && cx <= (x + w) as f64 && cy >= y as f64 && cy <= (y + h) as f64
    };
    let mut items: Vec<(CFRetained<AXUIElement>, Snap)> = Vec::new();
    let mut budget = Budget::new(FOCUS_NODES, HOT_DEADLINE);
    walk(&root, 0, FOCUS_DEPTH, &mut budget, &mut |el, snap, depth| {
        if depth == 0 {
            return WalkStep::Descend;
        }
        let candidate = snap
            .rect
            .is_some_and(|r| r.size.width > 0.0 && r.size.height > 0.0 && inside(&r));
        if candidate && is_focusable(el) {
            items.push((el.retain(), snap.clone()));
        }
        WalkStep::Descend
    });
    let sw = system_wide();
    // Counted, because the failure line below is the whole answer to the question this
    // function was written for: will a plugin that publishes elements hand over the keyboard
    // when asked? "Nothing took it" covers two different worlds — the accessibility layer
    // refused the write (nothing more to try; the corner click stays the only route) and the
    // write was accepted while the focus did not move (a fault on our side, and fixable).
    // The per-candidate reason goes to `note_error`, which is trace-level for most errors
    // and throttled to one line per five seconds for the rest, so with a dozen candidates it
    // says nothing usable. These two numbers cost nothing and separate the worlds.
    let mut refused = 0usize;
    let mut first_refusal: Option<&'static str> = None;
    let mut accepted_but_not_focused = 0usize;
    for (el, snap) in &items {
        let err = unsafe { el.set_attribute_value(a_focused(), CFBoolean::new(true).as_ref()) };
        if err != AXError::Success {
            refused += 1;
            if first_refusal.is_none() {
                first_refusal = Some(err_name(err));
            }
            note_error(err, "AXFocused");
            continue;
        }
        if !attribute_element(&sw, a_focused_element()).is_some_and(|f| *f == **el) {
            accepted_but_not_focused += 1;
            continue;
        }
        let name = if snap.title.is_empty() { snap.description.clone() } else { snap.title.clone() };
        let ctype = ctype_for_role(&snap.role);
        // Logged, not traced: this runs once per press of a key somebody chose to press, and
        // which element took the keyboard is the whole answer.
        crate::logging::line(
            "macos",
            &format!(
                "focus_within({hwnd}): the keyboard is on '{name}' ({}) — the first of {} \
                 focusable element(s) inside {x},{y} {w}x{h} to take it, in {} ms",
                snap.role,
                items.len(),
                t.elapsed().as_millis()
            ),
        );
        return Some((name, ctype));
    }
    crate::logging::line(
        "macos",
        &format!(
            "focus_within({hwnd}): nothing inside {x},{y} {w}x{h} took keyboard focus — {} \
             focusable candidate(s), {refused} refused the write ({}), {accepted_but_not_focused} \
             accepted it and did not become the focused element. A plugin that publishes no \
             element at all has nothing to ask and reports 0 candidates.",
            items.len(),
            first_refusal.unwrap_or("none")
        ),
    );
    None
}

/// Can this element be given keyboard focus?
///
/// Asking whether `AXFocused` is *settable* is the direct question — the AX equivalent of
/// UIA's `IsKeyboardFocusable` — and it is a separate round trip per candidate, which is why
/// it is asked only after the cheaper geometry test has already rejected the invisible ones.
fn is_focusable(el: &AXUIElement) -> bool {
    let mut settable: u8 = 0;
    let err = unsafe { el.is_attribute_settable(a_focused(), NonNull::from(&mut settable)) };
    if err != AXError::Success {
        return false;
    }
    settable != 0
}
