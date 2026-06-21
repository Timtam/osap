//! `host.window` backend: enumerate visible top-level windows and read the
//! foreground window. Windows implementation; other platforms return empty for
//! now (the macOS backend will use AXUIElement / CGWindowList — see
//! `docs/window-matching.md`).

/// A snapshot of a window's matchable properties (normalized across platforms).
pub struct WinInfo {
    pub hwnd: isize,
    pub title: String,
    pub class: String,
    pub pid: u32,
    pub exe: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[cfg(windows)]
pub fn enumerate() -> Vec<WinInfo> {
    imp::enumerate()
}
#[cfg(windows)]
pub fn active() -> Option<WinInfo> {
    imp::active()
}

#[cfg(not(windows))]
pub fn enumerate() -> Vec<WinInfo> {
    Vec::new()
}
#[cfg(not(windows))]
pub fn active() -> Option<WinInfo> {
    None
}

#[cfg(windows)]
mod imp {
    use super::WinInfo;

    use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowTextLengthW,
        GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    pub fn enumerate() -> Vec<WinInfo> {
        let mut hwnds: Vec<isize> = Vec::new();
        unsafe {
            EnumWindows(Some(enum_proc), &mut hwnds as *mut Vec<isize> as LPARAM);
        }
        hwnds.into_iter().filter_map(window_info).collect()
    }

    pub fn active() -> Option<WinInfo> {
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_null() {
            return None;
        }
        window_info(hwnd as isize)
    }

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
        let vec = &mut *(lparam as *mut Vec<isize>);
        vec.push(hwnd as isize);
        1 // TRUE — keep enumerating
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
}
