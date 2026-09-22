//! Windows implementation of the platform [`Backend`](super::Backend):
//! window enumeration (Win32), global hotkeys (`RegisterHotKey` + `GetMessage`, and matched
//! in the low-level keyboard hook as well — see `hotkey_hook.rs`), the low-level keyboard
//! hook on a thread of its own (`keyboard_hook_thread`),
//! foreground-change events (`SetWinEventHook`), and screen capture (GDI, or desktop
//! duplication for the modules that declare it — see `dxgi.rs`).

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicI32, AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard};

use super::dxgi::{self, Caller, Fallback};
use super::hotkey_hook::{self, Down, Mods, OsPress, Route, NO_ID};
use super::{
    Backend, CaptureFn, CaptureSource, CapturedImage, ControlInfo, DumpNode, HostEvents,
    MouseButton, OcrLine, OcrShot, OcrText, OcrThread, OcrWord, OcrWorker, Recognise, WinInfo,
    CAPTURE_FAILED, DUPLICATION_UNANSWERED,
};
use crate::ocr::plan::{self, Plan};
use crate::ocr::types::Rect;

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, GetPixel, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    SetThreadPriority, PROCESS_QUERY_LIMITED_INFORMATION, THREAD_PRIORITY_HIGHEST,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, FILTERKEYS, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_KEYBOARD,
    INPUT_MOUSE,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, VK_CONTROL, VK_LWIN,
    VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumChildWindows,
    EnumWindows, GetAncestor, GetClassNameW, GetClientRect, GetCursorPos,
    GetForegroundWindow, GetMessageW, SystemParametersInfoW, FKF_FILTERKEYSON, LLKHF_INJECTED,
    SPI_GETFILTERKEYS, SPI_GETKEYBOARDDELAY,
    GetGUIThreadInfo, GetSystemMetrics, GetWindowRect,
    MsgWaitForMultipleObjects, PeekMessageW, PM_REMOVE, QS_ALLINPUT, WM_QUIT,
    GetMessageTime, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    WindowFromPoint,
    GUI_INMENUMODE, GUI_POPUPMENUMODE, GUI_SYSTEMMENUMODE,
    PostMessageW, PostThreadMessageW, RegisterClassW, SetCursorPos, SetWindowsHookExW,
    TranslateMessage,
    EVENT_OBJECT_FOCUS, EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND, GA_PARENT, GA_ROOT,
    GUITHREADINFO,
    HC_ACTION, HWND_MESSAGE, KBDLLHOOKSTRUCT, MSG, SM_CXSCREEN, SM_CYSCREEN,
    WH_KEYBOARD_LL, WINEVENT_OUTOFCONTEXT, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_NULL, WM_SYSKEYDOWN,
    WM_SYSKEYUP, WNDCLASSW,
};

thread_local! {
    /// HWNDs whose window became foreground, queued by the WinEvent hook and
    /// drained by the event loop on the same thread.
    static FOREGROUND_QUEUE: RefCell<Vec<isize>> = RefCell::new(Vec::new());
    /// Set when the focused element changed (coalesced; drained by the loop). A
    /// focus change need not raise a foreground event (e.g. focusing into a
    /// plugin embedded in an already-foreground DAW host window).
    static FOCUS_DIRTY: std::cell::Cell<bool> = std::cell::Cell::new(false);
    /// Deadlines for delayed re-checks: a window can become foreground before it is
    /// matchable (empty title / not yet shown — window_info None, so active() is nil),
    /// then settle with NO further event. After such a foreground event we queue a few
    /// re-checks here so the window is picked up once it has a title and is visible.
    static DELAYED_RECHECK: RefCell<Vec<std::time::Instant>> = RefCell::new(Vec::new());
}

/// HWND (as isize) of the lazily-created message-only window that owns global
/// hotkeys, so `WM_HOTKEY` is routed to our window proc by `DispatchMessage`
/// and survives a foreign message loop (e.g. wxWidgets', which would drop a
/// NULL-hwnd thread message).
static HOTKEY_HWND: AtomicIsize = AtomicIsize::new(0);

/// Thread id of the event loop — the pump — so the hooks can wake `GetMessage` after queueing
/// something for it: the WinEvent hook runs on that thread, the keyboard hook on its own.
static PUMP_THREAD: AtomicU32 = AtomicU32::new(0);

// What the keyboard hook shares with the pump. The hook runs on a thread of its own
// (`keyboard_hook_thread`), so these are behind locks rather than thread-locals. Every lock is
// held for a copy, a push or a table lookup — microseconds — and never across a call into
// another program, so the hook never waits for more than that.

/// (vk, modifier-mask) pairs currently intercepted (+ suppressed) by the hook. Written by the
/// pump when the captured set changes.
static CAPTURED_KEYS: Mutex<Vec<(u32, u8)>> = Mutex::new(Vec::new());
/// Captured key-downs (vk, modifier-mask), queued by the hook for the event loop.
static KEY_QUEUE: Mutex<Vec<(u32, u8)>> = Mutex::new(Vec::new());
/// Hotkey presses, drained by the pump: ids the message-only window proc received as
/// `WM_HOTKEY` on the pump's thread, and ids the keyboard hook matched itself on its own (see
/// `hotkey_hook`), each with the way it came, so that the pump can tell the two deliveries of
/// one press apart. Drained in place rather than taken, so its capacity stays.
static HOTKEY_QUEUE: Mutex<Vec<(i32, Route)>> = Mutex::new(Vec::new());
/// The hotkeys Windows GRANTED, matched in the hook as well as by `RegisterHotKey` — see
/// `hotkey_hook` for why. `None` until the first grant, which is also the table's only
/// allocation. Filed and removed by the pump; the hook looks combinations up and keeps its
/// record of held keys in it.
static HOOK_HOTKEYS: Mutex<Option<hotkey_hook::Table>> = Mutex::new(None);

/// A lock that a panic elsewhere cannot poison for good: the hook must go on answering, and
/// the data behind every lock here is valid after any single push or copy.
fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

thread_local! {
    /// (The hook's thread.) Keys whose DOWN was let through because a screen reader's modifier
    /// was held, so that their UP is let through as well even if the modifier has since been
    /// released. A down without its up leaves the application holding a key nobody is pressing.
    static SCREEN_READER_PASSED: RefCell<Vec<u32>> = RefCell::new(Vec::new());
    /// (The hook's thread.) The modifiers as the hook saw them go by, for judging a late call
    /// by the moment of its key event — see `hotkey_hook::Mods`.
    static HOOK_MODS: Cell<Mods> = Cell::new(Mods::default());
    /// (The pump.) Pairs a late hook's press with the `WM_HOTKEY` the system posted for it
    /// anyway. Never touched by the hook.
    static HOTKEY_DEDUPE: RefCell<hotkey_hook::Dedupe> =
        RefCell::new(hotkey_hook::Dedupe::default());
    /// (The pump.) The window that was in front when the last "arrived through RegisterHotKey"
    /// line was written, so that it is written once per window rather than once per press.
    static MISS_LOGGED_FOR: Cell<isize> = const { Cell::new(0) };
}

/// A hotkey has been filed in [`HOOK_HOTKEYS`] at some point, so the hook consults it. Set
/// once, from registration on the pump thread, and never cleared: an empty table answers "not
/// mine" as cheaply as a flag would, and the flag only spares every keystroke the lock while
/// no module has registered anything.
static HOOK_HOTKEYS_PRESENT: AtomicBool = AtomicBool::new(false);

/// The auto-repeat threshold last put in force, for saying so when the keyboard settings
/// change it — see `repeat_threshold_now`.
static REPEAT_MS_IN_FORCE: AtomicU32 = AtomicU32::new(hotkey_hook::REPEAT_MS);

/// The key sent to mask a swallowed hotkey's modifiers — see `hotkey_hook::needs_mask_key`.
///
/// 0xE8 is listed as unassigned in the virtual-key table, so no application and no screen
/// reader binds it; it is the key AutoHotkey documents for the same job, as the one with the
/// fewest side effects. Its default there is Ctrl, which a screen reader takes as "stop
/// speaking", and would silence the announcement the hotkey is about to make.
const VK_MASK_KEY: u16 = 0xE8;

/// Whether a screen reader's modifier key is held, as OUR OWN HOOK saw it go past.
///
/// The first version of this check asked `GetAsyncKeyState`, and the tester reported that
/// NVDA+Space still did not get through. The reason is that a screen reader installs a
/// low-level hook of its own and SUPPRESSES its modifier there — so by the time anybody asks
/// the OS whether Insert is down, the key the user is holding is not there to be found. The
/// check was asking the wrong witness.
///
/// This hook is a witness that cannot be fooled either way. If our hook runs before the screen
/// reader's, we see the modifier go down and record it here before it is eaten. If the screen
/// reader's runs first, it eats the whole combination and we never see the second key at all,
/// which is the same outcome by a different route.
static SCREEN_READER_MOD_DOWN: AtomicBool = AtomicBool::new(false);

static KEY_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static FG_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// HWND (isize) the captured-key suppression is scoped to (0 = global). The hook
/// only intercepts a captured key while this window is foreground — so a menu a
/// control opened (another window) gets Tab/Enter natively, ReaHotkey-style.
/// Which modifier is currently held with nothing pressed since — 0 when none is, which is
/// also what any other key down resets it to. See the tap branch in the hook.
static TAP_ARMED: AtomicI32 = AtomicI32::new(0);
static KEY_SCOPE: AtomicIsize = AtomicIsize::new(0);

/// Set by an overlay while a (Qt/UIA) menu is open in the focused plugin, so its
/// captured nav keys (Tab/Enter) pass through to the menu. The Win32 menu check
/// (`popup_menu_open`) only sees `#32768` menus, not a plugin's own Qt menus.
static MENU_OPEN: AtomicBool = AtomicBool::new(false);

pub struct WindowsBackend;

impl WindowsBackend {
    pub fn new() -> Self {
        Self
    }

    /// Files a combination for the keyboard hook once `RegisterHotKey` has answered, and makes
    /// sure the hook is there to match it. `granted` is that answer: a refused combination is
    /// not filed (`hotkey_hook::file_if_granted`), and nothing else happens for it.
    ///
    /// **When the hook is installed, and what that costs.** With the first granted hotkey —
    /// at once, while modules load if that is when it comes — and for the rest of the session,
    /// the same lifetime captured keys give it. Not taken down again when the last hotkey goes:
    /// an overlay releases and registers its control hotkeys on every focus move, and an idle
    /// hook costs microseconds per keystroke. In the module manager the host's own reload key
    /// is registered at start, so there the hook is present from the start whatever the
    /// modules do. It runs on a thread of its own that does nothing else
    /// (`keyboard_hook_thread`), so a busy pump no longer holds up the keyboard: the price of a
    /// long callback is that the hotkey's own callback waits for it, not that the user's typing
    /// does.
    fn file_for_hook(&self, id: i32, vk: u32, mods: u32, granted: bool) {
        // Read before the lock, so the hook never waits for the system call.
        let repeat_ms = granted.then(repeat_threshold_now);
        let filed = {
            let mut t = locked(&HOOK_HOTKEYS);
            let filed = hotkey_hook::file_if_granted(&mut t, granted, id, vk, mods);
            if let (true, Some(t), Some(ms)) = (filed, t.as_mut(), repeat_ms) {
                t.set_repeat_ms(ms);
            }
            filed
        };
        if !filed {
            return;
        }
        if !HOOK_HOTKEYS_PRESENT.swap(true, Ordering::Relaxed) {
            // Room for a burst of presses, so a push from inside the hook rarely allocates.
            locked(&HOTKEY_QUEUE).reserve(32);
            crate::logging::line(
                "keys",
                "registered hotkeys are matched in the keyboard hook as well from now on, so a \
                 program that switches hotkeys off while it is in front does not silence them; \
                 RegisterHotKey still claims each one and delivers it whenever the hook does not",
            );
        }
        if let Err(e) = self.watch_keys() {
            // Not fatal: the registrations stand, and WM_HOTKEY delivers the keys as it always
            // did. Said once; the next grant simply tries again.
            static SAID: AtomicBool = AtomicBool::new(false);
            if !SAID.swap(true, Ordering::Relaxed) {
                crate::logging::line(
                    "keys",
                    &format!(
                        "the keyboard hook is not available ({e}); hotkeys stay on RegisterHotKey \
                         alone"
                    ),
                );
            }
        }
    }
}

/// The auto-repeat threshold for the keyboard settings in force now — see
/// `hotkey_hook::repeat_threshold` — and a line in the log when they changed it. Two
/// `SystemParametersInfoW` calls, microseconds, on the pump; asked with every grant, because
/// the settings can change at any time and an overlay grants on every focus move.
fn repeat_threshold_now() -> u32 {
    let (delay, filter_keys) = unsafe {
        let mut setting: i32 = 0;
        let delay = (SystemParametersInfoW(
            SPI_GETKEYBOARDDELAY,
            0,
            &mut setting as *mut i32 as *mut core::ffi::c_void,
            0,
        ) != 0)
            .then_some(setting.max(0) as u32);
        let mut fk: FILTERKEYS = std::mem::zeroed();
        fk.cbSize = std::mem::size_of::<FILTERKEYS>() as u32;
        let filter_keys = (SystemParametersInfoW(
            SPI_GETFILTERKEYS,
            fk.cbSize,
            &mut fk as *mut FILTERKEYS as *mut core::ffi::c_void,
            0,
        ) != 0
            && fk.dwFlags & FKF_FILTERKEYSON != 0)
            .then_some((fk.iDelayMSec, fk.iRepeatMSec));
        (delay, filter_keys)
    };
    let ms = hotkey_hook::repeat_threshold(delay, filter_keys);
    if REPEAT_MS_IN_FORCE.swap(ms, Ordering::Relaxed) != ms {
        crate::logging::line(
            "keys",
            &format!(
                "a key-down within {ms} ms of the same key's last one, with no key-up between, \
                 now counts as auto-repeat for the hotkeys the hook matches (keyboard delay \
                 setting {delay:?}, FilterKeys delay and repeat {filter_keys:?} ms)"
            ),
        );
    }
    ms
}

/// Stateless GDI screen-region capture (BitBlt → GetDIBits → RGBA, top-down). Free-
/// standing (uses no `self`) so the async image worker can call it off the main thread
/// without holding the non-`Send` `Rc<dyn Backend>`; `WindowsBackend::capture` and
/// `capture_fn` both route through it. GDI screen reads are thread-safe; the ~1-frame
/// DWM-compositor cost then lands on the worker, not the event loop.
///
/// The standard source, unchanged, and still the fallback of the duplication path. Visible
/// to `dxgi.rs` for its live tests, which compare the two.
pub(super) fn capture_screen(x: i32, y: i32, w: i32, h: i32) -> Option<CapturedImage> {
    if w <= 0 || h <= 0 {
        return None;
    }
    // The buffer length in usize, and refused when it does not fit.
    //
    // It was `(w * h * 4) as usize`, computed in i32 — so a region a caller passes in (Lua can
    // ask for any rectangle) needs only ~23 megapixels to wrap that multiplication negative,
    // and `vec![0u8; negative as usize]` is an allocation of about four exabytes. Nothing
    // downstream could have caught it: the panic happens before any guard sees the result.
    let bytes = match (w as i64)
        .checked_mul(h as i64)
        .and_then(|n| n.checked_mul(4))
        .filter(|n| *n <= isize::MAX as i64)
    {
        Some(n) => n as usize,
        None => return None,
    };
    unsafe {
        let screen_dc = GetDC(std::ptr::null_mut());
        if screen_dc.is_null() {
            return None;
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        if mem_dc.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            return None;
        }
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        if bmp.is_null() {
            DeleteDC(mem_dc);
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            return None;
        }
        let old = SelectObject(mem_dc, bmp);
        // CHECKED, because the alternative is worse than an error: GDI leaves the bitmap as it
        // found it, GetDIBits then dutifully copies a buffer of zeros, and the caller receives a
        // perfectly black picture that every downstream guard accepts as a genuine capture. An
        // overlay that reads pixels for a living must be able to tell "the screen is dark" from
        // "the screen was never read".
        let blitted = BitBlt(mem_dc, 0, 0, w, h, screen_dc, x, y, SRCCOPY) != 0;

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB as u32;

        let mut buf = vec![0u8; bytes];
        let lines = GetDIBits(
            mem_dc,
            bmp,
            0,
            h as u32,
            buf.as_mut_ptr() as *mut core::ffi::c_void,
            &mut bmi,
            DIB_RGB_COLORS,
        );

        SelectObject(mem_dc, old);
        DeleteObject(bmp);
        DeleteDC(mem_dc);
        ReleaseDC(std::ptr::null_mut(), screen_dc);

        if !blitted || lines != h {
            return None;
        }

        // GDI returns BGRA; swap to RGBA. Alpha is forced opaque in the same pass, as the macOS
        // capture does. A BitBlt of the screen measured on the development machine gave 255
        // for all 20,000 pixels read, but nothing in GDI promises it, and a capture with alpha
        // 0 is dangerous twice over: `host.screen.save` writes an invisible PNG, and a template
        // built from it (`host.screen.template{ capture = … }`) would be all wildcards,
        // matching everywhere.
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
        Some(CapturedImage {
            w: w as u32,
            h: h as u32,
            rgba: buf,
        })
    }
}

/// Every region from `src`, one answer each, in order — the one place that decides between
/// the two ways of reading the screen.
///
/// Standard is `capture_screen` per region, exactly as before this existed. Duplication sends
/// every region in one request (one GPU sync for all of them) and, when it cannot answer,
/// either reads the standard way or fails each region with the reason, as the module's
/// `fallback` says. The error is a sentence that begins with [`CAPTURE_FAILED`]; the OCR
/// binding answers instead of raising only for the one that begins with
/// [`DUPLICATION_UNANSWERED`] (see `capture_source::answers_instead_of_raising`).
fn capture_all(
    regions: &[(i32, i32, i32, i32)],
    src: CaptureSource,
    caller: Caller,
) -> Vec<Result<CapturedImage, String>> {
    let CaptureSource::Duplication { or_standard } = src else {
        // Exactly the read every module has always had.
        return regions
            .iter()
            .map(|&(x, y, w, h)| capture_screen(x, y, w, h).ok_or_else(|| CAPTURE_FAILED.to_string()))
            .collect();
    };
    match duplicate(regions, caller) {
        Ok(images) => images,
        Err(why) => {
            dxgi::note_fallback(why, or_standard);
            regions
                .iter()
                .map(|r| if or_standard { standard_instead(r) } else { Err(unanswered(why)) })
                .collect()
        }
    }
}

/// The standard read of one region, standing in for duplication.
fn standard_instead(&(x, y, w, h): &(i32, i32, i32, i32)) -> Result<CapturedImage, String> {
    // Too large for the duplication path is too large for its fallback too: GDI is protected
    // from a 14 GB allocation only by its bitmap creation happening to fail first. (A module
    // that reads the standard way keeps exactly the read it always had, without this limit.)
    if w > 0 && h > 0 && !dxgi::fits(w, h) {
        return Err(format!("{CAPTURE_FAILED}: {}", Fallback::TooLarge.describe()));
    }
    capture_screen(x, y, w, h).ok_or_else(|| CAPTURE_FAILED.to_string())
}

/// The error a read gets when duplication could not answer and the module forbade the
/// fallback. It begins with [`DUPLICATION_UNANSWERED`], which is how the OCR binding tells it
/// from every other failure — a degenerate or oversized region still raises.
fn unanswered(why: Fallback) -> String {
    format!("{DUPLICATION_UNANSWERED} — {}", why.describe())
}

/// Every region through desktop duplication in ONE request — or the reason nothing came back.
fn duplicate(
    regions: &[(i32, i32, i32, i32)],
    caller: Caller,
) -> Result<Vec<Result<CapturedImage, String>>, Fallback> {
    duplicate_with(regions, |rects| dxgi::capture(rects, caller))
}

/// [`duplicate`] with the engine call passed in, so the mapping can be tested.
///
/// A degenerate region fails on its own, as `capture_screen` fails it, and so does one too
/// large to allocate for; neither reaches the engine or spoils the rest of the batch. The
/// engine gets the others in order, and its answers go back to the places they were asked
/// from.
fn duplicate_with(
    regions: &[(i32, i32, i32, i32)],
    read: impl FnOnce(&[(i32, i32, i32, i32)]) -> Result<Vec<CapturedImage>, Fallback>,
) -> Result<Vec<Result<CapturedImage, String>>, Fallback> {
    let mut out: Vec<Result<CapturedImage, String>> = regions
        .iter()
        .map(|&(_, _, w, h)| {
            Err(if w > 0 && h > 0 {
                format!("{CAPTURE_FAILED}: {}", Fallback::TooLarge.describe())
            } else {
                CAPTURE_FAILED.to_string()
            })
        })
        .collect();
    let asked: Vec<usize> =
        (0..regions.len()).filter(|&i| dxgi::fits(regions[i].2, regions[i].3)).collect();
    if asked.is_empty() {
        return Ok(out);
    }
    let rects: Vec<(i32, i32, i32, i32)> = asked.iter().map(|&i| regions[i]).collect();
    let images = read(&rects)?;
    for (i, img) in asked.into_iter().zip(images) {
        out[i] = Ok(img);
    }
    Ok(out)
}

/// The image worker's capture routine (`Backend::capture_fn`): a plain `fn`, so it is `Send`,
/// and the duplication state it reaches is the engine thread's, not this backend's.
fn capture_on_worker(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Option<CapturedImage>> {
    capture_all(regions, src, Caller::Worker).into_iter().map(Result::ok).collect()
}

// ── host.ocr.read's two threads ─────────────────────────────────────────────────────────────

/// The capture stage of `host.ocr.read` (`OcrWorker::capture`), on its own thread: every region
/// of one read from one moment, and roughly how many bytes that is.
fn ocr_capture(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> (OcrShot, usize) {
    let shot = capture_for_read(regions, src);
    let bytes = shot.iter().map(|r| r.as_ref().map_or(0, |c| c.rgba.len())).sum();
    (shot, bytes)
}

/// The pixels for a read. Desktop duplication takes every region as a piece of one frame in one
/// request, as `recognizeMany` does; the standard source photographs the bounding box when that
/// is not wasteful (`ocr::plan`) and each region on its own otherwise, or when the box came back
/// clipped at a screen edge.
fn capture_for_read(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> OcrShot {
    if let CaptureSource::Duplication { or_standard } = src {
        match duplicate(regions, Caller::Worker) {
            Ok(caps) => return caps,
            Err(why) => {
                dxgi::note_fallback(why, or_standard);
                if !or_standard {
                    return regions.iter().map(|_| Err(unanswered(why))).collect();
                }
            }
        }
    }
    let one = |&(x, y, w, h): &(i32, i32, i32, i32)| -> Result<CapturedImage, String> {
        capture_screen(x, y, w, h).ok_or_else(|| CAPTURE_FAILED.to_string())
    };
    let rects: Vec<Rect> = regions.iter().copied().map(Rect::from_tuple).collect();
    match plan::capture_plan(&rects) {
        Plan::Each => regions.iter().map(one).collect(),
        Plan::BoundingBox(b) => match capture_screen(b.x, b.y, b.w, b.h) {
            // A capture of a different size than asked for was clipped at a screen edge, and
            // every offset into it would point somewhere else.
            Some(big) if big.w as i32 == b.w && big.h as i32 == b.h => rects
                .iter()
                .zip(regions)
                .map(|(r, t)| match plan::offset_in(r, &b) {
                    Some((ox, oy)) if !r.is_empty() => crop(&big, ox, oy, r.w, r.h)
                        .ok_or_else(|| "region outside the captured area".to_string()),
                    _ => one(t),
                })
                .collect(),
            _ => regions.iter().map(one).collect(),
        },
    }
}

/// The recognise stage (`OcrWorker::recognise`): each piece through exactly what `recognize`
/// runs — the small-text crop, the blank guard, `Windows.Media.Ocr`, and for a small region the
/// neural recogniser beside it, started at the same moment and used only when the system engine
/// reads nothing. One region at a time through `Recognise::each`, so a closing application
/// starts no further region, and so no further neural recognition either.
fn ocr_recognise(
    shot: &OcrShot,
    _regions: &[(i32, i32, i32, i32)],
    ctx: &Recognise,
) -> Vec<Result<OcrText, String>> {
    ctx.each(shot.iter(), |piece| match piece {
        Ok(img) => recognize_image(img, ctx.lang),
        Err(e) => Err(e.clone()),
    })
}

/// The OCR languages installed on this machine, and the user's own list, as Windows spells them.
/// On the recognise thread, whose apartment `ensure_winrt` set up.
fn ocr_languages() -> crate::ocr::lang::Languages {
    use windows::Media::Ocr::OcrEngine;
    use windows::System::UserProfile::GlobalizationPreferences;
    ensure_winrt();
    let available: Vec<String> = match OcrEngine::AvailableRecognizerLanguages() {
        Ok(list) => list
            .into_iter()
            .filter_map(|l| l.LanguageTag().ok().map(|t| t.to_string()))
            .collect(),
        Err(e) => {
            crate::logging::line("ocr", &format!("the OCR languages could not be listed: {e}"));
            Vec::new()
        }
    };
    let preferred: Vec<String> = match GlobalizationPreferences::Languages() {
        Ok(list) => list.into_iter().map(|t| t.to_string()).collect(),
        Err(e) => {
            crate::logging::line("ocr", &format!("the user's languages could not be read: {e}"));
            Vec::new()
        }
    };
    crate::ocr::lang::Languages { fast: available.clone(), available, preferred }
}

/// WinRT's multithreaded apartment, entered once per thread and never left.
///
/// Per thread, not once per process: the `Once` this replaces initialised whichever thread
/// happened to recognise first and no other, and with two OCR threads beside the event loop that
/// is no longer one thread. Never uninitialised — an engine released after its thread left the
/// apartment is the fault that would buy. A thread already in a single-threaded apartment
/// (wxWidgets puts the event loop in one) answers `RPC_E_CHANGED_MODE` and keeps it; that is
/// logged once, because which apartment the event loop really has has never been measured.
fn ensure_winrt() {
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};
    thread_local! {
        static DONE: Cell<bool> = const { Cell::new(false) };
    }
    static SAID: AtomicBool = AtomicBool::new(false);
    if DONE.with(|d| d.replace(true)) {
        return;
    }
    if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
        if !SAID.swap(true, Ordering::Relaxed) {
            let name = std::thread::current().name().unwrap_or("unnamed").to_string();
            crate::logging::line(
                "ocr",
                &format!(
                    "the '{name}' thread keeps the COM apartment it already had ({e}); text \
                     recognition runs in it"
                ),
            );
        }
    }
}

/// A pixel through `src`: `GetPixel` for the standard source, a 1x1 region of the duplicated
/// picture otherwise. `None` only when duplication could not answer and the module forbade
/// the fallback.
fn pixel_through(x: i32, y: i32, src: CaptureSource) -> Option<(u8, u8, u8)> {
    let CaptureSource::Duplication { or_standard } = src else {
        return Some(pixel_gdi(x, y));
    };
    // A point on no monitor has no picture in either path. `GetPixel` says CLR_INVALID there,
    // which `colorref_rgb` turns into black, the same black a region read gives off the
    // desktop — so the answer is the standard one, and the engine is not started for it.
    if off_every_monitor(x, y) {
        return Some(pixel_gdi(x, y));
    }
    // Its fallback is `GetPixel`, not a 1x1 blit, so a module that allows the standard way
    // gets exactly the answer it got before.
    match dxgi::capture(&[(x, y, 1, 1)], Caller::Pump) {
        Ok(images) => images.first().map(|c| (c.rgba[0], c.rgba[1], c.rgba[2])),
        Err(why) => {
            dxgi::note_fallback(why, or_standard);
            or_standard.then(|| pixel_gdi(x, y))
        }
    }
}

/// Whether `(x, y)` lies on no monitor at all.
fn off_every_monitor(x: i32, y: i32) -> bool {
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONULL};
    unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL) }.is_null()
}

/// The foreground window's client area in screen pixels, for the first-read comparison.
fn foreground_client() -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut rc: RECT = std::mem::zeroed();
        let mut origin = POINT { x: 0, y: 0 };
        if GetClientRect(hwnd, &mut rc) == 0 || ClientToScreen(hwnd, &mut origin) == 0 {
            return None;
        }
        Some((origin.x, origin.y, rc.right - rc.left, rc.bottom - rc.top))
    }
}

/// The standard pixel read, unchanged: `GetPixel` on the screen DC.
fn pixel_gdi(x: i32, y: i32) -> (u8, u8, u8) {
    // The SCREEN, deliberately, and with a caveat the callers have to know: this is what is
    // composited at that point, which after a focus change is briefly still the window that
    // used to be there. Measured — with REAPER behind a browser, the same point reads
    // 255,255,255 from the screen and 99,99,99 from PrintWindow(PW_RENDERFULLCONTENT) on the
    // window itself. A probe that runs at the moment an overlay activates can therefore read
    // the previous window. Rendering the window instead would be immune, and is not done
    // here because it renders the WHOLE window per call; the callers re-ask instead.
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let c = GetPixel(dc, x, y); // COLORREF = 0x00BBGGRR
        ReleaseDC(std::ptr::null_mut(), dc);
        colorref_rgb(c)
    }
}

/// A COLORREF (0x00BBGGRR) as (r, g, b). CLR_INVALID, what `GetPixel` answers off the
/// desktop, comes out as black: that is what every region read gives there (BitBlt and the
/// duplication canvas alike), and a point should not read white where the region around it
/// reads black.
fn colorref_rgb(c: u32) -> (u8, u8, u8) {
    const CLR_INVALID: u32 = 0xFFFF_FFFF;
    if c == CLR_INVALID {
        return (0, 0, 0);
    }
    ((c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8)
}

/// What `DllGetVersion` fills in. Declared here because `windows-sys` does not carry it:
/// the function is not exported for linking, it is fetched by name at run time.
#[repr(C)]
#[derive(Default)]
struct DllVersionInfo {
    cb_size: u32,
    major: u32,
    minor: u32,
    build: u32,
    platform_id: u32,
}

/// Which common controls this process actually got, which is not a cosmetic question.
///
/// The application manifest asks for Common Controls **6**, and wxWidgets checks: below
/// that it warns and falls back to the pre-XP controls. Those are not merely uglier — the
/// v5 tree has no checkbox support at all, so the module list would lose the very thing it
/// exists for, and the older controls answer accessibility questions worse across the
/// board. For a screen-reader user that is the difference between a usable window and one
/// that reads as a blank.
///
/// `LoadLibraryW` rather than a version resource read, because it resolves through the
/// same activation context wxWidgets goes through: this answers "what will this process
/// get", not "what is installed".
fn common_controls() -> String {
    let dll: Vec<u16> = "comctl32.dll".encode_utf16().chain(std::iter::once(0)).collect();
    let h = unsafe { LoadLibraryW(dll.as_ptr()) };
    if h.is_null() {
        return "comctl32.dll could not be loaded at all".to_string();
    }
    // SAFETY: the name is a NUL-terminated ASCII literal, and the signature is the
    // documented one for DllGetVersion.
    let f = unsafe { GetProcAddress(h, c"DllGetVersion".as_ptr() as *const u8) };
    let Some(f) = f else {
        return "version unknown (comctl32.dll has no DllGetVersion, so it predates v4.71)"
            .to_string();
    };
    let get: unsafe extern "system" fn(*mut DllVersionInfo) -> i32 =
        unsafe { std::mem::transmute(f) };
    let mut info =
        DllVersionInfo { cb_size: std::mem::size_of::<DllVersionInfo>() as u32, ..Default::default() };
    let hr = unsafe { get(&mut info) };
    if hr < 0 {
        return format!("version unknown (DllGetVersion failed, 0x{hr:08x})");
    }
    let v = format!("{}.{} (build {})", info.major, info.minor, info.build);
    if info.major >= 6 {
        format!("{v} — the manifest took")
    } else {
        format!(
 "{v} — THE MANIFEST DID NOT TAKE. wxWidgets will say so and fall back to the pre-XP controls: no checkboxes in the module list, and everything reads worse to a screen reader"
        )
    }
}

impl Backend for WindowsBackend {
    fn environment(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let (w, h) = self.screen_size();
        out.push(("display".to_string(), format!("{w}x{h} px (primary)")));
        // Whether we are DPI-aware decides whether every coordinate in every module is
        // real or scaled behind our back — the manifest asks for per-monitor v2, and this
        // is the line that proves it took.
        let dpi = unsafe { GetDpiForSystem() };
        out.push((
            "system dpi".to_string(),
            format!("{dpi} ({}%)", (dpi as f32 / 96.0 * 100.0).round() as i32),
        ));
        out.push(("common controls".to_string(), common_controls()));
        // No "screen reader" line here any more. It asked whether `nvdaControllerClient64`
        // or `SAAPI64` was loaded in this process, which was a fair proxy only while Tolk
        // loaded them on demand — with speech going through prism nothing loads either, so
        // the line reported "none detected (speech falls back to SAPI)" with NVDA running
        // and prism speaking through it. The speech layer logs the real answer a moment
        // later, and it is a better answer: which backend WILL speak, rather than which DLL
        // happens to be in memory.
        out
    }

    fn enumerate_windows(&self) -> Vec<WinInfo> {
        let mut hwnds: Vec<isize> = Vec::new();
        unsafe {
            EnumWindows(Some(enum_proc), &mut hwnds as *mut Vec<isize> as LPARAM);
        }
        hwnds.into_iter().filter_map(|h| window_info(h, true)).collect()
    }

    fn active_window(&self) -> Option<WinInfo> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }
        window_info(hwnd as isize, false)
    }

    fn focus_window(&self, id: isize) -> bool {
        let hwnd = id as HWND;
        if hwnd.is_null() {
            return false;
        }
        unsafe {
            // Restored first: a minimised window can be made foreground and stay invisible,
            // which for somebody who cannot see the screen is the worst of both answers.
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd) != 0
        }
    }

    /// Compared at the TOP LEVEL, because an overlay's origin is often a child.
    ///
    /// An embedded plug-in is a control inside its host's window, and the point under the
    /// pointer resolves to whichever child is drawn there — usually a different one. The
    /// question worth asking is not "is this the same window" but "does this point belong to
    /// the same application window", so both sides go up to their root first.
    fn window_owns_point(&self, hwnd: isize, x: i32, y: i32) -> Option<bool> {
        unsafe {
            let at = WindowFromPoint(POINT { x, y });
            if at.is_null() {
                return None; // nothing there to compare against, which is not an accusation
            }
            let theirs = GetAncestor(at, GA_ROOT);
            let ours = GetAncestor(hwnd as HWND, GA_ROOT);
            if theirs.is_null() || ours.is_null() {
                return None;
            }
            Some(theirs == ours)
        }
    }

    fn window_controls(&self, hwnd_val: isize) -> Vec<ControlInfo> {
        let mut hwnds: Vec<isize> = Vec::new();
        unsafe {
            EnumChildWindows(
                hwnd_val as HWND,
                Some(enum_proc),
                &mut hwnds as *mut Vec<isize> as LPARAM,
            );
        }
        hwnds.into_iter().filter_map(control_info).collect()
    }

    fn window_focus_chain(&self) -> Vec<ControlInfo> {
        unsafe {
            let fg = GetForegroundWindow();
            if fg.is_null() {
                return Vec::new();
            }
            let tid = GetWindowThreadProcessId(fg, std::ptr::null_mut());
            let mut gti: GUITHREADINFO = std::mem::zeroed();
            gti.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
            let focused = if GetGUIThreadInfo(tid, &mut gti) != 0 && !gti.hwndFocus.is_null() {
                gti.hwndFocus
            } else {
                fg
            };
            // Walk from the focused control up to the top-level window.
            let mut hwnds: Vec<isize> = Vec::new();
            let mut h = focused;
            while !h.is_null() {
                hwnds.push(h as isize);
                if h == fg {
                    break;
                }
                let parent = GetAncestor(h, GA_PARENT);
                if parent.is_null() || parent == h {
                    break;
                }
                h = parent;
            }
            hwnds.into_iter().filter_map(control_info).collect()
        }
    }

    fn element_find(&self, hwnd: isize, name: &str, control_type: i32) -> bool {
        super::uia::element_find(hwnd, name, control_type)
    }

    fn element_find_any(&self, hwnd: isize, names: &[String], types: &[i32]) -> Option<usize> {
        super::uia::element_find_any(hwnd, names, types)
    }

    fn element_locate(&self, hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)> {
        super::uia::element_locate(hwnd, name, control_type)
    }
    fn element_locate_via(
        &self,
        hwnd: isize,
        via_name: &str,
        via_type: i32,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        super::uia::element_locate_via(hwnd, via_name, via_type, name, control_type)
    }

    fn element_plugin_locate(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        super::uia::element_plugin_locate(hwnd, container_name, name, control_type)
    }

    fn element_dump(&self, hwnd: isize) -> Vec<DumpNode> {
        super::uia::element_dump(hwnd)
    }

    fn element_raw_dump(&self, hwnd: isize) -> Vec<DumpNode> {
        super::uia::element_raw_dump(hwnd)
    }

    fn element_state_probe(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)> {
        super::uia::element_state_probe(hwnd, container_name, name, control_type)
    }

    fn element_class_nav_point(
        &self,
        hwnd: isize,
        class_substr: &str,
        ctype: i32,
        child: i32,
        sibling: i32,
    ) -> Option<(i32, i32)> {
        super::uia::element_class_nav_point(hwnd, class_substr, ctype, child, sibling)
    }

    fn element_focus_step(&self, hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)> {
        super::uia::element_focus_step(hwnd, direction)
    }

    fn screen_size(&self) -> (i32, i32) {
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
    }

    fn pixel(&self, x: i32, y: i32, src: CaptureSource) -> Option<(u8, u8, u8)> {
        pixel_through(x, y, src)
    }

    fn compare_capture_sources(&self, who: &str, read: (i32, i32, i32, i32)) -> bool {
        let Some((region, window)) = dxgi::comparison_region(read, foreground_client()) else {
            return true; // nothing this path could read, so nothing to compare
        };
        // Its own duplication read, so the module's read is not held up or changed by it —
        // and when duplication cannot answer yet (still opening, backing off), a later read
        // asks again.
        let dup = match dxgi::capture(&[region], Caller::Pump) {
            Ok(mut images) => match images.pop() {
                Some(img) => img,
                None => return true,
            },
            Err(_) => return false,
        };
        let (x, y, w, h) = region;
        let what = if window {
            format!("the foreground window's {w}x{h} client area at {x},{y} (the read itself was {}x{})", read.2, read.3)
        } else {
            format!("the {w}x{h} region it read at {x},{y}")
        };
        dxgi::log_comparison(who, &what, &dup, capture_screen(x, y, w, h).as_ref());
        true
    }

    fn capture(&self, x: i32, y: i32, w: i32, h: i32, src: CaptureSource) -> Option<CapturedImage> {
        capture_all(&[(x, y, w, h)], src, Caller::Pump).pop().and_then(Result::ok)
    }

    fn capture_fn(&self) -> CaptureFn {
        capture_on_worker
    }

    fn ocr_worker(&self) -> OcrWorker {
        OcrWorker {
            present: true,
            init_thread: |_: OcrThread| ensure_winrt(),
            capture: ocr_capture,
            recognise: ocr_recognise,
            languages: ocr_languages,
        }
    }

    fn ocr(
        &self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        lang: Option<&str>,
        src: CaptureSource,
    ) -> Result<OcrText, String> {
        let cap = capture_all(&[(x, y, w, h)], src, Caller::Pump)
            .pop()
            .unwrap_or_else(|| Err(CAPTURE_FAILED.to_string()))?;
        recognize_image(&cap, lang)
    }

    /// Several regions, one capture. See the trait for why the recognitions stay separate.
    fn ocr_regions(
        &self,
        regions: &[(i32, i32, i32, i32)],
        lang: Option<&str>,
        src: CaptureSource,
    ) -> Vec<Result<OcrText, String>> {
        // Duplication reads N small rectangles out of ONE acquired frame, one GPU sync for all
        // of them — its cost grows with area, unlike GDI's, so the bounding box below would be
        // the expensive way round. When it cannot answer and the module allows it, this falls
        // through to the standard path, bounding box and all.
        if let CaptureSource::Duplication { or_standard } = src {
            match duplicate(regions, Caller::Pump) {
                Ok(caps) => {
                    return caps.into_iter().map(|c| c.and_then(|img| recognize_image(&img, lang))).collect();
                }
                Err(why) => {
                    dxgi::note_fallback(why, or_standard);
                    if !or_standard {
                        return regions.iter().map(|_| Err(unanswered(why))).collect();
                    }
                }
            }
        }
        let one_each = |b: &Self| -> Vec<Result<OcrText, String>> {
            regions
                .iter()
                .map(|(x, y, w, h)| b.ocr(*x, *y, *w, *h, lang, CaptureSource::Standard))
                .collect()
        };
        if regions.len() < 2 || regions.iter().any(|(_, _, w, h)| *w <= 0 || *h <= 0) {
            return one_each(self);
        }
        let x0 = regions.iter().map(|r| r.0).min().unwrap_or(0);
        let y0 = regions.iter().map(|r| r.1).min().unwrap_or(0);
        let x1 = regions.iter().map(|r| r.0 + r.2).max().unwrap_or(0);
        let y1 = regions.iter().map(|r| r.1 + r.3).max().unwrap_or(0);
        let (bw, bh) = (x1 - x0, y1 - y0);
        let big = match self.capture(x0, y0, bw, bh, CaptureSource::Standard) {
            Some(c) => c,
            None => return one_each(self),
        };
        // A capture that came back a different size than asked for was CLIPPED — the region ran
        // off a screen edge — and every offset computed below would then point somewhere else.
        // Falling back to one capture each is slower and right, which is the correct trade for
        // an overlay that speaks what it read.
        if big.w as i32 != bw || big.h as i32 != bh {
            return one_each(self);
        }
        regions
            .iter()
            .map(|(x, y, w, h)| match crop(&big, x - x0, y - y0, *w, *h) {
                Some(sub) => recognize_image(&sub, lang),
                None => Err("region outside the captured area".to_string()),
            })
            .collect()
    }

    fn cursor_pos(&self) -> (i32, i32) {
        unsafe {
            let mut p: POINT = std::mem::zeroed();
            GetCursorPos(&mut p);
            (p.x, p.y)
        }
    }

    fn mouse_move(&self, x: i32, y: i32) {
        unsafe {
            SetCursorPos(x, y);
        }
    }

    fn mouse_click(&self, x: i32, y: i32, button: MouseButton) {
        let (down, up) = button_flags(button);
        unsafe {
            SetCursorPos(x, y);
            send_mouse_event(down, 0);
            send_mouse_event(up, 0);
        }
    }

    /// A drag with actual MOVEMENT in it, spread over time.
    ///
    /// What this used to be — press here, warp there, release — is not a drag as far as a lot
    /// of controls are concerned, and ON:EAR's width slider proved both halves of that.
    ///
    /// It is not movement. `SetCursorPos` puts the pointer somewhere; it does not say the
    /// pointer moved. The button events carry their own position, so for a CLICK the two are
    /// indistinguishable — which is why every click in this project has always worked — but a
    /// control watching for motion while its button is held can miss a warp entirely. The log
    /// showed the cursor arriving at each requested position and the slider sitting still.
    ///
    /// And it has no duration. A control that reads the SPEED of a drag rather than its
    /// distance answers an instantaneous jump with an enormous change: that same slider moved
    /// eighteen per cent for two pixels, where its own geometry says four.
    ///
    /// So: injected moves, interpolated, with a pause between them. The pauses block this
    /// thread for about sixty milliseconds, which is the cost of the gesture being believable
    /// and is only ever paid when somebody deliberately adjusts something. macOS has posted a
    /// real drag event between its press and release all along; this brings Windows level.
    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton) {
        let (down, up) = button_flags(button);
        unsafe {
            SetCursorPos(x1, y1);
            move_injected(x1, y1);
            send_mouse_event(down, 0);
            const STEPS: i32 = 16;
            for i in 1..=STEPS {
                let t = f64::from(i) / f64::from(STEPS);
                move_injected(
                    (f64::from(x1) + f64::from(x2 - x1) * t).round() as i32,
                    (f64::from(y1) + f64::from(y2 - y1) * t).round() as i32,
                );
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            send_mouse_event(up, 0);
        }
    }

    fn mouse_down(&self, x: i32, y: i32, button: MouseButton) {
        let (down, _) = button_flags(button);
        unsafe {
            SetCursorPos(x, y);
            send_mouse_event(down, 0);
        }
    }

    fn mouse_up(&self, x: i32, y: i32, button: MouseButton) {
        let (_, up) = button_flags(button);
        unsafe {
            SetCursorPos(x, y);
            send_mouse_event(up, 0);
        }
    }

    fn mouse_scroll(&self, x: i32, y: i32, delta: i32) {
        unsafe {
            SetCursorPos(x, y);
            send_mouse_event(MOUSEEVENTF_WHEEL, delta); // already in WHEEL_DELTA units
        }
    }

    fn key_post(&self, hwnd: isize, key: &str) -> Result<(), String> {
        let vk = parse_key(key)? as u16;
        // The scan code goes in the lParam because some applications read it rather than the
        // virtual key, and Melodyne decodes keys itself (it imports GetKeyboardState, ToUnicode
        // and MapVirtualKeyW) rather than leaving it to the defaults.
        let scan = unsafe { MapVirtualKeyW(vk as u32, 0) } as isize;
        let lp_down = 1isize | (scan << 16);
        let lp_up = 1isize | (scan << 16) | (1 << 30) | (1 << 31);
        unsafe {
            PostMessageW(hwnd as HWND, WM_KEYDOWN, vk as usize, lp_down);
            PostMessageW(hwnd as HWND, WM_KEYUP, vk as usize, lp_up);
        }
        Ok(())
    }

    fn key_send(&self, combo: &str) -> Result<(), String> {
        // The shared parser, so a spec means here what it means to a hotkey. Its modifiers are
        // roles, which on Windows are the keys of their names.
        let (vk, mask) = super::parse_key_spec(combo)?;
        if mask & super::MASK_TAP != 0 {
            return Err(format!(
                "'{combo}' is a modifier tap, which is something to capture, not a key to send"
            ));
        }
        let mod_vks = modifier_vks(mask);
        let key_vk = vk as u16;
        unsafe {
            for &vk in &mod_vks {
                send_key_event(vk, 0, 0);
            }
            send_key_event(key_vk, 0, 0);
            send_key_event(key_vk, 0, KEYEVENTF_KEYUP);
            for &vk in mod_vks.iter().rev() {
                send_key_event(vk, 0, KEYEVENTF_KEYUP);
            }
        }
        Ok(())
    }

    fn type_text(&self, text: &str) {
        unsafe {
            for u in text.encode_utf16() {
                send_key_event(0, u, KEYEVENTF_UNICODE);
                send_key_event(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
            }
        }
    }

    fn register_hotkey(&self, id: i32, spec: &str) -> Result<(), String> {
        let (mods, vk) = parse_spec(spec)?;
        let hwnd = hotkey_window();
        let granted = unsafe { RegisterHotKey(hwnd, id, mods | MOD_NOREPEAT, vk) } != 0;
        // Granted: matched in the hook from now on as well, so a program that switches
        // registered hotkeys off while it is in front does not switch this one off. Refused:
        // another program holds it — and therefore NOT filed. The hook could take it anyway,
        // which is exactly why it must not: the other program's key would stop working and
        // nothing anywhere would say why. The answer goes in as it came; `file_if_granted`
        // is the rule, and it is tested with a refusal.
        self.file_for_hook(id, vk, mods, granted);
        if !granted {
            return Err(format!(
                "RegisterHotKey failed for '{spec}' (already in use by another app?)"
            ));
        }
        Ok(())
    }

    fn unregister_hotkey(&self, id: i32) {
        let hwnd = HOTKEY_HWND.load(Ordering::Relaxed) as HWND;
        unsafe {
            UnregisterHotKey(hwnd, id);
        }
        // Out of the hook with the registration, whatever the OS answered: an id that is no
        // longer ours must not be matched by the hook for a moment longer than by Windows.
        if let Some(t) = locked(&HOOK_HOTKEYS).as_mut() {
            t.remove(id);
        }
    }

    fn watch_foreground(&self) -> Result<(), String> {
        // Idempotent: one foreground/focus hook pair serves every module (the
        // dispatcher routes events to whichever module has a matching trigger),
        // so a module loaded later — at startup or hot-loaded at runtime — only
        // needs the hooks present, not re-installed. Guard like watch_keys.
        if FG_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        PUMP_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut::<core::ffi::c_void>() as HMODULE,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        if hook.is_null() {
            FG_HOOK_INSTALLED.store(false, Ordering::SeqCst);
            return Err("SetWinEventHook failed".to_string());
        }
        // Also track focus changes within a window — focusing into a plugin
        // embedded in a DAW host doesn't raise a foreground event.
        unsafe {
            SetWinEventHook(
                EVENT_OBJECT_FOCUS,
                EVENT_OBJECT_FOCUS,
                std::ptr::null_mut::<core::ffi::c_void>() as HMODULE,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
        }
        // And the foreground window's TITLE changing. Some windows (e.g. Komplete
        // Kontrol's custom-drawn Preferences dialog) become foreground with an empty
        // title, then set it a beat later with no focus event — so the foreground
        // re-check runs before the window is matchable and it stays undetected until
        // the next foreground change (Alt+Tab). A name-change on the foreground window
        // re-checks once the title is finally there.
        unsafe {
            SetWinEventHook(
                EVENT_OBJECT_NAMECHANGE,
                EVENT_OBJECT_NAMECHANGE,
                std::ptr::null_mut::<core::ffi::c_void>() as HMODULE,
                Some(win_event_proc),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            );
        }
        // Intentionally leak the hook handles: they live for the process lifetime.
        Ok(())
    }

    fn set_captured_keys(&self, keys: &[(u32, u8)]) {
        // Built, and the old set freed, outside the lock, so the hook waits for a swap and
        // nothing more.
        let keys = keys.to_vec();
        let old = std::mem::replace(&mut *locked(&CAPTURED_KEYS), keys);
        drop(old);
    }

    fn set_key_scope(&self, to_foreground: bool) {
        let hwnd = if to_foreground {
            let fg = unsafe { GetForegroundWindow() };
            fg as isize
        } else {
            0
        };
        KEY_SCOPE.store(hwnd, Ordering::Relaxed);
    }

    fn set_menu_open(&self, open: bool) {
        MENU_OPEN.store(open, Ordering::Relaxed);
    }

    fn modifiers_down(&self) -> bool {
        unsafe {
            let down = |k: u16| (GetAsyncKeyState(k as i32) as u16 & 0x8000) != 0;
            down(VK_MENU) || down(VK_CONTROL) || down(VK_SHIFT) || down(VK_LWIN) || down(VK_RWIN)
        }
    }

    /// `ToUnicodeEx` against the layout of the thread that owns the foreground window — the one
    /// the user is typing into, which need not be ours: layouts are per thread.
    ///
    /// Flag bit 2 leaves the kernel's keyboard state alone (Windows 10 1607 and later). Without
    /// it, a dead key asked about here would be left waiting in the dead-key buffer and would
    /// combine with whatever the user types next.
    fn layout_char(&self, vk: u32, mask: u8) -> Option<(String, bool)> {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
            GetKeyboardLayout, MapVirtualKeyExW, ToUnicodeEx, MAPVK_VK_TO_VSC, VK_LCONTROL,
            VK_LMENU, VK_LSHIFT,
        };
        const DOWN: u8 = 0x80;
        unsafe {
            let fg = GetForegroundWindow();
            let thread = if fg.is_null() {
                0
            } else {
                GetWindowThreadProcessId(fg, std::ptr::null_mut())
            };
            let hkl = GetKeyboardLayout(thread);
            let mut state = [0u8; 256];
            if mask & super::MASK_SHIFT != 0 {
                state[VK_SHIFT as usize] = DOWN;
                state[VK_LSHIFT as usize] = DOWN;
            }
            if mask & super::MASK_CTRL != 0 {
                state[VK_CONTROL as usize] = DOWN;
                state[VK_LCONTROL as usize] = DOWN;
            }
            if mask & super::MASK_ALT != 0 {
                state[VK_MENU as usize] = DOWN;
                state[VK_LMENU as usize] = DOWN;
            }
            let scan = MapVirtualKeyExW(vk, MAPVK_VK_TO_VSC, hkl);
            let mut buf = [0u16; 8];
            let n = ToUnicodeEx(vk, scan, state.as_ptr(), buf.as_mut_ptr(), buf.len() as i32, 0x4, hkl);
            let (text, dead) = match n {
                // A dead key: the buffer holds its spacing form.
                n if n < 0 => (String::from_utf16_lossy(&buf[..1]), true),
                0 => return None,
                n => (String::from_utf16_lossy(&buf[..(n as usize).min(buf.len())]), false),
            };
            // Ctrl with a letter "types" a control character; that is not a character anybody
            // loses.
            if text.chars().all(char::is_control) {
                return None;
            }
            Some((text, dead))
        }
    }

    fn native_menu_open(&self) -> bool {
        popup_menu_open()
    }

    fn take_menu_pass_through(&self) -> Vec<(u32, u8)> {
        MENU_PASS.lock().map(|mut m| std::mem::take(&mut *m)).unwrap_or_default()
    }

    /// Visible top-level windows of a process — including a `#32768` popup menu, which is a
    /// top-level window owned by the thread that opened it, and a toolkit's self-drawn
    /// popup, which is usually a tool window of its own. `EnumWindows` is local and
    /// microseconds, so this can be asked on the menu watch's tick.
    fn windows_of(&self, pid: u32) -> Vec<crate::backend::WindowSpot> {
        let mut hwnds: Vec<isize> = Vec::new();
        unsafe {
            EnumWindows(Some(enum_proc), &mut hwnds as *mut Vec<isize> as LPARAM);
        }
        let mut out = Vec::new();
        for h in hwnds {
            let hwnd = h as HWND;
            unsafe {
                if IsWindowVisible(hwnd) == 0 {
                    continue;
                }
                let mut owner: u32 = 0;
                GetWindowThreadProcessId(hwnd, &mut owner);
                if owner != pid {
                    continue;
                }
                let mut cbuf = [0u16; 256];
                let cn = GetClassNameW(hwnd, cbuf.as_mut_ptr(), cbuf.len() as i32);
                let class = String::from_utf16_lossy(&cbuf[..cn.max(0) as usize]);
                // A tooltip is a top-level window of the process too, and the consumer
                // treats any newcomer during a hold as the menu: the click that opened the
                // menu leaves the pointer on the control, and a toolkit that shows its tip
                // anyway would confirm a menu that is not there and end the hold when the
                // tip goes. Win32's class and WinForms' wrapper of it both carry the name;
                // Qt's tooltip window class carries "ToolTip".
                if class.contains("tooltips_class32") || class.contains("ToolTip") {
                    continue;
                }
                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetWindowRect(hwnd, &mut rect);
                out.push(crate::backend::WindowSpot {
                    id: h as u64,
                    layer: 0,
                    class,
                    x: rect.left,
                    y: rect.top,
                    w: rect.right - rect.left,
                    h: rect.bottom - rect.top,
                });
            }
        }
        out
    }

    /// Starts the keyboard hook's thread and waits for it to say whether the hook is in —
    /// see `keyboard_hook_thread`. Idempotent: every capture and every granted hotkey asks.
    fn watch_keys(&self) -> Result<(), String> {
        if KEY_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
            return Ok(()); // already installed
        }
        // The thread the hook wakes when it has queued something: this one, the pump.
        PUMP_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);
        let result = match std::thread::Builder::new()
            .name("keyboard-hook".to_string())
            .spawn(move || keyboard_hook_thread(ready_tx))
        {
            Err(e) => Err(format!("could not start the keyboard hook's thread: {e}")),
            // It answers once, straight after SetWindowsHookExW, which does not block; a
            // sender dropped unanswered means the thread ended first, and is a failure too.
            Ok(_) => ready_rx.recv().unwrap_or_else(|_| {
                Err("the keyboard hook's thread ended before it answered".to_string())
            }),
        };
        match &result {
            Ok(()) => crate::logging::line(
                "keys",
                "the keyboard hook is installed, on a thread of its own that does nothing but \
                 answer it, so a busy main thread does not hold up the keyboard",
            ),
            Err(_) => KEY_HOOK_INSTALLED.store(false, Ordering::SeqCst),
        }
        result
    }

    /// The headless loop: wait for input, but never for longer than one tick.
    ///
    /// It used to sit in a blocking `GetMessageW`, and in a headless session there is no
    /// window of ours and nobody typing at it, so no message ever arrives. Everything hanging
    /// off the tick was therefore dead rather than merely uncalled: `host.timer`, async image
    /// results, and the speech pump that says a refused line through the other path and looks
    /// for a screen reader that came back.
    ///
    /// This was one of TWO defects between a headless module and its timers, and they were
    /// separated by experiment rather than by reading, because the first hid the second
    /// completely. `Manager::run` would not enter this loop at all for a module whose only
    /// reason to be alive was a timer, so the first measurement — zero ticks in ten idle
    /// seconds — proved nothing about the loop. With that guard fixed and this wait still
    /// blocking, the loop was entered and no timer fired for four seconds. With both fixed,
    /// `every(500)` fires twice a second, which is the number that was wanted.
    ///
    /// The shape here is the one the macOS loop already had (`CFRunLoop::run_in_mode` with a
    /// 0.015 timeout) and the one the GUI path gets from its 15 ms wx timer. Three copies of
    /// the same interval, which is the point rather than an accident: if the two headless
    /// paths and the GUI path do not deliver on the same cadence, headless stops being a fair
    /// test of the rest — and headless is the only way module Luau is ever run on a Mac here.
    fn run_event_loop(&self, events: &mut dyn HostEvents) -> Result<(), String> {
        /// Matches `timer.start(15, …)` in `gui.rs` and the macOS run-loop interval.
        const TICK_MS: u32 = 15;

        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            // Returns as soon as anything is queued, or when the interval is up. Its return
            // value is deliberately ignored: an empty queue and a timeout are the same
            // instruction here — drain what is there, then tick.
            unsafe {
                MsgWaitForMultipleObjects(0, std::ptr::null(), 0, TICK_MS, QS_ALLINPUT);
            }
            // Drain, without blocking on an empty queue. `PeekMessageW` is what makes the
            // timeout above meaningful; `GetMessageW` would give the block straight back.
            loop {
                if unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } == 0 {
                    break;
                }
                if msg.message == WM_QUIT {
                    return Ok(());
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            // Hotkeys reach our window proc during dispatch; the WinEvent hooks queued
            // foreground events on this thread, the keyboard hook its keys and hotkeys from
            // its own. Drain them all.
            self.pump_pending(events);
            events.on_tick();
        }
    }

    fn pump_pending(&self, events: &mut dyn HostEvents) {
        let hotkeys: Vec<(i32, Route)> = locked(&HOTKEY_QUEUE).drain(..).collect();
        for (id, route) in hotkeys {
            if settle_hotkey(id, route) {
                events.on_hotkey(id);
            }
        }
        let pending: Vec<isize> = FOREGROUND_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        let mut unmatched_fg = false;
        for hwnd in pending {
            match window_info(hwnd, true) {
                Some(win) => events.on_window_activate(win),
                // Foreground, but not matchable yet — remember to re-check shortly.
                None => unmatched_fg = true,
            }
        }
        let pending_keys: Vec<(u32, u8)> = std::mem::take(&mut *locked(&KEY_QUEUE));
        for (vk, mask) in pending_keys {
            events.on_key(vk, mask);
        }
        // Game controllers, from the hub their own thread feeds (never a thread-local: that
        // thread is not this one). After the activations, so a press is handled against the
        // window that is in front now.
        super::gamepad::drain_into(events);
        if FOCUS_DIRTY.with(|f| f.replace(false)) {
            events.on_focus_change();
        }

        // A window that became foreground before it was matchable (empty title / not yet
        // shown) fires no further event once it settles — e.g. Komplete Kontrol's
        // Preferences dialog on a re-open sets its title/visibility a beat after becoming
        // foreground. Queue a few delayed re-checks so it is picked up then. Coalesced
        // (only armed when nothing is pending) so window churn can't pile these up.
        if unmatched_fg {
            DELAYED_RECHECK.with(|d| {
                let mut v = d.borrow_mut();
                if v.is_empty() {
                    let now = std::time::Instant::now();
                    v.push(now + std::time::Duration::from_millis(200));
                    v.push(now + std::time::Duration::from_millis(500));
                    v.push(now + std::time::Duration::from_millis(1000));
                }
            });
        }
        let now = std::time::Instant::now();
        let fire = DELAYED_RECHECK.with(|d| {
            let mut v = d.borrow_mut();
            let fire = v.iter().any(|&t| now >= t);
            v.retain(|&t| now < t);
            fire
        });
        if fire {
            events.on_focus_change();
        }
    }
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let vec = &mut *(lparam as *mut Vec<isize>);
    vec.push(hwnd as isize);
    1 // TRUE — keep enumerating
}

/// Class name + screen geometry of a child control (visible only), for embedded
/// plugin detection.
fn control_info(hwnd_val: isize) -> Option<ControlInfo> {
    let hwnd = hwnd_val as HWND;
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return None;
        }
        let mut cbuf = [0u16; 256];
        let cn = GetClassNameW(hwnd, cbuf.as_mut_ptr(), cbuf.len() as i32);
        let class = String::from_utf16_lossy(&cbuf[..cn.max(0) as usize]);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetWindowRect(hwnd, &mut rect);
        let mut client = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut client);
        let mut crect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(hwnd, &mut crect);
        Some(ControlInfo {
            hwnd: hwnd_val,
            class,
            x: rect.left,
            y: rect.top,
            w: rect.right - rect.left,
            h: rect.bottom - rect.top,
            client_x: client.x,
            client_y: client.y,
            client_w: crect.right - crect.left,
            client_h: crect.bottom - crect.top,
        })
    }
}

/// Lazily creates a hidden message-only window that owns our global hotkeys.
/// Registering hotkeys against a real window (rather than NULL) means
/// `WM_HOTKEY` is dispatched to [`hotkey_wndproc`] by whichever loop pumps the
/// thread — including wxWidgets' — instead of being a NULL-hwnd thread message
/// the GUI loop would silently discard.
fn hotkey_window() -> HWND {
    let existing = HOTKEY_HWND.load(Ordering::Relaxed);
    if existing != 0 {
        return existing as HWND;
    }
    unsafe {
        let hmod = GetModuleHandleW(std::ptr::null());
        let class_name: Vec<u16> = "AutomationPlatformHotkeys\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(hotkey_wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hmod,
            hIcon: std::ptr::null_mut(),
            hCursor: std::ptr::null_mut(),
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        RegisterClassW(&wc); // ignored if the class is already registered
        let hwnd = CreateWindowExW(
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
        );
        HOTKEY_HWND.store(hwnd as isize, Ordering::Relaxed);
        hwnd
    }
}

unsafe extern "system" fn hotkey_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_HOTKEY {
        // Stamped, so the pump can tell a press the keyboard hook already dispatched from a new
        // one: `GetMessageTime` is on the same tick clock as the hook's key events.
        let time = GetMessageTime() as u32;
        locked(&HOTKEY_QUEUE).push((wparam as i32, Route::Os { time }));
        return 0;
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    // OBJID_WINDOW == 0, CHILDID_SELF == 0: only the top-level foreground window.
    if event == EVENT_SYSTEM_FOREGROUND && id_object == 0 && id_child == 0 && !hwnd.is_null() {
        let v = hwnd as isize;
        FOREGROUND_QUEUE.with(|q| q.borrow_mut().push(v));
        // Also re-check via the focus path: window_info() drops a window with an empty
        // title, so the queued activate can be lost for a window that isn't matchable
        // yet — a fresh active() re-check via the focus dispatch doesn't depend on that.
        FOCUS_DIRTY.with(|f| f.set(true));
        // Wake the event loop so it drains the queue even without a real message.
        let tid = PUMP_THREAD.load(Ordering::Relaxed);
        if tid != 0 {
            PostThreadMessageW(tid, WM_NULL, 0, 0);
        }
    } else if event == EVENT_OBJECT_FOCUS {
        // Focus moved (possibly within the same top-level window); coalesce and
        // let the loop re-check via the focus chain.
        FOCUS_DIRTY.with(|f| f.set(true));
        let tid = PUMP_THREAD.load(Ordering::Relaxed);
        if tid != 0 {
            PostThreadMessageW(tid, WM_NULL, 0, 0);
        }
    } else if event == EVENT_OBJECT_NAMECHANGE && id_object == 0 && id_child == 0 {
        // The foreground window's title just changed — re-check, so a window that
        // became foreground before it had a (matchable) title is caught the moment it
        // gets one. Filtered to OBJID_WINDOW + the current foreground window, so the
        // (frequent) name changes of other windows / child objects cost only this test.
        if !hwnd.is_null() && hwnd == GetForegroundWindow() {
            FOCUS_DIRTY.with(|f| f.set(true));
            let tid = PUMP_THREAD.load(Ordering::Relaxed);
            if tid != 0 {
                PostThreadMessageW(tid, WM_NULL, 0, 0);
            }
        }
    }
}

/// Captured keys the hook let through because a menu was open. A mutex rather than a
/// thread-local because the hook runs on its own thread and the reader is the pump.
static MENU_PASS: Mutex<Vec<(u32, u8)>> = Mutex::new(Vec::new());
const MENU_PASS_MAX: usize = 32;

fn note_menu_pass(vk: u32, mask: u8) {
    if let Ok(mut m) = MENU_PASS.lock() {
        if m.len() < MENU_PASS_MAX {
            m.push((vk, mask));
        }
    }
}

/// True while a standard Win32 popup menu (class "#32768") is open — ReaHotkey's
/// `WinExist("ahk_class #32768")` check. While a menu is up, captured navigation
/// keys must pass through to it: its window is owned by the plugin, so the
/// foreground window doesn't change and a foreground check alone can't see it.
/// Is the FOREGROUND application currently in a menu?
///
/// This used to ask `FindWindowW("#32768")` — is there a visible popup-menu window ANYWHERE —
/// and that is a system-wide question answering a local one. Any other application with a menu
/// open (or a cached menu window that happens to be visible) made it say yes, and once the
/// overlay believes a menu is up it gives up ALL of its hotkeys so they can reach that menu.
/// Live consequence: Alt+V stopped reaching the overlay and REAPER's View menu got it instead.
///
/// `GetGUIThreadInfo(0, …)` reports the FOREGROUND thread, and its flags say whether that
/// thread is in menu mode — which is exactly the question. Scoped to the application the user
/// is actually in, so another program's menu cannot silently disarm us.
fn popup_menu_open() -> bool {
    unsafe {
        let mut gti: GUITHREADINFO = std::mem::zeroed();
        gti.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(0, &mut gti) == 0 {
            return false;
        }
        gti.flags & (GUI_INMENUMODE | GUI_POPUPMENUMODE | GUI_SYSTEMMENUMODE) != 0
    }
}

/// The keyboard hook's thread: installs the hook, says whether that worked, and then does
/// nothing but answer it.
///
/// **Why a thread of its own.** Windows calls a low-level hook on the thread that installed
/// it, by sending that thread a message, and every keystroke on the machine waits until the
/// hook has answered — up to `LowLevelHooksTimeout` (Microsoft documents no default; since
/// Windows 10 1709 anything above one second counts as one second), after which the key goes
/// on without it, a captured key reaches the application, and the hook may be removed without
/// notice. On the pump that wait was every OCR call, every long callback, every speech engine
/// opening — measured pump iterations of 400 and 729 ms, during which all typing on the
/// machine stood still, in every session once registered hotkeys were matched here as well.
/// Here nothing else runs, so the hook answers in microseconds whatever the pump is doing, and
/// what it queues waits for the pump instead of the keyboard waiting for it.
///
/// At the highest normal thread priority, so that our own recognition threads, busy on every
/// core, cannot keep it from being scheduled: it runs for microseconds per keystroke, which
/// takes nothing from anybody.
///
/// It never ends, and never needs to: Windows removes the hook with the process.
fn keyboard_hook_thread(ready: std::sync::mpsc::SyncSender<Result<(), String>>) {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        let hmod = GetModuleHandleW(std::ptr::null());
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), hmod, 0);
        if hook.is_null() {
            let error = GetLastError();
            let _ = ready.send(Err(format!("SetWindowsHookExW(WH_KEYBOARD_LL) failed (error {error})")));
            return;
        }
        let _ = ready.send(Ok(()));
        drop(ready);
        // Nothing is ever posted to this thread. GetMessageW is where Windows delivers the
        // hook's calls, as sent messages, and it returns only for a posted one.
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// Wakes the pump after the hook queued something for it.
fn wake_pump() {
    let tid = PUMP_THREAD.load(Ordering::Relaxed);
    if tid != 0 {
        unsafe {
            PostThreadMessageW(tid, WM_NULL, 0, 0);
        }
    }
}

/// Runs on the hook's own thread (`keyboard_hook_thread`) for every key event on the machine.
unsafe extern "system" fn ll_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode;
        let msg = wparam as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        if is_down || is_up {
            // How late this call is. Nearly always 0: the thread does nothing else. When the
            // machine was too loaded to schedule it, the events queued meanwhile arrive in
            // order, late, and each is judged as of its own moment — see `hotkey_hook`.
            let late =
                hotkey_hook::lateness(GetTickCount(), kb.time, kb.flags & LLKHF_INJECTED != 0);
            // GetAsyncKeyState, NOT GetKeyState: a low-level hook runs on the thread
            // that installed it, and GetKeyState reports that THREAD's view of the
            // keyboard — updated only by the messages it retrieves. Our thread never
            // receives the keystrokes (they belong to the focused application), so the
            // modifiers read as up and every combination collapsed to mask 0. Unmodified
            // keys like Tab worked, which is why this stayed hidden.
            //
            // And only for a call on time. The asynchronous state is the keyboard as it is
            // NOW, which for a late call is later than its key event: plain v typed during a
            // stall, with Alt held by the time the hook got to it, would read as Alt+V. A late
            // call is judged by the modifiers this hook saw go by before its key.
            let down = |k: u16| (GetAsyncKeyState(k as i32) as u16 & 0x8000) != 0;
            let mask = HOOK_MODS.with(|m| {
                let mut mods = m.get();
                let mask = hotkey_hook::event_mask(&mut mods, late, || {
                    let mut mask: u8 = 0;
                    if down(VK_SHIFT) {
                        mask |= 1;
                    }
                    if down(VK_CONTROL) {
                        mask |= 2;
                    }
                    if down(VK_MENU) {
                        mask |= 4;
                    }
                    if down(0x5B) || down(0x5C) {
                        mask |= 8; // VK_LWIN / VK_RWIN
                    }
                    mask
                });
                mods.on_key(vk, is_down);
                m.set(mods);
                mask
            });
            // A MODIFIER TAP: pressed and released with nothing in between.
            //
            // Melodyne is why. It swallows Alt, F10 and even the WM_SYSCOMMAND that would
            // normally open its menu bar, so the only way in is a click — and the way every
            // other application offers that is a bare Alt. Recognising one needs the hook,
            // because "was another key pressed while Alt was down" is not a question anything
            // else in the system can answer after the fact.
            //
            // Never suppressed, and that is the whole difficulty: Alt has to keep working as a
            // modifier, so this dispatches on the way past rather than intercepting.
            // Insert, numpad 0 and CapsLock: NVDA's modifier on the desktop and laptop
            // layouts respectively, and JAWS's. Recorded on the way past, whatever anyone
            // downstream does with it.
            if vk == 0x2D || vk == 0x60 || vk == 0x14 {
                SCREEN_READER_MOD_DOWN.store(is_down, Ordering::Relaxed);
            }
            // Which modifier is this, as `key_spec` names it? The hook reports the SIDE that
            // was pressed — left Alt is 0xA4, right Alt 0xA5 — while a spec says "Alt tap"
            // and resolves to the generic 0x12, so a capture has to catch either side without
            // the caller having to say which.
            //
            // All four modifiers, not only Alt. Alt was the one this was built for (it is the
            // key that opens a menu bar on its own, so an application already treats a bare
            // one as meaningful), and the other three were left unarmed — which made
            // `host.keys.capture("Ctrl tap", …)` a registration that parses, returns a token
            // and can never fire. Silence is the worst of the three possible answers.
            let generic = match vk {
                0x10 | 0xA0 | 0xA1 => Some(0x10u32), // Shift
                0x11 | 0xA2 | 0xA3 => Some(0x11),    // Ctrl
                0x12 | 0xA4 | 0xA5 => Some(0x12),    // Alt
                0x5B | 0x5C => Some(0x5B),           // Win
                _ => None,
            };
            if is_down && generic.is_none() {
                // Any ordinary key ends a pending tap: "pressed and released with nothing in
                // between" is the whole definition, and this is the "in between". CapsLock
                // arrives here too, which is right — it is a screen reader's modifier, not a
                // tap.
                TAP_ARMED.store(0, Ordering::Relaxed);
            } else if is_down {
                // Arm only FROM REST, and only for this key.
                //
                // Auto-repeat while held must not re-arm, or a long hold followed by a release
                // would read as a tap. And a second, different modifier pressed on top means
                // the user is building a combination — Alt held, then Ctrl — so the arm is
                // dropped rather than handed over. Without that, releasing Alt while Ctrl was
                // still down fired a tap the user never made.
                let armed = TAP_ARMED.load(Ordering::Relaxed);
                if armed == 0 {
                    TAP_ARMED.store(vk as i32, Ordering::Relaxed);
                } else if armed != vk as i32 {
                    TAP_ARMED.store(0, Ordering::Relaxed);
                }
            } else if let Some(generic) = generic {
                let armed = TAP_ARMED.swap(0, Ordering::Relaxed);
                if armed == vk as i32 {
                    let wanted = locked(&CAPTURED_KEYS)
                        .iter()
                        .any(|&(v, m)| v == generic && m == crate::backend::MASK_TAP);
                    let scope = KEY_SCOPE.load(Ordering::Relaxed);
                    let in_scope = scope == 0 || GetForegroundWindow() as isize == scope;
                    if wanted && in_scope && !popup_menu_open() && !MENU_OPEN.load(Ordering::Relaxed)
                    {
                        locked(&KEY_QUEUE).push((generic, crate::backend::MASK_TAP));
                        wake_pump();
                    }
                }
            }
            // The two keys that END a menu, remembered whenever the runtime says a plugin
            // menu is open — captured or not. Escape is captured by no overlay and Return
            // only while the focused control wants it, so a record kept inside the
            // captured-key branch below never held the Escape that cancelled a menu, and the
            // runtime's hold ran its full course after it. Noted only; never suppressed.
            if is_down && mask == 0 && (vk == 0x0D || vk == 0x1B) && MENU_OPEN.load(Ordering::Relaxed) {
                let scope = KEY_SCOPE.load(Ordering::Relaxed);
                if scope == 0 || GetForegroundWindow() as isize == scope {
                    note_menu_pass(vk, mask);
                }
            }
            // The hotkeys' record of which keys are held (`hotkey_hook::Table`) is kept for every
            // key that is not a modifier, whatever happens to it below: the captured keys'
            // branch can return before the hotkeys' is reached, and a key it took whose up
            // the record never saw would make the next real press of that key read as a
            // repeat — swallowed, without a callback. Every key-up is recorded here, and a
            // key-down the captures take is recorded where they take it.
            let hotkeys_filed =
                generic.is_none() && HOOK_HOTKEYS_PRESENT.load(Ordering::Relaxed);
            if hotkeys_filed && is_up {
                if let Some(t) = locked(&HOOK_HOTKEYS).as_mut() {
                    t.on_up(vk);
                }
            }
            // Match only the exact combo, so "Tab" (mask 0) leaves Alt+Tab alone.
            let matched = locked(&CAPTURED_KEYS).iter().any(|&(v, m)| v == vk && m == mask);
            if matched {
                // A keystroke made with a SCREEN READER'S own modifier held is addressed to
                // the screen reader, whatever this overlay has claimed.
                //
                // NVDA's modifier is Insert on the desktop layout and CapsLock on the laptop
                // one; JAWS uses Insert too. Not one of them is a Windows modifier, so none
                // reaches the mask above — and this hook saw NVDA+Space as a bare Space with
                // an empty mask, an exact match for any overlay that had claimed "Space".
                // Every overlay in this project claims Space. So we ate it.
                //
                // What that does to the person at the keyboard is out of all proportion to the
                // size of this check. NVDA+Space is what leaves focus mode; with it swallowed,
                // the reported experience was "NVDA hung, and every letter seemed to be
                // typed" — which is exactly what focus mode you cannot leave feels like. The
                // same applied to NVDA+Tab, NVDA+Enter and the arrows: the commands for
                // asking a screen reader what is going on were the commands we were deaf to.
                //
                // Only ever passes MORE through, and only while one of those keys is
                // physically down, so it cannot take a key away from an overlay: nobody
                // navigates one holding Insert.
                let screen_reader_held = SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed)
                    || down(0x2D)
                    || down(0x60)
                    || down(0x14);
                let owes_up = !is_down && SCREEN_READER_PASSED.with(|s| s.borrow().contains(&vk));
                if screen_reader_held || owes_up {
                    SCREEN_READER_PASSED.with(|s| {
                        let mut v = s.borrow_mut();
                        if is_down {
                            if !v.contains(&vk) {
                                v.push(vk);
                            }
                        } else {
                            v.retain(|&k| k != vk);
                        }
                    });
                    if hotkeys_filed && is_down {
                        note_captured_down(kb, vk, mask, late, true);
                    }
                    return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
                }
                // Intercept a captured nav key only while the overlay should own
                // it (ReaHotkey's GetContext): the scoped window is foreground AND
                // no popup menu is open. A control that opened a #32768 menu must
                // let Tab/Enter/arrows reach the menu natively — the menu window is
                // owned by the plugin, so the foreground doesn't change.
                let scope = KEY_SCOPE.load(Ordering::Relaxed);
                let in_scope = scope == 0 || GetForegroundWindow() as isize == scope;
                if in_scope && !popup_menu_open() && !MENU_OPEN.load(Ordering::Relaxed) {
                    if is_down {
                        locked(&KEY_QUEUE).push((vk, mask));
                        wake_pump();
                        if hotkeys_filed {
                            note_captured_down(kb, vk, mask, late, false);
                        }
                    }
                    return 1; // suppress the matched combo (down + up)
                } else if in_scope && is_down {
                    // Let through because a menu is open. Remembered, not discarded: the
                    // overlay runtime asks for these to learn that Return or Escape reached
                    // the menu, which where nothing can see the menu itself is the best
                    // available word that it is closing. See `take_menu_pass_through`.
                    note_menu_pass(vk, mask);
                }
            }
            // A registered hotkey, matched here as well as by RegisterHotKey — see
            // `hotkey_hook`. After the captured keys, so a capture of the same combination
            // still wins exactly as it did when the hook swallowed it before RegisterHotKey
            // could see it. Never a modifier: a hotkey's key is an ordinary key. A key-up has
            // been recorded above and always goes through.
            if hotkeys_filed && is_down && hook_hotkey(kb, vk, mask, late) {
                return 1;
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// A key-down the captured keys' branch took, or let through for a screen reader, entered in
/// the hotkeys' record of held keys (`Table::note_down`) before that branch returns. When it
/// was let through (`passed`) and the combination is also a granted hotkey, `RegisterHotKey`
/// may deliver it next, and the pump is told to expect that rather than to explain it.
fn note_captured_down(kb: &KBDLLHOOKSTRUCT, vk: u32, mask: u8, late: u32, passed: bool) {
    let expected = {
        let mut t = locked(&HOOK_HOTKEYS);
        let Some(t) = t.as_mut() else { return };
        t.note_down(vk, kb.time);
        if passed {
            t.lookup(vk, mask)
        } else {
            NO_ID
        }
    };
    if expected != NO_ID {
        locked(&HOTKEY_QUEUE).push((expected, Route::Passed { time: kb.time, late }));
    }
}

/// The hook's half of a registered hotkey, for a key-down: whether to swallow it. Queues the
/// press for the pump when it is one, and a note when it lets a granted combination through.
///
/// The rules the captured keys follow, and which of them apply here:
///
/// - **A screen reader's modifier held** (Insert, numpad zero, Caps Lock): let through, as a
///   captured key is. RegisterHotKey then decides, exactly as it did before the hook matched
///   anything — Windows ignores those keys when it matches a hotkey, and so the press arrives
///   as `WM_HOTKEY` if the screen reader passes it on.
/// - **The capture scope and the menu fall-through do not apply**, deliberately. They exist so
///   that a captured key can be shared with the application's own UI; a registered hotkey was
///   never shared — it fired whatever window was in front and whatever menu was open — and
///   the overlay runtime gives its control hotkeys back itself while a menu is open. Applying
///   them here would change when a hotkey fires, which is not what matching it here is for.
/// - **The tap form** is untouched: a hotkey's key-down is an ordinary key, which has already
///   dropped a pending tap above, exactly as it did on its way to RegisterHotKey.
///
/// Key-ups are recorded by the caller and always let through, as RegisterHotKey lets them
/// through to the application. Auto-repeat is swallowed without a second dispatch, as
/// `MOD_NOREPEAT` does.
unsafe fn hook_hotkey(kb: &KBDLLHOOKSTRUCT, vk: u32, mask: u8, late: u32) -> bool {
    enum Act {
        Fire(i32),
        Repeat,
        Pass(i32),
        Nothing,
    }
    let act = {
        let mut t = locked(&HOOK_HOTKEYS);
        let Some(t) = t.as_mut() else { return false };
        let down = t.on_down(vk, mask, kb.time);
        if matches!(down, Down::Miss) {
            return false;
        }
        // Asked only for a granted combination: three GetAsyncKeyState calls are cheap, but
        // not free, and every other key goes past without them.
        let held = |k: i32| (GetAsyncKeyState(k) as u16 & 0x8000) != 0;
        let screen_reader_held = SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed)
            || held(0x2D)
            || held(0x60)
            || held(0x14);
        match down {
            // Its repeats are let through too, below: the registered path is handling it.
            Down::Fire(id) if screen_reader_held => Act::Pass(id),
            Down::Repeat if screen_reader_held => Act::Nothing,
            Down::Fire(id) => Act::Fire(id),
            Down::Repeat => Act::Repeat,
            // The key was down before the combination was complete: not a press here, and
            // RegisterHotKey may still answer for it.
            Down::Pass(id) => Act::Pass(id),
            Down::Miss => Act::Nothing,
        }
    };
    // No lock is held from here on: the masking key below comes back through this hook.
    match act {
        Act::Fire(id) => {
            locked(&HOTKEY_QUEUE).push((id, Route::Hook { time: kb.time, late }));
            if hotkey_hook::needs_mask_key(mask) {
                send_mask_key();
            }
            wake_pump();
            true
        }
        Act::Repeat => true,
        Act::Pass(id) => {
            // Not a dispatch: a note for the pump that the WM_HOTKEY coming for this press is
            // expected, and needs no explaining. No wake either — the WM_HOTKEY wakes it.
            locked(&HOTKEY_QUEUE).push((id, Route::Passed { time: kb.time, late }));
            false
        }
        Act::Nothing => false,
    }
}

/// Presses and releases [`VK_MASK_KEY`] while the user still holds the hotkey's modifiers.
///
/// Sent from inside the hook, at the moment the hotkey's key is swallowed — not later, when the
/// modifier comes up, which is where AutoHotkey sends it. Then the order is certain: this runs
/// while the modifier is still down, so the mask lands between its press and its release
/// whatever the system does with input injected from inside a hook. No lock is held while it
/// is sent, so the hook's own calls for these two events find nothing held; they match no
/// table and go straight through.
unsafe fn send_mask_key() {
    let mut inputs: [INPUT; 2] = std::mem::zeroed();
    for (input, flags) in inputs.iter_mut().zip([0, KEYEVENTF_KEYUP]) {
        input.r#type = INPUT_KEYBOARD;
        input.Anonymous.ki.wVk = VK_MASK_KEY;
        input.Anonymous.ki.dwFlags = flags;
    }
    SendInput(2, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32);
}

/// The pump's half: whether a hotkey press that reached the queue is dispatched. Says so in
/// the log when a press came the other way from the one expected, because that is the only
/// trace either case leaves.
fn settle_hotkey(id: i32, route: Route) -> bool {
    HOTKEY_DEDUPE.with(|d| {
        let mut d = d.borrow_mut();
        match route {
            Route::Hook { time, late } => {
                if d.hook_press(id, time, late) {
                    return true;
                }
                crate::logging::line(
                    "keys",
                    &format!(
                        "hotkey id {id}: the keyboard hook matched a press {late} ms late that \
                         RegisterHotKey had already delivered; not dispatched a second time"
                    ),
                );
                false
            }
            Route::Passed { time, late } => {
                d.hook_pass(id, time, late);
                false
            }
            Route::Os { time } => match d.os_press(id, time) {
                OsPress::Duplicate { late } => {
                    crate::logging::line(
                        "keys",
                        &format!(
                            "hotkey id {id}: RegisterHotKey delivered a press the keyboard hook \
                             had already dispatched — the hook ran {late} ms late (its thread was \
                             not scheduled in time), so Windows had passed the key on without \
                             waiting; not dispatched a second time"
                        ),
                    );
                    false
                }
                OsPress::Deliver { expected } => {
                    let hooked = !expected
                        && KEY_HOOK_INSTALLED.load(Ordering::Relaxed)
                        && locked(&HOOK_HOTKEYS).as_ref().is_some_and(|t| t.holds(id));
                    // Once per window in front: in front of an elevated window this is every
                    // press, and one line says as much as a hundred.
                    let first_for_window = hooked && {
                        let fg = unsafe { GetForegroundWindow() } as isize;
                        MISS_LOGGED_FOR.with(|w| w.replace(fg)) != fg
                    };
                    if first_for_window {
                        // The hook should have seen this press and did not; every press the
                        // hook lets through on purpose was noted as expected. What is left: an
                        // elevated window in front — a hook of an ordinary process is not
                        // called for input to it, RegisterHotKey is, which is why it stays —
                        // a hook running late, whose own press then follows and is dropped
                        // with a line of its own, or a hook Windows removed after it timed out.
                        crate::logging::line(
                            "keys",
                            &format!(
                                "hotkey id {id} arrived through RegisterHotKey, not through the \
                                 keyboard hook (said once per window in front): an elevated \
                                 window is in front, or the hook ran late (a \"not dispatched a \
                                 second time\" line then follows), or Windows has removed the \
                                 hook after it timed out (captured keys would then have stopped \
                                 too)"
                            ),
                        );
                    }
                    true
                }
            },
        }
    })
}

// `require_title`: drop windows with no title (the default — keeps the window LIST and
// the foreground-event stream free of untitled system windows). The ACTIVE window is
// looked up with `false`, because a focused window is always relevant even when it has
// no title — e.g. Komplete Kontrol's custom-drawn "Save preset" dialog is an untitled
// #32770, and dropping it would make host.window.active() nil so nothing could match it.
fn window_info(hwnd_val: isize, require_title: bool) -> Option<WinInfo> {
    let hwnd = hwnd_val as HWND;
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return None;
        }
        let title_len = GetWindowTextLengthW(hwnd);
        if require_title && title_len <= 0 {
            return None; // skip windows without a title
        }
        let mut buf: Vec<u16> = vec![0; title_len.max(0) as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);

        let mut cbuf = [0u16; 256];
        let cn = GetClassNameW(hwnd, cbuf.as_mut_ptr(), cbuf.len() as i32);
        let class = String::from_utf16_lossy(&cbuf[..cn.max(0) as usize]);

        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let exe = process_exe(pid).unwrap_or_default();

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        GetWindowRect(hwnd, &mut rect);

        // Client-area origin in screen coords (overlay coordinates are relative
        // to the client area, like AutoHotkey's default Client coord mode).
        let mut client = POINT { x: 0, y: 0 };
        ClientToScreen(hwnd, &mut client);
        let mut crect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        GetClientRect(hwnd, &mut crect);

        Some(WinInfo {
            hwnd: hwnd_val,
            title,
            class,
            pid,
            exe,
            // Windows has no such concept; the field exists for the platform that does.
            bundle_id: String::new(),
            x: rect.left,
            y: rect.top,
            w: rect.right - rect.left,
            h: rect.bottom - rect.top,
            client_x: client.x,
            client_y: client.y,
            client_w: crect.right - crect.left,
            client_h: crect.bottom - crect.top,
        })
    }
}

fn process_exe(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        // Return just the file name (e.g. "reaper.exe").
        Some(full.rsplit(['\\', '/']).next().unwrap_or(&full).to_string())
    }
}

/// Parses a spec like "Ctrl+Alt+H" or "Ctrl+Shift+Win+Alt+F6" into Win32 modifier flags +
/// virtual-key code, as `RegisterHotKey` takes them: the shared parser, then its mask as `MOD_*`
/// bits (each role is the key of its name here).
/// There used to be a second parser here, and it had no tap branch — so a tap was refused as
/// an "unknown key", where macOS says what a tap is and where it belongs. `pub(super)` for
/// `hotkey_hook`'s tests, which check it against the mask the hook computes.
pub(super) fn parse_spec(spec: &str) -> Result<(u32, u32), String> {
    let (vk, mask) = super::parse_key_spec(spec)?;
    if mask & super::MASK_TAP != 0 {
        return Err(format!(
            "'{spec}' is a modifier tap, which cannot be a global hotkey — capture it with \
             host.keys instead"
        ));
    }
    let mut mods: u32 = 0;
    for (bit, flag) in [
        (super::MASK_CTRL, MOD_CONTROL),
        (super::MASK_ALT, MOD_ALT),
        (super::MASK_SHIFT, MOD_SHIFT),
        (super::MASK_WIN, MOD_WIN),
    ] {
        if mask & bit != 0 {
            mods |= flag;
        }
    }
    Ok((mods, vk))
}

/// The modifier keys `key_send` holds for `mask`, in the order it presses them: Ctrl, Alt,
/// Shift, Win — the canonical order, not the order the spec was written in, which is gone with
/// the parse; no application has been seen to care. Released in reverse.
fn modifier_vks(mask: u8) -> Vec<u16> {
    [
        (super::MASK_CTRL, 0x11u16),
        (super::MASK_ALT, 0x12),
        (super::MASK_SHIFT, 0x10),
        (super::MASK_WIN, 0x5B),
    ]
    .iter()
    .filter(|(bit, _)| mask & bit != 0)
    .map(|(_, vk)| *vk)
    .collect()
}

/// Maps a friendly key name to a Win32 virtual-key code.
fn parse_key(key: &str) -> Result<u32, String> {
    super::key_to_vk(key).ok_or_else(|| format!("unknown key '{key}'"))
}

/// Saves a captured image next to the executable for OCR debugging
/// (`AUTOMATION_PLATFORM_OCR_DEBUG=1`).
fn save_debug(cap: &CapturedImage, name: &str) {
    if let Some(img) = image::RgbaImage::from_raw(cap.w, cap.h, cap.rgba.clone()) {
        let path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(name)))
            .unwrap_or_else(|| std::path::PathBuf::from(name));
        let _ = img.save(&path);
    }
}

/// Background padding (px) added around the upscaled region. Windows.Media.Ocr is
/// far more reliable when small text isn't flush against the image edge.
const OCR_PAD: u32 = 24;

/// Upscales a captured region (sharp filter) and frames it with a background
/// border — both help Windows.Media.Ocr read small UI text (Tesseract, which
/// ReaHotkey uses, doesn't need this).
fn upscale(cap: &CapturedImage, factor: u32) -> CapturedImage {
    use image::{imageops, ImageBuffer, RgbaImage};
    let img = match RgbaImage::from_raw(cap.w, cap.h, cap.rgba.clone()) {
        Some(i) => i,
        None => {
            return CapturedImage {
                w: cap.w,
                h: cap.h,
                rgba: cap.rgba.clone(),
            }
        }
    };
    let (sw, sh) = (cap.w * factor, cap.h * factor);
    let resized = imageops::resize(&img, sw, sh, imageops::FilterType::Lanczos3);
    let bg = *resized.get_pixel(0, 0);
    let (pw, ph) = (sw + OCR_PAD * 2, sh + OCR_PAD * 2);
    let mut canvas: RgbaImage = ImageBuffer::from_pixel(pw, ph, bg);
    imageops::overlay(&mut canvas, &resized, OCR_PAD as i64, OCR_PAD as i64);
    CapturedImage {
        w: pw,
        h: ph,
        rgba: canvas.into_raw(),
    }
}

/// Content-tight preprocessing result: the processed image plus the parameters
/// needed to map word coordinates back to the original captured region.
struct Tightened {
    img: CapturedImage,
    off_x: u32, // content-crop origin within the capture
    off_y: u32,
    scale: u32, // integer upscale applied to the crop
    pad: u32,   // background border added around the upscaled crop
    /// The content crop's size within the capture — the whole capture when nothing was
    /// cropped. The neural recogniser crops to the same box by the same rule, so this is also
    /// where its answer lies (`OcrText::fallback`).
    cw: u32,
    ch: u32,
    /// Nothing in the region differs from its own background — there is no content here at
    /// all, as opposed to content this step chose not to crop to.
    blank: bool,
}

/// Crops a captured region to its content bounding box (pixels differing from
/// the corner background) and upscales it so small glyphs become large and
/// framed — much more reliable for Windows.Media.Ocr than upscaling the whole
/// padded region (a "poor man's detection" for fixed UI regions with padding).
/// Falls back to a plain upscale when no distinct content is found.
fn tighten(cap: &CapturedImage) -> Tightened {
    use image::{imageops, ImageBuffer, RgbaImage};

    let whole = || Tightened {
        img: upscale(cap, 3),
        off_x: 0,
        off_y: 0,
        scale: 3,
        pad: OCR_PAD,
        cw: cap.w,
        ch: cap.h,
        blank: false,
    };
    if cap.w < 3 || cap.h < 3 {
        return whole();
    }
    let img = match RgbaImage::from_raw(cap.w, cap.h, cap.rgba.clone()) {
        Some(i) => i,
        None => return whole(),
    };

    // Background colour = average of the four corners; content = pixels far from it.
    let at = |x: u32, y: u32| {
        let p = img.get_pixel(x, y).0;
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let cs = [
        at(0, 0),
        at(cap.w - 1, 0),
        at(0, cap.h - 1),
        at(cap.w - 1, cap.h - 1),
    ];
    let bg = [
        (cs[0][0] + cs[1][0] + cs[2][0] + cs[3][0]) / 4.0,
        (cs[0][1] + cs[1][1] + cs[2][1] + cs[3][1]) / 4.0,
        (cs[0][2] + cs[1][2] + cs[2][2] + cs[3][2]) / 4.0,
    ];
    let thr = 55.0f32;
    let (mut x0, mut y0, mut x1, mut y1) = (cap.w, cap.h, 0u32, 0u32);
    let mut found = false;
    for y in 0..cap.h {
        for x in 0..cap.w {
            let p = img.get_pixel(x, y).0;
            let d = ((p[0] as f32 - bg[0]).powi(2)
                + (p[1] as f32 - bg[1]).powi(2)
                + (p[2] as f32 - bg[2]).powi(2))
            .sqrt();
            if d > thr {
                found = true;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if !found {
        // Every pixel is its own background. Said out loud rather than handed on, because
        // what happens downstream to a flat rectangle is that a recogniser answers anyway:
        // an empty search box and five tiles whose captions are clipped away all came back
        // as "cYanmaGtaYellowb", the same string from six different places. A module cannot
        // tell that from a reading, and neither can the person listening to it.
        let mut w = whole();
        w.blank = true;
        return w;
    }

    let m = 3i32;
    let x0 = (x0 as i32 - m).max(0) as u32;
    let y0 = (y0 as i32 - m).max(0) as u32;
    let x1 = (x1 as i32 + m).min(cap.w as i32 - 1) as u32;
    let y1 = (y1 as i32 + m).min(cap.h as i32 - 1) as u32;
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    // Content found, so this is not the blank case whatever else happens below.
    let cropped = imageops::crop_imm(&img, x0, y0, cw, ch).to_image();

    // Upscale so the content is ~64px tall (Windows.Media.Ocr likes big glyphs).
    let scale = (64 / ch.max(1)).clamp(3, 10);
    let resized = imageops::resize(&cropped, cw * scale, ch * scale, imageops::FilterType::Lanczos3);
    let bg_px = *resized.get_pixel(0, 0);
    let (pw, ph) = (cw * scale + OCR_PAD * 2, ch * scale + OCR_PAD * 2);
    let mut canvas: RgbaImage = ImageBuffer::from_pixel(pw, ph, bg_px);
    imageops::overlay(&mut canvas, &resized, OCR_PAD as i64, OCR_PAD as i64);

    Tightened {
        img: CapturedImage {
            w: pw,
            h: ph,
            rgba: canvas.into_raw(),
        },
        off_x: x0,
        off_y: y0,
        scale,
        pad: OCR_PAD,
        cw,
        ch,
        blank: false,
    }
}

/// Runs Windows.Media.Ocr (WinRT) over a captured region: the engine's text, every word in
/// reading order, and the same words grouped into the engine's lines.
fn run_ocr(
    img: &CapturedImage,
    lang: Option<&str>,
) -> windows::core::Result<(String, Vec<OcrWord>, Vec<OcrLine>)> {
    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;

    ensure_winrt();

    // SoftwareBitmap wants BGRA, opaque. `capture_screen` and duplication both deliver alpha
    // 255 now, off the desktop included, but this also receives crops of a capture and the
    // `tighten` (upscaled) copies, which are built elsewhere — so the alpha is forced here too
    // rather than trusted.
    let mut bgra = img.rgba.clone();
    for px in bgra.chunks_exact_mut(4) {
        px.swap(0, 2);
        px[3] = 255;
    }

    let buffer = CryptographicBuffer::CreateFromByteArray(&bgra)?;
    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        img.w as i32,
        img.h as i32,
    )?;

    let engine = match lang {
        Some(code) => {
            OcrEngine::TryCreateFromLanguage(&Language::CreateLanguage(&HSTRING::from(code))?)?
        }
        None => OcrEngine::TryCreateFromUserProfileLanguages()?,
    };

    let result = engine.RecognizeAsync(&bitmap)?.get()?;

    let text = result.Text()?.to_string();
    let mut words = Vec::new();
    let mut lines = Vec::new();
    for line in result.Lines()? {
        let mut line_words = Vec::new();
        for word in line.Words()? {
            let r = word.BoundingRect()?;
            line_words.push(OcrWord {
                text: word.Text()?.to_string(),
                x: r.X as i32,
                y: r.Y as i32,
                w: r.Width as i32,
                h: r.Height as i32,
            });
        }
        words.extend(line_words.iter().cloned());
        lines.push(OcrLine { text: line.Text()?.to_string(), words: line_words });
    }
    Ok((text, words, lines))
}

fn button_flags(button: MouseButton) -> (u32, u32) {
    match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    }
}

/// A pointer move the system reports AS a move, in absolute screen coordinates.
///
/// SendInput wants 0..65535 across the primary screen rather than pixels, so this is the
/// conversion. Scaled against the primary display only, like `screen_size` elsewhere in this
/// file — a drag on a second monitor is a thing to fix when somebody has one to fix it on.
unsafe fn move_injected(x: i32, y: i32) {
    let w = GetSystemMetrics(SM_CXSCREEN).max(2);
    let h = GetSystemMetrics(SM_CYSCREEN).max(2);
    let mut input: INPUT = std::mem::zeroed();
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi.dx = (i64::from(x) * 65535 / i64::from(w - 1)) as i32;
    input.Anonymous.mi.dy = (i64::from(y) * 65535 / i64::from(h - 1)) as i32;
    input.Anonymous.mi.dwFlags = MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE;
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

unsafe fn send_mouse_event(flags: u32, data: i32) {
    let mut input: INPUT = std::mem::zeroed();
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi.dwFlags = flags;
    input.Anonymous.mi.mouseData = data as u32; // wheel delta reinterpreted
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

unsafe fn send_key_event(vk: u16, scan: u16, flags: u32) {
    let mut input: INPUT = std::mem::zeroed();
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;
    input.Anonymous.ki.wScan = scan;
    input.Anonymous.ki.dwFlags = flags;
    SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
}

/// Crop a sub-image out of a capture. `x`/`y` are relative to the capture's own top-left.
fn crop(src: &CapturedImage, x: i32, y: i32, w: i32, h: i32) -> Option<CapturedImage> {
    if x < 0 || y < 0 || w <= 0 || h <= 0 {
        return None;
    }
    let (sx, sy, sw, sh) = (x as usize, y as usize, w as usize, h as usize);
    let (bw, bh) = (src.w as usize, src.h as usize);
    if sx + sw > bw || sy + sh > bh || src.rgba.len() < bw * bh * 4 {
        return None;
    }
    let mut rgba = Vec::with_capacity(sw * sh * 4);
    for row in 0..sh {
        let o = ((sy + row) * bw + sx) * 4;
        rgba.extend_from_slice(&src.rgba[o..o + sw * 4]);
    }
    Some(CapturedImage { w: w as u32, h: h as u32, rgba })
}

/// Everything the OCR path does AFTER the pixels are in hand: crop-and-upscale for small
/// regions, Windows.Media.Ocr, the concurrent neural fallback, and mapping word boxes back.
///
/// Split out of `ocr` so that a caller with SEVERAL regions can pay for one capture instead of
/// one per region — measured at ~17 ms against 4-6 ms for the recognition itself, so the
/// capture was two thirds of a two-region read.
fn recognize_image(cap: &CapturedImage, lang: Option<&str>) -> Result<OcrText, String> {
        let debug = crate::appcfg::ocr_debug();
        if debug {
            save_debug(cap, "ocr-debug-raw.png");
        }
        // Windows.Media.Ocr struggles with small UI text, especially lone glyphs.
        // For a small region, crop to the actual content and upscale *that* so the
        // glyphs are large (far better than scaling the whole padded region, which
        // leaves tiny digits tiny). Large regions (e.g. a whole window) are left
        // alone so multi-word layout and speed are preserved.
        let small = crate::ocr::policy::is_small(cap.w as i32, cap.h as i32);

        // Run the neural recognizer (PaddleOCR via ONNX Runtime) CONCURRENTLY for
        // small regions. Its result is used only when Windows.Media.Ocr comes back
        // empty — notably a lone digit, which WinRT rejects regardless of size — so
        // that case costs about max(winrt, paddle) instead of their sum. When WinRT
        // succeeds the background thread just finishes unused (negligible at human
        // focus rates). WinRT stays the trusted primary and the only multi-word path.
        //
        // Counted from before the spawn until the thread is done, because a thread nobody
        // joins can still be inside ONNX Runtime when the application exits; `run` waits for
        // the count to reach zero (see `paddle_ocr::InFlight`). Not started at all once that
        // wait has begun: `start` answers `None` then, and the system engine reads alone.
        let paddle = small
            .then(|| {
                let running = super::paddle_ocr::IN_FLIGHT.start()?;
                let probe = CapturedImage {
                    w: cap.w,
                    h: cap.h,
                    rgba: cap.rgba.clone(),
                };
                Some(std::thread::spawn(move || {
                    let _running = running;
                    super::paddle_ocr::recognize(&probe)
                }))
            })
            .flatten();

        let t_tight = std::time::Instant::now();
        let tight = if small { Some(tighten(cap)) } else { None };
        let tight_ms = t_tight.elapsed().as_secs_f64() * 1000.0;
        let img: &CapturedImage = tight.as_ref().map(|t| &t.img).unwrap_or(cap);
        if debug {
            save_debug(img, "ocr-debug.png");
        }
        // An empty region reads as empty. Nothing else can honestly come out of it, and
        // something else was: the neural fallback, given a rectangle with no content in it,
        // returns a plausible-looking string rather than nothing, and both the primary and
        // the fallback were being asked. Answering here saves the primary recognition on a
        // region there was never anything to read in. It does NOT save the fallback's: that
        // thread was spawned above, before the crop that finds the region blank, so it runs to
        // the end and its answer is dropped with the handle (it stays counted in
        // `paddle_ocr::IN_FLIGHT` until then, like any other it is not waited for).
        if tight.as_ref().is_some_and(|t| t.blank) {
            drop(paddle);
            // `skipped` is how a caller learns this branch was taken: no engine's answer was
            // used, so the empty answer is the guard's and not a reading of anything.
            return Ok(OcrText {
                text: String::new(),
                words: Vec::new(),
                lines: Vec::new(),
                fallback: None,
                skipped: true,
            });
        }
        let t_win = std::time::Instant::now();
        let (mut text, mut words, mut lines) =
            run_ocr(img, lang).map_err(|e| format!("OCR failed: {e}"))?;
        let win_ms = t_win.elapsed().as_secs_f64() * 1000.0;

        let mut used_paddle = false;
        // What the JOIN costs, which is the number that decides whether a read of an empty
        // region is expensive or merely useless.
        //
        // It was never measured. The line below reported WinRT's time and marked the fallback
        // with a bare "+paddle", so "the empty case costs max(winrt, paddle)" was an argument
        // about the code rather than an observation of it — and this project has spent a day
        // learning what those are worth. The two are not the same claim either: the recogniser
        // runs concurrently, so the join costs whatever is LEFT of paddle after WinRT finished,
        // which is zero when paddle was quicker and everything when it was not.
        let mut wait_ms = 0.0;
        if let Some(handle) = paddle {
            if text.trim().is_empty() {
                let t_wait = std::time::Instant::now();
                let got = handle.join().ok().flatten();
                wait_ms = t_wait.elapsed().as_secs_f64() * 1000.0;
                if let Some(t) = got {
                    text = t;
                    words.clear(); // recognition-only fallback returns text without boxes
                    lines.clear();
                    used_paddle = true;
                }
            }
            // else: WinRT won; the paddle thread finishes in the background, counted in
            // `paddle_ocr::IN_FLIGHT` until it does.
        }
        // Behind TRACE as well as the OCR-debug switch. Saving the images is for "was the region
        // right"; the timings answer "where did the time go", which is a different question and
        // was reachable only by also writing PNG files for every read.
        if debug || crate::appcfg::trace() {
            crate::logging::line(
                "ocr",
                &format!(
                    "{}x{} tighten {:.1}ms winrt {:.1}ms{} -> '{}'",
                    cap.w,
                    cap.h,
                    tight_ms,
                    win_ms,
                    if wait_ms > 0.0 {
                        format!(
                            " + waited {wait_ms:.1}ms for paddle ({})",
                            if used_paddle { "which answered" } else { "which had nothing" }
                        )
                    } else {
                        String::new()
                    },
                    text.replace('\n', " ")
                ),
            );
        }
        // Map word coordinates from the processed image back to the captured region.
        if let Some(t) = &tight {
            let (s, pad) = (t.scale as i32, t.pad as i32);
            let back = |word: &mut OcrWord| {
                word.x = (word.x - pad) / s + t.off_x as i32;
                word.y = (word.y - pad) / s + t.off_y as i32;
                word.w /= s;
                word.h /= s;
            };
            words.iter_mut().for_each(back);
            lines.iter_mut().flat_map(|l| l.words.iter_mut()).for_each(back);
        }
        // Where the neural recogniser's answer lies: the content crop, which it takes by the
        // same rule as `tighten`, so the two agree on the box.
        let fallback = used_paddle
            .then(|| tight.as_ref().map(|t| (t.off_x as i32, t.off_y as i32, t.cw as i32, t.ch as i32)))
            .flatten();
        Ok(OcrText { text, words, lines, fallback, skipped: false })
}

/// The shared parser's mask, converted to what `RegisterHotKey` and `SendInput` are given. The
/// grammar is tested in `backend::key_grammar_tests`; these hold the two conversions, which no
/// other test reaches.
#[cfg(test)]
mod key_conversion_tests {
    use super::*;

    #[test]
    fn a_spec_becomes_the_register_hotkey_modifiers_it_names() {
        assert_eq!(
            parse_spec("Ctrl+Shift+Win+Alt+F5"),
            Ok((MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_WIN, 0x74))
        );
        assert_eq!(parse_spec("Ctrl+S"), Ok((MOD_CONTROL, 0x53)));
        // Every spelling of a role is that role: Cmd is Ctrl, Meta is Win, Option is Alt.
        assert_eq!(parse_spec("Cmd+S"), Ok((MOD_CONTROL, 0x53)));
        assert_eq!(parse_spec("Meta+E"), Ok((MOD_WIN, 0x45)));
        assert_eq!(parse_spec("Option+V"), Ok((MOD_ALT, 0x56)));
        assert!(parse_spec("Mod+S").unwrap_err().contains("Mod"));
        assert!(parse_spec("Global+F5").unwrap_err().contains("Global"));
        assert_eq!(parse_spec("Alt+V"), Ok((MOD_ALT, 0x56)));
        assert_eq!(parse_spec("Shift+Tab"), Ok((MOD_SHIFT, 0x09)));
        assert_eq!(parse_spec("Win+E"), Ok((MOD_WIN, 0x45)));
        assert_eq!(parse_spec("F6"), Ok((0, 0x75)));
        let e = parse_spec("Alt tap").unwrap_err();
        assert!(e.contains("tap"), "{e}");
        assert!(parse_spec("Hyper+X").unwrap_err().contains("Hyper"));
    }

    #[test]
    fn a_sent_chord_holds_its_modifiers_in_one_order() {
        assert_eq!(modifier_vks(0), Vec::<u16>::new());
        let all = super::super::key_spec("Ctrl+Shift+Win+Alt+F5").unwrap().1;
        assert_eq!(modifier_vks(all), vec![0x11, 0x12, 0x10, 0x5B]);
        // What `host.input.send("Ctrl+C")` holds: Control, the key that copies here.
        let copy = super::super::key_spec("Ctrl+C").unwrap().1;
        assert_eq!(modifier_vks(copy), vec![0x11]);
        assert_eq!(modifier_vks(super::super::MASK_SHIFT), vec![0x10]);
        assert_eq!(modifier_vks(super::super::MASK_ALT), vec![0x12]);
        assert_eq!(modifier_vks(super::super::MASK_CTRL), vec![0x11]);
        assert_eq!(modifier_vks(super::super::MASK_WIN), vec![0x5B]);
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    fn img(w: i32, h: i32) -> CapturedImage {
        CapturedImage { w: w as u32, h: h as u32, rgba: vec![7; (w * h * 4) as usize] }
    }

    #[test]
    fn a_batch_sends_only_the_readable_regions_and_puts_each_answer_back_in_its_place() {
        let regions = [(0, 0, 10, 10), (5, 5, 0, 4), (0, 0, 60_000, 60_000), (1, 2, 3, 4)];
        let mut sent = Vec::new();
        let out = duplicate_with(&regions, |rects| {
            sent = rects.to_vec();
            Ok(rects.iter().map(|&(_, _, w, h)| img(w, h)).collect())
        })
        .unwrap();
        assert_eq!(sent, vec![(0, 0, 10, 10), (1, 2, 3, 4)], "only the readable ones, in order");
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].as_ref().map(|i| (i.w, i.h)).ok(), Some((10, 10)));
        assert_eq!(out[3].as_ref().map(|i| (i.w, i.h)).ok(), Some((3, 4)));
        // The two it could not send fail on their own, the way the standard path fails them —
        // and neither reads as "duplication could not answer", so neither answers an OCR call
        // with an error table instead of raising.
        let degenerate = out[1].as_ref().err().unwrap();
        let too_large = out[2].as_ref().err().unwrap();
        assert_eq!(degenerate, CAPTURE_FAILED);
        assert!(too_large.starts_with(CAPTURE_FAILED) && too_large.contains("40 million"));
        assert!(!degenerate.starts_with(DUPLICATION_UNANSWERED));
        assert!(!too_large.starts_with(DUPLICATION_UNANSWERED));
    }

    #[test]
    fn a_batch_with_nothing_readable_never_asks_the_engine() {
        let out = duplicate_with(&[(0, 0, 0, 0)], |_| panic!("the engine was asked")).unwrap();
        assert_eq!(out[0].as_ref().err().map(String::as_str), Some(CAPTURE_FAILED));
    }

    #[test]
    fn an_engine_that_cannot_answer_fails_the_batch_with_its_reason() {
        let got = duplicate_with(&[(0, 0, 4, 4)], |_| Err(Fallback::SecureDesktop));
        assert_eq!(got.err(), Some(Fallback::SecureDesktop));
        assert!(unanswered(Fallback::SecureDesktop).starts_with(DUPLICATION_UNANSWERED));
    }

    #[test]
    fn a_point_on_no_monitor_reads_the_same_through_both_sources() {
        // GetPixel answers CLR_INVALID there, which reads as black, as a region read does.
        // Both sources must give that answer, and neither may need the engine for it — this
        // test never starts one.
        assert_eq!(colorref_rgb(0xFFFF_FFFF), (0, 0, 0));
        assert_eq!(colorref_rgb(0x00FF_FFFF), (255, 255, 255));
        let (x, y) = (-30_000, -30_000);
        assert!(off_every_monitor(x, y));
        let standard = pixel_through(x, y, CaptureSource::Standard);
        assert_eq!(pixel_through(x, y, CaptureSource::Duplication { or_standard: false }), standard);
        assert_eq!(pixel_through(x, y, CaptureSource::Duplication { or_standard: true }), standard);
    }
}
