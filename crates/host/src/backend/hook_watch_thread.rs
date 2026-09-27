//! The keyboard hook's watch on Windows: one thread named `keyboard-watch`, with a message-only
//! window that hears four things and asks the hook's thread to install the hook again when
//! one of them says it is needed. The decisions are in [`super::hook_watch`]; this file is the
//! plumbing around them.
//!
//! - **Raw Input from the keyboard** (usage page 1, usage 6, `RIDEV_INPUTSINK`, so it arrives
//!   whichever window is in front). Posted to this window, never waited for, so no timeout can
//!   take it away — the witness the hook's own silence cannot be. Only the physical key-downs
//!   are compared with the hook's calls ([`note_hook_call`]).
//! - **Session changes** (`WTSRegisterSessionNotification`): unlocked, connected to the console
//!   or to a remote client again.
//! - **Suspend and resume** (`RegisterSuspendResumeNotification` with a callback: a
//!   message-only window "does not receive broadcast messages", which is how `WM_POWERBROADCAST`
//!   reaches ordinary windows, and the callback needs no window at all).
//! - **The window in front** (`SetWinEventHook`, `EVENT_SYSTEM_FOREGROUND`, out of context, so
//!   delivered here). Not for the hook's survival: for what it recorded as held. When a window
//!   the hook is not called for comes to the front — and at every session change and suspend
//!   above — a key it saw go down can go up unseen, and the backend forgets what it recorded
//!   from key-downs alone ([`super::windows::forget_keys_held_out_of_sight`]). Here rather than
//!   on the pump, whose foreground events can wait behind a long OCR call: a record forgotten
//!   late could be one the user has made since.
//!
//! **Why a thread of its own, and not the hook's or the pump's.** The hook's thread must do
//! nothing but answer the hook: every keystroke on the machine waits for it. The pump stalls for
//! whole OCR calls, and a witness that reads the hook's state late would — were it comparing
//! counts — take a live hook for a dead one; [`super::hook_watch::Witness`] judges each key by
//! its own timestamp so lateness does not matter, but it still has no business on the thread
//! that runs every module. This thread runs for microseconds per key-down.
//!
//! **Why no crate.** `multiinput` (0.1.0, MIT) is the one maintained-looking Raw Input crate on
//! crates.io that covers keyboards; its thread polls a channel with a one-nanosecond sleep in a
//! loop — a core kept busy for as long as the application runs, which is days — it panics on a
//! failed `GetRawInputBuffer`, brings a second binding crate (`winapi`), and owns its window, so
//! the session and power notifications would need a second one. What is needed here is one
//! `RegisterRawInputDevices` call and one `GetRawInputData` per message, in the `windows-sys`
//! bindings the backend already uses. `dhc` is XInput and joysticks only.
//!
//! **Nothing here writes on the hook's thread.** The hook's thread swaps the hooks and posts the
//! outcome back here ([`report`]); the log line is written on this thread, because a file write
//! on the hook's thread just after a resume — with the disk still spinning up — is exactly the
//! kind of delay that gets a hook removed.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TokenIntegrityLevel,
    TOKEN_MANDATORY_LABEL, TOKEN_QUERY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{
    RegisterSuspendResumeNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
};
use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, GetThreadDesktop, GetUserObjectInformationW, OpenInputDesktop, DESKTOP_READOBJECTS,
    HDESK, UOI_NAME,
};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThreadId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, RegisterRawInputDevices, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RIDEV_INPUTSINK, RID_INPUT, RIM_TYPEKEYBOARD,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetForegroundWindow, GetMessageTime,
    GetMessageW, GetWindowThreadProcessId, PostMessageW, PostThreadMessageW, RegisterClassW,
    DEVICE_NOTIFY_CALLBACK, EVENT_SYSTEM_FOREGROUND, HWND_MESSAGE, MSG, WINEVENT_OUTOFCONTEXT,
    WM_APP, WM_INPUT, WM_WTSSESSION_CHANGE, WNDCLASSW,
};

use super::hook_watch::{self, Change, Outcome, Reason, Verdict, Witness};

/// Posted to the hook's thread (a thread message, no window): install the hook again. `WPARAM`
/// is the [`Reason`], encoded. Handled in `keyboard_hook_thread`'s loop.
pub(super) const WM_APP_REHOOK: u32 = WM_APP + 0x51;
/// Posted here by the suspend/resume callback; `WPARAM` is the `PBT_*` event.
const WM_APP_POWER: u32 = WM_APP + 0x52;
/// Posted here by the hook's thread: `WPARAM` the reason as it was asked, `LPARAM` the
/// [`Outcome`], encoded.
const WM_APP_REHOOKED: u32 = WM_APP + 0x53;

/// The tick of the hook's latest call, with the lowest bit set so that 0 means "never called".
/// One relaxed store per call, from inside the hook; a millisecond of precision lost against a
/// comparison with a second of slack.
static HOOK_CALLED_AT: AtomicU32 = AtomicU32::new(0);
/// The hook's thread, where re-installs are asked for.
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);
/// This thread's message-only window, 0 until it exists.
static WATCH_HWND: AtomicIsize = AtomicIsize::new(0);
static STARTED: AtomicBool = AtomicBool::new(false);
/// A re-install's reply the hook's thread could not post here (`hook_watch::pack_reply`), 0
/// when there is none. Looked at before every key-down and every change the watch handles, so
/// a reply lost to a full queue is read late rather than never: the watch asks for no second
/// re-install for a session change while it waits for one.
static LOST_REPLY: AtomicU64 = AtomicU64::new(0);

/// The hook was called, at `now` (`GetTickCount`). Called first thing in the hook, for every
/// key event of every kind.
pub(super) fn note_hook_call(now: u32) {
    HOOK_CALLED_AT.store(record_of_call(now), Ordering::Relaxed);
}

fn last_hook_call() -> Option<u32> {
    call_of_record(HOOK_CALLED_AT.load(Ordering::Relaxed))
}

/// [`HOOK_CALLED_AT`]'s encoding: the tick with its lowest bit set, so no call is ever 0.
fn record_of_call(now: u32) -> u32 {
    now | 1
}

fn call_of_record(record: u32) -> Option<u32> {
    (record != 0).then_some(record)
}

/// Starts the watch for the hook installed on `hook_thread`. Once per process; later calls do
/// nothing. Does not wait for the thread: nothing the caller does depends on it, and the pump
/// is the caller.
pub(super) fn start(hook_thread: u32) {
    HOOK_THREAD.store(hook_thread, Ordering::SeqCst);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let spawned = std::thread::Builder::new().name("keyboard-watch".to_string()).spawn(|| {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
        WATCH_HWND.store(0, Ordering::SeqCst);
        if let Err(p) = outcome {
            let text = p
                .downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "no message".to_string());
            crate::logging::line(
                "keys",
                &format!(
                    "the keyboard watch stopped after a panic ({text}); a keyboard hook Windows \
                     removes is not installed again until the application restarts"
                ),
            );
        }
    });
    if let Err(e) = spawned {
        crate::logging::line(
            "keys",
            &format!(
                "could not start the keyboard watch's thread ({e}); a keyboard hook Windows \
                 removes is not installed again until the application restarts"
            ),
        );
    }
}

/// The hook's thread reports a re-install asked with `reason` (as it came, encoded). Posts it
/// here and returns: no log line is written on the hook's thread. A reply that cannot be posted
/// — the watch's queue full, after the watch was held up — is left in [`LOST_REPLY`] for the
/// watch to pick up.
pub(super) fn report(reason: usize, outcome: Outcome) {
    let hwnd = WATCH_HWND.load(Ordering::SeqCst);
    // SAFETY: posting to our own message-only window.
    let posted =
        hwnd != 0 && unsafe { PostMessageW(hwnd as HWND, WM_APP_REHOOKED, reason, outcome.encode()) } != 0;
    if !posted {
        LOST_REPLY.store(hook_watch::pack_reply(reason, outcome), Ordering::SeqCst);
    }
}

/// What the witness needs to decide whether the hook could have seen a key.
struct Look {
    /// This process's integrity level (the RID of its mandatory label).
    own_level: Option<u32>,
    /// The name of the desktop this thread — and the hook, and the process — is on.
    desktop: Option<Vec<u16>>,
    /// The last window in front that was asked about: (window, process, its level).
    cache: Option<(isize, u32, Option<u32>)>,
}

struct State {
    witness: Witness,
    look: Look,
    /// A re-install asked of the hook's thread and not answered yet.
    pending: Option<Reason>,
    /// Said that the hook's thread has not answered, for the current `pending`.
    said_unanswered: bool,
    /// Said that a request could not be posted.
    said_post_failed: bool,
    /// `WTSRegisterSessionNotification` refused, with this error, and not accepted since. Tried
    /// again at the key-downs [`hook_watch::retry_at`] picks, counted in `keys_since_refusal`.
    session_refused: Option<u32>,
    keys_since_refusal: u64,
    /// Said that a screen reader's modifier the hook recorded as held was forgotten (once per
    /// process: NVDA's menu alone does it every time it is opened with Insert).
    said_forgot: bool,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn run() {
    let hwnd = create_window();
    if hwnd.is_null() {
        // SAFETY: plain error query.
        let err = unsafe { GetLastError() };
        crate::logging::line(
            "keys",
            &format!(
                "could not create the keyboard watch's window (error {err}); a keyboard hook \
                 Windows removes is not installed again until the application restarts"
            ),
        );
        return;
    }
    WATCH_HWND.store(hwnd as isize, Ordering::SeqCst);

    let raw = register_raw_input(hwnd);
    let session = register_session(hwnd);
    let power = register_power();
    let foreground = register_foreground().map(|_| ());
    // SAFETY: the pseudo-handle of this process, and this thread's own desktop; neither is
    // closed (neither needs to be).
    let (own_level, desktop) = unsafe {
        (process_level(GetCurrentProcess()), desktop_name(GetThreadDesktop(GetCurrentThreadId())))
    };
    let said = |r: &Result<(), u32>| match r {
        Ok(()) => "yes".to_string(),
        Err(e) => format!("NO (error {e})"),
    };
    crate::logging::line(
        "keys",
        &format!(
            "keyboard watch: raw input from the keyboard is compared with the hook's calls, and \
             the hook is installed again after it misses {} key-downs in a row, and after a \
             resume or an unlock. Raw input {}; session notifications {}{}; suspend/resume \
             notifications {}; foreground events {}; this process's integrity level {}; \
             desktop {}",
            hook_watch::MISSES_TO_REHOOK,
            said(&raw),
            said(&session),
            if session.is_err() { " — tried again at the 1st, 2nd, 4th, 8th … key-down" } else { "" },
            said(&power),
            said(&foreground),
            own_level.map_or("unknown — no key is counted".to_string(), |l| format!("{l:#06x}")),
            desktop
                .as_ref()
                .map_or("unknown — no key is counted".to_string(), |d| String::from_utf16_lossy(d)),
        ),
    );
    STATE.with(|s| {
        *s.borrow_mut() = Some(State {
            witness: Witness::default(),
            look: Look { own_level, desktop, cache: None },
            pending: None,
            said_unanswered: false,
            said_post_failed: false,
            session_refused: session.err(),
            keys_since_refusal: 0,
            said_forgot: false,
        });
    });

    let mut msg: MSG = unsafe { std::mem::zeroed() };
    // SAFETY: a standard message loop on this thread's own queue.
    unsafe {
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            DispatchMessageW(&msg);
        }
    }
}

fn create_window() -> HWND {
    // SAFETY: registering a class and creating a message-only window on this thread; the class
    // name outlives both calls.
    unsafe {
        let hmod = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = "AutomationPlatformKeyboardWatch\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(watch_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hmod,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            std::ptr::null_mut(),
            hmod,
            std::ptr::null(),
        )
    }
}

/// The keyboard as raw input, to this window, in the background too. Without `RIDEV_NOLEGACY`
/// and without `RIDEV_NOHOTKEYS`: nothing about the keyboard changes for anybody, this process
/// included — raw input is a copy. One window per device class per process may hold this
/// registration; nothing else in the application registers the keyboard (the gamepad thread
/// deliberately uses device notifications instead of raw input).
fn register_raw_input(hwnd: HWND) -> Result<(), u32> {
    let rid = RAWINPUTDEVICE {
        usUsagePage: 0x01, // generic desktop
        usUsage: 0x06,     // keyboard
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };
    // SAFETY: one valid structure, its size given.
    let ok = unsafe { RegisterRawInputDevices(&rid, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) };
    if ok != 0 {
        Ok(())
    } else {
        // SAFETY: plain error query.
        Err(unsafe { GetLastError() })
    }
}

fn register_session(hwnd: HWND) -> Result<(), u32> {
    // SAFETY: our own window, which lives as long as the process (it is never destroyed, so
    // the registration needs no matching unregistration).
    if unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) } != 0 {
        Ok(())
    } else {
        // SAFETY: plain error query. RPC_S_INVALID_BINDING (1702) when the Remote Desktop
        // services have not started yet; tried again on later key-downs.
        Err(unsafe { GetLastError() })
    }
}

/// Suspend and resume, through a callback. The parameters are leaked: the registration lasts as
/// long as the process, and Windows keeps the pointer.
fn register_power() -> Result<(), u32> {
    let params: &'static mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS = Box::leak(Box::new(
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(power_changed), Context: std::ptr::null_mut() },
    ));
    // SAFETY: with DEVICE_NOTIFY_CALLBACK the recipient is a pointer to the parameters, which
    // live for the rest of the process.
    let handle = unsafe {
        RegisterSuspendResumeNotification(
            params as *mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS as HANDLE,
            DEVICE_NOTIFY_CALLBACK,
        )
    };
    if handle != 0 {
        Ok(())
    } else {
        // SAFETY: plain error query.
        Err(unsafe { GetLastError() })
    }
}

/// The window in front, as it changes: out of context, so the events are delivered to this
/// thread while it waits in `GetMessageW`, from every process on this desktop. Lasts as long as
/// the process.
fn register_foreground() -> Result<HWINEVENTHOOK, u32> {
    // SAFETY: an out-of-context hook with a callback of ours that lives for the process.
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(foreground_changed),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook.is_null() {
        // SAFETY: plain error query (SetWinEventHook documents none; 0 then).
        Err(unsafe { GetLastError() })
    } else {
        Ok(hook)
    }
}

/// A window came to the front. If it is one the hook is not called for, what the hook recorded
/// as held from key-downs alone is forgotten — see `hook_watch::keys_go_unseen`.
unsafe extern "system" fn foreground_changed(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // OBJID_WINDOW, CHILDID_SELF: the top-level window itself.
    if event != EVENT_SYSTEM_FOREGROUND || id_object != 0 || id_child != 0 || hwnd.is_null() {
        return;
    }
    // Unwinding out of a callback of the system's is an abort; see `watch_wndproc`.
    let caught = std::panic::catch_unwind(|| {
        with_state(|s| {
            let level = level_of_window(&mut s.look, hwnd as isize);
            if hook_watch::keys_go_unseen(s.look.own_level, level.flatten()) {
                let what = format!(
                    "a window the keyboard hook is not called for came to the front ({})",
                    match level.flatten() {
                        Some(l) => format!("integrity level {l:#06x}, above this process's"),
                        None => "its integrity level could not be read".to_string(),
                    }
                );
                forget_out_of_sight(s, &what);
            }
        });
    });
    if caught.is_err() {
        static SAID: AtomicBool = AtomicBool::new(false);
        if !SAID.swap(true, Ordering::Relaxed) {
            crate::logging::line("keys", "the keyboard watch panicked handling a foreground change");
        }
    }
}

/// The keyboard went where the hook is not called (`what`, for the log): the backend forgets
/// what the hook recorded as held from key-downs alone. Said once per process when that
/// included a screen reader's modifier — the line that tells a session whose captured keys all
/// went to the screen reader from one whose hook Windows removed.
fn forget_out_of_sight(s: &mut State, what: &str) {
    let Some(age) = super::windows::forget_keys_held_out_of_sight() else {
        return;
    };
    if !s.said_forgot {
        s.said_forgot = true;
        crate::logging::line(
            "keys",
            &format!(
                "{what} while the keyboard hook had a screen reader's modifier recorded as held \
                 (seen going down {age} ms before): forgotten, since its key-up goes where the hook \
                 is not called. Kept, it would have let every captured key through to the \
                 application, as a keystroke meant for the screen reader, until that modifier next \
                 went up in front of a window the hook sees. Said once"
            ),
        );
    }
}

/// Runs on a thread of the system's. Posts the event here and returns; nothing in it can panic.
unsafe extern "system" fn power_changed(
    _context: *const core::ffi::c_void,
    kind: u32,
    _setting: *const core::ffi::c_void,
) -> u32 {
    let hwnd = WATCH_HWND.load(Ordering::SeqCst);
    if hwnd != 0 {
        PostMessageW(hwnd as HWND, WM_APP_POWER, kind as usize, 0);
    }
    0 // ERROR_SUCCESS
}

unsafe extern "system" fn watch_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // An unwind out of a window procedure is an abort; whatever goes wrong in here must cost
    // the watch at most, never the process that carries the user's keyboard overlay.
    let caught = std::panic::catch_unwind(|| match msg {
        WM_INPUT => on_raw_input(lparam as HRAWINPUT),
        WM_WTSSESSION_CHANGE => on_change(
            hook_watch::on_session_change(wparam as u32),
            &format!("the session changed (WTS event {})", wparam as u32),
        ),
        WM_APP_POWER => on_change(
            hook_watch::on_power(wparam as u32),
            &format!("the machine suspended or resumed (PBT event {:#x})", wparam as u32),
        ),
        WM_APP_REHOOKED => on_rehooked(wparam, lparam),
        _ => {}
    });
    if caught.is_err() {
        static SAID: AtomicBool = AtomicBool::new(false);
        if !SAID.swap(true, Ordering::Relaxed) {
            crate::logging::line("keys", &format!("the keyboard watch panicked handling message {msg:#x}"));
        }
    }
    match msg {
        WM_APP_POWER | WM_APP_REHOOKED => 0,
        // WM_INPUT goes on to DefWindowProcW as well, which is how the system frees the input.
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Runs `f` on the watch's state. Skipped, rather than a panic, if it is already borrowed —
/// which would take a callback delivered while another one runs, and nothing here waits in a
/// way that delivers one.
fn with_state(f: impl FnOnce(&mut State)) {
    STATE.with(|s| {
        if let Ok(mut state) = s.try_borrow_mut() {
            if let Some(state) = state.as_mut() {
                f(state);
            }
        }
    });
}

/// A reply the hook's thread could not post ([`report`]), handled as if it had arrived.
fn take_lost_reply() {
    let packed = LOST_REPLY.swap(0, Ordering::SeqCst);
    if packed == 0 {
        return;
    }
    if let Some((reason, outcome)) = hook_watch::unpack_reply(packed) {
        settle(reason, outcome);
    }
}

fn on_raw_input(handle: HRAWINPUT) {
    // SAFETY: a RAWINPUT buffer on the stack, its size passed in and out; a keyboard report
    // fits (the union's largest member is the mouse's). Anything that does not fit is refused
    // by the call with (UINT)-1 and skipped.
    let (raw, got) = unsafe {
        let mut raw: RAWINPUT = std::mem::zeroed();
        let mut size = std::mem::size_of::<RAWINPUT>() as u32;
        let got = GetRawInputData(
            handle,
            RID_INPUT,
            &mut raw as *mut RAWINPUT as *mut core::ffi::c_void,
            &mut size,
            std::mem::size_of::<RAWINPUTHEADER>() as u32,
        );
        (raw, got)
    };
    if got == 0 || got == u32::MAX || raw.header.dwType != RIM_TYPEKEYBOARD {
        return;
    }
    // SAFETY: the header says this is a keyboard report.
    let kb = unsafe { raw.data.keyboard };
    if !hook_watch::physical_down(!raw.header.hDevice.is_null(), kb.Flags, kb.VKey, kb.MakeCode) {
        return;
    }
    // The input's own timestamp, on the tick clock the hook's calls are stamped with — see
    // `hook_watch::event_time` for when it is not believed.
    // SAFETY: plain queries: the message being dispatched, and the tick clock.
    let time = unsafe { hook_watch::event_time(GetMessageTime() as u32, GetTickCount()) };
    take_lost_reply();
    with_state(|s| {
        if let Some(refused) = s.session_refused {
            s.keys_since_refusal += 1;
            if hook_watch::retry_at(s.keys_since_refusal) {
                let hwnd = WATCH_HWND.load(Ordering::SeqCst) as HWND;
                if register_session(hwnd).is_ok() {
                    s.session_refused = None;
                    crate::logging::line(
                        "keys",
                        &format!(
                            "keyboard watch: session notifications registered now, at key-down {} \
                             after they were refused (error {refused})",
                            s.keys_since_refusal
                        ),
                    );
                }
            }
        }
        let look = &mut s.look;
        let verdict = s.witness.key_down(time, last_hook_call(), || hook_can_see_now(look));
        if let Verdict::Rehook { downs } = verdict {
            request(s, Reason::Missed { downs });
        }
    });
}

/// A session change or a suspend/resume (`what`, for the log). Every one that means anything
/// took the keyboard somewhere the hook is not called, or brings it back from there, so what the
/// hook recorded as held is forgotten at each; a re-install is asked for where
/// [`hook_watch::on_session_change`] and [`hook_watch::on_power`] say so.
fn on_change(change: Change, what: &str) {
    take_lost_reply();
    with_state(|s| {
        if change != Change::Nothing {
            forget_out_of_sight(s, what);
        }
        match change {
            Change::Rehook(reason) => {
                s.witness.restart();
                request(s, reason);
            }
            Change::Restart => s.witness.restart(),
            Change::Nothing => {}
        }
    });
}

/// Asks the hook's thread to install the hook again.
///
/// While a request is on its way, another one for a session change adds nothing (a remote
/// connect and the unlock that follows it come together). One from the witness is posted all
/// the same: it comes only after the bar, raised by the request before it, has been reached
/// again, so a reply that went missing costs one round of missed key-downs, never the feature —
/// and a second swap does no harm. That the reply is overdue is said once.
fn request(s: &mut State, reason: Reason) {
    if let Some(asked) = s.pending {
        if !matches!(reason, Reason::Missed { .. }) {
            return;
        }
        if !s.said_unanswered {
            s.said_unanswered = true;
            crate::logging::line(
                "keys",
                &format!(
                    "the keyboard hook's thread has not answered the request to install the hook \
                     again ({}), and the hook has missed more key-downs since; asked again. If \
                     no \"installed again\" line follows, that thread is not being scheduled, or \
                     is stuck",
                    asked.words()
                ),
            );
        }
    }
    let tid = HOOK_THREAD.load(Ordering::SeqCst);
    // SAFETY: a thread message to the hook's thread, which has a queue (it pumps one).
    let posted = tid != 0 && unsafe { PostThreadMessageW(tid, WM_APP_REHOOK, reason.encode(), 0) } != 0;
    if posted {
        if s.pending.is_none() {
            s.said_unanswered = false;
        }
        s.pending = Some(reason);
    } else if !s.said_post_failed {
        s.said_post_failed = true;
        // SAFETY: plain error query.
        let err = unsafe { GetLastError() };
        crate::logging::line(
            "keys",
            &format!(
                "could not ask the keyboard hook's thread to install the hook again ({}): \
                 PostThreadMessageW error {err}",
                reason.words()
            ),
        );
    }
}

fn on_rehooked(wparam: WPARAM, lparam: LPARAM) {
    match (Reason::decode(wparam), Outcome::decode(lparam)) {
        (Some(reason), Some(outcome)) => settle(reason, outcome),
        // Nothing posts one that does not decode; the request is answered all the same, so
        // the next one is not held back by it.
        _ => with_state(|s| s.pending = None),
    }
}

/// A re-install's reply: the request is answered, the witness learns the outcome, and the line
/// is written.
fn settle(reason: Reason, outcome: Outcome) {
    // SAFETY: reads the tick clock.
    let now = unsafe { GetTickCount() };
    let by_witness = matches!(reason, Reason::Missed { .. });
    let mut needed = hook_watch::MISSES_TO_REHOOK;
    with_state(|s| {
        s.pending = None;
        s.said_unanswered = false;
        s.witness.rehooked(by_witness, outcome.installed(), now);
        needed = s.witness.needed();
    });
    let mut text = hook_watch::line(reason, outcome);
    if needed != hook_watch::MISSES_TO_REHOOK {
        // Only after a re-install the witness asked for, or a failed one: say how long the
        // next one takes, so a log with several of them in a row reads as the back-off it is.
        text.push_str(&format!(
            ". Until the hook is called again, the next re-install for missed keys takes {needed} \
             of them in a row"
        ));
    }
    crate::logging::line("keys", &text);
}

/// Whether the hook could have seen a key pressed now: the keyboard is on our desktop, and the
/// window in front belongs to a process no higher than ours (see `hook_watch::hook_sees`).
/// Asked only while the hook looks as if it missed a key.
fn hook_can_see_now(look: &mut Look) -> bool {
    if !input_desktop_is(look.desktop.as_deref()) {
        return false;
    }
    // SAFETY: a plain query.
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() {
        return false;
    }
    match level_of_window(look, fg as isize) {
        Some(level) => hook_watch::hook_sees(look.own_level, level),
        None => false,
    }
}

/// The integrity level of the process a window belongs to: `None` when the window has no
/// process (it is gone), `Some(None)` when the level could not be read. The last window asked
/// about is remembered with its process, so the witness asking again and again while the same
/// window stays in front costs one query.
fn level_of_window(look: &mut Look, window: isize) -> Option<Option<u32>> {
    let mut pid = 0u32;
    // SAFETY: a plain query; it validates the handle.
    if unsafe { GetWindowThreadProcessId(window as HWND, &mut pid) } == 0 {
        return None;
    }
    Some(match look.cache {
        Some((w, p, level)) if w == window && p == pid => level,
        _ => {
            let level = level_of_pid(pid);
            look.cache = Some((window, pid, level));
            level
        }
    })
}

/// Whether the desktop that has the keyboard is `ours`. The lock screen and the secure desktop
/// of a UAC prompt cannot even be opened from here, which answers no as well.
fn input_desktop_is(ours: Option<&[u16]>) -> bool {
    let Some(ours) = ours else {
        return false;
    };
    // SAFETY: the handle is checked and closed; the name is read into a buffer of ours.
    unsafe {
        let input: HDESK = OpenInputDesktop(0, 0, DESKTOP_READOBJECTS);
        if input.is_null() {
            return false;
        }
        let name = desktop_name(input);
        CloseDesktop(input);
        name.as_deref() == Some(ours)
    }
}

/// A desktop's name, as UTF-16 without the terminator.
unsafe fn desktop_name(desktop: HDESK) -> Option<Vec<u16>> {
    if desktop.is_null() {
        return None;
    }
    let mut buf = [0u16; 128];
    let mut needed = 0u32;
    let ok = GetUserObjectInformationW(
        desktop as HANDLE,
        UOI_NAME,
        buf.as_mut_ptr() as *mut core::ffi::c_void,
        (buf.len() * 2) as u32,
        &mut needed,
    );
    if ok == 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Some(buf[..len].to_vec())
}

fn level_of_pid(pid: u32) -> Option<u32> {
    // SAFETY: the handle is checked and closed. PROCESS_QUERY_LIMITED_INFORMATION is granted
    // for an elevated process of the same user as well; for a protected or a system process it
    // may not be, and then the level is unknown and the key is not counted.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let level = process_level(process);
        CloseHandle(process);
        level
    }
}

/// The RID of a process's mandatory integrity label: 0x1000 low, 0x2000 medium, 0x2010 medium
/// with UIAccess, 0x3000 high, 0x4000 system.
unsafe fn process_level(process: HANDLE) -> Option<u32> {
    let mut token: HANDLE = std::ptr::null_mut();
    if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
        return None;
    }
    // A TOKEN_MANDATORY_LABEL and the SID it points to; a SID is at most 68 bytes.
    let mut buf = [0u64; 16];
    let mut got = 0u32;
    let ok = GetTokenInformation(
        token,
        TokenIntegrityLevel,
        buf.as_mut_ptr() as *mut core::ffi::c_void,
        std::mem::size_of_val(&buf) as u32,
        &mut got,
    );
    CloseHandle(token);
    if ok == 0 {
        return None;
    }
    let label = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
    let sid = label.Label.Sid;
    if sid.is_null() {
        return None;
    }
    let count = *GetSidSubAuthorityCount(sid);
    if count == 0 {
        return None;
    }
    Some(*GetSidSubAuthority(sid, count as u32 - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reading this process's own level and desktop touches no window, no input and no other
    /// process — the parts of the watch that can run headless. The level of an ordinary test
    /// process is medium or, in an elevated shell, high.
    #[test]
    fn this_process_has_a_level_and_a_desktop() {
        let (level, desktop) = unsafe {
            (process_level(GetCurrentProcess()), desktop_name(GetThreadDesktop(GetCurrentThreadId())))
        };
        let level = level.expect("the integrity level of this process");
        assert!((0x1000..=0x4000).contains(&level), "{level:#x}");
        let desktop = desktop.expect("the name of this thread's desktop");
        assert!(!desktop.is_empty());
        assert!(hook_watch::hook_sees(Some(level), level_of_pid(std::process::id())));
    }

    /// The four registrations the watch makes, on a message-only window of the test's own —
    /// invisible, never focusable — and each taken back at once. Nothing is read: no message
    /// of this thread is ever retrieved, so not one key of whoever is at the machine is seen.
    /// What it proves is that the structures, flags and sizes are ones Windows accepts, which
    /// otherwise shows only as a "NO (error …)" in a running application's log.
    #[test]
    fn the_watch_registrations_are_accepted() {
        use windows_sys::Win32::System::RemoteDesktop::WTSUnRegisterSessionNotification;
        use windows_sys::Win32::UI::Input::RIDEV_REMOVE;
        use windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow;

        let hwnd = create_window();
        assert!(!hwnd.is_null(), "a message-only window");
        assert_eq!(register_raw_input(hwnd), Ok(()));
        let remove = RAWINPUTDEVICE {
            usUsagePage: 0x01,
            usUsage: 0x06,
            dwFlags: RIDEV_REMOVE,
            hwndTarget: std::ptr::null_mut(),
        };
        // SAFETY: one valid structure; RIDEV_REMOVE with no target, as the call requires.
        let removed =
            unsafe { RegisterRawInputDevices(&remove, 1, std::mem::size_of::<RAWINPUTDEVICE>() as u32) };
        assert_ne!(removed, 0, "the raw input registration taken back");
        // The session notification needs the Remote Desktop services, which a CI machine may
        // not run; its refusal is what the watch retries on key-downs, so only an accepted one
        // is taken back here.
        if register_session(hwnd).is_ok() {
            // SAFETY: the window registered just above.
            unsafe { WTSUnRegisterSessionNotification(hwnd) };
        }
        // The suspend/resume callback: WATCH_HWND is 0 in a test, so a notification that did
        // arrive would post nothing.
        assert_eq!(register_power(), Ok(()));
        // The foreground events: delivered only while this thread waits for messages, which it
        // never does, and taken back at once.
        let events = register_foreground().expect("an out-of-context foreground hook");
        // SAFETY: the hook registered just above, on this thread.
        assert_ne!(unsafe { windows_sys::Win32::UI::Accessibility::UnhookWinEvent(events) }, 0);
        // SAFETY: our own window, on this thread.
        unsafe { DestroyWindow(hwnd) };
    }

    #[test]
    fn the_hook_call_record_starts_empty_and_never_reads_as_empty_again() {
        assert_eq!(call_of_record(0), None, "never called");
        // A call at tick 0 — a real tick, once every 49.7 days — is still a call.
        assert_eq!(call_of_record(record_of_call(0)), Some(1));
        assert_eq!(call_of_record(record_of_call(u32::MAX)), Some(u32::MAX));
        assert_eq!(call_of_record(record_of_call(1_000)), Some(1_001));
    }
}
