//! Clicks into another program's window that leave the window in front where it is (Windows):
//! `host.input.click`'s `via = "post"` and `via = "noactivate"`, and the check its `window` asks
//! for. Why it exists: TODO.md, "W3: acting in the plug-in without the round trip"; what it is
//! measured with: the HTML probe's W10.
//!
//! **Why an ordinary click is not enough.** A click into a window that is not active is itself an
//! activation: Windows sends the window `WM_MOUSEACTIVATE`, and passed on to `DefWindowProc` that
//! activates its top-level window before the button-down is delivered. A page in front loses the
//! keyboard to the plug-in window it clicks into, and a screen reader says so — the focus leaving,
//! and coming back when the page is brought back. Two ways around it, each the module's choice
//! for each call:
//!
//! - **Posted** ([`post_click`]): the move, the press and the release posted to the window a real
//!   click at that point reaches, in that window's own client coordinates. Posting skips Windows'
//!   input path — no hit test asked of the program, no `WM_MOUSEACTIVATE`, no activation — and
//!   never waits for the other program. What the program makes of it is its own business: the
//!   common toolkits take the position from the message and the buttons from its `wParam`; one that
//!   reads the mouse through raw input never sees it, and nothing answers whether one arrived.
//! - **The no-activate style** ([`prepare_hold`] and the job it hands back): the window's top-level
//!   window is given `WS_EX_NOACTIVATE`, which Windows reads as "a click does not activate this",
//!   and then clicked for real. Changing another program's window sends its thread
//!   `WM_STYLECHANGING` and waits for its answer, so it is done on a thread of the host's, never
//!   on the event loop. And it cannot be undone right after each click: the undoing is a sent
//!   message too, which the program takes before the queued click, and the click would activate
//!   after all. So the style is held until an event ends it — another window coming to the front,
//!   the module stopping, the application ending — and every window given it is written down
//!   beside the application first ([`LEDGER`]), so a run that ended without giving it back (a
//!   crash, a kill) gives it back at the next start. Everything that gives, takes back or writes
//!   down the style runs on that one thread — the end of the application included, which hands it
//!   a last job and waits for it a bounded time ([`exit_job`]) — so the list and the windows are
//!   never changed by two threads at once.
//!
//! Only an owned top-level window gets the style — a plug-in's floating window, which its DAW's
//! main window owns — never a window nothing owns or one marked for the taskbar: a program's main
//! window, a standalone instrument's, could lose its taskbar button and its place in Alt+Tab while
//! it held it.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, SetLastError, FILETIME, HWND, POINT, RECT,
};
use windows_sys::Win32::Graphics::Gdi::{MonitorFromWindow, ScreenToClient, MONITOR_DEFAULTTONEAREST};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, GetAwarenessFromDpiAwarenessContext, GetDpiForMonitor,
    GetDpiForWindow, GetWindowDpiAwarenessContext, SetThreadDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_SHIFT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetClassNameW, GetClientRect, GetForegroundWindow, GetWindow, GetWindowLongPtrW,
    GetWindowRect, GetWindowThreadProcessId, IsChild, IsWindow, PostMessageW, SendMessageTimeoutW,
    SetCursorPos, SetWindowLongPtrW, WindowFromPoint, GA_ROOT, GWL_EXSTYLE, GW_OWNER,
    SMTO_ABORTIFHUNG, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
    WM_NULL, WM_RBUTTONDOWN, WM_RBUTTONUP, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
};

use super::{ActJob, MouseButton, COVERED};
use crate::logging;

/// The window given as `window` is not there any more.
pub(super) const GONE: &str = "the window is gone";
/// The point lies outside the window given as `window`.
pub(super) const OUTSIDE: &str = "the point is not in the window";
/// A posted click is a click in a window's client area: on its frame or title bar a real click
/// is another message, which nothing here posts.
pub(super) const FRAME: &str = "the point is on the window's frame, not in its client area";
/// The program runs elevated and this application does not: Windows refuses a message, a style
/// change and an injected click from a process of lower rights (a click without saying so).
pub(super) const HIGHER: &str = "the window's program runs with higher rights";
/// The window's top-level window is one nothing owns, or one marked for the taskbar.
pub(super) const MAIN_WINDOW: &str = "the window belongs to a program's main window, which never gets the no-activate \
     style: only a window another window owns — a plug-in's floating window — does";
/// The program set the style back, or never took it.
pub(super) const NOT_KEPT: &str = "the window did not keep the no-activate style";
/// A click with the style that reached the thread after the application began to end.
pub(super) const ENDING: &str = "the application is ending";

/// How long a program may take to answer the question asked before its window's style is
/// changed — the same bound the accessibility calls have — and at the end, when the application
/// is closing anyway, half of it.
const ANSWER_MS: u32 = 1000;
const ANSWER_AT_EXIT_MS: u32 = 500;

/// The file beside the application that names every window holding the style given here, written
/// before the style is given and rewritten when it is taken back; deleted when none holds it.
const LEDGER: &str = "held-window-styles.txt";
const LEDGER_HEADER: &str = "# Windows of other programs that this application gave the no-activate style \
     (WS_EX_NOACTIVATE) for host.input.click's via = \"noactivate\". It gives it back at its next start. \
     Leave this file alone while the application runs.";

// The `MK_*` bits of a mouse message's wParam (WinUser.h); windows-sys keeps them in a feature
// of their own that nothing else here needs.
const MK_LBUTTON: usize = 0x0001;
const MK_RBUTTON: usize = 0x0002;
const MK_SHIFT: usize = 0x0004;
const MK_CONTROL: usize = 0x0008;
const MK_MBUTTON: usize = 0x0010;

// ── The window a click is for ─────────────────────────────────────────────────────────────────

/// The window a real click at (x, y) reaches, when that is `window` or a window inside it: the
/// check `host.input.click`'s `window` asks for, made before any way of clicking. Local calls
/// only: nothing is asked of the window's program.
///
/// `WindowFromPoint` does not ask another program's windows whether they let a click through
/// (`WM_NCHITTEST`), so a transparent window a toolkit lays over its content — JUCE's over an
/// OpenGL editor — is taken for the window drawn there. The posted click's log line names the
/// window it went into, which is where such a case shows.
pub(super) fn target(window: isize, x: i32, y: i32) -> Result<HWND, String> {
    let w = window as HWND;
    // SAFETY: every handle is checked by the calls it is passed to; a stale one fails them.
    unsafe {
        if w.is_null() || IsWindow(w) == 0 {
            return Err(GONE.into());
        }
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(w, &mut r) == 0 {
            return Err(GONE.into());
        }
        if !(x >= r.left && x < r.right && y >= r.top && y < r.bottom) {
            return Err(OUTSIDE.into());
        }
        let at = WindowFromPoint(POINT { x, y });
        if at.is_null() || (at != w && IsChild(w, at) == 0) {
            return Err(COVERED.into());
        }
        Ok(at)
    }
}

/// A window's class name, for the log.
fn class_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: the buffer and its length go together; a stale handle answers 0.
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

// ── The posted click ──────────────────────────────────────────────────────────────────────────

/// A point in physical client pixels, in the coordinates of a window whose own DPI is
/// `window_dpi` on a monitor of `monitor_dpi`: what a posted message's `lParam` has to carry.
///
/// Windows does not convert a posted message for the window it is posted to: the window reads
/// the point in its own coordinates, whatever its DPI awareness. A per-monitor aware window's DPI
/// is its monitor's, and the point stays as it is; a window that is not aware draws at 96 and is
/// stretched to the monitor, so its coordinates are the physical ones times 96 over the monitor's;
/// a system-aware one draws at the system's DPI. `GetDpiForWindow` answers exactly that DPI for
/// each kind. Rounded to the nearest pixel; a DPI of 0 — not answered — leaves the point as it is.
pub(super) fn scaled_client(physical: (i32, i32), window_dpi: u32, monitor_dpi: u32) -> (i32, i32) {
    if window_dpi == 0 || monitor_dpi == 0 || window_dpi == monitor_dpi {
        return physical;
    }
    let f = f64::from(window_dpi) / f64::from(monitor_dpi);
    ((f64::from(physical.0) * f).round() as i32, (f64::from(physical.1) * f).round() as i32)
}

/// A window's DPI awareness in words, for the log: `awareness` as
/// `GetAwarenessFromDpiAwarenessContext` answers it, `v2` whether its context is per-monitor v2.
pub(super) fn awareness_words(awareness: i32, v2: bool) -> &'static str {
    match awareness {
        2 if v2 => "per-monitor v2",
        2 => "per-monitor",
        1 => "system",
        0 => "unaware",
        _ => "of an awareness Windows did not say",
    }
}

/// A mouse message's `lParam`: the client point, x in the low word and y in the high word, each
/// as the 16 bits Windows reads back with its sign — `MAKELPARAM`, zero above them.
pub(super) fn mouse_lparam(x: i32, y: i32) -> isize {
    (((y as u16 as u32) << 16) | (x as u16 as u32)) as isize
}

/// A mouse message's `wParam`: the button held (`MK_LBUTTON` and its kin) and Ctrl and Shift as
/// the keyboard has them — what a real click's message says.
pub(super) fn mouse_wparam(held: Option<MouseButton>, ctrl: bool, shift: bool) -> usize {
    let button = match held {
        Some(MouseButton::Left) => MK_LBUTTON,
        Some(MouseButton::Right) => MK_RBUTTON,
        Some(MouseButton::Middle) => MK_MBUTTON,
        None => 0,
    };
    button | if ctrl { MK_CONTROL } else { 0 } | if shift { MK_SHIFT } else { 0 }
}

/// A button's down and up messages.
pub(super) fn button_messages(button: MouseButton) -> (u32, u32) {
    match button {
        MouseButton::Left => (WM_LBUTTONDOWN, WM_LBUTTONUP),
        MouseButton::Right => (WM_RBUTTONDOWN, WM_RBUTTONUP),
        MouseButton::Middle => (WM_MBUTTONDOWN, WM_MBUTTONUP),
    }
}

/// What a failed post, style change or answer says, by its error code.
pub(super) fn refused(what: &str, error: u32) -> String {
    match error {
        5 => HIGHER.to_string(),
        _ => format!("the window did not take {what} (error {error})"),
    }
}

/// Whether a point in a window's own client coordinates lies in a client rectangle read in the
/// window's own coordinates too (its left and top are 0).
pub(super) fn in_client(p: (i32, i32), client: (i32, i32)) -> bool {
    p.0 >= 0 && p.1 >= 0 && p.0 < client.0 && p.1 < client.1
}

/// Where (x, y) is in `at`'s own client coordinates, its DPI mode and its monitor's scale — or
/// why it is not in its client area. Both sides of the comparison in the window's own
/// coordinates: the point mapped on a thread switched to per-monitor v2, so it is in physical
/// pixels whatever the thread was, and then scaled into the window's DPI ([`scaled_client`]);
/// the client rectangle read on the thread switched to the window's own context, where no
/// scaling of Windows' can come between — whether `GetClientRect` scales another context's read
/// is nothing to build on.
fn client_place(at: HWND, x: i32, y: i32) -> Result<((i32, i32), &'static str, u32), String> {
    // SAFETY: local calls on a handle `target` checked; the thread's context is put back.
    unsafe {
        let ctx = GetWindowDpiAwarenessContext(at);
        let old = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let mut p = POINT { x, y };
        let mapped = ScreenToClient(at, &mut p) != 0;
        if !ctx.is_null() {
            SetThreadDpiAwarenessContext(ctx);
        }
        let mut c: RECT = std::mem::zeroed();
        let measured = GetClientRect(at, &mut c) != 0;
        if !old.is_null() {
            SetThreadDpiAwarenessContext(old);
        }
        if !mapped || !measured {
            return Err(GONE.into());
        }
        let words = awareness_words(
            GetAwarenessFromDpiAwarenessContext(ctx),
            AreDpiAwarenessContextsEqual(ctx, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0,
        );
        let (mut dx, mut dy) = (0u32, 0u32);
        let monitor = MonitorFromWindow(at, MONITOR_DEFAULTTONEAREST);
        if GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) != 0 {
            dx = 0;
        }
        let own = scaled_client((p.x, p.y), GetDpiForWindow(at), dx);
        if !in_client(own, (c.right, c.bottom)) {
            return Err(FRAME.into());
        }
        Ok((own, words, dx))
    }
}

/// `host.input.click`'s `via = "post"`: the click posted to the window a real click at (x, y)
/// reaches, when that is `window` or one inside it, the pointer moved there first unless
/// `move_pointer` is false. On the event loop: every call here is local or a post, and a post
/// never waits for the other program. `Ok` says the three messages were posted — not that the
/// program did anything with them; one line in the log either way.
pub(super) fn post_click(window: isize, x: i32, y: i32, button: MouseButton, move_pointer: bool) -> Result<(), String> {
    let at = target(window, x, y)?;
    let ((cx, cy), mode, monitor_dpi) = client_place(at, x, y)?;
    // SAFETY: local calls; the posts go to a window `target` checked, and fail if it went since.
    unsafe {
        if move_pointer {
            SetCursorPos(x, y);
        }
        let ctrl = GetAsyncKeyState(i32::from(VK_CONTROL)) < 0;
        let shift = GetAsyncKeyState(i32::from(VK_SHIFT)) < 0;
        let lp = mouse_lparam(cx, cy);
        let (down, up) = button_messages(button);
        for (msg, wp) in [
            (WM_MOUSEMOVE, mouse_wparam(None, ctrl, shift)),
            (down, mouse_wparam(Some(button), ctrl, shift)),
            (up, mouse_wparam(None, ctrl, shift)),
        ] {
            if PostMessageW(at, msg, wp, lp) == 0 {
                let why = refused("the message", GetLastError());
                logging::line("input", &format!("posted click for window {window}: refused at 0x{msg:04x}: {why}"));
                return Err(why);
            }
        }
    }
    logging::line(
        "input",
        &format!(
            "posted click for window {window} ('{}'): into {} ('{}') at client {cx},{cy}; that window is {mode}, \
             monitor {} %; pointer moved: {}",
            class_of(window as HWND),
            at as isize,
            class_of(at),
            monitor_dpi * 100 / 96,
            if move_pointer { "yes" } else { "no" }
        ),
    );
    Ok(())
}

// ── The no-activate style ─────────────────────────────────────────────────────────────────────

/// Which window, exactly: its handle and what makes a reused handle tell itself apart — the
/// process, when that process was started, and the class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub hwnd: isize,
    pub pid: u32,
    /// The process's creation time, in 100-nanosecond units since 1601 (`GetProcessTimes`).
    pub created: u64,
    pub class: String,
}

impl Identity {
    /// The window as it is now, or `None` when it is gone.
    fn of(hwnd: HWND) -> Option<Identity> {
        // SAFETY: local calls; the process handle is closed before returning.
        unsafe {
            if hwnd.is_null() || IsWindow(hwnd) == 0 {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            let mut created = 0u64;
            if !process.is_null() {
                let (mut c, mut e, mut k, mut u): (FILETIME, FILETIME, FILETIME, FILETIME) =
                    (std::mem::zeroed(), std::mem::zeroed(), std::mem::zeroed(), std::mem::zeroed());
                if GetProcessTimes(process, &mut c, &mut e, &mut k, &mut u) != 0 {
                    created = (u64::from(c.dwHighDateTime) << 32) | u64::from(c.dwLowDateTime);
                }
                CloseHandle(process);
            }
            Some(Identity { hwnd: hwnd as isize, pid, created, class: class_of(hwnd) })
        }
    }

    /// Whether the window is still the one this names: not gone, and its handle not handed to
    /// another window since.
    fn still_there(&self) -> bool {
        Identity::of(self.hwnd as HWND).as_ref() == Some(self)
    }
}

/// A window this application gave the no-activate style.
#[derive(Clone, Debug)]
struct Held {
    who: Identity,
    /// The modules whose clicks it is held for; empty for one left over by an earlier run.
    owners: Vec<usize>,
    /// The window that was in front at its last click: the style is given back when another
    /// comes to the front. 0 for one left over by an earlier run, which any window ends.
    front: isize,
}

/// The windows holding the style given here. Changed on the thread alone; locked only to read or
/// change the list — never across a call that asks another program anything.
static HELD: Mutex<Vec<Held>> = Mutex::new(Vec::new());
/// Held windows plus clicks handed to the thread and not made yet: whether an event has anything
/// to give back, asked on the event loop without a lock.
static INTEREST: AtomicUsize = AtomicUsize::new(0);
/// Windows written down and about to be given the style: in the ledger from before they are given
/// it until they hold it, or failed to get it — the line a crash in between leaves for the next
/// start.
static ADDING: Mutex<Vec<Identity>> = Mutex::new(Vec::new());
/// The ledger is written by one writer at a time.
static LEDGER_LOCK: Mutex<()> = Mutex::new(());
/// Set when the application begins to end, before its last give-back is handed to the thread:
/// from then on no window is given the style ([`hold`] refuses), so no click still on its way to
/// the thread gives one after that give-back.
static STOPPING: AtomicBool = AtomicBool::new(false);

/// How long a program may take to answer before its window's style is changed now: half as long
/// once the application is ending.
fn answer_ms() -> u32 {
    if STOPPING.load(Ordering::SeqCst) {
        ANSWER_AT_EXIT_MS
    } else {
        ANSWER_MS
    }
}

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// A click handed to the thread and not made yet, counted in [`INTEREST`] until it is dropped —
/// made, refused, or never run.
struct Pending;

impl Pending {
    fn new() -> Pending {
        INTEREST.fetch_add(1, Ordering::SeqCst);
        Pending
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        INTEREST.fetch_sub(1, Ordering::SeqCst);
    }
}

/// One ledger line: the handle, the process, its creation time and the class, tab-separated —
/// the class last, as the one part that could hold anything.
pub(super) fn ledger_line(who: &Identity) -> String {
    format!("{}\t{}\t{}\t{}", who.hwnd, who.pid, who.created, who.class)
}

/// The windows a ledger names; comments, blank lines and lines that do not parse are left out.
pub(super) fn parse_ledger(text: &str) -> Vec<Identity> {
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let mut parts = l.splitn(4, '\t');
            let hwnd = parts.next()?.trim().parse().ok()?;
            let pid = parts.next()?.trim().parse().ok()?;
            let created = parts.next()?.trim().parse().ok()?;
            let class = parts.next()?.to_string();
            Some(Identity { hwnd, pid, created, class })
        })
        .collect()
}

fn ledger_path() -> std::path::PathBuf {
    crate::portable::base_dir().join(LEDGER)
}

/// Writes the ledger: every window held, and every one about to be ([`ADDING`]) — or deletes it
/// when that leaves none. Written to a file of its own first and moved over the old one, so a
/// crash in the middle leaves the old ledger or the new one, never half of either.
fn write_ledger() -> Result<(), String> {
    let _one = locked(&LEDGER_LOCK);
    let mut whom: Vec<Identity> = locked(&HELD).iter().map(|h| h.who.clone()).collect();
    for a in locked(&ADDING).iter() {
        if !whom.contains(a) {
            whom.push(a.clone());
        }
    }
    let path = ledger_path();
    if whom.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", path.display())),
            _ => Ok(()),
        };
    }
    let mut text = String::from(LEDGER_HEADER);
    text.push('\n');
    for w in &whom {
        text.push_str(&ledger_line(w));
        text.push('\n');
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Whether the window's program answers within `ms` — asked before its window's style is
/// changed, which waits for it without a bound. A probe, not a decision about any state: a
/// program that does not answer keeps its window as it is. One that answers and then hangs as
/// its style is changed still holds the thread until it answers that.
fn answers(hwnd: HWND, ms: u32) -> Result<(), String> {
    // SAFETY: a stale handle fails the call.
    unsafe {
        let mut result = 0usize;
        if SendMessageTimeoutW(hwnd, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, ms, &mut result) != 0 {
            return Ok(());
        }
        match GetLastError() {
            5 => Err(HIGHER.to_string()),
            1460 | 0 => Err(format!("the window's program did not answer within {ms} ms")),
            e => Err(format!("the window's program did not answer (error {e})")),
        }
    }
}

/// The window's extended style, read back.
fn ex_style(hwnd: HWND) -> u32 {
    // SAFETY: a stale handle answers 0.
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 }
}

/// Sets the extended style to `style`; `Err` when Windows answered that it did not. What the
/// window holds afterwards its callers read back either way: a change Windows made can still be
/// answered with an error, and a style that is on is held — written down and given back —
/// whatever the answer was.
fn set_ex_style(hwnd: HWND, style: u32) -> Result<(), String> {
    // SAFETY: a cross-process style change, made on a thread of the host's after `answers`.
    unsafe {
        SetLastError(0);
        if SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize) == 0 {
            let e = GetLastError();
            if e != 0 {
                return Err(refused("the style", e));
            }
        }
    }
    Ok(())
}

/// `host.input.click`'s `via = "noactivate"`, its part on the event loop: the checks every click
/// for a window makes, and that the window's top-level window is one another window owns; then
/// the job — the style, held, and the click — for a thread of the host's. Local calls only. The
/// job writes what refused it on the thread to the log itself, so a click handed over without a
/// callback is accounted for too.
pub(super) fn prepare_hold(window: isize, x: i32, y: i32, button: MouseButton, owner: usize) -> Result<ActJob, String> {
    if STOPPING.load(Ordering::SeqCst) {
        return Err(ENDING.into());
    }
    target(window, x, y)?;
    // SAFETY: local calls on a handle `target` checked.
    let (root, front) = unsafe { (GetAncestor(window as HWND, GA_ROOT), GetForegroundWindow() as isize) };
    if root.is_null() {
        return Err(GONE.into());
    }
    // SAFETY: as above.
    let owned = unsafe { !GetWindow(root, GW_OWNER).is_null() };
    if !owned || ex_style(root) & WS_EX_APPWINDOW != 0 {
        return Err(MAIN_WINDOW.into());
    }
    let who = Identity::of(root).ok_or_else(|| GONE.to_string())?;
    let pending = Pending::new();
    Ok(Box::new(move || {
        let _pending = pending;
        let made = hold_and_click(&who, front, owner, x, y, button);
        if let Err(why) = &made {
            logging::line("input", &refusal_line(window, x, y, why));
        }
        made
    }))
}

/// The log line of a click with the style refused on the thread — the words the event loop's
/// refusals use (lib.rs).
pub(super) fn refusal_line(window: isize, x: i32, y: i32, why: &str) -> String {
    format!("click at {x},{y} for window {window} (via = \"noactivate\") refused: {why}")
}

/// On the thread: the style given — or found already given, by this application or by the
/// program itself — and the click made, as a real one: the pointer to the point, the button
/// pressed and released.
fn hold_and_click(who: &Identity, front: isize, owner: usize, x: i32, y: i32, button: MouseButton) -> Result<(), String> {
    if !who.still_there() {
        return Err(GONE.into());
    }
    hold(who, front, owner)?;
    let (down, up) = super::windows::button_flags(button);
    // SAFETY: the system's input, as every click of the host's.
    unsafe {
        SetCursorPos(x, y);
        super::windows::send_mouse_event(down, 0);
        super::windows::send_mouse_event(up, 0);
    }
    logging::line("input", &format!("clicked window {} ('{}') at {x},{y} without activating it", who.hwnd, who.class));
    Ok(())
}

/// The style on `who`, for module `owner`'s clicks made while `front` is in front — none once the
/// application is ending, whose last give-back may already be on its way.
fn hold(who: &Identity, front: isize, owner: usize) -> Result<(), String> {
    if STOPPING.load(Ordering::SeqCst) {
        return Err(ENDING.into());
    }
    let hwnd = who.hwnd as HWND;
    let known = {
        let mut held = locked(&HELD);
        match held.iter_mut().find(|h| h.who == *who) {
            Some(h) => {
                if !h.owners.contains(&owner) {
                    h.owners.push(owner);
                }
                h.front = front;
                true
            }
            None => false,
        }
    };
    let style = ex_style(hwnd);
    if style & WS_EX_NOACTIVATE != 0 {
        if !known {
            logging::line(
                "input",
                &format!("window {} ('{}') has the no-activate style of its own: clicked as it is", who.hwnd, who.class),
            );
        }
        return Ok(());
    }
    answers(hwnd, answer_ms())?;
    // Written down before it is given: a crash in between leaves a line the next start checks
    // against the window and drops, never a style nobody gives back.
    if !known {
        locked(&ADDING).push(who.clone());
        if let Err(e) = write_ledger() {
            forget_unheld(who);
            return Err(format!("the list of windows to give their style back could not be written ({e})"));
        }
    }
    let set = set_ex_style(hwnd, style | WS_EX_NOACTIVATE);
    // Read back whatever the answer was: a style that is on is held — and so given back — even
    // when setting it was answered with an error.
    if ex_style(hwnd) & WS_EX_NOACTIVATE == 0 {
        if !known {
            forget_unheld(who);
        }
        return Err(set.err().unwrap_or_else(|| NOT_KEPT.to_string()));
    }
    if !known {
        // Held first, then off the list of those about to be: in one of the two throughout.
        locked(&HELD).push(Held { who: who.clone(), owners: vec![owner], front });
        INTEREST.fetch_add(1, Ordering::SeqCst);
        locked(&ADDING).retain(|w| w != who);
    }
    if let Err(why) = set {
        logging::line(
            "input",
            &format!("window {} ('{}') has the no-activate style although setting it was answered: {why}", who.hwnd, who.class),
        );
    }
    logging::line(
        "input",
        &format!(
            "window {} ('{}') of process {} given the no-activate style{} for clicks that leave the window in front \
             where it is",
            who.hwnd,
            who.class,
            who.pid,
            if known { " again (it had lost it)" } else { "" }
        ),
    );
    Ok(())
}

/// A style that could not be given to a window not held before: off the list of those about to
/// be, and its ledger line gone again.
fn forget_unheld(who: &Identity) {
    locked(&ADDING).retain(|w| w != who);
    if let Err(e) = write_ledger() {
        logging::line("input", &format!("the list of windows to give their style back could not be written: {e}"));
    }
}

/// Which held windows an event gives the style back to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GiveBack {
    /// Those held for module `owner` and no other (it stopped running).
    Owner(usize),
    /// Those whose window in front at the last click is not this one (another came to the front).
    Front(isize),
    /// All of them.
    All,
}

/// Whether `rule` gives the style back to a window held for `owners`, last clicked with `front`
/// in front; `owners` loses `owner` for [`GiveBack::Owner`] either way.
fn gives_back(rule: GiveBack, owners: &mut Vec<usize>, front: isize) -> bool {
    match rule {
        GiveBack::Owner(o) => {
            owners.retain(|x| *x != o);
            owners.is_empty()
        }
        GiveBack::Front(f) => front != f,
        GiveBack::All => true,
    }
}

/// Pure, for the tests: [`gives_back`].
#[cfg(test)]
pub(super) fn gives_back_for(rule: GiveBack, owners: &[usize], front: isize) -> (bool, Vec<usize>) {
    let mut o = owners.to_vec();
    let r = gives_back(rule, &mut o, front);
    (r, o)
}

/// The job that gives the style back by `rule` — `None` when nothing holds it or is about to.
/// Its decision is made on the thread, against the windows held then, after every click handed to
/// it before. `why` is for the log.
pub(super) fn give_back_job(rule: GiveBack, why: &str) -> Option<ActJob> {
    if INTEREST.load(Ordering::SeqCst) == 0 {
        return None;
    }
    let why = why.to_string();
    Some(Box::new(move || {
        give_back(rule, &why);
        Ok(())
    }))
}

/// Gives the style back by `rule`, on the thread. A program that does not answer in time
/// ([`answer_ms`]) keeps its window held — and in the ledger — for the next event, or the next
/// start. The ledger is rewritten once, at the end: until then it still names every window
/// taken off the list here, so an end of the application in the middle leaves their lines.
fn give_back(rule: GiveBack, why: &str) {
    let ms = answer_ms();
    let taken: Vec<Held> = {
        let mut held = locked(&HELD);
        let mut out = Vec::new();
        let mut i = 0;
        while i < held.len() {
            let front = held[i].front;
            if gives_back(rule, &mut held[i].owners, front) {
                out.push(held.remove(i));
            } else {
                i += 1;
            }
        }
        out
    };
    if taken.is_empty() {
        return;
    }
    let mut kept = Vec::new();
    for h in taken {
        match take_style_off(&h.who, ms) {
            Ok(words) => {
                INTEREST.fetch_sub(1, Ordering::SeqCst);
                logging::line("input", &format!("window {} ('{}'): {words} — {why}", h.who.hwnd, h.who.class));
            }
            Err(e) => {
                logging::line(
                    "input",
                    &format!("window {} ('{}') keeps the no-activate style for now: {e} — {why}", h.who.hwnd, h.who.class),
                );
                kept.push(h);
            }
        }
    }
    locked(&HELD).extend(kept);
    if let Err(e) = write_ledger() {
        logging::line("input", &format!("the list of windows to give their style back could not be written: {e}"));
    }
}

/// The style taken off `who` again, if it is still the window it was and still has it.
fn take_style_off(who: &Identity, ms: u32) -> Result<&'static str, String> {
    if !who.still_there() {
        return Ok("gone, nothing to give back");
    }
    let hwnd = who.hwnd as HWND;
    let style = ex_style(hwnd);
    if style & WS_EX_NOACTIVATE == 0 {
        return Ok("the no-activate style was off already");
    }
    answers(hwnd, ms)?;
    let set = set_ex_style(hwnd, style & !WS_EX_NOACTIVATE);
    // Read back whatever the answer was, as `hold` does: a style still on stays held.
    if ex_style(hwnd) & WS_EX_NOACTIVATE != 0 {
        return Err(set.err().unwrap_or_else(|| "the window kept the no-activate style".into()));
    }
    Ok("the no-activate style given back")
}

/// At exit: from now on no window is given the style, and the job that gives it back to every
/// window holding it — for the thread, after everything handed to it before, so after the last
/// click that could give it; `None` when nothing holds it or is about to. Each program is given
/// half a second to answer; one that does not keeps its line in the ledger, and the next start
/// gives the style back.
pub(super) fn exit_job() -> Option<ActJob> {
    STOPPING.store(true, Ordering::SeqCst);
    if INTEREST.load(Ordering::SeqCst) == 0 {
        return None;
    }
    Some(Box::new(|| {
        give_back(GiveBack::All, ENDING);
        Ok(())
    }))
}

/// At the start: the windows the ledger names, given their style back where they are still the
/// window named and still have it — what a run that ended without giving it back left. A window
/// whose program does not answer is held as one left over, given back at the next event.
pub(super) fn left_over_job() -> Option<ActJob> {
    let text = std::fs::read_to_string(ledger_path()).ok()?;
    Some(Box::new(move || {
        let named = parse_ledger(&text);
        for who in named {
            match take_style_off(&who, ANSWER_MS) {
                Ok(words) => logging::line(
                    "input",
                    &format!("window {} ('{}'), named by the last run: {words}", who.hwnd, who.class),
                ),
                Err(e) => {
                    logging::line(
                        "input",
                        &format!("window {} ('{}'), named by the last run, keeps the no-activate style for now: {e}", who.hwnd, who.class),
                    );
                    locked(&HELD).push(Held { who, owners: Vec::new(), front: 0 });
                    INTEREST.fetch_add(1, Ordering::SeqCst);
                }
            }
        }
        let written = write_ledger();
        if let Err(e) = &written {
            logging::line("input", &format!("the list of windows to give their style back could not be written: {e}"));
        }
        written
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_scaled_into_the_windows_own_dpi() {
        // Per-monitor aware: its DPI is the monitor's, and nothing changes.
        assert_eq!(scaled_client((300, 150), 144, 144), (300, 150));
        // Not aware, at 150 %: it draws at 96 and is stretched, so its pixels are two thirds.
        assert_eq!(scaled_client((300, 150), 96, 144), (200, 100));
        assert_eq!(scaled_client((301, 151), 96, 144), (201, 101), "rounded to the nearest");
        // System-aware at 150 % on a monitor at 100 %: it draws larger and is shrunk.
        assert_eq!(scaled_client((200, 100), 144, 96), (300, 150));
        // Not answered: left as it is.
        assert_eq!(scaled_client((300, 150), 0, 144), (300, 150));
        assert_eq!(scaled_client((300, 150), 96, 0), (300, 150));
    }

    /// The frame check compares in the window's own coordinates: a point in the right third of a
    /// window that is not DPI aware, at 150 %, is in its client area — its physical x beyond the
    /// rectangle the window itself has, its own x inside it.
    #[test]
    fn the_client_area_is_checked_in_the_windows_own_coordinates() {
        let own_client = (400, 300);
        assert!(in_client(scaled_client((590, 10), 96, 144), own_client));
        assert!(!in_client((590, 10), own_client), "the physical point against the window's own rectangle");
        assert!(!in_client(scaled_client((600, 10), 96, 144), own_client), "just right of it");
        assert!(in_client((0, 0), own_client) && in_client((399, 299), own_client));
        assert!(!in_client((-1, 0), own_client) && !in_client((0, 300), own_client));
    }

    /// A click with the style that the thread refused is a line in the log, in the words the event
    /// loop's refusals use — also when no callback waits for it; and the end of the application
    /// gives the style to no window after its last give-back: `hold` refuses first thing once it
    /// began, and the exit's job is made after that is set.
    #[test]
    fn a_refusal_on_the_thread_is_logged_and_the_end_gives_no_style() {
        assert_eq!(
            refusal_line(2755348, 940, 310, NOT_KEPT),
            "click at 940,310 for window 2755348 (via = \"noactivate\") refused: the window did not keep the no-activate style"
        );
        const SRC: &str = include_str!("click_win.rs");
        let part = |from: &str, to: &str| &SRC[SRC.find(from).unwrap()..SRC.find(to).unwrap()];
        let prepare = part("pub(super) fn prepare_hold(", "pub(super) fn refusal_line(");
        let job = prepare.find("Ok(Box::new(move || {").unwrap();
        assert!(prepare[job..].contains("logging::line(\"input\", &refusal_line(window, x, y, why));"));
        let hold = part("fn hold(who: &Identity", "fn forget_unheld(");
        assert!(hold.find("STOPPING.load(").unwrap() < hold.find("answers(hwnd").unwrap());
        let exit = part("pub(super) fn exit_job(", "pub(super) fn left_over_job(");
        assert!(exit.find("STOPPING.store(true").unwrap() < exit.find("INTEREST.load(").unwrap());
        // Nothing gives the style back on another thread than the one that gives it.
        assert!(!SRC.contains(concat!("fn give_back", "_now(")));
    }

    #[test]
    fn the_messages_say_the_point_the_button_and_the_keys() {
        assert_eq!(mouse_lparam(120, 45), (45 << 16) | 120);
        // A negative coordinate is the 16 bits Windows reads back with its sign.
        assert_eq!(mouse_lparam(-1, 2), (2 << 16) | 0xFFFF);
        assert_eq!(mouse_wparam(None, false, false), 0);
        assert_eq!(mouse_wparam(Some(MouseButton::Left), false, false), MK_LBUTTON);
        assert_eq!(mouse_wparam(Some(MouseButton::Right), true, false), MK_RBUTTON | MK_CONTROL);
        assert_eq!(mouse_wparam(Some(MouseButton::Middle), false, true), MK_MBUTTON | MK_SHIFT);
        assert_eq!(mouse_wparam(None, true, true), MK_CONTROL | MK_SHIFT, "the release holds no button");
        assert_eq!(button_messages(MouseButton::Left), (WM_LBUTTONDOWN, WM_LBUTTONUP));
        assert_eq!(button_messages(MouseButton::Right), (WM_RBUTTONDOWN, WM_RBUTTONUP));
        assert_eq!(button_messages(MouseButton::Middle), (WM_MBUTTONDOWN, WM_MBUTTONUP));
    }

    #[test]
    fn a_dpi_mode_and_a_refusal_are_said_in_words() {
        assert_eq!(awareness_words(2, true), "per-monitor v2");
        assert_eq!(awareness_words(2, false), "per-monitor");
        assert_eq!(awareness_words(1, false), "system");
        assert_eq!(awareness_words(0, false), "unaware");
        assert_eq!(awareness_words(-1, false), "of an awareness Windows did not say");
        assert_eq!(refused("the message", 5), HIGHER);
        assert_eq!(refused("the message", 1816), "the window did not take the message (error 1816)");
    }

    #[test]
    fn the_ledger_names_each_window_so_a_reused_handle_is_told_apart() {
        let a = Identity { hwnd: 0x2a0b14, pid: 4242, created: 133_400_000_000_000_000, class: "#32770".into() };
        let b = Identity { hwnd: 77, pid: 9, created: 1, class: "a class\twith a tab".into() };
        let text = format!("{LEDGER_HEADER}\n{}\n\nnot a line\n{}\n", ledger_line(&a), ledger_line(&b));
        assert_eq!(parse_ledger(&text), vec![a.clone(), b]);
        assert_eq!(ledger_line(&a), "2755348\t4242\t133400000000000000\t#32770");
        assert!(parse_ledger("1\t2\tthree\tX\n# 1\t2\t3\tY\n").is_empty(), "a line that does not parse is left out");
    }

    #[test]
    fn an_event_gives_back_what_it_ends_and_nothing_else() {
        // The module that stopped: given back once no other module clicks through the window.
        assert_eq!(gives_back_for(GiveBack::Owner(3), &[3], 100), (true, vec![]));
        assert_eq!(gives_back_for(GiveBack::Owner(3), &[3, 5], 100), (false, vec![5]));
        assert_eq!(gives_back_for(GiveBack::Owner(3), &[5], 100), (false, vec![5]));
        // Another window in front: given back; the same window in front again: kept.
        assert_eq!(gives_back_for(GiveBack::Front(200), &[3], 100), (true, vec![3]));
        assert_eq!(gives_back_for(GiveBack::Front(100), &[3], 100), (false, vec![3]));
        // One left over by an earlier run: whichever window comes to the front, and any stop.
        assert_eq!(gives_back_for(GiveBack::Front(100), &[], 0), (true, vec![]));
        assert_eq!(gives_back_for(GiveBack::Owner(1), &[], 0), (true, vec![]));
        assert!(gives_back_for(GiveBack::All, &[1, 2], 100).0);
    }
}
