//! Platform backend abstraction. The host talks to the OS only through this
//! trait; one implementation per platform, selected at compile time: Windows,
//! macOS, and a stub that answers nothing anywhere else. See `docs/macos-port.md`
//! for the second one, which was written without a Mac to run it on.
//!
//! Game controllers are the one OS input that does NOT go through the trait: they live in
//! [`gamepad`], beside it, with a hub of their own that a thread or a dispatch queue feeds
//! and every backend's `pump_pending` drains. See that module for why.

use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::errors::{common, input, HostError};

/// Game controllers, observed from the background. Outside the [`Backend`] trait on purpose.
pub mod gamepad;

/// A capture kept as a value — what `host.screen.snapshot` holds — and the geometry every read
/// of one uses. Pure; see the file.
pub mod frame;

/// What the Permissions page and the start announcement say about the [`Permission`]s — words
/// only, with no window in them. Pure; see the file. Public, as [`Permission`] is, because
/// `crates/macos-check` borrows this file and has no page to use them.
pub mod grants;

#[cfg(windows)]
mod windows;
/// A landing of the keyboard on a control (HW1), for a page that gives the front back
/// (`webview_win.rs`).
#[cfg(windows)]
pub(crate) use windows::start_landing;
/// Registered hotkeys matched in the low-level keyboard hook as well as by `RegisterHotKey`:
/// the table and the one-press rules, with no OS call in them, so their tests run anywhere
/// Windows builds. See the file.
#[cfg(windows)]
mod hotkey_hook;
/// Keeping the keyboard hook alive for days: when to install it again, when to take it out of
/// the chain for this process's own windows, and the comparison of raw input with the hook's
/// calls that notices a hook Windows removed. Pure; see the file.
#[cfg(windows)]
mod hook_watch;
/// The thread that watches the keyboard hook on Windows — raw input, session and power
/// notifications, the window in front — and asks the hook's thread to install it again, or to
/// take it out of the chain while one of this process's windows is in front. See the file.
#[cfg(windows)]
mod hook_watch_thread;
/// DXGI Desktop Duplication, the second way of reading the screen on Windows — for the modules
/// that declare `[screen] capture = "duplication"`, and for nothing else. See the file.
#[cfg(windows)]
mod dxgi;
/// The neural recogniser, PaddleOCR's recognition model under ONNX Runtime: on Windows beside
/// `Windows.Media.Ocr` on every small region, on a Mac loaded from the bundle and, in this build,
/// only measured. See the file.
#[cfg(any(windows, target_os = "macos"))]
mod paddle_ocr;
#[cfg(windows)]
mod uia;
/// Clicks into another program's window that leave the window in front where it is —
/// `host.input.click`'s `via = "post"` and `via = "noactivate"` — and the check its `window` asks
/// for. See the file.
#[cfg(windows)]
mod click_win;
#[cfg(target_os = "macos")]
mod macos;
/// Bringing this process to the front, and whether it has a Dock icon — see
/// `macos::app`. Re-exported so `gui.rs` can reach it without a macOS-only `use`.
#[cfg(target_os = "macos")]
pub(crate) use macos::app::{
    activate_self, note_frontmost_before_gui, restore_frontmost_after_gui_start, set_regular,
};
/// Following this application's own windows once the window system is up — see `macos::own`.
/// Re-exported for the same reason: `gui.rs` calls it as wxWidgets hands over.
#[cfg(target_os = "macos")]
pub(crate) use macos::own::start as start_own_windows;
/// Whether this application became the active one since this was last asked — see `macos::own`.
/// For the Permissions page, which reads the permissions again then.
#[cfg(target_os = "macos")]
pub(crate) use macos::own::take_became_active;
/// No permissions page anywhere else, and so nothing to read again.
#[cfg(not(target_os = "macos"))]
pub(crate) fn take_became_active() -> bool {
    false
}
/// `host.gui.webview`'s window on a Mac — see `macos::page` — with the interface `webview_win.rs`
/// has, which `webview_lua.rs` uses as it uses that one; `crates/macos-check` names it, so the two
/// cannot drift apart unseen on Windows, and `page-check` builds pages with it too.
#[cfg(target_os = "macos")]
pub(crate) use macos::page;
/// Asking the user to allow Apple Events to VoiceOver — see `macos::perm`. Re-exported for
/// the same reason as the app calls above: `gui.rs` reaches it without a macOS-only `use`,
/// and the path resolves identically in `crates/macos-check`, which borrows this file.
#[cfg(target_os = "macos")]
pub(crate) use macos::perm::request_voiceover_automation;
/// Whether that request's dialog is on screen — asked by the speech path, which must not talk
/// over it (`speech/vo_script.rs`). Re-exported for the same reason.
#[cfg(target_os = "macos")]
pub(crate) use macos::perm::voiceover_automation_asking;
/// The four permissions as something a window can show — see `macos::perm::permissions`.
/// Re-exported for the same reason as the call above: `gui.rs` reaches it without a
/// macOS-only `use`, and the path resolves identically in `crates/macos-check`.
#[cfg(target_os = "macos")]
pub(crate) use macos::perm::{ask_for, open_pane, permissions};
/// `automation-platform ocr-bench`, which measures Vision on fixed pictures — see
/// `macos::ocr::bench` — and returns the exit code. Every process of it — the run, each child it
/// starts, `--paddle`, `--paddle-probe` — releases ONNX Runtime here before `main` hands the code
/// to `exit()` ([`release_ocr`]), or ends through [`end_now`] with the same code when it cannot.
/// For `lib.rs`, and so that `crates/macos-check` can name it.
#[cfg(target_os = "macos")]
pub(crate) fn ocr_bench(args: &[String]) -> i32 {
    let code = macos::ocr::bench::run(args);
    if !release_ocr() {
        eprintln!("OCR BENCH: ONNX Runtime could not be released; ending without exit()'s teardown, status {code}");
        end_now(code);
    }
    code
}

/// `automation-platform page-check`, which measures how this application takes the front and gives
/// it back, and checks the page window — see `macos::page_check` — and returns the exit code. For `lib.rs`, and so that
/// `crates/macos-check` can name it.
#[cfg(target_os = "macos")]
pub(crate) fn page_check(args: &[String]) -> i32 {
    macos::page_check::run(args)
}

/// `automation-platform capture-check`, which captures the screen the ways the host does and says
/// which path answered — see `macos::capture_check` — and returns the exit code. For `lib.rs`,
/// and so that `crates/macos-check` can name it.
#[cfg(target_os = "macos")]
pub(crate) fn capture_check(args: &[String]) -> i32 {
    macos::capture_check::run(args)
}

/// `automation-platform ocr-bench --paddle` on Windows: the neural recogniser measured alone on
/// the bench's pictures, beside what `Windows.Media.Ocr` reads and what Windows answers — see
/// `paddle_ocr::bench`; and `--paddle-probe`, whether it loads and reads. For
/// `ocr::bench::elsewhere`; no host API. (On a Mac `macos::ocr::bench` calls both itself.)
#[cfg(windows)]
pub(crate) fn ocr_bench_paddle(o: &crate::ocr::bench::Options) -> i32 {
    if o.paddle_probe {
        return paddle_ocr::probe();
    }
    let path = o.out.clone().unwrap_or_else(|| crate::ocr::bench::free_name(crate::portable::base_dir()));
    paddle_ocr::bench_rows(o, Some(&windows::bench_system_read), path)
}
/// The backend of a platform with none of its own. Compiled into every test build as well: the
/// tests that run module VMs against the host's key bindings use it, with a window in front
/// they set and the captures it was handed (`key_scope_tests.rs`).
#[cfg(any(test, not(any(windows, target_os = "macos"))))]
pub(crate) mod stub;

/// The macOS key table, compiled on every other platform too, for its tests alone.
///
/// It is a table of integers with no platform dependency, and it is the one part of that
/// backend whose correctness can be checked without a Mac: that every key `key_to_vk`
/// accepts has a macOS key code, and that the translation round-trips. Left inside `mod
/// macos` those tests would only ever run on a machine nobody on the project has.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/keys.rs"]
mod macos_keys;

/// The same trick for the frontmost-window memory: pure bookkeeping over pids, handles and
/// timestamps, so the rules it enforces can be executed rather than merely read. They decide
/// whether a blind user's overlay clicks into a rectangle that is still true, and every one
/// of them was got wrong at least once under review.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/front_memory.rs"]
mod macos_front_memory;

/// And the walk budget, for the third time and the same reason: a counter and a clock, with
/// nothing macOS about them. This one bounds how long an accessibility walk may take, and the
/// next session's most valuable keypress points it at the largest tree it has ever seen.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/budget.rs"]
mod macos_budget;

/// And how late a key reached the event tap: a timestamp in units nobody here has seen, read
/// two ways, which is exactly the kind of arithmetic that should run before a Mac relies on it.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/key_age.rs"]
mod macos_key_age;

/// And ScreenCaptureKit's back-off: a clock handed in and a doubling, which decide whether every
/// OCR and pixel read of a days-long session still reaches ScreenCaptureKit after one slow
/// capture after a wake.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/backoff.rs"]
mod macos_backoff;

/// And the App Nap activity's bookkeeping: which activity captured keys, a controller and a
/// first setup's wait call for, and that every reason gets its line the first time it holds it —
/// the lines a Mac session looks for.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/activity_reasons.rs"]
mod macos_activity_reasons;

/// And the handle table's bookkeeping, generic over the element: which window a handle lies in
/// and what a sweep drops — a window gone takes its controls with it, and a handle is never
/// reused.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/handle_table.rs"]
mod macos_handle_table;

/// And which requests Screen Recording and Input Monitoring get, in which order, and what the
/// log says for each answer: decided without a Mac, for the permission a blind tester otherwise
/// has to add to its list by hand.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/enrol.rs"]
mod macos_enrol;

/// And whether Screen Recording is granted and Input Monitoring needed, judged by the evidence of
/// the run rather than by the system's checks alone — the verdicts the Permissions page and the
/// start announcement give a blind tester, and the words for each.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/evidence.rs"]
mod macos_evidence;

/// And the posted click's arithmetic: which refusal a point gets, and where in the window the
/// event says it is — the number a click into another program's window lands by.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/post_click.rs"]
mod macos_post_click;

/// Where a permission stands, for something that has to SHOW it rather than log it.
///
/// `Missing` and `Unknown` are constructed on macOS only, which is the whole point of the
/// type living here: the window that reads them compiles everywhere so it can be checked
/// where it is written. The allow says that out loud rather than letting a warning teach
/// somebody to stop reading them.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grant {
    Granted,
    Missing,
    /// The system would not say. Not the same as missing, and kept apart from it on purpose:
    /// telling somebody who cannot see the screen that a permission is absent, when it may be
    /// present, sends them to a settings pane to fix what is not broken.
    Unknown,
}

impl Grant {
    /// The word a screen reader says. Short, because it is read before the explanation on
    /// every visit to the list.
    pub fn word(self) -> &'static str {
        match self {
            Grant::Granted => "granted",
            Grant::Missing => "NOT granted",
            Grant::Unknown => "cannot be determined",
        }
    }
}

/// One permission, as a person needs it described.
///
/// Platform-neutral although only macOS has any, and deliberately so: it is what lets the
/// window that displays them be compiled and run on the machine this project is written on.
/// `gui.rs` cannot be borrowed by `crates/macos-check` — it needs wxWidgets — so anything
/// inside a `cfg(target_os = "macos")` block there is checked by exactly one thing, the macOS
/// CI job, and only after a push. A list that is simply empty everywhere else costs nothing
/// and moves that check back to the desk.
pub struct Permission {
    /// What the system calls it, so the name in the window matches the name in the pane.
    pub name: &'static str,
    pub state: Grant,
    /// What stops working without it — the sentence that makes it worth granting.
    pub without: &'static str,
    /// What decided `state` (and `blocking`), where more than the system's own check was
    /// weighed: a sentence, said on the page before `without` and in the start announcement
    /// while it is missing. `None` where the system's check is the whole answer.
    pub judged: Option<String>,
    /// The settings-pane URL, for the button beside it.
    pub anchor: &'static str,
    /// Whether the button beside it goes through `ask_for`, which may ask the system instead of
    /// opening the pane, or only opens the pane.
    pub can_ask: bool,
    /// Whether nothing works without it — and therefore whether its absence is worth putting
    /// a window in front of somebody who did not ask for one, and counts in the page's head.
    ///
    /// Accessibility and Screen Recording gate everything. Automation only affects which voice
    /// speaks, is asked for by the switch that wants it, and interrupting a launch over it would
    /// be pestering somebody about a feature they never turned on. Input Monitoring is optional
    /// until the run shows captured keys not reaching the overlay (`macos/evidence.rs`).
    pub blocking: bool,
}

/// The permissions this platform needs the user to grant. Empty where there are none.
#[cfg(not(target_os = "macos"))]
pub(crate) fn permissions() -> Vec<Permission> {
    Vec::new()
}

/// Opens a settings pane. False where there is no such thing to open.
#[cfg(not(target_os = "macos"))]
pub(crate) fn open_pane(_anchor: &str) -> bool {
    false
}

/// What pressing a permission's button did, and so what the page does next.
///
/// The pane is not opened by a press that has just asked: the request may have put a system
/// dialog on screen, and a settings window opened on top of it takes the focus — and a screen
/// reader — away from the dialog, which is then left somewhere its user cannot find it. That
/// press says so instead, and the next press, asking nothing, opens the pane.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Asked {
    /// Open the settings pane now.
    pub open_pane: bool,
    /// Said after the press, where there is something to say.
    pub say: Option<&'static str>,
}

impl Asked {
    /// Nothing was asked: the pane opens, and the pane speaks for itself.
    pub const PANE: Asked = Asked { open_pane: true, say: None };
}

/// Asks the system for a permission, where there is something to ask — see [`Asked`]. Where
/// there is nothing, the answer is to open the pane.
#[cfg(not(target_os = "macos"))]
pub(crate) fn ask_for(_name: &str) -> Asked {
    Asked::PANE
}

/// One running application, as `host.window.apps()` lists them and as `find` narrows its
/// search by. The same three identities a window table's `app` carries, so a matcher's `app`
/// clause tests against either without knowing which.
#[derive(Clone, Debug)]
pub struct AppInfo {
    pub pid: u32,
    pub exe: String,
    pub bundle_id: String,
}

/// One on-screen window of a process, as the window manager lists it — including the ones
/// the accessibility tree never mentions, which is the point. A plugin's popup menu that is
/// drawn as a window of its own shows up here the moment it opens and leaves the moment it
/// closes; `host.window.windowsOf` is how the overlay runtime's menu watch sees that.
#[derive(Clone, Debug)]
pub struct WindowSpot {
    /// The platform's own id: `CGWindowID` on macOS, the HWND on Windows. Stable for the
    /// life of the window, which is all the comparison needs.
    pub id: u64,
    /// The window server's level. Menus sit above ordinary windows (101 for an `NSMenu`);
    /// Windows has no such number and reports 0.
    pub layer: i32,
    /// The Win32 window class; empty on macOS, which has no such thing.
    pub class: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The window the system gives the keyboard to, as `host.window.foreground()` reports it —
/// shown or not.
///
/// Not a [`WinInfo`], on purpose. `active_window` answers "which window is in front", and a
/// window nobody can see is not that: it reads a hidden one as none, and the host keeps its
/// answer for the epoch. This answers the narrower question the other cannot: which window
/// the keys go to right now, unless a hotkey or a hook takes them first. A window can hold the
/// keyboard while it is not on screen — after Escape in Komplete Kontrol's menu the user's
/// next key was lost (2026-09-27), and the likeliest reading of that log, not measured, is
/// that KK only hid its menu's window and the key went into it — and the only way to see that
/// is a reading that does not drop hidden windows and is not kept. So: identity and visibility
/// only, as little asked as the platform allows (nothing of the application on Windows), and
/// read afresh on every call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Foreground {
    /// In the id space of [`WinInfo::hwnd`]: the HWND on Windows, the interned handle on
    /// macOS, so it compares with `host.window.active().id` and is what `focus` takes.
    pub id: isize,
    pub pid: u32,
    /// Whether the window is on screen, as far as the platform says: see `foreground_window`.
    pub shown: bool,
}

/// macOS: where the ids of this application's own windows start.
///
/// A window of another application gets its id from the accessibility handle table, a counter
/// that starts at 1 and never comes near this. A window of this application's own is answered by
/// AppKit instead (`macos/own.rs`), never by an accessibility query into this process, and its id
/// is this plus the window server's number for it — the number `windowNumber` gives in the process
/// and `CGWindowID` gives outside it. So the two id spaces cannot meet, an own id says by itself
/// which window it is, and it is the same window for as long as that window lives. 2⁴⁰ plus a
/// 32-bit number is far inside the 2⁵³ a Luau number holds exactly.
#[cfg(any(test, target_os = "macos"))]
pub(crate) const OWN_BASE: isize = 1 << 40;

/// The id of this application's own window with the window server's number `number`. `None` for
/// a number no window has: 0 or less (a window not yet ordered in has none) or past 32 bits.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn own_id(number: i64) -> Option<isize> {
    let n = u32::try_from(number).ok().filter(|&n| n != 0)?;
    Some(OWN_BASE + n as isize)
}

/// The window server's number behind an own id ([`own_id`]); `None` for any other id.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn own_number(id: isize) -> Option<u32> {
    u32::try_from(id.checked_sub(OWN_BASE)?).ok().filter(|&n| n != 0)
}

/// What one of this application's own windows is, where that changes an answer.
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnKind {
    /// A page's window (`host.gui.webview`): a window of this application placed over another
    /// application's on purpose.
    Page,
    /// Any other: the module manager, a dialog, the announcement window.
    Other,
}

/// macOS: what `host.window.ownsPoint` answers when the window drawn at the point is one of this
/// application's own, of kind `kind`. A page is drawn over the plug-in window and would take the
/// click, so `false`, as for another application's window (docs/api/gui.md, "A page over the
/// plug-in window"). Any other window of ours answers `None`, as it did before there were pages:
/// the announcement window sits over the plug-in, and a press must not be refused on its account.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn own_window_owns_point(kind: OwnKind) -> Option<bool> {
    match kind {
        OwnKind::Page => Some(false),
        OwnKind::Other => None,
    }
}

/// macOS: an AppKit frame — origin at the bottom-left of the primary display, y upwards — as the
/// whole points every coordinate crossing the [`Backend`] boundary is in: origin at the top-left
/// of that display, y downwards. `primary_h` is the primary display's height in points. Rounded
/// as accessibility rectangles are.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn cocoa_frame_top_left(x: f64, y: f64, w: f64, h: f64, primary_h: f64) -> (i32, i32, i32, i32) {
    (x.round() as i32, (primary_h - (y + h)).round() as i32, w.round() as i32, h.round() as i32)
}

/// macOS: the two ways this application hands the front to another application (`macos/front.rs`).
/// `page-check` measures both on every Mac runner CI has; which one the application uses is
/// `front::HANDING`.
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Handing {
    /// (a), the VPS Avenger control tool's: our window ordered out, this application deactivated if
    /// it is the active one, then the other application asked to become the active one. With no
    /// window of ours leaving — `host.window.focus` — nothing is deactivated: a deactivation hands
    /// the front to the application next in line first, which need not be the one asked for.
    Deactivate,
    /// (b), macOS 14's cooperative hand-over: this application, still the active one, yields its
    /// activation to the other, which is then asked to activate from it; our window is ordered out
    /// last. A request refused is asked again (a).
    Yield,
}

#[cfg(any(test, target_os = "macos"))]
impl Handing {
    /// The way, as the log and `page-check` name it.
    pub(crate) fn word(self) -> &'static str {
        match self {
            Handing::Deactivate => "(a), the control tool's way",
            Handing::Yield => "(b), yielded and then asked from this application",
        }
    }
}

/// macOS: what is known when the front is to be handed to another application, for [`hand_plan`].
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct HandState {
    /// For a window of ours giving up the keyboard (a page leaving): whether it had it — the key
    /// window, with this application the active one unless the window is a panel that does not
    /// activate it. `None` for `host.window.focus`, where no window of ours is leaving.
    pub(crate) leaving_key: Option<bool>,
    /// The application the front goes to is this one.
    pub(crate) target_ours: bool,
    /// The workspace names that application the active one. Its word can lag this application's
    /// own activation, so it counts only while this application is not the active one: two
    /// applications are never active at once ([`hand_plan`]).
    pub(crate) target_front: bool,
    /// This application is the active one ([`active_for_handing`]).
    pub(crate) ours_active: bool,
    /// The two calls of (b) exist on this macOS (14 and later).
    pub(crate) cooperative: bool,
    /// A focus asked for before is still waiting for its application's activation.
    pub(crate) pending: bool,
}

/// macOS: whether this application is the active one, for handing the front over
/// ([`HandState::ours_active`]): AppKit's word (`appkit_active`) — except while the keyboard is in a
/// panel of ours that took it without activating this application (`lent`: a page described
/// `activates = false`) and the workspace names another application the frontmost one
/// (`front_ours` false). AppKit says this application is active then all the same (`page-check`'s
/// non-activating panel, on every runner CI has), and it is not: the other application is, it
/// announces no activation when the front goes back to it, and this one has no activation to yield
/// or to give up. With the workspace naming this application, it is the active one in earnest —
/// another window of its own made it so — and AppKit's word stands.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn active_for_handing(appkit_active: bool, lent: bool, front_ours: bool) -> bool {
    appkit_active && (!lent || front_ours)
}

/// Why nothing is asked, for [`HandPlan::Keep`].
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeepWhy {
    /// Our window leaving did not have the keyboard: a module moved on first, or the user went to
    /// another application, and neither is pulled back.
    NotKey,
    /// Our window leaving still had the keyboard, but a focus asked for before waits for its
    /// application's activation: a module that brought the plug-in forward first, before the
    /// system told our window it no longer has the keyboard. That focus stands; the window it came
    /// from does not override it.
    Waiting,
    /// The window to go to is this application's own.
    Ours,
}

#[cfg(any(test, target_os = "macos"))]
impl KeepWhy {
    /// The reason, as the log says it.
    pub(crate) fn words(self) -> &'static str {
        match self {
            KeepWhy::NotKey => "our window did not have the keyboard, so the front is not ours to hand over",
            KeepWhy::Waiting => {
                "a focus asked for before waits for its application's activation, and the window it came \
                 from does not override it"
            }
            KeepWhy::Ours => "the window to go to is this application's own",
        }
    }
}

/// What handing the front to another application comes to ([`hand_plan`]).
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HandPlan {
    /// Nothing is asked; our window leaving is ordered out all the same.
    Keep(KeepWhy),
    /// The application is the active one already, so no activation will be announced: its window
    /// is raised and focused now.
    AtOnce,
    /// Its activation is asked for, this way, and its window raised and focused when the workspace
    /// says it has become the active application.
    Ask(Handing),
}

/// macOS: how the front is handed over (`macos/front.rs`), `how` being the way the application uses.
///
/// A window of ours hands the front over only while it has the keyboard — Windows' rule for a page
/// (`webview_win.rs`): a module that brought the plug-in forward first has already moved on, and a
/// user who went to another application is not pulled back. On a Mac a window learns that it lost
/// the keyboard only after the activation that took it, so a focus still waiting counts as moved
/// on as well, and is left to its activation. `host.window.focus` has no window of ours leaving; a
/// focus still waiting is then an older request, which the newer one replaces. An application in
/// front already gets its window focused at once — only while this application is not the active
/// one, since the workspace's word lags this application's own activation. (b) is asked for only
/// where it exists and this application is the active one, which is the only application that can
/// yield its activation; (a) otherwise.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn hand_plan(s: HandState, how: Handing) -> HandPlan {
    if s.leaving_key == Some(false) {
        return HandPlan::Keep(KeepWhy::NotKey);
    }
    if s.leaving_key.is_some() && s.pending {
        return HandPlan::Keep(KeepWhy::Waiting);
    }
    if s.target_ours {
        return HandPlan::Keep(KeepWhy::Ours);
    }
    if s.target_front && !s.ours_active {
        return HandPlan::AtOnce;
    }
    match how {
        Handing::Yield if s.cooperative && s.ours_active => HandPlan::Ask(Handing::Yield),
        _ => HandPlan::Ask(Handing::Deactivate),
    }
}

/// macOS: what the workspace naming an application the active one does to a focus that waits for
/// another application's activation (`macos/front.rs`).
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OnActivation {
    /// The application waited for: its window is raised and focused now.
    Focus,
    /// Another application came to the front instead: the focus is given up.
    Drop,
    /// The application waited for, but a key was pressed since the focus was asked for: the user
    /// has moved on, and the keyboard went somewhere else meanwhile, so the focus is given up and
    /// the application's own choice of window stands.
    Typed,
    /// This application's own. The workspace's notice can come after the hand-over it preceded —
    /// a page shown and hidden within a few milliseconds — so it decides nothing. This application
    /// taking the front anew gives the focus up itself (`app::activate_self`), and so does AppKit
    /// telling it in-process that it became the active one again ([`on_own_edge`]).
    Wait,
}

/// macOS: [`OnActivation`] for the workspace naming process `activated` the active application,
/// while the focus waits for process `waiting`; `ours` is this process, and `typed_since` says
/// whether the event tap saw a key pressed, not repeated, after the focus was asked for.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn on_activation(waiting: i32, activated: i32, ours: i32, typed_since: bool) -> OnActivation {
    if activated == ours {
        OnActivation::Wait
    } else if activated != waiting {
        OnActivation::Drop
    } else if typed_since {
        OnActivation::Typed
    } else {
        OnActivation::Focus
    }
}

/// macOS: AppKit's in-process word on this application, for a focus waiting for another
/// application's activation (`macos/front.rs`).
#[cfg(any(test, target_os = "macos"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OwnEdge {
    /// This application stopped being the active one.
    Resigned,
    /// This application became the active one.
    Became,
}

/// macOS: what AppKit's `edge` does to a focus waiting for another application's activation, with
/// `resigned` saying whether this application has stopped being the active one since the focus was
/// asked for: whether that is so now, and whether the focus is given up. Becoming the active one
/// again after that is the user coming back to this application, and gives it up. A becoming with
/// no stop before it gives nothing up: it was under way when the focus was asked for — this
/// application's own activation, asked just before — and the request made after it is the newer
/// one. AppKit posts both in-process, in the order they happened, which the workspace's notices,
/// from another process, do not promise.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn on_own_edge(resigned: bool, edge: OwnEdge) -> (bool, bool) {
    match edge {
        OwnEdge::Resigned => (true, false),
        OwnEdge::Became => (resigned, resigned),
    }
}

/// A snapshot of a window's matchable properties (normalized across platforms).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WinInfo {
    pub hwnd: isize,
    pub title: String,
    pub class: String,
    pub pid: u32,
    pub exe: String,
    /// The application's bundle identifier — `"com.native-instruments.Kontakt8"`. Empty
    /// where the platform has no such thing, which is everywhere but macOS.
    ///
    /// It exists because `exe` is not a stable identity there: the executable inside a
    /// bundle is named by the vendor's build system rather than by the product, so
    /// "Ableton Live 11 Suite.exe" on Windows is plain "Live" on macOS while the bundle id
    /// still says `com.ableton.live`. A matcher that wants one identity for both platforms
    /// matches `app.name`; one that wants to be exact on macOS matches this.
    pub bundle_id: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Screen coords of the client-area top-left. Overlay coordinates are
    /// relative to the client area (as in AutoHotkey's default Client mode),
    /// not the window frame.
    pub client_x: i32,
    pub client_y: i32,
    /// Size of the CLIENT area. Distinct from w/h, which are the window frame:
    /// a top-level window's frame is wider than its content by its borders, and
    /// coordinates measured from the right edge of the content (a plugin header
    /// laid out from the right) land outside it if they use w. Live: a Kontakt 8
    /// standalone window is 1026 wide around 1010 of plugin.
    pub client_w: i32,
    pub client_h: i32,
}

/// A child control (window) inside a top-level window: its class and geometry,
/// for detecting plugins embedded in a host (DAW) window.
#[derive(Clone)]
pub struct ControlInfo {
    pub hwnd: isize,
    pub class: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Screen coords of the control's client-area top-left — the overlay's
    /// coordinate origin when the plugin is embedded in a host window.
    pub client_x: i32,
    pub client_y: i32,
    /// Size of the control's CLIENT area — see WinInfo::client_w. Equal to w/h
    /// for the borderless Qt windows plugins use, but not in general.
    pub client_w: i32,
    pub client_h: i32,
}

/// A captured screen region (RGBA, row-major, top-down).
pub struct CapturedImage {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// How a read looks at the screen. Chosen per module VM from its manifest's `[screen]` table
/// (see `capture_source.rs`), never per call.
///
/// A parameter rather than a thread-local "current source" set by each binding, although
/// `docs/screen-frame-sharing-design.md` expected the Windows duplication path to fit behind
/// the unchanged `capture()` signature. The choice is per module, and the image worker needs it
/// too, on another thread; a thread-local is an implicit parameter that one forgotten reset
/// leaks into the next module's call, while this one is checked by the compiler at every call
/// site. That deviation is recorded in the design document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaptureSource {
    /// The way every module has always read the screen: GDI (`BitBlt`, `GetPixel`) on Windows;
    /// ScreenCaptureKit or CoreGraphics on macOS.
    #[default]
    Standard,
    /// Windows: DXGI Desktop Duplication. `or_standard` says what happens when it cannot
    /// answer — read the standard way instead (true), or fail the read (false, the manifest's
    /// `fallback = "none"`). Every other platform reads the standard way whatever this says.
    Duplication { or_standard: bool },
}

/// The capture routine the image worker holds ([`Backend::capture_fn`]): several regions from
/// one source, one answer per region in the same order — the picture, or why there is none.
///
/// Several rather than one, because that is what the worker has in hand — every distinct
/// region of a batch — and a duplication read of three regions is one request and one GPU
/// sync where three single calls would be three. A plain `fn`, as before, so it is `Send`
/// and the worker never touches the `Rc` backend. Named once, so that everything which passes
/// it along — the image worker, its batch runner, their tests — says `CaptureFn` and nothing
/// more.
pub type CaptureFn = fn(&[(i32, i32, i32, i32)], CaptureSource) -> Vec<Result<CapturedImage, HostError>>;

/// The first-read comparison as a thread that captures makes it ([`Backend::compare_fn`],
/// [`OcrWorker::compare`]): `who` is the module, the region the one its read asked for. `false`
/// when it could not be made yet — duplication still opening, or backing off — so a later read
/// asks again. A plain `fn`, as [`CaptureFn`] is, so no thread touches the `Rc` backend.
pub type CompareFn = fn(&str, (i32, i32, i32, i32)) -> bool;

/// The comparison of a platform with one way of reading the screen: there is nothing to compare,
/// so it is made at once.
pub fn no_comparison(_who: &str, _region: (i32, i32, i32, i32)) -> bool {
    true
}

/// Where a VM's first-read comparison stands (`capture_source.rs`), as the tests read it.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compare {
    /// To be made by the VM's next read.
    Owed,
    /// One request carries it to the thread that captures for that request.
    InFlight,
    /// Made: never again for this VM.
    Done,
}

/// A VM's first-read comparison, shared between the event loop, which hands it to a read
/// ([`CompareSlot::take`]), and the thread that makes it. Holds 0 while it is owed, `u64::MAX`
/// once it is made, and in flight the number of the one request that carries it: a request that
/// ends without making it gives it back only while it is still that request's.
#[derive(Debug)]
pub struct CompareSlot(AtomicU64);

const COMPARE_OWED: u64 = 0;
const COMPARE_DONE: u64 = u64::MAX;

/// The number of the next request to carry a comparison, for the whole process.
static NEXT_CARRIER: AtomicU64 = AtomicU64::new(1);

impl CompareSlot {
    /// A VM's comparison: owed for one that reads through desktop duplication, made for every
    /// other, which has nothing to compare.
    pub fn new(owed: bool) -> CompareSlot {
        CompareSlot(AtomicU64::new(if owed { COMPARE_OWED } else { COMPARE_DONE }))
    }

    #[cfg(test)]
    pub fn state(&self) -> Compare {
        match self.0.load(Ordering::SeqCst) {
            COMPARE_OWED => Compare::Owed,
            COMPARE_DONE => Compare::Done,
            _ => Compare::InFlight,
        }
    }

    /// Hands the comparison to a read of `region` by module `who` when it is owed. From then on it
    /// is in flight until that request makes it or ends without making it; `None` while it is in
    /// flight or made.
    pub fn take(self: &Arc<Self>, who: &str, region: (i32, i32, i32, i32)) -> Option<CompareReq> {
        let n = NEXT_CARRIER.fetch_add(1, Ordering::Relaxed);
        self.0.compare_exchange(COMPARE_OWED, n, Ordering::SeqCst, Ordering::SeqCst).ok()?;
        Some(CompareReq(Arc::new(Carried { slot: self.clone(), n, who: who.to_string(), region })))
    }
}

/// The comparison one request carries to the thread that captures for it ([`CompareSlot::take`]).
/// Cloned with the request it travels in, and settled once: by [`CompareReq::make`] or
/// [`CompareReq::settle`]. When the last clone goes without that — a read superseded, refused or
/// joined to another, a snapshot cancelled, a search ended — it is owed again, for the module's
/// next read.
#[derive(Clone, Debug)]
pub struct CompareReq(Arc<Carried>);

#[derive(Debug)]
struct Carried {
    slot: Arc<CompareSlot>,
    n: u64,
    who: String,
    region: (i32, i32, i32, i32),
}

impl CompareReq {
    /// Makes the comparison through `compare` on the calling thread: the thread about to capture
    /// for the request, before it does. Nothing when it is no longer this request's to make. One
    /// that panics counts as made, since it would fail the same way at every read.
    pub fn make(&self, compare: CompareFn) {
        let c = &*self.0;
        if c.slot.0.load(Ordering::SeqCst) != c.n {
            return;
        }
        let made = match crate::logging::contain(|| compare(&c.who, c.region)) {
            Ok(made) => made,
            Err(report) => {
                crate::logging::line(
                    "capture",
                    &format!("[{}] the first-read comparison failed inside the application and is not made again: {report}", c.who),
                );
                true
            }
        };
        self.settle(made);
    }

    /// Records whether it was made: done for good, or owed again for the module's next read.
    pub fn settle(&self, made: bool) {
        let c = &*self.0;
        let to = if made { COMPARE_DONE } else { COMPARE_OWED };
        let _ = c.slot.0.compare_exchange(c.n, to, Ordering::SeqCst, Ordering::SeqCst);
    }
}

impl Drop for Carried {
    fn drop(&mut self) {
        // Given back only while it is still this request's: made, or carried by a later request,
        // it stays as it is.
        let _ = self.slot.0.compare_exchange(self.n, COMPARE_OWED, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// The error of a capture the standard path could not make — and it cannot say more: an empty
/// region, one too large to allocate for, or a read the operating system refused
/// (`screen::CAPTURE_FAILED`, `failed`). Every capture error is what a module is told: the error
/// value `pixel`, `profile`, the image searches, `save` and the cells calls return beside their
/// `nil` or `false` — the capture errors of `docs/api/screen.md` — and its message begins with
/// these words; a `fallback = "none"` module's for a read desktop duplication could not answer
/// begins with [`DUPLICATION_UNANSWERED`]. A text read answers each of them as a reading with
/// `status = "failed"` and this `error`, never a raise.
pub fn capture_failed() -> HostError {
    HostError::of(&crate::errors::screen::CAPTURE_FAILED)
}

/// The first words of the capture error a `fallback = "none"` module is told when desktop
/// duplication had no picture to give — routine there, at every fullscreen switch and UAC prompt;
/// with the default fallback the standard path answers instead. Each such error's Reason is its
/// tail's (`dxgi::Fallback::reason`).
pub const DUPLICATION_UNANSWERED: &str = "screen capture failed: desktop duplication could not answer";

/// Tells the Windows duplication engine a module that reads through it has just come to the
/// front, so the first read does not also pay for opening it. Nothing anywhere else, and
/// nothing on Windows either unless duplication is switched on and has not given up.
pub fn prewarm_capture() {
    #[cfg(windows)]
    dxgi::prewarm();
}

/// What the duplication path did since the last call — reads, microseconds spent in them,
/// and reads it could not answer — for the observation log line. Zeros where it does not
/// exist, and zeros on Windows while no module has asked for it.
pub fn take_duplication_counters() -> (u64, u64, u64) {
    #[cfg(windows)]
    let c = dxgi::take_counters();
    #[cfg(not(windows))]
    let c = (0, 0, 0);
    c
}

/// Stops the duplication engine's thread, if it was ever started, before the process exits —
/// a thread still inside a graphics driver call while the process tears down is the same
/// class of fault as the OCR warm-up race `warmup_ocr` documents. Bounded: it waits half a
/// second and then leaves the thread to the process exit.
pub fn shutdown_capture() {
    #[cfg(windows)]
    dxgi::shutdown();
}

/// A recognized word with its bounding box (in capture-region coordinates).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcrWord {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// One line as the engine returned it — Windows' `OcrLine`, one Vision observation — with its
/// words in capture-region coordinates. What `host.ocr.recognize` groups into rows
/// (`ocr::pipeline::rows`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OcrLine {
    pub text: String,
    pub words: Vec<OcrWord>,
}

/// OCR result: the full recognized text plus per-word boxes.
pub struct OcrText {
    pub text: String,
    pub words: Vec<OcrWord>,
    /// The engine's lines, the same words grouped as it grouped them. Empty when the fallback
    /// recogniser answered, which locates nothing.
    pub lines: Vec<OcrLine>,
    /// Set when Windows' fallback recogniser answered instead of the system one: the content
    /// crop it read, `(x, y, w, h)` in capture-region coordinates. `text` is its answer and
    /// `words` and `lines` are empty; `host.ocr.recognize` shares the crop out among the text's
    /// tokens as approximate boxes.
    pub fallback: Option<(i32, i32, i32, i32)>,
    /// The blank guard answered instead of the engine: the region's content crop found no
    /// ink, so the recogniser was never asked, and the empty `text` and `words` are the
    /// guard's answer rather than a reading. It rides on the result because the only other
    /// record of it is a log line, and an instrument reading a blank region cannot tell
    /// "the engine found nothing" from "the engine was not asked" without it — the probe
    /// printed the same "nothing invented" verdict for both. False for every other empty
    /// result: a failed capture or a zero-sized region is a different fault, and the log
    /// names those on its own.
    pub skipped: bool,
}

/// What `host.ocr.recognize` says when the platform has no recogniser at all: `not_supported`.
pub fn no_recogniser() -> HostError {
    HostError::of(&crate::errors::ocr::NO_RECOGNISER)
}

/// The pixels the OCR capture stage took for one read, handed to the recognise stage. `Send`,
/// because it crosses from the one thread to the other; what it holds is the platform's own.
#[cfg(windows)]
pub type OcrShot = Vec<Result<CapturedImage, HostError>>;
#[cfg(target_os = "macos")]
pub type OcrShot = macos::ocr::Shot;
#[cfg(not(any(windows, target_os = "macos")))]
pub type OcrShot = ();

/// The snapshot rounds' capture routine ([`OcrWorker::frames`]): several regions from one
/// source, one frame per region in the same order — or why there is none — recording which path
/// answered each. `poll` is true when every request of the round is a change wait, which asks
/// again and again: macOS then skips its flat-picture watch. A plain `fn`, so it is `Send`.
pub type FramesFn = fn(&[(i32, i32, i32, i32)], CaptureSource, bool) -> Vec<Result<frame::Frame, HostError>>;

/// The pixel reads' capture routine ([`OcrWorker::points`]), for a handler that waits and for a
/// callback: the colours at the points, in order, read from one source on the capture thread —
/// `single`, each point as `host.screen.pixel` reads one (`GetPixel` on the Windows standard
/// path), and otherwise all of them as `host.screen.pixels` reads a list, from as few captures as
/// [`frame::plan_points`] allows. All or nothing, with the reason. A plain `fn`, so it is `Send`.
pub type PointsFn = fn(&[(i32, i32)], CaptureSource, bool) -> Result<Vec<(u8, u8, u8)>, HostError>;

/// The recognise stage's routine ([`OcrWorker::recognise`]): each region of a capture, one answer
/// per region in order — its text, or why it could not be read, which its reading's `error`
/// carries. A plain `fn`, so it is `Send`.
pub type RecogniseFn<S> = fn(&S, &[(i32, i32, i32, i32)], &Recognise) -> Vec<Result<OcrText, HostError>>;

/// Which of the two OCR threads is starting — see [`OcrWorker::init_thread`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcrThread {
    Capture,
    Recognise,
}

/// What one recognition is asked for, beside the pixels.
///
/// `fast_ok` and `preempt` steer the macOS ladder; Windows has no optional pass to skip, and
/// reads neither. `stop` and `region_done` are for both, through [`Recognise::each`].
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct Recognise<'a> {
    /// The language as the platform lists it, already resolved (`ocr::lang`). `None` is the
    /// engine's own default; a read always names one.
    pub lang: Option<&'a str>,
    /// Whether the fast model reads `lang` too (macOS; its last rung is skipped when not).
    pub fast_ok: bool,
    /// For a background read: set while interactive work waits, and a recogniser with optional
    /// extra passes skips them (macOS's bigger-then-faster rungs). `None` for interactive reads.
    pub preempt: Option<&'a std::sync::atomic::AtomicBool>,
    /// Set when the application is closing: the regions not started yet are answered
    /// [`closing`] without being read. Without it a job of 64 regions went on recognising —
    /// and on Windows kept starting neural recognitions — after the exit had stopped waiting
    /// for it, which is the fault the exit's waits are there to prevent.
    pub stop: Option<&'a std::sync::atomic::AtomicBool>,
    /// Called after each region is answered: the service's hang guard counts from the last
    /// region answered, not from the start of the job.
    pub region_done: Option<&'a dyn Fn()>,
}

/// What a region is answered when the application closed before it was read: `cancelled`, which
/// no module is handed, since no callback runs after the exit has begun.
pub fn closing() -> HostError {
    HostError::of(&crate::errors::ocr::CLOSING)
}

impl Recognise<'_> {
    /// Answers `items` one region at a time, in order, through `read` — every recogniser's loop,
    /// so that the exit and the hang guard see each region: a region is not started once
    /// `stop` is set, and `region_done` is told after each one that was.
    #[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
    pub fn each<T>(
        &self,
        items: impl IntoIterator<Item = T>,
        mut read: impl FnMut(T) -> Result<OcrText, HostError>,
    ) -> Vec<Result<OcrText, HostError>> {
        items
            .into_iter()
            .map(|item| {
                if self.stop.is_some_and(|s| s.load(std::sync::atomic::Ordering::Acquire)) {
                    return Err(closing());
                }
                let answer = read(item);
                if let Some(done) = self.region_done {
                    done();
                }
                answer
            })
            .collect()
    }
}

/// The OCR threads' view of the platform: bare functions, so neither thread ever touches the
/// `Rc` backend — the same arrangement as [`CaptureFn`] for the image worker. Generic over the
/// pixels so the service can be tested with a fake; the platform's is [`OcrShot`].
pub struct OcrWorker<S = OcrShot> {
    /// Whether this platform has a text recogniser at all.
    pub present: bool,
    /// Runs first on each of the two threads: WinRT's apartment on Windows, the recognise
    /// thread's quality of service on macOS.
    pub init_thread: fn(OcrThread),
    /// Runs on the recognise thread once it has read the languages, before its first job, with
    /// the language a read without `lang` is made in: the recogniser's first pass, made here so
    /// that the first read does not pay for it, and said in the log. On the hang clock, like a
    /// job. On macOS one Vision pass over a line of printed words, after the warm-up on a thread
    /// of its own has ended (`ocr::warm_up_recognise`); nothing on Windows.
    pub warm_up: fn(Option<&str>),
    /// Photographs `regions` through the source, at once: all of them from one moment. Returns
    /// the pixels and roughly how many bytes they hold. One failed region fails in its own slot.
    pub capture: fn(&[(i32, i32, i32, i32)], CaptureSource) -> (S, usize),
    /// Recognises each region of a capture ([`RecogniseFn`]).
    pub recognise: RecogniseFn<S>,
    /// The languages the engine reads and the user prefers. Called on the recognise thread.
    pub languages: fn() -> crate::ocr::lang::Languages,
    /// Photographs a snapshot round, on the capture thread: `host.screen.snapshot`'s pictures
    /// and change waits share the thread with the text reads' captures.
    pub frames: FramesFn,
    /// Reads the points a handler waits for in `host.screen.pixel` or `pixels`, on the capture
    /// thread, through the functions those calls read with on the event loop ([`PointsFn`]) —
    /// never through `frames`, whose pictures on a Mac are taken at the display's own resolution.
    pub points: PointsFn,
    /// Makes a module's first-read comparison, on the capture thread, before the capture of the
    /// read or the snapshot that carries it ([`CompareReq`]).
    pub compare: CompareFn,
    /// Which display the screen point lies on, as a number two points on one display share: the
    /// capture thread never plans one capture of regions on two displays, which on macOS can
    /// differ in scale. 0 for every point where one capture may span displays (Windows), and
    /// on macOS for a point on none. On the capture thread, under its lock: cheap.
    pub display_of: fn(i32, i32) -> u32,
    /// The pixels a read of a snapshot hands the recogniser: the parts of `frame` under the
    /// regions — which lie inside [`frame::Frame::ocr_rect`] — in the shape `recognise` takes, as
    /// if they had just been captured. On the recognise thread; nothing is captured.
    pub shot_of: fn(&frame::Frame, &[(i32, i32, i32, i32)]) -> S,
}

impl<S> Clone for OcrWorker<S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S> Copy for OcrWorker<S> {}

impl OcrWorker {
    /// The worker of a platform without a recogniser: nothing is captured, every region fails,
    /// and so does every snapshot the capture thread would take. Its points are answered as the
    /// one such platform answers them on the event loop (`stub.rs`): a single point black, a list
    /// a failed capture.
    pub fn none() -> OcrWorker {
        OcrWorker {
            present: false,
            init_thread: |_| {},
            warm_up: |_| {},
            capture: |_, _| (OcrShot::default(), 0),
            recognise: |_, regions, _| regions.iter().map(|_| Err(no_recogniser())).collect(),
            languages: crate::ocr::lang::Languages::default,
            frames: |regions, _, _| regions.iter().map(|_| Err(capture_failed())).collect(),
            points: |pts, _, single| if single { Ok(vec![(0, 0, 0); pts.len()]) } else { Err(capture_failed()) },
            compare: no_comparison,
            display_of: |_, _| 0,
            shot_of: |_, _| OcrShot::default(),
        }
    }
}

/// One element of an accessibility dump: what it is, what it is called, and WHERE it is.
///
/// The rectangle was added after the first real macOS session. A dump without it says a
/// plugin has a slider and does not say where — which is half an answer when the whole
/// purpose of the dump is to let someone author coordinates for a machine they cannot see.
/// It also measures things nothing else can: the first probe of a plugin showed every
/// Windows-authored coordinate sitting exactly one title bar too high on macOS, and the
/// window's own close button is the element that says how tall that title bar is.
///
/// Screen coordinates, and `(0, 0, 0, 0)` for an element with no rectangle of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DumpNode {
    pub depth: i32,
    pub name: String,
    pub class: String,
    pub ctype: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Where [`Backend::element_focus_step`] left the keyboard: the element the focus landed on.
///
/// A struct rather than the tuple it was, because it gained a fifth field that is itself
/// optional, and `(name, ctype, index, count, bounds)` is the kind of tuple that gets two of
/// its integers swapped at a call site without a compiler noticing.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusStep {
    /// The element's name as the platform publishes it (UIA's Name; `AXTitle`, else
    /// `AXDescription`). Often empty: a plug-in's own controls are mostly drawn, not labelled.
    pub name: String,
    pub ctype: i32,
    /// 1-based, and stable for an element while the ring keeps its shape.
    pub index: i32,
    pub count: i32,
    /// The landed element's rectangle — see [`FocusBounds`].
    pub bounds: Option<FocusBounds>,
}

/// The rectangle of the element a focus step landed on, in screen coordinates: the space
/// `host.ocr.recognize` regions and `host.input.click` use on the platform — pixels on Windows,
/// points on macOS (which may carry a fraction there; the caller rounds).
///
/// It exists so that a stop that publishes NO NAME can still be told apart: the one thing the
/// accessibility tree does say about it is where it is. In a Mac tester's log of 2026-09-27,
/// Kontakt 7 standalone's ring of 29 stops was mostly stops with no name — FILE, LIBRARY, VIEW,
/// SHOP, Search, Brand, Sound Type and Character were the ones with one — so the screen reader
/// said a role and little else, and the overlay runtime now reads such a stop's text off the
/// screen in this rectangle. Nothing here knows about that: it is the element's geometry, for
/// any caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusBounds {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl FocusBounds {
    /// The rectangle, or `None` when there is none worth handing on: an empty one (`w` or `h`
    /// not above zero — a collapsed element, or UIA's answer for one with no place on screen)
    /// or one whose numbers are not finite. One rule for every backend, so `nil` means the same
    /// on both systems.
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Option<FocusBounds> {
        let finite = x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite();
        if !finite || w <= 0.0 || h <= 0.0 {
            return None;
        }
        Some(FocusBounds { x, y, w, h })
    }
}

/// Mouse button for input synthesis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

// What a click for a `window` is refused with, on both platforms, is a Reason of errors.rs: the
// window gone (`common::WINDOW_GONE`), the point outside it (`input::OUTSIDE`), on its frame
// (`input::FRAME` — a posted click is a click in a window's client area: on its frame or title bar
// a real click is something else, another message on Windows, a move or a zoom of the window on a
// Mac, which nothing posts), or under another window (`input::COVERED`); and a way of acting a
// platform does not have yet answers `common::NOT_YET`.

/// A piece of work a backend hands the host for a thread of the host's own, because it may wait
/// for another program — a window's style changed across processes — and so must never run on
/// the event loop. `Ok` when it was done; `Err` with what a module is told. Runs once, on a thread
/// with nothing else of the host's on it.
pub type ActJob = Box<dyn FnOnce() -> Result<(), HostError> + Send>;

/// Why `host.input.post` or `host.input.send` sent nothing: a mistake in the call — a spec that
/// does not parse, a key with no name, a modifier tap — which raises with its sentence, or what
/// only the moment decides, which the call answers as `false, err`. A sentence converts into a
/// mistake, so a parser's `?` raises as it always did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyRefusal {
    Mistake(String),
    Answer(HostError),
}

impl From<String> for KeyRefusal {
    fn from(why: String) -> Self {
        KeyRefusal::Mistake(why)
    }
}

/// The sentence of either: what a log line or a dialog says of it.
impl std::fmt::Display for KeyRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyRefusal::Mistake(why) => f.write_str(why),
            KeyRefusal::Answer(e) => write!(f, "{e}"),
        }
    }
}

/// A question to a window's accessibility tree: one `host.element` call, with its arguments.
///
/// One type for every call, so that the host treats them alike: hands them to the thread that asks
/// them (`element.rs`), lets two that are the same share one answer, keeps an answer for the epoch.
/// Plain data, and nothing in it holds anything of the window.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ElementAsk {
    /// `host.element.find`.
    Find { hwnd: isize, name: String, ctype: i32 },
    /// `host.element.findAny`.
    FindAny { hwnd: isize, names: Vec<String>, types: Vec<i32> },
    /// `host.element.locate`.
    Locate { hwnd: isize, name: String, ctype: i32 },
    /// `host.element.locateVia`.
    LocateVia { hwnd: isize, via_name: String, via_type: i32, name: String, ctype: i32 },
    /// `host.element.pluginLocate`.
    PluginLocate { hwnd: isize, container: String, name: String, ctype: i32 },
    /// `host.element.stateProbe`.
    StateProbe { hwnd: isize, container: String, name: String, ctype: i32 },
    /// `host.element.dump`.
    Dump { hwnd: isize },
    /// `host.element.rawDump`.
    RawDump { hwnd: isize },
    /// `host.element.classNavPoint`.
    ClassNavPoint { hwnd: isize, class: String, ctype: i32, child: i32, sibling: i32 },
    /// `host.element.focusStep`.
    FocusStep { hwnd: isize, direction: i32 },
    /// `host.element.focusWithin`.
    FocusWithin { hwnd: isize, x: i32, y: i32, w: i32, h: i32 },
    /// `host.element.press`.
    Press { hwnd: isize, name: String, ctype: i32 },
    /// The host's own, never a module's: a landing of `host.window.focus` puts the keyboard on
    /// window `hwnd`, a control, and reads it back (`focus_landing.rs`, HW1).
    FocusPut { hwnd: isize },
}

impl ElementAsk {
    /// The window it is about.
    pub fn hwnd(&self) -> isize {
        match self {
            ElementAsk::Find { hwnd, .. }
            | ElementAsk::FindAny { hwnd, .. }
            | ElementAsk::Locate { hwnd, .. }
            | ElementAsk::LocateVia { hwnd, .. }
            | ElementAsk::PluginLocate { hwnd, .. }
            | ElementAsk::StateProbe { hwnd, .. }
            | ElementAsk::Dump { hwnd }
            | ElementAsk::RawDump { hwnd }
            | ElementAsk::ClassNavPoint { hwnd, .. }
            | ElementAsk::FocusStep { hwnd, .. }
            | ElementAsk::FocusWithin { hwnd, .. }
            | ElementAsk::Press { hwnd, .. }
            | ElementAsk::FocusPut { hwnd } => *hwnd,
        }
    }

    /// Whether it acts on the program — presses an element, moves its keyboard focus — rather than
    /// read: such a question is put every time it is asked, never shared with another or answered
    /// from what an earlier one found.
    pub fn acts(&self) -> bool {
        matches!(
            self,
            ElementAsk::FocusStep { .. } | ElementAsk::FocusWithin { .. } | ElementAsk::Press { .. } | ElementAsk::FocusPut { .. }
        )
    }

    /// The call's name in the module's words: `"findAny"`.
    pub fn call_name(&self) -> &'static str {
        match self {
            ElementAsk::Find { .. } => "find",
            ElementAsk::FindAny { .. } => "findAny",
            ElementAsk::Locate { .. } => "locate",
            ElementAsk::LocateVia { .. } => "locateVia",
            ElementAsk::PluginLocate { .. } => "pluginLocate",
            ElementAsk::StateProbe { .. } => "stateProbe",
            ElementAsk::Dump { .. } => "dump",
            ElementAsk::RawDump { .. } => "rawDump",
            ElementAsk::ClassNavPoint { .. } => "classNavPoint",
            ElementAsk::FocusStep { .. } => "focusStep",
            ElementAsk::FocusWithin { .. } => "focusWithin",
            ElementAsk::Press { .. } => "press",
            ElementAsk::FocusPut { .. } => "focusPut",
        }
    }

    /// What it asks about, for the log: the names a search looks for, the container and the
    /// element, the class; empty for a dump and a focus step.
    pub fn detail(&self) -> String {
        match self {
            ElementAsk::Find { name, .. } | ElementAsk::Locate { name, .. } | ElementAsk::Press { name, .. } => name.clone(),
            ElementAsk::FindAny { names, .. } => names.join("/"),
            ElementAsk::LocateVia { via_name, name, .. } => format!("{via_name} > {name}"),
            ElementAsk::PluginLocate { container, name, .. } | ElementAsk::StateProbe { container, name, .. } => {
                format!("{container} > {name}")
            }
            ElementAsk::ClassNavPoint { class, .. } => class.clone(),
            ElementAsk::Dump { .. }
            | ElementAsk::RawDump { .. }
            | ElementAsk::FocusStep { .. }
            | ElementAsk::FocusWithin { .. }
            | ElementAsk::FocusPut { .. } => String::new(),
        }
    }

    /// What a tree with nothing in it answers — and for a press, a platform that cannot press:
    /// the stub backend's answer to every question.
    pub fn nothing(&self) -> ElementOutcome {
        Ok(match self {
            ElementAsk::Find { .. } => ElementAnswer::Found(false),
            ElementAsk::FindAny { .. } => ElementAnswer::Index(None),
            ElementAsk::Locate { .. }
            | ElementAsk::LocateVia { .. }
            | ElementAsk::PluginLocate { .. }
            | ElementAsk::ClassNavPoint { .. } => ElementAnswer::Point(None),
            ElementAsk::StateProbe { .. } => ElementAnswer::State(None),
            ElementAsk::Dump { .. } | ElementAsk::RawDump { .. } => ElementAnswer::Nodes(Vec::new(), None),
            ElementAsk::FocusStep { .. } => ElementAnswer::Step(None),
            ElementAsk::FocusWithin { .. } => ElementAnswer::Within(None),
            ElementAsk::FocusPut { .. } => ElementAnswer::Put(false),
            ElementAsk::Press { .. } => return Err(ElementFailure { why: HostError::of(&common::NOT_YET), unanswered: false }),
        })
    }
}

/// What an accessibility tree answered to an [`ElementAsk`].
#[derive(Clone, Debug, PartialEq)]
pub enum ElementAnswer {
    /// `find`: whether there is such an element.
    Found(bool),
    /// `findAny`: the 1-based index of the name found, or none.
    Index(Option<usize>),
    /// `locate`, `locateVia`, `pluginLocate`, `classNavPoint`: the element's centre on screen, or
    /// none — not found, or it has no rectangle there.
    Point(Option<(i32, i32)>),
    /// `stateProbe`: the Toggle state (-1 for an element without one) and the legacy state word,
    /// or none when the element is not there.
    State(Option<(i32, i32)>),
    /// `dump`, `rawDump`: what the walk found, and — when it stopped before the end because the
    /// program did not answer one of its questions, or at a bound — why, so that a part of the tree
    /// does not read as all of it: a partial answer.
    Nodes(Vec<DumpNode>, Option<HostError>),
    /// `focusStep`: where the keyboard focus landed, or none.
    Step(Option<FocusStep>),
    /// `focusWithin`: the name and control type of the element that took the focus, or none.
    Within(Option<(String, i32)>),
    /// `press`: the program accepted the action.
    Pressed,
    /// The host's `FocusPut`: whether the keyboard is on the control, or inside it, afterwards.
    Put(bool),
}

impl ElementAnswer {
    /// Whether it says that nothing is there — what a question the program did not answer in full
    /// must not be read as (see [`ElementFailure`]).
    pub fn is_nothing(&self) -> bool {
        matches!(
            self,
            ElementAnswer::Found(false)
                | ElementAnswer::Index(None)
                | ElementAnswer::Point(None)
                | ElementAnswer::State(None)
                | ElementAnswer::Step(None)
                | ElementAnswer::Within(None)
                | ElementAnswer::Put(false)
        )
    }
}

/// A question that got no answer: what is not known is not "not there". The program did not
/// answer in time, refused (it runs with higher rights), went away while it was asked — or, for a
/// press, did not take the action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementFailure {
    /// What a module is told: `the program did not answer within 1000 ms`, code `timeout`.
    pub why: HostError,
    /// The program did not answer within the bound the platform sets (UI Automation's timeout):
    /// the questions to the same program that waited behind this one are not put to it.
    pub unanswered: bool,
}

/// An [`ElementAsk`]'s outcome.
pub type ElementOutcome = Result<ElementAnswer, ElementFailure>;

/// How a platform that asks accessibility questions on a thread of their own answers them there —
/// see [`Backend::element_runner`]. Plain functions, so that the host's thread can hold them.
#[derive(Clone, Copy)]
pub struct ElementRunner {
    /// Called once, on that thread, before its first question: on Windows it enters the
    /// multithreaded COM apartment, which is where Microsoft wants a UI Automation client.
    pub init: fn(),
    /// Answers one question, on that thread.
    pub run: fn(&ElementAsk) -> ElementOutcome,
    /// The process that answers questions about window `hwnd` — 0 for a window that is gone. Cheap
    /// and local, asked on the event loop for every question.
    pub process_of: fn(isize) -> u32,
}

/// Which windows an event gives their activation back to — see
/// [`Backend::give_back_activation`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GiveBackWhen {
    /// Module `idx` stopped running: the windows held for its clicks and no other module's.
    Stopped(usize),
    /// Window `hwnd` came to the front: the windows held while another was in front.
    CameForward(isize),
}

/// OS-level operations the host needs. Lua-agnostic on purpose: callback/state
/// mapping stays in the host; only OS specifics live behind this trait.
pub trait Backend {
    /// What this machine looks like from the backend's side, as name/value pairs, written
    /// to the log once at startup.
    ///
    /// This exists for the failures we cannot see. The platform is used by a blind person,
    /// increasingly on a machine nobody here owns, and the interesting faults are
    /// environmental: a permission that was never granted, a display scale that makes every
    /// coordinate half of what it should be, a screen reader that is not running. Each of
    /// those otherwise arrives as "it does not work" with no way back to a cause. Asked
    /// once, so it may take its time and prompt nothing.
    fn environment(&self) -> Vec<(String, String)>;

    fn enumerate_windows(&self) -> Vec<WinInfo>;

    /// The windows of these processes only.
    ///
    /// The unfiltered listing asks every running application a question, and on macOS that
    /// is a cross-process call per application with a timeout each; one wedged process
    /// costs the whole timeout, on the thread that carries the event tap. The third Mac
    /// session put a number on it: 2.0-2.3 s of a 3 s stall per press was this listing,
    /// every press, because some unrelated application never answered. A matcher that
    /// names an application — and every shipped one does — has no business asking the
    /// others, so `find`/`findAll` resolve the named ones through `running_apps` (local,
    /// cheap) and come here with the pids. The default filters the full listing, which is
    /// right where enumeration is local (Windows); the macOS backend overrides it.
    fn enumerate_windows_of(&self, pids: &[u32]) -> Vec<WinInfo> {
        self.enumerate_windows().into_iter().filter(|w| pids.contains(&w.pid)).collect()
    }

    /// Every running application, without asking any of them anything.
    ///
    /// Local on every platform: the workspace's own list on macOS, and on Windows the
    /// owners of the visible top-level windows, which is the same set of processes a window
    /// search could ever find anything in. What a matcher's `app` clause is tested against
    /// before any window is asked for.
    fn running_apps(&self) -> Vec<AppInfo> {
        let mut out: Vec<AppInfo> = Vec::new();
        for w in self.enumerate_windows() {
            if !out.iter().any(|a| a.pid == w.pid) {
                out.push(AppInfo { pid: w.pid, exe: w.exe.clone(), bundle_id: w.bundle_id.clone() });
            }
        }
        out
    }

    /// Every on-screen window this process owns, from the window manager rather than from
    /// accessibility — see [`WindowSpot`]. Cheap (one system-wide list, no cross-process
    /// call) and asked by the overlay runtime's `newWindow` menu test after a menu-opening click.
    fn windows_of(&self, _pid: u32) -> Vec<WindowSpot> {
        Vec::new()
    }

    /// The captured keys the hook let through to the application because a menu was open,
    /// since the last call, as they were filed for module `owner`. Drained on read. Return and
    /// Escape are what the overlay runtime wants: they are the keys that end a menu, and where
    /// nothing can see the menu itself, one of them going through is the best available word
    /// that it is closing.
    ///
    /// A key let through by a menu flag is filed for every module whose flag counted for the
    /// window in front ([`menu_flag_owners`]); one let through by a menu the system drew, for
    /// the module that would have taken it.
    fn take_menu_pass_through(&self, _owner: u32) -> Vec<(u32, u8)> {
        Vec::new()
    }

    /// Drops the captured keys and hotkey presses queued for the event loop and not delivered
    /// yet, and returns them: the host's guard stopped a callback that held the loop, and what
    /// was pressed meanwhile must not reach whichever module takes the key next, all at once
    /// (`stops::mark`). A press already taken into the pump's batch before the stall began still
    /// arrives. On Windows a `RegisterHotKey` press made during the stall, which reaches the
    /// queue only after it, is dropped too as it arrives, by its time stamp
    /// ([`HostEvents::on_stall_hotkeys_dropped`]). Nothing on a platform whose queue cannot fill
    /// during a stall: on macOS the tap runs on the stalled thread itself.
    fn drop_queued_input(&self) -> DroppedInput {
        DroppedInput::default()
    }

    fn active_window(&self) -> Option<WinInfo>;

    /// The window that gets the keyboard now, shown or not — see [`Foreground`].
    ///
    /// Asked afresh on every call and never kept for the epoch: the one thing it exists to
    /// tell is a change that has no event of its own (a window hidden while it keeps the
    /// foreground). So it has to be cheap enough to ask whenever a caller wonders, and it asks
    /// as little as the platform allows:
    ///
    /// - **Windows:** `GetForegroundWindow`, `GetWindowThreadProcessId`, `IsWindowVisible`,
    ///   `IsIconic` and DWM's cloaked attribute — all local.
    /// - **macOS:** the frontmost application's `AXFocusedWindow` and that window's
    ///   `AXMinimized`, two reads of the application that answers them; `None` without asking
    ///   while it is in the not-answering quarantine, and `None` when a read times out.
    ///
    /// `None`: no window has the keyboard, or it cannot be said which.
    ///
    /// No default body, like `active_window`: a backend that forgot it would answer `None`
    /// forever, which reads exactly like "no window has the keyboard" and fails no test.
    fn foreground_window(&self) -> Option<Foreground>;

    /// Brings a window to the front and gives it the keyboard.
    ///
    /// Missing until now, and its absence had a name: on Windows a screen-reader user gets
    /// back to a plugin's window with OSARA's F6, and macOS has no equivalent — VoiceOver
    /// offers no command for it, so a plugin window opened inside a DAW can be genuinely
    /// hard to return to. Nothing in this platform could offer one either, because nothing
    /// here could focus a window.
    ///
    /// `Ok` when the system accepted it; otherwise why not: declined by the system (`refused`), no
    /// such window (`gone`), a hidden window of this application (`hidden`). Both platforms can
    /// refuse — Windows will not let an application steal the foreground under some conditions,
    /// and on macOS the window's own application has to be activated as well as the window raised
    /// — so the answer is reported rather than assumed. The default is a platform without it.
    fn focus_window(&self, id: isize) -> Result<(), HostError> {
        let _ = id;
        Err(HostError::of(&common::NOT_YET))
    }

    /// The keyboard landings of `host.window.focus` (HW1, `focus_landing.rs`): the puts they want
    /// made since the last call, each `(landing, control)` — asked on the accessibility thread by
    /// the host, never on the event loop. Windows only; none elsewhere.
    fn take_focus_puts(&self) -> Vec<(u64, isize)> {
        Vec::new()
    }

    /// The answer to one of [`take_focus_puts`](Self::take_focus_puts)'s puts: whether the keyboard
    /// is on the control afterwards, or why it could not be asked.
    fn focus_put_answered(&self, landing: u64, took: Result<bool, String>) {
        let _ = (landing, took);
    }

    /// A module made a `host.input` call: a landing of `host.window.focus` under way ends, so that
    /// it never moves the keyboard after something acted.
    fn input_made(&self) {}

    /// Is `hwnd`'s window the one lying under this screen point?
    ///
    /// Every hotspot in this project clicks a screen coordinate that was worked out from a
    /// window's own frame, and until now nothing checked that the window was still the thing
    /// on top there. A notification, a tooltip, another application raised over ours: the
    /// click goes to whichever window owns the pixel, and the overlay says "activated"
    /// regardless. For somebody who cannot see the screen that is a press with no way to tell
    /// where it landed.
    ///
    /// `None` means the platform cannot say, and is deliberately not `false`: a backend that
    /// has no answer must not stop a click that would have worked. Only a definite "that
    /// belongs to somebody else" refuses.
    fn window_owns_point(&self, _hwnd: isize, _x: i32, _y: i32) -> Option<bool> {
        None
    }

    /// The same question about a window as [`windows_of`](Self::windows_of) lists it — its
    /// [`WindowSpot::id`], which is not a window handle on every platform. A popup a plug-in
    /// draws as a window of its own is known to the overlay runtime only from that list, and
    /// clicking an item in it wants to know that nothing is drawn over the item. The same three
    /// answers, and `None` from a backend that cannot say.
    fn listed_window_owns_point(&self, _id: u64, _x: i32, _y: i32) -> Option<bool> {
        None
    }

    /// Whether a click at (x, y) may be made for `window` — `host.input.click`'s `window`: the
    /// window is there, the point is in it, and nothing else is drawn over it. `Err` with what the
    /// module is told. Asks nothing of any program, so it is cheap on the event loop.
    ///
    /// The default goes by [`window_owns_point`](Self::window_owns_point) and refuses only its
    /// definite "no", with `input::COVERED`; Windows says which of three things it is.
    fn click_check(&self, window: isize, x: i32, y: i32) -> Result<(), HostError> {
        match self.window_owns_point(window, x, y) {
            Some(false) => Err(HostError::of(&input::COVERED)),
            _ => Ok(()),
        }
    }

    /// `host.input.click`'s `via = "post"`: the click posted to the window a real click at (x, y)
    /// reaches, when that is `window` or a window inside it — as messages to that window in its own
    /// client coordinates on Windows, as events handed to its process with the window's number and
    /// the place in it on a Mac — with the pointer moved there first unless `move_pointer` is false.
    /// Nothing comes to the front. On the event loop: it never waits for the program to take the
    /// click (on a Mac the window's place may be an accessibility read, bounded as every one is).
    /// `Ok` says it was posted, not that the program did anything with it; one line in the log
    /// either way. The default is a platform without it, `common::NOT_YET`.
    fn mouse_click_posted(
        &self,
        _window: isize,
        _x: i32,
        _y: i32,
        _button: MouseButton,
        _move_pointer: bool,
    ) -> Result<(), HostError> {
        Err(HostError::of(&common::NOT_YET))
    }

    /// `host.input.click`'s `via = "noactivate"`, its part on the event loop: the checks of
    /// [`click_check`](Self::click_check), and whether the window's top-level window may be given
    /// the no-activate style at all — then the job that gives it and clicks, for a thread of the
    /// host's ([`ActJob`]), because changing another program's window waits for that program.
    /// `owner` is the module the click is for, whose stop gives the style back. The default is a
    /// platform without it, `common::NOT_YET`.
    fn mouse_click_noactivate(
        &self,
        _window: isize,
        _x: i32,
        _y: i32,
        _button: MouseButton,
        _owner: usize,
    ) -> Result<ActJob, HostError> {
        Err(HostError::of(&common::NOT_YET))
    }

    /// The job that gives back their activation — takes the no-activate style off again — to the
    /// windows `when` ends, for the same thread as the clicks, after them; `why` for the log.
    /// `None` when no window holds the style and no click that gives it is on its way, which is
    /// asked without a lock: this runs on every window coming to the front. The default is a
    /// platform that never gives it.
    fn give_back_activation(&self, _when: GiveBackWhen, _why: &str) -> Option<ActJob> {
        None
    }

    /// At exit: from now on no window is given the no-activate style, and the job that gives every
    /// window holding it its activation back — for the same thread as the clicks, after
    /// everything handed to it before; `None` when no window holds it and no click that gives it
    /// is on its way. Each program has half a second to answer; what is not given back, the next
    /// start sees to. The default is a platform that never gives it.
    fn give_back_activation_at_exit(&self) -> Option<ActJob> {
        None
    }

    /// At the start: the job that gives their activation back to the windows the last run left
    /// with the no-activate style — a crash, a kill — as it wrote them down; `None` when it left
    /// none. For the same thread as the clicks, before any of them.
    fn give_back_left_over_activation(&self) -> Option<ActJob> {
        None
    }

    /// Child controls (descendant windows) of a top-level window, for detecting
    /// embedded plugins by control class + locating their coordinate origin.
    fn window_controls(&self, hwnd: isize) -> Vec<ControlInfo>;

    /// The chain of controls from the currently-focused element up to its
    /// top-level window — for detecting whether focus is inside an embedded
    /// plugin's control (which a foreground check alone can't see).
    fn window_focus_chain(&self) -> Vec<ControlInfo>;

    /// The thread of their own on which this platform asks every `host.element` question, if it
    /// has one: Windows, where UI Automation is asked from the multithreaded COM apartment, as
    /// Microsoft recommends for a client, and never from the event loop (`element.rs`). `None`: the
    /// host asks [`element_ask`](Self::element_ask) on the event loop.
    fn element_runner(&self) -> Option<ElementRunner> {
        None
    }

    /// Answers `ask` on the calling thread — the event loop: what the host does with every
    /// `host.element` question on a platform without [`element_runner`](Self::element_runner).
    /// The questions, by [`ElementAsk`]'s variants:
    ///
    /// - `Find`: whether `hwnd`'s tree holds an element with that name and control type, an empty
    ///   name being any element of the type.
    /// - `FindAny`: "is any of these names present as any of these control types?" — one
    ///   traversal per name — and the 1-based index of the name found.
    /// - `Locate`: that element's centre on screen, to click it; none when it has no rectangle.
    /// - `LocateVia`: the same, searched for within a container element (`via_name`/`via_type`)
    ///   — a hosted fragment a search from the window does not cross.
    /// - `PluginLocate`: ReaHotkey's `GetPluginUIAElement` + `FindElement`: the element that IS the
    ///   plug-in (its name, Window or Pane, the `ni::qt::QuickWindow` class preferred over the
    ///   `…QWindowIcon` host), then the target within it, over the RAW tree.
    /// - `StateProbe`: what that element says about its own state: the Toggle pattern's state
    ///   (-1 without one) and LegacyIAccessible's state word.
    /// - `Dump`, `RawDump`: the elements of the window's tree that carry a name or a class, by a
    ///   condition-based search and by the raw walker.
    /// - `ClassNavPoint`: the centre of the element reached from the first one whose class contains
    ///   `class` and whose control type is `ctype`, by `child` (nth child, 0 = none) and then
    ///   `sibling` raw-view siblings — ReaHotkey's `FindElement(ClassName)` + `WalkTree`.
    /// - `FocusStep`: the Tab pass-through of a standalone plug-in window: the keyboard focus moved
    ///   to the next (`direction` >= 0) or previous focusable element, wrapping, and where it landed.
    /// - `FocusWithin`: the keyboard focus given to the first focusable element whose centre lies
    ///   inside the rectangle, and which element took it.
    /// - `Press`: the element's own action, the keyboard focus left where it is.
    ///
    /// `Err` only for a question that got no answer ([`ElementFailure`]); "nothing there" is an
    /// answer.
    fn element_ask(&self, ask: &ElementAsk) -> ElementOutcome;

    /// Size of the primary display in pixels.
    fn screen_size(&self) -> (i32, i32);

    /// A pointer to the stateless screen-capture routine, so a worker thread can capture without
    /// holding the (`Rc`, non-`Send`) backend: each region captured through the source, or why it
    /// could not be — a capture error ([`capture_failed`]), which the module is told. Used
    /// by the image worker; the `screen-capture` thread captures through [`OcrWorker::frames`] and
    /// [`OcrWorker::points`]. No read of the screen is made on the calling thread of the backend: a
    /// `host.screen` call that cannot wait raises (task.rs).
    ///
    /// The Windows and macOS routines here, Windows' `compare_fn` (a Mac compares nothing), and the
    /// capture thread's `frames` and `points` begin with the loop guard's screen guard
    /// (`loop_guard::off_loop_screen`): on the event loop they are a bug — a panic in a debug build,
    /// a `[loop] screen capture on the event loop` line in a release one. The stub backend reads
    /// nothing and asks none.
    ///
    /// `Err` for a capture the standard path refused — an empty region, one too large — or, for a
    /// Windows module that declared `fallback = "none"`, one duplication could not answer, with the
    /// reason.
    fn capture_fn(&self) -> CaptureFn;

    /// The first-read comparison the image worker makes before it captures for a task that
    /// carries one ([`CompareReq`]), off the main thread as [`Backend::capture_fn`] is. The
    /// default is a platform with one way of reading the screen, which has nothing to compare.
    fn compare_fn(&self) -> CompareFn {
        no_comparison
    }

    /// The functions `host.ocr.recognize`'s two threads call — see [`OcrWorker`]. Taken once, when
    /// the service starts. The default is a platform with no recogniser.
    fn ocr_worker(&self) -> OcrWorker {
        OcrWorker::none()
    }

    /// Current mouse cursor position (screen coordinates).
    fn cursor_pos(&self) -> (i32, i32);
    /// The pointer to (x, y); moves as motion while a button is held, where the platform tells
    /// the two apart (Windows: an injected motion event against a placement, `windows.rs`).
    fn mouse_move(&self, x: i32, y: i32);
    fn mouse_click(&self, x: i32, y: i32, button: MouseButton);
    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton);

    /// Press a mouse button and LEAVE IT DOWN, and release it, as separate acts.
    ///
    /// `mouse_drag` presses and moves in the same breath, which is the right shape for a
    /// scrollbar and the wrong one for anything that only reveals itself while held —
    /// Melodyne's tool variants appear as a flyout after a press is sustained, and a drag that
    /// moves immediately misses them entirely.
    ///
    /// Split rather than given a timed variant with sleeps, because the sleeps would be in the
    /// pump: holding, gliding and settling is the better part of a second, and nothing else —
    /// speech, hotkeys, detection — may stop for that long. Split, a caller composes the
    /// gesture from timers and the pump keeps running between the parts.
    fn mouse_down(&self, x: i32, y: i32, button: MouseButton);
    fn mouse_up(&self, x: i32, y: i32, button: MouseButton);
    /// `delta` is in WHEEL UNITS, where 120 is one notch — the unit the operating system
    /// itself uses. Notches were the unit here until a control turned out to move too far for
    /// one of them to be a usable step.
    fn mouse_scroll(&self, x: i32, y: i32, delta: i32);
    /// Sends a key combo like "Ctrl+S".
    /// Deliver a key to a specific window as a MESSAGE, bypassing the input queue.
    ///
    /// For a hotkey that has to act AND still let the application have the key. Synthesising it
    /// the ordinary way does not work: our own hotkey registration sees synthesised input just
    /// as it sees real input, so the send re-triggers the handler that sent it. A posted message
    /// goes straight to the window and is seen by nobody else.
    ///
    /// This existed once, was removed when its last caller went away, and is back because
    /// Melodyne's sub-tools are reached by pressing a function key REPEATEDLY — documented by
    /// Celemony, unlike the press-and-hold-and-drag gesture it replaces. Posting is also the
    /// path most likely to work here: Melodyne runs its own message pump with its own
    /// accelerator table, so a posted WM_KEYDOWN reaches TranslateAccelerator while a sent one
    /// would bypass it.
    ///
    /// A key name that is not one raises ([`KeyRefusal::Mistake`]); what the moment decides — the
    /// window gone, a program of higher rights, a key the layout cannot type — is answered.
    fn key_post(&self, hwnd: isize, key: &str) -> Result<(), KeyRefusal>;

    /// Sends a key combo like "Ctrl+S" as synthetic input. A spec that does not parse raises
    /// ([`KeyRefusal::Mistake`]); a key the layout cannot type, or an event that could not be
    /// made, is answered.
    fn key_send(&self, combo: &str) -> Result<(), KeyRefusal>;
    /// Types Unicode text.
    fn type_text(&self, text: &str);

    /// Registers a global hotkey identified by `id` from a spec like "Ctrl+Alt+H". A spec this
    /// backend's parser refuses is a mistake ([`KeyRefusal::Mistake`]); a combination the system
    /// would not give it — another program holds it — is answered ([`KeyRefusal::Answer`]).
    fn register_hotkey(&self, id: i32, spec: &str) -> Result<(), KeyRefusal>;
    /// Registers a hotkey whose press the operating system delivers itself — `host.hotkey.register`
    /// with `{ foreground = true }` (experimental). On Windows that is `RegisterHotKey` without
    /// the keyboard hook's matching, so a press arrives as `WM_HOTKEY`, which is what gives this
    /// process the right to bring a window of its own forward; see `hotkey_foreground` in lib.rs
    /// for the price. Everywhere else a hotkey is the system's own delivery already, so the
    /// default is the ordinary registration.
    fn register_foreground_hotkey(&self, id: i32, spec: &str) -> Result<(), KeyRefusal> {
        self.register_hotkey(id, spec)
    }
    /// Why hotkey `id`, registered, is not held at the system now, or `None` when it is: a macOS
    /// letter no key of the current layout types, kept and registered once a layout types it. The
    /// default holds every hotkey it registered.
    fn hotkey_parked(&self, _id: i32) -> Option<HostError> {
        None
    }
    /// Unregisters a previously registered hotkey by id.
    fn unregister_hotkey(&self, id: i32);

    /// Starts watching foreground-window changes (delivered as `on_window_activate`).
    fn watch_foreground(&self) -> Result<(), String>;

    /// Replaces the captures the keyboard hook or the event tap intercepts and suppresses, each
    /// with the module that holds it, in registration order — see [`Captured`]. A taken key is
    /// delivered as `on_key`, with its module. A capture matches only when the pressed modifier
    /// state equals its mask (so "Tab", mask 0, does not swallow Alt+Tab).
    fn set_captured_keys(&self, keys: &[Captured]);
    /// Replaces what each module set with `host.keys.scope` and `host.keys.menuOpen` — see
    /// [`OwnerKeys`] and [`capture_decision`], which the hook or the tap decides every key by.
    /// The host keeps the table; this is the hook's copy, swapped under the same lock as the
    /// captures.
    fn set_key_owners(&self, owners: &[OwnerKeys]);
    /// The window `host.keys.scope(true)` pins a module's captures to: the window in front at
    /// this moment, as the hook or the tap will compare it — so a menu opened by a control (which
    /// brings another window to the front) receives the keys natively (ReaHotkey's
    /// `HotIf WinActive` model). 0 for everywhere. Windows: `GetForegroundWindow`. macOS:
    /// [`key_scope_window`].
    fn resolve_key_scope(&self) -> isize;
    /// The window in front as the hook or the tap compares every captured key with it, asked of
    /// nobody: what a hotkey that arrives at a busy module notes, so that a registration of the
    /// module's made again while the press waits takes it only in the window it was pressed in —
    /// judged as a captured key is. Windows: `GetForegroundWindow`. macOS: the window the tap was
    /// last told is in front. Never the accessibility question [`Backend::resolve_key_scope`]
    /// asks on a Mac: a hotkey arrives on the thread that also carries the event tap, at the
    /// moment the frontmost application is most likely to be slow, and that question pins nothing
    /// here.
    fn key_front(&self) -> isize;
    /// The keyboard now, for passing key `vk`, pressed with the roles in `mask` on the physical key
    /// `phys` ([`Pressed::phys`]), on to the program in front ([`pass_on_strokes`]) — see
    /// [`KeyboardNow`]: the window in front, whether the key is held down — down as far as the
    /// system knows, or its key-down kept from the system by the hook or the tap with no key-up
    /// seen since — the modifier roles held down, whether a screen reader's own key is held, and how
    /// many key-downs the hook or the tap has let through to the program so far. Windows:
    /// `GetForegroundWindow`, `GetAsyncKeyState` and the hook's own record; asks nobody but the
    /// system, microseconds. macOS: the window in front asked afresh of the frontmost application —
    /// one accessibility round trip, the tap's own idea of it can be stale — the HID system's key
    /// and modifier state, and the tap's own record; the key is `phys`, or looked up for `mask`, as
    /// a letter typed with Command is.
    fn keyboard_now(&self, vk: u32, mask: u8, phys: Option<u16>) -> KeyboardNow;
    /// Sends `strokes` — key `vk`, pressed with the roles in `mask` on the physical key `phys`
    /// ([`Pressed::phys`]: that key, where known, so that the numeric keypad's Enter is that Enter
    /// again), and the modifiers [`pass_on_strokes`] chose — to the program in front, each event
    /// marked so that the hook or the tap lets it through untouched: no capture takes it, and it
    /// does not count as a press, a hold or a modifier tap of the user's, nor as a key let through.
    /// Windows: one `SendInput`, whose events go in one after another with no other input between
    /// them; one it takes only a part of has what that part left down released, and is an `Err`.
    /// macOS: `CGEventPost` at the HID level from an event source of its own, every event built
    /// before the first is posted. `Err` says what failed; nothing is left down.
    fn pass_on_key(&self, vk: u32, mask: u8, phys: Option<u16>, strokes: &[Stroke]) -> Result<(), String>;
    /// Are any of Ctrl / Alt / Shift / Win held down right now? A hotkey callback runs
    /// WHILE its own combination is still pressed, so anything it synthesises afterwards
    /// arrives with those modifiers attached — see the note in the overlay runtime.
    fn modifiers_down(&self) -> bool;
    /// The user's double-click time: the interval in which quick repeats of a key count on
    /// (`press.count`, taps.rs) for a binding that names no `within` of its own. Asked at every
    /// press, so a setting the user changes holds from the next press on. Windows:
    /// `GetDoubleClickTime`; macOS: `NSEvent.doubleClickInterval`. The default, for a platform with
    /// neither, is Windows' default, [`DEFAULT_DOUBLE_CLICK`].
    fn double_click_time(&self) -> std::time::Duration {
        DEFAULT_DOUBLE_CLICK
    }
    /// The moment the last [`Backend::pump_pending`] began taking the presses queued for the host —
    /// hotkeys first, captured keys after them — so that a press made before it has been handed
    /// over by the time the pump returns. What a `taps = "final"` sequence's deadline is measured
    /// against (taps.rs, `tick`), so that a press made before the deadline, and still queued
    /// while the tick ran, goes on with its sequence instead of following it as a second. `None`
    /// before the first pump, and for a backend that queues no presses (the default): the tick's
    /// own moment then.
    fn input_taken_at(&self) -> Option<std::time::Instant> {
        None
    }
    /// What the keyboard layout the user is typing in types for `vk` with the modifiers in
    /// `mask`, and whether that is a dead key — the one question `host.keys.check` needs the
    /// running system for, asked only for the shape [`layout_question`] names. `None` when it
    /// types nothing (or only a control character), and wherever the platform cannot say.
    fn layout_char(&self, _vk: u32, _mask: u8) -> Option<(String, bool)> {
        None
    }
    /// Is a NATIVE popup menu on screen? One window-class lookup, no traversal —
    /// the cheap half of "is a menu open", which the key hook already uses and
    /// which the menu watch should ask before paying for an accessibility walk.
    fn native_menu_open(&self) -> bool;
    /// Installs the low-level keyboard hook (idempotent): the Windows hook's thread, the macOS
    /// event tap. `Err` with what `host.keys.capture` answers; the next call tries again.
    fn watch_keys(&self) -> Result<(), HostError>;

    /// Starts listening for what the operating system does to the whole session — sleep and
    /// wake, lock and unlock, the session going and coming back, display changes — which then
    /// arrive through `pump_pending` as [`HostEvents::on_system`]. Called once, on the event
    /// loop's thread, before the loop runs; idempotent. Installs no hook and takes no key. The
    /// default hears nothing; the macOS backend starts its listeners in `MacBackend::new`
    /// instead, for the whole run.
    fn watch_system(&self) {}

    /// The display part of [`environment`](Self::environment): the lines written again after a
    /// display or scale change, when they differ from the last ones written. The default has
    /// none.
    fn display_environment(&self) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Runs the platform event loop, dispatching OS events into `events`.
    /// Blocks until the process is terminated.
    fn run_event_loop(&self, events: &mut dyn HostEvents) -> Result<(), String>;

    /// Drains OS events accumulated since the last call (hotkeys, foreground
    /// changes, captured keys) into `events`. Non-blocking — meant to be driven
    /// from a host-owned loop such as a GUI timer tick, where the GUI toolkit
    /// (not us) owns the message pump.
    fn pump_pending(&self, events: &mut dyn HostEvents);
}

/// Whether tick `time` is not after tick `until` on a 32-bit millisecond clock that wraps (every
/// 49.7 days for Windows' `GetTickCount`): `time` lies in the half of the circle before `until`.
/// Pure, for Windows' drop of the `RegisterHotKey` presses made during a stall that ended in a stop.
pub fn tick_not_after(time: u32, until: u32) -> bool {
    until.wrapping_sub(time) < 0x8000_0000
}

/// What [`Backend::drop_queued_input`] dropped: captured keys as (virtual key, modifier mask,
/// owning module), and hotkey presses by id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DroppedInput {
    pub keys: Vec<(u32, u8, u32)>,
    pub hotkeys: Vec<i32>,
}

/// The double-click time where the system names none ([`Backend::double_click_time`]): Windows'
/// default.
pub const DEFAULT_DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(500);

/// A hotkey press as the backend hands it to the host ([`HostEvents::on_hotkey`]), once: a late
/// hook's report and the system's delivery of the same press are paired before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotkeyPress {
    /// How many key-downs the hook or the tap had let through to the program when it came in
    /// ([`Pressed::seq`]): as the hook matched it, or as the system delivered it.
    pub seq: u32,
    /// How many of those are the press's own key-down: 1 on a Mac, whose tap sees a hotkey's
    /// key-down at the HID level and lets it go on before Carbon matches it (queue.rs,
    /// `push_hotkey`) — and, with no tap in, counts nothing, so no other key can be in the count
    /// either. On Windows 0 for a press the hook swallowed, or was not called for, which only
    /// `RegisterHotKey` delivered; and 1 for one the hook let through on purpose for
    /// `RegisterHotKey` to deliver (a screen reader's modifier held), whose key-down it counted as it
    /// let it go on (windows.rs, `settle_hotkey`). What the host's count of quick repeats takes for
    /// no other key going down (taps.rs).
    pub own: u32,
    /// When it was pressed, as near as the platform says: Windows from the key event's or the
    /// message's stamp on the tick clock, a Mac when Carbon handed it over. What the quick repeats
    /// are timed by, rather than when the pump got to it.
    pub at: std::time::Instant,
}

/// Sink for OS events, implemented by the host to bridge into Luau callbacks.
pub trait HostEvents {
    /// Hotkey `id` was pressed, as `press` says.
    fn on_hotkey(&mut self, id: i32, press: HotkeyPress);
    fn on_window_activate(&mut self, win: WinInfo);
    /// The keyboard focus moved (possibly within the same top-level window).
    fn on_focus_change(&mut self);
    /// A captured key fired; `mods` is the pressed modifier bitmask (MASK_*). `owner` is the
    /// module the hook or the tap took it for ([`capture_decision`]), `repeat` whether it is the
    /// keyboard's auto-repeat of a key held down, and `pressed` where and how it was pressed
    /// ([`Taken`]).
    fn on_key(&mut self, vk: u32, mods: u8, owner: u32, repeat: bool, pressed: Pressed);
    /// What the game-controller sources saw since the last drain, in order — see
    /// [`gamepad::drain_into`]. A default body, so a sink that does not care about pads
    /// (a test's) need not say so.
    fn on_gamepad(&mut self, _events: Vec<gamepad::PadEvent>) {}
    /// What the operating system did to the whole session since the last drain — a sleep, a
    /// lock, the session switched away from, a display change — in order, each stamped with the
    /// wall-clock time the backend heard it; see [`crate::system_events`]. That can be well
    /// before this call: the notice that the machine is about to sleep may only be delivered
    /// once it has woken again. Delivered before the hotkeys, keys and window changes of the
    /// same drain, so they see the epochs it turned over. A default body, as for pads.
    fn on_system(&mut self, _events: Vec<crate::system_events::Stamped>) {}
    /// Hotkey presses dropped as they reached the queue, by their time stamp: made during the stall
    /// a stop ended, after [`Backend::drop_queued_input`] (Windows' `RegisterHotKey`, whose
    /// message is posted to the stalled thread). The host names them in the log. A default body,
    /// as for pads.
    fn on_stall_hotkeys_dropped(&mut self, _ids: Vec<i32>) {}
    /// One turn of the event loop has finished delivering events.
    ///
    /// The GUI path has a timer tick for the work that has to happen whether or not anything
    /// happened; a headless loop had nowhere to put it, which is how the Windows speech
    /// deadline came to be unreachable there — it lives in `Speech::pump`, and nothing in
    /// that loop called it. So every event loop now has the same hook.
    fn on_tick(&mut self) {}
}

/// Maps a friendly key name ("Tab", "a", "F1", "Right", "Escape") to a Win32
/// virtual-key code. (VK codes; only used by the Windows backend.)
pub fn key_to_vk(name: &str) -> Option<u32> {
    let k = name.trim();
    if k.chars().count() == 1 {
        let c = k.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Some(c.to_ascii_uppercase() as u32);
        }
        if c.is_ascii_digit() {
            return Some(c as u32);
        }
    }
    let lower = k.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix('f').and_then(|s| s.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Some(0x70 + (n - 1));
        }
    }
    Some(match lower.as_str() {
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "esc" | "escape" => 0x1B,
        "tab" => 0x09,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        _ => return None,
    })
}

/// The name `key_to_vk` would accept for a virtual key, for reporting a key back to a module
/// in the spelling it registered it with. `None` for anything the spec grammar cannot name.
pub fn vk_name(vk: u32) -> Option<String> {
    if (b'A' as u32..=b'Z' as u32).contains(&vk) || (b'0' as u32..=b'9' as u32).contains(&vk) {
        return char::from_u32(vk).map(|c| c.to_string());
    }
    if (0x70..=0x87).contains(&vk) {
        return Some(format!("F{}", vk - 0x70 + 1));
    }
    Some(
        match vk {
            0x20 => "Space",
            0x0D => "Return",
            0x1B => "Escape",
            0x09 => "Tab",
            0x08 => "Backspace",
            0x2E => "Delete",
            0x26 => "Up",
            0x28 => "Down",
            0x25 => "Left",
            0x27 => "Right",
            0x24 => "Home",
            0x23 => "End",
            0x21 => "PageUp",
            0x22 => "PageDown",
            _ => return None,
        }
        .to_string(),
    )
}

/// Modifier bitmask for `host.keys` (matches the pressed modifier state exactly).
///
/// The bits are modifier ROLES, not keys: [`MASK_CTRL`] is the Ctrl role, which is the
/// Control key on Windows and Linux and the Command key on macOS; [`MASK_WIN`] is the Win role,
/// the Windows (Super) key there and the Control key on a Mac. See [`KeyOs`] for the rule and
/// [`role_words`] for the table. Every mask the host stores, compares or hands a module is in
/// roles; only a backend's edge turns one into keys (`macos/keys.rs` holds that table).
pub const MASK_SHIFT: u8 = 1;
pub const MASK_CTRL: u8 = 2;
pub const MASK_ALT: u8 = 4;
pub const MASK_WIN: u8 = 8;

/// A modifier pressed and released with nothing in between — "Alt tap" and friends.
///
/// Its own mask bit rather than a mask of zero, because a bare modifier IS a mask of zero as
/// far as the ordinary matcher is concerned, and every combination would then look like one.
/// The hook never suppresses a tap: the modifier has to keep working as a modifier.
pub const MASK_TAP: u8 = 0x10;

/// macOS: the roles that hold Control and Option together — Win and Alt — which is VoiceOver's
/// modifier. Every chord that holds both is VoiceOver's before it is anybody else's.
pub const MAC_VOICEOVER_LAYER: u8 = MASK_WIN | MASK_ALT;

/// One capture as the keyboard hook or the event tap holds it: a combination, and the module
/// that captured it, by the index the host numbers modules with.
///
/// The host hands the whole set over in registration order — the order `host.keys.capture` was
/// called in — and only the captures of enabled modules. The order is what decides between two
/// modules that capture one key with a scope in front ([`capture_decision`]); each module holds
/// one capture per combination.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Captured {
    pub vk: u32,
    pub mask: u8,
    pub owner: u32,
}

/// What one module set with `host.keys.scope` and `host.keys.menuOpen`: the window its captures
/// are pinned to, 0 for everywhere, and whether it says a menu is open. A module with no entry
/// captures everywhere and has no menu open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerKeys {
    pub owner: u32,
    pub scope: isize,
    pub menu: bool,
}

/// Why the hook or the tap let a captured key-down through to the application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassWhy {
    /// Every capture of it belongs to a module scoped to a window that is not in front.
    OutOfScope,
    /// The application in front has a menu open that the system drew.
    MenuMode,
    /// A module whose flag counts for the window in front says a menu is open
    /// ([`menu_flag_owners`]).
    MenuFlag,
    /// One of this application's own windows has the keyboard, and no capture applies there: the
    /// Mac tap's rule. The Windows hook never says it — it is out of the chain then.
    OwnWindow,
}

/// What the hook or the tap does with a captured key-down ([`capture_decision`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// Taken, for `owner`: the module of the earliest capture of the combination whose scope is
    /// the window in front or everywhere.
    Take { owner: u32 },
    /// Let through to the application. `owner` is the module that would have taken it; none
    /// when no capture of the combination is in scope.
    Pass { why: PassWhy, owner: Option<u32> },
}

/// The window module `owner`'s captures are scoped to: 0, everywhere, when it never set one.
pub fn scope_of(owners: &[OwnerKeys], owner: u32) -> isize {
    owners.iter().find(|e| e.owner == owner).map_or(0, |e| e.scope)
}

/// Whether module `owner`'s captures count in window `front`: it is scoped to that window or to
/// everywhere. The rule [`capture_decision`] takes a key by, and the host offers a key a busy
/// module let go of by (`captures::offered_to`).
pub fn in_scope(owners: &[OwnerKeys], owner: u32, front: isize) -> bool {
    let s = scope_of(owners, owner);
    s == 0 || s == front
}

/// The modules whose menu flag counts for the window `front`: each says a menu is open, is
/// scoped to that window or to everywhere, and holds a capture in `set` right now. A module
/// that captures nothing cannot open a window's keys to a menu: its flag may be one it set
/// before its last capture went, and nobody would be left to clear it.
pub fn menu_flag_owners<'a>(
    set: &'a [Captured],
    owners: &'a [OwnerKeys],
    front: isize,
) -> impl Iterator<Item = u32> + 'a {
    owners
        .iter()
        .filter(move |e| {
            e.menu && (e.scope == 0 || e.scope == front) && set.iter().any(|c| c.owner == e.owner)
        })
        .map(|e| e.owner)
}

/// Whether a key-down of `(vk, mask)` is taken, and for which module — one decision for the
/// Windows hook and the Mac tap. `None`: no module captures the combination.
///
/// - **This application's own window lets it through**, before a menu is asked: while one of its
///   windows has the keyboard (`own_has_keyboard`), no capture applies, and a captured
///   combination goes to that window — `owner` is the module that would have taken it, if any,
///   which is all the scopes are walked for then. The Mac tap's rule, the counterpart of the
///   Windows hook being out of the chain then; the hook passes `false`, as it is never called for
///   those keys.
/// - **The earliest capture in scope takes it**: of the captures of the combination, in
///   registration order, the first whose module is scoped to `front` or to everywhere. A
///   capture scoped to another window does not take it, however early; with none in scope the
///   key goes to the application.
/// - **A menu lets it through**: a menu the system drew in the application in front
///   (`native_menu`, asked only once a capture is in scope, as it costs a call on Windows), or
///   a menu flag that counts for the window in front ([`menu_flag_owners`]) — the flag of one
///   module lets every capture in that window through, since a menu open there is open for
///   every key pressed there.
///
/// The scope is a whole top-level window. Two plug-ins inside one DAW window share it, so there
/// the earlier registration wins.
///
/// `front` is the window in front as the hook or the tap knows it: on Windows
/// `GetForegroundWindow`, on a Mac the window the tap was last told is in front. Called for
/// every key event a capture matches, inside the hook or the tap: it walks the set once, stops
/// at the first capture in scope, and looks the owners up linearly — tens of comparisons, no
/// allocation, nothing asked of another process.
pub fn capture_decision(
    set: &[Captured],
    owners: &[OwnerKeys],
    vk: u32,
    mask: u8,
    front: isize,
    own_has_keyboard: bool,
    native_menu: impl FnOnce() -> bool,
) -> Option<Capture> {
    let mut any = false;
    let mut taker = None;
    for c in set.iter().filter(|c| c.vk == vk && c.mask == mask) {
        any = true;
        if in_scope(owners, c.owner, front) {
            taker = Some(c.owner);
            break;
        }
    }
    if !any {
        return None;
    }
    if own_has_keyboard {
        return Some(Capture::Pass { why: PassWhy::OwnWindow, owner: taker });
    }
    let Some(owner) = taker else {
        return Some(Capture::Pass { why: PassWhy::OutOfScope, owner: None });
    };
    if native_menu() {
        return Some(Capture::Pass { why: PassWhy::MenuMode, owner: Some(owner) });
    }
    if menu_flag_owners(set, owners, front).next().is_some() {
        return Some(Capture::Pass { why: PassWhy::MenuFlag, owner: Some(owner) });
    }
    Some(Capture::Take { owner })
}

/// macOS: whether the tap swallows a key-down before it is decided: the keyboard's auto-repeat
/// (`repeat`) of a key whose key-down the tap swallowed for a capture and whose key-up has not come
/// yet (`down_swallowed`), while one of this application's own windows has the keyboard
/// (`own_has_keyboard`) — a page the capture's own callback showed, say. It is the same press: its
/// start was taken, and its key-up is swallowed wherever it lands, so a repeat let through would be
/// a key held in our window that never comes up — a `keydown` with `repeat` set and no `keyup`, and
/// a held Return pressing the focused button again at every repeat. Not handed to the module
/// either: no capture applies in our window. Anything else is decided as ever
/// ([`capture_decision`]): a key pressed in our window goes on to it, repeats and all, and anywhere
/// else a held key's repeats are decided as any key-down. The Windows hook is out of the chain
/// while one of this application's windows is in front, so it never asks.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn swallow_held_repeat(repeat: bool, down_swallowed: bool, own_has_keyboard: bool) -> bool {
    repeat && down_swallowed && own_has_keyboard
}

/// A captured key-down the hook or the tap took, as it queues it for the pump: the combination,
/// the module it was taken for, whether it is the keyboard's auto-repeat of a key held down, and
/// where and how it was pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Taken {
    pub vk: u32,
    pub mask: u8,
    pub owner: u32,
    pub repeat: bool,
    pub pressed: Pressed,
}

/// A press as the hook or the tap saw it, kept with the key: what decides, when its module's
/// mailbox delivers it, whether a key its module let go of meanwhile may still go to the program
/// in front, and how ([`pass_on_strokes`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pressed {
    /// The window that was in front when it was pressed, as the hook or the tap compares every
    /// key with it ([`Backend::key_front`]); 0 when none was known.
    pub front: isize,
    /// How many key-downs the hook or the tap had let through to the program by then
    /// ([`KeyboardNow::let_through`]): a key let through after it, which reached the program
    /// before it, moves the count on.
    pub seq: u32,
    /// The physical key, where the platform names one apart from the virtual key: Windows the scan
    /// code, with 0xE000 for an extended key (as `MapVirtualKeyW` with `MAPVK_VK_TO_VSC_EX` writes
    /// it), macOS the keycode. `None` for a hotkey, whose press the system matched, and for a key
    /// whose event carried no scan code.
    pub phys: Option<u16>,
    /// When the hook or the tap queued the key for the main thread, for the trace's `dispatch`
    /// line: how long it waited there. `None` for a hotkey.
    pub queued: Option<std::time::Instant>,
}

/// How many keys one module's record of keys let through for a menu holds until it is read
/// (`host.keys.passedThrough`): nobody presses more than a few keys inside a menu between two
/// ticks of the runtime's menu timer, and a reader that never comes must not grow it for ever.
pub const MENU_PASS_MAX: usize = 32;

/// Files a key let through for a menu under module `owner` in the hook's or the tap's record
/// of `(owner, vk, mask)`, unless that module's share of it is full. Inside the hook or the
/// tap: one walk of a record of at most [`MENU_PASS_MAX`] keys per module.
pub fn file_menu_pass(record: &mut Vec<(u32, u32, u8)>, owner: u32, vk: u32, mask: u8) {
    if record.iter().filter(|p| p.0 == owner).count() < MENU_PASS_MAX {
        record.push((owner, vk, mask));
    }
}

/// Takes module `owner`'s keys out of the record, in the order they were filed; the other
/// modules' stay.
pub fn take_menu_passes(record: &mut Vec<(u32, u32, u8)>, owner: u32) -> Vec<(u32, u8)> {
    let mine = record.iter().filter(|p| p.0 == owner).map(|p| (p.1, p.2)).collect();
    record.retain(|p| p.0 != owner);
    mine
}

/// The keyboard as the backend reads it at the moment a key that a module's registration kept
/// from the program in front is to go to that program after all (`captures::pass_on`): the module
/// no longer holds the key by the time its busy mailbox delivers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyboardNow {
    /// The window in front, in the numbers [`Pressed::front`] is written in; `Some(0)` when no
    /// window is, `None` when nobody could say (a Mac's frontmost application that did not answer).
    pub front: Option<isize>,
    /// Whether the key itself is held down: down as far as the system knows, or its key-down kept
    /// from the system by the hook or the tap with no key-up seen since.
    pub key_held: bool,
    /// The modifier roles held down ([`MASK_SHIFT`], [`MASK_CTRL`], [`MASK_ALT`], [`MASK_WIN`]).
    pub mods: u8,
    /// Whether a screen reader's own key is held down — Insert, the numeric keypad's 0 or Caps
    /// Lock, as the hook tells them (Windows; never on a Mac): a key sent now would reach the
    /// screen reader as one of its commands. No press is kept from the program with one held.
    pub reader_mod: bool,
    /// How many key-downs the hook or the tap has let through to the program so far, counted from
    /// any number and wrapping: every key-down that is not a modifier's and that goes on towards
    /// the program — typed keys no capture took, keys let through for a menu or a screen reader,
    /// a held key's repeats, keys another program or this one's `host.input` sent — and never one
    /// a capture or a hotkey took, nor one passed on ([`Backend::pass_on_key`]).
    pub let_through: u32,
}

/// One event of a key passed on to the program in front, in the order it is sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    /// The key of modifier role `role` ([`MASK_CTRL`] and the others) goes down or up: one the
    /// press was made with and nobody holds now.
    Modifier { role: u8, down: bool },
    /// The key itself goes down or up.
    Key { down: bool },
}

/// Why a key a module's registration kept from the program in front is not passed on to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotPassed {
    /// Another window is in front than the one it was pressed in.
    OtherWindow,
    /// No window was known to be in front when it was pressed, so nothing can say the one in
    /// front now is that window.
    NoWindow,
    /// The key itself is held down: a key-down and a key-up sent now would end a press the user is
    /// still making.
    KeyHeld,
    /// Modifier roles are held now that the press was made without: sent now, it would arrive as
    /// another combination.
    OtherModifiers(u8),
    /// A modifier tap, which the hook and the tap never keep from the program: it had it at the
    /// press.
    Tap,
    /// Nobody could say which window is in front now (a Mac's frontmost application that did not
    /// answer), so nothing can say it is the one it was pressed in.
    FrontUnknown,
    /// A screen reader's own key is held now ([`KeyboardNow::reader_mod`]): sent now, it would
    /// reach the screen reader as one of its commands, not the program.
    ReaderModifier,
    /// A key-down the hook or the tap let through after the press has reached the program first
    /// ([`Pressed::seq`]): sent now, it would arrive after a key typed after it, and act on what
    /// that key changed — a Delete on the line a Down moved to.
    Overtaken,
    /// A hotkey of this application holds the key's combination now: the system would hand the
    /// key sent to that hotkey, not to the program (decided by the host, `captures::pass_on`).
    HotkeyHolds,
}

/// The order a key passed on presses the modifiers it presses, `key_send`'s: Ctrl, Alt, Shift,
/// Win. They are released in the reverse order.
const PASS_ON_ROLES: [u8; 4] = [MASK_CTRL, MASK_ALT, MASK_SHIFT, MASK_WIN];

/// The four role bits of a mask, without [`MASK_TAP`].
const ROLE_BITS: u8 = MASK_SHIFT | MASK_CTRL | MASK_ALT | MASK_WIN;

/// The key events that pass a key on to the program in front as it was pressed — the same key
/// with the same modifiers — or why it is not passed on. `mask` is the press's, `pressed` where
/// and when it was pressed, `now` the keyboard as it is.
///
/// Passed on only while the window it was pressed in is still in front, the key itself is not
/// held, no modifier is held that the press was made without, no screen reader's key is held, and
/// no key-down has reached the program since the press ([`Pressed::seq`]): sent, it arrives where
/// it would have, had nothing captured it — after the keys typed before it, before any typed after
/// it. A modifier the press had that nobody holds now goes down before the key and up after it, in
/// [`PASS_ON_ROLES`]'s order and back; one the user holds is left alone, so a held modifier is
/// never released under the user's finger and none is left down. The key goes down and up, once: a
/// held key's repeats, folded into one while it waited, are one more such event and pass on as one
/// more press. A tap is never passed on. The checks go in the order of [`NotPassed`]'s reasons as
/// written here, and the first that fails is the one given. Pure, so it is tested on every
/// platform; the backend reads `now` and sends the events.
pub fn pass_on_strokes(mask: u8, pressed: Pressed, now: KeyboardNow) -> Result<Vec<Stroke>, NotPassed> {
    if mask & MASK_TAP != 0 {
        return Err(NotPassed::Tap);
    }
    if pressed.front == 0 {
        return Err(NotPassed::NoWindow);
    }
    match now.front {
        None => return Err(NotPassed::FrontUnknown),
        Some(front) if front != pressed.front => return Err(NotPassed::OtherWindow),
        Some(_) => {}
    }
    if now.key_held {
        return Err(NotPassed::KeyHeld);
    }
    let roles = mask & ROLE_BITS;
    let extra = now.mods & ROLE_BITS & !roles;
    if extra != 0 {
        return Err(NotPassed::OtherModifiers(extra));
    }
    if now.reader_mod {
        return Err(NotPassed::ReaderModifier);
    }
    if now.let_through != pressed.seq {
        return Err(NotPassed::Overtaken);
    }
    let pressed: Vec<u8> = PASS_ON_ROLES.iter().copied().filter(|r| roles & r != 0 && now.mods & r == 0).collect();
    let mut out = Vec::with_capacity(pressed.len() * 2 + 2);
    out.extend(pressed.iter().map(|&role| Stroke::Modifier { role, down: true }));
    out.push(Stroke::Key { down: true });
    out.push(Stroke::Key { down: false });
    out.extend(pressed.iter().rev().map(|&role| Stroke::Modifier { role, down: false }));
    Ok(out)
}

/// What the first `sent` of `strokes` left down, as the key-ups that release it: the key first,
/// then the modifiers in the reverse of the order they went down. For a sequence the system took
/// only a part of (Windows' `SendInput` says how many events it inserted), so that nothing stays
/// down. Pure.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn left_down(strokes: &[Stroke], sent: usize) -> Vec<Stroke> {
    let sent = &strokes[..sent.min(strokes.len())];
    let mut key_down = false;
    let mut mods: Vec<u8> = Vec::new();
    for s in sent {
        match *s {
            Stroke::Key { down } => key_down = down,
            Stroke::Modifier { role, down: true } => mods.push(role),
            Stroke::Modifier { role, down: false } => mods.retain(|r| *r != role),
        }
    }
    let mut out = Vec::new();
    if key_down {
        out.push(Stroke::Key { down: false });
    }
    out.extend(mods.iter().rev().map(|&role| Stroke::Modifier { role, down: false }));
    out
}

/// Where a Mac's `host.keys.scope(true)` pins a module's captures, and why.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeFrom {
    /// The frontmost application named its window in front.
    Asked,
    /// It did not; the window the tap was last told is in front.
    TapFront,
    /// Neither knows one: everywhere.
    Nowhere,
}

/// The Mac's choice of the window `host.keys.scope(true)` pins: the window the frontmost
/// application names (`asked`), else the window the tap was last told is in front
/// (`tap_front`, which every key is compared with anyway), else everywhere.
///
/// The fallback was everywhere at first, so that an overlay's keys would rather be claimed
/// everywhere than nowhere. Once the scope became each module's own, everywhere meant that a
/// module whose window could not be named claimed its keys in every other window too, before
/// the module whose window is in front. `asked` is `None` when the application did not answer
/// and `Some(0)` when it answered with no window; neither names one, so both fall back. Pure,
/// so it is tested on every platform.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn key_scope_window(asked: Option<isize>, tap_front: isize) -> (isize, ScopeFrom) {
    match asked {
        Some(w) if w != 0 => (w, ScopeFrom::Asked),
        _ if tap_front != 0 => (tap_front, ScopeFrom::TapFront),
        _ => (0, ScopeFrom::Nowhere),
    }
}

/// Which platform's reading of a key spec is wanted.
///
/// There is one grammar, and its modifier names are ROLES — the Qt convention, decided
/// 2026-09-22 (it replaced the 2026-08-20 "positional" rule and the `Mod`/`Global` tokens):
/// Ctrl, Alt, Win and Shift. On Windows and Linux each is the key of that name. On macOS Ctrl
/// is Command, Alt is Option, Win is Control and Shift is Shift, as Qt's `ControlModifier` is
/// Command there and its `MetaModifier` Control. So `"Ctrl+C"` copies on both platforms, and
/// VoiceOver's Control+Option layer is Win+Alt in a spec.
///
/// Parsing does not depend on the platform — every spelling names the same role everywhere
/// ([`modifier_mask`]). What does is which key each role is ([`role_words`]), which
/// combinations the system keeps for itself, and the words a key is said in. So every
/// function below that depends on one of those takes the platform as an argument, and the
/// macOS answers are tested on the machine this project is written on, the way `macos/keys.rs`
/// is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOs {
    Windows,
    Macos,
    /// Read the Windows way — the stub backend's platforms have no hotkeys of their own to
    /// be different about — and said with `Super` for the Windows key.
    Linux,
}

impl KeyOs {
    /// The platform this build runs on.
    pub const CURRENT: KeyOs = if cfg!(target_os = "macos") {
        KeyOs::Macos
    } else if cfg!(windows) {
        KeyOs::Windows
    } else {
        KeyOs::Linux
    };
}

/// One modifier role as a platform writes and says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoleWords {
    /// The mask bit: [`MASK_CTRL`], [`MASK_ALT`], [`MASK_SHIFT`] or [`MASK_WIN`].
    pub role: u8,
    /// The spelling [`normalize_spec_for`] writes it in on this platform. Parses back to the
    /// same role on every platform.
    pub spec: &'static str,
    /// The key this role is on this platform's keyboard, spelled out, for speech.
    pub spoken: &'static str,
    /// The same, as the platform abbreviates it in writing, for a log line or a label.
    pub short: &'static str,
}

const fn role(role: u8, spec: &'static str, spoken: &'static str, short: &'static str) -> RoleWords {
    RoleWords { role, spec, spoken, short }
}

/// Windows, in the order Windows writes modifiers: Ctrl+Alt+Shift+Win.
const WINDOWS_ROLES: [RoleWords; 4] = [
    role(MASK_CTRL, "Ctrl", "Control", "Ctrl"),
    role(MASK_ALT, "Alt", "Alt", "Alt"),
    role(MASK_SHIFT, "Shift", "Shift", "Shift"),
    role(MASK_WIN, "Win", "Windows", "Win"),
];

/// Linux: the Windows words, with the key a Linux keyboard calls Super.
const LINUX_ROLES: [RoleWords; 4] = [
    role(MASK_CTRL, "Ctrl", "Control", "Ctrl"),
    role(MASK_ALT, "Alt", "Alt", "Alt"),
    role(MASK_SHIFT, "Shift", "Shift", "Shift"),
    role(MASK_WIN, "Win", "Super", "Super"),
];

/// macOS, in the order Apple prints modifiers (⌃⌥⇧⌘): Control, Option, Shift, Command — which
/// are the Win, Alt, Shift and Ctrl roles. The Control key is `Meta` in a spec and `Control`
/// in both styles of words: `Ctrl` written in a Mac's log would read as the spec spelling,
/// which is Command.
const MACOS_ROLES: [RoleWords; 4] = [
    role(MASK_WIN, "Meta", "Control", "Control"),
    role(MASK_ALT, "Option", "Option", "Option"),
    role(MASK_SHIFT, "Shift", "Shift", "Shift"),
    role(MASK_CTRL, "Cmd", "Command", "Cmd"),
];

/// The role table for `os`: which key each modifier role is there, and how it is written and
/// said, in the order the platform writes modifiers.
pub fn role_words(os: KeyOs) -> &'static [RoleWords; 4] {
    match os {
        KeyOs::Windows => &WINDOWS_ROLES,
        KeyOs::Macos => &MACOS_ROLES,
        KeyOs::Linux => &LINUX_ROLES,
    }
}

/// The words for one role bit on `os`.
fn role_word(os: KeyOs, bit: u8) -> &'static RoleWords {
    role_words(os)
        .iter()
        .find(|w| w.role == bit)
        .expect("every role bit is in every platform's table")
}

/// The modifier role one modifier name stands for, or `None` for a word that is not a
/// modifier. The same on every platform, and the one place a modifier name is read: the parser,
/// both `key_send`s and the Windows hotkey conversion all come through here.
///
/// - `Ctrl`, `Control`, `Cmd`, `Command`: the Ctrl role — Control on Windows and Linux, Command
///   on macOS.
/// - `Alt`, `Option`: the Alt role — Alt, and Option on macOS.
/// - `Win`, `Super`, `Meta`: the Win role — the Windows key, Super on Linux, and the Control key
///   on macOS. A Mac author writes `Meta` (or `Win`) for the Mac's Control key.
/// - `Shift`.
pub fn modifier_mask(name: &str) -> Option<u8> {
    Some(match name.trim().to_ascii_lowercase().as_str() {
        "shift" => MASK_SHIFT,
        "ctrl" | "control" | "cmd" | "command" => MASK_CTRL,
        "alt" | "option" => MASK_ALT,
        "win" | "super" | "meta" => MASK_WIN,
        _ => return None,
    })
}

/// The virtual key a role is as a key in its own right — the one a `"<modifier> tap"` names:
/// the generic Win32 code of the key the role is on Windows. A backend that has other keys for
/// the roles translates it at its edge, as `macos/keys.rs` does.
fn role_vk(bit: u8) -> u32 {
    match bit {
        MASK_SHIFT => 0x10,
        MASK_CTRL => 0x11,
        MASK_ALT => 0x12,
        _ => 0x5B,
    }
}

/// The role of a tap's virtual key, the reverse of [`role_vk`]; `None` for any other key.
fn vk_role(vk: u32) -> Option<u8> {
    [MASK_SHIFT, MASK_CTRL, MASK_ALT, MASK_WIN].into_iter().find(|&bit| role_vk(bit) == vk)
}

/// Parses a key spec like "Tab", "Shift+Tab", "Ctrl+S" or "Alt tap" into (vk, modifier mask),
/// or says which part it could not read. The same on every platform: the mask is in roles (see
/// [`KeyOs`]).
///
/// The error names the part, because it reaches a module author: `host.input.send` raises it,
/// and the Windows hotkey path logs it with the conflict it causes.
pub fn parse_key_spec(spec: &str) -> Result<(u32, u8), String> {
    // "<modifier> tap": pressed alone, no other key in between, and passed through.
    if let Some(rest) = spec.strip_suffix(" tap").or_else(|| spec.strip_suffix(" Tap")) {
        // Resolved here rather than in `key_to_vk`, deliberately: a bare modifier is a key in a
        // TAP and a mistake anywhere else, and putting it in the general table would make
        // "Alt" quietly acceptable as an ordinary hotkey.
        let name = rest.trim();
        let vk = match modifier_mask(name) {
            Some(bit) => role_vk(bit),
            None => key_to_vk(name).ok_or_else(|| format!("unknown key '{name}' in '{spec}'"))?,
        };
        return Ok((vk, MASK_TAP));
    }
    let parts: Vec<&str> = spec
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let (key, mods) = parts.split_last().ok_or_else(|| "empty key spec".to_string())?;
    let mut mask = 0u8;
    // The Ctrl role has two families of spelling, and a spec that uses one of each names two
    // keys to its author — `"Control+Command+F"` is Control-Command-F to a Mac user — where the roles make
    // them one. ORed together it would quietly be Command+F, so it is refused with the Mac's
    // own spelling for the Control key. Repeating one family (`"Ctrl+Control+S"`) is harmless.
    let (mut ctrl_word, mut cmd_word) = (None, None);
    for m in mods {
        mask |= modifier_mask(m).ok_or_else(|| format!("unknown modifier '{m}' in '{spec}'"))?;
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => ctrl_word = ctrl_word.or(Some(*m)),
            "cmd" | "command" => cmd_word = cmd_word.or(Some(*m)),
            _ => {}
        }
    }
    if let (Some(a), Some(b)) = (ctrl_word, cmd_word) {
        return Err(format!(
            "'{a}' and '{b}' in '{spec}' are both the Ctrl role, which is Command on a Mac; \
             the Mac's Control key is written 'Meta'"
        ));
    }
    let vk = key_to_vk(key).ok_or_else(|| format!("unknown key '{key}' in '{spec}'"))?;
    Ok((vk, mask))
}

/// [`parse_key_spec`] without the reason.
pub fn key_spec(spec: &str) -> Option<(u32, u8)> {
    parse_key_spec(spec).ok()
}

/// How a key is put into words by [`describe_key_for`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyStyle {
    /// Whole words, for speech: "Control+Alt+P", "Shift+Command+F9", "Up Arrow".
    Spoken,
    /// The platform's written abbreviations, for a label or a log line: "Ctrl+Alt+P",
    /// "Shift+Cmd+F9", "Up".
    Short,
}

/// How one modifier role is written on `os`. `style` `None` is the parser's own spelling —
/// what [`normalize_spec_for`] produces and what reads back as the same key.
fn modifier_word(os: KeyOs, bit: u8, style: Option<KeyStyle>) -> &'static str {
    let w = role_word(os, bit);
    match style {
        None => w.spec,
        Some(KeyStyle::Spoken) => w.spoken,
        Some(KeyStyle::Short) => w.short,
    }
}

/// How the key itself is written. `None` for the parser's own spelling.
///
/// Two of these are asymmetric and bite on a Mac: the key Windows calls Backspace is labelled
/// Delete there, and what Windows calls Delete is Forward Delete. Return is Enter on a Windows
/// keyboard. None of the words below reads back as the same key, which is why a description
/// is never a spec.
fn key_word(os: KeyOs, vk: u32, style: Option<KeyStyle>) -> String {
    let named = vk_name(vk).unwrap_or_else(|| format!("vk {vk:#04x}"));
    let Some(style) = style else { return named };
    let mac = os == KeyOs::Macos;
    let spoken = style == KeyStyle::Spoken;
    let word = match vk {
        0x0D if mac => "Return",
        0x0D => "Enter",
        0x08 if mac => "Delete",
        0x2E if mac => "Forward Delete",
        0x1B if spoken => "Escape",
        0x1B => "Esc",
        0x21 => "Page Up",
        0x22 => "Page Down",
        0x26 if spoken => "Up Arrow",
        0x28 if spoken => "Down Arrow",
        0x25 if spoken => "Left Arrow",
        0x27 if spoken => "Right Arrow",
        _ => return named,
    };
    word.to_string()
}

/// A resolved key in words; `style` `None` is the parser's own spelling.
fn key_words(os: KeyOs, vk: u32, mask: u8, style: Option<KeyStyle>) -> String {
    if mask & MASK_TAP != 0 {
        let name = match vk_role(vk) {
            Some(bit) => modifier_word(os, bit, style).to_string(),
            None => key_word(os, vk, style),
        };
        return match style {
            Some(KeyStyle::Spoken) => format!("{name} pressed on its own"),
            _ => format!("{name} tap"),
        };
    }
    let mut out = String::new();
    for w in role_words(os) {
        if mask & w.role != 0 {
            out.push_str(modifier_word(os, w.role, style));
            out.push('+');
        }
    }
    out.push_str(&key_word(os, vk, style));
    out
}

/// The canonical spelling of a resolved key on `os`: the modifiers in the platform's order and
/// in its spec words ([`role_words`]), the key in [`vk_name`]'s. Two specs name the same key
/// exactly when this is equal for both, and it parses back to the same `(vk, mask)` — on every
/// platform, because parsing does not depend on one.
pub fn spec_name_for(os: KeyOs, vk: u32, mask: u8) -> String {
    key_words(os, vk, mask, None)
}

/// The key `spec` stands for, in `os`'s canonical spelling ([`spec_name_for`]); `None` when it
/// does not parse. `"ctrl + s"` and `"Control+S"` are `"Ctrl+S"` on Windows and `"Cmd+S"` on
/// macOS; `"Alt+Shift+Ctrl+Win+X"` is `"Ctrl+Alt+Shift+Win+X"` on Windows and
/// `"Meta+Option+Shift+Cmd+X"` on macOS.
pub fn normalize_spec_for(os: KeyOs, spec: &str) -> Option<String> {
    key_spec(spec).map(|(vk, mask)| spec_name_for(os, vk, mask))
}

/// A resolved key as `os` says it.
pub fn describe_key_for(os: KeyOs, vk: u32, mask: u8, style: KeyStyle) -> String {
    key_words(os, vk, mask, Some(style))
}

/// `spec` as `os` says it; `None` when it does not parse. Not a spec: `"Delete"` in a Mac
/// description is the key the parser calls Backspace, and `"Control"` there is the Win role.
pub fn describe_spec_for(os: KeyOs, spec: &str, style: KeyStyle) -> Option<String> {
    key_spec(spec).map(|(vk, mask)| describe_key_for(os, vk, mask, style))
}

/// How the host's "Binding conflict" and "Binding unavailable" dialogs name a hotkey the log
/// names by `spec` — the spelling [`hotkey_claim_for`] recorded it in.
///
/// On Windows and Linux the spec's words are the keys' own names, so the dialog reads as the
/// log does. On a Mac they are not: the spec calls the Control key `Meta` and Command `Cmd`, and
/// a user told "Meta+Shift+F6" is not told which keys to look for. So a Mac's dialog says the
/// key in [`describe_key_for`]'s spoken words — "Control+Shift+F6" — and its log line keeps the
/// spec. A spec that does not parse is shown as written.
pub fn dialog_key_words_for(os: KeyOs, spec: &str) -> String {
    match os {
        KeyOs::Macos => {
            describe_spec_for(os, spec, KeyStyle::Spoken).unwrap_or_else(|| spec.to_string())
        }
        KeyOs::Windows | KeyOs::Linux => spec.to_string(),
    }
}

/// macOS: the log line for a capture of a combination the system keeps for itself
/// ([`reserved_for`]), or `None` for any other capture and on any other platform.
///
/// `host.keys.capture` raises only for a spec that does not parse, so a capture of `"Ctrl+Q"`
/// written for Windows is Command+Q on a Mac, and the event tap is asked to take it from the
/// system for as long as the capture holds. Said, not refused: a capture is the module's own
/// decision about the keys of its window, and what the tap does with each of these chords has
/// not been measured. Windows is left out on purpose — its two reserved combinations are about
/// `RegisterHotKey`, and the hook captures F12 like any other key. Called by the macOS event
/// tap only; it lives here so its words are tested where the project is written.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn reserved_capture_line(os: KeyOs, vk: u32, mask: u8) -> Option<String> {
    if os != KeyOs::Macos {
        return None;
    }
    let why = reserved_for(os, vk, mask)?;
    let key = describe_key_for(os, vk, mask, KeyStyle::Short);
    Some(format!(
        "the captured key '{key}' is one macOS keeps for itself ({why}): a hotkey on it is \
         refused, and this capture asks the event tap to take it from the system while it \
         holds. host.os.pick gives the Mac another key"
    ))
}

/// The modifier table a capture callback is handed, from the mask the key arrived with: one
/// field per role, named for the role. On macOS `ctrl` is therefore Command held and `win`
/// Control held.
pub fn capture_mods_fields(mask: u8) -> [(&'static str, bool); 4] {
    [
        ("shift", mask & MASK_SHIFT != 0),
        ("ctrl", mask & MASK_CTRL != 0),
        ("alt", mask & MASK_ALT != 0),
        ("win", mask & MASK_WIN != 0),
    ]
}

/// `host.keys.check` reasons. Structural only: what the platform does with a combination, never
/// what some other program happens to hold — JAWS and NVDA are extensible, and a list of their
/// keys would be wrong the day somebody installs an add-on.
pub const REASON_PARSE: &str = "parse";
/// The system keeps the combination for itself; `host.hotkey.register` refuses it.
pub const REASON_RESERVED: &str = "reserved";
/// macOS: the combination holds Control and Option together, VoiceOver's modifier — the Win and
/// Alt roles ([`MAC_VOICEOVER_LAYER`]).
pub const REASON_VOICEOVER: &str = "voiceover";
/// macOS: the key has no key code (F21–F24).
pub const REASON_NO_KEYCODE: &str = "no_keycode";
/// A modifier pressed on its own: something `host.keys.capture` watches, and never a hotkey.
pub const REASON_TAP: &str = "tap";
/// Windows: Ctrl+Alt is AltGr, and this one types a character in the current layout.
pub const REASON_ALTGR: &str = "altgr";
/// macOS: Option with this key types a character in the current layout.
pub const REASON_COMPOSES: &str = "composes";

/// Does `reason` mean the combination cannot work — as opposed to working and costing the
/// user a character their layout types with it? A tap works: as the capture it is.
pub fn reason_blocks(reason: &str) -> bool {
    matches!(reason, REASON_PARSE | REASON_RESERVED | REASON_VOICEOVER | REASON_NO_KEYCODE)
}

/// Does `host.hotkey.register` refuse a spec `check` gives this reason — raising for one that
/// does not parse or is a tap, answering `nil, err` for a key the system keeps or has no key code
/// for ([`hotkey_refusal_for`])? Every reason that makes a registration impossible rather than
/// merely costly: VoiceOver's layer is not one of them, because a Mac whose VoiceOver modifier is
/// Caps Lock, or that runs no VoiceOver, gets the key. The overlay runtime skips its control
/// hotkeys by the same list.
pub fn reason_refuses_hotkey(reason: &str) -> bool {
    matches!(reason, REASON_PARSE | REASON_RESERVED | REASON_NO_KEYCODE | REASON_TAP)
}

/// Why `os` keeps `(vk, mask)` for itself, or `None` when it does not. Exact matches: the
/// modifier state has to be the one listed. The masks are roles, so the macOS entries are
/// written with the keys they are there.
///
/// - Windows, from the `RegisterHotKey` documentation: F12 is kept for the debugger even when
///   none is running, and Win+L locks the computer before any application sees it.
/// - macOS: Command+F5 turns VoiceOver on and off, Option+Command+F5 opens the Accessibility
///   Shortcuts panel — the two a blind user can least afford to lose — and the system chords
///   that have no Windows counterpart a module could be expected to know about. Carbon would
///   register the application-menu ones (Command+H, +M, +Q) system-wide, taking hide,
///   minimise and quit away from every application for as long as the hotkey is held. In a
///   spec Command is `Ctrl` (or `Cmd`), so `"Ctrl+Q"` is refused there.
pub fn reserved_for(os: KeyOs, vk: u32, mask: u8) -> Option<&'static str> {
    // The Mac keys, as the roles they are.
    const CMD: u8 = MASK_CTRL;
    const OPT: u8 = MASK_ALT;
    const CTL: u8 = MASK_WIN;
    const SHF: u8 = MASK_SHIFT;
    const WINDOWS: &[(u32, u8, &str)] = &[
        (0x7B, 0, "F12 is kept for the debugger, even when none is running"),
        (0x4C, MASK_WIN, "Win+L locks the computer"),
    ];
    const MACOS: &[(u32, u8, &str)] = &[
        (0x74, CMD, "Command+F5 turns VoiceOver on and off"),
        (0x74, CMD | OPT, "Option+Command+F5 opens the Accessibility Shortcuts panel"),
        (0x09, CMD, "Command+Tab is the application switcher"),
        (0x09, CMD | SHF, "Shift+Command+Tab is the application switcher"),
        // The grammar cannot name the grave key today; listed so the day it can, this holds.
        (0xC0, CMD, "Command+` moves between the windows of the application in front"),
        (0x20, CMD, "Command+Space is Spotlight"),
        (0x48, CMD, "Command+H hides the application in front"),
        (0x4D, CMD, "Command+M minimises the window in front"),
        (0x51, CMD, "Command+Q quits the application in front"),
        (0x51, CMD | SHF, "Shift+Command+Q logs out"),
        (0x51, CMD | OPT | SHF, "Option+Shift+Command+Q logs out without asking"),
        (0x51, CMD | CTL, "Control+Command+Q locks the screen"),
        (0x1B, CMD | OPT, "Option+Command+Escape opens Force Quit"),
        (0x33, CMD | SHF, "Shift+Command+3 takes a screenshot"),
        (0x34, CMD | SHF, "Shift+Command+4 takes a screenshot"),
        (0x35, CMD | SHF, "Shift+Command+5 opens the screenshot tools"),
    ];
    let table = match os {
        KeyOs::Windows => WINDOWS,
        KeyOs::Macos => MACOS,
        KeyOs::Linux => return None,
    };
    table.iter().find(|(v, m, _)| *v == vk && *m == mask).map(|(_, _, why)| *why)
}

/// Does macOS have a key code for this virtual key? Everything [`key_to_vk`] produces except
/// F21–F24; `macos/keys.rs` checks this against its table. A letter always has one here: which
/// key types it is the keyboard layout's answer, which can change while the application runs,
/// so a hotkey on a letter no key of the current layout types is accepted and parked by the
/// macOS backend until a layout that types it is selected, not refused as `no_keycode`.
pub fn has_mac_keycode(vk: u32) -> bool {
    !(0x84..=0x87).contains(&vk)
}

/// Would this combination type a character, so that holding it takes that character from the
/// user? The one shape per platform where the answer can be yes, and the reason it would be:
/// Ctrl+Alt without Win on Windows, which is AltGr there; Option without Control or Command on
/// macOS — the Alt role without the Win or Ctrl role — which composes. The keyboard layout is
/// asked only then.
pub fn layout_question(os: KeyOs, mask: u8) -> Option<&'static str> {
    if mask & MASK_TAP != 0 {
        return None;
    }
    match os {
        KeyOs::Windows => (mask & (MASK_CTRL | MASK_ALT) == MASK_CTRL | MASK_ALT
            && mask & MASK_WIN == 0)
            .then_some(REASON_ALTGR),
        KeyOs::Macos => {
            (mask & MASK_ALT != 0 && mask & (MASK_CTRL | MASK_WIN) == 0).then_some(REASON_COMPOSES)
        }
        KeyOs::Linux => None,
    }
}

/// What `host.keys.check` answers.
#[derive(Debug, Default, PartialEq)]
pub struct KeyCheck {
    /// The canonical spec ([`spec_name_for`]), `None` when it did not parse.
    pub resolved: Option<String>,
    pub reasons: Vec<&'static str>,
    /// For `altgr` / `composes`: what the layout types with it.
    pub produces: Option<String>,
    /// And whether that is a dead key, waiting for the next keystroke to combine with.
    pub dead_key: bool,
}

impl KeyCheck {
    /// No reason that stops the combination working. `altgr` and `composes` do not: the key
    /// fires, and the character is what it costs.
    pub fn ok(&self) -> bool {
        !self.reasons.iter().any(|r| reason_blocks(r))
    }
}

/// [`KeyCheck`] for `spec` on `os`. `layout` answers the one question that needs the running
/// system — what the current keyboard layout types for `(vk, mask)`, and whether it is a dead
/// key — and is asked only when [`layout_question`] says the answer can matter.
pub fn check_spec_for(
    os: KeyOs,
    spec: &str,
    layout: impl FnOnce(u32, u8) -> Option<(String, bool)>,
) -> KeyCheck {
    let Some((vk, mask)) = key_spec(spec) else {
        return KeyCheck { reasons: vec![REASON_PARSE], ..KeyCheck::default() };
    };
    let mut check = KeyCheck { resolved: Some(spec_name_for(os, vk, mask)), ..KeyCheck::default() };
    if mask & MASK_TAP != 0 {
        check.reasons.push(REASON_TAP);
    }
    if reserved_for(os, vk, mask).is_some() {
        check.reasons.push(REASON_RESERVED);
    }
    if os == KeyOs::Macos {
        // The whole layer, not a list: VoiceOver's default modifier is the pair, and every
        // chord that holds both is VoiceOver's before it is anybody else's. Structural, so it
        // is said whether or not VoiceOver is running on the machine that asks — the module is
        // written for the machines where it is. In a spec the pair is Win+Alt.
        if mask & MAC_VOICEOVER_LAYER == MAC_VOICEOVER_LAYER {
            check.reasons.push(REASON_VOICEOVER);
        }
        if !has_mac_keycode(vk) {
            check.reasons.push(REASON_NO_KEYCODE);
        }
    }
    if let Some(reason) = layout_question(os, mask) {
        if let Some((text, dead)) = layout(vk, mask) {
            check.reasons.push(reason);
            check.produces = Some(text);
            check.dead_key = dead;
        }
    }
    check
}

/// Why `host.hotkey.register` refuses `spec` on `os`, or `None` when it does not. `None` for a
/// spec that does not parse, too: the parser's own error is the one to raise for that.
///
/// Everything the operating system can never hold is refused here, when the module asks, with
/// the reason — a tap, a key the platform has no code for, a combination the system keeps. Left
/// to the claim, each of these failed inside `refresh_hotkeys` instead, and reached the user as
/// the "Binding unavailable" dialog, which blames another application that uses the key. The
/// [`reason_refuses_hotkey`] reasons of [`check_spec_for`], and derived from them, so the two
/// cannot say different things.
///
/// A tap is a mistake in the call on every platform, and raises ([`KeyRefusal::Mistake`]); a key
/// this platform keeps or has no code for is answered `nil, err` (`taken`, `not_supported`).
pub fn hotkey_refusal_for(os: KeyOs, spec: &str) -> Option<KeyRefusal> {
    use crate::errors::hotkey;
    let (vk, mask) = key_spec(spec)?;
    let check = check_spec_for(os, spec, |_, _| None);
    let reason = check.reasons.iter().copied().find(|r| reason_refuses_hotkey(r))?;
    let shown = describe_key_for(os, vk, mask, KeyStyle::Short);
    Some(match reason {
        REASON_TAP => KeyRefusal::Mistake(format!(
            "the hotkey '{spec}' ({shown}) is a modifier pressed on its own, which cannot be a \
             global hotkey: capture it with host.keys.capture instead"
        )),
        REASON_NO_KEYCODE => KeyRefusal::Answer(HostError::with(
            &hotkey::NO_KEYCODE,
            format!(
                "the hotkey '{spec}' ({shown}) cannot be registered: macOS has no key code for {}. \
                 Choose another key",
                key_word(os, vk, Some(KeyStyle::Short))
            ),
        )),
        _ => KeyRefusal::Answer(HostError::with(
            &hotkey::KEPT_BY_SYSTEM,
            format!(
                "the hotkey '{spec}' ({shown}) is kept by the system and cannot be registered: {}. \
                 Choose another combination",
                reserved_for(os, vk, mask).unwrap_or("the system uses it")
            ),
        )),
    })
}

/// What `host.hotkey.register` claims for `spec` on `os`: its `(vk, mask)`, and the spelling the
/// claim is recorded, logged and handed to the backend under — the canonical one
/// ([`spec_name_for`]), or the author's own for a spec that does not parse, which the backend
/// then refuses with the parser's error. `Err` for everything [`hotkey_refusal_for`] refuses —
/// the sentence the binding raises, or the error value it answers: then there is nothing to
/// record and nothing to register, so a refused spec reaches neither the backend nor, on
/// Windows, the keyboard hook's table. One function, so that "refused before anything is
/// registered" is its order and not a convention of the binding's.
pub fn hotkey_claim_for(os: KeyOs, spec: &str) -> Result<(Option<(u32, u8)>, String), KeyRefusal> {
    if let Some(why) = hotkey_refusal_for(os, spec) {
        return Err(why);
    }
    let binding = key_spec(spec);
    let spelled =
        binding.map_or_else(|| spec.to_string(), |(vk, mask)| spec_name_for(os, vk, mask));
    Ok((binding, spelled))
}

/// Warms up the secondary OCR engine (loads its model off the hot path) so the
/// first OCR after launch is instant. Returns the warmup thread's handle (where
/// available) so the caller can join it before the process exits — a detached
/// thread still inside ONNX Runtime init when the process tears down races ort's
/// static cleanup and faults (the headless / fast-exit "segfault").
///
/// On a Mac the thread first waits for Vision's warm-up to end (`macos::ocr::VISION_WARM`):
/// two engines warming at once compete on the few cores of an Intel Mac. It loads ONNX Runtime
/// from the bundle then, and says once in the log whether the recogniser is ready. Called before
/// Vision's warm-up has even started (that is `MacBackend::new`), which is why the exit calls
/// [`stop_ocr_warmup`] before it joins this thread.
pub fn warmup_ocr() -> Option<std::thread::JoinHandle<()>> {
    #[cfg(windows)]
    let h = Some(paddle_ocr::warmup());
    #[cfg(target_os = "macos")]
    let h = paddle_ocr::warmup_after(&macos::ocr::VISION_WARM);
    #[cfg(not(any(windows, target_os = "macos")))]
    let h: Option<std::thread::JoinHandle<()>> = None;
    h
}

/// The exit's call, right before it joins [`warmup_ocr`]'s thread: on a Mac that thread may still
/// wait for a Vision warm-up that never ended, or never began because the manager could not be
/// made, and from here it stops waiting and ends without touching ONNX Runtime. Nothing to do
/// elsewhere.
pub fn stop_ocr_warmup() {
    #[cfg(target_os = "macos")]
    macos::ocr::VISION_WARM.stop();
}

/// The exit's last word on text recognition, before [`settle_ocr`]: on a Mac the neural
/// recogniser's counts — its readings beside Vision's, and who answered each read — are written
/// once more, with everything counted since their last line (`ocr/shadow.rs`); when no read was
/// counted, only where the recogniser loaded. A lock and one line. Nothing elsewhere: Windows
/// keeps no such counts.
pub fn ocr_exit_report() {
    #[cfg(target_os = "macos")]
    macos::ocr::shadow_report();
}

/// Waits, for at most half a second, until no recognition of the secondary OCR engine is
/// still running, before the process exits. The engine's recognitions are left to finish on
/// threads of their own whenever the primary engine answers first, and one still inside ONNX
/// Runtime at exit is the same fault `warmup_ocr` joins its thread against. When there was
/// anything to wait for, logs how long it waited or that it gave up. Nothing to wait for
/// anywhere but Windows and macOS, where that engine exists.
pub fn settle_ocr() {
    #[cfg(any(windows, target_os = "macos"))]
    paddle_ocr::settle();
}

/// The last thing a process does with text recognition before it ends, after [`settle_ocr`]: on
/// a Mac, the neural recogniser's session and ONNX Runtime's environment released while all they
/// use is still there (`paddle_ocr::release`), because ONNX Runtime 1.22 aborts in `exit()`'s
/// teardown on an environment still alive. Waits up to half a second for a call still inside
/// ONNX Runtime; returns at once when there is none, or when the recogniser was never loaded, and
/// a second call finds nothing. True when the process may end through `exit()` — returning from
/// `main`; false when something was still inside ONNX Runtime and nothing was released, and it
/// must end through [`end_now`]. Logged when it released something or gave up. Always true
/// elsewhere: Windows links ONNX Runtime in statically, and its exit is as it always was.
pub fn release_ocr() -> bool {
    #[cfg(target_os = "macos")]
    let safe = !matches!(paddle_ocr::release(), paddle_ocr::Released::Busy(_));
    #[cfg(not(target_os = "macos"))]
    let safe = true;
    safe
}

/// Ends this process at once with `code`, for one whose [`release_ocr`] said false: standard
/// output and error flushed, then `_exit` on a Mac — no `atexit` handler and no static destructor
/// runs, ONNX Runtime's included, so nothing can abort; the log needs nothing, as every line is
/// written when it is said. `std::process::exit` elsewhere, where it is never needed.
pub fn end_now(code: i32) -> ! {
    use std::io::Write as _;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    #[cfg(target_os = "macos")]
    // SAFETY: `_exit` ends the process; nothing after it runs.
    unsafe {
        libc::_exit(code)
    }
    #[cfg(not(target_os = "macos"))]
    std::process::exit(code)
}

/// The backend for the current platform.
pub fn platform() -> Rc<dyn Backend> {
    #[cfg(windows)]
    let backend: Rc<dyn Backend> = Rc::new(windows::WindowsBackend::new());
    #[cfg(target_os = "macos")]
    let backend: Rc<dyn Backend> = macos::new();
    #[cfg(not(any(windows, target_os = "macos")))]
    let backend: Rc<dyn Backend> = Rc::new(stub::StubBackend::default());
    backend
}

#[cfg(test)]
mod recognise_each_tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicBool, Ordering};

    fn text(t: &str) -> Result<OcrText, HostError> {
        Ok(OcrText { text: t.into(), words: vec![], lines: vec![], fallback: None, skipped: false })
    }

    /// Every region is reported as it is answered, and once the application is closing the rest
    /// are answered without being read — not one more recognition starts.
    #[test]
    fn regions_after_the_stop_are_not_read_and_each_answered_one_is_reported() {
        let stop = AtomicBool::new(false);
        let done = Cell::new(0);
        let tick = || done.set(done.get() + 1);
        let ctx = Recognise { lang: None, fast_ok: true, preempt: None, stop: Some(&stop), region_done: Some(&tick) };
        let read = Cell::new(0);
        let answers = ctx.each(0..5, |i| {
            read.set(read.get() + 1);
            if i == 1 {
                stop.store(true, Ordering::Release); // the exit begins during the second region
            }
            text(&i.to_string())
        });
        assert_eq!(read.get(), 2, "the third region was never started");
        assert_eq!(done.get(), 2);
        assert_eq!(answers.len(), 5, "every region is still answered");
        assert_eq!(answers[1].as_ref().unwrap().text, "1");
        assert!(answers[2..].iter().all(|a| a.as_ref().err() == Some(&closing())));
        assert_eq!(closing().reason(), &crate::errors::ocr::CLOSING);

        // Neither is required: the legacy shape reads everything and reports nothing.
        let plain = Recognise { lang: None, fast_ok: true, preempt: None, stop: None, region_done: None };
        assert_eq!(plain.each(0..3, |_| text("x")).len(), 3);
    }
}

#[cfg(test)]
mod key_grammar_tests {
    use super::*;
    use KeyOs::{Linux, Macos, Windows};

    const ALL: [KeyOs; 3] = [Windows, Macos, Linux];

    /// Every spelling names one role, on every platform: the parse has no platform in it.
    #[test]
    fn every_spelling_names_its_role() {
        for (names, bit) in [
            (&["Ctrl", "Control", "Cmd", "Command", "ctrl", "CONTROL", " cmd "][..], MASK_CTRL),
            (&["Alt", "Option", "alt", "OPTION"][..], MASK_ALT),
            (&["Win", "Super", "Meta", "win", "META"][..], MASK_WIN),
            (&["Shift", "SHIFT"][..], MASK_SHIFT),
        ] {
            for name in names {
                assert_eq!(modifier_mask(name), Some(bit), "{name}");
                assert_eq!(key_spec(&format!("{name}+X")), Some((0x58, bit)), "{name}+X");
            }
        }
        assert_eq!(key_spec("Cmd+S"), key_spec("Ctrl+S"));
        assert_eq!(key_spec("Command+S"), key_spec("Control+S"));
        assert_eq!(key_spec("Meta+S"), key_spec("Win+S"));
        assert_eq!(key_spec("Super+S"), key_spec("Win+S"));
        assert_eq!(key_spec("Option+S"), key_spec("Alt+S"));
        assert_eq!(key_spec("Shift+Tab"), Some((0x09, MASK_SHIFT)));
        assert_eq!(key_spec("Ctrl+Alt+Shift+Win+X"), Some((0x58, 0x0F)));
    }

    /// A spec that spells the Ctrl role both ways names two keys to its author, and would be one
    /// key under the roles: `"Control+Command+F"` would be Command+F (Find) on a Mac rather than
    /// Control-Command-F, and `"Control+Command+Q"` refused as if it were Command+Q. Refused as a
    /// parse error instead, naming the Mac's word for the Control key. One family repeated is
    /// still one key.
    #[test]
    fn the_ctrl_role_spelled_both_ways_is_refused() {
        for spec in [
            "Control+Command+F",
            "Ctrl+Cmd+F",
            "cmd + ctrl + F",
            "Command+Shift+Control+F6",
            "Control+Command+Q",
        ] {
            let e = parse_key_spec(spec).unwrap_err();
            assert!(e.contains("both the Ctrl role") && e.contains("'Meta'"), "{spec}: {e}");
            for os in ALL {
                assert_eq!(normalize_spec_for(os, spec), None, "{spec}");
                assert_eq!(check_spec_for(os, spec, |_, _| None).reasons, vec![REASON_PARSE]);
            }
        }
        let e = parse_key_spec("Control+Command+F").unwrap_err();
        assert!(e.starts_with("'Control' and 'Command' in 'Control+Command+F'"), "{e}");
        for (spec, same) in [
            ("Ctrl+Control+S", "Ctrl+S"),
            ("Cmd+Command+S", "Ctrl+S"),
            ("Alt+Option+S", "Alt+S"),
            ("Win+Meta+S", "Win+S"),
            ("Meta+Command+F", "Ctrl+Win+F"),
        ] {
            assert_eq!(key_spec(spec), key_spec(same), "{spec}");
        }
    }

    /// The tokens of 2026-09-21 are gone: they are unknown modifiers like any other word.
    #[test]
    fn mod_and_global_are_not_modifiers_any_more() {
        for word in ["Mod", "Global", "mod", "GLOBAL", "Hyper"] {
            assert_eq!(modifier_mask(word), None, "{word}");
            let e = parse_key_spec(&format!("{word}+F6")).unwrap_err();
            assert!(e.contains(&format!("unknown modifier '{word}'")), "{e}");
            let e = parse_key_spec(&format!("{word} tap")).unwrap_err();
            assert!(e.contains(&format!("unknown key '{word}'")), "{e}");
            for os in ALL {
                let spec = format!("{word}+S");
                assert_eq!(normalize_spec_for(os, &spec), None);
                assert_eq!(describe_spec_for(os, &spec, KeyStyle::Spoken), None);
                assert_eq!(check_spec_for(os, &spec, |_, _| None).reasons, vec![REASON_PARSE]);
                // The backend's parser raises for it; the claim keeps the author's spelling.
                assert_eq!(hotkey_claim_for(os, &spec), Ok((None, spec.clone())));
            }
        }
    }

    /// The role table: which key each role is, per platform — the Qt convention on a Mac.
    #[test]
    fn the_role_table_per_platform() {
        let table = |os| {
            role_words(os).iter().map(|w| (w.role, w.spec, w.spoken, w.short)).collect::<Vec<_>>()
        };
        assert_eq!(
            table(Windows),
            vec![
                (MASK_CTRL, "Ctrl", "Control", "Ctrl"),
                (MASK_ALT, "Alt", "Alt", "Alt"),
                (MASK_SHIFT, "Shift", "Shift", "Shift"),
                (MASK_WIN, "Win", "Windows", "Win"),
            ]
        );
        assert_eq!(
            table(Macos),
            vec![
                (MASK_WIN, "Meta", "Control", "Control"),
                (MASK_ALT, "Option", "Option", "Option"),
                (MASK_SHIFT, "Shift", "Shift", "Shift"),
                (MASK_CTRL, "Cmd", "Command", "Cmd"),
            ]
        );
        assert_eq!(
            table(Linux),
            vec![
                (MASK_CTRL, "Ctrl", "Control", "Ctrl"),
                (MASK_ALT, "Alt", "Alt", "Alt"),
                (MASK_SHIFT, "Shift", "Shift", "Shift"),
                (MASK_WIN, "Win", "Super", "Super"),
            ]
        );
        for os in ALL {
            let mut roles: Vec<u8> = role_words(os).iter().map(|w| w.role).collect();
            roles.sort_unstable();
            assert_eq!(roles, vec![MASK_SHIFT, MASK_CTRL, MASK_ALT, MASK_WIN], "{os:?}: each role once");
            for w in role_words(os) {
                assert_eq!(modifier_mask(w.spec), Some(w.role), "{os:?}: '{}' reads back", w.spec);
            }
        }
    }

    /// What a Mac makes of the roles, key by key.
    #[test]
    fn on_a_mac_ctrl_is_command_and_win_is_control() {
        let d = |s| describe_spec_for(Macos, s, KeyStyle::Spoken).unwrap();
        assert_eq!(d("Ctrl+C"), "Command+C");
        assert_eq!(d("Control+C"), "Command+C");
        assert_eq!(d("Cmd+C"), "Command+C");
        assert_eq!(d("Win+C"), "Control+C");
        assert_eq!(d("Meta+C"), "Control+C");
        assert_eq!(d("Super+C"), "Control+C");
        assert_eq!(d("Alt+C"), "Option+C");
        assert_eq!(d("Meta+Alt+Right"), "Control+Option+Right Arrow");
        assert_eq!(d("Ctrl+Alt+Shift+Win+X"), "Control+Option+Shift+Command+X");
    }

    /// The runtime's calibration keys: Ctrl+Alt+Shift on Windows as they always were, and
    /// Command+Option+Shift on a Mac — off VoiceOver's layer, which is Control+Option.
    #[test]
    fn the_calibration_keys_are_command_option_shift_on_a_mac() {
        for k in ["S", "T", "V"] {
            let spec = format!("Ctrl+Alt+Shift+{k}");
            let (_, mask) = key_spec(&spec).unwrap();
            assert_eq!(mask, MASK_CTRL | MASK_ALT | MASK_SHIFT);
            assert_eq!(
                describe_spec_for(Windows, &spec, KeyStyle::Spoken).unwrap(),
                format!("Control+Alt+Shift+{k}")
            );
            assert_eq!(
                describe_spec_for(Macos, &spec, KeyStyle::Spoken).unwrap(),
                format!("Option+Shift+Command+{k}")
            );
            let c = check_spec_for(Macos, &spec, |_, _| None);
            assert!(c.ok() && c.reasons.is_empty(), "{spec} on a Mac: {:?}", c.reasons);
        }
    }

    /// A tap is one role pressed on its own; on a Mac "Ctrl tap" is a Command tap.
    #[test]
    fn taps_take_one_role() {
        for (specs, vk) in [
            (&["Ctrl tap", "Control tap", "Cmd tap", "Command Tap"][..], 0x11),
            (&["Alt tap", "Option tap"][..], 0x12),
            (&["Win tap", "Super tap", "Meta tap"][..], 0x5B),
            (&["Shift tap"][..], 0x10),
        ] {
            for s in specs {
                assert_eq!(key_spec(s), Some((vk, MASK_TAP)), "{s}");
            }
        }
        let d = |os, s| describe_spec_for(os, s, KeyStyle::Spoken).unwrap();
        assert_eq!(d(Windows, "Ctrl tap"), "Control pressed on its own");
        assert_eq!(d(Macos, "Ctrl tap"), "Command pressed on its own");
        assert_eq!(d(Macos, "Win tap"), "Control pressed on its own");
        assert_eq!(d(Macos, "Alt tap"), "Option pressed on its own");
        assert_eq!(d(Linux, "Win tap"), "Super pressed on its own");
        let n = |os, s| normalize_spec_for(os, s).unwrap();
        assert_eq!(n(Windows, "Cmd tap"), "Ctrl tap");
        assert_eq!(n(Macos, "Ctrl tap"), "Cmd tap");
        assert_eq!(n(Macos, "Win tap"), "Meta tap");
        assert_eq!(n(Macos, "Alt tap"), "Option tap");
    }

    /// The part that did not parse is named, because the message reaches the module author.
    #[test]
    fn a_spec_that_does_not_parse_says_which_part() {
        let e = parse_key_spec("Hyper+F6").unwrap_err();
        assert!(e.contains("unknown modifier 'Hyper'"), "{e}");
        let e = parse_key_spec("Ctrl+Foo").unwrap_err();
        assert!(e.contains("unknown key 'Foo'"), "{e}");
        assert!(parse_key_spec("").unwrap_err().contains("empty"));
        assert!(parse_key_spec("+ +").unwrap_err().contains("empty"));
        // A modifier on its own is not a key, outside the tap form.
        assert!(key_spec("Alt").is_none());
        assert!(key_spec("Ctrl+Cmd").is_none());
    }

    #[test]
    fn normalize_is_one_spelling_per_key() {
        let n = |os, s| normalize_spec_for(os, s);
        assert_eq!(n(Windows, "ctrl + s").as_deref(), Some("Ctrl+S"));
        assert_eq!(n(Windows, "Control+S").as_deref(), Some("Ctrl+S"));
        assert_eq!(n(Windows, "Cmd+S").as_deref(), Some("Ctrl+S"));
        assert_eq!(n(Windows, "Meta+X").as_deref(), Some("Win+X"));
        assert_eq!(n(Windows, "Ctrl+Shift+Win+Alt+F6").as_deref(), Some("Ctrl+Alt+Shift+Win+F6"));
        assert_eq!(n(Windows, "Option+P").as_deref(), Some("Alt+P"));
        assert_eq!(n(Windows, "shift+tab").as_deref(), Some("Shift+Tab"));
        assert_eq!(n(Windows, "Ctrl+enter").as_deref(), Some("Ctrl+Return"));
        assert_eq!(n(Windows, "Alt+Shift+Ctrl+Win+x").as_deref(), Some("Ctrl+Alt+Shift+Win+X"));
        assert_eq!(n(Windows, "Hyper+X"), None);

        assert_eq!(n(Macos, "Ctrl+S").as_deref(), Some("Cmd+S"));
        assert_eq!(n(Macos, "Control+S").as_deref(), Some("Cmd+S"));
        assert_eq!(n(Macos, "Win+X").as_deref(), Some("Meta+X"));
        assert_eq!(n(Macos, "Cmd+Shift+F6").as_deref(), Some("Shift+Cmd+F6"));
        assert_eq!(n(Macos, "Alt+P").as_deref(), Some("Option+P"));
        assert_eq!(n(Macos, "Alt+Shift+Ctrl+Win+x").as_deref(), Some("Meta+Option+Shift+Cmd+X"));
        assert_eq!(n(Linux, "Win+E").as_deref(), Some("Win+E"));

        // What the runtime keys its claim maps by: one key, however it is written, and the same
        // one on every platform.
        for os in ALL {
            assert_eq!(n(os, "Cmd+S"), n(os, "Ctrl+S"), "{os:?}");
            assert_eq!(n(os, "Option+P"), n(os, "Alt+P"), "{os:?}");
            assert_ne!(n(os, "Win+S"), n(os, "Ctrl+S"), "{os:?}");
        }
    }

    /// The canonical spelling reads back as the same key and is its own canonical spelling, for
    /// every key name and every modifier word the grammar has, on every platform — and one
    /// platform's spelling reads back as the same key on every other.
    #[test]
    fn normalize_round_trips() {
        let mut keys: Vec<String> = ('A'..='Z').map(String::from).collect();
        keys.extend(('0'..='9').map(String::from));
        keys.extend((1..=24).map(|n| format!("F{n}")));
        for k in [
            "Space", "Enter", "Return", "Esc", "Escape", "Tab", "Backspace", "Delete", "Del", "Up",
            "Down", "Left", "Right", "Home", "End", "PageUp", "PageDown",
        ] {
            keys.push(k.to_string());
        }
        let mods = [
            "", "Ctrl+", "Control+", "Cmd+", "Command+", "Alt+", "Option+", "Shift+", "Win+",
            "Super+", "Meta+", "Ctrl+Alt+Shift+Win+", "Meta+Alt+", "Cmd+Shift+",
        ];
        for os in ALL {
            for m in mods {
                for k in &keys {
                    let spec = format!("{m}{k}");
                    let n = normalize_spec_for(os, &spec).unwrap_or_else(|| panic!("{spec} lost"));
                    assert_eq!(key_spec(&n), key_spec(&spec), "{os:?} {spec} -> {n}");
                    assert_eq!(normalize_spec_for(os, &n).as_deref(), Some(n.as_str()), "{os:?} {n}");
                    for other in ALL {
                        let theirs = normalize_spec_for(other, &n).unwrap();
                        assert_eq!(key_spec(&theirs), key_spec(&spec), "{os:?} {n} on {other:?}");
                    }
                }
            }
            for tap in [
                "Alt tap", "Ctrl tap", "Shift tap", "Win tap", "Cmd tap", "Meta tap", "Option tap",
            ] {
                let n = normalize_spec_for(os, tap).unwrap();
                assert_eq!(key_spec(&n), key_spec(tap), "{os:?} {tap} -> {n}");
            }
        }
    }

    #[test]
    fn describe_says_each_platform_in_its_own_words() {
        use KeyStyle::{Short, Spoken};
        let d = |os, s, st| describe_spec_for(os, s, st).unwrap();
        assert_eq!(d(Windows, "Ctrl+Shift+F9", Spoken), "Control+Shift+F9");
        assert_eq!(d(Macos, "Ctrl+Shift+F9", Spoken), "Shift+Command+F9");
        assert_eq!(d(Macos, "Ctrl+Shift+F9", Short), "Shift+Cmd+F9");
        assert_eq!(d(Windows, "Ctrl+Shift+Win+Alt+F5", Spoken), "Control+Alt+Shift+Windows+F5");
        assert_eq!(d(Windows, "Ctrl+Shift+Win+Alt+F5", Short), "Ctrl+Alt+Shift+Win+F5");
        assert_eq!(d(Macos, "Cmd+Shift+F5", Short), "Shift+Cmd+F5");
        assert_eq!(d(Macos, "Cmd+Shift+F5", Spoken), "Shift+Command+F5");
        assert_eq!(d(Macos, "Ctrl+Shift+Win+Alt+F6", Spoken), "Control+Option+Shift+Command+F6");
        assert_eq!(d(Macos, "Ctrl+Shift+Win+Alt+F6", Short), "Control+Option+Shift+Cmd+F6");
        assert_eq!(d(Macos, "Meta+Tab", Spoken), "Control+Tab");
        assert_eq!(d(Macos, "Win+X", Short), "Control+X");
        assert_eq!(d(Macos, "Alt+V", Spoken), "Option+V");
        assert_eq!(d(Windows, "Alt+V", Spoken), "Alt+V");
        assert_eq!(d(Windows, "Win+E", Spoken), "Windows+E");
        assert_eq!(d(Windows, "Win+E", Short), "Win+E");
        assert_eq!(d(Linux, "Win+E", Spoken), "Super+E");
        assert_eq!(d(Linux, "Win+E", Short), "Super+E");
        // The two keys that swap names on a Mac, and Return.
        assert_eq!(d(Macos, "Backspace", Spoken), "Delete");
        assert_eq!(d(Macos, "Delete", Spoken), "Forward Delete");
        assert_eq!(d(Windows, "Backspace", Spoken), "Backspace");
        assert_eq!(d(Windows, "Del", Spoken), "Delete");
        assert_eq!(d(Windows, "Return", Spoken), "Enter");
        assert_eq!(d(Macos, "Enter", Spoken), "Return");
        assert_eq!(d(Windows, "Shift+Up", Spoken), "Shift+Up Arrow");
        assert_eq!(d(Windows, "Shift+Up", Short), "Shift+Up");
        assert_eq!(d(Windows, "Esc", Short), "Esc");
        assert_eq!(d(Windows, "Esc", Spoken), "Escape");
        assert_eq!(d(Windows, "PageDown", Spoken), "Page Down");
        assert_eq!(d(Windows, "Alt tap", Spoken), "Alt pressed on its own");
        assert_eq!(d(Macos, "Alt tap", Spoken), "Option pressed on its own");
        assert_eq!(d(Macos, "Cmd tap", Short), "Cmd tap");
        assert_eq!(d(Macos, "Meta tap", Short), "Control tap");
        assert_eq!(describe_spec_for(Windows, "Hyper+X", Spoken), None);
    }

    /// The reserved lists, in role terms: on a Mac Command is `Ctrl` (or `Cmd`) in a spec and
    /// Control is `Win` (or `Meta`), so `"Ctrl+Q"` is Command+Q there and `"Win+Q"` is not.
    #[test]
    fn the_system_keeps_what_it_keeps_and_nothing_near_it() {
        let reserved = |os, s: &str| {
            let (vk, mask) = key_spec(s).unwrap();
            reserved_for(os, vk, mask).is_some()
        };
        assert!(reserved(Windows, "F12"));
        assert!(!reserved(Windows, "Shift+F12"));
        assert!(!reserved(Windows, "Ctrl+F12"));
        assert!(reserved(Windows, "Win+L"));
        assert!(reserved(Windows, "Meta+L"), "Meta is the Win role on Windows too");
        assert!(!reserved(Windows, "Win+Shift+L"));
        for s in ["Ctrl+H", "Ctrl+Q", "Ctrl+Tab", "Ctrl+Space", "Ctrl+F5", "Ctrl+Alt+F5", "Cmd+L"] {
            assert!(!reserved(Windows, s), "{s} is nobody's on Windows");
        }
        for s in [
            "Ctrl+F5", "Cmd+F5", "Ctrl+Alt+F5", "Cmd+Option+F5", "Ctrl+Tab", "Ctrl+Shift+Tab",
            "Ctrl+Space", "Ctrl+H", "Ctrl+M", "Ctrl+Q", "Command+Q", "Ctrl+Shift+Q", "Ctrl+Win+Q",
            "Cmd+Meta+Q", "Cmd+Option+Shift+Q", "Ctrl+Alt+Esc", "Ctrl+Shift+3", "Ctrl+Shift+4",
            "Ctrl+Shift+5",
        ] {
            assert!(reserved(Macos, s), "{s} on a Mac");
        }
        for s in [
            "F5", "Ctrl+Shift+F5", "Win+F5", "Meta+Alt+F5", "Win+Q", "Win+Tab", "Meta+Tab",
            "Meta+Space", "Win+H", "Option+H", "Ctrl+Shift+6", "Ctrl+W", "Ctrl+L", "Alt+Q",
            "Meta+Cmd+Shift+Q",
        ] {
            assert!(!reserved(Macos, s), "{s} on a Mac");
        }
        assert!(!reserved(Macos, "F12"), "F12 is the debugger's on Windows only");
        assert!(!reserved(Macos, "Win+L"), "Control+L on a Mac");
        for s in ["F12", "Win+L", "Ctrl+Q", "Cmd+Q"] {
            assert!(!reserved(Linux, s));
        }
    }

    /// `check`'s structural reasons, per platform, in role terms: VoiceOver's Control+Option is
    /// Win+Alt in a spec, and Ctrl+Alt is Command+Option on a Mac, which is off it.
    #[test]
    fn check_names_the_structure_and_not_a_list() {
        let none = |_: u32, _: u8| -> Option<(String, bool)> { None };
        for spec in ["Win+Alt+X", "Meta+Option+X", "Super+Alt+X", "Ctrl+Shift+Win+Alt+F6"] {
            let c = check_spec_for(Macos, spec, none);
            assert_eq!(c.reasons, vec![REASON_VOICEOVER], "{spec}");
            assert!(!c.ok(), "{spec}");
            assert!(check_spec_for(Windows, spec, none).ok(), "{spec} on Windows");
        }
        assert_eq!(check_spec_for(Macos, "Win+Alt+X", none).resolved.as_deref(), Some("Meta+Option+X"));
        for spec in ["Ctrl+Alt+X", "Cmd+Option+X", "Ctrl+Alt+Shift+S", "Cmd+Shift+F6", "Win+X", "Alt+X"] {
            assert!(check_spec_for(Macos, spec, none).reasons.is_empty(), "{spec} on a Mac");
        }
        assert_eq!(check_spec_for(Macos, "F21", none).reasons, vec![REASON_NO_KEYCODE]);
        assert!(check_spec_for(Windows, "F21", none).ok());
        let c = check_spec_for(Windows, "F12", none);
        assert_eq!(c.reasons, vec![REASON_RESERVED]);
        assert!(!c.ok());
        let c = check_spec_for(Macos, "Ctrl+Q", none);
        assert_eq!(c.reasons, vec![REASON_RESERVED]);
        assert_eq!(c.resolved.as_deref(), Some("Cmd+Q"));
        let c = check_spec_for(Windows, "Hyper+X", none);
        assert_eq!(c.reasons, vec![REASON_PARSE]);
        assert_eq!(c.resolved, None);
        assert!(!c.ok());
        // A tap works — as the capture it is — and is said, because no hotkey can be one.
        for os in ALL {
            let c = check_spec_for(os, "Alt tap", |_, _| panic!("a tap types nothing"));
            assert_eq!(c.reasons, vec![REASON_TAP], "{os:?}");
            assert!(c.ok(), "{os:?}");
        }
    }

    /// The layout is asked only where the answer can matter, and what it says is informational.
    #[test]
    fn check_asks_the_layout_only_for_altgr_and_option() {
        let never = |_: u32, _: u8| -> Option<(String, bool)> { panic!("the layout was asked") };
        let at = |_: u32, _: u8| Some(("@".to_string(), false));
        let c = check_spec_for(Windows, "Ctrl+Alt+Q", at);
        assert_eq!(c.reasons, vec![REASON_ALTGR]);
        assert_eq!(c.produces.as_deref(), Some("@"));
        assert!(c.ok(), "AltGr costs a character; the key still fires");
        assert!(check_spec_for(Windows, "Ctrl+Alt+Q", |_, _| None).reasons.is_empty());
        check_spec_for(Windows, "Ctrl+Alt+Win+Q", never);
        check_spec_for(Windows, "Ctrl+Shift+Win+Alt+F6", never);
        check_spec_for(Windows, "Alt+Q", never);
        let c = check_spec_for(Macos, "Alt+E", |vk, mask| {
            assert_eq!((vk, mask), (0x45, MASK_ALT));
            Some(("\u{b4}".to_string(), true))
        });
        assert_eq!(c.reasons, vec![REASON_COMPOSES]);
        assert!(c.dead_key);
        assert!(c.ok());
        check_spec_for(Macos, "Alt+Shift+E", |vk, mask| {
            assert_eq!((vk, mask), (0x45, MASK_ALT | MASK_SHIFT));
            None
        });
        // Option with Command (Ctrl) or with Control (Win) is a shortcut, not a character.
        check_spec_for(Macos, "Ctrl+Alt+E", never);
        check_spec_for(Macos, "Cmd+Alt+E", never);
        check_spec_for(Macos, "Win+Alt+E", never);
        check_spec_for(Linux, "Ctrl+Alt+Q", never);
    }

    /// What no operating system can hold is refused when the module asks, with the reason —
    /// not left to the claim, where it used to end in a dialog blaming another application.
    /// A key the system keeps or has no key code for is answered (`taken`, `not_supported`); a tap
    /// raises on every platform.
    #[test]
    fn register_refuses_what_can_never_be_held() {
        use crate::errors::hotkey;
        let answered = |r: &KeyRefusal| match r {
            KeyRefusal::Answer(e) => Some(e.reason()),
            KeyRefusal::Mistake(_) => None,
        };
        let why = hotkey_refusal_for(Windows, "F12").unwrap();
        assert_eq!(answered(&why), Some(&hotkey::KEPT_BY_SYSTEM));
        let why = why.to_string();
        assert!(why.contains("'F12'") && why.contains("debugger"), "{why}");
        let why = hotkey_refusal_for(Macos, "Ctrl+Q").unwrap();
        assert_eq!(answered(&why), Some(&hotkey::KEPT_BY_SYSTEM));
        let why = why.to_string();
        assert!(why.contains("'Ctrl+Q' (Cmd+Q)") && why.contains("quits"), "{why}");
        assert_eq!(hotkey_refusal_for(Windows, "Ctrl+Q"), None);
        assert_eq!(hotkey_refusal_for(Macos, "Win+Q"), None, "Control+Q on a Mac");
        for os in ALL {
            let why = hotkey_refusal_for(os, "Alt tap").unwrap();
            assert_eq!(answered(&why), None, "a tap is a mistake in the call: {os:?}");
            let why = why.to_string();
            assert!(why.contains("on its own") && why.contains("host.keys.capture"), "{os:?}: {why}");
        }
        let why = hotkey_refusal_for(Macos, "Shift+F21").unwrap();
        assert_eq!(answered(&why), Some(&hotkey::NO_KEYCODE));
        let why = why.to_string();
        assert!(why.contains("no key code for F21"), "{why}");
        assert_eq!(hotkey_refusal_for(Windows, "F21"), None, "Windows has F21");
        assert_eq!(hotkey_refusal_for(Windows, "Ctrl+F12"), None);
        assert_eq!(hotkey_refusal_for(Macos, "Win+Alt+X"), None, "VoiceOver's is said, not refused");
        assert_eq!(hotkey_refusal_for(Windows, "Hyper+X"), None, "the parser answers that one");
    }

    /// What `host.hotkey.register` hands on: the canonical spelling for a spec it accepts, the
    /// author's for one that does not parse (the backend's parser raises for it), and for a
    /// refused one no spec at all — the refusal is all there is.
    #[test]
    fn a_claim_is_the_canonical_spec_and_a_refusal_claims_nothing() {
        assert_eq!(hotkey_claim_for(Windows, "ctrl+s"), Ok((Some((0x53, MASK_CTRL)), "Ctrl+S".into())));
        assert_eq!(
            hotkey_claim_for(Windows, "Ctrl+Shift+Win+Alt+F6"),
            Ok((Some((0x75, 0x0F)), "Ctrl+Alt+Shift+Win+F6".into()))
        );
        assert_eq!(
            hotkey_claim_for(Macos, "Cmd+Shift+F6"),
            Ok((Some((0x75, MASK_SHIFT | MASK_CTRL)), "Shift+Cmd+F6".into()))
        );
        assert_eq!(hotkey_claim_for(Macos, "Ctrl+S"), Ok((Some((0x53, MASK_CTRL)), "Cmd+S".into())));
        assert_eq!(hotkey_claim_for(Windows, "Hyper+X"), Ok((None, "Hyper+X".into())));
        for (os, spec) in [(Windows, "F12"), (Windows, "Win+L"), (Macos, "Ctrl+Q"), (Macos, "F21")] {
            assert_eq!(hotkey_claim_for(os, spec), Err(hotkey_refusal_for(os, spec).unwrap()));
        }
        for os in ALL {
            assert!(hotkey_claim_for(os, "Alt tap").is_err(), "{os:?}");
        }
    }

    /// `register` refuses exactly the specs whose `check` carries a reason
    /// [`reason_refuses_hotkey`] names — the list the overlay runtime skips its control hotkeys
    /// by, so a key the runtime asks for is never one the host raises for.
    #[test]
    fn check_says_what_register_refuses() {
        let specs = [
            "F12", "Win+L", "Ctrl+Q", "Cmd+Q", "Win+Q", "Ctrl+F5", "Cmd+Alt+F5", "Ctrl+Shift+3",
            "Ctrl+Tab", "Meta+Tab", "Alt tap", "Ctrl tap", "F21", "Ctrl+F24", "Ctrl+Alt+X",
            "Win+Alt+X", "Ctrl+Alt+Q", "Ctrl+Shift+Win+Alt+F6", "Cmd+Shift+F6", "Ctrl+Shift+F9",
            "Alt+P", "Tab", "Hyper+X", "Mod+S",
        ];
        for os in ALL {
            for s in specs {
                let c = check_spec_for(os, s, |_, _| None);
                let says = c.reasons.iter().any(|r| *r != REASON_PARSE && reason_refuses_hotkey(r));
                assert_eq!(hotkey_refusal_for(os, s).is_some(), says, "{os:?} {s}: {:?}", c.reasons);
            }
        }
    }

    /// The capture callback's modifier table names the roles: `ctrl` is the Ctrl role — Command
    /// on a Mac — and `win` the Win role, Control there. (The Mac's flags become that mask in
    /// `macos/keys.rs`, whose test composes the two.)
    #[test]
    fn the_capture_table_is_the_roles() {
        assert_eq!(
            capture_mods_fields(MASK_CTRL | MASK_SHIFT),
            [("shift", true), ("ctrl", true), ("alt", false), ("win", false)]
        );
        assert_eq!(
            capture_mods_fields(MASK_WIN | MASK_ALT),
            [("shift", false), ("ctrl", false), ("alt", true), ("win", true)]
        );
        assert!(capture_mods_fields(0).iter().all(|(_, held)| !held));
        assert!(capture_mods_fields(MASK_TAP).iter().all(|(_, held)| !held));
    }

    /// The dialogs name a key by the spec on Windows and Linux, unchanged, and in the Mac's
    /// spoken words on a Mac, where the spec calls the Control key `Meta`.
    #[test]
    fn the_dialogs_say_a_mac_key_in_its_words() {
        for spec in ["Ctrl+Alt+Shift+Win+F5", "Ctrl+Shift+F9", "Alt+B", "Ctrl+Return", "Mod+S"] {
            assert_eq!(dialog_key_words_for(Windows, spec), spec);
            assert_eq!(dialog_key_words_for(Linux, spec), spec);
        }
        assert_eq!(dialog_key_words_for(Macos, "Shift+Cmd+F5"), "Shift+Command+F5");
        assert_eq!(dialog_key_words_for(Macos, "Meta+Shift+F6"), "Control+Shift+F6");
        assert_eq!(
            dialog_key_words_for(Macos, "Meta+Option+Shift+Cmd+F8"),
            "Control+Option+Shift+Command+F8"
        );
        assert_eq!(dialog_key_words_for(Macos, "Option+B"), "Option+B");
        // Not a spec: shown as the module wrote it.
        assert_eq!(dialog_key_words_for(Macos, "Mod+S"), "Mod+S");
    }

    /// A capture of a combination macOS keeps is said in the log on a Mac, with the reason;
    /// nothing is said for a free one, and nothing on Windows, whose hook captures F12 like any
    /// other key.
    #[test]
    fn a_reserved_capture_is_said_on_a_mac_only() {
        let line = |os, s: &str| {
            let (vk, mask) = key_spec(s).unwrap();
            reserved_capture_line(os, vk, mask)
        };
        let q = line(Macos, "Ctrl+Q").expect("Command+Q is the system's");
        assert!(q.contains("'Cmd+Q'") && q.contains("Command+Q quits"), "{q}");
        let tab = line(Macos, "Ctrl+Tab").expect("Command+Tab is the system's");
        assert!(tab.contains("'Cmd+Tab'") && tab.contains("application switcher"), "{tab}");
        for s in ["Meta+Tab", "Meta+Shift+Tab", "Ctrl+1", "Tab", "Ctrl+Alt+Shift+S", "Ctrl+L"] {
            assert_eq!(line(Macos, s), None, "{s} is free on a Mac");
        }
        for s in ["F12", "Win+L", "Ctrl+Q", "Ctrl+Tab"] {
            assert_eq!(line(Windows, s), None, "{s} on Windows");
            assert_eq!(line(Linux, s), None, "{s} on Linux");
        }
    }

    /// Every key a module or tool asks for reads on Windows exactly as it did before the roles
    /// (the table is what the host answered for them on 2026-09-22, before the change; the
    /// tokens' keys under the literal spellings they went back to). Windows is unchanged: its
    /// roles are its keys.
    #[test]
    fn windows_answers_are_unchanged_for_every_shipped_key() {
        #[rustfmt::skip]
        const BEFORE: &[(&str, u32, u8, &str, &str, &str, &[&str])] = &[
        ("1", 0x31, 0, "1", "1", "1", &[]),
        ("A", 0x41, 0, "A", "A", "A", &[]),
        ("Alt tap", 0x12, 16, "Alt tap", "Alt pressed on its own", "Alt tap", &["tap"]),
        ("Alt+1", 0x31, 4, "Alt+1", "Alt+1", "Alt+1", &[]),
        ("Alt+2", 0x32, 4, "Alt+2", "Alt+2", "Alt+2", &[]),
        ("Alt+3", 0x33, 4, "Alt+3", "Alt+3", "Alt+3", &[]),
        ("Alt+8", 0x38, 4, "Alt+8", "Alt+8", "Alt+8", &[]),
        ("Alt+9", 0x39, 4, "Alt+9", "Alt+9", "Alt+9", &[]),
        ("Alt+B", 0x42, 4, "Alt+B", "Alt+B", "Alt+B", &[]),
        ("Alt+C", 0x43, 4, "Alt+C", "Alt+C", "Alt+C", &[]),
        ("Alt+E", 0x45, 4, "Alt+E", "Alt+E", "Alt+E", &[]),
        ("Alt+F", 0x46, 4, "Alt+F", "Alt+F", "Alt+F", &[]),
        ("Alt+H", 0x48, 4, "Alt+H", "Alt+H", "Alt+H", &[]),
        ("Alt+L", 0x4c, 4, "Alt+L", "Alt+L", "Alt+L", &[]),
        ("Alt+M", 0x4d, 4, "Alt+M", "Alt+M", "Alt+M", &[]),
        ("Alt+N", 0x4e, 4, "Alt+N", "Alt+N", "Alt+N", &[]),
        ("Alt+P", 0x50, 4, "Alt+P", "Alt+P", "Alt+P", &[]),
        ("Alt+Q", 0x51, 4, "Alt+Q", "Alt+Q", "Alt+Q", &[]),
        ("Alt+R", 0x52, 4, "Alt+R", "Alt+R", "Alt+R", &[]),
        ("Alt+S", 0x53, 4, "Alt+S", "Alt+S", "Alt+S", &[]),
        ("Alt+V", 0x56, 4, "Alt+V", "Alt+V", "Alt+V", &[]),
        ("Alt+W", 0x57, 4, "Alt+W", "Alt+W", "Alt+W", &[]),
        ("Alt+X", 0x58, 4, "Alt+X", "Alt+X", "Alt+X", &[]),
        ("Alt+Y", 0x59, 4, "Alt+Y", "Alt+Y", "Alt+Y", &[]),
        ("B", 0x42, 0, "B", "B", "B", &[]),
        ("Control+Alt+H", 0x48, 6, "Ctrl+Alt+H", "Control+Alt+H", "Ctrl+Alt+H", &[]),
        ("Control+L", 0x4c, 2, "Ctrl+L", "Control+L", "Ctrl+L", &[]),
        ("Control+Option+H", 0x48, 6, "Ctrl+Alt+H", "Control+Alt+H", "Ctrl+Alt+H", &[]),
        ("Ctrl tap", 0x11, 16, "Ctrl tap", "Control pressed on its own", "Ctrl tap", &["tap"]),
        ("Ctrl+1", 0x31, 2, "Ctrl+1", "Control+1", "Ctrl+1", &[]),
        ("Ctrl+2", 0x32, 2, "Ctrl+2", "Control+2", "Ctrl+2", &[]),
        ("Ctrl+9", 0x39, 2, "Ctrl+9", "Control+9", "Ctrl+9", &[]),
        ("Ctrl+Alt+1", 0x31, 6, "Ctrl+Alt+1", "Control+Alt+1", "Ctrl+Alt+1", &[]),
        ("Ctrl+Alt+3", 0x33, 6, "Ctrl+Alt+3", "Control+Alt+3", "Ctrl+Alt+3", &[]),
        ("Ctrl+Alt+C", 0x43, 6, "Ctrl+Alt+C", "Control+Alt+C", "Ctrl+Alt+C", &[]),
        ("Ctrl+Alt+H", 0x48, 6, "Ctrl+Alt+H", "Control+Alt+H", "Ctrl+Alt+H", &[]),
        ("Ctrl+Alt+I", 0x49, 6, "Ctrl+Alt+I", "Control+Alt+I", "Ctrl+Alt+I", &[]),
        ("Ctrl+Alt+O", 0x4f, 6, "Ctrl+Alt+O", "Control+Alt+O", "Ctrl+Alt+O", &[]),
        ("Ctrl+Alt+S", 0x53, 6, "Ctrl+Alt+S", "Control+Alt+S", "Ctrl+Alt+S", &[]),
        ("Ctrl+Alt+Shift+S", 0x53, 7, "Ctrl+Alt+Shift+S", "Control+Alt+Shift+S", "Ctrl+Alt+Shift+S", &[]),
        ("Ctrl+Alt+Shift+T", 0x54, 7, "Ctrl+Alt+Shift+T", "Control+Alt+Shift+T", "Ctrl+Alt+Shift+T", &[]),
        ("Ctrl+Alt+Shift+V", 0x56, 7, "Ctrl+Alt+Shift+V", "Control+Alt+Shift+V", "Ctrl+Alt+Shift+V", &[]),
        ("Ctrl+Alt+U", 0x55, 6, "Ctrl+Alt+U", "Control+Alt+U", "Ctrl+Alt+U", &[]),
        ("Ctrl+Alt+Win+F8", 0x77, 14, "Ctrl+Alt+Win+F8", "Control+Alt+Windows+F8", "Ctrl+Alt+Win+F8", &[]),
        ("Ctrl+L", 0x4c, 2, "Ctrl+L", "Control+L", "Ctrl+L", &[]),
        ("Ctrl+N", 0x4e, 2, "Ctrl+N", "Control+N", "Ctrl+N", &[]),
        ("Ctrl+P", 0x50, 2, "Ctrl+P", "Control+P", "Ctrl+P", &[]),
        ("Ctrl+R", 0x52, 2, "Ctrl+R", "Control+R", "Ctrl+R", &[]),
        ("Ctrl+S", 0x53, 2, "Ctrl+S", "Control+S", "Ctrl+S", &[]),
        ("Ctrl+Shift+F10", 0x79, 3, "Ctrl+Shift+F10", "Control+Shift+F10", "Ctrl+Shift+F10", &[]),
        ("Ctrl+Shift+F11", 0x7a, 3, "Ctrl+Shift+F11", "Control+Shift+F11", "Ctrl+Shift+F11", &[]),
        ("Ctrl+Shift+F9", 0x78, 3, "Ctrl+Shift+F9", "Control+Shift+F9", "Ctrl+Shift+F9", &[]),
        ("Ctrl+Shift+N", 0x4e, 3, "Ctrl+Shift+N", "Control+Shift+N", "Ctrl+Shift+N", &[]),
        ("Ctrl+Shift+P", 0x50, 3, "Ctrl+Shift+P", "Control+Shift+P", "Ctrl+Shift+P", &[]),
        ("Ctrl+Shift+Tab", 0x09, 3, "Ctrl+Shift+Tab", "Control+Shift+Tab", "Ctrl+Shift+Tab", &[]),
        ("Ctrl+Shift+Win+Alt+F5", 0x74, 15, "Ctrl+Alt+Shift+Win+F5", "Control+Alt+Shift+Windows+F5", "Ctrl+Alt+Shift+Win+F5", &[]),
        ("Ctrl+Shift+Win+Alt+F6", 0x75, 15, "Ctrl+Alt+Shift+Win+F6", "Control+Alt+Shift+Windows+F6", "Ctrl+Alt+Shift+Win+F6", &[]),
        ("Ctrl+Tab", 0x09, 2, "Ctrl+Tab", "Control+Tab", "Ctrl+Tab", &[]),
        ("Ctrl+U", 0x55, 2, "Ctrl+U", "Control+U", "Ctrl+U", &[]),
        ("Down", 0x28, 0, "Down", "Down Arrow", "Down", &[]),
        ("Escape", 0x1b, 0, "Escape", "Escape", "Esc", &[]),
        ("F1", 0x70, 0, "F1", "F1", "F1", &[]),
        ("F10", 0x79, 0, "F10", "F10", "F10", &[]),
        ("F2", 0x71, 0, "F2", "F2", "F2", &[]),
        ("F3", 0x72, 0, "F3", "F3", "F3", &[]),
        ("F4", 0x73, 0, "F4", "F4", "F4", &[]),
        ("F5", 0x74, 0, "F5", "F5", "F5", &[]),
        ("F6", 0x75, 0, "F6", "F6", "F6", &[]),
        ("Left", 0x25, 0, "Left", "Left Arrow", "Left", &[]),
        ("Meta+Shift+Tab", 0x09, 9, "Shift+Win+Tab", "Shift+Windows+Tab", "Shift+Win+Tab", &[]),
        ("Meta+Tab", 0x09, 8, "Win+Tab", "Windows+Tab", "Win+Tab", &[]),
        ("Option+P", 0x50, 4, "Alt+P", "Alt+P", "Alt+P", &[]),
        ("Option+V", 0x56, 4, "Alt+V", "Alt+V", "Alt+V", &[]),
        ("Q", 0x51, 0, "Q", "Q", "Q", &[]),
        ("Return", 0x0d, 0, "Return", "Enter", "Enter", &[]),
        ("Right", 0x27, 0, "Right", "Right Arrow", "Right", &[]),
        ("Shift tap", 0x10, 16, "Shift tap", "Shift pressed on its own", "Shift tap", &["tap"]),
        ("Shift+Tab", 0x09, 1, "Shift+Tab", "Shift+Tab", "Shift+Tab", &[]),
        ("Space", 0x20, 0, "Space", "Space", "Space", &[]),
        ("Tab", 0x09, 0, "Tab", "Tab", "Tab", &[]),
        ("Tab      ", 0x09, 0, "Tab", "Tab", "Tab", &[]),
        ("Up", 0x26, 0, "Up", "Up Arrow", "Up", &[]),
        ("Win tap", 0x5b, 16, "Win tap", "Windows pressed on its own", "Win tap", &["tap"]),
        ("c", 0x43, 0, "C", "C", "C", &[]),
        ("down", 0x28, 0, "Down", "Down Arrow", "Down", &[]),
        ("k", 0x4b, 0, "K", "K", "K", &[]),
        ("left", 0x25, 0, "Left", "Left Arrow", "Left", &[]),
        ("right", 0x27, 0, "Right", "Right Arrow", "Right", &[]),
        ("s", 0x53, 0, "S", "S", "S", &[]),
        ("up", 0x26, 0, "Up", "Up Arrow", "Up", &[]),
        ("x", 0x58, 0, "X", "X", "X", &[]),
        ];
        assert!(BEFORE.len() > 80);
        for &(spec, vk, mask, normalized, spoken, short, reasons) in BEFORE {
            assert_eq!(key_spec(spec), Some((vk, mask)), "{spec}");
            assert_eq!(normalize_spec_for(Windows, spec).as_deref(), Some(normalized), "{spec}");
            assert_eq!(describe_spec_for(Windows, spec, KeyStyle::Spoken).as_deref(), Some(spoken), "{spec}");
            assert_eq!(describe_spec_for(Windows, spec, KeyStyle::Short).as_deref(), Some(short), "{spec}");
            let c = check_spec_for(Windows, spec, |_, _| None);
            assert_eq!(c.reasons, reasons, "{spec}");
            assert_eq!(c.resolved.as_deref(), Some(normalized), "{spec}");
            let tap = mask & MASK_TAP != 0;
            assert_eq!(hotkey_refusal_for(Windows, spec).is_some(), tap, "{spec}");
            if !tap {
                assert_eq!(hotkey_claim_for(Windows, spec), Ok((Some((vk, mask)), normalized.to_string())));
            }
        }
    }

    /// No key a shipped module, tool or example asks for becomes refused, on either platform.
    ///
    /// Read from the sources rather than listed, so a key added later is checked too: every
    /// string literal in their Luau that parses as a key spec. A literal on a line that is one
    /// platform's entry of a `host.os.pick` (`windows = "…"`, `macos = "…"`) is that platform's
    /// only — the overlay runtime's tab keys are Ctrl+Tab on Windows and Control+Tab (`Meta+Tab`)
    /// on a Mac, where Ctrl+Tab would be Command+Tab, the application switcher.
    #[test]
    fn no_shipped_key_is_refused() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        let mut stack: Vec<std::path::PathBuf> =
            ["modules", "tools", "examples"].iter().map(|d| root.join(d)).collect();
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "luau") {
                    files.push(p);
                }
            }
        }
        assert!(files.len() > 20, "the module sources were not found under {}", root.display());
        let mut seen = 0;
        let mut picked_for_a_mac = 0;
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap();
            // Per line, so one stray quote in a comment cannot turn the rest of the file inside
            // out; comments are scanned too, which only means more specs checked.
            for line in text.lines() {
                let oses: &[KeyOs] = if line.contains("macos =") {
                    &[Macos]
                } else if line.contains("windows =") || line.contains("linux =") {
                    &[Windows]
                } else {
                    &[Windows, Macos]
                };
                for lit in line.split('"').skip(1).step_by(2) {
                    // A tap is a capture only; one written in a module is one it captures.
                    let tap = key_spec(lit).is_some_and(|(_, m)| m & MASK_TAP != 0);
                    if lit.len() > 40 || key_spec(lit).is_none() || tap {
                        continue;
                    }
                    seen += 1;
                    if oses == [Macos] {
                        picked_for_a_mac += 1;
                    }
                    for &os in oses {
                        assert_eq!(hotkey_refusal_for(os, lit), None, "{} asks for '{lit}'", f.display());
                    }
                }
            }
        }
        assert!(seen > 20, "only {seen} key specs found; the scan is not reading the modules");
        assert!(picked_for_a_mac >= 2, "the pick entries were not recognised ({picked_for_a_mac})");
    }
}

/// Step K's one decision for the Windows hook and the Mac tap, as a table — and the two small
/// rules beside it, the menu record per module and the Mac's window for a scope.
#[cfg(test)]
mod capture_decision_tests {
    use super::*;

    const W1: isize = 0x1001;
    const W2: isize = 0x2002;
    const TAB: u32 = 0x09;
    const SPACE: u32 = 0x20;

    fn cap(owner: u32, vk: u32, mask: u8) -> Captured {
        Captured { vk, mask, owner }
    }

    fn own(owner: u32, scope: isize, menu: bool) -> OwnerKeys {
        OwnerKeys { owner, scope, menu }
    }

    fn take(owner: u32) -> Option<Capture> {
        Some(Capture::Take { owner })
    }

    fn pass(why: PassWhy, owner: Option<u32>) -> Option<Capture> {
        Some(Capture::Pass { why, owner })
    }

    fn no_menu() -> bool {
        false
    }

    /// In scope, out of scope, everywhere; the earliest capture in scope beats an earlier one
    /// scoped elsewhere; no capture at all; the mask matched exactly.
    #[test]
    fn the_earliest_capture_in_scope_takes_the_key() {
        // Module 1 captured Tab first, scoped to W1; module 2 next, scoped to W2; module 3
        // last, never scoped.
        let set = [cap(1, TAB, 0), cap(2, TAB, 0), cap(3, TAB, 0), cap(1, SPACE, 0)];
        let owners = [own(1, W1, false), own(2, W2, false)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W1, false, no_menu), take(1));
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W2, false, no_menu), take(2), "1's is earlier, but scoped to W1");
        assert_eq!(capture_decision(&set, &owners, TAB, 0, 0x3003, false, no_menu), take(3), "everywhere");
        assert_eq!(
            capture_decision(&set, &owners, SPACE, 0, W2, false, no_menu),
            pass(PassWhy::OutOfScope, None),
            "only module 1 captures Space, scoped to W1: to the application"
        );
        assert_eq!(capture_decision(&set, &owners, SPACE, 0, W1, false, no_menu), take(1));
        assert_eq!(capture_decision(&set, &owners, 0x0D, 0, W1, false, no_menu), None, "nobody captures Return");
        assert_eq!(capture_decision(&set, &owners, TAB, MASK_SHIFT, W1, false, no_menu), None, "Shift+Tab is not Tab");
        assert_eq!(capture_decision(&set, &owners, TAB, MASK_ALT, W1, false, no_menu), None, "nor is Alt+Tab");
        // A module without an entry is everywhere; an entry with scope 0 is too.
        let owners = [own(1, 0, false)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W2, false, no_menu), take(1));
        assert_eq!(capture_decision(&set, &[], TAB, 0, W2, false, no_menu), take(1));
    }

    /// Two modules in one window share its scope: the earlier registration takes the key.
    #[test]
    fn in_one_window_the_earlier_registration_wins() {
        let set = [cap(2, TAB, 0), cap(1, TAB, 0)];
        let owners = [own(1, W1, false), own(2, W1, false)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W1, false, no_menu), take(2));
    }

    /// A menu the system drew lets a key in scope through, and is asked only once a capture is
    /// in scope — it costs a call on Windows.
    #[test]
    fn a_native_menu_lets_the_key_through_and_is_asked_only_in_scope() {
        let set = [cap(1, TAB, 0)];
        let owners = [own(1, W1, false)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W1, false, || true), pass(PassWhy::MenuMode, Some(1)));
        let asked = std::cell::Cell::new(false);
        let d = capture_decision(&set, &owners, TAB, 0, W2, false, || {
            asked.set(true);
            true
        });
        assert_eq!(d, pass(PassWhy::OutOfScope, None));
        assert!(!asked.get(), "out of scope: not asked");
        let d = capture_decision(&set, &owners, SPACE, 0, W1, false, || {
            asked.set(true);
            true
        });
        assert_eq!(d, None);
        assert!(!asked.get(), "nothing captured: not asked");
    }

    /// The menu flag, by the maintainer's answer to question 2: it counts for the window it was
    /// set for — the module's scope, or every window for a module that has none — and lets every
    /// capture there through, another module's included; but only while the module that set it
    /// holds a capture.
    #[test]
    fn a_menu_flag_counts_for_its_window_while_its_module_captures() {
        let set = [cap(1, TAB, 0), cap(2, TAB, 0), cap(2, SPACE, 0)];
        // Module 2's flag, scoped to W2: module 2's keys there, and module 1's in W2 too.
        let owners = [own(1, 0, false), own(2, W2, true)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W2, false, no_menu), pass(PassWhy::MenuFlag, Some(1)));
        assert_eq!(capture_decision(&set, &owners, SPACE, 0, W2, false, no_menu), pass(PassWhy::MenuFlag, Some(2)));
        // Not in W1: module 2's flag is not W1's.
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W1, false, no_menu), take(1));
        // A module with no scope says it for every window.
        let owners = [own(1, W1, false), own(2, 0, true)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W1, false, no_menu), pass(PassWhy::MenuFlag, Some(1)));
        assert_eq!(capture_decision(&set, &owners, TAB, 0, 0x3003, false, no_menu), pass(PassWhy::MenuFlag, Some(2)));
        // A flag whose module holds no capture counts nowhere.
        let owners = [own(1, 0, false), own(3, 0, true)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W1, false, no_menu), take(1));
        assert_eq!(menu_flag_owners(&set, &owners, W1).count(), 0);
        // Whose flags count, for the record of keys let through for a menu.
        let owners = [own(1, W1, true), own(2, 0, true), own(4, W1, true)];
        assert_eq!(menu_flag_owners(&set, &owners, W1).collect::<Vec<_>>(), vec![1, 2], "4 captures nothing");
        assert_eq!(menu_flag_owners(&set, &owners, W2).collect::<Vec<_>>(), vec![2]);
        // A flag never takes a key: with nothing in scope the reason is the scope.
        let owners = [own(1, W1, true), own(2, W1, false)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W2, false, no_menu), pass(PassWhy::OutOfScope, None));
    }

    /// One of this application's own windows has the keyboard: every captured combination goes
    /// to it, naming the module that would have taken it, before the scope's menu, a menu flag or
    /// the native menu is looked at — the native menu not even asked; a modifier tap with them; a
    /// combination nobody captures is still nobody's.
    #[test]
    fn our_own_window_with_the_keyboard_lets_every_capture_through() {
        let set = [cap(1, TAB, 0), cap(2, SPACE, 0), cap(1, 0x12, MASK_TAP)];
        let owners = [own(1, 0, false), own(2, W2, true)];
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W1, true, no_menu), pass(PassWhy::OwnWindow, Some(1)));
        assert_eq!(
            capture_decision(&set, &owners, SPACE, 0, W1, true, no_menu),
            pass(PassWhy::OwnWindow, None),
            "2 is scoped to W2: nobody would have taken it"
        );
        assert_eq!(
            capture_decision(&set, &owners, SPACE, 0, W2, true, no_menu),
            pass(PassWhy::OwnWindow, Some(2)),
            "ahead of 2's own menu flag"
        );
        assert_eq!(capture_decision(&set, &owners, 0x12, MASK_TAP, W1, true, no_menu), pass(PassWhy::OwnWindow, Some(1)));
        assert_eq!(capture_decision(&set, &owners, 0x0D, 0, W1, true, no_menu), None, "nobody captures Return");
        let asked = std::cell::Cell::new(false);
        let d = capture_decision(&set, &owners, TAB, 0, W1, true, || {
            asked.set(true);
            true
        });
        assert_eq!(d, pass(PassWhy::OwnWindow, Some(1)));
        assert!(!asked.get(), "the native menu is not asked");
        // Without it, the same keys as ever.
        assert_eq!(capture_decision(&set, &owners, TAB, 0, W1, false, no_menu), take(1));
        assert_eq!(capture_decision(&set, &owners, SPACE, 0, W2, false, no_menu), pass(PassWhy::MenuFlag, Some(2)));
    }

    /// The Mac tap asks whether one of our windows has the keyboard for key-downs only — at its
    /// decision, and just before it for the repeat of a key it swallowed — so what it records for
    /// every event goes on being recorded: a modifier's change (`MOD_DOWN`), the up of a down it
    /// swallowed (`take_suppressed`) and how late a key came all come first. Held where it is
    /// written, since no test here runs the tap.
    #[test]
    fn the_mac_tap_asks_for_our_own_windows_only_where_it_decides() {
        const TAP: &str = include_str!("macos/tap.rs");
        let body = |sig: &str| {
            let at = TAP.find(sig).unwrap_or_else(|| panic!("{sig}"));
            let end = TAP[at..].find("\n}\n").unwrap();
            &TAP[at..at + end]
        };
        assert_eq!(TAP.matches("own::has_keyboard()").count(), 2, "two places ask");
        let decide = body("fn decide(vk: u32, mask: u8, front: isize, menu_keys: bool)");
        let asked = decide.find("let own = super::own::has_keyboard();").expect("decide asks");
        let decided = decide
            .find("capture_decision(&state.set, &state.owners, vk, mask, front, own, watch::native_menu_open)")
            .expect("and hands the answer to the decision");
        assert!(asked < decided);
        assert!(decide.contains("if menu_keys && !own {"), "nothing is filed for a menu then");
        let cb = body("unsafe extern \"C-unwind\" fn tap_callback(");
        let at = cb.find("decide(vk, mask, front, true)").unwrap();
        let held = cb
            .find("if crate::backend::swallow_held_repeat(repeat, held_back(keycode), super::own::has_keyboard()) {")
            .expect("the repeat of a swallowed key is asked about");
        assert!(held < at, "before the decision, which would let it through");
        for first in ["modifier_changed(keycode, flags)", "take_suppressed(keycode)", "note_lateness(ev, keycode)", "TAP_ARMED.store(0, Ordering::Relaxed);"] {
            let f = cb.find(first).unwrap_or_else(|| panic!("{first}"));
            assert!(f < held, "{first} comes after where our window is asked");
        }
        assert!(cb[held..at].contains("return std::ptr::null_mut();"), "swallowed, not let through");
        assert!(!cb[held..at].contains("queue::push_key"), "nor handed to the module");
        let modifier = body("fn modifier_changed(keycode: u16, flags: CGEventFlags)");
        let recorded = modifier.find("MOD_DOWN.fetch_or(bit, Ordering::Relaxed);").unwrap();
        assert!(recorded < modifier.find("decide(modifier.vk, MASK_TAP, front, false)").unwrap(), "a tap is decided after it is recorded");
        assert!(body("fn pass_words(why: PassWhy)").contains("PassWhy::OwnWindow =>"));
    }

    /// A key held since a capture took it does not repeat into our own window, which would never
    /// see it come up; every other repeat, and every key pressed there, is decided as ever.
    #[test]
    fn a_held_captured_key_does_not_repeat_into_our_own_window() {
        assert!(swallow_held_repeat(true, true, true));
        assert!(!swallow_held_repeat(false, true, true), "a press of its own is decided");
        assert!(!swallow_held_repeat(true, false, true), "a key held in our window repeats there");
        assert!(!swallow_held_repeat(true, true, false), "anywhere else a repeat is decided as ever");
        assert!(!swallow_held_repeat(false, false, true));
        assert!(!swallow_held_repeat(false, false, false));
    }

    /// The modifier tap is decided as any key: its own mask, the same scopes and menus.
    #[test]
    fn a_modifier_tap_is_decided_as_any_key() {
        let set = [cap(1, 0x12, MASK_TAP), cap(2, 0x12, 0)];
        let owners = [own(1, W1, false)];
        assert_eq!(capture_decision(&set, &owners, 0x12, MASK_TAP, W1, false, no_menu), take(1));
        assert_eq!(capture_decision(&set, &owners, 0x12, MASK_TAP, W2, false, no_menu), pass(PassWhy::OutOfScope, None));
        assert_eq!(capture_decision(&set, &owners, 0x12, MASK_TAP, W1, false, || true), pass(PassWhy::MenuMode, Some(1)));
        let owners = [own(1, W1, true)];
        assert_eq!(capture_decision(&set, &owners, 0x12, MASK_TAP, W1, false, no_menu), pass(PassWhy::MenuFlag, Some(1)));
        assert_eq!(scope_of(&owners, 1), W1);
        assert_eq!(scope_of(&owners, 2), 0);
    }

    /// Each module's record of keys let through for a menu: filed in order, bounded per module,
    /// taken by its module only.
    #[test]
    fn the_menu_record_is_each_module_s_own() {
        let mut r = Vec::new();
        file_menu_pass(&mut r, 1, TAB, 0);
        file_menu_pass(&mut r, 2, 0x0D, 0);
        file_menu_pass(&mut r, 1, 0x1B, 0);
        assert_eq!(take_menu_passes(&mut r, 1), vec![(TAB, 0), (0x1B, 0)]);
        assert_eq!(take_menu_passes(&mut r, 1), vec![], "drained on read");
        for _ in 0..MENU_PASS_MAX + 5 {
            file_menu_pass(&mut r, 3, TAB, 0);
        }
        file_menu_pass(&mut r, 2, TAB, 0);
        assert_eq!(take_menu_passes(&mut r, 3).len(), MENU_PASS_MAX, "a reader that never comes");
        assert_eq!(take_menu_passes(&mut r, 2), vec![(0x0D, 0), (TAB, 0)], "another module's full record takes nothing from 2");
        assert!(r.is_empty());
    }

    /// The Mac's window for `host.keys.scope(true)`: the application's answer, else the window
    /// the tap was last told is in front, else everywhere.
    #[test]
    fn a_mac_scope_falls_back_to_the_window_the_tap_saw_in_front() {
        assert_eq!(key_scope_window(Some(W1), W2), (W1, ScopeFrom::Asked));
        assert_eq!(key_scope_window(None, W2), (W2, ScopeFrom::TapFront), "the application did not answer");
        assert_eq!(key_scope_window(Some(0), W2), (W2, ScopeFrom::TapFront), "it answered with no window");
        assert_eq!(key_scope_window(None, 0), (0, ScopeFrom::Nowhere));
        assert_eq!(key_scope_window(Some(0), 0), (0, ScopeFrom::Nowhere));
    }
}

/// This application's own windows on a Mac: their ids, what `ownsPoint` answers over them, their
/// frames, and that the accessibility backend never asks this process about them.
#[cfg(test)]
mod own_window_tests {
    use super::*;

    /// An own id is the window number above every handle the accessibility table hands out, and
    /// gives the number back; nothing else is one.
    #[test]
    fn an_own_id_is_the_window_number_above_every_handle() {
        assert_eq!(own_id(1), Some(OWN_BASE + 1));
        assert_eq!(own_id(0x1234), Some(OWN_BASE + 0x1234));
        assert_eq!(own_id(0), None, "a window never ordered in has no number");
        assert_eq!(own_id(-1), None);
        assert_eq!(own_id(i64::from(u32::MAX) + 1), None, "past what a window number holds");
        for n in [1u32, 77, 0x1234, u32::MAX] {
            assert_eq!(own_number(own_id(i64::from(n)).unwrap()), Some(n), "{n}");
        }
        for id in [0, 1, 4096, 1 << 39, OWN_BASE, OWN_BASE + u32::MAX as isize + 1, -1, -(OWN_BASE + 1), isize::MAX, isize::MIN] {
            assert_eq!(own_number(id), None, "{id:#x}");
        }
        // A Luau number holds the largest exactly.
        let top = own_id(i64::from(u32::MAX)).unwrap();
        assert_eq!(top as f64 as isize, top);
    }

    /// Only a page of ours refuses a point; another window of ours cannot say.
    #[test]
    fn only_a_page_of_ours_refuses_a_point() {
        assert_eq!(own_window_owns_point(OwnKind::Page), Some(false));
        assert_eq!(own_window_owns_point(OwnKind::Other), None);
    }

    /// An AppKit frame is flipped against the primary display's height, and rounded.
    #[test]
    fn an_appkit_frame_is_flipped_to_the_top_left() {
        // A 520×640 window whose bottom edge is 100 points above the bottom of a 900-point display.
        assert_eq!(cocoa_frame_top_left(40.0, 100.0, 520.0, 640.0, 900.0), (40, 160, 520, 640));
        // On a display above the primary one: above its top edge.
        assert_eq!(cocoa_frame_top_left(-200.0, 1000.0, 100.0, 50.0, 900.0), (-200, -150, 100, 50));
        assert_eq!(cocoa_frame_top_left(10.4, 0.0, 99.6, 20.5, 900.5), (10, 880, 100, 21));
    }

    /// Every entry point of the accessibility backend that could reach this process answers from
    /// AppKit first (`own.rs`), or answers "not known", before its first question through
    /// accessibility. Held where it is written: only a Mac runs it, and its log (MW17 in TODO.md)
    /// is the only other witness.
    #[test]
    fn our_own_windows_are_never_asked_through_accessibility() {
        const AX: &str = include_str!("macos/ax.rs");
        let body = |sig: &str| {
            let at = AX.find(sig).unwrap_or_else(|| panic!("{sig}"));
            let end = AX[at..].find("\n}\n").unwrap();
            &AX[at..at + end]
        };
        for (sig, ours, first_asked) in [
            ("pub fn foreground_window_id() -> Option<isize>", "if let Some(id) = super::own::key_id_in_front_of(front) {", "front_window_of(pid)"),
            ("pub fn foreground_window_id() -> Option<isize>", "if is_ours(pid) {\n        return Some(super::own::front_id());", "front_window_of(pid)"),
            ("pub fn foreground_window() -> Option<crate::backend::Foreground>", "if let Some(f) = super::own::foreground(front) {", "app_element(pid)"),
            ("pub fn foreground_window() -> Option<crate::backend::Foreground>", "if is_ours(pid) {", "app_element(pid)"),
            ("pub fn active_window() -> Option<WinInfo>", "if front_is_ours(front) {", "front_window_of(pid)"),
            ("pub fn window_info(handle: isize, require_title: bool)", "return super::own::info(handle, require_title);", "handles::get(handle)"),
            ("pub fn focus_window(handle: isize) -> Result<(), HostError>", "return super::own::focus(handle);", "handles::get(handle)"),
            ("pub(super) fn window_id(handle: isize) -> u32", "if let Some(n) = crate::backend::own_number(handle) {", "handles::get(handle)"),
            ("pub(super) fn frontmost_window_element()", "if is_ours(pid) {", "front_window_of(pid)"),
            ("pub fn window_focus_chain() -> Vec<ControlInfo>", "if keyboard_is_ours(front) {", "system_wide()"),
            ("pub fn focus_step(hwnd: isize, direction: i32)", "if keyboard_is_ours(frontmost_pid()) {", "walk("),
            ("pub fn focus_within(hwnd: isize, x: i32, y: i32, w: i32, h: i32)", "if keyboard_is_ours(frontmost_pid()) {", "walk("),
            ("fn enumerate_windows_in(pids: &[u32], timeout: f32, what: &str)", "out.extend(super::own::windows_info());", "app_element(pid)"),
            ("fn number_owns_point(ours: u32, x: i32, y: i32, what: &str)", "super::own::kind_of(hit as u32)", "window_owner(hit as u32)"),
        ] {
            let f = body(sig);
            let at = f.find(ours).unwrap_or_else(|| panic!("{sig}: no `{ours}`"));
            let asked = f.find(first_asked).unwrap_or_else(|| panic!("{sig}: no `{first_asked}`"));
            assert!(at < asked, "{sig}: `{first_asked}` comes before `{ours}`");
        }
        assert!(body("fn is_ours(pid: i32) -> bool").contains("pid == std::process::id() as i32"));
        // The reads of where the keyboard is go by AppKit's word inside this process as well as by
        // the workspace's, so neither one's lag lets a read reach this process: our window key
        // before the workspace names us would otherwise be asked through the system-wide element.
        let reads = body("fn keyboard_is_ours(front: Option<i32>) -> bool");
        assert!(reads.contains("front.is_some_and(is_ours) || super::own::has_keyboard()"));
        assert!(!reads.contains("key_id_in_front_of"), "not tied to the workspace");
        // The answers naming the window in front follow the application the workspace names, which
        // `watch` resolves an activation through.
        assert!(body("fn front_is_ours(front: Option<i32>) -> bool")
            .contains("front.is_some_and(is_ours) || super::own::key_id_in_front_of(front).is_some()"));
    }
}

/// The front on a Mac: this application taking it, and handing it to another application's window
/// (`macos/front.rs`) — the decision, which is pure, and where the code that only a Mac runs asks
/// and acts, held where it is written.
#[cfg(test)]
mod front_tests {
    use super::*;

    /// `host.window.focus` of another application's window, with this application the active one,
    /// both ways available and no focus waiting.
    fn focus_call() -> HandState {
        HandState {
            leaving_key: None,
            target_ours: false,
            target_front: false,
            ours_active: true,
            cooperative: true,
            pending: false,
        }
    }

    /// A window of ours hands the front over only while it has the keyboard: not even an
    /// application in front already is focused then.
    #[test]
    fn a_window_of_ours_hands_the_front_over_only_while_it_has_the_keyboard() {
        let not_key = HandState { leaving_key: Some(false), ..focus_call() };
        for how in [Handing::Deactivate, Handing::Yield] {
            assert_eq!(hand_plan(not_key, how), HandPlan::Keep(KeepWhy::NotKey));
            assert_eq!(hand_plan(HandState { target_front: true, ..not_key }, how), HandPlan::Keep(KeepWhy::NotKey));
            assert_eq!(hand_plan(HandState { ours_active: false, ..not_key }, how), HandPlan::Keep(KeepWhy::NotKey));
            assert_eq!(hand_plan(HandState { pending: true, ..not_key }, how), HandPlan::Keep(KeepWhy::NotKey));
        }
        let key = HandState { leaving_key: Some(true), ..focus_call() };
        assert_eq!(hand_plan(key, Handing::Deactivate), HandPlan::Ask(Handing::Deactivate));
        assert_eq!(hand_plan(key, Handing::Yield), HandPlan::Ask(Handing::Yield));
        // `host.window.focus` has no window of ours leaving, so nothing holds it back.
        assert_eq!(hand_plan(HandState { ours_active: false, ..focus_call() }, Handing::Deactivate), HandPlan::Ask(Handing::Deactivate));
    }

    /// A module that focused the plug-in before hiding its page: the page may still read as the key
    /// window, since a Mac tells a window it lost the keyboard after the activation that took it,
    /// and the focus asked for first stands. A newer `host.window.focus` replaces an older one.
    #[test]
    fn a_focus_still_waiting_is_not_overridden_by_a_window_of_ours_leaving() {
        for how in [Handing::Deactivate, Handing::Yield] {
            let leaving = HandState { leaving_key: Some(true), pending: true, ..focus_call() };
            assert_eq!(hand_plan(leaving, how), HandPlan::Keep(KeepWhy::Waiting));
            let at_front = HandState { target_front: true, ours_active: false, ..leaving };
            assert_eq!(hand_plan(at_front, how), HandPlan::Keep(KeepWhy::Waiting), "nor focused at once");
            assert_eq!(hand_plan(HandState { pending: true, ..focus_call() }, how), HandPlan::Ask(how), "the newer focus asks");
        }
    }

    /// An application in front already announces no activation, so its window is focused at once
    /// and nothing is asked: with a panel of ours that does not activate this application leaving,
    /// or with this application not the active one.
    #[test]
    fn an_application_in_front_already_is_focused_at_once() {
        for how in [Handing::Deactivate, Handing::Yield] {
            let inactive = HandState { target_front: true, ours_active: false, ..focus_call() };
            assert_eq!(hand_plan(inactive, how), HandPlan::AtOnce);
            let leaving = HandState { leaving_key: Some(true), ..inactive };
            assert_eq!(hand_plan(leaving, how), HandPlan::AtOnce, "a panel that does not activate this application");
        }
    }

    /// While a panel of ours that took the keyboard without activating this application has it,
    /// AppKit calls this application active (page-check, every CI runner) and the workspace names
    /// the DAW: the DAW is the active one, so the front goes back to it at once — its activation
    /// would never be announced — whether the panel leaves or a module focuses the plug-in window
    /// first; nor is (b) asked of an application with no activation to yield. Once the workspace
    /// names this application, another window of ours made it active in earnest, and AppKit's word
    /// stands; without the panel it always does.
    #[test]
    fn a_panel_that_does_not_activate_this_application_hands_the_front_back_at_once() {
        assert!(!active_for_handing(true, true, false), "AppKit's word, with the keyboard only lent");
        assert!(active_for_handing(true, true, true), "this application named the frontmost one");
        assert!(active_for_handing(true, false, false), "no such panel: the workspace may lag");
        assert!(!active_for_handing(false, true, false) && !active_for_handing(false, false, true));
        for how in [Handing::Deactivate, Handing::Yield] {
            let lent = HandState { target_front: true, ours_active: active_for_handing(true, true, false), ..focus_call() };
            assert_eq!(hand_plan(HandState { leaving_key: Some(true), ..lent }, how), HandPlan::AtOnce, "the panel leaving");
            assert_eq!(hand_plan(lent, how), HandPlan::AtOnce, "host.window.focus of the plug-in window");
            let elsewhere = HandState { target_front: false, ..lent };
            assert_eq!(hand_plan(elsewhere, how), HandPlan::Ask(Handing::Deactivate), "nothing to yield");
        }
    }

    /// The workspace's word lags this application's own activation: while this application is the
    /// active one, the application it still names is asked, not focused at once in the background.
    #[test]
    fn an_application_the_workspace_still_names_while_this_one_is_active_is_asked() {
        let stale = HandState { target_front: true, ..focus_call() };
        assert_eq!(hand_plan(stale, Handing::Deactivate), HandPlan::Ask(Handing::Deactivate));
        assert_eq!(hand_plan(stale, Handing::Yield), HandPlan::Ask(Handing::Yield));
        let leaving = HandState { leaving_key: Some(true), ..stale };
        assert_eq!(hand_plan(leaving, Handing::Deactivate), HandPlan::Ask(Handing::Deactivate));
    }

    /// (b) only where macOS has it and this application is the active one, the only one that can
    /// yield; (a) everywhere else.
    #[test]
    fn the_cooperative_way_only_where_it_exists_and_this_application_is_active() {
        assert_eq!(hand_plan(focus_call(), Handing::Yield), HandPlan::Ask(Handing::Yield));
        assert_eq!(hand_plan(HandState { cooperative: false, ..focus_call() }, Handing::Yield), HandPlan::Ask(Handing::Deactivate), "before macOS 14");
        assert_eq!(hand_plan(HandState { ours_active: false, ..focus_call() }, Handing::Yield), HandPlan::Ask(Handing::Deactivate), "nothing to yield");
        assert_eq!(hand_plan(focus_call(), Handing::Deactivate), HandPlan::Ask(Handing::Deactivate), "(a) is never turned into (b)");
    }

    /// The front is never handed to this application itself.
    #[test]
    fn this_application_is_not_handed_its_own_front() {
        assert_eq!(hand_plan(HandState { target_ours: true, ..focus_call() }, Handing::Deactivate), HandPlan::Keep(KeepWhy::Ours));
        assert_eq!(hand_plan(HandState { target_ours: true, target_front: true, ..focus_call() }, Handing::Yield), HandPlan::Keep(KeepWhy::Ours));
    }

    /// The log and `page-check` name each way and each reason apart.
    #[test]
    fn each_way_and_reason_has_its_own_words() {
        assert!(Handing::Deactivate.word().starts_with("(a)"));
        assert!(Handing::Yield.word().starts_with("(b)"));
        assert!(KeepWhy::NotKey.words().contains("keyboard"));
        let words = [KeepWhy::NotKey.words(), KeepWhy::Waiting.words(), KeepWhy::Ours.words()];
        for (i, a) in words.iter().enumerate() {
            for b in &words[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    /// The focus waits for its own application's activation; another application's gives it up,
    /// and this application's own — which can be announced after the hand-over — decides nothing.
    /// A key pressed since it was asked for gives it up at its application's activation: the user
    /// moved on, and nothing is raised or focused late.
    #[test]
    fn the_focus_waits_for_its_application_s_activation() {
        assert_eq!(on_activation(510, 510, 77, false), OnActivation::Focus);
        assert_eq!(on_activation(510, 510, 77, true), OnActivation::Typed);
        for typed in [false, true] {
            assert_eq!(on_activation(510, 611, 77, typed), OnActivation::Drop);
            assert_eq!(on_activation(510, 77, 77, typed), OnActivation::Wait);
        }
    }

    /// This application becoming the active one again gives a waiting focus up only after it had
    /// stopped being the active one since the focus was asked for: the user came back. A becoming
    /// with no stop before it was under way at the request, which is the newer one.
    #[test]
    fn this_application_back_in_front_gives_the_focus_up_only_after_it_had_left() {
        assert_eq!(on_own_edge(false, OwnEdge::Became), (false, false));
        let (resigned, gives_up) = on_own_edge(false, OwnEdge::Resigned);
        assert_eq!((resigned, gives_up), (true, false));
        assert_eq!(on_own_edge(resigned, OwnEdge::Resigned), (true, false));
        assert_eq!(on_own_edge(resigned, OwnEdge::Became), (true, true));
    }

    /// The body of the function `sig` in `src`, to its closing brace at the start of a line.
    fn body<'a>(src: &'a str, sig: &str) -> &'a str {
        let at = src.find(sig).unwrap_or_else(|| panic!("{sig}"));
        let end = src[at..].find("\n}\n").unwrap();
        &src[at..at + end]
    }

    /// The position of `what` in `f`, which has to be there.
    fn at(f: &str, what: &str) -> usize {
        f.find(what).unwrap_or_else(|| panic!("no `{what}`"))
    }

    /// The activation is asked for now and the window focused when the workspace announces it —
    /// before the activation's window is read, so the host hears the window asked for, and with the
    /// log's read of the focus chain after everything else on that path — and each way's calls come
    /// in its order. Held where it is written: only a Mac runs it, and `page-check` on CI is its
    /// other witness.
    #[test]
    fn the_front_is_asked_for_now_and_the_window_focused_on_the_event() {
        const WATCH: &str = include_str!("macos/watch.rs");
        const AX: &str = include_str!("macos/ax.rs");
        const FRONT: &str = include_str!("macos/front.rs");
        const APP: &str = include_str!("macos/app.rs");
        const TAP: &str = include_str!("macos/tap.rs");

        let activated = body(WATCH, "fn activated(app: &NSRunningApplication, announce: bool) {");
        let focused = at(activated, "super::front::activated(pid);");
        let read = at(activated, "super::ax::foreground_window_id()");
        let observed = at(activated, "ensure_observer(pid, &name);");
        let chain = at(activated, "super::front::report_chain();");
        assert!(focused < read && read < observed && observed < chain, "focus, window read, observer, then the log's chain");
        let notice = body(WATCH, "fn on_app_activated(notification: &NSNotification) {");
        assert!(at(notice, "if !STARTED.load(Ordering::SeqCst) {") < at(notice, "super::front::activated(app.processIdentifier());"));
        assert!(at(notice, "super::front::activated(app.processIdentifier());") < at(notice, "super::front::report_chain();"));
        assert!(at(notice, "super::front::report_chain();") < at(notice, "activated(app, true);"));
        assert!(body(WATCH, "pub fn start() -> Result<(), String> {").contains("hook_activations();"));

        // `host.window.focus` hands the front over; the raise, the focus and the read of the chain
        // are no longer made at the call.
        let focus = body(AX, "pub fn focus_window(handle: isize) -> Result<(), HostError> {");
        assert!(focus.contains("super::front::hand_front_to(entry.pid, handle, None, super::front::HANDING)"));
        for gone in ["AXRaise", "window_focus_chain()", "activateWithOptions"] {
            assert!(!focus.contains(gone), "{gone}");
        }

        // A focus still waiting is given up only when something is asked, and the notices it waits
        // on are subscribed to before the request.
        let hand = body(FRONT, "pub(super) fn hand_front_to(");
        let keep = at(hand, "HandPlan::Keep(why) => {");
        let at_once = at(hand, "HandPlan::AtOnce => {");
        let ask = at(hand, "HandPlan::Ask(planned) => {");
        assert!(!hand[keep..at_once].contains("cancel("), "a plan that asks nothing keeps the focus waiting");
        assert!(hand[at_once..ask].contains("cancel("));
        assert!(at(&hand[at_once..ask], "focus_now(pid, window, None)") < at(&hand[at_once..ask], "report_chain();"));
        let asking = &hand[ask..];
        let requested = at(asking, "request(app.as_deref(), active, &target, planned, leaving)");
        // Whether this application is the active one is AppKit's word, unless a panel of ours has
        // the keyboard only lent (`own::active`, `active_for_handing`), for the plan and for the
        // request alike.
        let active = at(hand, "let active = super::own::active(front);");
        assert!(active < at(hand, "ours_active: active,") && active < keep);
        assert!(!hand.contains("isActive()"), "never AppKit's word alone");
        // A panel that had the keyboard only lent leaving, its application in front already: where
        // AppKit gave the keyboard to another window of ours, this application lets it go — after
        // the order out, and with no activation asked of anybody.
        let leaves = at(&hand[at_once..ask], "w.orderOut(None);");
        assert!(leaves < at(&hand[at_once..ask], "keyboard_kept(app.as_deref(), w, &name);"));
        let kept = body(FRONT, "fn keyboard_kept(");
        let read = at(kept, "super::own::key_now(app)");
        let let_go = at(kept, "app.deactivate();");
        assert!(read < at(kept, "let Some(kept) = kept else { return };") && at(kept, "let Some(kept) = kept else { return };") < let_go);
        for call in ["activateWithOptions", "activateIgnoringOtherApps", "activate_self", ".activate("] {
            assert!(!kept.contains(call), "{call}");
        }
        for before in ["cancel(", "super::watch::hook_activations();", "hook_own();", "pressed: super::tap::pressed(),"] {
            assert!(at(asking, before) < requested, "{before}");
        }
        assert!(!body(FRONT, "fn focus_now(").contains("window_focus_chain()"), "the chain is read by report_chain alone");

        let request = body(FRONT, "fn request(");
        let yielded = at(request, "app.yieldActivationToApplication(target);");
        let from = at(request, "target.activateFromApplication_options(");
        let refused = at(request, "if taken {");
        let out_b = at(request, "w.orderOut(None);");
        assert!(yielded < from && from < refused && refused < out_b, "(b): yield, ask from this application, then order out if taken");
        // (b) refused falls through to (a): our window out, this application deactivated only for a
        // window of ours leaving, then the request.
        let rest = &request[out_b + 1..];
        let out_a = at(rest, "w.orderOut(None);");
        let only_leaving = at(rest, "if leaving.is_some() && active {");
        let deactivated = at(rest, "app.deactivate();");
        let asked = at(rest, "target.activateWithOptions(");
        assert!(out_a < only_leaving && only_leaving < deactivated && deactivated < asked);
        assert!(!request[..out_b].contains("return (Handing::Yield, false)"), "a refused (b) is not left as it is");
        let cooperative = body(FRONT, "fn cooperative(");
        for asked in ["majorVersion >= 14", "respondsToSelector(sel!(yieldActivationToApplication:))", "respondsToSelector(sel!(activateFromApplication:options:))"] {
            assert!(cooperative.contains(asked), "{asked}");
        }

        // The events that end a focus waiting, and nothing timed.
        let on_event = body(FRONT, "pub(super) fn activated(pid: i32) {");
        assert!(on_event.contains("on_activation(w.pid, pid, ours, pressed != w.pressed)"));
        assert!(body(FRONT, "fn own_edge(edge: OwnEdge) {").contains("on_own_edge(w.resigned, edge)"));
        let hook = body(FRONT, "fn hook_own() {");
        for name in ["NSApplicationDidResignActiveNotification", "NSApplicationDidBecomeActiveNotification"] {
            assert!(hook.contains(name), "{name}");
        }
        for timed in ["Duration", "sleep", "after("] {
            assert!(!FRONT.contains(timed), "{timed}");
        }
        // The tap counts a key pressed — not a repeat, nor one this process posted — before anything
        // can return it.
        let callback = body(TAP, "unsafe extern \"C-unwind\" fn tap_callback(");
        let counted = at(callback, "PRESSED.fetch_add(1, Ordering::Relaxed);");
        assert!(at(callback, "let repeat = ") < counted && counted < at(callback, "swallow_held_repeat("));
        assert!(callback[..counted].contains("EventSourceUnixProcessID"));

        // This application takes the front as the tool does, and gives up a focus still waiting.
        let take = body(APP, "pub(crate) fn activate_self() {");
        assert!(take.contains("activateIgnoringOtherApps(true)"));
        assert!(!take.contains("activateWithOptions"));
        assert!(at(take, "super::front::cancel(") < at(take, "activateIgnoringOtherApps(true)"));
    }

    /// A tool page described `activates = false` is a panel given the mask that says so when it is
    /// made, the one time it can be given; it takes the keyboard with no activation asked for —
    /// giving up what `activate_self` gives up — and is answered at the call, as nothing is on its
    /// way. So is the page's own window focused through `host.window.focus`. Held where it is
    /// written, as above; `page-check` counts the activations on CI.
    #[test]
    fn a_page_that_does_not_activate_this_application_asks_for_no_activation() {
        const PAGE: &str = include_str!("macos/page.rs");
        const OWN: &str = include_str!("macos/own.rs");
        const APP: &str = include_str!("macos/app.rs");

        let lent = body(APP, "pub(crate) fn keyboard_without_activating() {");
        assert!(lent.contains("super::front::cancel(") && lent.contains("super::page::superseded();"));
        for call in ["activateIgnoringOtherApps", "activateWithOptions", ".activate("] {
            assert!(!lent.contains(call), "{call}");
        }

        let made = body(PAGE, "fn make_window(");
        let panel = at(made, "if recipe.panel {");
        let mask = at(made, "style |= NSWindowStyleMask::NonactivatingPanel;");
        assert!(panel < at(made, "if !state.activates {") && at(made, "if !state.activates {") < mask);
        assert!(mask < at(made, "initWithContentRect"), "given when the panel is made");

        let from = at(PAGE, "fn take_front(");
        let take = &PAGE[from..from + at(&PAGE[from..], "pub(crate) fn hide(")];
        let choose = at(take, "if self.state.activates {");
        let activates = at(take, "super::app::activate_self();");
        let lends = at(take, "super::app::keyboard_without_activating();");
        assert!(choose < activates && activates < lends && lends < at(take, "w.makeKeyAndOrderFront(None);"));
        assert_eq!(take.matches("activate_self()").count(), 1, "only for a window that activates");
        assert!(take.contains("if was_active || !self.state.activates {"), "declined at once: nothing is on its way");

        let focus = body(OWN, "pub(super) fn focus(id: isize) -> Result<(), HostError> {");
        let decided = at(focus, "let lends = nonactivating(&w);");
        assert!(decided < at(focus, "super::app::keyboard_without_activating();"));
        assert!(decided < at(focus, "super::app::activate_self();"));
        // Answered at the call, as nothing is on its way: the window key, or not.
        let made = at(focus, "w.makeKeyAndOrderFront(None);");
        let read = at(focus, "let key = w.isKeyWindow();");
        let answered = at(focus, "if key {\n        Ok(())\n    } else {\n        Err(HostError::of(&crate::errors::common::DECLINED))");
        assert!(made < at(focus, "if !lends {") && at(focus, "if !lends {") < read && read < answered);

        // Whether this application is the active one, for a show's answer and for a request whose
        // page left, is the hand-over's: not AppKit's word while a panel of ours has the keyboard
        // only lent.
        let active = body(OWN, "pub(super) fn active(front: Option<i32>) -> bool {");
        assert!(active.contains("active_for_handing(appkit, keyboard_lent(), front == Some(std::process::id() as i32))"));
        assert!(take.contains("let was_active = super::own::active(super::ax::frontmost_pid());"));
        let came_back = body(PAGE, "pub(super) fn became_active() {");
        assert!(came_back.contains("let active = super::own::active(super::ax::frontmost_pid());"));
        for f in [take, came_back] {
            assert!(!f.contains("isActive()"), "never AppKit's word alone");
        }
    }
}

/// Passing on a key a module no longer holds when its busy mailbox delivers it: the decision and
/// the key events, as the Windows hook's and the Mac tap's backends send them.
#[cfg(test)]
mod pass_on_tests {
    use super::*;

    const W1: isize = 0x1001;
    const W2: isize = 0x2002;

    /// Pressed in `front`, with 7 key-downs let through before it.
    fn at(front: isize) -> Pressed {
        Pressed { front, seq: 7, phys: None, queued: None }
    }

    /// The keyboard with `front` in front, the key `key_held` or not, `mods` held, no screen
    /// reader's key, and the count where [`at`] left it: nothing typed since the press.
    fn now(front: isize, key_held: bool, mods: u8) -> KeyboardNow {
        KeyboardNow { front: Some(front), key_held, mods, reader_mod: false, let_through: 7 }
    }

    fn m(role: u8, down: bool) -> Stroke {
        Stroke::Modifier { role, down }
    }

    const DOWN: Stroke = Stroke::Key { down: true };
    const UP: Stroke = Stroke::Key { down: false };

    /// A bare key in the window it was pressed in, with nothing held: its key-down and its key-up,
    /// and nothing else.
    #[test]
    fn a_bare_key_is_its_down_and_its_up() {
        assert_eq!(pass_on_strokes(0, at(W1), now(W1, false, 0)), Ok(vec![DOWN, UP]));
    }

    /// The modifiers the press had and nobody holds now go down before the key, Ctrl, Alt, Shift,
    /// Win, and up after it in the reverse order; one the user holds is left alone, neither pressed
    /// nor released — a held modifier is never let go under the user's finger.
    #[test]
    fn the_press_s_modifiers_nobody_holds_go_down_around_it_and_held_ones_are_left_alone() {
        let all = MASK_SHIFT | MASK_CTRL | MASK_ALT | MASK_WIN;
        assert_eq!(
            pass_on_strokes(all, at(W1), now(W1, false, 0)),
            Ok(vec![
                m(MASK_CTRL, true),
                m(MASK_ALT, true),
                m(MASK_SHIFT, true),
                m(MASK_WIN, true),
                DOWN,
                UP,
                m(MASK_WIN, false),
                m(MASK_SHIFT, false),
                m(MASK_ALT, false),
                m(MASK_CTRL, false),
            ])
        );
        // Shift+Tab with Shift still held: the key alone.
        assert_eq!(pass_on_strokes(MASK_SHIFT, at(W1), now(W1, false, MASK_SHIFT)), Ok(vec![DOWN, UP]));
        // Ctrl+Alt+V with Alt held: Ctrl around the key, Alt untouched.
        assert_eq!(
            pass_on_strokes(MASK_CTRL | MASK_ALT, at(W1), now(W1, false, MASK_ALT)),
            Ok(vec![m(MASK_CTRL, true), DOWN, UP, m(MASK_CTRL, false)])
        );
    }

    /// Every sequence leaves nothing down: each modifier it presses it releases, after the key's
    /// own key-up; and the key goes down and up once, in that order.
    #[test]
    fn no_sequence_leaves_a_modifier_down() {
        for mask in 0..=ROLE_BITS {
            for held in 0..=ROLE_BITS {
                let Ok(s) = pass_on_strokes(mask, at(W1), now(W1, false, held)) else {
                    assert!(held & !mask != 0, "refused only for a modifier the press was made without");
                    continue;
                };
                assert!(left_down(&s, s.len()).is_empty(), "mask {mask} held {held}: {s:?}");
                let up = s.iter().position(|x| *x == UP).unwrap();
                assert_eq!(s.iter().position(|x| *x == DOWN), Some(up - 1), "{s:?}");
                assert_eq!(s.iter().filter(|x| matches!(x, Stroke::Key { .. })).count(), 2, "{s:?}");
                assert!(s[up + 1..].iter().all(|x| matches!(x, Stroke::Modifier { down: false, .. })));
                assert!(held & !mask == 0, "mask {mask} held {held}");
            }
        }
    }

    /// Not passed on: another window in front, none known at the press, nobody able to say which
    /// is in front now, the key itself held, a modifier held that the press was made without, a
    /// screen reader's key held, a key let through since the press — and a tap, which was never
    /// kept. Each check in its order: the first that fails is the reason.
    #[test]
    fn what_is_not_passed_on_and_why() {
        assert_eq!(pass_on_strokes(0, at(W1), now(W2, false, 0)), Err(NotPassed::OtherWindow));
        assert_eq!(pass_on_strokes(0, at(W1), now(0, false, 0)), Err(NotPassed::OtherWindow), "nothing in front now");
        assert_eq!(pass_on_strokes(0, at(0), now(0, false, 0)), Err(NotPassed::NoWindow), "unknown at the press");
        let unknown = KeyboardNow { front: None, ..now(W1, false, 0) };
        assert_eq!(pass_on_strokes(0, at(W1), unknown), Err(NotPassed::FrontUnknown), "nobody could say now");
        assert_eq!(pass_on_strokes(0, at(W1), now(W1, true, 0)), Err(NotPassed::KeyHeld));
        assert_eq!(pass_on_strokes(MASK_SHIFT, at(W1), now(W1, true, MASK_SHIFT)), Err(NotPassed::KeyHeld));
        assert_eq!(pass_on_strokes(0, at(W1), now(W1, false, MASK_CTRL)), Err(NotPassed::OtherModifiers(MASK_CTRL)));
        assert_eq!(
            pass_on_strokes(MASK_SHIFT, at(W1), now(W1, false, MASK_SHIFT | MASK_ALT | MASK_WIN)),
            Err(NotPassed::OtherModifiers(MASK_ALT | MASK_WIN))
        );
        let reader = KeyboardNow { reader_mod: true, ..now(W1, false, 0) };
        assert_eq!(pass_on_strokes(0, at(W1), reader), Err(NotPassed::ReaderModifier));
        // A Down typed after the press reached the program first: one more let through. Any
        // difference counts.
        for count in [8, 6, 0, u32::MAX] {
            let typed = KeyboardNow { let_through: count, ..now(W1, false, 0) };
            assert_eq!(pass_on_strokes(0, at(W1), typed), Err(NotPassed::Overtaken), "{count}");
        }
        // The count wraps round: one more past the top is still one more.
        let top = Pressed { seq: u32::MAX, ..at(W1) };
        let wrapped = KeyboardNow { let_through: u32::MAX.wrapping_add(1), ..now(W1, false, 0) };
        assert_eq!(pass_on_strokes(0, top, wrapped), Err(NotPassed::Overtaken), "past the wrap");
        let same = KeyboardNow { let_through: u32::MAX, ..now(W1, false, 0) };
        assert_eq!(pass_on_strokes(0, top, same), Ok(vec![DOWN, UP]), "at the top, nothing typed since");
        assert_eq!(pass_on_strokes(MASK_TAP, at(W1), now(W1, false, 0)), Err(NotPassed::Tap));
        assert_eq!(pass_on_strokes(MASK_TAP | MASK_ALT, at(W1), now(W1, false, 0)), Err(NotPassed::Tap));
        // The window first: a key pressed elsewhere is not passed on whatever is held.
        assert_eq!(pass_on_strokes(0, at(W1), now(W2, true, MASK_CTRL)), Err(NotPassed::OtherWindow));
        // Held before overtaken: a key still held, whose repeats reach the program once its
        // capture is gone, says it is held.
        let held = KeyboardNow { let_through: 9, ..now(W1, true, 0) };
        assert_eq!(pass_on_strokes(0, at(W1), held), Err(NotPassed::KeyHeld));
        // The physical key decides nothing here: the backend sends it.
        let numpad_enter = Pressed { phys: Some(0xE01C), ..at(W1) };
        assert_eq!(pass_on_strokes(0, numpad_enter, now(W1, false, 0)), Ok(vec![DOWN, UP]));
    }

    /// A sequence the system took only a part of: what that part left down, released key first,
    /// then the modifiers in the reverse order; nothing for none of it, or all of it.
    #[test]
    fn a_part_sent_has_what_it_left_down_released() {
        let s = pass_on_strokes(MASK_CTRL | MASK_SHIFT, at(W1), now(W1, false, 0)).unwrap();
        assert_eq!(s, vec![m(MASK_CTRL, true), m(MASK_SHIFT, true), DOWN, UP, m(MASK_SHIFT, false), m(MASK_CTRL, false)]);
        assert_eq!(left_down(&s, 0), vec![]);
        assert_eq!(left_down(&s, 1), vec![m(MASK_CTRL, false)]);
        assert_eq!(left_down(&s, 2), vec![m(MASK_SHIFT, false), m(MASK_CTRL, false)]);
        assert_eq!(left_down(&s, 3), vec![UP, m(MASK_SHIFT, false), m(MASK_CTRL, false)]);
        assert_eq!(left_down(&s, 4), vec![m(MASK_SHIFT, false), m(MASK_CTRL, false)]);
        assert_eq!(left_down(&s, 5), vec![m(MASK_CTRL, false)]);
        assert_eq!(left_down(&s, 6), vec![]);
        assert_eq!(left_down(&s, 99), vec![], "more than there are");
    }

    /// The Mac's half, held where it is written, since no test here runs the tap: the tap lets an
    /// event carrying the mark through before it records, matches or counts anything — a
    /// modifier's change, a key-up's swallowed key-down, a capture — ending only a pending
    /// modifier tap, for a key-down; every other key-down it returns is counted, and a key it
    /// takes carries the count and its keycode. Every event `input::pass_on` posts carries the
    /// mark, from the pass-on's own source, built before the first is posted, on the keycode it
    /// was pressed on; `keyboard_now` asks the window in front afresh, and the tap's record beside
    /// the HID system, for that keycode.
    #[test]
    fn the_mac_tap_lets_the_keys_passed_on_through_first() {
        const TAP: &str = include_str!("macos/tap.rs");
        const INPUT: &str = include_str!("macos/input.rs");
        const QUEUE: &str = include_str!("macos/queue.rs");
        let body = |src: &'static str, sig: &str| {
            let at = src.find(sig).unwrap_or_else(|| panic!("{sig}"));
            let end = src[at..].find("\n}\n").unwrap();
            &src[at..at + end]
        };
        let cb = body(TAP, "unsafe extern \"C-unwind\" fn tap_callback(");
        let marked = concat!(
            "if CGEvent::integer_value_field(Some(ev), CGEventField::EventSourceUserData) == keys::PASS_ON_MARK {\n",
            "        if etype == CGEventType::KeyDown {\n",
            "            TAP_ARMED.store(0, Ordering::Relaxed);\n",
            "        }\n",
            "        return pass;\n",
            "    }\n",
        );
        let mark = cb.find(marked).expect("the tap asks for the mark, and ends only a pending tap for it");
        for later in ["modifier_changed(keycode, flags)", "take_suppressed(keycode)", "note_lateness(", "decide(vk, mask, front, true)", "LET_THROUGH.fetch_add("] {
            assert!(mark < cb.find(later).unwrap_or_else(|| panic!("{later}")), "{later} comes before the mark is asked");
        }
        // Every key-down returned past the mark is counted; none is returned bare after it.
        let rest = &cb[mark + marked.len()..];
        let key_down = rest.find("if etype != CGEventType::KeyDown {").unwrap();
        assert_eq!(rest[key_down..].matches("return pass;").count(), 1, "only the check itself, for what is no key-down");
        assert!(rest[key_down..].starts_with("if etype != CGEventType::KeyDown {\n        return pass;\n    }"));
        assert_eq!(rest[key_down..].matches("return through();").count(), 4);
        assert!(rest.contains("        return through(); // not a keyboard event we can name"));
        assert!(rest.contains("let pressed = Pressed { front, seq: LET_THROUGH.load(Ordering::Relaxed), phys: Some(keycode), queued: Some(Instant::now()) };"));
        let pass = body(INPUT, "pub fn pass_on(");
        assert!(pass.contains("let code = phys\n        .or_else(|| keys::vk_to_keycode_for(vk, mask))"));
        assert!(pass.contains("let src = pass_source()"));
        assert_eq!(pass.matches("source()").count(), pass.matches("pass_source()").count(), "never the shared source");
        assert!(body(INPUT, "fn pass_source(").contains("CGEventSource::new(CGEventSourceStateID::Private)"));
        let set = pass.find("CGEventField::EventSourceUserData, keys::PASS_ON_MARK").expect("every event marked");
        let posted = pass.find("for event in &built {").expect("posted after all are built");
        assert!(set < posted);
        assert!(pass[posted..].contains("post(event);"));
        let now = body(INPUT, "pub fn keyboard_now(");
        assert!(now.contains("let key_held = phys\n        .or_else(|| keys::vk_to_keycode_for(vk, mask))"));
        assert!(now.contains("super::tap::held_back(code)"));
        assert!(now.contains("front: super::ax::foreground_window_id(),"), "asked afresh, not the tap's idea");
        assert!(now.contains("let_through: super::tap::let_through(),"));
        // A hotkey press carries the count the tap had when Carbon handed it over, and when.
        assert!(body(QUEUE, "pub fn push_hotkey(").contains("push((id, super::tap::let_through(), Instant::now()))"));
        // Handed on with its own key-down in that count, which the quick repeats take for no other
        // key; and after the moment the tick measures a `"final"` sequence's end by.
        let drain = body(QUEUE, "pub fn drain(");
        let taken = drain.find("TAKEN.with(|t| t.set(Some(Instant::now())));").expect("the moment noted");
        let handed = drain.find("events.on_hotkey(id, crate::backend::HotkeyPress { seq, own: 1, at });").expect("own: 1");
        assert!(taken < drain.find("in HOTKEYS.with(|q| std::mem::take(").unwrap() && handed < drain.find("events.on_key(").unwrap());
    }
}

/// The permissions as the Mac judges them by the evidence of the run (`macos/evidence.rs`, the
/// rules; `backend/grants.rs`, the words), held where they are wired: only a Mac runs these files,
/// and `gui.rs`'s half reaches the macOS CI job alone. The rules are executed above, in
/// `macos_evidence`; these say every caller asks them.
#[cfg(test)]
mod permission_evidence_wiring_tests {
    const PERM: &str = include_str!("macos/perm.rs");
    const CAPTURE: &str = include_str!("macos/capture.rs");
    const OCR: &str = include_str!("macos/ocr.rs");
    const TAP: &str = include_str!("macos/tap.rs");
    const HOTKEY: &str = include_str!("macos/hotkey.rs");
    const OWN: &str = include_str!("macos/own.rs");
    const GUI: &str = include_str!("../gui.rs");

    /// The body of the function `sig` in `src`, to its closing brace at the start of a line.
    fn body<'a>(src: &'a str, sig: &str) -> &'a str {
        let at = src.find(sig).unwrap_or_else(|| panic!("{sig}"));
        let end = src[at..].find("\n}\n").unwrap();
        &src[at..at + end]
    }

    fn at(f: &str, what: &str) -> usize {
        f.find(what).unwrap_or_else(|| panic!("no `{what}`"))
    }

    #[test]
    fn screen_recording_is_judged_everywhere_and_preflighted_in_one_place() {
        // The system's check is asked in the verdict and nowhere else: not by the page, the
        // start's block, the requests, the captures or the text recognition.
        assert_eq!(PERM.matches("CGPreflightScreenCaptureAccess()").count(), 1);
        let judged = body(PERM, "pub(super) fn screen_recording() -> ScreenVerdict {");
        assert!(judged.contains("CGPreflightScreenCaptureAccess(),"));
        assert!(judged.contains("super::capture::seen(),") && judged.contains("other_window_names,"));
        for (src, name) in [(CAPTURE, "capture.rs"), (OCR, "ocr.rs")] {
            assert!(!src.contains("CGPreflightScreenCaptureAccess()"), "{name}");
            assert!(src.contains("super::perm::screen_recording()"), "{name}");
        }
        for sig in ["pub fn permissions() -> Vec<Permission> {", "pub fn request_screen_recording_once() {", "fn ask_for_screen_recording(trigger: Trigger) -> Asked {", "pub fn environment_report() -> Vec<(String, String)> {"] {
            assert!(body(PERM, sig).contains("screen_recording()"), "{sig}");
        }
        // The window list is looked at for whether a name is there, never for what it says.
        let names = body(PERM, "fn other_window_names() -> Names {");
        assert!(names.contains("dict_has_string(&dict, name)"));
        assert!(!names.contains("to_string"));
    }

    #[test]
    fn screencapturekit_s_answers_are_kept_and_the_first_capture_is_judged_after_it() {
        let grab = body(CAPTURE, "fn grab(x: i32, y: i32, w: i32, h: i32, best: bool) -> Option<Grabbed> {");
        let asked = at(grab, "let outcome = sck_capture(rect);");
        assert!(asked < at(grab, "SckOutcome::Image(_) => note_seen(Seen::Picture),"));
        assert!(asked < at(grab, "SckOutcome::Error { declined: true, .. } => note_refused(),"));
        let probe = body(CAPTURE, "fn sck_probe(");
        assert!(probe.contains("note_seen(Seen::Picture)") && probe.contains("note_refused()"));
        // A refusal keeps what the window names said as it came, so that names read later
        // overrule only one that came while they were hidden.
        assert!(body(CAPTURE, "fn note_refused() {")
            .contains("super::evidence::after_refusal(seen(), super::perm::other_window_names)"));
        assert!(!CAPTURE.contains("note_seen(Seen::Declined"));
        assert!(body(PERM, "fn capture_requests(").contains("super::capture::note_shareable_content(&sck);"));
        for sig in ["fn capture_rgba(", "pub fn frame("] {
            let f = body(CAPTURE, sig);
            assert!(at(f, "grab(vx, vy, vw, vh,") < at(f, "first_capture_judged();"), "{sig}");
        }
    }

    #[test]
    fn input_monitoring_is_needed_only_by_what_the_keys_showed() {
        let perms = body(PERM, "pub fn permissions() -> Vec<Permission> {");
        assert!(perms.contains("blocking: super::evidence::listen_needed(listen == Listen::Granted, keys),"));
        // A shortcut is the witness: Carbon's handler tells the tap, after queueing the press.
        let handler = body(HOTKEY, "unsafe extern \"C\" fn hotkey_handler(");
        assert!(at(handler, "queue::push_hotkey(") < at(handler, "super::tap::note_shortcut();"));
        let note = body(TAP, "pub(super) fn note_shortcut() {");
        assert!(at(note, "ON_SINCE_MS.load(") < at(note, "secure_input()") && at(note, "secure_input()") < at(note, "DEAF.store(true"));
        // A tap the system switched off in a stall missed the key without being deaf.
        assert!(at(note, "CGEvent::tap_is_enabled(") < at(note, "DEAF.store(true"));
        // Every switching on starts the second again: the install, both re-enables, a re-creation.
        assert_eq!(TAP.matches("note_on();").count(), 4);
        let callback = body(TAP, "unsafe extern \"C-unwind\" fn tap_callback(");
        assert!(at(callback, "CGEvent::tap_enable(unsafe { &*port }, true);") < at(callback, "note_on();"));
        // The tap counts the keyboard reaching it with the presses, not with its own posts.
        assert!(at(callback, "PRESSED.fetch_add(1, Ordering::Relaxed);") < at(callback, "CALLED.store(true, Ordering::Relaxed);"));
        assert!(body(TAP, "fn create_and_attach(").contains("REFUSED_TRUSTED.store(trusted, Ordering::Relaxed);"));
    }

    #[test]
    fn the_page_reads_the_permissions_again_when_it_comes_forward() {
        let start = body(OWN, "pub(crate) fn start() {");
        assert!(start.contains("NSApplicationDidBecomeActiveNotification"));
        assert!(start.contains("BECAME_ACTIVE.store(true, Ordering::Relaxed)"));
        assert!(body(OWN, "pub(crate) fn take_became_active() -> bool {").contains("BECAME_ACTIVE.swap(false"));
        // Its tab selected, its window activated, this application made active — each a look
        // that says what changed; Re-check now says the count as well.
        for trigger in [
            "notebook.on_page_changed(move |event| {",
            "frame.on_activate(move |event| {",
            "if crate::backend::take_became_active() {",
        ] {
            let from = at(GUI, trigger);
            assert!(GUI[from..from + 400].contains("look(Look::Forward);"), "{trigger}");
        }
        assert!(GUI.contains("recheck.on_click(move |_| look(Look::Recheck));"));
        // Relabelled in place, and only where the text changed.
        assert!(GUI.contains("relabel(&head, &crate::backend::grants::head(&all));"));
        assert!(GUI.contains("relabel(line, &crate::backend::grants::line(p));"));
        // The start counts and names through the same words, and says a translocated copy.
        let start = at(GUI, "let start = crate::backend::grants::start(");
        assert!(GUI[start..start + 300].contains("moved.spoken().as_deref(),"));
        assert!(!GUI.contains("p.blocking && p.state == crate::backend::Grant::Missing"), "the start's own count is gone");
    }
}
