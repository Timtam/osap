//! Windows implementation of the platform [`Backend`](super::Backend):
//! window enumeration (Win32), global hotkeys (`RegisterHotKey` + `GetMessage`),
//! foreground-change events (`SetWinEventHook`), and screen capture (GDI).

use std::cell::RefCell;
use std::sync::atomic::{AtomicU32, Ordering};

use super::{Backend, CapturedImage, HostEvents, WinInfo};

use windows_sys::Win32::Foundation::{CloseHandle, HMODULE, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetPixel, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    SRCCOPY,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumWindows, GetClassNameW, GetForegroundWindow, GetMessageW,
    GetSystemMetrics, GetWindowRect, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
    IsWindowVisible, PostThreadMessageW, TranslateMessage, EVENT_SYSTEM_FOREGROUND, MSG,
    SM_CXSCREEN, SM_CYSCREEN, WINEVENT_OUTOFCONTEXT, WM_HOTKEY, WM_NULL,
};

thread_local! {
    /// HWNDs whose window became foreground, queued by the WinEvent hook and
    /// drained by the event loop on the same thread.
    static FOREGROUND_QUEUE: RefCell<Vec<isize>> = RefCell::new(Vec::new());
}

/// Thread id of the event loop, so the WinEvent hook can wake `GetMessage`.
static HOOK_THREAD: AtomicU32 = AtomicU32::new(0);

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
    let k = key.trim();
    if k.len() == 1 {
        let c = k.chars().next().unwrap();
        if c.is_ascii_alphabetic() {
            return Ok(c.to_ascii_uppercase() as u32);
        }
        if c.is_ascii_digit() {
            return Ok(c as u32);
        }
    }
    let lower = k.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix('f').and_then(|s| s.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Ok(0x70 + (n - 1)); // VK_F1 == 0x70
        }
    }
    let vk = match lower.as_str() {
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
        _ => return Err(format!("unknown key '{key}'")),
    };
    Ok(vk)
}
