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
    NSAccessibilityValueAttribute, NSAccessibilityWindowsAttribute, NSRunningApplication,
    NSWorkspace,
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
    kCGWindowBounds, kCGWindowNumber, kCGWindowOwnerPID, CGRectMakeWithDictionaryRepresentation,
    CGWindowListCopyWindowInfo, CGWindowListOption,
};
use objc2_foundation::NSString;

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
const MESSAGING_TIMEOUT: f32 = 1.0;

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
const BUSY_PENALTY: Duration = Duration::from_millis(5000);

/// Note that an application is not answering.
fn note_busy(pid: i32) {
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
                        "accessibility timed out reading {what} after {MESSAGING_TIMEOUT}s — the \
                         target application is busy or not responding; the answer was dropped"
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
/// `"/NI%.Kontakt"` — and `uia_dump` prints exactly this string, so an author reading a dump
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
    budget: &mut i32,
    visit: &mut impl FnMut(&AXUIElement, &Snap, i32) -> WalkStep,
) -> bool {
    if *budget <= 0 {
        // Said out loud, once, and not at trace level. A walk that ran out of budget returns
        // exactly what a walk that finished and found nothing returns, so without this line
        // a plugin whose tree is larger than the bound answers "that element is not here" to
        // every question, for ever, and the overlay simply never activates — with nothing
        // anywhere to distinguish it from a plugin we do not support.
        if !BUDGET_REPORTED.with(Cell::get) {
            BUDGET_REPORTED.with(|c| c.set(true));
            crate::logging::line(
                "macos",
                "an accessibility walk hit its node budget and stopped early — any answer from it is 'not found so far', not 'not there'. If detection is failing on a large plugin, this is the first thing to look at.",
            );
        }
        return true;
    }
    if depth > max_depth {
        return true;
    }
    *budget -= 1;
    let snap = snapshot(el);
    match visit(el, &snap, depth) {
        WalkStep::Stop => return false,
        WalkStep::Skip => return true,
        WalkStep::Descend => {}
    }
    for child in children(el) {
        if *budget <= 0 {
            return true;
        }
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
/// The ids stay UIA ids on every platform — they are what `host.uia.type` hands modules and
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
    for child in children(el).into_iter().take(24) {
        let Some(r) = element_rect_cg(&child) else {
            continue;
        };
        // The window's own buttons, which are the measuring instrument when there is no
        // content view to find. See below.
        if let Some(s) = attribute_string(&child, a_subrole()) {
            if matches!(s.as_str(), "AXCloseButton" | "AXMinimizeButton" | "AXZoomButton") {
                let centre = r.origin.y + r.size.height / 2.0 - frame.origin.y;
                if centre > 0.0 && button_centre.is_none_or(|c| centre < c) {
                    button_centre = Some(centre);
                }
                continue;
            }
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
    let snap = snapshot(&el);
    let title = snap.title.clone();
    if require_title && title.is_empty() {
        return None;
    }
    // Minimised is this platform's "not visible": the window still exists and still answers,
    // it is simply not on screen, and its geometry is stale.
    if attribute_bool(&el, a_minimized()) == Some(true) {
        return None;
    }
    let frame = snap.rect?;
    let pid = element_pid(&el);
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

/// Visible windows with a non-empty title.
///
/// Cold: no shipped module calls `host.window.list`, and the prelude's `find`/`findAll`
/// helpers are the only callers. Correctness over speed — but still bounded, because this
/// asks every running application a question and one of them being wedged must not become
/// our problem.
pub fn enumerate_windows() -> Vec<WinInfo> {
    let t = Instant::now();
    let mut out = Vec::new();
    let apps = NSWorkspace::sharedWorkspace().runningApplications();
    for app in apps.iter().take(256) {
        let pid = app.processIdentifier();
        if pid <= 0 {
            continue;
        }
        let app_el = app_element(pid);
        let Some(v) = attribute(&app_el, a_windows()) else {
            continue; // no windows, or a process with no accessibility surface at all
        };
        let Some(arr) = v.downcast_ref::<CFArray>() else {
            continue;
        };
        // SAFETY: AXWindows is documented as an array of AXUIElementRef.
        let typed: &CFArray<AXUIElement> = unsafe { arr.cast_unchecked::<AXUIElement>() };
        for i in 0..typed.len().min(64) {
            if let Some(w) = typed.get(i) {
                if let Some(info) = win_info(w, true) {
                    out.push(info);
                }
            }
        }
    }
    let ms = t.elapsed().as_millis();
    crate::logging::trace("macos", || {
        format!("enumerate_windows: {} window(s) in {ms} ms", out.len())
    });
    if ms > SLOW_MS {
        crate::logging::line("macos", &format!("enumerate_windows blocked the pump for {ms} ms"));
    }
    out
}

/// The element of the frontmost application's frontmost window.
///
/// `AXFocusedWindow` is the one that would receive a keystroke, which is the question the
/// host is really asking; `AXMainWindow` and then the first of `AXWindows` stand in for it
/// while a window is still coming up, because a dialog that is foreground before it has
/// settled is exactly the case this has to answer for.
pub(super) fn frontmost_window_element() -> Option<(CFRetained<AXUIElement>, i32)> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let pid = app.processIdentifier();
    if pid <= 0 {
        return None;
    }
    // Asked of the workspace, which is local and cheap, BEFORE anything is asked of the
    // application itself. The three reads below are tried in turn, so an application that
    // is not answering charges the messaging timeout three times over — and this is the
    // hottest question in the system.
    if is_busy(pid) {
        crate::logging::trace("macos", || {
            format!("skipping pid {pid}: it did not answer a moment ago")
        });
        return None;
    }
    // How long the answer actually took, when it came at all.
    //
    // The timeout above was set from an assumption about healthy applications and cost a tester
    // his whole session; the replacement is a guess too, and this is what turns the next one into
    // a measurement. Only slow successes are reported — a fast answer is not news, and a failure
    // already writes its own line.
    let started = Instant::now();
    let app_el = app_element(pid);
    let el = attribute_element(&app_el, a_focused_window())
        .or_else(|| attribute_element(&app_el, a_main_window()))
        .or_else(|| {
            let v = attribute(&app_el, a_windows())?;
            let arr = v.downcast_ref::<CFArray>()?;
            // SAFETY: AXWindows is an array of AXUIElementRef.
            let typed: &CFArray<AXUIElement> = unsafe { arr.cast_unchecked::<AXUIElement>() };
            typed.get(0)
        })?;
    let ms = started.elapsed().as_millis();
    if ms >= 100 {
        crate::logging::line(
            "macos",
            &format!("pid {pid} answered the frontmost-window question after {ms} ms"),
        );
    }
    Some((el, pid))
}

/// The frontmost window. A title is NOT required — an untitled dialog is exactly the case
/// this has to answer for.
pub fn active_window() -> Option<WinInfo> {
    let t = Instant::now();
    let (el, _pid) = frontmost_window_element()?;
    let info = win_info(el, false);
    let ms = t.elapsed().as_millis();
    crate::logging::trace("macos", || match &info {
        Some(w) => format!(
            "active_window: id {} '{}' class {} at {},{} {}x{} client {},{} {}x{} in {ms} ms",
            w.hwnd, w.title, w.class, w.x, w.y, w.w, w.h, w.client_x, w.client_y, w.client_w,
            w.client_h
        ),
        None => format!("active_window: none in {ms} ms"),
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

/// The handle of the frontmost window, or 0. Used for the key-scope snapshot.
///
/// Returns the interned handle without building a whole `WinInfo`, because the caller wants
/// identity and nothing else — but it does intern, so the number it hands back is the same
/// one `active_window` reports and the comparison at key-press time is meaningful.
pub fn foreground_window_id() -> isize {
    match frontmost_window_element() {
        Some((el, pid)) => handles::intern(el, pid, 0),
        None => 0,
    }
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
#[allow(dead_code)]
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
    Some(ControlInfo {
        hwnd: handles::intern(el, pid, 0),
        class,
        x,
        y,
        w,
        h,
        // There is no frame-versus-client distinction below the window: an accessibility
        // element's rectangle is its content, with no border and no title bar of its own.
        // The Windows backend reports the same thing for the borderless windows plugins
        // use — `mod.rs` says so in as many words.
        client_x: x,
        client_y: y,
        client_w: w,
        client_h: h,
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
    let mut out: Vec<ControlInfo> = Vec::new();
    let mut budget = CONTROL_NODES;
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
    crate::logging::trace("macos", || {
        format!(
            "window_controls({hwnd}): {} surface(s), {} node(s) visited, {ms} ms",
            out.len(),
            CONTROL_NODES - budget
        )
    });
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
/// is the focused control. Never empty while there is a foreground window — where nothing
/// reports focus, the window stands in for it, exactly as the Windows version falls back
/// from `hwndFocus` to the foreground window.
pub fn window_focus_chain() -> Vec<ControlInfo> {
    let t = Instant::now();
    let sw = system_wide();
    // Where the frontmost window is asked for only when it is needed. This runs on every
    // overlay recheck, and resolving the frontmost application costs a workspace query plus
    // two or three cross-process reads that the usual case — something does have focus —
    // has no use for.
    let (start, pid) = match attribute_element(&sw, a_focused_element()) {
        Some(f) => {
            let pid = element_pid(&f);
            (Some(f), pid)
        }
        None => match frontmost_window_element() {
            Some((w, p)) => (Some(w), p),
            None => (None, 0),
        },
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
    let mut budget = QUERY_NODES;
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
/// keeps the keys — the safe direction — and a `uia_dump` of the plugin while its menu is
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
    let mut budget = QUERY_NODES;
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
            QUERY_NODES - budget
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
    budget: i32,
) -> Option<(CFRetained<AXUIElement>, Snap)> {
    let mut found: Option<(CFRetained<AXUIElement>, Snap)> = None;
    let mut budget = budget;
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
    let mut budget = QUERY_NODES;
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
        crate::logging::trace("macos", || {
            format!("plugin_locate({hwnd}): no container named '{container_name}'")
        });
        return None;
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
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut budget = DUMP_NODES;
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
    crate::logging::line(
        "macos",
        &format!("dump({hwnd}): {} element(s), {} node(s) visited", out.len(), DUMP_NODES - budget),
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
    let mut budget = QUERY_NODES;
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
        let mut budget = QUERY_NODES;
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
/// 1. **The ring is scoped to the window's first child**, not the window. That drops the
///    frame and the menu bar — which are siblings of the content, not inside it — without
///    needing a list of roles to exclude.
/// 2. **Candidates are re-enumerated on every step**, because a plugin's tree changes shape
///    as the user moves through it and a remembered list would step into elements that no
///    longer exist.
/// 3. **Focus is read back to confirm it landed.** Plenty of elements accept the request and
///    do nothing; those are skipped rather than silently announced.
/// 4. **The index is 1-based**, because the caller uses "the index I entered at has come
///    round again" as its ring-completed signal, and a 0 would fire that on the first step.
///
/// It also has to raise a real focus event, which setting `AXFocused` does — the screen
/// reader announces the element and the overlay deliberately stays quiet.
pub fn focus_step(hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)> {
    let root = root_of(hwnd, "focus_step")?;
    let t = Instant::now();
    // ReaHotkey's ElementFromPath(1): the content area. Falls back to the window when it has
    // no children of its own.
    let scope = children(&root).into_iter().next().unwrap_or_else(|| root.clone());

    let mut items: Vec<(CFRetained<AXUIElement>, Snap)> = Vec::new();
    let mut budget = FOCUS_NODES;
    walk(&scope, 0, FOCUS_DEPTH, &mut budget, &mut |el, snap, depth| {
        if depth == 0 {
            return WalkStep::Descend; // the scope itself is not a stop on the ring
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
        return Some((name, ctype, idx + 1, count));
    }
    crate::logging::line(
        "macos",
        &format!("focus_step({hwnd}): none of {count} candidate(s) accepted focus"),
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
