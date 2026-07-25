//! Windows implementation of the platform [`Backend`](super::Backend):
//! window enumeration (Win32), global hotkeys (`RegisterHotKey` + `GetMessage`),
//! foreground-change events (`SetWinEventHook`), and screen capture (GDI).

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};

use super::{Backend, CapturedImage, ControlInfo, HostEvents, MouseButton, OcrText, OcrWord, WinInfo};

use windows_sys::Win32::Foundation::{CloseHandle, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, ClientToScreen, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject,
    GetDC, GetDIBits, GetPixel, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_KEYBOARD, INPUT_MOUSE,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, VK_CONTROL, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumChildWindows,
    EnumWindows, FindWindowW, GetAncestor, GetClassNameW, GetCursorPos, GetForegroundWindow,
    GetGUIThreadInfo, GetMessageW, GetSystemMetrics, GetWindowRect,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    PostThreadMessageW, RegisterClassW, SetCursorPos, SetWindowsHookExW, TranslateMessage,
    EVENT_OBJECT_FOCUS, EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND, GA_PARENT, GUITHREADINFO,
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
}

static KEY_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static FG_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// HWND (isize) the captured-key suppression is scoped to (0 = global). The hook
/// only intercepts a captured key while this window is foreground — so a menu a
/// control opened (another window) gets Tab/Enter natively, ReaHotkey-style.
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
    unsafe {
        let screen_dc = GetDC(std::ptr::null_mut());
        if screen_dc.is_null() {
            return None;
        }
        let mem_dc = CreateCompatibleDC(screen_dc);
        let bmp = CreateCompatibleBitmap(screen_dc, w, h);
        let old = SelectObject(mem_dc, bmp);
        BitBlt(mem_dc, 0, 0, w, h, screen_dc, x, y, SRCCOPY);

        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bmi.bmiHeader.biWidth = w;
        bmi.bmiHeader.biHeight = -h; // top-down
        bmi.bmiHeader.biPlanes = 1;
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = BI_RGB as u32;

        let mut buf = vec![0u8; (w * h * 4) as usize];
        GetDIBits(
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

impl Backend for WindowsBackend {
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

    fn uia_find(&self, hwnd: isize, name: &str, control_type: i32) -> bool {
        super::uia::uia_find(hwnd, name, control_type)
    }

    fn uia_locate(&self, hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)> {
        super::uia::uia_locate(hwnd, name, control_type)
    }

    fn uia_dump(&self, hwnd: isize) -> Vec<(i32, String, String, i32)> {
        super::uia::uia_dump(hwnd)
    }

    fn uia_class_nav_point(
        &self,
        hwnd: isize,
        class_substr: &str,
        ctype: i32,
        child: i32,
        sibling: i32,
    ) -> Option<(i32, i32)> {
        super::uia::uia_class_nav_point(hwnd, class_substr, ctype, child, sibling)
    }

    fn uia_focus_step(&self, hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)> {
        super::uia::uia_focus_step(hwnd, direction)
    }

    fn screen_size(&self) -> (i32, i32) {
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
    }

    fn pixel(&self, x: i32, y: i32) -> (u8, u8, u8) {
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
        let debug = std::env::var_os("AUTOMATION_PLATFORM_OCR_DEBUG").is_some();
        if debug {
            save_debug(&cap, "ocr-debug-raw.png");
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

        let tight = if small { Some(tighten(&cap)) } else { None };
        let img: &CapturedImage = tight.as_ref().map(|t| &t.img).unwrap_or(&cap);
        if debug {
            save_debug(img, "ocr-debug.png");
        }
        let t_win = std::time::Instant::now();
        let (mut text, mut words) = run_ocr(img, lang).map_err(|e| format!("OCR failed: {e}"))?;
        let win_ms = t_win.elapsed().as_secs_f64() * 1000.0;

        let mut used_paddle = false;
        if let Some(handle) = paddle {
            if text.trim().is_empty() {
                if let Some(t) = handle.join().ok().flatten() {
                    text = t;
                    words.clear(); // recognition-only fallback returns text without boxes
                    used_paddle = true;
                }
            }
            // else: WinRT won; the paddle thread finishes in the background.
        }
        if debug {
            crate::logging::line(
                "ocr",
                &format!(
                    "{}x{} winrt {:.1}ms{} -> '{}'",
                    cap.w,
                    cap.h,
                    win_ms,
                    if used_paddle { " +paddle" } else { "" },
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

    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton) {
        let (down, up) = button_flags(button);
        unsafe {
            SetCursorPos(x1, y1);
            send_mouse_event(down, 0);
            SetCursorPos(x2, y2);
            send_mouse_event(up, 0);
        }
    }

    fn mouse_scroll(&self, x: i32, y: i32, amount: i32) {
        unsafe {
            SetCursorPos(x, y);
            send_mouse_event(MOUSEEVENTF_WHEEL, amount * 120); // 120 == WHEEL_DELTA
        }
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
        Some(ControlInfo {
            hwnd: hwnd_val,
            class,
            x: rect.left,
            y: rect.top,
            w: rect.right - rect.left,
            h: rect.bottom - rect.top,
            client_x: client.x,
            client_y: client.y,
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
fn popup_menu_open() -> bool {
    const MENU_CLASS: [u16; 7] = [
        b'#' as u16, b'3' as u16, b'2' as u16, b'7' as u16, b'6' as u16, b'8' as u16, 0,
    ];
    let hwnd = unsafe { FindWindowW(MENU_CLASS.as_ptr(), std::ptr::null()) };
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) != 0 }
}

unsafe extern "system" fn ll_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        let vk = kb.vkCode;
        let msg = wparam as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        if is_down || is_up {
            let down = |k: u16| (GetKeyState(k as i32) as u16 & 0x8000) != 0;
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
            // Match only the exact combo, so "Tab" (mask 0) leaves Alt+Tab alone.
            let matched =
                CAPTURED_KEYS.with(|c| c.borrow().iter().any(|&(v, m)| v == vk && m == mask));
            if matched {
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

        Some(WinInfo {
            hwnd: hwnd_val,
            title,
            class,
            pid,
            exe,
            x: rect.left,
            y: rect.top,
            w: rect.right - rect.left,
            h: rect.bottom - rect.top,
            client_x: client.x,
            client_y: client.y,
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
        return whole();
    }

    let m = 3i32;
    let x0 = (x0 as i32 - m).max(0) as u32;
    let y0 = (y0 as i32 - m).max(0) as u32;
    let x1 = (x1 as i32 + m).min(cap.w as i32 - 1) as u32;
    let y1 = (y1 as i32 + m).min(cap.h as i32 - 1) as u32;
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
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
