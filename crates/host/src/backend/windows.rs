//! Windows implementation of the platform [`Backend`](super::Backend):
//! window enumeration (Win32), global hotkeys (`RegisterHotKey` + `GetMessage`),
//! foreground-change events (`SetWinEventHook`), and screen capture (GDI).

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use super::{Backend, CapturedImage, HostEvents, MouseButton, OcrText, OcrWord, WinInfo};

use windows_sys::Win32::Foundation::{CloseHandle, HMODULE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetPixel, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    SRCCOPY,
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
    CallNextHookEx, DispatchMessageW, EnumWindows, GetClassNameW, GetCursorPos, GetForegroundWindow,
    GetMessageW, GetSystemMetrics, GetWindowRect, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, PostThreadMessageW, SetCursorPos, SetWindowsHookExW,
    TranslateMessage, EVENT_SYSTEM_FOREGROUND, HC_ACTION, KBDLLHOOKSTRUCT, MSG, SM_CXSCREEN,
    SM_CYSCREEN, WH_KEYBOARD_LL, WINEVENT_OUTOFCONTEXT, WM_HOTKEY, WM_KEYDOWN, WM_KEYUP, WM_NULL,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

thread_local! {
    /// HWNDs whose window became foreground, queued by the WinEvent hook and
    /// drained by the event loop on the same thread.
    static FOREGROUND_QUEUE: RefCell<Vec<isize>> = RefCell::new(Vec::new());
}

/// Thread id of the event loop, so the WinEvent hook can wake `GetMessage`.
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);

thread_local! {
    /// (vk, modifier-mask) pairs currently intercepted (+ suppressed) by the hook.
    static CAPTURED_KEYS: RefCell<Vec<(u32, u8)>> = RefCell::new(Vec::new());
    /// Captured key-downs (vk, modifier-mask) queued for the event loop.
    static KEY_QUEUE: RefCell<Vec<(u32, u8)>> = RefCell::new(Vec::new());
}

static KEY_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

pub struct WindowsBackend;

impl WindowsBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Backend for WindowsBackend {
    fn enumerate_windows(&self) -> Vec<WinInfo> {
        let mut hwnds: Vec<isize> = Vec::new();
        unsafe {
            EnumWindows(Some(enum_proc), &mut hwnds as *mut Vec<isize> as LPARAM);
        }
        hwnds.into_iter().filter_map(window_info).collect()
    }

    fn active_window(&self) -> Option<WinInfo> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }
        window_info(hwnd as isize)
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
        let (text, words) = run_ocr(&cap, lang).map_err(|e| format!("OCR failed: {e}"))?;
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
        let ok = unsafe { RegisterHotKey(std::ptr::null_mut(), id, mods | MOD_NOREPEAT, vk) };
        if ok == 0 {
            return Err(format!(
                "RegisterHotKey failed for '{spec}' (already in use by another app?)"
            ));
        }
        Ok(())
    }

    fn unregister_hotkey(&self, id: i32) {
        unsafe {
            UnregisterHotKey(std::ptr::null_mut(), id);
        }
    }

    fn watch_foreground(&self) -> Result<(), String> {
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
            return Err("SetWinEventHook failed".to_string());
        }
        // Intentionally leak the hook handle: it lives for the process lifetime.
        Ok(())
    }

    fn set_captured_keys(&self, keys: &[(u32, u8)]) {
        CAPTURED_KEYS.with(|c| *c.borrow_mut() = keys.to_vec());
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
            if msg.message == WM_HOTKEY {
                events.on_hotkey(msg.wParam as i32);
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            // The WinEvent hook ran during dispatch and queued foreground changes.
            let pending: Vec<isize> =
                FOREGROUND_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
            for hwnd in pending {
                if let Some(win) = window_info(hwnd) {
                    events.on_window_activate(win);
                }
            }
            let pending_keys: Vec<(u32, u8)> =
                KEY_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
            for (vk, mask) in pending_keys {
                events.on_key(vk, mask);
            }
        }
        Ok(())
    }
}

unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
    let vec = &mut *(lparam as *mut Vec<isize>);
    vec.push(hwnd as isize);
    1 // TRUE — keep enumerating
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
        // Wake the event loop so it drains the queue even without a real message.
        let tid = HOOK_THREAD.load(Ordering::Relaxed);
        if tid != 0 {
            PostThreadMessageW(tid, WM_NULL, 0, 0);
        }
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
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

fn window_info(hwnd_val: isize) -> Option<WinInfo> {
    let hwnd = hwnd_val as HWND;
    unsafe {
        if IsWindowVisible(hwnd) == 0 {
            return None;
        }
        let title_len = GetWindowTextLengthW(hwnd);
        if title_len <= 0 {
            return None; // skip windows without a title
        }
        let mut buf: Vec<u16> = vec![0; title_len as usize + 1];
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
