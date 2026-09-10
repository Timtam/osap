//! Platform backend abstraction. The host talks to the OS only through this
//! trait; one implementation per platform, selected at compile time: Windows,
//! macOS, and a stub that answers nothing anywhere else. See `docs/macos-port.md`
//! for the second one, which was written without a Mac to run it on.

use std::rc::Rc;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod paddle_ocr;
#[cfg(windows)]
mod uia;
#[cfg(target_os = "macos")]
mod macos;
/// Bringing this process to the front, and whether it has a Dock icon — see
/// `macos::app`. Re-exported so `gui.rs` can reach it without a macOS-only `use`.
#[cfg(target_os = "macos")]
pub(crate) use macos::app::{
    activate_self, note_frontmost_before_gui, restore_frontmost_after_gui_start, set_regular,
};
/// Asking the user to allow Apple Events to VoiceOver — see `macos::perm`. Re-exported for
/// the same reason as the app calls above: `gui.rs` reaches it without a macOS-only `use`,
/// and the path resolves identically in `crates/macos-check`, which borrows this file.
#[cfg(target_os = "macos")]
pub(crate) use macos::perm::request_voiceover_automation;
/// The four permissions as something a window can show — see `macos::perm::permissions`.
/// Re-exported for the same reason as the call above: `gui.rs` reaches it without a
/// macOS-only `use`, and the path resolves identically in `crates/macos-check`.
#[cfg(target_os = "macos")]
pub(crate) use macos::perm::{ask_for, open_pane, permissions};
#[cfg(not(any(windows, target_os = "macos")))]
mod stub;

/// The macOS key table, compiled on every other platform too, for its tests alone.
///
/// It is a table of integers with no platform dependency, and it is the one part of that
/// backend whose correctness can be checked without a Mac: that every key `key_to_vk`
/// accepts has a macOS key code, and that the translation round-trips. Left inside `mod
/// macos` those tests would only ever run on a machine nobody on the project has.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/keys.rs"]
mod macos_keys;

/// The same trick for the frontmost-window memory: pure bookkeeping over pids, handles and
/// timestamps, so the rules it enforces can be executed rather than merely read. They decide
/// whether a blind user's overlay clicks into a rectangle that is still true, and every one
/// of them was got wrong at least once under review.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/front_memory.rs"]
mod macos_front_memory;

/// And the walk budget, for the third time and the same reason: a counter and a clock, with
/// nothing macOS about them. This one bounds how long an accessibility walk may take, and the
/// next session's most valuable keypress points it at the largest tree it has ever seen.
#[cfg(all(test, not(target_os = "macos")))]
#[path = "macos/budget.rs"]
mod macos_budget;

/// Where a permission stands, for something that has to SHOW it rather than log it.
///
/// `Missing` and `Unknown` are constructed on macOS only, which is the whole point of the
/// type living here: the window that reads them compiles everywhere so it can be checked
/// where it is written. The allow says that out loud rather than letting a warning teach
/// somebody to stop reading them.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grant {
    Granted,
    Missing,
    /// The system would not say. Not the same as missing, and kept apart from it on purpose:
    /// telling somebody who cannot see the screen that a permission is absent, when it may be
    /// present, sends them to a settings pane to fix what is not broken.
    Unknown,
}

impl Grant {
    /// The word a screen reader says. Short, because it is read before the explanation on
    /// every visit to the list.
    pub fn word(self) -> &'static str {
        match self {
            Grant::Granted => "granted",
            Grant::Missing => "NOT granted",
            Grant::Unknown => "cannot be determined",
        }
    }
}

/// One permission, as a person needs it described.
///
/// Platform-neutral although only macOS has any, and deliberately so: it is what lets the
/// window that displays them be compiled and run on the machine this project is written on.
/// `gui.rs` cannot be borrowed by `crates/macos-check` — it needs wxWidgets — so anything
/// inside a `cfg(target_os = "macos")` block there is checked by exactly one thing, the macOS
/// CI job, and only after a push. A list that is simply empty everywhere else costs nothing
/// and moves that check back to the desk.
pub struct Permission {
    /// What the system calls it, so the name in the window matches the name in the pane.
    pub name: &'static str,
    pub state: Grant,
    /// What stops working without it — the sentence that makes it worth granting.
    pub without: &'static str,
    /// The settings-pane URL, for the button beside it.
    pub anchor: &'static str,
    /// Whether the application can raise the system's own consent dialog for this one, or
    /// whether the pane is the only route.
    pub can_ask: bool,
    /// Whether nothing works without it — and therefore whether its absence is worth putting
    /// a window in front of somebody who did not ask for one.
    ///
    /// Three of the four gate everything. The fourth only affects which voice speaks, is
    /// asked for by the switch that wants it, and interrupting a launch over it would be
    /// pestering somebody about a feature they never turned on.
    pub blocking: bool,
}

/// The permissions this platform needs the user to grant. Empty where there are none.
#[cfg(not(target_os = "macos"))]
pub(crate) fn permissions() -> Vec<Permission> {
    Vec::new()
}

/// Opens a settings pane. False where there is no such thing to open.
#[cfg(not(target_os = "macos"))]
pub(crate) fn open_pane(_anchor: &str) -> bool {
    false
}

/// Raises the system's own consent dialog, where there is one. False otherwise.
#[cfg(not(target_os = "macos"))]
pub(crate) fn ask_for(_name: &str) -> bool {
    false
}

/// One running application, as `host.window.apps()` lists them and as `find` narrows its
/// search by. The same three identities a window table's `app` carries, so a matcher's `app`
/// clause tests against either without knowing which.
#[derive(Clone, Debug)]
pub struct AppInfo {
    pub pid: u32,
    pub exe: String,
    pub bundle_id: String,
}

/// A snapshot of a window's matchable properties (normalized across platforms).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WinInfo {
    pub hwnd: isize,
    pub title: String,
    pub class: String,
    pub pid: u32,
    pub exe: String,
    /// The application's bundle identifier — `"com.native-instruments.Kontakt8"`. Empty
    /// where the platform has no such thing, which is everywhere but macOS.
    ///
    /// It exists because `exe` is not a stable identity there: the executable inside a
    /// bundle is named by the vendor's build system rather than by the product, so
    /// "Ableton Live 11 Suite.exe" on Windows is plain "Live" on macOS while the bundle id
    /// still says `com.ableton.live`. A matcher that wants one identity for both platforms
    /// matches `app.name`; one that wants to be exact on macOS matches this.
    pub bundle_id: String,
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

/// One element of an accessibility dump: what it is, what it is called, and WHERE it is.
///
/// The rectangle was added after the first real macOS session. A dump without it says a
/// plugin has a slider and does not say where — which is half an answer when the whole
/// purpose of the dump is to let someone author coordinates for a machine they cannot see.
/// It also measures things nothing else can: the first probe of a plugin showed every
/// Windows-authored coordinate sitting exactly one title bar too high on macOS, and the
/// window's own close button is the element that says how tall that title bar is.
///
/// Screen coordinates, and `(0, 0, 0, 0)` for an element with no rectangle of its own.
pub struct DumpNode {
    pub depth: i32,
    pub name: String,
    pub class: String,
    pub ctype: i32,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
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
    /// What this machine looks like from the backend's side, as name/value pairs, written
    /// to the log once at startup.
    ///
    /// This exists for the failures we cannot see. The platform is used by a blind person,
    /// increasingly on a machine nobody here owns, and the interesting faults are
    /// environmental: a permission that was never granted, a display scale that makes every
    /// coordinate half of what it should be, a screen reader that is not running. Each of
    /// those otherwise arrives as "it does not work" with no way back to a cause. Asked
    /// once, so it may take its time and prompt nothing.
    fn environment(&self) -> Vec<(String, String)>;

    fn enumerate_windows(&self) -> Vec<WinInfo>;

    /// The windows of these processes only.
    ///
    /// The unfiltered listing asks every running application a question, and on macOS that
    /// is a cross-process call per application with a timeout each; one wedged process
    /// costs the whole timeout, on the thread that carries the event tap. The third Mac
    /// session put a number on it: 2.0-2.3 s of a 3 s stall per press was this listing,
    /// every press, because some unrelated application never answered. A matcher that
    /// names an application — and every shipped one does — has no business asking the
    /// others, so `find`/`findAll` resolve the named ones through `running_apps` (local,
    /// cheap) and come here with the pids. The default filters the full listing, which is
    /// right where enumeration is local (Windows); the macOS backend overrides it.
    fn enumerate_windows_of(&self, pids: &[u32]) -> Vec<WinInfo> {
        self.enumerate_windows().into_iter().filter(|w| pids.contains(&w.pid)).collect()
    }

    /// Every running application, without asking any of them anything.
    ///
    /// Local on every platform: the workspace's own list on macOS, and on Windows the
    /// owners of the visible top-level windows, which is the same set of processes a window
    /// search could ever find anything in. What a matcher's `app` clause is tested against
    /// before any window is asked for.
    fn running_apps(&self) -> Vec<AppInfo> {
        let mut out: Vec<AppInfo> = Vec::new();
        for w in self.enumerate_windows() {
            if !out.iter().any(|a| a.pid == w.pid) {
                out.push(AppInfo { pid: w.pid, exe: w.exe.clone(), bundle_id: w.bundle_id.clone() });
            }
        }
        out
    }

    fn active_window(&self) -> Option<WinInfo>;

    /// Brings a window to the front and gives it the keyboard.
    ///
    /// Missing until now, and its absence had a name: on Windows a screen-reader user gets
    /// back to a plugin's window with OSARA's F6, and macOS has no equivalent — VoiceOver
    /// offers no command for it, so a plugin window opened inside a DAW can be genuinely
    /// hard to return to. Nothing in this platform could offer one either, because nothing
    /// here could focus a window.
    ///
    /// Returns whether the system accepted it. Both platforms can refuse — Windows will not
    /// let an application steal the foreground under some conditions, and on macOS the
    /// window's own application has to be activated as well as the window raised — so the
    /// answer is reported rather than assumed.
    fn focus_window(&self, id: isize) -> bool {
        let _ = id;
        false
    }

    /// Is `hwnd`'s window the one lying under this screen point?
    ///
    /// Every hotspot in this project clicks a screen coordinate that was worked out from a
    /// window's own frame, and until now nothing checked that the window was still the thing
    /// on top there. A notification, a tooltip, another application raised over ours: the
    /// click goes to whichever window owns the pixel, and the overlay says "activated"
    /// regardless. For somebody who cannot see the screen that is a press with no way to tell
    /// where it landed.
    ///
    /// `None` means the platform cannot say, and is deliberately not `false`: a backend that
    /// has no answer must not stop a click that would have worked. Only a definite "that
    /// belongs to somebody else" refuses.
    fn window_owns_point(&self, _hwnd: isize, _x: i32, _y: i32) -> Option<bool> {
        None
    }

    /// Child controls (descendant windows) of a top-level window, for detecting
    /// embedded plugins by control class + locating their coordinate origin.
    fn window_controls(&self, hwnd: isize) -> Vec<ControlInfo>;

    /// The chain of controls from the currently-focused element up to its
    /// top-level window — for detecting whether focus is inside an embedded
    /// plugin's control (which a foreground check alone can't see).
    fn window_focus_chain(&self) -> Vec<ControlInfo>;

    /// Whether `hwnd`'s UI Automation subtree contains an element with the given
    /// Name + ControlType — for confirming a plugin's identity.
    fn element_find(&self, hwnd: isize, name: &str, control_type: i32) -> bool;

    /// "Is any of these names present as any of these control types?" — one traversal
    /// per name instead of one per name×type pair. Returns the 1-based index of the
    /// matching name (so the caller learns which), or None.
    fn element_find_any(&self, hwnd: isize, names: &[String], types: &[i32]) -> Option<usize>;

    /// Screen-pixel centre of that UIA element (to click it), or None if not
    /// found / it has no on-screen rect.
    fn element_locate(&self, hwnd: isize, name: &str, control_type: i32) -> Option<(i32, i32)>;

    /// Like `element_locate`, but first descends into a container element
    /// (`via_name`/`via_type`) and searches for the target within it — crosses a
    /// hosted-fragment boundary a search from `hwnd` does not (a DAW-embedded plugin
    /// whose UI hangs off an identity pane). Click-centre, or None.
    fn element_locate_via(
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
    fn element_plugin_locate(
        &self,
        hwnd: isize,
        container_name: &str,
        name: &str,
        control_type: i32,
    ) -> Option<(i32, i32)>;

    /// Dev/diagnostic: the "interesting" elements of `hwnd`'s UIA subtree (raw
    /// view), as (depth, Name, ClassName, ControlType). For discovering plugin
    /// identity properties.
    fn element_dump(&self, hwnd: isize) -> Vec<DumpNode>;

    /// Like `element_dump`, but over the RAW TreeWalker, which crosses into a hosted Qt
    /// fragment that the condition-based dump cannot see.
    fn element_raw_dump(&self, hwnd: isize) -> Vec<DumpNode>;

    /// What a named element reports about its own state (Toggle pattern, then
    /// LegacyIAccessible state bits), as a diagnostic string. None when not found.
    fn element_state_probe(
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
    fn element_class_nav_point(
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
    fn element_focus_step(&self, hwnd: isize, direction: i32) -> Option<(String, i32, i32, i32)>;

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

    /// Several regions, ONE screen touch.
    ///
    /// Measured on Windows: recognising a 67x13 read-out costs 4-6 ms, cropping and upscaling
    /// it another 0.5 — while the capture underneath costs a fixed ~17 ms compositor frame
    /// whatever its size. So two adjacent read-outs on the same row, read one after the other,
    /// spend two thirds of their time photographing the screen twice. Melodyne's selection
    /// watcher does exactly that eight times a second.
    ///
    /// The regions are NOT merged into one recognition: that was tried and it lost values. The
    /// per-region fallback to the neural recogniser only fires for a region that came back
    /// empty, and a merged strip is never empty, so a note name that WinRT dropped stayed
    /// dropped while the cents beside it came through. Only the capture is shared; every region
    /// is still recognised on its own, with its own fallback, and its word coordinates are
    /// still relative to itself.
    ///
    /// The default implementation is the honest one for a backend that has not specialised it:
    /// the same calls in the same order, one capture each. Overriding it is an optimisation,
    /// never a change in meaning.
    fn ocr_regions(
        &self,
        regions: &[(i32, i32, i32, i32)],
        lang: Option<&str>,
    ) -> Vec<Result<OcrText, String>> {
        regions
            .iter()
            .map(|(x, y, w, h)| self.ocr(*x, *y, *w, *h, lang))
            .collect()
    }

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
    /// `delta` is in WHEEL UNITS, where 120 is one notch — the unit the operating system
    /// itself uses. Notches were the unit here until a control turned out to move too far for
    /// one of them to be a usable step.
    fn mouse_scroll(&self, x: i32, y: i32, delta: i32);
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

    /// Sends a key combo like "Ctrl+S" as synthetic input.
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
    /// One turn of the event loop has finished delivering events.
    ///
    /// The GUI path has a timer tick for the work that has to happen whether or not anything
    /// happened; a headless loop had nowhere to put it, which is how the Windows speech
    /// deadline came to be unreachable there — it lives in `Speech::pump`, and nothing in
    /// that loop called it. So every event loop now has the same hook.
    fn on_tick(&mut self) {}
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
    #[cfg(target_os = "macos")]
    let backend: Rc<dyn Backend> = macos::new();
    #[cfg(not(any(windows, target_os = "macos")))]
    let backend: Rc<dyn Backend> = Rc::new(stub::StubBackend);
    backend
}
