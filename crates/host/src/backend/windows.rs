//! Windows implementation of the platform [`Backend`](super::Backend):
//! window enumeration (Win32), global hotkeys (`RegisterHotKey` + `GetMessage`),
//! foreground-change events (`SetWinEventHook`), and screen capture (GDI).

use std::cell::RefCell;
use std::sync::atomic::{AtomicI32, AtomicBool, AtomicIsize, AtomicU32, Ordering};

use super::{
    Backend, CapturedImage, ControlInfo, DumpNode, HostEvents, MouseButton, OcrText, OcrWord,
    WinInfo,
};

use windows_sys::Win32::Foundation::{CloseHandle, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, GetPixel, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
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
    GetForegroundWindow,
    GetGUIThreadInfo, GetMessageW, GetSystemMetrics, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
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
    /// Hotkey ids received by the message-only window proc, drained by the loop.
    static HOTKEY_QUEUE: RefCell<Vec<i32>> = RefCell::new(Vec::new());
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

/// Thread id of the event loop, so the WinEvent hook can wake `GetMessage`.
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// (vk, modifier-mask) pairs currently intercepted (+ suppressed) by the hook.
    static CAPTURED_KEYS: RefCell<Vec<(u32, u8)>> = RefCell::new(Vec::new());
    /// Captured key-downs (vk, modifier-mask) queued for the event loop.
    static KEY_QUEUE: RefCell<Vec<(u32, u8)>> = RefCell::new(Vec::new());
    /// Keys whose DOWN was let through because a screen reader's modifier was held, so
    /// that their UP is let through as well even if the modifier has since been released.
    /// A down without its up leaves the application holding a key nobody is pressing.
    static SCREEN_READER_PASSED: RefCell<Vec<u32>> = RefCell::new(Vec::new());
}

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
}

/// Stateless GDI screen-region capture (BitBlt → GetDIBits → RGBA, top-down). Free-
/// standing (uses no `self`) so the async image worker can call it off the main thread
/// without holding the non-`Send` `Rc<dyn Backend>`; `WindowsBackend::capture` and
/// `capture_fn` both route through it. GDI screen reads are thread-safe; the ~1-frame
/// DWM-compositor cost then lands on the worker, not the event loop.
fn capture_screen(x: i32, y: i32, w: i32, h: i32) -> Option<CapturedImage> {
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

        // GDI returns BGRA; swap to RGBA.
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        Some(CapturedImage {
            w: w as u32,
            h: h as u32,
            rgba: buf,
        })
    }
}

/// Is a DLL of that name loaded in THIS process? Used only for the startup report: the
/// speech clients are loaded by the `tts` crate on demand, so their presence is a fair
/// proxy for "a screen reader answered", and it costs one call rather than a process walk.
fn module_running(name: &str) -> bool {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    !unsafe { GetModuleHandleW(wide.as_ptr()) }.is_null()
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
            "{v} — THE MANIFEST DID NOT TAKE. wxWidgets will say so and fall back to the              pre-XP controls: no checkboxes in the module list, and everything reads worse              to a screen reader"
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
        out.push((
            "screen reader".to_string(),
            match (module_running("nvdaControllerClient64"), module_running("SAAPI64")) {
                (true, _) => "NVDA client loaded".to_string(),
                (_, true) => "System Access client loaded".to_string(),
                _ => "none detected (speech falls back to SAPI)".to_string(),
            },
        ));
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

    fn pixel(&self, x: i32, y: i32) -> (u8, u8, u8) {
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
            ((c & 0xFF) as u8, ((c >> 8) & 0xFF) as u8, ((c >> 16) & 0xFF) as u8)
        }
    }

    fn capture(&self, x: i32, y: i32, w: i32, h: i32) -> Option<CapturedImage> {
        capture_screen(x, y, w, h)
    }

    fn capture_fn(&self) -> fn(i32, i32, i32, i32) -> Option<CapturedImage> {
        capture_screen
    }

    fn ocr(
        &self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        lang: Option<&str>,
    ) -> Result<OcrText, String> {
        let cap = self
            .capture(x, y, w, h)
            .ok_or_else(|| "screen capture failed".to_string())?;
        recognize_image(&cap, lang)
    }

    /// Several regions, one capture. See the trait for why the recognitions stay separate.
    fn ocr_regions(
        &self,
        regions: &[(i32, i32, i32, i32)],
        lang: Option<&str>,
    ) -> Vec<Result<OcrText, String>> {
        let one_each = |b: &Self| -> Vec<Result<OcrText, String>> {
            regions.iter().map(|(x, y, w, h)| b.ocr(*x, *y, *w, *h, lang)).collect()
        };
        if regions.len() < 2 || regions.iter().any(|(_, _, w, h)| *w <= 0 || *h <= 0) {
            return one_each(self);
        }
        let x0 = regions.iter().map(|r| r.0).min().unwrap_or(0);
        let y0 = regions.iter().map(|r| r.1).min().unwrap_or(0);
        let x1 = regions.iter().map(|r| r.0 + r.2).max().unwrap_or(0);
        let y1 = regions.iter().map(|r| r.1 + r.3).max().unwrap_or(0);
        let (bw, bh) = (x1 - x0, y1 - y0);
        let big = match self.capture(x0, y0, bw, bh) {
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
        let parts: Vec<&str> = combo
            .split('+')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();
        let (key_part, mod_parts) = parts
            .split_last()
            .ok_or_else(|| "empty key combo".to_string())?;
        let mut mod_vks: Vec<u16> = Vec::new();
        for m in mod_parts {
            let vk: u16 = match m.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => 0x11,
                "alt" | "option" => 0x12,
                "shift" => 0x10,
                "win" | "super" | "cmd" | "command" | "meta" => 0x5B,
                other => return Err(format!("unknown modifier '{other}'")),
            };
            mod_vks.push(vk);
        }
        let key_vk = parse_key(key_part)? as u16;
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
        let ok = unsafe { RegisterHotKey(hwnd, id, mods | MOD_NOREPEAT, vk) };
        if ok == 0 {
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
    }

    fn watch_foreground(&self) -> Result<(), String> {
        // Idempotent: one foreground/focus hook pair serves every module (the
        // dispatcher routes events to whichever module has a matching trigger),
        // so a module loaded later — at startup or hot-loaded at runtime — only
        // needs the hooks present, not re-installed. Guard like watch_keys.
        if FG_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        HOOK_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
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
        CAPTURED_KEYS.with(|c| *c.borrow_mut() = keys.to_vec());
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

    fn native_menu_open(&self) -> bool {
        popup_menu_open()
    }

    fn watch_keys(&self) -> Result<(), String> {
        if KEY_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
            return Ok(()); // already installed
        }
        HOOK_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
        let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };
        let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), hmod, 0) };
        if hook.is_null() {
            KEY_HOOK_INSTALLED.store(false, Ordering::SeqCst);
            return Err("SetWindowsHookExW(WH_KEYBOARD_LL) failed".to_string());
        }
        Ok(())
    }

    fn run_event_loop(&self, events: &mut dyn HostEvents) -> Result<(), String> {
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            let res = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if res == 0 || res == -1 {
                break; // WM_QUIT or error
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // Hotkeys reach our window proc during dispatch; the hooks queued
            // foreground/key events on this thread. Drain them all.
            self.pump_pending(events);
        }
        Ok(())
    }

    fn pump_pending(&self, events: &mut dyn HostEvents) {
        let hotkeys: Vec<i32> = HOTKEY_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for id in hotkeys {
            events.on_hotkey(id);
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
        let pending_keys: Vec<(u32, u8)> = KEY_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for (vk, mask) in pending_keys {
            events.on_key(vk, mask);
        }
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
        HOTKEY_QUEUE.with(|q| q.borrow_mut().push(wparam as i32));
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
        let tid = HOOK_THREAD.load(Ordering::Relaxed);
        if tid != 0 {
            PostThreadMessageW(tid, WM_NULL, 0, 0);
        }
    } else if event == EVENT_OBJECT_FOCUS {
        // Focus moved (possibly within the same top-level window); coalesce and
        // let the loop re-check via the focus chain.
        FOCUS_DIRTY.with(|f| f.set(true));
        let tid = HOOK_THREAD.load(Ordering::Relaxed);
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
            let tid = HOOK_THREAD.load(Ordering::Relaxed);
            if tid != 0 {
                PostThreadMessageW(tid, WM_NULL, 0, 0);
            }
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

unsafe extern "system" fn ll_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode;
        let msg = wparam as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        if is_down || is_up {
            // GetAsyncKeyState, NOT GetKeyState: a low-level hook runs on the thread
            // that installed it, and GetKeyState reports that THREAD's view of the
            // keyboard — updated only by the messages it retrieves. Our thread never
            // receives the keystrokes (they belong to the focused application), so the
            // modifiers read as up and every combination collapsed to mask 0. Unmodified
            // keys like Tab worked, which is why this stayed hidden.
            let down = |k: u16| (GetAsyncKeyState(k as i32) as u16 & 0x8000) != 0;
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
                    let wanted = CAPTURED_KEYS.with(|c| {
                        c.borrow()
                            .iter()
                            .any(|&(v, m)| v == generic && m == crate::backend::MASK_TAP)
                    });
                    let scope = KEY_SCOPE.load(Ordering::Relaxed);
                    let in_scope = scope == 0 || GetForegroundWindow() as isize == scope;
                    if wanted && in_scope && !popup_menu_open() && !MENU_OPEN.load(Ordering::Relaxed)
                    {
                        KEY_QUEUE.with(|q| q.borrow_mut().push((generic, crate::backend::MASK_TAP)));
                        let tid = HOOK_THREAD.load(Ordering::Relaxed);
                        if tid != 0 {
                            PostThreadMessageW(tid, WM_NULL, 0, 0);
                        }
                    }
                }
            }
            // Match only the exact combo, so "Tab" (mask 0) leaves Alt+Tab alone.
            let matched =
                CAPTURED_KEYS.with(|c| c.borrow().iter().any(|&(v, m)| v == vk && m == mask));
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
                        KEY_QUEUE.with(|q| q.borrow_mut().push((vk, mask)));
                        let tid = HOOK_THREAD.load(Ordering::Relaxed);
                        if tid != 0 {
                            PostThreadMessageW(tid, WM_NULL, 0, 0);
                        }
                    }
                    return 1; // suppress the matched combo (down + up)
                }
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
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

/// Parses a spec like "Ctrl+Alt+H" into Win32 modifier flags + virtual-key code.
fn parse_spec(spec: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = spec
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let (key_part, mod_parts) = parts
        .split_last()
        .ok_or_else(|| "empty hotkey spec".to_string())?;

    let mut mods: u32 = 0;
    for m in mod_parts {
        match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" | "option" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" | "super" | "cmd" | "command" | "meta" => mods |= MOD_WIN,
            other => return Err(format!("unknown modifier '{other}'")),
        }
    }
    Ok((mods, parse_key(key_part)?))
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
        blank: false,
    }
}

/// Runs Windows.Media.Ocr (WinRT) over a captured region.
fn run_ocr(img: &CapturedImage, lang: Option<&str>) -> windows::core::Result<(String, Vec<OcrWord>)> {
    use std::sync::Once;

    use windows::core::HSTRING;
    use windows::Globalization::Language;
    use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Security::Cryptography::CryptographicBuffer;
    use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    });

    // SoftwareBitmap wants BGRA; GDI leaves alpha at zero, so force it opaque.
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
    for line in result.Lines()? {
        for word in line.Words()? {
            let r = word.BoundingRect()?;
            words.push(OcrWord {
                text: word.Text()?.to_string(),
                x: r.X as i32,
                y: r.Y as i32,
                w: r.Width as i32,
                h: r.Height as i32,
            });
        }
    }
    Ok((text, words))
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
        let small = cap.w <= 400 && cap.h <= 200;

        // Run the neural recognizer (PaddleOCR via ONNX Runtime) CONCURRENTLY for
        // small regions. Its result is used only when Windows.Media.Ocr comes back
        // empty — notably a lone digit, which WinRT rejects regardless of size — so
        // that case costs about max(winrt, paddle) instead of their sum. When WinRT
        // succeeds the background thread just finishes unused (negligible at human
        // focus rates). WinRT stays the trusted primary and the only multi-word path.
        let paddle = small.then(|| {
            let probe = CapturedImage {
                w: cap.w,
                h: cap.h,
                rgba: cap.rgba.clone(),
            };
            std::thread::spawn(move || super::paddle_ocr::recognize(&probe))
        });

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
        // the fallback were being asked. Answering here also saves both recognitions on a
        // region there was never anything to read in.
        if tight.as_ref().is_some_and(|t| t.blank) {
            drop(paddle);
            return Ok(OcrText { text: String::new(), words: Vec::new() });
        }
        let t_win = std::time::Instant::now();
        let (mut text, mut words) = run_ocr(img, lang).map_err(|e| format!("OCR failed: {e}"))?;
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
                    used_paddle = true;
                }
            }
            // else: WinRT won; the paddle thread finishes in the background.
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
            for word in &mut words {
                word.x = (word.x - pad) / s + t.off_x as i32;
                word.y = (word.y - pad) / s + t.off_y as i32;
                word.w /= s;
                word.h /= s;
            }
        }
        Ok(OcrText { text, words })
}
