//! Platform backend abstraction. The host talks to the OS only through this
//! trait; one implementation per platform, selected at compile time. Currently:
//! Windows (real) and a stub (other platforms). The macOS backend will be a
//! second `impl Backend`. See `docs/architecture-feasibility-study.md` §3.

use std::rc::Rc;

#[cfg(windows)]
mod windows;
#[cfg(not(windows))]
mod stub;

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

/// A captured screen region (RGBA, row-major, top-down).
pub struct CapturedImage {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
}

/// A recognized word with its bounding box (in capture-region coordinates).
pub struct OcrWord {
    pub text: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// OCR result: the full recognized text plus per-word boxes.
pub struct OcrText {
    pub text: String,
    pub words: Vec<OcrWord>,
}

/// Mouse button for input synthesis.
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// OS-level operations the host needs. Lua-agnostic on purpose: callback/state
/// mapping stays in the host; only OS specifics live behind this trait.
pub trait Backend {
    fn enumerate_windows(&self) -> Vec<WinInfo>;
    fn active_window(&self) -> Option<WinInfo>;

    /// Size of the primary display in pixels.
    fn screen_size(&self) -> (i32, i32);
    /// Color (r, g, b) of the pixel at screen coordinates.
    fn pixel(&self, x: i32, y: i32) -> (u8, u8, u8);
    /// Captures a screen region into an RGBA image.
    fn capture(&self, x: i32, y: i32, w: i32, h: i32) -> Option<CapturedImage>;

    /// Recognizes text in a screen region.
    fn ocr(&self, x: i32, y: i32, w: i32, h: i32, lang: Option<&str>) -> Result<OcrText, String>;

    /// Current mouse cursor position (screen coordinates).
    fn cursor_pos(&self) -> (i32, i32);
    fn mouse_move(&self, x: i32, y: i32);
    fn mouse_click(&self, x: i32, y: i32, button: MouseButton);
    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton);
    fn mouse_scroll(&self, x: i32, y: i32, amount: i32);
    /// Sends a key combo like "Ctrl+S".
    fn key_send(&self, combo: &str) -> Result<(), String>;
    /// Types Unicode text.
    fn type_text(&self, text: &str);

    /// Registers a global hotkey identified by `id` from a spec like "Ctrl+Alt+H".
    fn register_hotkey(&self, id: i32, spec: &str) -> Result<(), String>;
    /// Unregisters a previously registered hotkey by id.
    fn unregister_hotkey(&self, id: i32);

    /// Starts watching foreground-window changes (delivered as `on_window_activate`).
    fn watch_foreground(&self) -> Result<(), String>;

    /// Sets the virtual-key codes to intercept + suppress via the low-level
    /// keyboard hook; captured keys are delivered as `on_key`.
    fn set_captured_keys(&self, vks: &[u32]);
    /// Installs the low-level keyboard hook (idempotent).
    fn watch_keys(&self) -> Result<(), String>;

    /// Runs the platform event loop, dispatching OS events into `events`.
    /// Blocks until the process is terminated.
    fn run_event_loop(&self, events: &mut dyn HostEvents) -> Result<(), String>;
}

/// Sink for OS events, implemented by the host to bridge into Luau callbacks.
pub trait HostEvents {
    fn on_hotkey(&mut self, id: i32);
    fn on_window_activate(&mut self, win: WinInfo);
    fn on_key(&mut self, vk: u32, shift: bool, ctrl: bool, alt: bool);
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

/// The backend for the current platform.
pub fn platform() -> Rc<dyn Backend> {
    #[cfg(windows)]
    let backend: Rc<dyn Backend> = Rc::new(windows::WindowsBackend::new());
    #[cfg(not(windows))]
    let backend: Rc<dyn Backend> = Rc::new(stub::StubBackend);
    backend
}
