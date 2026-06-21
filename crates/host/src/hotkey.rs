//! `host.hotkey` backend.
//!
//! Windows: Win32 `RegisterHotKey` + a `GetMessage` loop on the registering
//! (main) thread; a global hotkey posts `WM_HOTKEY`, which we dispatch to the
//! corresponding Luau callback. Other platforms: not yet implemented (the API
//! exists but returns an error) — see `docs/window-matching.md` /
//! `docs/prior-art-vocr.md` for the planned macOS path (Carbon `RegisterEventHotKey`).

#[cfg(windows)]
mod imp {
    use std::cell::RefCell;
    use std::rc::Rc;

    use mlua::{Function, Lua};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, TranslateMessage, MSG, WM_HOTKEY,
    };

    use crate::HostState;

    /// Registers a global hotkey and stores its Luau callback.
    pub fn register(
        lua: &Lua,
        state: &Rc<RefCell<HostState>>,
        spec: &str,
        cb: Function,
    ) -> mlua::Result<()> {
        let (mods, vk) = parse_spec(spec).map_err(mlua::Error::external)?;

        let id = {
            let mut st = state.borrow_mut();
            st.hotkey_counter += 1;
            st.hotkey_counter
        };

        let ok = unsafe { RegisterHotKey(std::ptr::null_mut(), id, mods | MOD_NOREPEAT, vk) };
        if ok == 0 {
            return Err(mlua::Error::external(format!(
                "RegisterHotKey failed for '{spec}' (already in use by another app?)"
            )));
        }

        let key = lua.create_registry_value(cb)?;
        state.borrow_mut().hotkeys.push((id, key));
        println!("  [hotkey] registered '{spec}' (id {id})");
        Ok(())
    }

    /// Runs the Win32 message loop on the calling thread and dispatches
    /// `WM_HOTKEY` to the stored callbacks. Blocks until the process is terminated.
    pub fn run_loop(lua: &Lua, state: &Rc<RefCell<HostState>>) -> mlua::Result<()> {
        println!("» Listening for hotkeys — press Ctrl+C to quit.");
        let mut msg: MSG = unsafe { std::mem::zeroed() };
        loop {
            let res = unsafe { GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) };
            if res == 0 || res == -1 {
                break; // WM_QUIT or error
            }
            if msg.message == WM_HOTKEY {
                let id = msg.wParam as i32;
                let func: Option<Function> = {
                    let st = state.borrow();
                    match st.hotkeys.iter().find(|(hid, _)| *hid == id) {
                        Some((_, key)) => Some(lua.registry_value::<Function>(key)?),
                        None => None,
                    }
                };
                if let Some(f) = func {
                    if let Err(e) = f.call::<()>(()) {
                        eprintln!("  [hotkey] callback error: {e}");
                    }
                }
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    /// Parses a spec like `"Ctrl+Alt+H"` into Win32 modifier flags + virtual-key code.
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
}

#[cfg(windows)]
pub use imp::{register, run_loop};

#[cfg(not(windows))]
mod imp_other {
    use std::cell::RefCell;
    use std::rc::Rc;

    use mlua::{Function, Lua};

    use crate::HostState;

    pub fn register(
        _lua: &Lua,
        _state: &Rc<RefCell<HostState>>,
        spec: &str,
        _cb: Function,
    ) -> mlua::Result<()> {
        Err(mlua::Error::external(format!(
            "host.hotkey is not implemented on this platform yet (spec '{spec}')"
        )))
    }

    pub fn run_loop(_lua: &Lua, _state: &Rc<RefCell<HostState>>) -> mlua::Result<()> {
        Ok(())
    }
}

#[cfg(not(windows))]
pub use imp_other::{register, run_loop};
