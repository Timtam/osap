//! Platform backend abstraction. The host talks to the OS only through this
//! trait; one implementation per platform, selected at compile time. Currently:
//! Windows (real) and a stub (other platforms). The macOS backend will be a
//! second `impl Backend`. See `docs/architecture-feasibility-study.md` §3.

use std::rc::Rc;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod paddle_ocr;
#[cfg(windows)]
mod uia;
#[cfg(not(windows))]
mod stub;

/// A snapshot of a window's matchable properties (normalized across platforms).
#[derive(Clone)]
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
    /// Screen coords of the client-area top-left. Overlay coordinates are
    /// relative to the client area (as in AutoHotkey's default Client mode),
    /// not the window frame.
    pub client_x: i32,
    pub client_y: i32,
    /// Size of the CLIENT area. Distinct from w/h, which are the window frame:
    /// a top-level window's frame is wider than its content by its borders, and
    /// coordinates measured from the right edge of the content (a plugin header
    /// laid out from the right) land outside it if they use w. Live: a Kontakt 8
    /// standalone window is 1026 wide around 1010 of plugin.
    pub client_w: i32,
    pub client_h: i32,
}

/// A child control (window) inside a top-level window: its class and geometry,
/// for detecting plugins embedded in a host (DAW) window.
#[derive(Clone)]
pub struct ControlInfo {
    pub hwnd: isize,
    pub class: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// Screen coords of the control's client-area top-left — the overlay's
    /// coordinate origin when the plugin is embedded in a host window.
    pub client_x: i32,
    pub client_y: i32,
    /// Size of the control's CLIENT area — see WinInfo::client_w. Equal to w/h
    /// for the borderless Qt windows plugins use, but not in general.
    pub client_w: i32,
    pub client_h: i32,
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

    /// Child controls (descendant windows) of a top-level window, for detecting
    /// embedded plugins by control class + locating their coordinate origin.
    fn window_controls(&self, hwnd: isize) -> Vec<ControlInfo>;

    /// The chain of controls from the currently-focused element up to its
    /// top-level window — for detecting whether focus is inside an embedded
    /// plugin's control (which a foreground check alone can't see).
    fn window_focus_chain(&self) -> Vec<ControlInfo>;

    /// Whether `hwnd`'s UI Automation subtree contains an element with the given
    /// Name + ControlType — for confirming a plugin's identity.
    fn uia_find(&self, hwnd: isize, name: &str, control_type: i32) -> bool;

    /// "Is any of these names present as any of these control types?" — one traversal
    /// per name instead of one per name×type pair. Returns the 1-based index of the
    /// matching name (so the caller learns which), or None.
    fn uia_find_any(&self, hwnd: isize, names: &[String], types: &[i32]) -> Option<usize>;

    /// Screen-pixel centre of that UIA element (to click it), or None if not
    /// found / it has no on-screen rect.
    fn uia_locate(&self, hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)>;

    /// Like `uia_locate`, but first descends into a container element
    /// (`via_name`/`via_type`) and searches for the target within it — crosses a
    /// hosted-fragment boundary a search from `hwnd` does not (a DAW-embedded plugin
    /// whose UI hangs off an identity pane). Click-centre, or None.
    fn uia_locate_via(
        &self,
        hwnd: isize,
        via_name: &str,
        via_type: i32,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)>;

    /// ReaHotkey's `GetPluginUIAElement` + FindElement: locate the element that IS the
    /// plugin (Name == `container_name`, ControlType Window/Pane, `ni::qt::QuickWindow`
    /// class preferred over the `…QWindowIcon` host) and find the target within it, over
    /// the RAW tree walker — the only view that crosses into a DAW-embedded plugin's
    /// hosted Qt fragment. Empty `name` = any element of that type. Click-centre, or None.
    fn uia_plugin_locate(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)>;

    /// Dev/diagnostic: the "interesting" elements of `hwnd`'s UIA subtree (raw
    /// view), as (depth, Name, ClassName, ControlType). For discovering plugin
    /// identity properties.
    fn uia_dump(&self, hwnd: isize) -> Vec<(i32, String, String, i32)>;

    /// Like `uia_dump`, but over the RAW TreeWalker, which crosses into a hosted Qt
    /// fragment that the condition-based dump cannot see.
    fn uia_raw_dump(&self, hwnd: isize) -> Vec<(i32, String, String, i32)>;

    /// What a named element reports about its own state (Toggle pattern, then
    /// LegacyIAccessible state bits), as a diagnostic string. None when not found.
    fn uia_state_probe(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)>;

    /// Click-point (screen centre) of the element reached from the first element
    /// whose ClassName contains `class_substr` + ControlType == `ctype` by walking
    /// `child` (nth child, 0 = none) then `sibling` raw-view siblings. Ports
    /// ReaHotkey FindElement(ClassName) + WalkTree (KK browser, Kontakt What's-New).
    fn uia_class_nav_point(
        &self,
        hwnd: isize,
        class_substr: &str,
        ctype: i32,
        child: i32,
        sibling: i32,
    ) -> Option<(i32, i32)>;

    /// Tab pass-through for a standalone plugin window: SetFocus the next
    /// (`direction` >= 0) / previous keyboard-focusable descendant relative to the one
    /// focused now, wrapping at the ends, and return its (Name, ControlType, 1-based
    /// index, count) to announce. None if the window has no focusable descendants.
    fn uia_focus_step(&self, hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)>;

    /// Size of the primary display in pixels.
    fn screen_size(&self) -> (i32, i32);

    /// Color (r, g, b) of the pixel at screen coordinates.
    fn pixel(&self, x: i32, y: i32) -> (u8, u8, u8);
    /// Captures a screen region into an RGBA image.
    fn capture(&self, x: i32, y: i32, w: i32, h: i32) -> Option<CapturedImage>;

    /// A pointer to the stateless screen-capture routine, so a worker thread can
    /// capture without holding the (`Rc`, non-`Send`) backend. Same result as
    /// `capture`, callable off the main thread (used by the async image worker).
    fn capture_fn(&self) -> fn(i32, i32, i32, i32) -> Option<CapturedImage>;

    /// Recognizes text in a screen region.
    fn ocr(&self, x: i32, y: i32, w: i32, h: i32, lang: Option<&str>) -> Result<OcrText, String>;

    /// Current mouse cursor position (screen coordinates).
    fn cursor_pos(&self) -> (i32, i32);
    fn mouse_move(&self, x: i32, y: i32);
    fn mouse_click(&self, x: i32, y: i32, button: MouseButton);
    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton);

    /// Press a mouse button and LEAVE IT DOWN, and release it, as separate acts.
    ///
    /// `mouse_drag` presses and moves in the same breath, which is the right shape for a
    /// scrollbar and the wrong one for anything that only reveals itself while held —
    /// Melodyne's tool variants appear as a flyout after a press is sustained, and a drag that
    /// moves immediately misses them entirely.
    ///
    /// Split rather than given a timed variant with sleeps, because the sleeps would be in the
    /// pump: holding, gliding and settling is the better part of a second, and nothing else —
    /// speech, hotkeys, detection — may stop for that long. Split, a caller composes the
    /// gesture from timers and the pump keeps running between the parts.
    fn mouse_down(&self, x: i32, y: i32, button: MouseButton);
    fn mouse_up(&self, x: i32, y: i32, button: MouseButton);
    fn mouse_scroll(&self, x: i32, y: i32, amount: i32);
    /// Sends a key combo like "Ctrl+S".
    /// Deliver a key to a specific window as a MESSAGE, bypassing the input queue.
    ///
    /// For a hotkey that has to act AND still let the application have the key. Synthesising it
    /// the ordinary way does not work: our own hotkey registration sees synthesised input just
    /// as it sees real input, so the send re-triggers the handler that sent it. A posted message
    /// goes straight to the window and is seen by nobody else.
    ///
    /// This existed once, was removed when its last caller went away, and is back because
    /// Melodyne's sub-tools are reached by pressing a function key REPEATEDLY — documented by
    /// Celemony, unlike the press-and-hold-and-drag gesture it replaces. Posting is also the
    /// path most likely to work here: Melodyne runs its own message pump with its own
    /// accelerator table, so a posted WM_KEYDOWN reaches TranslateAccelerator while a sent one
    /// would bypass it.
    fn key_post(&self, hwnd: isize, key: &str) -> Result<(), String>;

    fn key_send(&self, combo: &str) -> Result<(), String>;

    /// Types Unicode text.
    fn type_text(&self, text: &str);

    /// Registers a global hotkey identified by `id` from a spec like "Ctrl+Alt+H".
    fn register_hotkey(&self, id: i32, spec: &str) -> Result<(), String>;
    /// Unregisters a previously registered hotkey by id.
    fn unregister_hotkey(&self, id: i32);

    /// Starts watching foreground-window changes (delivered as `on_window_activate`).
    fn watch_foreground(&self) -> Result<(), String>;

    /// Sets the (vk, modifier-mask) pairs to intercept + suppress via the
    /// low-level keyboard hook; captured keys are delivered as `on_key`. A pair
    /// matches only when the pressed modifier state equals the mask (so "Tab"
    /// (mask 0) does not swallow Alt+Tab).
    fn set_captured_keys(&self, keys: &[(u32, u8)]);
    /// Scopes captured-key suppression to the current foreground window (`true`)
    /// or makes it global again (`false`). While scoped, the hook only intercepts
    /// keys when that window is foreground — so a menu opened by a control (which
    /// brings another window to the foreground) receives the keys natively
    /// (ReaHotkey's `HotIf WinActive` model).
    fn set_key_scope(&self, to_foreground: bool);
    /// Marks a (Qt/UIA) menu as open/closed in the focused plugin, so captured
    /// nav keys pass through to it (the Win32 menu check misses plugin menus).
    fn set_menu_open(&self, open: bool);
    /// Are any of Ctrl / Alt / Shift / Win held down right now? A hotkey callback runs
    /// WHILE its own combination is still pressed, so anything it synthesises afterwards
    /// arrives with those modifiers attached — see the note in the overlay runtime.
    fn modifiers_down(&self) -> bool;
    /// Is a NATIVE popup menu on screen? One window-class lookup, no traversal —
    /// the cheap half of "is a menu open", which the key hook already uses and
    /// which the menu watch should ask before paying for an accessibility walk.
    fn native_menu_open(&self) -> bool;
    /// Installs the low-level keyboard hook (idempotent).
    fn watch_keys(&self) -> Result<(), String>;

    /// Runs the platform event loop, dispatching OS events into `events`.
    /// Blocks until the process is terminated.
    fn run_event_loop(&self, events: &mut dyn HostEvents) -> Result<(), String>;

    /// Drains OS events accumulated since the last call (hotkeys, foreground
    /// changes, captured keys) into `events`. Non-blocking — meant to be driven
    /// from a host-owned loop such as a GUI timer tick, where the GUI toolkit
    /// (not us) owns the message pump.
    fn pump_pending(&self, events: &mut dyn HostEvents);
}

/// Sink for OS events, implemented by the host to bridge into Luau callbacks.
pub trait HostEvents {
    fn on_hotkey(&mut self, id: i32);
    fn on_window_activate(&mut self, win: WinInfo);
    /// The keyboard focus moved (possibly within the same top-level window).
    fn on_focus_change(&mut self);
    /// A captured key fired; `mods` is the pressed modifier bitmask (MASK_*).
    fn on_key(&mut self, vk: u32, mods: u8);
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

/// Modifier bitmask for `host.keys` (matches the pressed modifier state exactly).
pub const MASK_SHIFT: u8 = 1;
pub const MASK_CTRL: u8 = 2;
pub const MASK_ALT: u8 = 4;
pub const MASK_WIN: u8 = 8;

/// Parses a key spec like "Tab", "Shift+Tab", "Ctrl+Right" into (vk, modifier mask).
/// A modifier pressed and released with nothing in between — "Alt tap" and friends.
///
/// Its own mask bit rather than a mask of zero, because a bare modifier IS a mask of zero as
/// far as the ordinary matcher is concerned, and every combination would then look like one.
/// The hook never suppresses a tap: the modifier has to keep working as a modifier.
pub const MASK_TAP: u8 = 0x10;

pub fn key_spec(spec: &str) -> Option<(u32, u8)> {
    // "<modifier> tap": pressed alone, no other key in between, and passed through.
    if let Some(rest) = spec.strip_suffix(" tap").or_else(|| spec.strip_suffix(" Tap")) {
        // Resolved here rather than in `key_to_vk`, deliberately: a bare modifier is a key in a
        // TAP and a mistake anywhere else, and putting it in the general table would make
        // "Alt" quietly acceptable as an ordinary hotkey.
        let vk = match rest.trim().to_ascii_lowercase().as_str() {
            "alt" | "option" => 0x12u32,
            "ctrl" | "control" => 0x11,
            "shift" => 0x10,
            "win" | "super" | "cmd" | "command" | "meta" => 0x5B,
            other => key_to_vk(other)?,
        };
        return Some((vk, MASK_TAP));
    }
    let parts: Vec<&str> = spec
        .split('+')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let (key, mods) = parts.split_last()?;
    let mut mask = 0u8;
    for m in mods {
        mask |= match m.to_ascii_lowercase().as_str() {
            "shift" => MASK_SHIFT,
            "ctrl" | "control" => MASK_CTRL,
            "alt" | "option" => MASK_ALT,
            "win" | "super" | "cmd" | "command" | "meta" => MASK_WIN,
            _ => return None,
        };
    }
    Some((key_to_vk(key)?, mask))
}

/// Warms up the secondary OCR engine (loads its model off the hot path) so the
/// first OCR after launch is instant. Returns the warmup thread's handle (where
/// available) so the caller can join it before the process exits — a detached
/// thread still inside ONNX Runtime init when the process tears down races ort's
/// static cleanup and faults (the headless / fast-exit "segfault").
pub fn warmup_ocr() -> Option<std::thread::JoinHandle<()>> {
    #[cfg(windows)]
    let h = Some(paddle_ocr::warmup());
    #[cfg(not(windows))]
    let h: Option<std::thread::JoinHandle<()>> = None;
    h
}

/// The backend for the current platform.
pub fn platform() -> Rc<dyn Backend> {
    #[cfg(windows)]
    let backend: Rc<dyn Backend> = Rc::new(windows::WindowsBackend::new());
    #[cfg(not(windows))]
    let backend: Rc<dyn Backend> = Rc::new(stub::StubBackend);
    backend
}
