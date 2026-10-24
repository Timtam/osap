//! Windows implementation of the platform [`Backend`](super::Backend):
//! window enumeration (Win32), global hotkeys (`RegisterHotKey` + `GetMessage`, and matched
//! in the low-level keyboard hook as well — see `hotkey_hook.rs`), the low-level keyboard
//! hook on a thread of its own (`keyboard_hook_thread`) and the watch that installs it again when
//! Windows removed it, and takes it out of the chain while one of this application's windows is in
//! front (`hook_watch.rs`, `hook_watch_thread.rs`),
//! foreground-change events (`SetWinEventHook`), and screen capture (GDI, or desktop
//! duplication for the modules that declare it — see `dxgi.rs`).

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicI32, AtomicBool, AtomicIsize, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use super::dxgi::{self, Fallback};
use super::frame::{self, Frame, FrameVia};
use super::hook_watch;
use super::hook_watch_thread;
use super::hotkey_hook::{self, Down, Mods, OsPress, Route, NO_ID};
use super::{
    capture_decision, menu_flag_owners, scope_of, Backend, Capture, CaptureFn, CaptureSource,
    CapturedImage, CompareFn, Captured, ControlInfo, HostEvents, KeyRefusal, MouseButton, OcrLine, OcrShot,
    OcrText, OcrThread, OcrWord, OcrWorker, OwnerKeys, Pressed, Recognise, Taken, WinInfo, capture_failed,
    DUPLICATION_UNANSWERED,
};
use crate::errors::{self, HostError};
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
use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindow, SetForegroundWindow, ShowWindow, SW_RESTORE};
use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW,
    SetThreadPriority, PROCESS_QUERY_LIMITED_INFORMATION, THREAD_PRIORITY_HIGHEST,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, FILTERKEYS, HWINEVENTHOOK};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD,
    INPUT_MOUSE, KEYEVENTF_EXTENDEDKEY, MAPVK_VK_TO_VSC_EX, MOUSEINPUT,
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
    MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
    MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_WHEEL, VK_CONTROL, VK_LBUTTON, VK_LWIN,
    VK_MBUTTON, VK_MENU, VK_RBUTTON, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, EnumChildWindows,
    EnumWindows, GetAncestor, GetClassNameW, GetClientRect, GetCursorPos,
    GetForegroundWindow, GetMessageW, SystemParametersInfoW, FKF_FILTERKEYSON, LLKHF_EXTENDED, LLKHF_INJECTED,
    SPI_GETFILTERKEYS, SPI_GETKEYBOARDDELAY,
    GetGUIThreadInfo, GetSystemMetrics, GetWindowRect,
    MsgWaitForMultipleObjects, PeekMessageW, PM_REMOVE, QS_ALLINPUT, WM_QUIT,
    GetMessageTime, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    WindowFromPoint,
    GUI_INMENUMODE, GUI_POPUPMENUMODE, GUI_SYSTEMMENUMODE,
    PostMessageW, PostThreadMessageW, RegisterClassW, SetCursorPos, SetWindowsHookExW,
    TranslateMessage, UnhookWindowsHookEx, HHOOK,
    EVENT_OBJECT_FOCUS, EVENT_OBJECT_NAMECHANGE, EVENT_SYSTEM_FOREGROUND, GA_PARENT, GA_ROOT,
    GUITHREADINFO,
    HC_ACTION, HWND_MESSAGE, KBDLLHOOKSTRUCT, MSG, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
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

/// The captures the hook intercepts (and suppresses), each with its module, and what each module
/// set with `host.keys.scope` and `menuOpen`: what [`super::capture_decision`] decides every key
/// by. Written by the pump when either changes; one lock, so the hook never sees a set and the
/// owners from two different moments.
static KEY_STATE: Mutex<KeyState> = Mutex::new(KeyState { set: Vec::new(), owners: Vec::new() });
/// Captured key-downs the hook took, with their module, queued for the event loop.
static KEY_QUEUE: Mutex<Vec<Taken>> = Mutex::new(Vec::new());

/// The hook's copy of the host's key table — see [`KEY_STATE`].
struct KeyState {
    set: Vec<Captured>,
    owners: Vec<OwnerKeys>,
}

/// Which keys a taken key-down has not been followed by a key-up for yet, one bit per virtual
/// key: a taken key-down whose bit is already set is the keyboard's auto-repeat of a key held
/// down ([`Taken::repeat`]). Every key-up passes through the hook, captured or not, and clears
/// its key's bit; another key going down does not. Forgotten with the rest of what the hook
/// recorded as held when the keyboard goes where the hook is not called
/// ([`forget_keys_held_out_of_sight`]), so that a key-up the hook missed does not make the next
/// deliberate press read as a repeat; and a key the system holds down as a hook of ours goes into
/// the chain is marked held ([`seed_held_keys`]), so that its auto-repeat does not read as a press
/// — nor does a taken key-down, on time, of a key the system holds down already
/// (`hotkey_hook::held_before`).
struct HeldBits([AtomicU64; 4]);

impl HeldBits {
    const fn new() -> Self {
        HeldBits([AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)])
    }

    /// The word and the bit of `vk`. Virtual keys are below 256; anything above wraps round
    /// rather than reaching outside the words.
    fn at(vk: u32) -> (usize, u64) {
        ((vk as usize / 64) & 3, 1u64 << (vk % 64))
    }

    /// A key-down the hook took: whether it repeats one that is still held. Marks it held.
    fn taken_down(&self, vk: u32) -> bool {
        let (word, bit) = Self::at(vk);
        self.0[word].fetch_or(bit, Ordering::Relaxed) & bit != 0
    }

    /// Marks `vk` held without a key-down: one the system holds down as a hook of ours goes into
    /// the chain ([`seed_held_keys`]).
    fn hold(&self, vk: u32) {
        let (word, bit) = Self::at(vk);
        self.0[word].fetch_or(bit, Ordering::Relaxed);
    }

    /// A key-up of `vk`, whatever happened to it.
    fn up(&self, vk: u32) {
        let (word, bit) = Self::at(vk);
        self.0[word].fetch_and(!bit, Ordering::Relaxed);
    }

    /// Whether `vk` is marked held.
    fn is_held(&self, vk: u32) -> bool {
        let (word, bit) = Self::at(vk);
        self.0[word].load(Ordering::Relaxed) & bit != 0
    }

    /// A key-down the hook took as `ck`, the key the captures know it as: whether it repeats one
    /// that is still held ([`HeldBits::taken_down`]). The keypad's Enter (`keypad_enter`) repeats
    /// one held under either of its names, Return or NumpadEnter: a capture of NumpadEnter that
    /// came into scope or went out of it while the key was held makes its next repeat the other
    /// name ([`super::keypad_enter_as`]), and that is still the same press.
    fn taken_down_as(&self, ck: u32, keypad_enter: bool) -> bool {
        let either = keypad_enter && (self.is_held(super::VK_RETURN) || self.is_held(super::VK_NUMPAD_ENTER));
        self.taken_down(ck) || either
    }

    /// A key-up of `vk`, whatever happened to it — of both of the keypad's Enter's names
    /// (`keypad_enter`), since it went down as whichever a capture in scope made it then.
    fn key_up(&self, vk: u32, keypad_enter: bool) {
        self.up(vk);
        if keypad_enter {
            self.up(super::VK_NUMPAD_ENTER);
        }
    }

    fn clear(&self) {
        for w in &self.0 {
            w.store(0, Ordering::Relaxed);
        }
    }
}

/// The keys whose last taken key-down is still held — see [`HeldBits`].
static CAPTURED_HELD: HeldBits = HeldBits::new();
/// The keys whose key-down the hook kept from the system — a captured key it took, a hotkey's key
/// it fired or swallowed as a repeat — with no key-up seen since: held down, although
/// `GetAsyncKeyState`, which a swallowed key-down never sets, says otherwise. What
/// `keyboard_now` asks besides the system, so that a key is not passed on to the program while the
/// user still holds it. Cleared by any key-up of the key, and forgotten with the rest of what the
/// hook recorded as held ([`forget_keys_held_out_of_sight`]).
static KEPT_BACK: HeldBits = HeldBits::new();

/// What `dwExtraInfo` carries on every key event this application sends to pass a key on to the
/// program in front (`pass_on_key`). The hook lets an event carrying it through untouched, before
/// anything else: no capture takes it, and it is no press, hold or tap of the user's. "PASS".
const PASS_ON_MARK: usize = 0x5041_5353;
/// What `dwExtraInfo` carries on every key event `host.input.send` sends (`key_send`). It changes
/// nothing about how the hook takes the key — a capture takes it and a hotkey matches it as if it
/// were typed, and let through it counts in [`LET_THROUGH`] — but a key-down that carries it is
/// left out of [`TAPS_THROUGH`], so a key a module sends ends no count of quick repeats (taps.rs).
/// "SEND".
const SEND_MARK: usize = 0x5345_4E44;
/// How many key-downs the hook has let go on towards the program, wrapping: every key-down that is
/// not a modifier's and that it does not swallow — typed keys no capture took, keys let through for
/// a menu or a screen reader, a held key's repeats, keys another program or this one's `host.input`
/// sent — and never one marked [`PASS_ON_MARK`], nor one of 0xFF: the hotkeys' masking key
/// ([`VK_MASK_KEY`]), and the code Windows gives a key that has no virtual key of its own, so such
/// a key typed after a captured one does not count as overtaking it. Each captured key and each
/// hotkey press carries the count of its press (`crate::backend::Pressed::seq`), and a key passed
/// on later is sent only while the count has not moved since: else a key typed after it has
/// reached the program first (`crate::backend::NotPassed::Overtaken`). One relaxed add per
/// key-down.
static LET_THROUGH: AtomicU32 = AtomicU32::new(0);
/// The key-downs of [`LET_THROUGH`] that end a count of quick repeats: all but those marked
/// [`SEND_MARK`], wrapping. Each press carries it beside [`LET_THROUGH`]'s
/// (`crate::backend::Pressed::taps_seq`). A count of its own rather than a count of the keys sent,
/// taken from `LET_THROUGH`'s: a press whose counts the pump reads — a `WM_HOTKEY`'s — reads each
/// with one load, never two counts a key the hook lets go on just then moves one after the other.
/// One relaxed add per key-down it counts.
static TAPS_THROUGH: AtomicU32 = AtomicU32::new(0);

/// The two counts of key-downs a press carries: [`LET_THROUGH`]'s and [`TAPS_THROUGH`]'s.
type Through = (u32, u32);

/// The two counts of key-downs let through so far, as a press carries them ([`Through`]).
fn through() -> Through {
    (LET_THROUGH.load(Ordering::Relaxed), TAPS_THROUGH.load(Ordering::Relaxed))
}
/// Hotkey presses, drained by the pump: ids the message-only window proc received as
/// `WM_HOTKEY` on the pump's thread, and ids the keyboard hook matched itself on its own (see
/// `hotkey_hook`), each with the way it came, so that the pump can tell the two deliveries of
/// one press apart, and the counts of key-downs let through when it came ([`through`]: as the
/// hook matched it, or as `WM_HOTKEY` reached the window). Drained in place rather than taken, so
/// its capacity stays.
static HOTKEY_QUEUE: Mutex<Vec<(i32, Route, Through)>> = Mutex::new(Vec::new());
/// The end of a stall the host's guard ended in a stop (`drop_queued_input`): a `RegisterHotKey`
/// press stamped at or before this tick is dropped when the next pump drains it — its `WM_HOTKEY`
/// was posted to the stalled thread and reaches [`HOTKEY_QUEUE`] only after the stall. The tick in
/// the low 32 bits, [`DROP_ARMED`] while it waits for that pump; read and cleared by it, on the
/// pump's thread like the stop.
static DROP_OS_UNTIL: AtomicU64 = AtomicU64::new(0);
const DROP_ARMED: u64 = 1 << 32;
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
    /// (The pump.) When the last pump began taking the queued presses, [`HOTKEY_QUEUE`] first and
    /// [`KEY_QUEUE`] after it (`Backend::input_taken_at`).
    static INPUT_TAKEN: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// A hotkey has been filed in [`HOOK_HOTKEYS`] at some point, so the hook consults it. Set
/// once, from registration on the pump thread, and never cleared: an empty table answers "not
/// mine" as cheaply as a flag would, and the flag only spares every keystroke the lock while
/// no module has registered anything.
static HOOK_HOTKEYS_PRESENT: AtomicBool = AtomicBool::new(false);

/// The keyboard watch has raw input from the keyboard, and so hands every physical key-up to the
/// hotkeys' record of held keys ([`key_up_reported`]), the ones the hook is not called for among
/// them. Set once, when the watch's registration is accepted, and never cleared: the registration
/// lasts as long as the process. Until then, a `RegisterHotKey` press is not judged to be the
/// system's first repeat of a press the hook kept (`hotkey_hook::Table::os_repeats`): without the
/// key-ups the hook does not see, a key let go of in one of this application's windows would still
/// read as held, and its next press within the auto-repeat threshold would be dropped.
static KEY_UPS_REPORTED: AtomicBool = AtomicBool::new(false);

/// (The keyboard watch's thread.) Its raw input from the keyboard was registered: from now on it
/// reports every physical key-up ([`KEY_UPS_REPORTED`]).
pub(super) fn key_ups_reported_from_now() {
    KEY_UPS_REPORTED.store(true, Ordering::Relaxed);
}

/// The auto-repeat threshold last put in force, for saying so when the keyboard settings
/// change it — see `repeat_threshold_now`.
static REPEAT_MS_IN_FORCE: AtomicU32 = AtomicU32::new(hotkey_hook::REPEAT_MS);

/// The key sent to mask a swallowed hotkey's modifiers — see `hotkey_hook::needs_mask_key`.
///
/// 0xFF is the code Windows gives a key that has no virtual key of its own (`VK__none_` in the
/// keyboard layouts' `kbd.h`): it names no key. Any key between a modifier's press and its
/// release masks the modifier, so this one does the job and nothing else. AutoHotkey's default
/// for the same job is Ctrl, a key press that programs act on; the unassigned 0xE8 it documents
/// as the alternative, and this hook sent before, is a code a program may still give a meaning
/// of its own. A key of 0xFF from the keyboard is not counted as let through either
/// ([`LET_THROUGH`]).
const VK_MASK_KEY: u16 = 0xFF;

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
///
/// **It can be left standing.** The hook sees only the key-ups of keys going to a window it is
/// called for. NVDA+N shows how that goes wrong: Insert goes down in front of the plugin, NVDA's
/// menu comes to the front, and Insert goes up to the menu — a window of a UIAccess program,
/// one integrity level above ours, whose input the hook of an ordinary process is not called
/// for. Left alone, the record would say "held" until Insert next went up in front of a window
/// the hook sees, and every captured key meanwhile would be let through to the screen reader: the
/// overlay would look dead while its hotkeys still worked. So the keyboard watch has it
/// forgotten whenever the keyboard goes where the hook cannot follow
/// ([`forget_keys_held_out_of_sight`]), and a captured key let through because of it says so
/// in the log, with how long ago the modifier was seen going down (`report_capture_passes`).
static SCREEN_READER_MOD_DOWN: AtomicBool = AtomicBool::new(false);
/// [`SCREEN_READER_MOD_DOWN`] as Insert and Caps Lock alone set it: the record numpad zero's own
/// key is asked against ([`reader_recorded`]). Numpad zero is the one of the three keys a spec
/// names, and the record holds its own key-down too: asked against the whole record, every press
/// of a capture or a hotkey of it would read as made with a screen reader's key held. Cleared with
/// the record by any of the three going up, and forgotten with it.
static SCREEN_READER_MOD_OTHER: AtomicBool = AtomicBool::new(false);
/// The tick at which the hook last saw a screen reader's modifier go down, for the log line
/// that says a captured key was let through because of it.
static SCREEN_READER_MOD_DOWN_AT: AtomicU32 = AtomicU32::new(0);

/// A key event of a screen reader's modifier — Insert, numpad zero or Caps Lock — entered in the
/// hook's record of them ([`SCREEN_READER_MOD_DOWN`], [`SCREEN_READER_MOD_OTHER`]): a key-down sets
/// it, a key-up of any of them clears it. Not one `host.input.send` sent (the hook does not call
/// this for one): a module's numpad zero, down and up, would otherwise clear the record of an
/// Insert the user holds.
fn note_reader_key(vk: u32, is_down: bool, now: u32) {
    if is_down {
        SCREEN_READER_MOD_DOWN_AT.store(now, Ordering::Relaxed);
    }
    SCREEN_READER_MOD_DOWN.store(is_down, Ordering::Relaxed);
    if vk != 0x60 || !is_down {
        SCREEN_READER_MOD_OTHER.store(is_down, Ordering::Relaxed);
    }
}

/// Whether the hook's record has a screen reader's modifier held, for a key event of `vk`: numpad
/// zero's own is asked against the record without its own key-down ([`SCREEN_READER_MOD_OTHER`]),
/// so its repeats are no press made with itself held, and an Insert or a Caps Lock seen going down
/// before it still sends it to the screen reader.
fn reader_recorded(vk: u32) -> bool {
    if vk == 0x60 {
        SCREEN_READER_MOD_OTHER.load(Ordering::Relaxed)
    } else {
        SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed)
    }
}

static KEY_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);
static FG_HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// Which modifier is currently held with nothing pressed since — 0 when none is, which is
/// also what any other key down resets it to. See the tap branch in the hook.
static TAP_ARMED: AtomicI32 = AtomicI32::new(0);

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
    /// hook costs microseconds per keystroke. In the module manager the host's own reload and
    /// quit keys are registered at start, so there the hook is present from the start whatever
    /// the modules do. It runs on a thread of its own that does nothing else
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
/// without holding the non-`Send` `Rc<dyn Backend>`; `capture_fn` and the capture thread's
/// functions route through it. GDI screen reads are thread-safe; the ~1-frame DWM-compositor cost
/// then lands on the worker or the capture thread, never the event loop.
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
/// `fallback` says. The error is a capture error ([`capture_failed`]) — for a request duplication
/// did not answer, one that begins with [`DUPLICATION_UNANSWERED`] ([`unanswered`]).
fn capture_all(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
    let CaptureSource::Duplication { or_standard } = src else {
        // Exactly the read every module has always had.
        return regions
            .iter()
            .map(|&(x, y, w, h)| capture_screen(x, y, w, h).ok_or_else(capture_failed))
            .collect();
    };
    match duplicate(regions) {
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
fn standard_instead(&(x, y, w, h): &(i32, i32, i32, i32)) -> Result<CapturedImage, HostError> {
    // Too large for the duplication path is too large for its fallback too: GDI is protected
    // from a 14 GB allocation only by its bitmap creation happening to fail first. (A module
    // that reads the standard way keeps exactly the read it always had, without this limit.)
    if w > 0 && h > 0 && !dxgi::fits(w, h) {
        return Err(too_large());
    }
    capture_screen(x, y, w, h).ok_or_else(capture_failed)
}

/// The error a read gets when duplication could not answer and the module forbade the
/// fallback: its message begins with [`DUPLICATION_UNANSWERED`], its code is its tail's
/// (`Fallback::reason`). A region too large for duplication is never sent to it, and is told so
/// as the standard path tells it ([`too_large`]).
fn unanswered(why: Fallback) -> HostError {
    match why {
        Fallback::TooLarge => too_large(),
        _ => HostError::with(why.reason(), format!("{DUPLICATION_UNANSWERED} — {}", why.describe())),
    }
}

/// What a region too large for desktop duplication is told, by duplication and by its fallback
/// alike: `screen capture failed: the region is larger than 40 million pixels`.
fn too_large() -> HostError {
    HostError::with(&errors::screen::CAPTURE_TOO_LARGE, format!("{}: {}", capture_failed(), Fallback::TooLarge.describe()))
}

/// Every region through desktop duplication in ONE request — or the reason nothing came back.
fn duplicate(regions: &[(i32, i32, i32, i32)]) -> Result<Vec<Result<CapturedImage, HostError>>, Fallback> {
    duplicate_with(regions, dxgi::capture)
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
) -> Result<Vec<Result<CapturedImage, HostError>>, Fallback> {
    let mut out: Vec<Result<CapturedImage, HostError>> =
        regions.iter().map(|&(_, _, w, h)| Err(if w > 0 && h > 0 { too_large() } else { capture_failed() })).collect();
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
/// and the duplication state it reaches is the engine thread's, not this backend's. Never the
/// event loop's.
fn capture_on_worker(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<CapturedImage, HostError>> {
    crate::loop_guard::off_loop_screen("the image worker's capture");
    capture_all(regions, src)
}

// ── Snapshots ───────────────────────────────────────────────────────────────────────────────

/// [`capture_all`] as frames: every region from `src`, one answer each, in order, each frame
/// recording the path that actually answered it — `Duplication` only for a picture duplication
/// gave, `Gdi` for the standard path and for the fallback to it. The same decisions as
/// `capture_all`, fallback and reasons included.
fn frames_all(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> Vec<Result<Frame, HostError>> {
    frames_with(regions, src, duplicate, |&(x, y, w, h)| {
        capture_screen(x, y, w, h).ok_or_else(capture_failed)
    })
}

/// [`frames_all`] with the duplication request and the standard capture passed in, so the
/// mapping can be tested without a graphics card or a screen.
fn frames_with(
    regions: &[(i32, i32, i32, i32)],
    src: CaptureSource,
    dup: impl FnOnce(&[(i32, i32, i32, i32)]) -> Result<Vec<Result<CapturedImage, HostError>>, Fallback>,
    gdi: impl Fn(&(i32, i32, i32, i32)) -> Result<CapturedImage, HostError>,
) -> Vec<Result<Frame, HostError>> {
    // The moment the picture came back, and a picture exactly the region's size or none: a
    // frame is read by index arithmetic, and a short one must never be kept.
    let kept = |r: &(i32, i32, i32, i32), img: Result<CapturedImage, HostError>, via: FrameVia| {
        img.and_then(|img| {
            Frame::from_image(Rect::from_tuple(*r), img, std::time::Instant::now(), via, None)
                .ok_or_else(capture_failed)
        })
    };
    let CaptureSource::Duplication { or_standard } = src else {
        return regions.iter().map(|r| kept(r, gdi(r), FrameVia::Gdi)).collect();
    };
    match dup(regions) {
        Ok(images) => regions.iter().zip(images).map(|(r, img)| kept(r, img, FrameVia::Duplication)).collect(),
        Err(why) => {
            dxgi::note_fallback(why, or_standard);
            if !or_standard {
                return regions.iter().map(|_| Err(unanswered(why))).collect();
            }
            // `standard_instead`'s rule: too large for duplication is too large for its fallback
            // too. The rest are planned as a standard round is — a region inside another's
            // capture cut from it, unions up to `UNION_MAX` pixels — so a round of several
            // regions that duplication could not answer costs a compositor frame per planned
            // capture, not one per region.
            let past = |&(_, _, w, h): &(i32, i32, i32, i32)| w > 0 && h > 0 && !dxgi::fits(w, h);
            let asked: Vec<usize> = (0..regions.len()).filter(|&i| !past(&regions[i])).collect();
            let rects: Vec<Rect> = asked.iter().map(|&i| Rect::from_tuple(regions[i])).collect();
            let (plan, at) = crate::ocr::snap_queue::group(&rects, crate::ocr::policy::UNION_MAX);
            let mut shots: Vec<Option<Result<Frame, HostError>>> =
                plan.iter().map(|p| Some(kept(&p.tuple(), gdi(&p.tuple()), FrameVia::Gdi))).collect();
            let mut users = vec![0usize; plan.len()];
            for &k in &at {
                users[k] += 1;
            }
            let mut out: Vec<Result<Frame, HostError>> = regions.iter().map(|_| Err(too_large())).collect();
            for (n, &i) in asked.iter().enumerate() {
                let (k, r) = (at[n], rects[n]);
                // The one region of a capture of exactly it takes the frame itself.
                let whole = users[k] == 1 && matches!(&shots[k], Some(Ok(f)) if f.rect == r);
                out[i] = if whole {
                    shots[k].take().unwrap_or_else(|| Err(capture_failed()))
                } else {
                    match &shots[k] {
                        Some(Ok(f)) => f.crop(r).ok_or_else(capture_failed),
                        Some(Err(e)) => Err(e.clone()),
                        None => Err(capture_failed()),
                    }
                };
            }
            out
        }
    }
}

/// The colours at `pts` for `host.screen.pixels`, never through `GetPixel`. A point on no
/// monitor has no picture in either path and reads black, as `pixel_through` answers it, with
/// nothing captured or asked for it (`frame::black_where_off`). The others: the standard path
/// captures the rectangles `frame::plan_points` plans for them — one `BitBlt` of the box they
/// lie in when it is small, one per 2048x976 tile they reach otherwise; desktop duplication gets
/// each of them as a 1x1 piece of ONE request, and when it cannot answer they are read the
/// standard way or fail with the reason, as the module's `fallback` says. The capture thread waits
/// for duplication up to 250 ms.
fn pixels_through(pts: &[(i32, i32)], src: CaptureSource) -> Result<Vec<(u8, u8, u8)>, HostError> {
    pixels_with(pts, src, dxgi::capture, off_every_monitor, |r| {
        capture_screen(r.x, r.y, r.w, r.h).ok_or_else(capture_failed)
    })
}

/// The pixel reads' capture (`OcrWorker::points`), on the capture thread, for a handler that waits
/// and for a callback: each point as [`pixel_through`] reads it for `host.screen.pixel` (`single`),
/// or all of them as [`pixels_through`] reads them for `host.screen.pixels`, waiting for desktop
/// duplication as the capture thread's reads do, up to 250 ms. Never on the event loop.
fn points_on_capture_thread(pts: &[(i32, i32)], src: CaptureSource, single: bool) -> Result<Vec<(u8, u8, u8)>, HostError> {
    crate::loop_guard::off_loop_screen("a pixel read a handler waits for");
    if single {
        pts.iter().map(|&(x, y)| pixel_through(x, y, src)).collect()
    } else {
        pixels_through(pts, src)
    }
}

/// [`pixels_through`] with the engine, the monitor test and the standard capture passed in.
fn pixels_with(
    pts: &[(i32, i32)],
    src: CaptureSource,
    dup: impl FnOnce(&[(i32, i32, i32, i32)]) -> Result<Vec<CapturedImage>, Fallback>,
    off: impl Fn(i32, i32) -> bool,
    gdi: impl FnMut(Rect) -> Result<CapturedImage, HostError>,
) -> Result<Vec<(u8, u8, u8)>, HostError> {
    frame::black_where_off(pts, off, |on| match src {
        CaptureSource::Standard => frame::read_points(on, gdi),
        CaptureSource::Duplication { or_standard } => {
            // Each distinct point once, in the order first asked, in one request.
            let mut asked: Vec<(i32, i32, i32, i32)> = Vec::new();
            let mut slot: std::collections::HashMap<(i32, i32), usize> = std::collections::HashMap::new();
            for &(x, y) in on {
                if !slot.contains_key(&(x, y)) {
                    slot.insert((x, y), asked.len());
                    asked.push((x, y, 1, 1));
                }
            }
            match dup(&asked) {
                Ok(images) if images.len() == asked.len() => on
                    .iter()
                    .map(|p| {
                        let img = &images[slot[p]];
                        img.rgba.get(0..3).map(|c| (c[0], c[1], c[2])).ok_or_else(capture_failed)
                    })
                    .collect(),
                Ok(_) => Err(capture_failed()),
                Err(why) => {
                    dxgi::note_fallback(why, or_standard);
                    if !or_standard {
                        return Err(unanswered(why));
                    }
                    frame::read_points(on, gdi)
                }
            }
        }
    })
}

// ── host.ocr.recognize's two threads ────────────────────────────────────────────────────────

/// The snapshot rounds' capture (`OcrWorker::frames`), on the capture thread: `frames_all` as
/// the image worker's reads wait for desktop duplication — up to 250 ms — with the path each
/// frame came from. `poll` means nothing here. Never on the event loop.
fn frames_on_capture_thread(regions: &[(i32, i32, i32, i32)], src: CaptureSource, _poll: bool) -> Vec<Result<Frame, HostError>> {
    crate::loop_guard::off_loop_screen("a snapshot round");
    frames_all(regions, src)
}

/// A read of a snapshot (`OcrWorker::shot_of`): each region's pixels copied out of the frame, as
/// the capture stage would have handed them over. A region not inside it — which the binding has
/// already cut to it, so only an empty one — fails in its own slot.
fn ocr_shot_of(frame: &Frame, regions: &[(i32, i32, i32, i32)]) -> OcrShot {
    crate::loop_guard::off_loop("a snapshot's pixels for a text read");
    regions
        .iter()
        .map(|&r| frame.crop_image(Rect::from_tuple(r)).ok_or_else(capture_failed))
        .collect()
}

/// The capture stage of `host.ocr.recognize` (`OcrWorker::capture`), on its own thread: every region
/// of one read from one moment, and roughly how many bytes that is.
fn ocr_capture(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> (OcrShot, usize) {
    crate::loop_guard::off_loop("a capture for a text read");
    let shot = capture_for_read(regions, src);
    let bytes = shot.iter().map(|r| r.as_ref().map_or(0, |c| c.rgba.len())).sum();
    (shot, bytes)
}

/// The pixels for a read. Desktop duplication takes every region as a piece of one frame in one
/// request; the standard source photographs the bounding box when that is not wasteful
/// (`ocr::plan`) and each region on its own otherwise, or when the box came back clipped at a
/// screen edge.
fn capture_for_read(regions: &[(i32, i32, i32, i32)], src: CaptureSource) -> OcrShot {
    if let CaptureSource::Duplication { or_standard } = src {
        match duplicate(regions) {
            Ok(caps) => return caps,
            Err(why) => {
                dxgi::note_fallback(why, or_standard);
                if !or_standard {
                    return regions.iter().map(|_| Err(unanswered(why))).collect();
                }
            }
        }
    }
    let one = |&(x, y, w, h): &(i32, i32, i32, i32)| -> Result<CapturedImage, HostError> {
        capture_screen(x, y, w, h).ok_or_else(capture_failed)
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
                    Some((ox, oy)) if !r.is_empty() => {
                        crop(&big, ox, oy, r.w, r.h).ok_or_else(|| HostError::of(&errors::ocr::CROP_OUTSIDE))
                    }
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
    regions: &[(i32, i32, i32, i32)],
    ctx: &Recognise,
) -> Vec<Result<OcrText, HostError>> {
    crate::loop_guard::off_loop("the recognise stage of a text read");
    // The display scale under each region, for a region handed on as captured
    // (`policy::pass_through_up`): the effective DPI of the monitor its middle lies on, over 96.
    // One enumeration of the monitors for the call — local calls, microseconds.
    let monitors = monitors_with_dpi();
    ctx.each(shot.iter().zip(regions.iter()), |(piece, r)| match piece {
        Ok(img) => recognize_image(img, ctx.lang, region_scale(&monitors, *r)),
        Err(e) => Err(e.clone()),
    })
}

/// The display scale under region `(x, y, w, h)`, in physical pixels: the effective DPI of the
/// monitor its middle lies on, over 96. `None` off every monitor, or where Windows would not say.
fn region_scale(monitors: &[MonitorDpi], (x, y, w, h): (i32, i32, i32, i32)) -> Option<f64> {
    let (cx, cy) = (x + w / 2, y + h / 2);
    monitors
        .iter()
        .find(|((l, t, r, b), _)| cx >= *l && cx < *r && cy >= *t && cy < *b)
        .and_then(|(_, dpi)| *dpi)
        .map(|dpi| dpi as f64 / 96.0)
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
/// picture otherwise. `Err` only when duplication could not answer and the module forbade the
/// fallback: the reason, as [`unanswered`] words it for every other read.
fn pixel_through(x: i32, y: i32, src: CaptureSource) -> Result<(u8, u8, u8), HostError> {
    let CaptureSource::Duplication { or_standard } = src else {
        return Ok(pixel_gdi(x, y));
    };
    // A point on no monitor has no picture in either path. `GetPixel` says CLR_INVALID there,
    // which `colorref_rgb` turns into black, the same black a region read gives off the
    // desktop — so the answer is the standard one, and the engine is not started for it.
    if off_every_monitor(x, y) {
        return Ok(pixel_gdi(x, y));
    }
    // Its fallback is `GetPixel`, not a 1x1 blit, so a module that allows the standard way
    // gets exactly the answer it got before.
    match dxgi::capture(&[(x, y, 1, 1)]) {
        Ok(images) => images
            .first()
            .map(|c| (c.rgba[0], c.rgba[1], c.rgba[2]))
            .ok_or_else(capture_failed),
        Err(why) => {
            dxgi::note_fallback(why, or_standard);
            if or_standard {
                Ok(pixel_gdi(x, y))
            } else {
                Err(unanswered(why))
            }
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

/// A module's first-read comparison, made by the thread about to capture for the read that carries
/// it ([`compare_on_worker`]), which waits for duplication as any read of its own does. The event
/// loop makes none: nothing captures there.
fn compare_sources(who: &str, read: (i32, i32, i32, i32)) -> bool {
    let Some((region, window)) = dxgi::comparison_region(read, foreground_client()) else {
        return true; // nothing this path could read, so nothing to compare
    };
    // Its own duplication read, so the module's read is not held up or changed by it — and when
    // duplication cannot answer yet (still opening, backing off), a later read asks again.
    let dup = match dxgi::capture(&[region]) {
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

/// The comparison the `screen-capture` thread and the image worker make (`Backend::compare_fn`,
/// `OcrWorker::compare`): a plain `fn`, as [`capture_on_worker`] is, and like it never the event
/// loop's.
fn compare_on_worker(who: &str, read: (i32, i32, i32, i32)) -> bool {
    crate::loop_guard::off_loop_screen("a first-read comparison");
    compare_sources(who, read)
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

/// A DPI as the scale Windows Settings shows: 96 is 100 %.
fn percent(dpi: u32) -> String {
    format!("{}%", (dpi as f32 / 96.0 * 100.0).round() as i32)
}

/// A monitor as (left, top, right, bottom) in physical pixels, and its effective DPI now.
type MonitorDpi = ((i32, i32, i32, i32), Option<u32>);

/// Every monitor, as (left, top, right, bottom) in physical pixels, with its effective DPI now
/// (`None` when Windows would not say). Local calls, microseconds.
fn monitors_with_dpi() -> Vec<MonitorDpi> {
    use windows_sys::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    unsafe extern "system" fn each(m: HMONITOR, _: HDC, r: *mut RECT, data: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `data` is the Vec below, alive for the whole enumeration, and `r` the monitor
        // rectangle the system hands every callback.
        let v = unsafe { &mut *(data as *mut Vec<MonitorDpi>) };
        if let Some(r) = unsafe { r.as_ref() } {
            let (mut x, mut y) = (0u32, 0u32);
            // SAFETY: the monitor handle the system just handed over; two out-parameters.
            let dpi = (unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut x, &mut y) } >= 0).then_some(x);
            v.push(((r.left, r.top, r.right, r.bottom), dpi));
        }
        1
    }
    let mut v: Vec<MonitorDpi> = Vec::new();
    // SAFETY: every monitor of the desktop, into the Vec above.
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(each),
            &mut v as *mut Vec<MonitorDpi> as LPARAM,
        );
    }
    v
}


/// The `[env]` line `cpu`, as on a Mac: the processor's name, its cores, and the machine's logical
/// processors in every processor group. A text recognition's cost is half the processor's, and a
/// log that does not name it cannot be held against one from another machine. Cheap, once at
/// start: four CPUID calls, and three system calls.
fn cpu_line() -> String {
    use windows_sys::Win32::System::Threading::{GetActiveProcessorCount, ALL_PROCESSOR_GROUPS};
    // SAFETY: no arguments but a group number; 0 is its answer on failure.
    let threads = Some(unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) }).filter(|n| *n > 0);
    crate::ocr::cost::cpu_text(cpu_brand().as_deref(), physical_cores(), threads, None)
}

/// The processor's brand string, from CPUID's three brand leaves: what Task Manager shows.
#[cfg(target_arch = "x86_64")]
fn cpu_brand() -> Option<String> {
    use std::arch::x86_64::__cpuid;
    // SAFETY: CPUID exists on every x86_64 processor; leaf 0x80000000 says how far the extended
    // leaves go, and the brand leaves are read only when they exist.
    #[allow(unused_unsafe)]
    let top = unsafe { __cpuid(0x8000_0000) }.eax;
    if top < 0x8000_0004 {
        return None;
    }
    let mut bytes = Vec::with_capacity(48);
    for leaf in 0x8000_0002u32..=0x8000_0004 {
        // SAFETY: as above, a leaf the processor said it has.
        #[allow(unused_unsafe)]
        let r = unsafe { __cpuid(leaf) };
        for v in [r.eax, r.ebx, r.ecx, r.edx] {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    Some(String::from_utf8_lossy(&bytes[..end]).trim().to_string())
}

/// Not asked on an ARM processor, which has no CPUID: the line says `?`. (The registry's
/// `ProcessorNameString` would name it; the application is not built for ARM Windows.)
#[cfg(not(target_arch = "x86_64"))]
fn cpu_brand() -> Option<String> {
    None
}

/// The processor's cores, from `GetLogicalProcessorInformation`: one entry per core among its
/// relationships. A machine of more than 64 logical processors has them counted in this process's
/// group only.
fn physical_cores() -> Option<u32> {
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
    };
    let each = std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
    let mut len: u32 = 0;
    // SAFETY: a null buffer of length 0 asks for the length needed, which is written to `len`.
    unsafe { GetLogicalProcessorInformation(std::ptr::null_mut(), &mut len) };
    let n = len as usize / each;
    if n == 0 {
        return None;
    }
    let mut buf = vec![SYSTEM_LOGICAL_PROCESSOR_INFORMATION::default(); n];
    // SAFETY: a buffer of exactly the `len` bytes the first call asked for.
    if unsafe { GetLogicalProcessorInformation(buf.as_mut_ptr(), &mut len) } == 0 {
        return None;
    }
    let got = (len as usize / each).min(n);
    u32::try_from(buf[..got].iter().filter(|i| i.Relationship == RelationProcessorCore).count()).ok()
}

/// What a hotkey the system would not register answers: another program holds the combination.
fn register_refused(spec: &str) -> HostError {
    HostError::with(&errors::hotkey::REGISTER_REFUSED, format!("RegisterHotKey failed for '{spec}' (already in use by another app?)"))
}

impl Backend for WindowsBackend {
    fn environment(&self) -> Vec<(String, String)> {
        let mut out = self.display_environment();
        out.push(("common controls".to_string(), common_controls()));
        out.push(("cpu".to_string(), cpu_line()));
        // No "screen reader" line here any more. It asked whether `nvdaControllerClient64`
        // or `SAAPI64` was loaded in this process, which was a fair proxy only while Tolk
        // loaded them on demand — with speech going through prism nothing loads either, so
        // the line reported "none detected (speech falls back to SAPI)" with NVDA running
        // and prism speaking through it. The speech layer logs the real answer a moment
        // later, and it is a better answer: which backend WILL speak, rather than which DLL
        // happens to be in memory.
        out
    }

    /// The primary display's size, the system DPI, and every monitor with its own scale.
    ///
    /// "system dpi" is fixed for the session in a per-monitor-aware process: Windows reports the
    /// DPI the session started with until the user signs out, whatever the scale is changed to
    /// since. So the monitors line gives each monitor's effective DPI now — what a scale change
    /// changed, written again after one (`system_events`).
    fn display_environment(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let (w, h) = self.screen_size();
        out.push(("display".to_string(), format!("{w}x{h} px (primary)")));
        // Whether we are DPI-aware decides whether every coordinate in every module is
        // real or scaled behind our back — the manifest asks for per-monitor v2, and this
        // is the line that proves it took.
        let dpi = unsafe { GetDpiForSystem() };
        out.push(("system dpi".to_string(), format!("{dpi} ({})", percent(dpi))));
        let monitors = monitors_with_dpi();
        out.push((
            "monitors".to_string(),
            format!(
                "{}: {}",
                monitors.len(),
                monitors
                    .iter()
                    .map(|((l, t, r, b), dpi)| match dpi {
                        Some(d) => format!("({l},{t})-({r},{b}) {d} dpi ({})", percent(*d)),
                        None => format!("({l},{t})-({r},{b}) dpi unknown"),
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
        out
    }

    /// Starts the keyboard watch's thread for the system events alone, unless the hook started
    /// it already — see `hook_watch_thread`. The pump is the thread that asks, and the one the
    /// watch wakes when it has queued an event.
    fn watch_system(&self) {
        PUMP_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
        hook_watch_thread::start_system();
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

    /// The foreground window as Windows has it, hidden or not — `active_window` drops a
    /// window that is not visible, through `window_info`, and that is exactly the one this is
    /// for (see [`super::Foreground`]). Five local calls, no message to the window: a hung
    /// application cannot hold it up.
    fn foreground_window(&self) -> Option<super::Foreground> {
        // SAFETY: no arguments, and the handle it returns is only ever passed to calls that
        // validate it.
        foreground_of(unsafe { GetForegroundWindow() })
    }

    /// A control's id brings its top-level window forward and then lands the keyboard on the
    /// control (HW1a); a top-level id is brought forward as ever — and lands on the control a page
    /// in front took the front from, when the id is the window that page came from (HW1b). See
    /// [`start_landing`]. A declined call starts no landing. A handle that names no window is
    /// `gone`; one Windows would not bring forward, `refused`.
    fn focus_window(&self, id: isize) -> Result<(), HostError> {
        let hwnd = id as HWND;
        // SAFETY: a local question about a handle, which a stale one answers no to.
        if hwnd.is_null() || unsafe { IsWindow(hwnd) } == 0 {
            return Err(HostError::of(&errors::window::NO_SUCH_WINDOW));
        }
        // SAFETY: plain queries and requests about a handle; a stale one is refused.
        let (root, accepted) = unsafe {
            let root = GetAncestor(hwnd, GA_ROOT);
            let root = if root.is_null() { hwnd } else { root };
            // Restored first: a minimised window can be made foreground and stay invisible,
            // which for somebody who cannot see the screen is the worst of both answers.
            if IsIconic(root) != 0 {
                ShowWindow(root, SW_RESTORE);
            }
            (root, SetForegroundWindow(root) != 0)
        };
        if !accepted {
            return Err(HostError::of(&errors::common::DECLINED));
        }
        // SAFETY: a plain query.
        let in_front = unsafe { GetForegroundWindow() } == root;
        let page = if root == hwnd { crate::webview_win::front_page_came_from() } else { None };
        let plan = crate::focus_landing::plan(id, root as isize, in_front, page);
        if let Some(control) = plan.land_on {
            start_landing(control, root as isize, plan.look_now);
        }
        Ok(())
    }

    fn take_focus_puts(&self) -> Vec<(u64, isize)> {
        FOCUS_PUTS.with(|p| std::mem::take(&mut *p.borrow_mut()))
    }

    fn focus_put_answered(&self, landing: u64, took: Result<bool, String>) {
        let answered = LANDING.with(|cell| {
            let mut slot = cell.borrow_mut();
            match slot.as_mut() {
                Some(l) if l.id == landing => {
                    l.landing.put_answered(took);
                    say_landing(l);
                    true
                }
                _ => false,
            }
        });
        // Read back on the accessibility thread; the next step is the thread's state now.
        if answered {
            landing_look();
        }
    }

    fn input_made(&self) {
        INPUT_CALLS.fetch_add(1, Ordering::Relaxed);
        landing_look();
    }

    /// Compared at the TOP LEVEL, because an overlay's origin is often a child.
    ///
    /// An embedded plug-in is a control inside its host's window, and the point under the
    /// pointer resolves to whichever child is drawn there — usually a different one. The
    /// question worth asking is not "is this the same window" but "does this point belong to
    /// the same application window", so both sides go up to their root first.
    fn window_owns_point(&self, hwnd: isize, x: i32, y: i32) -> Option<bool> {
        owns_point(hwnd as HWND, x, y)
    }

    /// `windows_of` lists top-level windows by their HWND, so a listed window's id is a handle
    /// already, and the question is `window_owns_point`'s.
    fn listed_window_owns_point(&self, id: u64, x: i32, y: i32) -> Option<bool> {
        self.window_owns_point(id as isize, x, y)
    }

    /// Stricter than `window_owns_point`, which compares top-level windows: the point has to be
    /// in `window` itself or a window inside it, so a click for a plug-in's own window is not made
    /// on the DAW's list beside it. And it says which of three things is wrong.
    ///
    /// Then the rights of the program the click goes into: one of higher rights drops the click on
    /// its way in, which `SendInput` does not say, so it is refused here with the posted click's
    /// `denied` (until 2026-10-24 it answered `true` and nothing was clicked).
    fn click_check(&self, window: isize, x: i32, y: i32) -> Result<(), HostError> {
        let at = super::click_win::target(window, x, y)?;
        if super::click_win::higher_rights(at) {
            return Err(HostError::of(&crate::errors::input::HIGHER));
        }
        Ok(())
    }

    fn mouse_click_posted(&self, window: isize, x: i32, y: i32, button: MouseButton, move_pointer: bool) -> Result<(), HostError> {
        super::click_win::post_click(window, x, y, button, move_pointer)
    }

    fn mouse_click_noactivate(
        &self,
        window: isize,
        x: i32,
        y: i32,
        button: MouseButton,
        owner: usize,
    ) -> Result<super::ActJob, HostError> {
        super::click_win::prepare_hold(window, x, y, button, owner)
    }

    fn give_back_activation(&self, when: super::GiveBackWhen, why: &str) -> Option<super::ActJob> {
        let rule = match when {
            super::GiveBackWhen::Stopped(idx) => super::click_win::GiveBack::Owner(idx),
            super::GiveBackWhen::CameForward(hwnd) => super::click_win::GiveBack::Front(hwnd),
        };
        super::click_win::give_back_job(rule, why)
    }

    fn give_back_activation_at_exit(&self) -> Option<super::ActJob> {
        super::click_win::exit_job()
    }

    fn give_back_left_over_activation(&self) -> Option<super::ActJob> {
        super::click_win::left_over_job()
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

    // Every question is asked on the host's accessibility thread, in the multithreaded apartment
    // (`uia.rs`), and never on the event loop.
    fn element_runner(&self) -> Option<super::ElementRunner> {
        Some(super::ElementRunner { init: super::uia::thread_init, run: super::uia::run, process_of: process_of_window })
    }

    // Never reached: this platform has the thread (`element_runner`), and a thread that could not
    // be started answers every question with why itself (`element.rs`). UI Automation is not asked
    // from the event loop, whose apartment is single-threaded and whose messages it would hold up.
    fn element_ask(&self, _ask: &super::ElementAsk) -> super::ElementOutcome {
        Err(super::ElementFailure { why: HostError::of(&errors::element::WRONG_THREAD), unanswered: false })
    }

    fn screen_size(&self) -> (i32, i32) {
        unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
    }

    fn capture_fn(&self) -> CaptureFn {
        capture_on_worker
    }

    fn compare_fn(&self) -> CompareFn {
        compare_on_worker
    }

    fn ocr_worker(&self) -> OcrWorker {
        OcrWorker {
            present: true,
            init_thread: |_: OcrThread| ensure_winrt(),
            // Nothing measured asks for one here: the first read on Windows is not slow.
            warm_up: |_| {},
            capture: ocr_capture,
            recognise: ocr_recognise,
            languages: ocr_languages,
            frames: frames_on_capture_thread,
            points: points_on_capture_thread,
            compare: compare_on_worker,
            // One standard capture may span monitors here: `BitBlt` of the virtual screen.
            display_of: |_, _| 0,
            shot_of: ocr_shot_of,
        }
    }

    /// Where the system does not give the position — another desktop has the input, the lock
    /// screen — `0, 0`, said in the log once (`cursor_or_origin`).
    fn cursor_pos(&self) -> (i32, i32) {
        let mut p: POINT = unsafe { std::mem::zeroed() };
        // SAFETY: a read of the pointer's place into a local.
        let read = unsafe { GetCursorPos(&mut p) } != 0;
        cursor_or_origin(read.then_some((p.x, p.y)), || unsafe { GetLastError() })
    }

    /// With a button held — one this backend pressed, or one down on the desktop — motion through
    /// the system's input ("The pointer moved as motion"); otherwise the pointer placed.
    fn mouse_move(&self, x: i32, y: i32) -> Result<(), HostError> {
        if button_held_now() {
            send_inputs(&[(motion_input(x, y, virtual_desk()), Holds::Neither)], &[]).answer()
        } else {
            place(x, y)
        }
    }

    fn mouse_click(&self, x: i32, y: i32, button: MouseButton) -> Result<(), HostError> {
        click_input(x, y, button)
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
    ///
    /// Every step is motion over the virtual desktop (`motion_input`), so a drag on any monitor
    /// lands where it is asked to.
    ///
    /// What Windows does not take ends it: placed nowhere or not pressed, nothing is pressed; a
    /// step not taken ends the steps, and the release is sent then all the same. The answer is the
    /// first event Windows did not take — but a release refused even on its own, which leaves the
    /// button down, says so first ([`drag_answer`]).
    fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton) -> Result<(), HostError> {
        let (down, up) = button_flags(button);
        let held = button_code(button);
        place(x1, y1)?;
        send_inputs(&[(motion_input(x1, y1, virtual_desk()), Holds::Neither), (mouse_input(down, 0), Holds::Down(held))], &[])
            .answer()?;
        let stepped = drag_steps((x1, y1), (x2, y2), |at| {
            let taken = send_inputs(&[(motion_input(at.0, at.1, virtual_desk()), Holds::Neither)], &[]).answer();
            if taken.is_ok() {
                std::thread::sleep(std::time::Duration::from_millis(4));
            }
            taken
        });
        let up = send_inputs(&[(mouse_input(up, 0), Holds::Up(held))], &[held]);
        drag_answer(stepped, released(button, &up))
    }

    /// The pointer placed, then the press — noted, so that the moves after it are motion. Placed
    /// nowhere, nothing is pressed.
    fn mouse_down(&self, x: i32, y: i32, button: MouseButton) -> Result<(), HostError> {
        let (down, _) = button_flags(button);
        place(x, y)?;
        send_inputs(&[(mouse_input(down, 0), Holds::Down(button_code(button)))], &[]).answer()?;
        note_button(button, true);
        Ok(())
    }

    /// Motion to (x, y), then the release, in one `SendInput` — no placement: so a press, a glide
    /// of moves and the release are one drag, and the release lands on the control that followed
    /// it. A motion Windows did not take is followed by the release on its own, where the pointer
    /// is; the answer says the motion was not taken.
    fn mouse_up(&self, x: i32, y: i32, button: MouseButton) -> Result<(), HostError> {
        let (_, up) = button_flags(button);
        let held = button_code(button);
        let sent = send_inputs(&[(motion_input(x, y, virtual_desk()), Holds::Neither), (mouse_input(up, 0), Holds::Up(held))], &[held]);
        if sent.let_go.contains(&held) {
            note_button(button, false);
        }
        sent.answer()
    }

    /// The pointer placed, then the wheel. Placed nowhere, nothing is scrolled.
    fn mouse_scroll(&self, x: i32, y: i32, delta: i32) -> Result<(), HostError> {
        place(x, y)?;
        // Already in WHEEL_DELTA units.
        send_inputs(&[(mouse_input(MOUSEEVENTF_WHEEL, delta), Holds::Neither)], &[]).answer()
    }

    /// The key-down and the key-up posted; a post Windows refuses is answered by its error — a
    /// program of higher rights, a handle that names no window, or the error's number — and the
    /// key-up is not posted after a refused key-down.
    fn key_post(&self, hwnd: isize, key: &str) -> Result<(), KeyRefusal> {
        let key_code = parse_key(key)?;
        let vk = os_key(key_code).0 as u16;
        // No handle names no window — and `PostMessageW` would post to this thread's own queue.
        if hwnd == 0 {
            return Err(KeyRefusal::Answer(HostError::of(&errors::common::WINDOW_GONE)));
        }
        // The scan code goes in the lParam because some applications read it rather than the
        // virtual key, and Melodyne decodes keys itself (it imports GetKeyboardState, ToUnicode
        // and MapVirtualKeyW) rather than leaving it to the defaults. An extended one with the
        // extended-key bit (24), as the keyboard delivers it, which is how a program tells the
        // keypad's Enter from the main one and its Divide from the main row's key of scan code
        // 0x35 ([`scan_code`]).
        let (lp_down, lp_up) = post_lparams(scan_code(key_code as u16));
        for (msg, lp) in [(WM_KEYDOWN, lp_down), (WM_KEYUP, lp_up)] {
            // SAFETY: a post to a handle, which a stale one fails.
            if unsafe { PostMessageW(hwnd as HWND, msg, vk as usize, lp) } == 0 {
                let why = super::click_win::post_refused(unsafe { GetLastError() });
                crate::logging::line("input", &format!("post of '{key}' to window {hwnd}: refused at 0x{msg:04x}: {why}"));
                return Err(KeyRefusal::Answer(why));
            }
        }
        Ok(())
    }

    fn key_send(&self, combo: &str) -> super::KeySend {
        // Built first, so that a mistake sends nothing; then sent, with whether anything went out
        // beside the answer (`key_sent`).
        let built = (|| -> Result<Vec<(INPUT, Holds)>, KeyRefusal> {
            // The shared parser, so a spec means here what it means to a hotkey. Its modifiers are
            // roles, which on Windows are the keys of their names.
            let (vk, mask) = super::parse_key_spec(combo)?;
            if mask & super::MASK_TAP != 0 {
                return Err(KeyRefusal::Mistake(format!(
                    "'{combo}' is a modifier tap, which is something to capture, not a key to send"
                )));
            }
            let mod_vks = modifier_vks(mask);
            let (key_vk, extended) = os_key(vk);
            let key_vk = key_vk as u16;
            let ext = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };
            // Every event marked, so that the hook leaves the key-down out of the count that ends
            // a count of quick repeats ([`SEND_MARK`]); it takes and matches the key as ever. One
            // `SendInput`, in order: one Windows stops taking part-way has the releases of the keys
            // it pressed sent on their own (`send_inputs`), so no modifier is left down.
            let mut events = Vec::with_capacity(2 * mod_vks.len() + 2);
            for &vk in &mod_vks {
                events.push((key_input(vk, 0, 0, SEND_MARK), Holds::Down(u32::from(vk))));
            }
            events.push((key_input(key_vk, 0, ext, SEND_MARK), Holds::Down(u32::from(key_vk))));
            events.push((key_input(key_vk, 0, ext | KEYEVENTF_KEYUP, SEND_MARK), Holds::Up(u32::from(key_vk))));
            for &vk in mod_vks.iter().rev() {
                events.push((key_input(vk, 0, KEYEVENTF_KEYUP, SEND_MARK), Holds::Up(u32::from(vk))));
            }
            Ok(events)
        })();
        match built {
            Ok(events) => key_sent(&send_inputs(&events, &[])),
            Err(mistake) => super::KeySend::all_or_nothing(Err(mistake)),
        }
    }

    /// Every UTF-16 unit's down and up, in one `SendInput`. One Windows stops taking part-way ends
    /// the typing there — a unit it took the down of has its up sent on its own — and the answer
    /// says how many characters were typed.
    fn type_text(&self, text: &str) -> Result<(), HostError> {
        let units: Vec<u16> = text.encode_utf16().collect();
        let events: Vec<(INPUT, Holds)> = units
            .iter()
            .enumerate()
            .flat_map(|(i, &u)| {
                // Keyed by the unit's place, so a repeated letter's halves are told apart.
                let held = i as u32;
                [
                    (key_input(0, u, KEYEVENTF_UNICODE, 0), Holds::Down(held)),
                    (key_input(0, u, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP, 0), Holds::Up(held)),
                ]
            })
            .collect();
        let sent = send_inputs(&events, &[]);
        match sent.error {
            None => Ok(()),
            Some(error) => {
                let (typed, of) = typed_of(text, sent.taken);
                crate::logging::line("input", &format!("typing: Windows took {} of {} events; {typed} of {of} characters were typed", sent.taken, events.len()));
                Err(HostError::with(&errors::input::TEXT_NOT_TAKEN, format!("Windows did not take the input after {typed} of {of} characters (error {error})")))
            }
        }
    }

    fn register_hotkey(&self, id: i32, spec: &str) -> Result<(), KeyRefusal> {
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
            return Err(KeyRefusal::Answer(register_refused(spec)));
        }
        Ok(())
    }

    /// `RegisterHotKey` and nothing more: not filed for the keyboard hook, so the hook lets the
    /// press through and Windows turns it into a `WM_HOTKEY`. A combination refused here belongs
    /// to another program, as with `register_hotkey`.
    fn register_foreground_hotkey(&self, id: i32, spec: &str) -> Result<(), KeyRefusal> {
        let (mods, vk) = parse_spec(spec)?;
        let hwnd = hotkey_window();
        if unsafe { RegisterHotKey(hwnd, id, mods | MOD_NOREPEAT, vk) } == 0 {
            return Err(KeyRefusal::Answer(register_refused(spec)));
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

    fn set_captured_keys(&self, keys: &[Captured]) {
        // Built, and the old set freed, outside the lock, so the hook waits for a swap and
        // nothing more.
        let new = keys.to_vec();
        if new.is_empty() {
            // No overlay holds keys any more: the next one's first let-through is explained
            // again, even in the same window (`report_capture_passes`).
            PASS_SAID_FOR.with(|said| said.set([None; 5]));
        }
        let old = std::mem::replace(&mut locked(&KEY_STATE).set, new);
        if old.as_slice() != keys {
            // The arrows noted under the set before are written now, and the next ones are
            // noted again (`note_arrow_let_through`).
            report_arrow_notes();
            ARROW_NOTED.store(0, Ordering::Relaxed);
        }
        drop(old);
    }

    fn set_key_owners(&self, owners: &[OwnerKeys]) {
        let new = owners.to_vec();
        let old = std::mem::replace(&mut locked(&KEY_STATE).owners, new);
        // The menu record of a module whose entry the host dropped — disabled, reloaded,
        // removed — goes with the entry. Any other module keeps its record until it reads it.
        let gone: Vec<u32> =
            old.iter().map(|e| e.owner).filter(|o| !owners.iter().any(|e| e.owner == *o)).collect();
        if !gone.is_empty() {
            locked(&MENU_PASS).retain(|p| !gone.contains(&p.0));
        }
    }

    /// The window in front, which the hook compares with a live `GetForegroundWindow` at every
    /// key. Windows answers with no window for a moment while the foreground changes hands; the
    /// scope is then every window, as it is on a Mac when neither the application nor the tap
    /// names one, and said as it is there.
    fn resolve_key_scope(&self) -> isize {
        // SAFETY: no arguments; reads which window is in front.
        let window = unsafe { GetForegroundWindow() as isize };
        if window == 0 {
            crate::logging::line(
                "keys",
                "key scope: no window was in front when asked, so the scope is every window for now",
            );
        }
        window
    }

    fn key_front(&self) -> isize {
        // SAFETY: as above.
        unsafe { GetForegroundWindow() as isize }
    }

    /// `GetAsyncKeyState` for the key and the modifiers, the hook's own record for a key whose
    /// key-down it kept from the system ([`KEPT_BACK`]), which the system cannot report, a screen
    /// reader's key as the hook tells it — its own record or the key down, as for a captured key —
    /// and the hook's count of keys let through ([`LET_THROUGH`]). The key by its virtual key: both
    /// Enters are `VK_RETURN` to the system, held or not, NumpadEnter as well ([`os_key`]).
    fn keyboard_now(&self, vk: u32, _mask: u8, _phys: Option<u16>) -> super::KeyboardNow {
        // SAFETY: reads which window is in front and the asynchronous key state.
        unsafe {
            let down = |k: i32| (GetAsyncKeyState(k) as u16 & 0x8000) != 0;
            let mut mods = 0;
            for (role, held) in [
                (super::MASK_SHIFT, down(VK_SHIFT as i32)),
                (super::MASK_CTRL, down(VK_CONTROL as i32)),
                (super::MASK_ALT, down(VK_MENU as i32)),
                (super::MASK_WIN, down(VK_LWIN as i32) || down(VK_RWIN as i32)),
            ] {
                if held {
                    mods |= role;
                }
            }
            super::KeyboardNow {
                front: Some(GetForegroundWindow() as isize),
                key_held: down(os_key(vk).0 as i32) || KEPT_BACK.is_held(vk),
                mods,
                reader_mod: SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed) || down(0x2D) || down(0x60) || down(0x14),
                let_through: LET_THROUGH.load(Ordering::Relaxed),
            }
        }
    }

    /// One `SendInput` of every event, marked with [`PASS_ON_MARK`] — see [`pass_on_inputs`].
    fn pass_on_key(&self, vk: u32, _mask: u8, phys: Option<u16>, strokes: &[super::Stroke]) -> Result<(), String> {
        let sent = send_marked(&pass_on_inputs(vk, phys, strokes, scan_code));
        if sent == strokes.len() {
            return Ok(());
        }
        // Taken in part — a program holding the input, or a window of a higher integrity level in
        // front, which Windows does not say: what that part pressed is released, so that nothing
        // stays down.
        let release = super::left_down(strokes, sent);
        let released = send_marked(&pass_on_inputs(vk, phys, &release, scan_code));
        Err(format!(
            "SendInput took {sent} of its {} key events{}",
            strokes.len(),
            if release.is_empty() {
                String::new()
            } else {
                format!(", and {released} of the {} that release what those pressed", release.len())
            }
        ))
    }

    fn modifiers_down(&self) -> bool {
        unsafe {
            let down = |k: u16| (GetAsyncKeyState(k as i32) as u16 & 0x8000) != 0;
            down(VK_MENU) || down(VK_CONTROL) || down(VK_SHIFT) || down(VK_LWIN) || down(VK_RWIN)
        }
    }

    /// `GetDoubleClickTime`: the user's setting, read afresh — one call into user32, no message.
    /// Windows answers 500 ms for a setting of 0 and at most 5000 ms; 0 is not expected, and is
    /// read as that default all the same.
    fn double_click_time(&self) -> std::time::Duration {
        use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
        match unsafe { GetDoubleClickTime() } {
            0 => super::DEFAULT_DOUBLE_CLICK,
            ms => std::time::Duration::from_millis(u64::from(ms)),
        }
    }

    /// Set by `pump_pending` just before it takes [`HOTKEY_QUEUE`]; [`KEY_QUEUE`] is taken after it
    /// in the same pump, so a key the hook queued before the moment is in that pump too. A
    /// `WM_HOTKEY` posted before it was dispatched by then: posted messages are retrieved before the
    /// timer that drives the pump. A hotkey press is timed on the tick clock, a few milliseconds
    /// apart from this one, which the count's own rule covers (taps.rs, `take_due`).
    fn input_taken_at(&self) -> Option<Instant> {
        INPUT_TAKEN.with(Cell::get)
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

    fn take_menu_pass_through(&self, owner: u32) -> Vec<(u32, u8)> {
        super::take_menu_passes(&mut locked(&MENU_PASS), owner)
    }

    /// The captured keys the hook queued during the stall, and the hotkey presses the hook matched
    /// or `RegisterHotKey` delivered meanwhile — each settled as if delivered, so the pairing of a
    /// hook press with its `WM_HOTKEY` stays right, and counted when it would have been dispatched.
    /// A `WM_HOTKEY` still in the thread's message queue is dropped as it arrives (`pump_pending`).
    /// What the pump took into its own batch before the stall began is not here: pressed before
    /// it, it still arrives, late.
    fn drop_queued_input(&self) -> crate::backend::DroppedInput {
        let keys: Vec<(u32, u8, u32)> =
            std::mem::take(&mut *locked(&KEY_QUEUE)).into_iter().map(|k| (k.vk, k.mask, k.owner)).collect();
        let pending: Vec<(i32, Route, Through)> = locked(&HOTKEY_QUEUE).drain(..).collect();
        let mut hotkeys = Vec::new();
        for (id, route, _) in pending {
            if settle_hotkey(id, route).is_some() {
                hotkeys.push(id);
            }
        }
        // SAFETY: a plain call.
        let now = unsafe { GetTickCount() };
        DROP_OS_UNTIL.store(DROP_ARMED | u64::from(now), Ordering::Release);
        crate::backend::DroppedInput { keys, hotkeys }
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
                // treats any newcomer after a menu-opening click as the menu: the click leaves
                // the pointer on the control, and a toolkit that shows its tip anyway would
                // count as a menu that is not there. Win32's class and WinForms' wrapper of it
                // both carry the name; Qt's tooltip window class carries "ToolTip".
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
    /// see `keyboard_hook_thread` — and then the watch that installs it again whenever Windows
    /// may have removed it (`hook_watch_thread`). Idempotent: every capture and every granted
    /// hotkey asks.
    fn watch_keys(&self) -> Result<(), HostError> {
        if KEY_HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
            return Ok(()); // already installed
        }
        // The thread the hook wakes when it has queued something: this one, the pump.
        PUMP_THREAD.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<u32, HostError>>(1);
        let result = match std::thread::Builder::new()
            .name("keyboard-hook".to_string())
            .spawn(move || keyboard_hook_thread(ready_tx))
        {
            Err(e) => Err(HostError::with(&errors::keys::HOOK_THREAD, format!("could not start the keyboard hook's thread: {e}"))),
            // It answers once, straight after SetWindowsHookExW, which does not block; a
            // sender dropped unanswered means the thread ended first, and is a failure too.
            Ok(_) => ready_rx.recv().unwrap_or_else(|_| Err(HostError::of(&errors::keys::HOOK_ENDED))),
        };
        match &result {
            Ok(hook_thread) => {
                crate::logging::line(
                    "keys",
                    "the keyboard hook is installed, on a thread of its own that does nothing but \
                     answer it, so a busy main thread does not hold up the keyboard",
                );
                hook_watch_thread::start(*hook_thread);
            }
            Err(_) => KEY_HOOK_INSTALLED.store(false, Ordering::SeqCst),
        }
        result.map(|_| ())
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
        // What the operating system did to the session — a sleep, an unlock, a display change —
        // first, so the window events after it are handled against a world already re-checked.
        // Desktop duplication's back-off and hang count are this backend's own to reset, before
        // anything asks it for a picture again — in full after a resume, an unlock or a
        // connection, and bounded when the batch holds nothing but display and scale changes,
        // which a display whose link keeps dropping sends every few seconds.
        let system = crate::system_events::take();
        if !system.is_empty() {
            if system.iter().any(|s| s.event.rechecks_capture()) {
                dxgi::system_changed(system.iter().any(|s| s.event.rechecks_capture() && !s.event.may_storm()));
            }
            events.on_system(system);
        }
        // Before either queue is taken: a press the hook or the window proc queued before this
        // moment is handed over in this pump (`input_taken_at`).
        INPUT_TAKEN.with(|t| t.set(Some(Instant::now())));
        let hotkeys: Vec<(i32, Route, Through)> = locked(&HOTKEY_QUEUE).drain(..).collect();
        // After a stop: a `RegisterHotKey` press made during the stall it ended reaches the queue
        // only now, and is dropped by its stamp, as the rest of the stall's presses were. Once, on
        // the first pump after the stop: posted messages are retrieved before the timer that
        // drives this pump, so they are all here by now.
        let drop_until = DROP_OS_UNTIL.swap(0, Ordering::AcqRel);
        let mut late = Vec::new();
        for (id, route, (seq, taps_seq)) in hotkeys {
            let stalled = drop_until & DROP_ARMED != 0
                && matches!(route, Route::Os { time } if super::tick_not_after(time, drop_until as u32));
            if let Some(own) = settle_hotkey(id, route) {
                if stalled {
                    late.push(id);
                } else {
                    events.on_hotkey(id, super::HotkeyPress { seq, taps_seq, own, at: tick_instant(route.time()) });
                }
            }
        }
        if !late.is_empty() {
            events.on_stall_hotkeys_dropped(late);
        }
        let pending: Vec<isize> = FOREGROUND_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        let mut unmatched_fg = false;
        for hwnd in pending {
            // A landing of host.window.focus looks at its window's foreground event, and ends at
            // another window's (HW1).
            landing_foreground(hwnd);
            match window_info(hwnd, true) {
                Some(win) => events.on_window_activate(win),
                // Foreground, but not matchable yet — remember to re-check shortly.
                None => unmatched_fg = true,
            }
        }
        let pending_keys: Vec<Taken> = std::mem::take(&mut *locked(&KEY_QUEUE));
        for k in pending_keys {
            events.on_key(k.vk, k.mask, k.owner, k.repeat, k.pressed);
        }
        report_capture_passes();
        report_arrow_notes();
        // Game controllers, from the hub their own thread feeds (never a thread-local: that
        // thread is not this one). After the activations, so a press is handled against the
        // window that is in front now.
        super::gamepad::drain_into(events);
        if FOCUS_DIRTY.with(|f| f.replace(false)) {
            // And at every focus event (HW1).
            landing_look();
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

// ── The keyboard after host.window.focus (HW1) ──────────────────────────────────────────────
// `host.window.focus(id)` of a control brings its window forward and then LANDS the keyboard on
// the control, and a page leaving the front gives the keyboard back to the control it took the
// front from (webview_win.rs). What a landing decides is `focus_landing.rs`'s, pure; here are its
// looks — `GetGUIThreadInfo` of the window's thread, local, no message sent — on the call's own
// turn when the window is in front already, at the window's foreground event and at every focus
// event the pump drains; and its puts, which the host asks on the accessibility thread
// (`take_focus_puts`, lib.rs `fire_focus_puts`, uia.rs `element_focus_put`), never here. Nothing
// here calls `AttachThreadInput`. One landing at a time: a newer one ends it. Logged under
// `[window]`.

/// The landing under way: the decisions and what they are about.
struct Landed {
    id: u64,
    landing: crate::focus_landing::Landing,
    control: isize,
    root: isize,
    thread: u32,
    /// `focus of control <id> ('<class>') in '<title>'`, the head of its log lines.
    head: String,
}

thread_local! {
    /// The landing under way, if any.
    static LANDING: RefCell<Option<Landed>> = const { RefCell::new(None) };
    /// The puts landings asked for, for the host to ask on the accessibility thread.
    static FOCUS_PUTS: RefCell<Vec<(u64, isize)>> = const { RefCell::new(Vec::new()) };
}

/// Landings started, for their ids.
static LANDINGS: AtomicU64 = AtomicU64::new(0);
/// `host.input` calls made ([`Backend::input_made`]): a landing ends at the first after its start.
static INPUT_CALLS: AtomicU64 = AtomicU64::new(0);

/// Window `hwnd`'s class, for a log line.
fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: a buffer of its stated length; a stale handle answers 0.
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Window `hwnd`'s title, for a log line.
fn title_text(hwnd: HWND) -> String {
    let mut buf = [0u16; 512];
    // SAFETY: as `class_name`.
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// Starts a landing on `control` inside `root` (`focus_landing::plan`): one under way ends, "a
/// newer focus". `look_now`: the window is in front already, and no foreground event will come. For
/// [`Backend::focus_window`] and for a page giving the front back (webview_win.rs).
pub(crate) fn start_landing(control: isize, root: isize, look_now: bool) {
    let head = format!("focus of control {control} ('{}') in '{}'", class_name(control as HWND), title_text(root as HWND));
    // SAFETY: a plain query; 0 for a window that is gone, whose looks then end the landing.
    let thread = unsafe { GetWindowThreadProcessId(root as HWND, std::ptr::null_mut()) };
    let id = LANDINGS.fetch_add(1, Ordering::Relaxed) + 1;
    let landing = crate::focus_landing::Landing::new(crate::focus_landing::KEY_DOWNS.count(), INPUT_CALLS.load(Ordering::Relaxed));
    LANDING.with(|cell| {
        let mut slot = cell.borrow_mut();
        if let Some(old) = slot.as_mut() {
            old.landing.newer();
            say_landing(old);
        }
        *slot = Some(Landed { id, landing, control, root, thread, head });
    });
    if look_now {
        landing_look();
    }
}

/// The lines a landing owes, under `[window]`.
fn say_landing(l: &mut Landed) {
    for line in l.landing.take_lines() {
        crate::logging::line("window", &format!("{}: {line}", l.head));
    }
}

/// One look at the landing's window (see the section's head). A put it asks for is queued for the
/// host; a landing that is over is let go.
fn landing_look() {
    LANDING.with(|cell| {
        let mut slot = cell.borrow_mut();
        let Some(l) = slot.as_mut() else { return };
        // SAFETY: plain queries about handles; stale ones answer no.
        let look = unsafe { gui_look(l.root, l.control, l.thread) };
        let step = l.landing.look(&look);
        if step == crate::focus_landing::Step::Put {
            FOCUS_PUTS.with(|p| p.borrow_mut().push((l.id, l.control)));
        }
        say_landing(l);
        if matches!(step, crate::focus_landing::Step::Ended(_)) {
            *slot = None;
        }
    });
}

/// What the root's GUI thread says: its active window, the window with the keyboard, and whether
/// the control is still a visible window inside the root.
unsafe fn gui_look(root: isize, control: isize, thread: u32) -> crate::focus_landing::Look {
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsChild, IsWindow};
    let mut gti: GUITHREADINFO = std::mem::zeroed();
    gti.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
    let read = thread != 0 && GetGUIThreadInfo(thread, &mut gti) != 0;
    let focus = if read { gti.hwndFocus } else { std::ptr::null_mut() };
    let (root_h, control_h) = (root as HWND, control as HWND);
    crate::focus_landing::Look {
        activated: read && gti.hwndActive == root_h,
        on_control: !focus.is_null() && (focus == control_h || IsChild(control_h, focus) != 0),
        focus_class: (!focus.is_null()).then(|| class_name(focus)),
        control_there: IsWindow(control_h) != 0
            && IsWindowVisible(control_h) != 0
            && (control_h == root_h || IsChild(root_h, control_h) != 0),
        key_downs: crate::focus_landing::KEY_DOWNS.count(),
        input_calls: INPUT_CALLS.load(Ordering::Relaxed),
    }
}

/// Window `hwnd` became the foreground window: the landing's own window is a look; another window
/// ends the landing.
fn landing_foreground(hwnd: isize) {
    let ours = LANDING.with(|cell| {
        let mut slot = cell.borrow_mut();
        let l = slot.as_mut()?;
        if l.root == hwnd {
            return Some(true);
        }
        l.landing.foreground(false);
        say_landing(l);
        *slot = None;
        Some(false)
    });
    if ours == Some(true) {
        landing_look();
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

/// The moment a hotkey press was made, from its stamp on the tick clock — the key event's (`time`
/// of the hook's report) or the message's (`GetMessageTime`): now, less how long ago the stamp
/// is. What the host times quick repeats by (`HotkeyPress::at`), and not when the pump got to the
/// press, which a busy event loop delays. A stamp after the clock, read a moment apart, is now.
fn tick_instant(time: u32) -> Instant {
    let ago = unsafe { GetTickCount() }.wrapping_sub(time);
    if ago >= 0x8000_0000 {
        return Instant::now();
    }
    let now = Instant::now();
    now.checked_sub(std::time::Duration::from_millis(u64::from(ago))).unwrap_or(now)
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
        locked(&HOTKEY_QUEUE).push((wparam as i32, Route::Os { time }, through()));
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

/// Captured keys the hook let through because a menu was open, as `(module, vk, mask)` — each
/// module's own record, read by `host.keys.passedThrough` (`super::file_menu_pass`). A mutex
/// rather than a thread-local because the hook runs on its own thread and the reader is the pump.
static MENU_PASS: Mutex<Vec<(u32, u32, u8)>> = Mutex::new(Vec::new());

fn note_menu_pass(owner: u32, vk: u32, mask: u8) {
    super::file_menu_pass(&mut locked(&MENU_PASS), owner, vk, mask);
}

/// Why the hook let a captured key-down through to the application instead of taking it. The
/// three that [`super::capture_decision`] decides for the hook, and the hook's own screen-reader
/// rule; each is an index into [`PASS_SAID_FOR`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PassWhy {
    /// The hook's own record says a screen reader's modifier is held.
    ReaderRecorded = 0,
    /// The system says a screen reader's modifier is down (`GetAsyncKeyState`).
    ReaderDown = 1,
    /// Every capture of it belongs to a module scoped to a window that is not in front.
    OutOfScope = 2,
    /// The window in front is in menu mode (`GetGUIThreadInfo`).
    MenuMode = 3,
    /// A module whose flag counts for the window in front declared a menu open
    /// (`host.keys.menuOpen`).
    MenuFlag = 4,
}

impl PassWhy {
    /// The hook's reason for what [`super::capture_decision`] decided. `None` for
    /// [`super::PassWhy::OwnWindow`], which the hook never hears: it tells the decision that none
    /// of this application's windows has the keyboard, since it is out of the chain while one has.
    fn of(why: super::PassWhy) -> Option<PassWhy> {
        match why {
            super::PassWhy::OutOfScope => Some(PassWhy::OutOfScope),
            super::PassWhy::MenuMode => Some(PassWhy::MenuMode),
            super::PassWhy::MenuFlag => Some(PassWhy::MenuFlag),
            super::PassWhy::OwnWindow => None,
        }
    }
}

/// A captured key-down the hook let through, and the state it decided by.
#[derive(Clone, Copy, Debug)]
struct PassNote {
    vk: u32,
    mask: u8,
    why: PassWhy,
    foreground: isize,
    /// The scope of the module of the earliest capture of the key, 0 for everywhere.
    scope: isize,
    /// For [`PassWhy::ReaderRecorded`]: how long ago the hook saw the modifier go down.
    reader_ms: u32,
}

/// Captured key-downs the hook let through, for the pump to explain in the log
/// ([`report_capture_passes`]). Every one of these rules exists for a reason, and each can also
/// be left standing by a state that went stale — a screen reader's modifier whose key-up went
/// elsewhere, a scope pinned to a window no longer in front, a menu flag nobody cleared — and
/// then the overlay looks dead while nothing says why. Without these lines the log of such a
/// session could not be told from one whose keyboard hook Windows had removed.
static CAPTURE_PASS: Mutex<Vec<PassNote>> = Mutex::new(Vec::new());
const CAPTURE_PASS_MAX: usize = 32;

fn note_capture_pass(note: PassNote) {
    if let Ok(mut m) = CAPTURE_PASS.lock() {
        if m.len() < CAPTURE_PASS_MAX {
            m.push(note);
        }
    }
}

thread_local! {
    /// (The pump.) The window in front each reason in [`PassWhy`] was last explained for, so
    /// that each is said once per window rather than once per key. Forgotten when the captured
    /// set is emptied, so the next overlay's first pass is said again even in the same window.
    static PASS_SAID_FOR: Cell<[Option<isize>; 5]> = const { Cell::new([None; 5]) };
}

/// Writes what [`CAPTURE_PASS`] gathered: one line per reason and window in front, for as long
/// as keys stay captured. On the pump, which drains the hook's other queues too.
fn report_capture_passes() {
    let notes = CAPTURE_PASS.lock().map(|mut m| std::mem::take(&mut *m)).unwrap_or_default();
    for n in notes {
        let first = PASS_SAID_FOR.with(|said| {
            let mut s = said.get();
            let first = s[n.why as usize] != Some(n.foreground);
            s[n.why as usize] = Some(n.foreground);
            said.set(s);
            first
        });
        if first {
            crate::logging::line("keys", &capture_pass_line(&n));
        }
    }
}

fn capture_pass_line(n: &PassNote) -> String {
    let key = super::vk_name(n.vk).unwrap_or_else(|| "a key".to_string());
    let why = match n.why {
        PassWhy::ReaderRecorded => format!(
            "the hook has a screen reader's modifier (Insert, numpad 0 or Caps Lock) recorded as \
             held — it saw one go down {} ms ago and has not seen it go up — and a keystroke made \
             with it is the screen reader's",
            n.reader_ms
        ),
        PassWhy::ReaderDown => "a screen reader's modifier (Insert, numpad 0 or Caps Lock) is down \
                                as far as the system knows, and a keystroke made with it is the \
                                screen reader's"
            .to_string(),
        PassWhy::OutOfScope => format!(
            "every capture of it is scoped to another window (host.keys.scope) — the earliest to \
             window {:#x} — and window {:#x} is in front",
            n.scope, n.foreground
        ),
        PassWhy::MenuMode => "the window in front is in menu mode (a Win32 menu is open)".to_string(),
        PassWhy::MenuFlag => "a module scoped to the window in front, or to every window, has \
                              declared a menu open (host.keys.menuOpen(true))"
            .to_string(),
    };
    format!(
        "captured {key} (vk {:#04x}/m{}) was let through to the application: {why}. Said once \
         per reason and window in front while keys are captured",
        n.vk, n.mask
    )
}

/// Whether the hook swallows a captured key it matched in scope, with no menu open: a key-down
/// always, a key-up only when its key-down was swallowed too. `down_reached_system` asks
/// `GetAsyncKeyState`, which inside a low-level hook does not yet include the event in hand —
/// so for a key-up it says whether the key-down went through to the system. A key-down the hook
/// swallowed never set it.
///
/// Why it matters: a key-down that got past the hook — pressed before the capture was made,
/// while a menu was open, or while Windows had removed the hook and the watch was installing it
/// again — reached the application. Swallowing its key-up would leave the application holding
/// the key, and the system too: the key would read as down everywhere, and its next press
/// would arrive as a repeat, for as long as the capture stood.
fn swallow_captured(is_down: bool, down_reached_system: impl FnOnce() -> bool) -> bool {
    is_down || !down_reached_system()
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
/// **Installed again, here, when the watch asks.** Windows removes a low-level hook that timed
/// out without telling it; the watch (`hook_watch_thread`) asks after every resume and unlock,
/// and when raw input shows the hook has stopped being called (`hook_watch`), and posts
/// `hook_watch_thread::WM_APP_REHOOK` to this thread, and [`reinstall`] swaps the hook.
///
/// **Out of the chain while one of this application's windows is in front.** Windows calls
/// neither the hook nor the hooks behind it for keys going to this process's windows
/// (`hook_watch::Place`), so the watch posts `hook_watch_thread::WM_APP_UNHOOK` while one of them
/// is in front, and [`take_out`] takes the hook out; while a window of another program is,
/// `WM_APP_REHOOK` again, and [`reinstall`] installs it, first in the chain.
///
/// Those two are the only messages ever posted to this thread, and the one other thing it does, a
/// few times in a session at most, or once per switch between this application's windows and
/// another program's. After each, where the hook is goes into an atomic the watch reads before it
/// asks for a move, and the answer goes back to the watch, which writes the log line: no file is
/// written on this thread. Every time a hook goes in, at start as well, the keys the system holds
/// down then are recorded as held before any key reaches it ([`seed_held_keys`]).
///
/// It never ends, and never needs to: Windows removes the hook with the process. It answers
/// `ready` with its thread id, for the watch to post to.
fn keyboard_hook_thread(ready: std::sync::mpsc::SyncSender<Result<u32, HostError>>) {
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
        let hmod = GetModuleHandleW(std::ptr::null());
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), hmod, 0);
        if hook.is_null() {
            let error = GetLastError();
            let refused = format!("SetWindowsHookExW(WH_KEYBOARD_LL) failed (error {error})");
            let _ = ready.send(Err(HostError::with(&errors::keys::HOOK_REFUSED, refused)));
            return;
        }
        HOOK.with(|h| h.set(hook));
        seed_held_keys();
        let _ = ready.send(Ok(GetCurrentThreadId()));
        drop(ready);
        // GetMessageW is where Windows delivers the hook's calls, as sent messages, and it
        // returns only for a posted one — which is the watch's request to install the hook
        // again or to take it out, and nothing else.
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.hwnd.is_null() && msg.message == hook_watch_thread::WM_APP_REHOOK {
                reinstall(hmod, msg.wParam);
                continue;
            }
            if msg.hwnd.is_null() && msg.message == hook_watch_thread::WM_APP_UNHOOK {
                take_out(msg.wParam);
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

thread_local! {
    /// (The hook's thread.) The hook currently installed, for taking it out when a new one
    /// replaces it; null while it is out of the chain ([`take_out`]).
    static HOOK: Cell<HHOOK> = const { Cell::new(std::ptr::null_mut()) };
    /// (The hook's thread.) Hooks of ours an earlier re-install could not take out, tried again
    /// at every later one — see `hook_watch::swap`. Empty unless `UnhookWindowsHookEx` refused
    /// twice in one re-install, which nothing documented makes it do.
    static STALE_HOOKS: RefCell<Vec<HHOOK>> = const { RefCell::new(Vec::new()) };
}

/// Installs the hook again, on the hook's thread, for the reason the watch gave (`reason`, as
/// encoded), and reports the outcome back to the watch.
///
/// **New first, then old out** (`hook_watch::swap`, where the order is tested). Between the
/// calls this thread handles no message, so no hook call is delivered to it: a key event that
/// arrives in between waits for the NEW hook, which is first in the chain, and when that call
/// returns through `CallNextHookEx` the old hook is already out of the chain — so no key is
/// handled twice, and none passes with no hook of ours at all. If the new one cannot be
/// installed, the old one stays: it may still work. If the old one cannot be taken out, the
/// new one is taken out again, rather than leave two of ours handling every key twice.
///
/// **What carries over.** Everything the application set — the captured set, the granted
/// hotkeys, the modules' scopes and menu flags — is in statics the new hook reads exactly as the
/// old one did. What the old hook remembered of keys going by is forgotten
/// ([`forget_seen_keys`]) only when Windows had removed it: a hook still installed saw every
/// key-up, and forgetting its record would drop a screen reader's modifier held across the swap.
///
/// **Back from out of the chain** ([`take_out`]) there is no old hook: the new one simply goes in,
/// first in the chain, and nothing is forgotten — that was done as it went out.
///
/// **What is held now.** Whenever the new hook is in, the keys the system holds down at that
/// moment are recorded as held ([`seed_held_keys`]), before this thread handles another message
/// and so before any key reaches the new hook: their key-downs went by while no hook of ours was
/// called, and their auto-repeat is no new press.
unsafe fn reinstall(hmod: HMODULE, reason: usize) {
    let current = HOOK.with(|h| h.get());
    let (now, outcome) = STALE_HOOKS.with(|stale| {
        hook_watch::swap(
            (!current.is_null()).then_some(current),
            &mut stale.borrow_mut(),
            || {
                let new = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), hmod, 0);
                if new.is_null() {
                    Err(GetLastError())
                } else {
                    Ok(new)
                }
            },
            |hook| if UnhookWindowsHookEx(hook) != 0 { Ok(()) } else { Err(GetLastError()) },
        )
    });
    HOOK.with(|h| h.set(now.unwrap_or(std::ptr::null_mut())));
    forget_after(outcome);
    if outcome.installed() {
        seed_held_keys();
    }
    hook_watch_thread::report(reason, outcome, now.is_some());
}

/// Takes the hook out of the chain, on the hook's thread, while one of this application's windows
/// is in front (`reason` is `hook_watch::Reason::Out`, as encoded), and reports the outcome back to
/// the watch (`hook_watch::take_out`, where it is tested). The hooks an earlier swap could not take
/// out go with it. A hook `UnhookWindowsHookEx` refuses stays in the chain, and the next
/// [`reinstall`] swaps it.
///
/// **What carries over** is what a re-install carries over: the captured set, the granted hotkeys,
/// the modules' scopes and menu flags stay as they are, for the hook that comes back. What the
/// hook recorded of keys going by is forgotten ([`forget_seen_keys`]), since it sees no key-up
/// until it is back; the keys owed a key-up for a screen reader stay owed. The hook that comes
/// back records the keys held then ([`reinstall`]).
unsafe fn take_out(reason: usize) {
    let current = HOOK.with(|h| h.get());
    let (now, outcome) = STALE_HOOKS.with(|stale| {
        hook_watch::take_out((!current.is_null()).then_some(current), &mut stale.borrow_mut(), |hook| {
            if UnhookWindowsHookEx(hook) != 0 {
                Ok(())
            } else {
                Err(GetLastError())
            }
        })
    });
    HOOK.with(|h| h.set(now.unwrap_or(std::ptr::null_mut())));
    forget_after(outcome);
    hook_watch_thread::report(reason, outcome, now.is_some());
}

/// (The hook's thread.) What the hook forgets once its thread has swapped it or taken it out:
/// what it recorded of keys going by, when the outcome says it may have missed their key-ups
/// (`hook_watch::Outcome::forget_seen_keys`) — Windows had removed the old one, or it went out of
/// the chain — and nothing otherwise.
fn forget_after(outcome: hook_watch::Outcome) {
    if outcome.forget_seen_keys() {
        forget_seen_keys();
    }
}

/// (The hook's thread.) What a re-installed hook forgets of the keys the old one saw go by when
/// Windows had removed the old one, and what a hook taken out of the chain forgets as it goes
/// ([`take_out`]) — the ones whose key-up it missed or will miss, and which would otherwise stay
/// wrong for good:
///
/// - a **screen reader's modifier** seen going down and never up, and a **pending modifier
///   tap** ([`forget_keys_held_out_of_sight`]);
/// - the **modifiers as the hook saw them go by**, which a late call is judged by (a call on
///   time asks the system and brings the record in line anyway).
///
/// Everything else carries over as it is. The captured set, the granted hotkeys, and the modules'
/// scopes and menu flags are the application's. The hotkeys' record of held keys stays: a
/// hotkey the hook dispatched kept its key-down from the system, which therefore cannot say later
/// that the key is held, and forgetting the record would make the repeat of a hotkey held across
/// the swap fire it a second time. A key-up the hook misses meanwhile ends the record all the same
/// when raw input reports it ([`key_up_reported`]), and a press remembered for longer than the
/// auto-repeat threshold counts as released anyway (see `hotkey_hook::REPEAT_MS`). The keys owed
/// their key-up for a screen reader (`SCREEN_READER_PASSED`) stay owed: letting a stray key-up
/// through is harmless, swallowing an owed one leaves the application holding a key. What the
/// system holds down as a hook goes in again is recorded afresh ([`seed_held_keys`]).
fn forget_seen_keys() {
    forget_keys_held_out_of_sight();
    HOOK_MODS.with(|m| m.set(Mods::default()));
}

/// (The hook's thread, as a hook of ours goes into the chain: at start, and after every re-install
/// that put one in.) Records the keys the system holds down now as held, so that a key whose
/// key-down went by while no hook of ours was called — out of the chain while one of this
/// application's windows was in front, removed by Windows, not installed yet — is not taken for
/// a new press when its auto-repeat reaches the hook, and its key-up ends it. A call on time asks
/// the system about its own key as well (`hotkey_hook::held_before`), which also covers a hook
/// that stayed in the chain while an elevated window or the secure desktop was in front; this is
/// the record for a call that comes late, which cannot ask, and for the modifiers. Seen live on
/// 2026-10-11: Ctrl+Alt+Shift+Win+F7 pressed in the HTML probe's page reached the application
/// through `RegisterHotKey`, its callback hid the page, the hook came back into the chain while F7
/// was still held, and F7's next repeat fired the hotkey again — the page came straight back.
///
/// Asked of the system (`GetAsyncKeyState`, which knows every key-down that reached it), each key
/// once:
///
/// - the **modifiers**, side by side, are taken as the system has them: the record a late call is
///   judged by, and the one that tells a held modifier's repeat from a press, which arms a
///   modifier tap (`hotkey_hook::Mods`). The hook never keeps a modifier's key-down from the
///   system, so the system's answer is the whole of it, and replaces the record;
/// - **every other key** the system holds down, but the mouse buttons (`hotkey_hook::seeds`), is
///   added to what the captured keys ([`CAPTURED_HELD`]) and the hotkeys
///   (`hotkey_hook::Table::seed_held`) record as held: a taken key-down of one is its auto-repeat,
///   and a hotkey's key repeating is let through to `RegisterHotKey`, whose `MOD_NOREPEAT` knows
///   the key is down, and never fires here. Every key, not only those captured or filed now: an
///   overlay gives its keys back and takes them again on every focus move, which is the moment the
///   hook comes back too. Only added to, never cleared: a key-down the hook swallowed never reached
///   the system, so a key the system holds up may still be held, and that record ends at its
///   key-up as before.
///
/// A screen reader's modifier needs nothing: its every key-down, repeats included, sets its record
/// again, and the system is asked about it at every key as well. 256 calls, once per switch, and
/// before this thread handles another message: no key waits for a hook that does not know yet
/// what is held.
fn seed_held_keys() {
    // SAFETY: reads the tick clock, and the system's state of one key at a time.
    let now = unsafe { GetTickCount() };
    seed_held_keys_with(now, |vk| unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 });
}

/// [`seed_held_keys`] with the system's answer passed in: `down` says whether a virtual key is
/// held down now, asked once for each, and `now` is the tick the records start from.
fn seed_held_keys_with(now: u32, down: impl Fn(u32) -> bool) {
    let mut asked = [false; 256];
    for (vk, held) in asked.iter_mut().enumerate() {
        *held = down(vk as u32);
    }
    let held = |vk: u32| asked.get(vk as usize).copied().unwrap_or(false);
    HOOK_MODS.with(|m| m.set(Mods::held_now(held)));
    for vk in (0..256).filter(|&vk| hotkey_hook::seeds(vk) && held(vk)) {
        CAPTURED_HELD.hold(vk);
    }
    if HOOK_HOTKEYS_PRESENT.load(Ordering::Relaxed) {
        if let Some(t) = locked(&HOOK_HOTKEYS).as_mut() {
            t.seed_held(now, held);
        }
    }
}

/// (The keyboard watch's thread.) A physical key-up raw input reported, of virtual key `vk`,
/// stamped `time`: it ends the press the hotkeys' record holds for that key when the press went
/// down no later than that (`hotkey_hook::Table::up_reported`). The hook is not called for a key
/// going to one of this application's windows, nor while it is out of the chain, and a press it
/// swallowed and whose key went up there would otherwise stay held until the auto-repeat
/// threshold — the next press of that key in another program's window swallowed as its repeat,
/// with no callback — and a press `RegisterHotKey` delivers meanwhile would be taken for its
/// repeat. Reported for every key-up, the ones the hook saw too: the press's time keeps a report
/// that comes late from ending a later press. One lock the hook takes for every key as well, held
/// for one array access; nothing while no hotkey was ever granted.
pub(super) fn key_up_reported(vk: u32, time: u32) {
    if HOOK_HOTKEYS_PRESENT.load(Ordering::Relaxed) {
        if let Some(t) = locked(&HOOK_HOTKEYS).as_mut() {
            t.up_reported(vk, time);
        }
    }
}

/// The keyboard went where the hook is not called — a window of a higher integrity level or one
/// of this application's own came to the front, the session was locked, disconnected or
/// suspended, or Windows removed the hook — so a key the hook saw go down may go up unseen.
/// Forgets what the hook recorded as held from key-downs alone: a **screen reader's modifier**
/// (else every captured key would be let through to the screen reader until that modifier next
/// went up in front of a window the hook sees), a **pending modifier tap** (else a tap could fire
/// at a release that ends a combination) and the **captured keys held down** (else the next
/// deliberate press of one would read as the keyboard's auto-repeat, [`CAPTURED_HELD`], and a key
/// its busy module let go of would not be passed on to the program until that key next went up,
/// [`KEPT_BACK`]).
/// Returns how long ago the screen reader's modifier was seen going down, when it was recorded as
/// held, for the watch's log line.
///
/// Any thread: a handful of atomics. The watch calls it on those events; the hook's thread after
/// a re-install that found the old hook gone, and after taking the hook out of the chain. The
/// cost of forgetting too much is one keystroke made while a screen reader's modifier was held
/// across such a moment, which is then taken by a capture of it instead of going to the screen
/// reader, the repeat of a captured key held across it, which reads as a new press, and a key
/// passed on to the program while it is still held across it — `GetAsyncKeyState` is asked as
/// well, and says so for every key whose key-down reached the system.
pub(super) fn forget_keys_held_out_of_sight() -> Option<u32> {
    TAP_ARMED.store(0, Ordering::Relaxed);
    CAPTURED_HELD.clear();
    KEPT_BACK.clear();
    ARROW_NOTED_HELD.store(0, Ordering::Relaxed);
    SCREEN_READER_MOD_OTHER.store(false, Ordering::Relaxed);
    if SCREEN_READER_MOD_DOWN.swap(false, Ordering::Relaxed) {
        // SAFETY: reads the tick clock.
        let now = unsafe { GetTickCount() };
        Some(now.wrapping_sub(SCREEN_READER_MOD_DOWN_AT.load(Ordering::Relaxed)))
    } else {
        None
    }
}

/// Wakes the pump after the hook — or the keyboard watch, with a system event — queued something
/// for it. Nothing when the loop has not said which thread it is yet.
pub(super) fn wake_pump() {
    let tid = PUMP_THREAD.load(Ordering::Relaxed);
    if tid != 0 {
        unsafe {
            PostThreadMessageW(tid, WM_NULL, 0, 0);
        }
    }
}

/// Runs on the hook's own thread (`keyboard_hook_thread`) for every key event on the machine.
unsafe extern "system" fn ll_keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Before anything, whatever the code: counted for the watch's re-install line. One relaxed add.
    hook_watch_thread::HOOK_ENTERED.fetch_add(1, Ordering::Relaxed);
    if code == HC_ACTION as i32 {
        // Then, for every event of every kind, before anything is read: the watch compares when the
        // hook was last called with the key-downs raw input saw (`hook_watch`). One relaxed store.
        let now = GetTickCount();
        hook_watch_thread::note_hook_call(now);
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        // A key this application sends to pass a key on to the program in front (`pass_on_key`):
        // through, before anything is recorded or matched. No capture takes it again, it is no
        // press, hold or modifier tap of the user's, and it is not counted as let through
        // (`LET_THROUGH`). Only the key's own key-down ends a pending modifier tap, as the key
        // would have: the program has had a key between that modifier's press and its release.
        if kb.dwExtraInfo == PASS_ON_MARK {
            let msg = wparam as u32;
            if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) && modifier_generic(kb.vkCode).is_none() {
                TAP_ARMED.store(0, Ordering::Relaxed);
            }
            return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
        }
        let vk = kb.vkCode;
        // A key `host.input.send` sent (`key_send`): taken, matched and counted as let through as a
        // typed key is, and left out of the count that ends a count of quick repeats
        // (`TAPS_THROUGH`).
        let sent = kb.dwExtraInfo == SEND_MARK;
        // The keypad's Enter: `VK_RETURN` to Windows, with the extended-key flag.
        let keypad_enter = is_keypad_enter(vk, kb.flags);
        let msg = wparam as u32;
        let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
        let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
        if is_down || is_up {
            // How late this call is. Nearly always 0: the thread does nothing else. When the
            // machine was too loaded to schedule it, the events queued meanwhile arrive in
            // order, late, and each is judged as of its own moment — see `hotkey_hook`.
            let late = hotkey_hook::lateness(now, kb.time, kb.flags & LLKHF_INJECTED != 0);
            // A key-down held 250 ms or more before it got here: kept for the watch's line
            // (`hook_watch_thread::note_late_key`), atomics and at most one post.
            if hook_watch::late_key_said(is_down, late) {
                hook_watch_thread::note_late_key(vk, late);
            }
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
            // And whether the record already had this key down, when it is a modifier: then this
            // key-down is its auto-repeat, which never arms a modifier tap below — a modifier held
            // as the hook came into the chain included (`seed_held_keys`).
            let (mask, modifier_was_held) = HOOK_MODS.with(|m| {
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
                let was_held = mods.holds(vk);
                mods.on_key(vk, is_down);
                m.set(mods);
                (mask, was_held)
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
            // downstream does with it — the user's, not one a module sent.
            if (vk == 0x2D || vk == 0x60 || vk == 0x14) && !sent {
                note_reader_key(vk, is_down, now);
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
            let generic = modifier_generic(vk);
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
                // would read as a tap: a key-down of a modifier the hook's record has down is its
                // repeat, and changes nothing here — also when the modifier went down before the
                // hook was there to see it, out of the chain or not installed yet. And a second,
                // different modifier pressed on top means the user is building a combination —
                // Alt held, then Ctrl — so the arm is dropped rather than handed over. Without
                // that, releasing Alt while Ctrl was still down fired a tap the user never made.
                if !modifier_was_held {
                    let armed = TAP_ARMED.load(Ordering::Relaxed);
                    if armed == 0 {
                        TAP_ARMED.store(vk as i32, Ordering::Relaxed);
                    } else if armed != vk as i32 {
                        TAP_ARMED.store(0, Ordering::Relaxed);
                    }
                }
            } else if let Some(generic) = generic {
                let armed = TAP_ARMED.swap(0, Ordering::Relaxed);
                if armed == vk as i32 {
                    // Decided as a captured key-down is, below: the earliest capture of the tap
                    // whose module's scope is in front, and no menu open there.
                    let tap = crate::backend::MASK_TAP;
                    let taken = {
                        let st = locked(&KEY_STATE);
                        if st.set.iter().any(|c| c.vk == generic && c.mask == tap) {
                            let front = GetForegroundWindow() as isize;
                            match capture_decision(&st.set, &st.owners, generic, tap, front, false, popup_menu_open) {
                                Some(Capture::Take { owner }) => Some((owner, front)),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    };
                    if let Some((owner, front)) = taken {
                        let (seq, taps_seq) = through();
                        let pressed = Pressed { front, seq, taps_seq, phys: None, queued: Some(Instant::now()) };
                        locked(&KEY_QUEUE).push(Taken { vk: generic, mask: tap, owner, repeat: false, pressed });
                        wake_pump();
                    }
                }
            }
            // The two keys that END a menu, remembered whenever a module's menu flag counts for
            // the window in front — captured or not — in the record of every module whose flag
            // does. Escape is captured by no overlay and Return only while the focused control
            // wants it, so a record kept inside the captured-key branch below never held the
            // Escape that cancelled a menu. Noted only; never suppressed.
            //
            // Remembered as noted, so that the captured-key branch below does not note the same
            // press again: a captured Return passes both, and one press read as "Return,
            // Return" in the runtime's log.
            let mut menu_key_noted = false;
            if is_down && mask == 0 && (vk == 0x0D || vk == 0x1B) {
                let st = locked(&KEY_STATE);
                if st.owners.iter().any(|e| e.menu) {
                    let front = GetForegroundWindow() as isize;
                    for owner in menu_flag_owners(&st.set, &st.owners, front) {
                        note_menu_pass(owner, vk, mask);
                        menu_key_noted = true;
                    }
                }
            }
            // Any key-up ends a hold: the next taken key-down of that key is a press of its own,
            // not the keyboard's auto-repeat (`CAPTURED_HELD`). Every key-up passes here.
            // The keypad's Enter comes up as both of its names (`HeldBits::key_up`).
            if is_up {
                CAPTURED_HELD.key_up(vk, keypad_enter);
                KEPT_BACK.key_up(vk, keypad_enter);
                arrow_up(vk);
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
                    t.on_up(vk, kb.time);
                }
            }
            // Match only the exact combo, so "Tab" (mask 0) leaves Alt+Tab alone — by the key the
            // captures know it as (`ck`): the keypad's Enter is NumpadEnter where a capture of that
            // is in scope, and Return everywhere else (`keypad_enter_as`); every other key is its
            // virtual key. The system is asked about the key by its own (`vk`), and so are the
            // hotkeys, which `RegisterHotKey` holds by it.
            let (ck, matched) = {
                let st = locked(&KEY_STATE);
                captured_key(&st.set, &st.owners, vk, keypad_enter, mask, || GetForegroundWindow() as isize)
            };
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
                // navigates one holding Insert. The hook's own record can outlive the key —
                // see SCREEN_READER_MOD_DOWN for how, and what forgets it — so a key let
                // through because of it is noted with how old the record is.
                //
                // Numpad0 is one of those keys, and the record holds its own key-down now: a
                // capture of it takes it while neither of the other two is held, recorded or down
                // (`reader_recorded`).
                let itself = vk == 0x60;
                let recorded = reader_recorded(vk);
                let screen_reader_held = recorded || down(0x2D) || (!itself && down(0x60)) || down(0x14);
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
                    if is_down && screen_reader_held {
                        note_capture_pass(PassNote {
                            vk: ck,
                            mask,
                            why: if recorded { PassWhy::ReaderRecorded } else { PassWhy::ReaderDown },
                            foreground: GetForegroundWindow() as isize,
                            // Not part of a screen reader's reason, and not said with it.
                            scope: 0,
                            reader_ms: now.wrapping_sub(SCREEN_READER_MOD_DOWN_AT.load(Ordering::Relaxed)),
                        });
                    }
                    if hotkeys_filed && is_down {
                        note_captured_down(kb, vk, mask, late, true, false);
                    }
                    count_let_through(is_down, generic, vk, sent);
                    return CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam);
                }
                // Intercept a captured nav key only while an overlay should own it (ReaHotkey's
                // GetContext): a module that captures it is scoped to the window in front, or to
                // every window, AND no menu is open there. A control that opened a #32768 menu
                // must let Tab/Enter/arrows reach the menu natively — the menu window is owned by
                // the plugin, so the foreground doesn't change. Which module takes it is decided
                // here, while the window in front is the one it was pressed in, and goes with the
                // key: the earliest capture in scope (`capture_decision`).
                let foreground = GetForegroundWindow() as isize;
                let decided = {
                    let st = locked(&KEY_STATE);
                    let d = capture_decision(&st.set, &st.owners, ck, mask, foreground, false, popup_menu_open);
                    if let (true, Some(Capture::Pass { why, owner })) = (is_down, d) {
                        let first = st.set.iter().find(|c| c.vk == ck && c.mask == mask);
                        let scope = first.map_or(0, |c| scope_of(&st.owners, c.owner));
                        if !menu_key_noted {
                            // Let through because a menu is open. Remembered, not discarded:
                            // the overlay runtime asks for these to log which keys reached the
                            // menu, and whether it was still there after an Escape — in the
                            // record of each module whose flag let it through, or of the module
                            // that would have taken it from a menu the system drew. See
                            // `take_menu_pass_through`.
                            match why {
                                super::PassWhy::MenuFlag => {
                                    for o in menu_flag_owners(&st.set, &st.owners, foreground) {
                                        note_menu_pass(o, ck, mask);
                                    }
                                }
                                super::PassWhy::MenuMode => {
                                    if let Some(o) = owner {
                                        note_menu_pass(o, ck, mask);
                                    }
                                }
                                super::PassWhy::OutOfScope | super::PassWhy::OwnWindow => {}
                            }
                        }
                        drop(st);
                        if let Some(why) = PassWhy::of(why) {
                            note_capture_pass(PassNote { vk: ck, mask, why, foreground, scope, reader_ms: 0 });
                        }
                    }
                    d
                };
                if let Some(Capture::Take { owner }) = decided {
                    if is_down {
                        // A key the system holds down already went down where no hook of ours
                        // was called for it — an elevated window, one of this application's own,
                        // the secure desktop in front — and this is its auto-repeat, as much as
                        // one whose key-down a capture took (`hotkey_hook::held_before`).
                        let system_held = hotkey_hook::held_before(late, || down(vk as u16));
                        let repeat = CAPTURED_HELD.taken_down_as(ck, keypad_enter) || system_held;
                        // The press as it was: the window, the counts of keys let through so far,
                        // and the key it was pressed on — the keypad's Enter is VK_RETURN too.
                        let (seq, taps_seq) = through();
                        let pressed = Pressed { front: foreground, seq, taps_seq, phys: physical_key(kb), queued: Some(Instant::now()) };
                        locked(&KEY_QUEUE).push(Taken { vk: ck, mask, owner, repeat, pressed });
                        wake_pump();
                        if hotkeys_filed {
                            note_captured_down(kb, vk, mask, late, false, !system_held);
                        }
                    }
                    // Suppress the matched combination — the key-up only when its key-down was
                    // suppressed too (see `swallow_captured`).
                    if swallow_captured(is_down, || down(vk as u16)) {
                        if is_down {
                            KEPT_BACK.taken_down(ck);
                        }
                        return 1;
                    }
                }
            }
            // A registered hotkey, matched here as well as by RegisterHotKey — see
            // `hotkey_hook`. After the captured keys, so a capture of the same combination
            // still wins exactly as it did when the hook swallowed it before RegisterHotKey
            // could see it. Never a modifier: a hotkey's key is an ordinary key. A key-up has
            // been recorded above and always goes through.
            if hotkeys_filed && is_down && hook_hotkey(kb, vk, mask, late) {
                KEPT_BACK.taken_down(vk);
                return 1;
            }
            // Neither a capture nor a hotkey took it: it goes on towards the program.
            count_let_through(is_down, generic, vk, sent);
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// The generic virtual key of a modifier key, as `key_spec` names it — either side of Shift, Ctrl
/// and Alt, and either Windows key — and `None` for every other key.
fn modifier_generic(vk: u32) -> Option<u32> {
    match vk {
        0x10 | 0xA0 | 0xA1 => Some(0x10), // Shift
        0x11 | 0xA2 | 0xA3 => Some(0x11), // Ctrl
        0x12 | 0xA4 | 0xA5 => Some(0x12), // Alt
        0x5B | 0x5C => Some(0x5B),        // Win
        _ => None,
    }
}

/// A key event the hook lets go on towards the program, counted in [`LET_THROUGH`] when it is a
/// key-down of a key that is not a modifier (`generic`, [`modifier_generic`]) and not 0xFF, the
/// hotkeys' masking key and the code of a key with no virtual key of its own — and in
/// [`TAPS_THROUGH`] as well unless `host.input.send` `sent` it ([`SEND_MARK`]). A relaxed add or
/// two, and with tracing on an arrow's note for the log ([`note_arrow_let_through`]).
fn count_let_through(is_down: bool, generic: Option<u32>, vk: u32, sent: bool) {
    if is_down && generic.is_none() && vk != u32::from(VK_MASK_KEY) {
        LET_THROUGH.fetch_add(1, Ordering::Relaxed);
        if !sent {
            TAPS_THROUGH.fetch_add(1, Ordering::Relaxed);
        }
        // With tracing on, an arrow's state of the window in front, for the log
        // (`note_arrow_let_through`).
        if let Some(bit) = arrow_bit(vk).filter(|_| crate::appcfg::trace()) {
            note_arrow_let_through(vk, bit);
        }
    }
}

/// An arrow key-down the hook let go on towards the program, with what the hook read at it: the
/// window in front, and its thread's focus, active and capture windows and flags
/// (`GetGUIThreadInfo(0)`). For the line [`report_arrow_notes`] writes on the pump. Atomics, so
/// the pump takes no lock to read it; `ready` is set last and taken by the pump.
struct ArrowNote {
    vk: AtomicU32,
    /// The tick the hook read it at.
    at: AtomicU32,
    front: AtomicIsize,
    /// `GetGUIThreadInfo` answered; else `flags` holds its error.
    read: AtomicBool,
    focus: AtomicIsize,
    active: AtomicIsize,
    capture: AtomicIsize,
    flags: AtomicU32,
    ready: AtomicBool,
}

impl ArrowNote {
    const fn new() -> Self {
        ArrowNote {
            vk: AtomicU32::new(0),
            at: AtomicU32::new(0),
            front: AtomicIsize::new(0),
            read: AtomicBool::new(false),
            focus: AtomicIsize::new(0),
            active: AtomicIsize::new(0),
            capture: AtomicIsize::new(0),
            flags: AtomicU32::new(0),
            ready: AtomicBool::new(false),
        }
    }
}

/// How many arrows are noted after each change of the captured set.
const ARROW_NOTES_MAX: u32 = 5;
static ARROW_NOTES: [ArrowNote; ARROW_NOTES_MAX as usize] = [const { ArrowNote::new() }; ARROW_NOTES_MAX as usize];
/// How many of [`ARROW_NOTES`] the hook has taken since the captured set last changed; set back
/// to 0 by `set_captured_keys` when it does.
static ARROW_NOTED: AtomicU32 = AtomicU32::new(0);
/// The arrows noted on their key-down and not released since, one bit each (`arrow_bit`): a held
/// arrow's auto-repeat is not noted again, so one held Right does not use up every note. Cleared
/// by the arrow's key-up, which every key-up passes ([`arrow_up`]), and with the rest of what the
/// hook recorded as held ([`forget_keys_held_out_of_sight`]).
static ARROW_NOTED_HELD: AtomicU32 = AtomicU32::new(0);

/// An arrow's bit in [`ARROW_NOTED_HELD`] (left, up, right, down), and `None` for any other key.
fn arrow_bit(vk: u32) -> Option<u32> {
    (0x25..=0x28).contains(&vk).then(|| 1 << (vk - 0x25))
}

/// A key-up of `vk`: when it is an arrow, its next key-down is a press of its own, noted again.
fn arrow_up(vk: u32) {
    if let Some(bit) = arrow_bit(vk) {
        ARROW_NOTED_HELD.fetch_and(!bit, Ordering::Relaxed);
    }
}

/// (The hook's thread, tracing on.) Notes an arrow key-down let through (a press, not a held
/// arrow's repeat) while a module's scope is pinned to the window in front, and while fewer than
/// [`ARROW_NOTES_MAX`] were noted since the captured set changed: those are the arrows that
/// reached no overlay in a window one watches, and arrows typed into another program, or a held
/// one repeating, would use the notes up before the one in question. The scope is looked up in
/// the key table under its lock, as for every captured key. `GetForegroundWindow` and
/// `GetGUIThreadInfo(0)` are calls the hook makes for captured keys as well; neither sends a
/// message. The rest is relaxed stores and a wake of the pump.
fn note_arrow_let_through(vk: u32, bit: u32) {
    if ARROW_NOTED.load(Ordering::Relaxed) >= ARROW_NOTES_MAX
        || ARROW_NOTED_HELD.load(Ordering::Relaxed) & bit != 0
    {
        return;
    }
    // SAFETY: a plain query.
    let front = unsafe { GetForegroundWindow() } as isize;
    if front == 0 || !locked(&KEY_STATE).owners.iter().any(|o| o.scope == front) {
        return;
    }
    ARROW_NOTED_HELD.fetch_or(bit, Ordering::Relaxed);
    // An add rather than a store, so that the pump setting it back to 0 meanwhile is not undone.
    let Some(slot) = ARROW_NOTES.get(ARROW_NOTED.fetch_add(1, Ordering::Relaxed) as usize) else {
        return;
    };
    // SAFETY: plain queries into a zeroed structure whose size is set, as the call needs.
    unsafe {
        let mut gti: GUITHREADINFO = std::mem::zeroed();
        gti.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        let read = GetGUIThreadInfo(0, &mut gti) != 0;
        // Its error at once, before another call can leave one of its own.
        let error = if read { 0 } else { GetLastError() };
        slot.vk.store(vk, Ordering::Relaxed);
        slot.at.store(GetTickCount(), Ordering::Relaxed);
        slot.front.store(front, Ordering::Relaxed);
        slot.read.store(read, Ordering::Relaxed);
        slot.focus.store(gti.hwndFocus as isize, Ordering::Relaxed);
        slot.active.store(gti.hwndActive as isize, Ordering::Relaxed);
        slot.capture.store(gti.hwndCapture as isize, Ordering::Relaxed);
        slot.flags.store(if read { gti.flags } else { error }, Ordering::Relaxed);
    }
    slot.ready.store(true, Ordering::Release);
    wake_pump();
}

/// (The pump.) Writes the arrows [`note_arrow_let_through`] noted, one line each:
/// `arrow vk 0x27 let through: window 0x… thread focus 0x… class '…'; active 0x…; capture 0x…;
/// flags 0x…`. The class is read here, not in the hook; it sends the window no message.
fn report_arrow_notes() {
    for (i, slot) in ARROW_NOTES.iter().enumerate() {
        if !slot.ready.swap(false, Ordering::Acquire) {
            continue;
        }
        let at = slot.at.load(Ordering::Relaxed);
        let front = slot.front.load(Ordering::Relaxed);
        let state = if slot.read.load(Ordering::Relaxed) {
            let focus = slot.focus.load(Ordering::Relaxed);
            let focus = if focus == 0 {
                "none".to_string()
            } else {
                let mut buf = [0u16; 256];
                // SAFETY: a plain query, given its buffer's length; a window gone since reads as
                // an empty class.
                let n = unsafe { GetClassNameW(focus as HWND, buf.as_mut_ptr(), buf.len() as i32) };
                format!("{focus:#x} class '{}'", String::from_utf16_lossy(&buf[..n.max(0) as usize]))
            };
            format!(
                "thread focus {focus}; active {:#x}; capture {:#x}; flags {:#x}",
                slot.active.load(Ordering::Relaxed),
                slot.capture.load(Ordering::Relaxed),
                slot.flags.load(Ordering::Relaxed)
            )
        } else {
            format!("its thread could not be read (GetGUIThreadInfo error {})", slot.flags.load(Ordering::Relaxed))
        };
        // SAFETY: reads the tick clock.
        let ago = unsafe { GetTickCount() }.wrapping_sub(at);
        crate::logging::line(
            "keys",
            &format!(
                "arrow vk {:#04x} let through: window {front:#x} {state} ({ago} ms ago; {} of the first \
                 {ARROW_NOTES_MAX} since the captured set changed)",
                slot.vk.load(Ordering::Relaxed),
                i + 1
            ),
        );
    }
}

/// The physical key of a key event, as `MapVirtualKeyW` with `MAPVK_VK_TO_VSC_EX` writes one: its
/// scan code, with 0xE000 for an extended key. `None` for an event that carried no scan code — one
/// another program sent by virtual key alone.
fn physical_key(kb: &KBDLLHOOKSTRUCT) -> Option<u16> {
    let scan = (kb.scanCode & 0xFF) as u16;
    (scan != 0).then_some(if kb.flags & LLKHF_EXTENDED != 0 { 0xE000 | scan } else { scan })
}

/// A key-down the captured keys' branch took, or let through for a screen reader, entered in
/// the hotkeys' record of held keys (`Table::note_down`) before that branch returns. When it
/// was let through (`passed`) and the combination is also a granted hotkey, `RegisterHotKey`
/// may deliver it next, and the pump is told to expect that rather than to explain it. `kept`
/// when the system never saw the key go down: a capture took it, and the system did not hold the
/// key down already — its repeats are then swallowed should they reach the hotkeys, and the
/// system's first repeat is no new press (`Table::os_repeats`).
fn note_captured_down(kb: &KBDLLHOOKSTRUCT, vk: u32, mask: u8, late: u32, passed: bool, kept: bool) {
    let expected = {
        let mut t = locked(&HOOK_HOTKEYS);
        let Some(t) = t.as_mut() else { return };
        t.note_down(vk, kb.time, kept);
        if passed {
            t.lookup(vk, mask)
        } else {
            NO_ID
        }
    };
    if expected != NO_ID {
        locked(&HOTKEY_QUEUE).push((expected, Route::Passed { time: kb.time, late }, through()));
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
        // A hotkey on Numpad0, one of those keys, is no press made with itself held: for it the
        // other two are asked about, recorded or down, as for a capture of it.
        let held = |k: i32| (GetAsyncKeyState(k) as u16 & 0x8000) != 0;
        let itself = vk == 0x60;
        let screen_reader_held = reader_recorded(vk)
            || held(0x2D)
            || (!itself && held(0x60))
            || held(0x14);
        match down {
            // Its repeats are let through too: the registered path is handling it, and its
            // key-down reached the system, so it is no press the hook kept from it.
            Down::Fire(id) if screen_reader_held => {
                t.passed_on(vk);
                Act::Pass(id)
            }
            // The system holds the key down already: its key-down went to the system while no
            // hook of ours was called for it — `RegisterHotKey` delivered that press, in front of
            // one of this application's windows, an elevated window or after Windows removed the
            // hook — and this is its auto-repeat, not a press (`hotkey_hook::held_before`). Let
            // through as the repeats of a press the registered path handled; `MOD_NOREPEAT` knows
            // the key is down and fires nothing. Seen live on 2026-10-11: the page's key pressed
            // in the page, its callback hid the page, and the key's repeat fired it again here.
            Down::Fire(id) if hotkey_hook::held_before(late, || held(vk as i32)) => {
                t.passed_on(vk);
                Act::Pass(id)
            }
            Down::Repeat if screen_reader_held => Act::Nothing,
            Down::Fire(id) => {
                t.dispatched(vk, late);
                Act::Fire(id)
            }
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
            locked(&HOTKEY_QUEUE).push((id, Route::Hook { time: kb.time, late }, through()));
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
            locked(&HOTKEY_QUEUE).push((id, Route::Passed { time: kb.time, late }, through()));
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

/// The pump's half: whether a hotkey press that reached the queue is dispatched — and then how
/// many of the key-downs let through to the program by the time it came in are its own
/// (`HotkeyPress::own`): 1 for a press the hook let through on purpose and `RegisterHotKey`
/// delivered (`expected`), whose key-down the hook counted as it let it go on, before the
/// `WM_HOTKEY` read the count; 0 for one the hook swallowed, or was not called for. `None` when it
/// is not dispatched. Says so in the log when a press came the other way from the one expected,
/// because that is the only trace either case leaves.
fn settle_hotkey(id: i32, route: Route) -> Option<u32> {
    HOTKEY_DEDUPE.with(|d| {
        let mut d = d.borrow_mut();
        match route {
            Route::Hook { time, late } => {
                if d.hook_press(id, time, late) {
                    return Some(0);
                }
                crate::logging::line(
                    "keys",
                    &format!(
                        "hotkey id {id}: the keyboard hook matched a press {late} ms late that \
                         RegisterHotKey had already delivered; not dispatched a second time"
                    ),
                );
                None
            }
            Route::Passed { time, late } => {
                d.hook_pass(id, time, late);
                None
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
                    None
                }
                // The system's first auto-repeat of a press the hook kept from it — dispatched, or
                // taken by a capture of the same combination — the key still held while the hook
                // is not called: a new press to the system, not to the user
                // (`hotkey_hook::Table::os_repeats`). Judged only while raw input reports the
                // key-ups the hook does not see (`KEY_UPS_REPORTED`). Its entry is taken back
                // out of the pairing, which it is no press for. Said every time: it happens once
                // per hold at most — when a hotkey's callback brings one of this application's
                // windows to the front while its key is down, say.
                OsPress::Deliver { expected: false }
                    if KEY_UPS_REPORTED.load(Ordering::Relaxed)
                        && locked(&HOOK_HOTKEYS).as_mut().is_some_and(|t| t.os_repeats(id, time)) =>
                {
                    d.withdraw_os(id, time);
                    crate::logging::line(
                        "keys",
                        &format!(
                            "hotkey id {id}: RegisterHotKey delivered the keyboard's auto-repeat of \
                             a press the keyboard hook had kept from the system, its key still held \
                             — the system never saw that key go down, so its first repeat was a new \
                             press to it (the hook was not called for it: out of the chain while one \
                             of our windows is in front, an elevated window in front, or removed by \
                             Windows); not dispatched a second time"
                        ),
                    );
                    None
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
                        // elevated window or one of this application's own in front — Windows
                        // does not call the hook for input to either, and RegisterHotKey still
                        // delivers it, which is why it stays — a hook running late, whose own
                        // press then follows and is dropped with a line of its own, or a hook
                        // Windows removed after it timed out, which the keyboard watch installs
                        // again (`hook_watch`).
                        crate::logging::line(
                            "keys",
                            &format!(
                                "hotkey id {id} arrived through RegisterHotKey, not through the \
                                 keyboard hook (said once per window in front): an elevated \
                                 window or one of this application's own is in front, or the \
                                 hook ran late (a \"not dispatched a second time\" line then \
                                 follows), or Windows has removed the hook after it timed out \
                                 (captured keys would then have stopped too, until the keyboard \
                                 watch installs it again: a \"keyboard hook was installed again\" \
                                 line)"
                            ),
                        );
                    }
                    // A press the hook let through: its key-down went on past the hook, which counted
                    // it before Windows matched the hotkey and posted its message.
                    Some(u32::from(expected))
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

/// `host.window.ownsPoint` here: whether the window drawn at (x, y) has the same top-level window as
/// `hwnd`. A handle that names no window owns nothing, so it is `Some(false)` wherever the point is
/// — `None`, read as permission, would let a click through for a window that is gone. `None` only
/// where there is nothing to compare against: no window at the point (off every screen). Local
/// calls that validate their handles, and none sends a window a message.
fn owns_point(hwnd: HWND, x: i32, y: i32) -> Option<bool> {
    // SAFETY: each call validates the handle it is given and answers null or false for one that is
    // not a window.
    unsafe {
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            return Some(false);
        }
        let at = WindowFromPoint(POINT { x, y });
        if at.is_null() {
            return None; // nothing there to compare against, which is not an accusation
        }
        let theirs = GetAncestor(at, GA_ROOT);
        let ours = GetAncestor(hwnd, GA_ROOT);
        if theirs.is_null() || ours.is_null() {
            // Destroyed between the calls: gone, as above, or nothing at the point any more.
            return if IsWindow(hwnd) == 0 { Some(false) } else { None };
        }
        Some(theirs == ours)
    }
}

/// `host.window.foreground()`'s answer for `hwnd`, the foreground window: its process, and
/// whether it is shown. Every call validates the handle and none sends the window a message,
/// so it is safe on any value and a hung application cannot hold it up. `None` for a null
/// handle and for one that is not a window (any more): it was destroyed between
/// `GetForegroundWindow` and here, and a window that does not exist has nothing to report.
fn foreground_of(hwnd: HWND) -> Option<super::Foreground> {
    if hwnd.is_null() {
        return None;
    }
    let mut pid: u32 = 0;
    // SAFETY: `pid` is a live u32; the handle is validated by the call, which returns 0 for
    // one that is not a window.
    if unsafe { GetWindowThreadProcessId(hwnd, &mut pid) } == 0 {
        return None;
    }
    // SAFETY: both validate the handle and answer false for one that is not a window.
    let visible = unsafe { IsWindowVisible(hwnd) } != 0;
    let minimised = unsafe { IsIconic(hwnd) } != 0;
    let shown = shown(visible, minimised, cloaked(hwnd));
    Some(super::Foreground { id: hwnd as isize, pid, shown })
}

/// Whether a foreground window counts as shown: visible (`WS_VISIBLE`), not minimised, and
/// not cloaked. A cloaked window is visible to everything but the compositor, which does not
/// draw it: a window on another virtual desktop, a suspended Store app's, the Start menu's and
/// search's hosts while they are closed. `IsWindowVisible` alone would call every one of those
/// shown.
fn shown(visible: bool, minimised: bool, cloaked: bool) -> bool {
    visible && !minimised && !cloaked
}

/// Whether DWM cloaks the window. A call that fails — no such window any more, or no
/// compositor to ask — counts as not cloaked, which leaves the answer to the other two tests.
fn cloaked(hwnd: HWND) -> bool {
    use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    let mut value: u32 = 0;
    // SAFETY: `value` is a live u32 and the size passed is its size, as DWMWA_CLOAKED needs.
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED as u32,
            &mut value as *mut u32 as *mut core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        )
    };
    hr >= 0 && value != 0
}

/// The process whose program answers accessibility questions about window `hwnd`: its owner, or 0
/// for a window that is gone. Local — the window manager's own record, no message to the window —
/// so the host asks it on the event loop for every question it puts (`element.rs`).
fn process_of_window(hwnd: isize) -> u32 {
    let mut pid = 0u32;
    // SAFETY: validates the handle; a stale or null one answers 0 and leaves `pid` 0.
    unsafe { GetWindowThreadProcessId(hwnd as HWND, &mut pid) };
    pid
}

pub(super) fn process_exe(pid: u32) -> Option<String> {
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

/// The virtual key a key of the host's goes out as, and whether with the extended-key flag: the
/// keypad's Enter ([`super::VK_NUMPAD_ENTER`]), which Windows gives no virtual key of its own, is
/// `VK_RETURN` extended; every other key is its own virtual key, without it.
fn os_key(vk: u32) -> (u32, bool) {
    if vk == super::VK_NUMPAD_ENTER {
        (super::VK_RETURN, true)
    } else {
        (vk, false)
    }
}

/// Whether a key event the hook is called for, of virtual key `vk` with the hook's `flags`, is the
/// keypad's Enter: `VK_RETURN` with `LLKHF_EXTENDED`, the flag the main Enter goes without.
fn is_keypad_enter(vk: u32, flags: u32) -> bool {
    vk == super::VK_RETURN && flags & LLKHF_EXTENDED != 0
}

/// The key a key event of the hook's is to the captures (`ck`), and whether a capture in `set` is
/// of it with the roles in `mask`: the keypad's Enter (`keypad_enter`) is NumpadEnter where a
/// capture of that is in scope for the window in front, and Return everywhere else
/// ([`super::keypad_enter_as`]) — `front` asked for it alone; every other key is its own virtual
/// key. Pure but for `front`, so the tests hand it a set.
fn captured_key(
    set: &[Captured],
    owners: &[OwnerKeys],
    vk: u32,
    keypad_enter: bool,
    mask: u8,
    front: impl FnOnce() -> isize,
) -> (u32, bool) {
    let ck = if keypad_enter { super::keypad_enter_as(set, owners, mask, front()) } else { vk };
    (ck, set.iter().any(|c| c.vk == ck && c.mask == mask))
}

/// The scan code a key of the host's (`vk`, as `key_to_vk` numbers it) goes out with, as
/// `MapVirtualKeyW` with `MAPVK_VK_TO_VSC_EX` gives one: its high byte 0xE0 or 0xE1 for a key it
/// gives as extended — of the key names, the keypad's Divide (0xE035), whose low byte alone is the
/// main row's key of 0x35. The arrows, Home, End, Page Up and Down and Delete it gives on the
/// keypad's codes, not extended (measured 2026-10-24: Up is 0x48). The keypad's Enter has no
/// virtual key of its own to ask about: it is Return's scan code, extended (0xE01C).
fn scan_code(vk: u16) -> u32 {
    if u32::from(vk) == super::VK_NUMPAD_ENTER {
        0xE01C
    } else {
        // SAFETY: MapVirtualKeyW only translates a code.
        unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC_EX) }
    }
}

/// The `lParam`s of a posted key's `WM_KEYDOWN` and `WM_KEYUP` for its scan code ([`scan_code`]):
/// a repeat count of 1, the scan code's low byte in bits 16–23, the extended-key bit (24) for an
/// extended key, and for the key-up the previous state and transition bits (30, 31).
fn post_lparams(scan: u32) -> (isize, isize) {
    let extended = if matches!(scan >> 8, 0xE0 | 0xE1) { 1 << 24 } else { 0 };
    let down = 1isize | (((scan & 0xFF) as isize) << 16) | extended;
    (down, down | (1 << 30) | (1 << 31))
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

pub(super) fn button_flags(button: MouseButton) -> (u32, u32) {
    match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    }
}

// ── The pointer moved as motion ──────────────────────────────────────────────
// `SetCursorPos` places the pointer without input: nothing goes through the system's input
// stream, no low-level hook or raw input sees it, and a control that holds a press and follows
// the pointer only on the motion it is told of sits still (ON:EAR's Width slider, measured:
// `mouse_drag`). So while a button is held, a move is ONE injected absolute motion event over the
// virtual desktop instead: the system carries the held button into the window's `WM_MOUSEMOVE`
// (`MK_LBUTTON`, `MK_RBUTTON`) itself, as for a hand on the mouse, and the control sees a drag.
// Absolute, since a relative move goes through the pointer's speed and "Enhance pointer
// precision" and lands elsewhere; over the virtual desktop, since without it the axis spans the
// primary monitor only; one event, not paced — interpolation is `mouse_drag`'s — and not
// `MOUSEEVENTF_MOVE_NOCOALESCE`: two coalesced moves end on the same point.

/// The buttons this backend pressed through the system's input and has not released since, a
/// bit each (`bit`): a move right after an injected press is motion, whatever the system's own
/// state shows by then. `mouse_down` sets a button's bit; `mouse_up`, `mouse_click`, `mouse_drag`
/// and click_win.rs's click without the front, which go through `SendInput` too, clear it; a
/// posted click goes past the system's input and leaves it.
static PRESSED: AtomicU8 = AtomicU8::new(0);

const fn bit(button: MouseButton) -> u8 {
    match button {
        MouseButton::Left => 1,
        MouseButton::Right => 2,
        MouseButton::Middle => 4,
    }
}

/// `mask` after `button` went down (`down`) or up.
fn pressed_after(mask: u8, button: MouseButton, down: bool) -> u8 {
    if down {
        mask | bit(button)
    } else {
        mask & !bit(button)
    }
}

/// Notes that this backend pressed (`down`) or released `button` through the system's input.
pub(super) fn note_button(button: MouseButton, down: bool) {
    // click_win.rs's thread clicks too: the bit is changed where nothing changed the mask meanwhile.
    let mut now = PRESSED.load(Ordering::SeqCst);
    while let Err(seen) =
        PRESSED.compare_exchange_weak(now, pressed_after(now, button, down), Ordering::SeqCst, Ordering::SeqCst)
    {
        now = seen;
    }
}

/// Whether a move is motion: a button this backend pressed is still down (`pressed`), or the
/// desktop has one down — the high bit of `GetAsyncKeyState` for the left, right and middle
/// buttons (`async_states`), which a button the user holds sets too. The low bit, "pressed since
/// the last call", says nothing of now.
fn moves_as_motion(pressed: u8, async_states: [i16; 3]) -> bool {
    pressed != 0 || async_states.iter().any(|s| (*s as u16 & 0x8000) != 0)
}

/// Whether a button is held now, by either source (`moves_as_motion`).
fn button_held_now() -> bool {
    // SAFETY: a read of the system's key state.
    let states = unsafe {
        [
            GetAsyncKeyState(i32::from(VK_LBUTTON)),
            GetAsyncKeyState(i32::from(VK_RBUTTON)),
            GetAsyncKeyState(i32::from(VK_MBUTTON)),
        ]
    };
    moves_as_motion(PRESSED.load(Ordering::SeqCst), states)
}

/// Pixel `v` on an axis of the virtual desktop that starts at `origin` and is `size` pixels long,
/// as `SendInput`'s absolute axis has it — 0 to 65,535 over the whole desktop — at the middle of
/// its pixel's share of the axis, so that the system's conversion back lands on the pixel however
/// it rounds: ⌊d × size / 65,536⌋, or the nearest pixel of d × (size − 1) / 65,535 — the first for
/// a desktop up to 32,768 pixels long, the second for any. A point off the desktop goes to its
/// nearest edge, where the system keeps a pointer.
fn desk_axis(v: i32, origin: i32, size: i32) -> i32 {
    let size = i64::from(size.max(1));
    let u = (i64::from(v) - i64::from(origin)).clamp(0, size - 1);
    ((2 * u + 1) * 65_536 / (2 * size)).clamp(0, 65_535) as i32
}

/// The virtual desktop: its left, top, width and height, in the physical pixels of this
/// per-monitor aware process (crates/app/windows/automation-platform.exe.manifest) — the space
/// `SetCursorPos` and every `host.input` coordinate are in.
fn virtual_desk() -> (i32, i32, i32, i32) {
    // SAFETY: reads of the system's metrics.
    unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        )
    }
}

/// The event that moves the pointer to (x, y) on virtual desktop `desk` (`virtual_desk`'s) as
/// motion: one absolute move over the virtual desktop, with no button, wheel or time of its own.
fn motion_input(x: i32, y: i32, desk: (i32, i32, i32, i32)) -> INPUT {
    let (left, top, w, h) = desk;
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: desk_axis(x, left, w),
                dy: desk_axis(y, top, h),
                mouseData: 0,
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_VIRTUALDESK | MOUSEEVENTF_ABSOLUTE,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// A button's or the wheel's event with no place of its own: `flags`, and `data` for the wheel.
fn mouse_input(flags: u32, data: i32) -> INPUT {
    // SAFETY: an all-zero INPUT is a valid one; the mouse fields are set below.
    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_MOUSE;
    input.Anonymous.mi.dwFlags = flags;
    input.Anonymous.mi.mouseData = data as u32; // wheel delta reinterpreted
    input
}

// ── What a call's input answers ─────────────────────────────────────────────────────────────
// `SendInput` says how many of its events Windows took — none when another desktop has the input,
// the lock screen or a UAC prompt, with the reason in `GetLastError` — and `SetCursorPos` whether
// it put the pointer there. Every call of `host.input` that goes through the system's input reads
// both and answers what it did not get (`errors::input::NOT_TAKEN`, `NOT_PLACED`). Windows does not
// say when it drops input into a program of higher rights (User Interface Privilege Isolation):
// that input is counted as taken. A call's events go in one `SendInput` where nothing is paced
// between them, which Windows does not interleave with other input; and a call never leaves what it
// pressed down because Windows stopped taking its events part-way: the releases it owes are sent on
// their own (`owed`), and the answer is still the event it did not take.

/// What one event of a call does to what is held: presses a button or a key, lets go of one, or
/// neither — a move, a wheel. By the button's ([`button_code`]) or the key's own number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Holds {
    Down(u32),
    Up(u32),
    Neither,
}

/// A button's number for [`Holds`], apart from every key's.
fn button_code(button: MouseButton) -> u32 {
    0x1_0000 + u32::from(bit(button))
}

/// The events after the first `taken` of `events` that let go of what the first `taken` pressed —
/// or of what `held` was down before the call — and did not let go of: the releases a call whose
/// events Windows stopped taking part-way still sends, so it leaves nothing down. Their places, in
/// order.
fn owed(events: &[Holds], taken: usize, held: &[u32]) -> Vec<usize> {
    let taken = taken.min(events.len());
    let mut down: Vec<u32> = held.to_vec();
    for e in &events[..taken] {
        match *e {
            Holds::Down(k) => down.push(k),
            Holds::Up(k) => down.retain(|d| *d != k),
            Holds::Neither => {}
        }
    }
    let mut out = Vec::new();
    for (i, e) in events.iter().enumerate().skip(taken) {
        if let Holds::Up(k) = *e {
            if down.contains(&k) {
                down.retain(|d| *d != k);
                out.push(i);
            }
        }
    }
    out
}

/// What [`send_inputs`] came to: how many of the events Windows took, its error when it did not
/// take them all, and what was let go of — by the events it took and the releases owed it sent.
#[derive(Debug, PartialEq, Eq)]
struct Sent {
    taken: usize,
    error: Option<u32>,
    let_go: Vec<u32>,
}

impl Sent {
    /// The call's answer: `Ok` when Windows took every event.
    fn answer(&self) -> Result<(), HostError> {
        match self.error {
            None => Ok(()),
            Some(e) => Err(HostError::with(&errors::input::NOT_TAKEN, format!("Windows did not take the input (error {e})"))),
        }
    }
}

/// What a `host.input.send` that reached `SendInput` came to: its answer, and whether a key event
/// went out — any Windows took. A send it took none of sent nothing, and owes no release.
fn key_sent(sent: &Sent) -> super::KeySend {
    super::KeySend { answer: sent.answer().map_err(KeyRefusal::Answer), went_out: sent.taken > 0 }
}

/// `events` in one `SendInput`, each with what it holds — `held` were down before — through
/// [`send_with`].
fn send_inputs(events: &[(INPUT, Holds)], held: &[u32]) -> Sent {
    send_with(events, held, |inputs| {
        // SAFETY: `inputs` holds `inputs.len()` initialised INPUTs for the length of the call.
        let took = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) } as usize;
        // SAFETY: the calling thread's last error, read at once.
        (took, if took < inputs.len() { unsafe { GetLastError() } } else { 0 })
    })
}

/// [`send_inputs`] with the system's `SendInput` handed in — how many it took of what it was given,
/// and the last error — so the rule can be tested: everything at once; when Windows took fewer, the
/// releases owed ([`owed`]) on their own, one `SendInput` each, and a line in the log.
fn send_with(events: &[(INPUT, Holds)], held: &[u32], mut send: impl FnMut(&[INPUT]) -> (usize, u32)) -> Sent {
    if events.is_empty() {
        return Sent { taken: 0, error: None, let_go: Vec::new() };
    }
    let inputs: Vec<INPUT> = events.iter().map(|(i, _)| *i).collect();
    let holds: Vec<Holds> = events.iter().map(|(_, h)| *h).collect();
    let (taken, error) = send(&inputs);
    let taken = taken.min(inputs.len());
    let mut let_go: Vec<u32> = holds[..taken].iter().filter_map(|h| if let Holds::Up(k) = h { Some(*k) } else { None }).collect();
    if taken == inputs.len() {
        return Sent { taken, error: None, let_go };
    }
    let owes = owed(&holds, taken, held);
    let mut sent_owed = 0;
    for &i in &owes {
        if send(std::slice::from_ref(&inputs[i])).0 == 1 {
            sent_owed += 1;
            if let Holds::Up(k) = holds[i] {
                let_go.push(k);
            }
        }
    }
    crate::logging::line(
        "input",
        &format!(
            "Windows took {taken} of {} input events (error {error}); {sent_owed} of the {} release(s) it owed sent on their own",
            inputs.len(),
            owes.len()
        ),
    );
    Sent { taken, error: Some(error), let_go }
}

/// The pointer put at (x, y) (`SetCursorPos`) — or what Windows answered when it did not.
fn place(x: i32, y: i32) -> Result<(), HostError> {
    // SAFETY: a call that moves the pointer; it fails, with a last error, rather than misbehave.
    if unsafe { SetCursorPos(x, y) } != 0 {
        return Ok(());
    }
    let e = unsafe { GetLastError() };
    Err(HostError::with(&errors::input::NOT_PLACED, format!("Windows did not put the pointer there (error {e})")))
}

/// `host.input.click` through the system's input, on the event loop or click_win.rs's thread: the
/// pointer placed, then the press and the release in one `SendInput`. Placed nowhere, or the press
/// not taken, nothing is clicked; a press Windows took without its release has the release sent on
/// its own ([`released`]).
pub(super) fn click_input(x: i32, y: i32, button: MouseButton) -> Result<(), HostError> {
    let (down, up) = button_flags(button);
    let held = button_code(button);
    place(x, y)?;
    let sent = send_inputs(&[(mouse_input(down, 0), Holds::Down(held)), (mouse_input(up, 0), Holds::Up(held))], &[]);
    if sent.taken == 0 {
        return sent.answer();
    }
    released(button, &sent)
}

/// The sixteen steps of a drag from `from` to `to`, each place handed to `step`, which sends it:
/// the first step not taken ends them, and is the answer.
fn drag_steps(from: (i32, i32), to: (i32, i32), mut step: impl FnMut((i32, i32)) -> Result<(), HostError>) -> Result<(), HostError> {
    const STEPS: i32 = 16;
    for i in 1..=STEPS {
        let t = f64::from(i) / f64::from(STEPS);
        let at = (
            (f64::from(from.0) + f64::from(to.0 - from.0) * t).round() as i32,
            (f64::from(from.1) + f64::from(to.1 - from.1) * t).round() as i32,
        );
        step(at)?;
    }
    Ok(())
}

/// A drag's answer, from its steps' and its release's: a button still down first, since the host
/// holds the button on that answer alone (lib.rs, `held_after`); then the first event not taken.
fn drag_answer(stepped: Result<(), HostError>, released: Result<(), HostError>) -> Result<(), HostError> {
    match released {
        Err(left) if left.reason() == &errors::input::LEFT_DOWN => Err(left),
        released => stepped.and(released),
    }
}

/// The answer of a call whose press of `button` Windows took, by what became of its release
/// (`sent`): let go of — which lets go of a button pressed before it too ([`PRESSED`]) — and the
/// call's own answer; or, when Windows refused the release with the call and again on its own, the
/// button still down — noted so, so the moves after it are motion — and `LEFT_DOWN`, which the
/// host takes to hold the button for the module (lib.rs, `held_after`).
fn released(button: MouseButton, sent: &Sent) -> Result<(), HostError> {
    let answer = release_answer(button_code(button), sent);
    let down = matches!(&answer, Err(e) if e.reason() == &errors::input::LEFT_DOWN);
    note_button(button, down);
    if down {
        crate::logging::line("input", &format!("the {button:?} button was pressed and Windows refused its release twice: it is still down"));
    }
    answer
}

/// [`released`]'s answer for the press of `held` ([`button_code`]), pure: what `sent` let go of
/// decides.
fn release_answer(held: u32, sent: &Sent) -> Result<(), HostError> {
    if sent.let_go.contains(&held) {
        return sent.answer();
    }
    let e = sent.error.unwrap_or(0);
    Err(HostError::with(&errors::input::LEFT_DOWN, format!("Windows took the press but not its release (error {e}): the button is still down")))
}

/// How many of `text`'s characters a `type_text` of it typed when Windows took `taken` of its events
/// — a UTF-16 unit's down, then its up — and how many it has: a character is typed once the down of
/// every unit of it was taken, since a `KEYEVENTF_UNICODE` down delivers its unit (`VK_PACKET`, then
/// `WM_CHAR`), and the up owed after it is sent on its own.
fn typed_of(text: &str, taken: usize) -> (usize, usize) {
    let mut events = 0;
    let mut typed = 0;
    for c in text.chars() {
        // The last unit's down is the character's last event but one.
        events += 2 * c.len_utf16();
        if events - 1 <= taken {
            typed += 1;
        }
    }
    (typed, text.chars().count())
}

/// The pointer's place as `GetCursorPos` gave it, or `0, 0` where it gave none, with a line in the
/// log the first time — `error` the system's reason, read only then.
fn cursor_or_origin(read: Option<(i32, i32)>, error: impl FnOnce() -> u32) -> (i32, i32) {
    static SAID: AtomicBool = AtomicBool::new(false);
    read.unwrap_or_else(|| {
        if !SAID.swap(true, Ordering::Relaxed) {
            crate::logging::line(
                "input",
                &format!("Windows did not give the pointer's place (error {}); reporting it at 0,0, which is a guess", error()),
            );
        }
        (0, 0)
    })
}

/// The key a modifier role is pressed on when a key is passed on: the left one of its pair, as a
/// hand on a keyboard would be likeliest to.
fn pass_on_modifier_vk(role: u8) -> u16 {
    match role {
        super::MASK_CTRL => 0xA2,  // VK_LCONTROL
        super::MASK_ALT => 0xA4,   // VK_LMENU
        super::MASK_SHIFT => 0xA0, // VK_LSHIFT
        _ => 0x5B,                 // VK_LWIN
    }
}

/// The key events of a key passed on, as `(virtual key, scan code, flags)` for `SendInput`:
/// each stroke's key — `vk` itself, or the left key of a modifier — with its scan code, the
/// extended-key flag where the key has one (the arrows, Insert, Delete, Home, End, Page Up and
/// Down, the Windows key, the numeric keypad's Enter: what an application reading the scan code or
/// the flag tells them by), and `KEYEVENTF_KEYUP` for a key going up. The key's own scan code is
/// `phys`, the one it was pressed on, where the hook had one — so the keypad's Enter goes as the
/// keypad's Enter, and a keypad arrow as the keypad's; otherwise, and for a modifier, `scan_ex`'s:
/// `MapVirtualKeyW` with `MAPVK_VK_TO_VSC_EX`, whose high byte is 0xE0 or 0xE1 for an extended key,
/// as `phys`'s is. Pure, so the tests hand it a table.
fn pass_on_inputs(vk: u32, phys: Option<u16>, strokes: &[super::Stroke], scan_ex: impl Fn(u16) -> u32) -> Vec<(u16, u16, u32)> {
    strokes
        .iter()
        .map(|s| {
            let (key, down, scan) = match *s {
                super::Stroke::Modifier { role, down } => {
                    let key = pass_on_modifier_vk(role);
                    (key, down, scan_ex(key))
                }
                super::Stroke::Key { down } => (os_key(vk).0 as u16, down, phys.map_or_else(|| scan_ex(vk as u16), u32::from)),
            };
            let mut flags = if down { 0 } else { KEYEVENTF_KEYUP };
            if matches!(scan >> 8, 0xE0 | 0xE1) {
                flags |= KEYEVENTF_EXTENDEDKEY;
            }
            (key, (scan & 0xFF) as u16, flags)
        })
        .collect()
}

/// Sends `events` — `(virtual key, scan code, flags)` each — in one `SendInput`, every one marked
/// with [`PASS_ON_MARK`] so that the hook lets it through untouched: how many Windows took.
fn send_marked(events: &[(u16, u16, u32)]) -> usize {
    if events.is_empty() {
        return 0;
    }
    let inputs: Vec<INPUT> = events
        .iter()
        .map(|&(vk, scan, flags)| {
            // SAFETY: an all-zero INPUT is a valid one; the keyboard fields are set below.
            let mut input: INPUT = unsafe { std::mem::zeroed() };
            input.r#type = INPUT_KEYBOARD;
            input.Anonymous.ki.wVk = vk;
            input.Anonymous.ki.wScan = scan;
            input.Anonymous.ki.dwFlags = flags;
            input.Anonymous.ki.dwExtraInfo = PASS_ON_MARK;
            input
        })
        .collect();
    // SAFETY: `inputs` holds `inputs.len()` initialised INPUTs for the length of the call.
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) as usize }
}

/// One key event for `SendInput`, carrying `extra` in its `dwExtraInfo` — [`SEND_MARK`] for
/// `host.input.send`'s, 0 for the rest.
fn key_input(vk: u16, scan: u16, flags: u32, extra: usize) -> INPUT {
    // SAFETY: an all-zero INPUT is a valid one; the keyboard fields are set below.
    let mut input: INPUT = unsafe { std::mem::zeroed() };
    input.r#type = INPUT_KEYBOARD;
    input.Anonymous.ki.wVk = vk;
    input.Anonymous.ki.wScan = scan;
    input.Anonymous.ki.dwFlags = flags;
    input.Anonymous.ki.dwExtraInfo = extra;
    input
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
/// `scale`: the display scale under the region, for one handed on as captured (`ocr_recognise`);
/// `None` where it is not known, and such a region is read as captured.
fn recognize_image(cap: &CapturedImage, lang: Option<&str>, scale: Option<f64>) -> Result<OcrText, HostError> {
        crate::loop_guard::off_loop("Windows.Media.Ocr");
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
        // that case costs about max(winrt, paddle) instead of their sum. WinRT stays the
        // trusted primary and the only multi-word path.
        //
        // Handed to the one recogniser thread (`paddle_ocr::ask`) before WinRT starts, and
        // cancelled by dropping the handle when WinRT answered or the region is blank: a
        // recognition not started yet is skipped, one started stops before the session if it
        // can. It used to be a thread per region, left to run to the end unread. Counted from
        // the hand-over until it is done (see `paddle_ocr::InFlight`), and not asked at all
        // once the exit's wait has begun: the system engine then reads alone.
        let paddle = small.then(|| super::paddle_ocr::ask(cap)).flatten();

        let t_tight = std::time::Instant::now();
        // A region handed on as captured is not cropped, but ENLARGED where its text is small: at
        // a display scale of 100 % — one pixel a point — three times, or two, within the budget
        // (`policy::pass_through_up`, as a Mac does at backing scale 1). Mapped back as a crop's
        // words are, from an offset of nothing. It costs: measured 2026-10-24 on an i7-8700K, a
        // release build, 110 ms against 22 for a 344x268 region, 261 against 58 for a 900x700 one,
        // the Lanczos enlargement 41 and 146 ms of it — where the 900x700 one read 73 words, not 31.
        let enlarged = (!small)
            .then(|| scale.map(|k| crate::ocr::policy::pass_through_up(cap.w as usize, cap.h as usize, k, None)))
            .flatten()
            .filter(|&f| f > 1)
            .map(|f| Tightened {
                img: upscale(cap, f as u32),
                off_x: 0,
                off_y: 0,
                scale: f as u32,
                pad: OCR_PAD,
                cw: cap.w,
                ch: cap.h,
                blank: false,
            });
        let tight = if small { Some(tighten(cap)) } else { enlarged };
        let tight_ms = t_tight.elapsed().as_secs_f64() * 1000.0;
        let img: &CapturedImage = tight.as_ref().map(|t| &t.img).unwrap_or(cap);
        if debug {
            save_debug(img, "ocr-debug.png");
        }
        // An empty region reads as empty. Nothing else can honestly come out of it, and
        // something else was: the neural fallback, given a rectangle with no content in it,
        // returns a plausible-looking string rather than nothing, and both the primary and
        // the fallback were being asked. Answering here saves the primary recognition on a
        // region there was never anything to read in, and the fallback's too as far as it can:
        // the region was handed to the recogniser above, before the crop that finds it blank,
        // and dropping the handle cancels it — skipped if the recogniser has not reached it,
        // stopped before the session if it has.
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
            run_ocr(img, lang).map_err(|e| HostError::with(&errors::ocr::RECOGNITION_FAILED, format!("OCR failed: {e}")))?;
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
        if let Some(asked) = paddle {
            if text.trim().is_empty() {
                let t_wait = std::time::Instant::now();
                let got = asked.wait();
                wait_ms = t_wait.elapsed().as_secs_f64() * 1000.0;
                if let Some(t) = got {
                    text = t;
                    words.clear(); // recognition-only fallback returns text without boxes
                    lines.clear();
                    used_paddle = true;
                }
            }
            // else: WinRT won, and `asked` is dropped here, which cancels the recognition —
            // skipped, or stopped at its next check; one already inside the session finishes
            // unread, counted in `paddle_ocr::IN_FLIGHT` until it does.
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

/// `ocr-bench --paddle`'s second opinion: a picture read exactly as a captured region is, by
/// [`recognize_image`] with no language — the system recogniser, the neural one beside it, the
/// blank guard. Crate-internal; the bench is its only caller.
pub(super) fn bench_system_read(cap: &CapturedImage) -> Result<OcrText, String> {
    recognize_image(cap, None, None).map_err(|e| e.to_string())
}

/// Every Windows function that photographs for a read or recognises asks the loop guard first,
/// and every function that captures the screen for a `host.screen` call asks its screen guard, so
/// on the event loop's thread a debug build panics before it does anything: nothing is captured
/// and no recogniser is asked here. (The guard's rules are tested in loop_guard.rs; this holds
/// that the guard is where it has to be.)
#[cfg(all(test, debug_assertions))]
mod loop_guard_tests {
    use super::*;

    type Call<'a> = (&'static str, Box<dyn Fn() + 'a>);

    /// The names of `calls` that, on a thread marked as the loop's, did not panic with a line that
    /// begins with `line`.
    fn quiet_of(calls: impl FnOnce() -> Vec<Call<'static>> + Send + 'static, line: &'static str) -> Vec<&'static str> {
        crate::quiet_expected_panics();
        let t = std::thread::spawn(move || {
            crate::loop_guard::mark_loop_thread();
            let mut quiet = Vec::new();
            for (name, call) in calls() {
                let raised = std::panic::catch_unwind(std::panic::AssertUnwindSafe(call));
                let guarded = raised.as_ref().err().and_then(|p| p.downcast_ref::<String>()).is_some_and(|m| m.starts_with(line));
                if !guarded {
                    quiet.push(name);
                }
            }
            quiet
        });
        t.join().unwrap()
    }

    #[test]
    fn every_capture_for_a_read_and_every_recognition_asks_the_guard_first() {
        let quiet = quiet_of(
            || {
                let cap = || CapturedImage { w: 4, h: 4, rgba: vec![0; 64] };
                let frame = move || Frame::from_image(Rect::new(0, 0, 4, 4), cap(), std::time::Instant::now(), FrameVia::Gdi, None);
                let ctx = Recognise { lang: None, fast_ok: false, preempt: None, stop: None, region_done: None };
                let calls: Vec<Call<'static>> = vec![
                    ("recognize_image", Box::new(move || drop(recognize_image(&cap(), None, None)))),
                    ("ocr_capture", Box::new(|| drop(ocr_capture(&[(0, 0, 4, 4)], CaptureSource::Standard)))),
                    ("ocr_recognise", Box::new(move || drop(ocr_recognise(&vec![Ok(cap())], &[(0, 0, 4, 4)], &ctx)))),
                    ("ocr_shot_of", Box::new(move || {
                        if let Some(f) = frame() {
                            drop(ocr_shot_of(&f, &[(0, 0, 4, 4)]));
                        }
                    })),
                    ("paddle_ocr::ask", Box::new(move || drop(super::super::paddle_ocr::ask(&cap())))),
                ];
                calls
            },
            "[loop] text recognition on the event loop: ",
        );
        assert!(quiet.is_empty(), "these ran on the event loop without asking the guard: {quiet:?}");
    }

    /// The captures for a `host.screen` call — the capture thread's snapshot rounds and points, the
    /// image worker's capture and the first-read comparison — each stopped at the screen guard,
    /// through the standard path, before `GetPixel`, a blit or a duplication request is made.
    #[test]
    fn every_screen_capture_asks_the_screen_guard_first() {
        let quiet = quiet_of(
            || {
                let src = CaptureSource::Standard;
                let calls: Vec<Call<'static>> = vec![
                    ("frames_on_capture_thread", Box::new(move || drop(frames_on_capture_thread(&[(0, 0, 4, 4)], src, false)))),
                    ("points_on_capture_thread", Box::new(move || drop(points_on_capture_thread(&[(0, 0)], src, true)))),
                    ("capture_on_worker", Box::new(move || drop(capture_on_worker(&[(0, 0, 4, 4)], src)))),
                    ("compare_on_worker", Box::new(|| {
                        let _ = compare_on_worker("a test", (0, 0, 4, 4));
                    })),
                ];
                calls
            },
            "[loop] screen capture on the event loop: ",
        );
        assert!(quiet.is_empty(), "these captured on the event loop without asking the screen guard: {quiet:?}");
    }
}

/// The `[env]` processor line, asked of this machine: it names something, and counts its cores.
#[cfg(test)]
mod cpu_line_tests {
    use super::*;

    #[test]
    fn the_cpu_line_names_this_processor_and_counts_its_cores() {
        let cores = physical_cores();
        assert!(cores.is_some_and(|n| n > 0), "{cores:?}");
        let line = cpu_line();
        assert!(line.starts_with('"') && line.contains(" cores, ") && line.ends_with(" threads"), "{line}");
        #[cfg(target_arch = "x86_64")]
        assert!(cpu_brand().is_some_and(|b| !b.is_empty() && !b.contains('\0')), "{line}");
    }
}

/// What the keyboard hook keeps and forgets when the watch installs it again.
#[cfg(test)]
mod hook_carry_over_tests {
    use super::*;
    use crate::backend::MASK_ALT;

    /// The hook's records are statics, and the two tests here set and forget them: they take
    /// turns, or one would forget what the other has just recorded.
    static RECORDS: Mutex<()> = Mutex::new(());

    /// What a re-install keeps and what it forgets: `forget_seen_keys`, which the hook's thread
    /// runs right after the swap. Sets the backend's statics, which no test outside this module
    /// touches, and no hook is installed in a test run; puts them back afterwards.
    #[test]
    fn a_reinstall_keeps_the_application_s_keys_and_forgets_what_the_old_hook_saw() {
        let _turn = locked(&RECORDS);
        let captured = vec![
            Captured { vk: 0x09, mask: 0, owner: 1 },
            Captured { vk: 0x09, mask: 1, owner: 1 },
            Captured { vk: 0x0D, mask: 0, owner: 2 },
        ];
        let owners = vec![OwnerKeys { owner: 1, scope: 0x1234, menu: true }];
        {
            let mut st = locked(&KEY_STATE);
            st.set = captured.clone();
            st.owners = owners.clone();
        }
        // Tab held down, taken: its next key-down would be a repeat, and it is held although the
        // system does not know it.
        assert!(!CAPTURED_HELD.taken_down(0x09));
        KEPT_BACK.taken_down(0x09);
        {
            let mut t = locked(&HOOK_HOTKEYS);
            // Alt+V granted (MOD_ALT is 0x1), and pressed: the hook fired it and swallows its
            // repeats from here on.
            assert!(hotkey_hook::file_if_granted(&mut t, true, 7, 0x56, 0x1));
            assert_eq!(t.as_mut().unwrap().on_down(0x56, MASK_ALT, 1_000), Down::Fire(7));
        }
        SCREEN_READER_MOD_DOWN.store(true, Ordering::Relaxed);
        TAP_ARMED.store(0xA4, Ordering::Relaxed);
        HOOK_MODS.with(|m| {
            let mut mods = Mods::default();
            mods.on_key(0xA4, true);
            m.set(mods);
        });
        SCREEN_READER_PASSED.with(|s| s.borrow_mut().push(0x20));

        // What the watch does when the keyboard goes where the hook is not called: the two
        // atomics, and how old the screen reader's record was.
        // SAFETY: reads the tick clock.
        let now = unsafe { GetTickCount() };
        SCREEN_READER_MOD_DOWN_AT.store(now.wrapping_sub(5_000), Ordering::Relaxed);
        let age = forget_keys_held_out_of_sight().expect("the modifier was recorded as held");
        assert!((5_000..60_000).contains(&age), "{age}");
        assert_eq!(forget_keys_held_out_of_sight(), None, "nothing left to forget");
        assert_eq!(TAP_ARMED.load(Ordering::Relaxed), 0);
        assert!(!CAPTURED_HELD.taken_down(0x09), "Tab's key-up may have gone by unseen: a new press");
        assert!(!KEPT_BACK.is_held(0x09), "and no longer held as far as the hook knows");
        assert_eq!(HOOK_MODS.with(|m| m.get()), {
            let mut mods = Mods::default();
            mods.on_key(0xA4, true);
            mods
        }, "the hook thread's record is the hook thread's to forget");

        SCREEN_READER_MOD_DOWN.store(true, Ordering::Relaxed);
        TAP_ARMED.store(0xA4, Ordering::Relaxed);
        forget_seen_keys();

        // The application's: the captured set, the modules' scopes and flags, and the granted
        // hotkeys, as they were.
        {
            let st = locked(&KEY_STATE);
            assert_eq!(st.set, captured);
            assert_eq!(st.owners, owners);
        }
        {
            let mut t = locked(&HOOK_HOTKEYS);
            let t = t.as_mut().unwrap();
            assert!(t.holds(7));
            assert_eq!(t.lookup(0x56, MASK_ALT), 7);
            // Alt+V held across the swap: its repeat is still a repeat, not a second press.
            assert_eq!(t.on_down(0x56, MASK_ALT, 1_030), Down::Repeat);
        }
        // Still owed its key-up.
        assert!(SCREEN_READER_PASSED.with(|s| s.borrow().contains(&0x20)));
        // What the old hook saw go by, whose key-ups it may have missed: forgotten.
        assert!(!SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed));
        assert_eq!(TAP_ARMED.load(Ordering::Relaxed), 0);
        assert_eq!(HOOK_MODS.with(|m| m.get()), Mods::default());
        assert!(!CAPTURED_HELD.taken_down(0x09), "forgotten again with the rest");

        {
            let mut st = locked(&KEY_STATE);
            st.set.clear();
            st.owners.clear();
        }
        CAPTURED_HELD.clear();
        *locked(&HOOK_HOTKEYS) = None;
        SCREEN_READER_PASSED.with(|s| s.borrow_mut().clear());
    }

    /// The module manager comes to the front and the hook's thread takes the hook out of the chain
    /// (`take_out`, with a stand-in for `UnhookWindowsHookEx`): the captured set, the modules'
    /// scopes and flags and the granted hotkeys stay for the hook that comes back, and what the
    /// hook recorded of keys going by is forgotten, the hook thread's record of the modifiers
    /// included, since it sees no key-up until it is back. Coming back forgets nothing more, and
    /// keeps the keys the application set.
    #[test]
    fn a_hook_out_of_the_chain_keeps_the_application_s_keys_and_forgets_what_it_recorded() {
        let _turn = locked(&RECORDS);
        let captured = vec![Captured { vk: 0x09, mask: 0, owner: 1 }, Captured { vk: 0x20, mask: 0, owner: 2 }];
        let owners = vec![OwnerKeys { owner: 1, scope: 0x1234, menu: false }];
        {
            let mut st = locked(&KEY_STATE);
            st.set = captured.clone();
            st.owners = owners.clone();
        }
        {
            let mut t = locked(&HOOK_HOTKEYS);
            assert!(hotkey_hook::file_if_granted(&mut t, true, 7, 0x56, 0x1));
        }
        // In a plug-in before the switch: Tab taken and held, a screen reader's Insert down, Alt
        // held for a tap, a key owed its key-up.
        CAPTURED_HELD.taken_down(0x09);
        KEPT_BACK.taken_down(0x09);
        SCREEN_READER_MOD_DOWN.store(true, Ordering::Relaxed);
        TAP_ARMED.store(0xA4, Ordering::Relaxed);
        HOOK_MODS.with(|m| {
            let mut mods = Mods::default();
            mods.on_key(0xA4, true);
            m.set(mods);
        });
        SCREEN_READER_PASSED.with(|s| s.borrow_mut().push(0x20));

        let mut stale = Vec::new();
        let (now, outcome) = hook_watch::take_out(Some(1u32), &mut stale, |_| Ok(()));
        assert_eq!(now, None, "nothing of ours left in the chain");
        forget_after(outcome);
        assert!(!SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed));
        assert_eq!(TAP_ARMED.load(Ordering::Relaxed), 0);
        assert!(!KEPT_BACK.is_held(0x09));
        assert!(!CAPTURED_HELD.is_held(0x09));
        assert_eq!(HOOK_MODS.with(|m| m.get()), Mods::default(), "the hook thread's record too");
        assert!(SCREEN_READER_PASSED.with(|s| s.borrow().contains(&0x20)), "still owed its key-up");

        // Back, first in the chain: nothing to take out, nothing more forgotten.
        TAP_ARMED.store(0xA5, Ordering::Relaxed);
        let (now, outcome) = hook_watch::swap(now, &mut stale, || Ok(2u32), |_| panic!("nothing to take out"));
        assert_eq!((now, outcome), (Some(2), hook_watch::Outcome::Installed { old: hook_watch::Old::WasOut }));
        forget_after(outcome);
        assert_eq!(TAP_ARMED.load(Ordering::Relaxed), 0xA5, "recorded since: kept");
        {
            let st = locked(&KEY_STATE);
            assert_eq!(st.set, captured);
            assert_eq!(st.owners, owners);
        }
        {
            let t = locked(&HOOK_HOTKEYS);
            let t = t.as_ref().unwrap();
            assert!(t.holds(7));
            assert_eq!(t.lookup(0x56, MASK_ALT), 7);
        }

        {
            let mut st = locked(&KEY_STATE);
            st.set.clear();
            st.owners.clear();
        }
        TAP_ARMED.store(0, Ordering::Relaxed);
        *locked(&HOOK_HOTKEYS) = None;
        SCREEN_READER_PASSED.with(|s| s.borrow_mut().clear());
    }

    /// The log of 2026-10-11, as the hook's thread and the keyboard watch see it. In the plug-in,
    /// Ctrl+Alt+Shift+Win+F7 fired through the hook, which kept F7's key-down from the system; the
    /// page came to the front and the hook went out of the chain; F7 went up in the page, which
    /// raw input reported. 17.9 s later F7 was pressed in the page, and RegisterHotKey delivered it;
    /// its callback hid the page, and the hook came back into the chain with the modifiers, F7 and
    /// a captured Tab still held. F7's auto-repeat must not fire the hotkey again, Tab's is a
    /// repeat, and the modifiers are recorded side by side, which tells a held one's repeat from a
    /// press; after F7's key-up the next press fires. `seed_held_keys_with` is handed the keys held
    /// in place of `GetAsyncKeyState`.
    #[test]
    fn the_hook_back_in_the_chain_takes_the_keys_held_then_as_held() {
        use crate::backend::{MASK_CTRL, MASK_SHIFT, MASK_TAP, MASK_WIN};
        const F7: u32 = 0x76;
        let _turn = locked(&RECORDS);
        let all_four = MASK_CTRL | MASK_ALT | MASK_SHIFT | MASK_WIN;
        let present = HOOK_HOTKEYS_PRESENT.swap(true, Ordering::Relaxed);
        locked(&KEY_STATE).set =
            vec![Captured { vk: 0x09, mask: 0, owner: 1 }, Captured { vk: 0x12, mask: MASK_TAP, owner: 1 }];
        {
            let mut t = locked(&HOOK_HOTKEYS);
            // MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN.
            assert!(hotkey_hook::file_if_granted(&mut t, true, 7, F7, 0xF));
            assert_eq!(t.as_mut().unwrap().on_down(F7, all_four, 30_000), Down::Fire(7));
        }
        let mut stale = Vec::new();
        let (now, outcome) = hook_watch::take_out(Some(1u32), &mut stale, |_| Ok(()));
        forget_after(outcome);
        key_up_reported(F7, 30_150);
        assert!(!locked(&HOOK_HOTKEYS).as_mut().unwrap().os_repeats(7, 47_900), "a press of its own");

        let (_, outcome) = hook_watch::swap(now, &mut stale, || Ok(2u32), |_| panic!("nothing to take out"));
        forget_after(outcome);
        // Held: the left Ctrl, Alt, Shift and Windows keys (the system says the generic ones are
        // down too), F7, Tab, and V, which no capture and no hotkey has yet.
        let held = [0xA2, 0xA4, 0xA0, 0x5B, 0x11, 0x12, 0x10, F7, 0x09, 0x56];
        let asked = std::cell::Cell::new(0);
        seed_held_keys_with(48_000, |vk| {
            asked.set(asked.get() + 1);
            held.contains(&vk)
        });
        assert_eq!(asked.get(), 256, "each key asked once");

        {
            let mut t = locked(&HOOK_HOTKEYS);
            // Alt+V, granted a moment after the seed: an overlay files its keys on the focus move
            // that brought the hook back.
            assert!(hotkey_hook::file_if_granted(&mut t, true, 8, 0x56, 0x1));
            let t = t.as_mut().unwrap();
            // A repeat the keyboard sent just before the hook was back, and the ones after.
            assert_eq!(t.on_down(F7, all_four, 47_990), Down::Pass(7));
            assert_eq!(t.on_down(F7, all_four, 48_400), Down::Pass(7));
            t.on_up(F7, 48_500);
            assert_eq!(t.on_down(F7, all_four, 48_900), Down::Fire(7));
            assert_eq!(t.on_down(0x56, MASK_ALT, 48_100), Down::Pass(8), "V's repeat");
        }
        assert!(CAPTURED_HELD.taken_down(0x09), "Tab's next taken key-down is a repeat");
        assert!(CAPTURED_HELD.is_held(0x56), "every key held, captured now or not");
        assert!(!CAPTURED_HELD.is_held(0x12) && !CAPTURED_HELD.is_held(0xA4), "a modifier is no key held there");
        assert!(!KEPT_BACK.is_held(F7) && !KEPT_BACK.is_held(0x09), "their key-downs reached the system");
        let mods = HOOK_MODS.with(|m| m.get());
        assert_eq!(mods.mask(), all_four);
        assert!(mods.holds(0xA4) && !mods.holds(0xA5), "by side");

        locked(&KEY_STATE).set.clear();
        CAPTURED_HELD.clear();
        *locked(&HOOK_HOTKEYS) = None;
        HOOK_MODS.with(|m| m.set(Mods::default()));
        HOOK_HOTKEYS_PRESENT.store(present, Ordering::Relaxed);
    }

    /// The pump's side of the system's first repeat (`settle_hotkey`). Without raw input's key-ups
    /// no `WM_HOTKEY` is judged to be one, and a press is delivered as before. With them, the
    /// review's case of 2026-10-11: the hook dispatched F7 and kept its key-down from the system,
    /// then ran late — F7's key-up and a new press still to handle, raw input's report of the
    /// key-up not there yet — and the new press's `WM_HOTKEY` reached the pump first. It is taken
    /// for the first press's repeat and dropped; the hook's own report of the new press, when it
    /// comes, is delivered, not paired with the one dropped.
    #[test]
    fn a_press_dropped_as_a_repeat_leaves_the_late_hooks_report_of_it_alone() {
        use crate::backend::{MASK_CTRL, MASK_SHIFT, MASK_WIN};
        const F7: u32 = 0x76;
        let _turn = locked(&RECORDS);
        let all_four = MASK_CTRL | MASK_ALT | MASK_SHIFT | MASK_WIN;
        let reported = KEY_UPS_REPORTED.swap(false, Ordering::Relaxed);
        let fired = |time: u32, late: u32| {
            let mut t = locked(&HOOK_HOTKEYS);
            let t = t.as_mut().unwrap();
            assert_eq!(t.on_down(F7, all_four, time), Down::Fire(7));
            t.dispatched(F7, late);
        };
        assert!(hotkey_hook::file_if_granted(&mut locked(&HOOK_HOTKEYS), true, 7, F7, 0xF));

        fired(10_000, 0);
        assert_eq!(settle_hotkey(7, Route::Hook { time: 10_000, late: 0 }), Some(0), "the press");
        assert_eq!(settle_hotkey(7, Route::Os { time: 10_500 }), Some(0), "not judged without raw input");
        locked(&HOOK_HOTKEYS).as_mut().unwrap().on_up(F7, 10_600);

        KEY_UPS_REPORTED.store(true, Ordering::Relaxed);
        fired(20_000, 0);
        assert_eq!(settle_hotkey(7, Route::Hook { time: 20_000, late: 0 }), Some(0), "the press");
        assert_eq!(settle_hotkey(7, Route::Os { time: 20_400 }), None, "taken for its repeat");
        {
            let mut t = locked(&HOOK_HOTKEYS);
            t.as_mut().unwrap().on_up(F7, 20_300);
        }
        fired(20_400, 650);
        assert_eq!(settle_hotkey(7, Route::Hook { time: 20_400, late: 650 }), Some(0), "the new press, once");
        // Its entry left in the pairing, the hook's report was dropped as its second delivery.
        let mut d = hotkey_hook::Dedupe::default();
        assert_eq!(d.os_press(7, 20_400), OsPress::Deliver { expected: false });
        assert!(!d.hook_press(7, 20_400, 650));

        *locked(&HOOK_HOTKEYS) = None;
        KEY_UPS_REPORTED.store(reported, Ordering::Relaxed);
    }

    /// A press the hook let through on purpose — a screen reader's modifier held, the key down
    /// before the rest of the combination — and `RegisterHotKey` then delivered: its key-down went on
    /// past the hook, which counted it before the `WM_HOTKEY` read the count, so the press
    /// is handed over with that key-down as its own (`HotkeyPress::own`) and its quick repeats count
    /// on. One the system delivered without the hook's note, and one the hook swallowed, have none.
    #[test]
    fn a_press_the_hook_let_through_has_its_own_key_down_in_the_count() {
        let _turn = locked(&RECORDS);
        assert_eq!(settle_hotkey(9, Route::Passed { time: 30_000, late: 0 }), None, "a note, not a press");
        assert_eq!(settle_hotkey(9, Route::Os { time: 30_004 }), Some(1), "its own key-down counted");
        assert_eq!(settle_hotkey(9, Route::Os { time: 31_000 }), Some(0), "no note: the hook was not called");
        assert_eq!(settle_hotkey(9, Route::Hook { time: 32_000, late: 0 }), Some(0), "swallowed");
    }

    /// The moment a press was made, from its stamp on the tick clock (`tick_instant`): now less the
    /// stamp's age. A stamp that reads as after the clock — the two read a moment apart — is now.
    #[test]
    fn a_hotkey_press_is_timed_by_its_stamp_on_the_tick_clock() {
        // SAFETY: a plain call.
        let tick = unsafe { GetTickCount() };
        let before = Instant::now();
        let ago = before.saturating_duration_since(tick_instant(tick.wrapping_sub(1000)));
        assert!(ago >= std::time::Duration::from_millis(950) && ago <= std::time::Duration::from_millis(1100), "{ago:?}");
        assert!(tick_instant(tick.wrapping_add(5000)) >= before, "a stamp after the clock is now");
    }

    /// What the watch does as a window comes to the front (`hook_watch_thread::front_came`): one of
    /// this application's own — the module manager, where Windows does not call the hook — forgets
    /// what the hook recorded as held, as NVDA's menu does, whatever the levels say; another
    /// program's ordinary window forgets nothing.
    #[test]
    fn a_window_of_this_application_coming_to_the_front_forgets_what_the_hook_recorded() {
        let _turn = locked(&RECORDS);
        const MEDIUM: u32 = 0x2000;
        // NVDA's Insert seen going down, a modifier tap pending, and Tab taken and still held.
        let record = || {
            SCREEN_READER_MOD_DOWN.store(true, Ordering::Relaxed);
            TAP_ARMED.store(0xA4, Ordering::Relaxed);
            CAPTURED_HELD.taken_down(0x09);
            KEPT_BACK.taken_down(0x09);
        };
        let mut said = false;
        record();
        hook_watch_thread::front_came(Some(MEDIUM), false, Some(MEDIUM), &mut said);
        assert!(SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed), "another program's window: kept");
        assert_eq!(TAP_ARMED.load(Ordering::Relaxed), 0xA4);
        assert!(KEPT_BACK.is_held(0x09));
        assert!(!said);

        hook_watch_thread::front_came(Some(MEDIUM), true, Some(MEDIUM), &mut said);
        assert!(!SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed), "its key-up goes where the hook is not called");
        assert_eq!(TAP_ARMED.load(Ordering::Relaxed), 0);
        assert!(!KEPT_BACK.is_held(0x09));
        assert!(!CAPTURED_HELD.taken_down(0x09), "Tab's key-up may have gone by unseen: a new press");
        assert!(said, "the screen reader's modifier forgotten is said, once");

        // With this process's own level unread, its own window is still one the hook is not called for.
        record();
        hook_watch_thread::front_came(None, true, None, &mut said);
        assert!(!SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed));
        assert!(!KEPT_BACK.is_held(0x09));

        CAPTURED_HELD.clear();
        KEPT_BACK.clear();
    }

    /// A captured key-up follows its key-down: swallowed only when the key-down was.
    #[test]
    fn a_captured_key_up_is_swallowed_only_when_its_key_down_was() {
        assert!(swallow_captured(true, || panic!("a key-down does not ask")));
        assert!(swallow_captured(false, || false), "the key-down was swallowed");
        assert!(
            !swallow_captured(false, || true),
            "the key-down reached the application — before the capture, or while the hook was \
             being installed again — so its key-up must too"
        );
    }

    #[test]
    fn a_let_through_says_why() {
        let note = |why, reader_ms| PassNote {
            vk: 0x09,
            mask: 0,
            why,
            foreground: 0x1234,
            scope: 0x5678,
            reader_ms,
        };
        let l = capture_pass_line(&note(PassWhy::ReaderRecorded, 36_000_000));
        assert!(l.starts_with("captured Tab (vk 0x09/m0) was let through"), "{l}");
        assert!(l.contains("recorded as held"), "{l}");
        assert!(l.contains("36000000 ms ago"), "{l}");
        let l = capture_pass_line(&note(PassWhy::OutOfScope, 0));
        assert!(l.contains("every capture of it is scoped to another window"), "{l}");
        assert!(l.contains("the earliest to window 0x5678"), "{l}");
        assert!(l.contains("window 0x1234 is in front"), "{l}");
        assert!(capture_pass_line(&note(PassWhy::MenuMode, 0)).contains("menu mode"));
        let l = capture_pass_line(&note(PassWhy::MenuFlag, 0));
        assert!(l.contains("host.keys.menuOpen") && l.contains("a module scoped to the window in front"), "{l}");
        assert!(capture_pass_line(&note(PassWhy::ReaderDown, 0)).contains("as far as the system"));
        // The three the shared decision names for the hook are the hook's own three; the fourth,
        // the Mac tap's, the hook never hears.
        assert_eq!(PassWhy::of(crate::backend::PassWhy::OutOfScope), Some(PassWhy::OutOfScope));
        assert_eq!(PassWhy::of(crate::backend::PassWhy::MenuMode), Some(PassWhy::MenuMode));
        assert_eq!(PassWhy::of(crate::backend::PassWhy::MenuFlag), Some(PassWhy::MenuFlag));
        assert_eq!(PassWhy::of(crate::backend::PassWhy::OwnWindow), None);
    }

    /// The repeat bit: down, down is a repeat; the key's up clears it; another key's up does
    /// not; forgetting clears every key's. A bitmap of its own, not the hook's.
    #[test]
    fn a_taken_key_down_with_its_key_still_held_is_a_repeat() {
        let held = HeldBits::new();
        assert!(!held.taken_down(0x09), "the first press");
        assert!(held.taken_down(0x09), "held: the keyboard's auto-repeat");
        assert!(held.taken_down(0x09));
        held.up(0x20);
        assert!(held.taken_down(0x09), "another key's up leaves Tab held");
        assert!(!held.taken_down(0x20), "Space is a press of its own");
        held.up(0x09);
        assert!(!held.taken_down(0x09), "released and pressed again: a new press");
        // Keys at both ends of the table, each in its own word.
        for vk in [0x01, 0x3F, 0x40, 0x7F, 0x80, 0xFE, 0xFF] {
            assert!(!held.taken_down(vk), "{vk:#x}");
            assert!(held.taken_down(vk), "{vk:#x}");
        }
        held.clear();
        for vk in [0x09, 0x20, 0x01, 0xFF] {
            assert!(!held.taken_down(vk), "forgotten: {vk:#x}");
        }
        // Past the table it wraps round rather than reaching outside it.
        held.clear();
        assert!(!held.taken_down(0x109));
        assert!(held.taken_down(0x09), "0x109 shares Tab's bit");
        // Asked without marking: what `keyboard_now` reads of the keys kept from the system.
        held.clear();
        assert!(!held.is_held(0x09));
        held.taken_down(0x09);
        assert!(held.is_held(0x09) && !held.is_held(0x20));
        assert!(held.is_held(0x09), "asking marks nothing and clears nothing");
        held.up(0x09);
        assert!(!held.is_held(0x09));
    }

    /// A key passed on, as `SendInput` takes it: the left key of each modifier the press had and
    /// nobody holds, the key itself, each with its scan code and the extended-key flag where the
    /// key has one, `KEYEVENTF_KEYUP` for a key going up — in the order the strokes say. The key on
    /// the scan code it was pressed on, where the hook had one: the keypad's Enter extended, a
    /// keypad arrow with Num Lock off not.
    #[test]
    fn a_key_passed_on_is_sent_with_its_scan_codes_and_extended_flags() {
        use crate::backend::{pass_on_strokes, KeyboardNow, Pressed, Stroke, MASK_CTRL, MASK_SHIFT, MASK_WIN};
        // MapVirtualKeyW(_, MAPVK_VK_TO_VSC_EX) on a US keyboard.
        let table = |k: u16| match k {
            0x09 => 0x0F,   // Tab
            0x0D => 0x1C,   // Enter
            0x25 => 0xE04B, // Left
            0xA2 => 0x1D,   // left Ctrl
            0xA0 => 0x2A,   // left Shift
            0xA4 => 0x38,   // left Alt
            0x5B => 0xE05B, // left Windows key
            _ => 0,
        };
        let now = KeyboardNow { front: Some(1), ..KeyboardNow::default() };
        let at = Pressed { front: 1, ..Pressed::default() };
        let up = KEYEVENTF_KEYUP;
        let ext = KEYEVENTF_EXTENDEDKEY;
        let s = pass_on_strokes(MASK_CTRL | MASK_SHIFT, at, now).unwrap();
        assert_eq!(
            pass_on_inputs(0x25, None, &s, table),
            vec![(0xA2, 0x1D, 0), (0xA0, 0x2A, 0), (0x25, 0x4B, ext), (0x25, 0x4B, ext | up), (0xA0, 0x2A, up), (0xA2, 0x1D, up)]
        );
        let s = pass_on_strokes(0, at, now).unwrap();
        assert_eq!(pass_on_inputs(0x09, None, &s, table), vec![(0x09, 0x0F, 0), (0x09, 0x0F, up)]);
        let s = pass_on_strokes(MASK_WIN, at, now).unwrap();
        assert_eq!(pass_on_inputs(0x09, None, &s, table), vec![(0x5B, 0x5B, ext), (0x09, 0x0F, 0), (0x09, 0x0F, up), (0x5B, 0x5B, ext | up)]);
        // The releases of a part sent are keys going up, the same way.
        assert_eq!(pass_on_inputs(0x09, None, &[Stroke::Modifier { role: crate::backend::MASK_ALT, down: false }], table), vec![(0xA4, 0x38, up)]);
        // Pressed on the keypad: its Enter is extended, unlike the main one; its 4 with Num Lock off
        // is VK_LEFT on a scan code that is not. The modifiers around them stay the mapped ones.
        let s = pass_on_strokes(0, at, now).unwrap();
        assert_eq!(pass_on_inputs(0x0D, Some(0xE01C), &s, table), vec![(0x0D, 0x1C, ext), (0x0D, 0x1C, ext | up)]);
        assert_eq!(pass_on_inputs(0x0D, None, &s, table), vec![(0x0D, 0x1C, 0), (0x0D, 0x1C, up)], "the main Enter");
        let s = pass_on_strokes(MASK_SHIFT, at, now).unwrap();
        assert_eq!(
            pass_on_inputs(0x25, Some(0x4B), &s, table),
            vec![(0xA0, 0x2A, 0), (0x25, 0x4B, 0), (0x25, 0x4B, up), (0xA0, 0x2A, up)]
        );
    }

    /// The physical key the hook reads off a key event: the scan code, 0xE000 with it for an
    /// extended key, nothing for an event sent by virtual key alone; and which keys are modifiers.
    #[test]
    fn the_hook_reads_the_physical_key_and_tells_the_modifiers() {
        // SAFETY: an all-zero KBDLLHOOKSTRUCT is a valid one.
        let mut kb: KBDLLHOOKSTRUCT = unsafe { std::mem::zeroed() };
        kb.vkCode = 0x0D;
        kb.scanCode = 0x1C;
        assert_eq!(physical_key(&kb), Some(0x1C), "the main Enter");
        kb.flags = LLKHF_EXTENDED;
        assert_eq!(physical_key(&kb), Some(0xE01C), "the keypad's");
        kb.flags = LLKHF_INJECTED;
        kb.scanCode = 0;
        assert_eq!(physical_key(&kb), None, "sent by virtual key alone");
        for vk in [0x10, 0xA0, 0xA1, 0x11, 0xA2, 0xA3, 0x12, 0xA4, 0xA5, 0x5B, 0x5C] {
            assert!(modifier_generic(vk).is_some(), "{vk:#x}");
        }
        for vk in [0x09, 0x0D, 0x14, 0x2D, 0x60, 0x25, 0x56, u32::from(VK_MASK_KEY)] {
            assert_eq!(modifier_generic(vk), None, "{vk:#x}");
        }
    }

    /// A key-down that reached the hook late is kept for the watch's line as soon as the hook knows
    /// how late it is, before anything is matched, so a key that is swallowed counts as well.
    #[test]
    fn a_late_key_down_is_kept_for_the_watch_before_anything_is_matched() {
        const SRC: &str = include_str!("windows.rs");
        let at = SRC.find("unsafe extern \"system\" fn ll_keyboard_proc(").unwrap();
        let hook = &SRC[at..at + SRC[at..].find("\n}\n").unwrap()];
        let late = hook.find("let late = hotkey_hook::lateness(").unwrap();
        let kept = hook
            .find("if hook_watch::late_key_said(is_down, late) {\n                hook_watch_thread::note_late_key(vk, late);")
            .expect("the hook keeps late key-downs for the watch");
        assert!(late < kept);
        assert!(kept < hook.find("let (mask, modifier_was_held) = HOOK_MODS.with(").unwrap());
    }

    /// The key that masks a swallowed hotkey's modifiers is 0xFF, the code no keyboard layout
    /// gives a key that has a virtual key: Windows gives it to the keys that have none. 0xE8, the
    /// unassigned code sent before, is a code a program may still give a meaning of its own. The
    /// pages name the same code, and that no key-down of it is counted as let through.
    #[test]
    fn the_masking_key_is_the_code_that_stands_for_no_key() {
        assert_eq!(VK_MASK_KEY, 0xFF);
        const HOTKEY: &str = include_str!("../../../../docs/api/hotkey.md");
        const KEYS: &str = include_str!("../../../../docs/api/keys.md");
        const NO_KEY: &str = "the code Windows gives a key that has no virtual key of its own";
        assert!(HOTKEY.contains("presses and releases the virtual key 0xFF while the modifiers"));
        assert!(HOTKEY.contains(NO_KEY) && KEYS.contains(NO_KEY));
        assert!(KEYS.contains("but for those of 0xFF: the masking key"));
        assert!(!HOTKEY.contains("0xE8") && !KEYS.contains("0xE8"));
    }

    /// The mark, where it has to be: every event `send_marked` sends carries it, and the hook lets
    /// an event carrying it through before it records, matches or counts anything — the screen
    /// reader's modifier, the modifiers as it saw them, a tap, the held keys, a capture, a hotkey —
    /// ending only a pending modifier tap, for its key's key-down. The count of keys let through:
    /// taken with each captured key and hotkey press, moved on only where the hook lets a key-down
    /// go on — past the screen reader's rule, and at the end.
    #[test]
    fn the_keys_passed_on_are_marked_and_the_hook_lets_them_through_first() {
        const SRC: &str = include_str!("windows.rs");
        let body = |sig: &str| {
            let at = SRC.find(sig).unwrap_or_else(|| panic!("{sig}"));
            let end = SRC[at..].find("\n}\n").unwrap();
            &SRC[at..at + end]
        };
        let send = body("fn send_marked(");
        assert!(send.contains("input.Anonymous.ki.dwExtraInfo = PASS_ON_MARK;"));
        assert_eq!(send.matches("SendInput(").count(), 1, "one SendInput for the whole sequence");
        let hook = body("unsafe extern \"system\" fn ll_keyboard_proc(");
        let mark = hook.find("if kb.dwExtraInfo == PASS_ON_MARK {").expect("the hook asks for the mark");
        for later in ["let vk = kb.vkCode;", "HOOK_MODS.with", "note_reader_key(", "CAPTURED_HELD.key_up", "capture_decision(", "hook_hotkey(", "count_let_through("] {
            assert!(mark < hook.find(later).unwrap_or_else(|| panic!("{later}")), "{later} comes before the mark is asked");
        }
        let marked = &hook[mark..mark + hook[mark..].find("\n        }\n").unwrap()];
        assert!(marked.contains(
            "if (msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN) && modifier_generic(kb.vkCode).is_none() {\n                TAP_ARMED.store(0, Ordering::Relaxed);\n            }\n            return CallNextHookEx("
        ));
        assert!(!marked.contains("LET_THROUGH") && !marked.contains("count_let_through"), "a key passed on is not counted");
        // Counted where a key-down goes on: past the screen reader's rule, and after the hotkeys.
        assert_eq!(hook.matches("count_let_through(is_down, generic, vk, sent);").count(), 2);
        assert!(hook.contains("count_let_through(is_down, generic, vk, sent);\n                    return CallNextHookEx("));
        assert!(hook.contains("                return 1;\n            }\n            // Neither a capture nor a hotkey took it: it goes on towards the program.\n            count_let_through(is_down, generic, vk, sent);\n        }\n    }\n    CallNextHookEx("));
        // Taken with each press: a captured key, a tap, a hotkey however it came — both counts.
        assert_eq!(hook.matches("let (seq, taps_seq) = through();").count(), 2, "the captured key and the tap");
        assert!(hook.contains("phys: physical_key(kb), queued: Some(Instant::now()) };\n                        locked(&KEY_QUEUE).push(Taken { vk: ck, mask, owner, repeat, pressed });"));
        assert!(body("fn through()").contains("(LET_THROUGH.load(Ordering::Relaxed), TAPS_THROUGH.load(Ordering::Relaxed))"));
        let code = &SRC[..SRC.find("mod hook_carry_over_tests {").unwrap()];
        assert_eq!(code.matches("locked(&HOTKEY_QUEUE).push((").count(), 4);
        assert_eq!(code.matches("}, through()));").count(), 4, "every hotkey press with its count");
        let pass = body("    fn pass_on_key(&self, vk: u32, _mask: u8, phys: Option<u16>, strokes: &[super::Stroke])");
        assert!(pass.contains("super::left_down(strokes, sent)"), "a part sent has what it pressed released");
        assert!(pass.contains("pass_on_inputs(vk, phys, strokes, scan_code)"));
        // The record of keys kept from the system: marked where the hook swallows a captured
        // key-down and a hotkey's, cleared by every key-up, and asked by `keyboard_now` — with the
        // screen reader's key as the hook tells it, and the count.
        assert!(hook.contains("if is_up {\n                CAPTURED_HELD.key_up(vk, keypad_enter);\n                KEPT_BACK.key_up(vk, keypad_enter);"));
        assert!(hook.contains("if swallow_captured(is_down, || down(vk as u16)) {\n                        if is_down {\n                            KEPT_BACK.taken_down(ck);"));
        assert!(hook.contains("hook_hotkey(kb, vk, mask, late) {\n                KEPT_BACK.taken_down(vk);\n                return 1;"));
        let now = body("    fn keyboard_now(&self, vk: u32, _mask: u8, _phys: Option<u16>)");
        assert!(now.contains("down(os_key(vk).0 as i32) || KEPT_BACK.is_held(vk)"));
        assert!(now.contains("reader_mod: SCREEN_READER_MOD_DOWN.load(Ordering::Relaxed) || down(0x2D) || down(0x60) || down(0x14),"));
        // The same keys as the hook's own rule; for numpad zero's own key the other two are asked
        // about, recorded or down (`reader_recorded`).
        assert!(hook.contains("let recorded = reader_recorded(vk);"));
        assert!(hook.contains("let screen_reader_held = recorded || down(0x2D) || (!itself && down(0x60)) || down(0x14);"));
        let hotkey = body("unsafe fn hook_hotkey(");
        assert!(hotkey.contains("let itself = vk == 0x60;"), "a hotkey on numpad zero the same");
        assert!(hotkey.contains("let screen_reader_held = reader_recorded(vk)\n            || held(0x2D)\n            || (!itself && held(0x60))"));
        // Recorded for the user's keys alone, not one a module sent.
        assert!(hook.contains("if (vk == 0x2D || vk == 0x60 || vk == 0x14) && !sent {\n                note_reader_key(vk, is_down, now);\n            }"));
        assert_eq!(code.matches("SCREEN_READER_MOD_DOWN.store(").count(), 1, "in note_reader_key alone");
        assert!(now.contains("let_through: LET_THROUGH.load(Ordering::Relaxed),"));
    }

    /// `host.input.send`'s mark: on every event `key_send` sends, the key's own and its modifiers',
    /// and on no other key event; read by the hook after the pass-on's mark and before anything is
    /// counted, and changing only the count that ends a count of quick repeats and the record of a
    /// screen reader's modifier — the key is still taken, matched and counted as let through, as a
    /// typed key is.
    #[test]
    fn host_input_send_s_keys_are_marked_and_end_no_count() {
        const SRC: &str = include_str!("windows.rs");
        let body = |sig: &str| {
            let at = SRC.find(sig).unwrap_or_else(|| panic!("{sig}"));
            let end = SRC[at..].find(if sig.starts_with("    ") { "\n    }\n" } else { "\n}\n" }).unwrap();
            &SRC[at..at + end]
        };
        assert_ne!(SEND_MARK, PASS_ON_MARK);
        let send = body("    fn key_send(&self, combo: &str)");
        assert_eq!(send.matches("key_input(").count(), 4);
        assert_eq!(send.matches(", SEND_MARK), Holds::").count(), 4, "every event marked");
        let text = body("    fn type_text(&self, text: &str)");
        assert_eq!(text.matches(", 0), Holds::").count(), 2, "typed text is not host.input.send");
        assert!(body("fn key_input(").contains("input.Anonymous.ki.dwExtraInfo = extra;"));
        let hook = body("unsafe extern \"system\" fn ll_keyboard_proc(");
        let pass_on = hook.find("if kb.dwExtraInfo == PASS_ON_MARK {").unwrap();
        let sent = hook.find("let sent = kb.dwExtraInfo == SEND_MARK;").expect("the hook reads the mark");
        assert!(pass_on < sent && sent < hook.find("count_let_through(").unwrap());
        assert!(sent < hook.find("note_reader_key(").unwrap(), "a screen reader's key a module sent is not recorded");
        assert_eq!(hook.matches("SEND_MARK").count(), 1, "nothing else about the key asks it");
        let counted = body("fn count_let_through(");
        assert!(counted.contains("LET_THROUGH.fetch_add(1, Ordering::Relaxed);\n        if !sent {\n            TAPS_THROUGH.fetch_add(1, Ordering::Relaxed);\n        }"));
        let code = &SRC[..SRC.find("mod hook_carry_over_tests {").unwrap()];
        assert_eq!(code.matches("TAPS_THROUGH.fetch_add(").count(), 1, "moved nowhere else");
    }

    /// The keypad's Enter goes out as Return's virtual key with the extended-key flag — sent,
    /// posted and passed on — and is asked about as Return; every other key is its own.
    #[test]
    fn the_keypad_s_enter_is_return_extended() {
        use crate::backend::{Stroke, VK_NUMPAD_ENTER, VK_RETURN};
        assert_eq!(os_key(VK_NUMPAD_ENTER), (VK_RETURN, true));
        for vk in [VK_RETURN, 0x60, 0x61, 0x69, 0x6A, 0x6B, 0x6D, 0x6E, 0x6F, 0x26] {
            assert_eq!(os_key(vk), (vk, false), "{vk:#04x}");
        }
        let key = [Stroke::Key { down: true }, Stroke::Key { down: false }];
        // Passed on with no scan code of its own: Return's key, Return's scan code, extended.
        let scan_ex = |k: u16| if k == VK_NUMPAD_ENTER as u16 { 0xE01C } else { 0x1C };
        assert_eq!(
            pass_on_inputs(VK_NUMPAD_ENTER, None, &key, scan_ex),
            vec![(0x0D, 0x1C, KEYEVENTF_EXTENDEDKEY), (0x0D, 0x1C, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP)]
        );
        // And on the scan code it was pressed on.
        assert_eq!(pass_on_inputs(VK_NUMPAD_ENTER, Some(0xE01C), &key, |_| 0)[0], (0x0D, 0x1C, KEYEVENTF_EXTENDEDKEY));
        const SRC: &str = include_str!("windows.rs");
        // Each method's own body, up to its closing brace: never this test's text.
        let body = |sig: &str| {
            let at = SRC.find(sig).unwrap_or_else(|| panic!("{sig}"));
            &SRC[at..at + SRC[at..].find("\n    }\n").unwrap()]
        };
        let send = body("    fn key_send(&self, combo: &str)");
        assert!(send.contains("let (key_vk, extended) = os_key(vk);"));
        assert!(send.contains("let ext = if extended { KEYEVENTF_EXTENDEDKEY } else { 0 };"));
        let post = body("    fn key_post(&self, hwnd: isize, key: &str)");
        assert!(post.contains("let key_code = parse_key(key)?;\n        let vk = os_key(key_code).0 as u16;"));
        assert!(post.contains("let (lp_down, lp_up) = post_lparams(scan_code(key_code as u16));"));
    }

    /// The scan code a key goes out with, passed on or posted: `MapVirtualKeyW`'s, asked here of
    /// the system itself — keys that sit where they sit on every layout — with the extended-key
    /// prefix where Windows gives one; the keypad's Enter Return's, extended. A posted key's
    /// `lParam` carries it, bit 24 set for an extended one.
    #[test]
    fn a_key_goes_out_on_its_own_scan_code_extended_where_the_keyboard_s_is() {
        use crate::backend::{VK_NUMPAD_ENTER, VK_RETURN};
        assert_eq!(scan_code(VK_NUMPAD_ENTER as u16), 0xE01C);
        assert_eq!(scan_code(VK_RETURN as u16), 0x1C, "the main Enter");
        assert_eq!(scan_code(0x6F), 0xE035, "the keypad's Divide, extended");
        assert_eq!(scan_code(0x60), 0x52, "numpad zero");
        assert_eq!(scan_code(0x6B), 0x4E, "the keypad's plus");
        assert_eq!(scan_code(0x26), 0x48, "Up: the keypad's code, not extended, as Windows gives it");
        assert_eq!(scan_code(0x70), 0x3B, "F1");
        let (down, up) = post_lparams(0xE035);
        assert_eq!(down, 1 | (0x35 << 16) | (1 << 24), "the keypad's Divide: bit 24");
        assert_eq!(up, down | (1 << 30) | (1 << 31));
        assert_eq!(post_lparams(0xE01C).0, 1 | (0x1C << 16) | (1 << 24), "the keypad's Enter");
        assert_eq!(post_lparams(0x1C).0, 1 | (0x1C << 16), "the main Enter: no bit 24");
        assert_eq!(post_lparams(0x3B), (1 | (0x3B << 16), 1 | (0x3B << 16) | (1 << 30) | (1 << 31)), "F1");
    }

    /// The hook tells the keypad's Enter by its extended-key flag, and names it as the captures in
    /// scope do: NumpadEnter where a capture of that is in scope for the window in front, Return
    /// everywhere else, and matches by that name; the main Enter, and every other key, is its own
    /// virtual key, for which the window in front is not asked. The decision, the notes, the held
    /// records and the queued press go by that name; the system and the hotkeys are asked by the
    /// key's own.
    #[test]
    fn the_hook_tells_the_keypad_s_enter_by_its_flag_and_names_it_by_the_captures_in_scope() {
        use crate::backend::{MASK_CTRL, VK_NUMPAD_ENTER, VK_RETURN};
        assert!(is_keypad_enter(VK_RETURN, LLKHF_EXTENDED));
        assert!(is_keypad_enter(VK_RETURN, LLKHF_EXTENDED | LLKHF_INJECTED), "sent with the flag");
        assert!(!is_keypad_enter(VK_RETURN, 0), "the main Enter");
        assert!(!is_keypad_enter(VK_RETURN, LLKHF_INJECTED));
        assert!(!is_keypad_enter(0x6F, LLKHF_EXTENDED), "the keypad's Divide is extended, and itself");
        assert!(!is_keypad_enter(0x26, LLKHF_EXTENDED));
        let unasked = || -> isize { panic!("the window in front is asked for the keypad's Enter alone") };
        let front = || 0x1234;
        let numpad = Captured { vk: VK_NUMPAD_ENTER, mask: 0, owner: 1 };
        let set = [numpad, Captured { vk: 0x09, mask: 0, owner: 1 }];
        assert_eq!(captured_key(&set, &[], VK_RETURN, true, 0, front), (VK_NUMPAD_ENTER, true));
        assert_eq!(captured_key(&set, &[], VK_RETURN, false, 0, unasked), (VK_RETURN, false), "the main Enter");
        assert_eq!(captured_key(&set, &[], VK_RETURN, true, MASK_CTRL, front), (VK_RETURN, false), "other modifiers");
        let elsewhere = [OwnerKeys { owner: 1, scope: 0x5678, menu: false }];
        assert_eq!(captured_key(&set, &elsewhere, VK_RETURN, true, 0, front), (VK_RETURN, false), "scoped elsewhere");
        // A capture of Return beside it: the main Enter is Return's, the keypad's NumpadEnter's,
        // and Return's again where the NumpadEnter capture is out of scope.
        let both = [Captured { vk: VK_RETURN, mask: 0, owner: 2 }, numpad];
        assert_eq!(captured_key(&both, &[], VK_RETURN, false, 0, unasked), (VK_RETURN, true));
        assert_eq!(captured_key(&both, &[], VK_RETURN, true, 0, front), (VK_NUMPAD_ENTER, true));
        assert_eq!(captured_key(&both, &elsewhere, VK_RETURN, true, 0, front), (VK_RETURN, true));
        assert_eq!(captured_key(&set, &[], 0x09, false, 0, unasked), (0x09, true), "every other key its own");

        const SRC: &str = include_str!("windows.rs");
        let at = SRC.find("unsafe extern \"system\" fn ll_keyboard_proc(").unwrap();
        let hook = &SRC[at..at + SRC[at..].find("\n}\n").unwrap()];
        assert!(hook.contains("let keypad_enter = is_keypad_enter(vk, kb.flags);"));
        assert!(hook.contains("captured_key(&st.set, &st.owners, vk, keypad_enter, mask, || GetForegroundWindow() as isize)"));
        assert!(hook.contains("capture_decision(&st.set, &st.owners, ck, mask, foreground, false, popup_menu_open)"));
        assert!(!hook.contains("capture_decision(&st.set, &st.owners, vk,"));
        assert!(hook.contains("let first = st.set.iter().find(|c| c.vk == ck && c.mask == mask);"));
        assert!(!hook.contains("c.vk == vk"), "nothing matched by the virtual key");
        assert_eq!(hook.matches("note_menu_pass(o, ck, mask);").count(), 2);
        assert!(hook.contains("let repeat = CAPTURED_HELD.taken_down_as(ck, keypad_enter) || system_held;"));
        assert!(hook.contains("KEPT_BACK.taken_down(ck);\n                        }\n                        return 1;"));
        assert!(hook.contains("locked(&KEY_QUEUE).push(Taken { vk: ck, mask, owner, repeat, pressed });"));
        assert!(hook.contains("let system_held = hotkey_hook::held_before(late, || down(vk as u16));"));
        assert!(hook.contains("hook_hotkey(kb, vk, mask, late)"));
    }

    /// The keypad's Enter held: its repeat is a repeat under either of its names, the one it went
    /// down as or the other, which a capture of NumpadEnter coming into scope or going out of it
    /// while it was held makes it; its key-up ends both. Every other key is its own bit alone.
    #[test]
    fn the_keypad_s_enter_is_held_under_either_name_and_comes_up_as_both() {
        use crate::backend::{VK_NUMPAD_ENTER, VK_RETURN};
        let held = HeldBits::new();
        assert!(!held.taken_down_as(VK_RETURN, true), "taken as Return: a press");
        assert!(held.taken_down_as(VK_NUMPAD_ENTER, true), "its repeat as NumpadEnter: the same press");
        assert!(held.taken_down_as(VK_RETURN, true), "and back");
        held.key_up(VK_RETURN, true);
        assert!(!held.is_held(VK_RETURN) && !held.is_held(VK_NUMPAD_ENTER), "up as both");
        assert!(!held.taken_down_as(VK_NUMPAD_ENTER, true), "taken as NumpadEnter: a press");
        assert!(held.taken_down_as(VK_RETURN, true), "its repeat as Return: the same press");
        held.key_up(VK_RETURN, true);
        assert!(!held.is_held(VK_RETURN) && !held.is_held(VK_NUMPAD_ENTER));
        // The main Enter's key-up ends the main Enter's alone.
        held.taken_down_as(VK_NUMPAD_ENTER, true);
        held.taken_down_as(VK_RETURN, false);
        held.key_up(VK_RETURN, false);
        assert!(!held.is_held(VK_RETURN) && held.is_held(VK_NUMPAD_ENTER));
        held.key_up(VK_RETURN, true);
        // Every other key: its own bit.
        assert!(!held.taken_down_as(0x09, false));
        assert!(!held.taken_down_as(0x20, false), "another key held is no repeat of this one");
        assert!(held.taken_down_as(0x09, false));
        held.key_up(0x09, false);
        assert!(!held.is_held(0x09) && held.is_held(0x20));
    }

    /// The record of a screen reader's modifier, for numpad zero's own key: asked without its own
    /// key-down, so it is no press made with itself held — repeats included — while an Insert or a
    /// Caps Lock seen going down before it sends it to the screen reader still, which the system
    /// cannot say when a later hook swallowed that key. Every other key is asked against the whole
    /// record, numpad zero's key-down included; any of the three going up clears it, and it is
    /// forgotten with the rest.
    #[test]
    fn numpad_zero_s_own_key_is_asked_against_the_record_without_itself() {
        let _turn = locked(&RECORDS);
        forget_keys_held_out_of_sight();
        note_reader_key(0x60, true, 1);
        assert!(!reader_recorded(0x60), "numpad zero alone");
        note_reader_key(0x60, true, 2);
        assert!(!reader_recorded(0x60), "its repeat");
        assert!(reader_recorded(0x20), "Space with it held is the screen reader's");
        note_reader_key(0x60, false, 3);
        assert!(!reader_recorded(0x20));
        note_reader_key(0x2D, true, 4);
        note_reader_key(0x60, true, 5);
        assert!(reader_recorded(0x60), "Insert seen going down: numpad zero is the screen reader's");
        assert!(reader_recorded(0x20));
        note_reader_key(0x60, false, 6);
        assert!(!reader_recorded(0x60) && !reader_recorded(0x20), "any of the three up clears it");
        note_reader_key(0x14, true, 7);
        assert!(reader_recorded(0x60), "Caps Lock as Insert");
        assert!(forget_keys_held_out_of_sight().is_some());
        assert!(!reader_recorded(0x60) && !reader_recorded(0x20), "forgotten with the record");
    }
}

/// `host.window.foreground()` on Windows: what counts as on screen, and the reading itself on
/// windows of this session, looked at and never touched.
#[cfg(test)]
mod foreground_tests {
    use super::*;

    /// Shown only when all three say so. Komplete Kontrol's hidden menu window (2026-09-27) is
    /// the first row; a minimised window that keeps the foreground and a cloaked one are the
    /// other two ways a window gets the keyboard with nothing of it on screen.
    #[test]
    fn a_window_is_shown_only_when_visible_not_minimised_and_not_cloaked() {
        assert!(!shown(false, false, false), "hidden");
        assert!(!shown(true, true, false), "minimised");
        assert!(!shown(true, false, true), "cloaked");
        assert!(!shown(false, true, true));
        assert!(shown(true, false, false));
    }

    /// A handle that is no window at all: DWM refuses the question, and a refusal counts as
    /// not cloaked, leaving the answer to the visibility tests. Local, and reads nothing.
    #[test]
    fn a_cloaked_question_that_fails_counts_as_not_cloaked() {
        assert!(!cloaked(std::ptr::null_mut()));
    }

    /// The backend's own body, on a window that is really there and hidden: it is named — the
    /// one thing `active_window` would not do — with its process, and not shown. Read-only: an
    /// existing hidden top-level window of this session (an input method's, say) is looked
    /// at, and nothing is created, shown or focused. One that changes while it is looked at is
    /// passed over for the next.
    #[test]
    fn a_hidden_window_is_named_with_its_process_and_not_shown() {
        use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
        let mut hwnds: Vec<isize> = Vec::new();
        // SAFETY: `enum_proc` pushes into the Vec whose address is passed, which outlives the call.
        unsafe { EnumWindows(Some(enum_proc), &mut hwnds as *mut Vec<isize> as LPARAM) };
        let hidden = |h: HWND| unsafe { IsWindow(h) != 0 && IsWindowVisible(h) == 0 };
        let mut checked = 0;
        for h in hwnds.into_iter().map(|h| h as HWND) {
            if !hidden(h) {
                continue;
            }
            let mut pid: u32 = 0;
            // SAFETY: `pid` is a live u32; the handle is validated by the call.
            let tid = unsafe { GetWindowThreadProcessId(h, &mut pid) };
            let answer = foreground_of(h);
            if tid == 0 || !hidden(h) {
                continue; // gone or shown meanwhile: says nothing either way
            }
            let f = answer.expect("a hidden window that exists is named");
            assert_eq!(f.id, h as isize);
            assert_eq!(f.pid, pid, "its own process");
            assert!(!f.shown, "a hidden window is not shown");
            checked += 1;
            if checked == 3 {
                break;
            }
        }
        if checked == 0 {
            // A session with no hidden top-level window at all (a bare service desktop) has
            // nothing to look at; the next test's handles are checked either way.
            eprintln!("no hidden top-level window in this session to look at");
        }
    }

    /// Nothing is named for a null handle, or for one that is not a window: the window was
    /// destroyed between `GetForegroundWindow` and the reading.
    #[test]
    fn a_handle_that_is_no_window_is_not_named() {
        use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
        assert_eq!(foreground_of(std::ptr::null_mut()), None);
        let gone = [0x7fff_fff1usize, 0x7ffe_1235, 0x7ffd_2469, 0x5a5a_a5a5, 0x3c3c_c3c3]
            .into_iter()
            .map(|v| v as HWND)
            .find(|&h| unsafe { IsWindow(h) } == 0)
            .expect("one of these is no window");
        assert_eq!(foreground_of(gone), None);
    }

    /// A handle that names no window owns no point: `false` — never the `nil` a caller reads as
    /// permission — wherever the point is, on a screen or off every one. It was `nil` until the
    /// answer of 2026-10-24. Local: nothing is asked of any window.
    #[test]
    fn a_handle_that_is_no_window_owns_no_point() {
        use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
        assert_eq!(owns_point(std::ptr::null_mut(), 0, 0), Some(false));
        let gone = [0x7fff_fff1usize, 0x7ffe_1235, 0x7ffd_2469, 0x5a5a_a5a5, 0x3c3c_c3c3]
            .into_iter()
            .map(|v| v as HWND)
            .find(|&h| unsafe { IsWindow(h) } == 0)
            .expect("one of these is no window");
        for (x, y) in [(0, 0), (10, 10), (-1_000_000, -1_000_000)] {
            assert_eq!(owns_point(gone, x, y), Some(false), "{x},{y}");
        }
        // The backend's answer, with `listed` too: a listed window's id is a handle here.
        let b = WindowsBackend::new();
        assert_eq!(b.window_owns_point(gone as isize, 0, 0), Some(false));
        assert_eq!(b.listed_window_owns_point(gone as u64, 0, 0), Some(false));
    }
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
        let big = out[2].as_ref().err().unwrap();
        assert_eq!(degenerate, &capture_failed());
        assert_eq!((big.reason(), big.message()), (&errors::screen::CAPTURE_TOO_LARGE, "screen capture failed: the region is larger than 40 million pixels"));
        assert!(!degenerate.message().starts_with(DUPLICATION_UNANSWERED));
        assert!(!big.message().starts_with(DUPLICATION_UNANSWERED));
    }

    #[test]
    fn a_batch_with_nothing_readable_never_asks_the_engine() {
        let out = duplicate_with(&[(0, 0, 0, 0)], |_| panic!("the engine was asked")).unwrap();
        assert_eq!(out[0].as_ref().err(), Some(&capture_failed()));
    }

    #[test]
    fn an_engine_that_cannot_answer_fails_the_batch_with_its_reason() {
        let got = duplicate_with(&[(0, 0, 4, 4)], |_| Err(Fallback::SecureDesktop));
        assert_eq!(got.err(), Some(Fallback::SecureDesktop));
        let told = unanswered(Fallback::SecureDesktop);
        assert!(told.message().starts_with(DUPLICATION_UNANSWERED), "{told}");
        assert_eq!(told.reason(), &errors::screen::DUPLICATION_SECURE_DESKTOP, "the secure desktop: denied");
    }

    /// A snapshot's frame is `capture_all`'s picture with the path that answered it: the same
    /// fallback decisions, the same reasons, and `via` telling duplication from the standard
    /// path it fell back to. `Opening` is a quiet reason, so the test writes no log line.
    #[test]
    fn frames_all_maps_duplication_failures_like_capture_all() {
        let gdi = |&(_, _, w, h): &(i32, i32, i32, i32)| -> Result<CapturedImage, HostError> {
            if w > 0 && h > 0 { Ok(img(w, h)) } else { Err(capture_failed()) }
        };
        let regions = [(0, 0, 4, 4), (5, 5, 0, 4)];
        let standard = frames_with(&regions, CaptureSource::Standard, |_| panic!("duplication was asked"), gdi);
        let f = standard[0].as_ref().expect("a frame");
        assert_eq!((f.rect, f.via), (Rect::new(0, 0, 4, 4), FrameVia::Gdi));
        assert!(f.img.rgba.chunks_exact(4).all(|p| p[3] == 255), "alpha forced opaque");
        assert_eq!(standard[1].as_ref().err(), Some(&capture_failed()));

        let dup_ok = |rects: &[(i32, i32, i32, i32)]| -> Result<Vec<Result<CapturedImage, HostError>>, Fallback> {
            duplicate_with(rects, |r| Ok(r.iter().map(|&(_, _, w, h)| img(w, h)).collect()))
        };
        let both = CaptureSource::Duplication { or_standard: true };
        let got = frames_with(&regions, both, dup_ok, gdi);
        assert_eq!(got[0].as_ref().map(|f| f.via).ok(), Some(FrameVia::Duplication));
        assert_eq!(got[1].as_ref().err(), Some(&capture_failed()), "a degenerate region fails on its own");

        let lost = |_: &[(i32, i32, i32, i32)]| -> Result<Vec<Result<CapturedImage, HostError>>, Fallback> { Err(Fallback::Overdue) };
        let got = frames_with(&[(0, 0, 4, 4), (0, 0, 60_000, 60_000)], both, lost, gdi);
        assert_eq!(got[0].as_ref().map(|f| f.via).ok(), Some(FrameVia::Gdi), "read the standard way, and it says so");
        let big = got[1].as_ref().err().unwrap();
        assert_eq!(big, &too_large(), "{big}");

        // Several regions duplication could not answer: planned as a standard round is — one
        // capture of two close regions and of one inside them, a far one on its own — each cut
        // out, every frame saying it came the standard way.
        let asked = std::cell::RefCell::new(Vec::new());
        let gdi_logged = |r: &(i32, i32, i32, i32)| -> Result<CapturedImage, HostError> {
            asked.borrow_mut().push(*r);
            gdi(r)
        };
        let group = [(0, 0, 4, 4), (10, 0, 4, 4), (1, 1, 2, 2), (3000, 3000, 4, 4)];
        let got = frames_with(&group, both, lost, gdi_logged);
        assert_eq!(*asked.borrow(), vec![(0, 0, 14, 4), (3000, 3000, 4, 4)], "one capture per planned rectangle");
        let rects: Vec<Rect> = got.iter().map(|f| f.as_ref().expect("a frame").rect).collect();
        assert_eq!(rects, group.iter().map(|&t| Rect::from_tuple(t)).collect::<Vec<_>>(), "each region, in order");
        assert!(got.iter().all(|f| f.as_ref().unwrap().via == FrameVia::Gdi));

        let none = CaptureSource::Duplication { or_standard: false };
        let got = frames_with(&[(0, 0, 4, 4)], none, lost, |_| panic!("the standard path was asked"));
        let why = got[0].as_ref().err().unwrap();
        assert!(why.message().starts_with(DUPLICATION_UNANSWERED) && why.message().ends_with("an earlier read has not come back yet"), "{why}");
        assert_eq!(why.code(), errors::Code::Busy);

        let short = frames_with(&[(0, 0, 4, 4)], CaptureSource::Standard, |_| unreachable!(), |_| Ok(img(4, 3)));
        assert_eq!(short[0].as_ref().err(), Some(&capture_failed()), "a picture of the wrong size is not kept");
    }

    /// `pixels`: the standard path captures the planned rectangles and samples them; desktop
    /// duplication gets every distinct point on a monitor as one 1x1 piece of ONE request, a
    /// point on no monitor reads black without asking it, and a request it cannot answer falls
    /// back or fails as the module's `fallback` says.
    #[test]
    fn pixels_plan_per_source() {
        // A picture whose pixel at screen (x, y) is (x, y, 9), whatever rectangle is captured.
        fn shot(r: Rect) -> CapturedImage {
            let mut rgba = Vec::new();
            for y in r.y..r.y + r.h {
                for x in r.x..r.x + r.w {
                    rgba.extend_from_slice(&[x as u8, y as u8, 9, 255]);
                }
            }
            CapturedImage { w: r.w as u32, h: r.h as u32, rgba }
        }
        let mut captured = Vec::new();
        let near = pixels_with(&[(10, 10), (12, 14)], CaptureSource::Standard, |_| panic!("duplication"), |_, _| false, |r| {
            captured.push(r);
            Ok(shot(r))
        });
        assert_eq!(near, Ok(vec![(10, 10, 9), (12, 14, 9)]));
        assert_eq!(captured, vec![Rect::new(10, 10, 3, 5)], "one capture of the box the points lie in");

        let off = |x: i32, _: i32| x < 0;
        let mut captured = Vec::new();
        let away = pixels_with(&[(-9, 0), (10, 10), (-5, 3)], CaptureSource::Standard, |_| panic!("duplication"), off, |r| {
            captured.push(r);
            Ok(shot(r))
        });
        assert_eq!(away, Ok(vec![(0, 0, 0), (10, 10, 9), (0, 0, 0)]), "black on no monitor, in the order asked");
        assert_eq!(captured, vec![Rect::new(10, 10, 1, 1)], "nothing captured for a point on no monitor");

        let mut sent = Vec::new();
        let dup = CaptureSource::Duplication { or_standard: false };
        let got = pixels_with(
            &[(3, 4), (-50, 4), (3, 4), (7, 8)],
            dup,
            |rects| {
                sent = rects.to_vec();
                Ok(rects.iter().map(|&r| shot(Rect::from_tuple(r))).collect())
            },
            off,
            |_| panic!("the standard path"),
        );
        assert_eq!(got, Ok(vec![(3, 4, 9), (0, 0, 0), (3, 4, 9), (7, 8, 9)]));
        assert_eq!(sent, vec![(3, 4, 1, 1), (7, 8, 1, 1)], "each point on a monitor once, in one request");
        let all_off = pixels_with(&[(-1, 0)], dup, |_| panic!("duplication asked for a point on no monitor"), off, |_| panic!("gdi"));
        assert_eq!(all_off, Ok(vec![(0, 0, 0)]));

        let lost = |_: &[(i32, i32, i32, i32)]| -> Result<Vec<CapturedImage>, Fallback> { Err(Fallback::Overdue) };
        let fell_back = pixels_with(&[(5, 6)], CaptureSource::Duplication { or_standard: true }, lost, off, |r| Ok(shot(r)));
        assert_eq!(fell_back, Ok(vec![(5, 6, 9)]));
        let failed = pixels_with(&[(5, 6)], dup, lost, off, |_| panic!("gdi under fallback = none"));
        assert!(failed.unwrap_err().message().starts_with(DUPLICATION_UNANSWERED));
        assert_eq!(pixels_with(&[], dup, |_| panic!("asked"), off, |_| panic!("gdi")), Ok(Vec::new()));
    }

    // ---- Live, on a real desktop: `cargo test -p host snapshot_live -- --ignored --nocapture` --
    //
    // What only a screen can say about snapshots and `pixels` (S1, S2, S4 in TODO.md), on the
    // test window `dxgi.rs` paints: four quadrants of known colours, no fade-in. Informational
    // where the session decides (a runner whose GDI sees nothing, M19); failing only where the
    // answer would be the host's own fault.

    /// The centre of each quadrant of the test window at `(x, y, w, h)`, as screen points.
    fn quadrant_points(x: i32, y: i32, w: i32, h: i32) -> Vec<(i32, i32)> {
        (0..4).map(|i| (x + (i % 2) * w / 2 + w / 4, y + (i / 2) * h / 2 + h / 4)).collect()
    }

    /// S1 and S2: `pixels` — one `BitBlt` of the box — gives exactly what `GetPixel` gives at each
    /// point, and costs one capture where eight `pixel` calls cost eight.
    #[test]
    #[ignore = "needs a real desktop session: cargo test -p host snapshot_live -- --ignored --nocapture"]
    fn snapshot_live_pixels_equal_getpixel() {
        use super::super::dxgi::tests::{dpi_aware, TestWindow, COLOURS};
        dpi_aware();
        let (x, y, w, h) = (200, 200, 240, 120);
        let win = TestWindow::show(x, y, w, h);
        win.settle(std::time::Duration::from_millis(300));
        let mut pts = quadrant_points(x, y, w, h);
        pts.extend(quadrant_points(x + 10, y + 5, w - 20, h - 10));
        let t = std::time::Instant::now();
        let got = pixels_through(&pts, CaptureSource::Standard);
        let one = t.elapsed();
        let t = std::time::Instant::now();
        let each: Vec<(u8, u8, u8)> = pts.iter().map(|&(px, py)| pixel_gdi(px, py)).collect();
        let eight = t.elapsed();
        println!(
            "SNAPSHOT LIVE: pixels of {} points in {:.1} ms, {} GetPixel calls in {:.1} ms",
            pts.len(),
            one.as_secs_f64() * 1000.0,
            pts.len(),
            eight.as_secs_f64() * 1000.0
        );
        let got = got.expect("the standard path always answers");
        if got[..4] != COLOURS[..] {
            println!("::warning::SNAPSHOT LIVE: the capture does not see the test window's colours here (M19): {got:?}");
        }
        assert_eq!(got, each, "a BitBlt of the box reads what GetPixel reads at every point (S1)");
    }

    /// S4: two snapshots of a still window are byte-identical within one path; and how a
    /// standard snapshot compares with a duplication one of the same region.
    #[test]
    #[ignore = "needs a real desktop session: cargo test -p host snapshot_live -- --ignored --nocapture"]
    fn snapshot_live_still_frames_are_identical() {
        use super::super::dxgi::tests::{dpi_aware, TestWindow};
        dpi_aware();
        crate::appcfg::set("desktop_duplication", true);
        let (x, y, w, h) = (200, 200, 240, 120);
        let win = TestWindow::show(x, y, w, h);
        win.settle(std::time::Duration::from_millis(300));
        let region = [(x, y, w, h)];
        let a = frames_all(&region, CaptureSource::Standard).pop().unwrap().expect("a standard frame");
        let b = frames_all(&region, CaptureSource::Standard).pop().unwrap().expect("a standard frame");
        assert_eq!((a.via, b.via), (FrameVia::Gdi, FrameVia::Gdi));
        assert!(a.img.rgba == b.img.rgba, "two standard snapshots of a still window differ");
        let dup = CaptureSource::Duplication { or_standard: false };
        let mut got = None;
        for _ in 0..40 {
            match frames_all(&region, dup).pop().unwrap() {
                Ok(f) => {
                    got = Some(f);
                    break;
                }
                Err(why) => {
                    println!("SNAPSHOT LIVE: duplication: {why}");
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }
        }
        let Some(d) = got else {
            println!("::warning::SNAPSHOT LIVE: desktop duplication did not answer in this session (M12)");
            return;
        };
        assert_eq!(d.via, FrameVia::Duplication);
        match dxgi::compare(&a.img, &d.img) {
            Some((pct, worst, ls, ld)) => println!(
                "SNAPSHOT LIVE: {pct:.2} % of pixels differ between the standard and the duplication snapshot, by up to {worst}; mean luminance {ls:.1} and {ld:.1}"
            ),
            None => panic!("a standard and a duplication snapshot of one region differ in size"),
        }
    }

    /// A change wait over a still window sees a second window appear beside it 100 ms later:
    /// the real standard capture, through the snapshot lane as the capture thread drives it, on
    /// a thread of its own while this one paints.
    #[test]
    #[ignore = "needs a real desktop session: cargo test -p host snapshot_live -- --ignored --nocapture"]
    fn snapshot_live_change_wait_sees_repaint() {
        use super::super::dxgi::tests::{dpi_aware, TestWindow};
        use crate::ocr::change::{ChangeSpec, Wait};
        use crate::ocr::sched::Owner;
        use crate::ocr::snap_queue::{SnapKind, SnapLane, SnapOutcome, SnapReq, SnapTicket};
        use crate::ocr::types::Priority;
        use std::sync::Arc;
        use std::time::{Duration, Instant};
        dpi_aware();
        let (x, y, w, h) = (240, 240, 200, 100);
        let first = TestWindow::show(x, y, w, h);
        first.settle(Duration::from_millis(300));
        let region = Rect::new(x, y, w, h);
        let asked = Instant::now();
        let spec = ChangeSpec { tolerance: 16, min_pixels: 4, settle: Duration::ZERO, timeout: Duration::from_millis(1000) };
        let req = SnapReq {
            ticket: SnapTicket { id: 1, owner: Owner { idx: 0, gen: 1 }, prio: Priority::Interactive },
            region,
            source: CaptureSource::Standard,
            kind: SnapKind::Change { wait: Box::new(Wait::new(region, &[], spec, None, asked)), start: asked },
            asked,
            cancel: Arc::new(AtomicBool::new(false)),
            compare: None,
        };
        let rounds = std::thread::spawn(move || {
            let mut lane = SnapLane::default();
            lane.submit(req);
            loop {
                let now = Instant::now();
                match lane.take(now) {
                    Some(round) => {
                        let frames = frames_all(&round.tuples(), round.source);
                        let done = round.run(frames, Instant::now(), 0);
                        if let Some(d) = lane.finish(done, Instant::now()).pop() {
                            return d;
                        }
                    }
                    None => std::thread::sleep(
                        lane.next_wake().map_or(Duration::from_millis(1), |t| t.saturating_duration_since(now)),
                    ),
                }
            }
        });
        first.settle(Duration::from_millis(100));
        let second = TestWindow::show(x + w / 2, y, w, h);
        second.settle(Duration::from_millis(1000));
        let d = rounds.join().expect("the round thread");
        match d.outcome {
            SnapOutcome::Picture { frame, frames, change: Some(c) } => {
                let waited = frame.taken.duration_since(asked);
                println!("SNAPSHOT LIVE: the change was seen {} ms after the wait began, in {frames} rounds", waited.as_millis());
                assert!(c.changed && c.settled);
                assert!(waited >= Duration::from_millis(100) && waited < Duration::from_millis(400), "{waited:?}");
            }
            _ => panic!("the change wait gave no picture"),
        }
        drop(second);
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

    /// What a module is told when duplication had no picture is part of the API: every reason
    /// `unanswered` can give is its tail's Reason, with that Reason's words — the numbers it was
    /// made with where the Reason has `…`. tools/api-index.py holds docs/api/screen.md's
    /// "Duplication errors" section, tail by tail, to the same Reasons, so a reason reworded here
    /// and not there fails one of the two.
    #[test]
    fn every_reason_a_module_is_told_is_its_tails_reason() {
        let all = [
            Fallback::SwitchedOff,
            Fallback::Disabled,
            Fallback::Crashed,
            Fallback::NoThread,
            Fallback::Closing,
            Fallback::Overdue,
            Fallback::TooLarge,
            Fallback::Unsupported(0x887A_0004_u32 as i32),
            Fallback::TooManyRecorders,
            Fallback::SecureDesktop,
            Fallback::AccessLost,
            Fallback::DeviceRemoved,
            Fallback::Rotated,
            Fallback::Topology,
            Fallback::NoFrameYet,
            Fallback::NoOutput,
            Fallback::Format(87),
            Fallback::Failed(0x8000_4005_u32 as i32),
        ];
        for why in all {
            // Making it already asserts the fit in a debug build; this holds in every build.
            let told = unanswered(why);
            assert_eq!(told.reason(), why.reason());
            assert!(errors::fits(told.reason().message, told.message()), "{told} is not {:?}", told.reason().message);
            if why != Fallback::TooLarge {
                assert!(told.message().starts_with(&format!("{DUPLICATION_UNANSWERED} — ")), "{told}");
            }
        }
        // And the one a region too large for duplication gets, beside it.
        assert_eq!(too_large().reason(), &errors::screen::CAPTURE_TOO_LARGE);
        assert_eq!(too_large().message(), errors::screen::CAPTURE_TOO_LARGE.message);
    }

    /// The 40 million pixels are a region's, and the calls that hold their region to a limit of
    /// their own before they capture never meet them: a snapshot's and a profile's, a grid's, a
    /// captured template's. So `CAPTURE_TOO_LARGE` names only the searches, `save` and `saveMarked`.
    #[test]
    fn only_the_calls_that_do_not_hold_their_region_meet_the_40_million() {
        let fits = |pixels: i64| dxgi::fits(1, pixels as i32);
        assert!(fits(40_000_000) && !fits(40_000_001), "a region's limit");
        assert!(fits(crate::backend::frame::MAX_FRAME_PIXELS), "snapshot, profile");
        assert!(fits(crate::cells::MAX_REGION_PIXELS as i64), "cells, matchCells");
        assert!(fits(crate::template::MAX_PIXELS as i64), "template{{ capture }}");
        assert!(!errors::screen::CAPTURE_TOO_LARGE.entries.iter().any(|e| {
            ["host-screen-snapshot", "host-screen-profile", "host-screen-cells", "host-screen-matchcells", "host-screen-template", "host-ocr-recognize"]
                .contains(e)
        }));
    }
}

/// The pointer moved as motion while a button is held (the VPS Avenger port's HG1): the events
/// built and the decisions made, never sent — nothing here moves the pointer or presses a button.
#[cfg(test)]
mod pointer_motion_tests {
    use super::*;

    /// Desktops of one, two and three monitors, some left of or above the primary, and the
    /// largest a single monitor has.
    const DESKS: [(i32, i32, i32, i32); 6] = [
        (0, 0, 1920, 1080),
        (-1920, 0, 3840, 1080),
        (0, -1440, 2560, 2520),
        (-2560, -360, 5120, 1800),
        (0, 0, 1366, 768),
        (0, 0, 7680, 4320),
    ];

    /// Every pixel of every desktop goes to a place on the axis from which the system's
    /// conversion back lands on that pixel, whichever of the two readings of it holds:
    /// ⌊d × size / 65,536⌋, and the nearest pixel of d × (size − 1) / 65,535.
    #[test]
    fn a_point_lands_on_its_own_pixel_on_every_desktop() {
        for (left, top, w, h) in DESKS {
            for (origin, size) in [(left, w), (top, h)] {
                for v in origin..origin + size {
                    let d = desk_axis(v, origin, size);
                    assert!((0..=65_535).contains(&d), "{v} on ({origin}, {size}): {d}");
                    let floor = origin + (i64::from(d) * i64::from(size) / 65_536) as i32;
                    assert_eq!(floor, v, "{v} on ({origin}, {size}): {d}, read back by the floor");
                    let nearest = origin + (f64::from(d) * f64::from(size - 1) / 65_535.0).round() as i32;
                    assert_eq!(nearest, v, "{v} on ({origin}, {size}): {d}, read back to the nearest pixel");
                }
            }
        }
    }

    /// A point off the desktop goes to its edge, where the system keeps a pointer.
    #[test]
    fn a_point_off_the_desktop_goes_to_its_edge() {
        for (left, _, w, _) in DESKS {
            assert_eq!(desk_axis(left - 5, left, w), desk_axis(left, left, w), "left of ({left}, {w})");
            assert_eq!(desk_axis(left + w + 5, left, w), desk_axis(left + w - 1, left, w), "right of ({left}, {w})");
            assert_eq!(desk_axis(i32::MIN, left, w), desk_axis(left, left, w));
            assert_eq!(desk_axis(i32::MAX, left, w), desk_axis(left + w - 1, left, w));
        }
    }

    /// One absolute move over the virtual desktop, with no button, wheel, time or extra of its own:
    /// the held button is the system's to carry into the window's mouse message.
    #[test]
    fn the_motion_event_is_one_absolute_virtual_desktop_move() {
        let desk = (-1920, 0, 3840, 1080);
        let input = motion_input(-100, 540, desk);
        assert_eq!(input.r#type, INPUT_MOUSE);
        // SAFETY: the mouse member is the one motion_input writes.
        let mi = unsafe { input.Anonymous.mi };
        assert_eq!(mi.dwFlags, MOUSEEVENTF_MOVE | MOUSEEVENTF_VIRTUALDESK | MOUSEEVENTF_ABSOLUTE);
        assert_eq!(mi.dwFlags, 0xC001);
        for not in [MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
            MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_WHEEL]
        {
            assert_eq!(mi.dwFlags & not, 0, "{not:#x}");
        }
        assert_eq!(mi.dwFlags & 0x2000, 0, "MOUSEEVENTF_MOVE_NOCOALESCE");
        assert_eq!((mi.mouseData, mi.time, mi.dwExtraInfo), (0, 0, 0));
        assert_eq!((mi.dx, mi.dy), (desk_axis(-100, -1920, 3840), desk_axis(540, 0, 1080)));
    }

    /// A move is motion while a button is held — one this backend pressed, or one the desktop has
    /// down now (the high bit) — and a placement otherwise; "pressed since the last call" (the low
    /// bit) is no button held.
    #[test]
    fn a_move_is_motion_only_while_a_button_is_held() {
        assert!(!moves_as_motion(0, [0, 0, 0]));
        assert!(moves_as_motion(bit(MouseButton::Left), [0, 0, 0]));
        assert!(moves_as_motion(bit(MouseButton::Middle), [0, 0, 0]));
        assert!(moves_as_motion(0, [i16::MIN, 0, 0]));
        assert!(!moves_as_motion(0, [0, 1, 0]));
        assert!(moves_as_motion(0, [0, 0, 0x8001u16 as i16]));
    }

    /// The backend's record of its own presses: a bit a button, set by its press and cleared by
    /// its release, a click or a drag of it.
    #[test]
    fn the_backend_knows_which_buttons_it_holds() {
        let (l, r, m) = (MouseButton::Left, MouseButton::Right, MouseButton::Middle);
        assert_eq!(pressed_after(pressed_after(0, l, true), l, false), 0);
        let both = pressed_after(pressed_after(0, l, true), r, true);
        assert_eq!(pressed_after(both, l, false), bit(r));
        assert_eq!(pressed_after(both, m, false), both, "a release of another button");
        assert_eq!(pressed_after(bit(l), l, true), bit(l), "pressed twice");
        assert_eq!([bit(l), bit(r), bit(m)], [1, 2, 4]);
    }

    /// The backend's calls are wired to the builder: a move is motion under `moves_as_motion` and a
    /// placement otherwise; a release is motion, then the button's up, and never a placement; a
    /// drag's steps are motion; and every way the backend presses or releases through the system's
    /// input keeps the record.
    #[test]
    fn move_up_and_drag_are_wired_to_the_motion_builder() {
        const SRC: &str = include_str!("windows.rs");
        const CLICK: &str = include_str!("click_win.rs");
        use crate::ocr_wiring_tests::body;
        let mv = body(SRC, "fn mouse_move(&self, x: i32, y: i32)");
        let held = mv.find("if button_held_now() {").expect("the move asks whether a button is held");
        let motion = mv.find("send_inputs(&[(motion_input(x, y, virtual_desk()), Holds::Neither)], &[]).answer()").expect("the move is motion");
        let placed = mv.find("place(x, y)").expect("the move is a placement otherwise");
        assert!(held < motion && motion < placed, "{mv}");
        assert!(body(SRC, "fn place(x: i32, y: i32) -> Result<(), HostError>").contains("SetCursorPos(x, y)"));
        let asked = body(SRC, "fn button_held_now() -> bool");
        assert!(asked.contains("moves_as_motion(PRESSED.load(Ordering::SeqCst), states)"));
        // The desktop's left, right and middle buttons, each asked.
        for vk in ["VK_LBUTTON", "VK_RBUTTON", "VK_MBUTTON"] {
            assert!(asked.contains(&format!("GetAsyncKeyState(i32::from({vk}))")), "{vk}: {asked}");
        }
        // The record changed as the button went, down or up — never only set.
        assert!(body(SRC, "pub(super) fn note_button(button: MouseButton, down: bool)").contains("pressed_after(now, button, down)"));
        // The whole virtual desktop, not the primary display.
        let desk = body(SRC, "fn virtual_desk() -> (i32, i32, i32, i32)");
        for metric in ["SM_XVIRTUALSCREEN", "SM_YVIRTUALSCREEN", "SM_CXVIRTUALSCREEN", "SM_CYVIRTUALSCREEN"] {
            assert!(desk.contains(&format!("GetSystemMetrics({metric})")), "{metric}: {desk}");
        }
        assert!(!desk.contains("SM_CXSCREEN") && !desk.contains("SM_CYSCREEN"), "{desk}");
        let up = body(SRC, "fn mouse_up(&self, x: i32, y: i32, button: MouseButton)");
        let m = up.find("(motion_input(x, y, virtual_desk()), Holds::Neither)").expect("the release is motion first");
        let u = up.find("(mouse_input(up, 0), Holds::Up(held))").expect("then the up");
        assert!(m < u && !up.contains("place(") && !up.contains("SetCursorPos"), "{up}");
        assert!(up.contains("if sent.let_go.contains(&held) {\n            note_button(button, false);"));
        let drag = body(SRC, "fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton)");
        assert!(drag.contains("(motion_input(x1, y1, virtual_desk()), Holds::Neither)") && drag.contains("(motion_input(at.0, at.1, virtual_desk()), Holds::Neither)"), "{drag}");
        assert!(drag.contains("drag_answer(stepped, released(button, &up))"));
        assert!(body(SRC, "fn released(button: MouseButton, sent: &Sent)").contains("note_button(button, down);"));
        assert!(!SRC.contains(&["move", "_injected"].concat()), "the primary display's normalisation is gone");
        assert!(body(SRC, "fn mouse_down(&self, x: i32, y: i32, button: MouseButton)").contains("note_button(button, true);"));
        assert!(body(SRC, "fn mouse_click(&self, x: i32, y: i32, button: MouseButton)").contains("click_input(x, y, button)"));
        assert!(body(SRC, "pub(super) fn click_input(x: i32, y: i32, button: MouseButton)").contains("released(button, &sent)"));
        assert!(body(CLICK, "fn hold_and_click(").contains("super::windows::click_input(x, y, button)?;"));
    }
}

/// What the backend answers about a window that is not there — the null handle, which no window
/// has — for each call that answers with an error value: refused before Windows is asked anything,
/// so nothing is focused, clicked or posted anywhere. (A handle made up to be stale could still be
/// a live window's on some machine; only null is none for certain. Windows' own refusal of a post,
/// by its error, is `click_win::post_refused`'s, tested there.)
#[cfg(test)]
mod answer_tests {
    use super::*;
    use crate::errors::{common, window};

    #[test]
    fn a_window_that_is_not_there_is_answered_gone() {
        let b = WindowsBackend;
        assert_eq!(b.focus_window(0), Err(HostError::of(&window::NO_SUCH_WINDOW)));
        assert_eq!(b.click_check(0, 10, 10), Err(HostError::of(&common::WINDOW_GONE)));
        assert_eq!(b.mouse_click_posted(0, 10, 10, MouseButton::Left, false), Err(HostError::of(&common::WINDOW_GONE)));
        assert!(b.mouse_click_noactivate(0, 10, 10, MouseButton::Left, 1).is_err_and(|e| e == HostError::of(&common::WINDOW_GONE)));
        assert_eq!(b.key_post(0, "F5"), Err(KeyRefusal::Answer(HostError::of(&common::WINDOW_GONE))));
        // A key name that is not one raises, before any window is asked. (`key_send` is not called
        // here: what it parses, it sends.)
        assert!(matches!(b.key_post(0, "Ctrl+S"), Err(KeyRefusal::Mistake(_))));
    }

    /// A post Windows refuses is answered by its error, where it is posted: each of the key-down
    /// and the key-up reads `PostMessageW`'s result and, refused, answers `post_refused` of the
    /// last error — the two halves posted in order, the up never after a refused down.
    #[test]
    fn a_post_windows_refuses_is_answered_by_its_error() {
        const SRC: &str = include_str!("windows.rs");
        let post = crate::ocr_wiring_tests::body(SRC, "fn key_post(&self, hwnd: isize, key: &str)");
        let posted = post.find("if unsafe { PostMessageW(hwnd as HWND, msg, vk as usize, lp) } == 0 {").expect("the post's result is read");
        let refused = post.find("let why = super::click_win::post_refused(unsafe { GetLastError() });").expect("by its error");
        let answered = post.find("return Err(KeyRefusal::Answer(why));").expect("and answered");
        assert!(post.contains("for (msg, lp) in [(WM_KEYDOWN, lp_down), (WM_KEYUP, lp_up)] {"), "{post}");
        assert!(posted < refused && refused < answered, "{post}");
    }
}

/// A region cut out of the bounding box it was captured in never misses it: the box is the union of
/// the regions, `offset_in` admits only a region inside it, and a capture of another size than the
/// box is not cut (`capture_for_read`). So `ocr::CROP_OUTSIDE` is never given, and names no entry.
#[cfg(test)]
mod ocr_crop_tests {
    use super::*;

    #[test]
    fn a_region_cut_from_its_bounding_box_is_always_inside_it() {
        let mut seed = 0x2545_F491_u32;
        let mut next = |m: i32| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % m as u32) as i32
        };
        let mut cut = 0;
        for _ in 0..2000 {
            let n = 2 + next(4) as usize;
            let rects: Vec<Rect> = (0..n).map(|_| Rect::from_tuple((next(400) - 100, next(300) - 50, next(120), next(40)))).collect();
            let Plan::BoundingBox(b) = plan::capture_plan(&rects) else { continue };
            let big = CapturedImage { w: b.w as u32, h: b.h as u32, rgba: vec![0; (b.w * b.h * 4) as usize] };
            for r in &rects {
                match plan::offset_in(r, &b) {
                    Some((ox, oy)) if !r.is_empty() => {
                        assert!(crop(&big, ox, oy, r.w, r.h).is_some(), "{r:?} in {b:?}");
                        cut += 1;
                    }
                    _ => assert!(r.is_empty(), "every region with an area is inside its box: {r:?} in {b:?}"),
                }
            }
        }
        assert!(cut > 1000, "{cut}");
        assert!(crate::errors::ocr::CROP_OUTSIDE.entries.is_empty());
    }
}

/// What a call through the system's input answers when Windows does not take all of it: the event
/// it stopped at, and the releases it owes sent on their own, so that nothing it pressed is left
/// down; with everything taken, nothing more is sent.
#[cfg(test)]
mod input_answer_tests {
    use super::*;
    use std::cell::RefCell;

    fn ev(h: Holds) -> (INPUT, Holds) {
        (mouse_input(0, 0), h)
    }

    #[test]
    fn a_call_owes_the_releases_of_what_it_pressed_and_windows_took() {
        use Holds::{Down, Neither, Up};
        // send: Ctrl down, A down, A up, Ctrl up.
        let send = [Down(17), Down(65), Up(65), Up(17)];
        assert_eq!(owed(&send, 4, &[]), Vec::<usize>::new(), "all taken");
        assert_eq!(owed(&send, 0, &[]), Vec::<usize>::new(), "nothing taken, nothing pressed");
        assert_eq!(owed(&send, 1, &[]), vec![3], "Ctrl went down: its release");
        assert_eq!(owed(&send, 2, &[]), vec![2, 3], "Ctrl and A: both, in order");
        assert_eq!(owed(&send, 3, &[]), vec![3]);
        // A click: the release of a press that went; a mouseUp's release, of a press before it.
        assert_eq!(owed(&[Down(1), Up(1)], 1, &[]), vec![1]);
        assert_eq!(owed(&[Neither, Up(1)], 0, &[1]), vec![1], "the release a mouseUp is for");
        assert_eq!(owed(&[Neither, Up(1)], 1, &[1]), vec![1]);
        assert_eq!(owed(&[Neither, Up(1)], 1, &[]), Vec::<usize>::new(), "nothing held, nothing owed");
        // Text: the unit whose down went.
        let text = [Down(0), Up(0), Down(1), Up(1)];
        assert_eq!(owed(&text, 3, &[]), vec![3]);
        assert_eq!(owed(&text, 2, &[]), Vec::<usize>::new());
    }

    /// A send went out when Windows took any of its events — all of them, or a part with the error —
    /// and not when it took none: what turns `host.inputEpoch()` over (`KeySend`).
    #[test]
    fn a_send_went_out_when_windows_took_any_of_its_events() {
        let answer = |e: u32| Err(KeyRefusal::Answer(HostError::with(&errors::input::NOT_TAKEN, format!("Windows did not take the input (error {e})"))));
        let all = Sent { taken: 4, error: None, let_go: vec![65, 17] };
        assert_eq!(key_sent(&all), super::super::KeySend { answer: Ok(()), went_out: true });
        let part = Sent { taken: 2, error: Some(5), let_go: vec![65, 17] };
        assert_eq!(key_sent(&part), super::super::KeySend { answer: answer(5), went_out: true });
        let none = Sent { taken: 0, error: Some(5), let_go: Vec::new() };
        assert_eq!(key_sent(&none), super::super::KeySend { answer: answer(5), went_out: false });
        // A mistake is found before anything is built, and sends nothing.
        let b = WindowsBackend::new();
        let tap = b.key_send("Ctrl tap");
        assert!(matches!(tap.answer, Err(KeyRefusal::Mistake(_))) && !tap.went_out, "{tap:?}");
        let unparsed = b.key_send("Ctrl+");
        assert!(matches!(unparsed.answer, Err(KeyRefusal::Mistake(_))) && !unparsed.went_out, "{unparsed:?}");
    }

    #[test]
    fn what_windows_does_not_take_is_answered_and_nothing_is_left_down() {
        // Everything taken: one SendInput, nothing more.
        let calls = RefCell::new(Vec::new());
        let all = send_with(&[ev(Holds::Down(1)), ev(Holds::Up(1))], &[], |i| {
            calls.borrow_mut().push(i.len());
            (i.len(), 0)
        });
        assert_eq!(all, Sent { taken: 2, error: None, let_go: vec![1] });
        assert_eq!(*calls.borrow(), vec![2]);
        assert_eq!(all.answer(), Ok(()));
        // The press taken, the release not: the release on its own, and the answer the error.
        let calls = RefCell::new(Vec::new());
        let half = send_with(&[ev(Holds::Down(1)), ev(Holds::Up(1))], &[], |i| {
            calls.borrow_mut().push(i.len());
            if calls.borrow().len() == 1 { (1, 5) } else { (1, 0) }
        });
        assert_eq!(*calls.borrow(), vec![2, 1], "the owed release, alone");
        assert_eq!(half, Sent { taken: 1, error: Some(5), let_go: vec![1] });
        let err = half.answer().unwrap_err();
        assert_eq!((err.reason(), err.message()), (&errors::input::NOT_TAKEN, "Windows did not take the input (error 5)"));
        // Nothing taken, and the owed release refused too: nothing let go of.
        let none = send_with(&[ev(Holds::Neither), ev(Holds::Up(1))], &[1], |_| (0, 5));
        assert_eq!(none, Sent { taken: 0, error: Some(5), let_go: vec![] });
        // Nothing to send sends nothing.
        assert_eq!(send_with(&[], &[], |_| panic!("asked")), Sent { taken: 0, error: None, let_go: vec![] });
    }

    /// A press Windows took answers by its release: let go of, the call's own answer — `Ok`, or the
    /// error of the release sent again on its own —; refused with the call and again on its own,
    /// `LEFT_DOWN`, the button still down, which the host holds for the module.
    #[test]
    fn a_press_whose_release_windows_refuses_twice_is_still_down() {
        let held = button_code(MouseButton::Left);
        let click = [ev(Holds::Down(held)), ev(Holds::Up(held))];
        let all = send_with(&click, &[], |i| (i.len(), 0));
        assert_eq!(release_answer(held, &all), Ok(()));
        let calls = RefCell::new(0);
        let owed = send_with(&click, &[], |_| {
            *calls.borrow_mut() += 1;
            if *calls.borrow() == 1 { (1, 5) } else { (1, 0) }
        });
        let e = release_answer(held, &owed).unwrap_err();
        assert_eq!(e.reason(), &errors::input::NOT_TAKEN, "let go of, on its own");
        let calls = RefCell::new(0);
        let twice = send_with(&click, &[], |_| {
            *calls.borrow_mut() += 1;
            if *calls.borrow() == 1 { (1, 5) } else { (0, 5) }
        });
        assert_eq!(*calls.borrow(), 2, "the release was sent again, on its own");
        let e = release_answer(held, &twice).unwrap_err();
        assert_eq!((e.reason(), e.message()), (&errors::input::LEFT_DOWN, "Windows took the press but not its release (error 5): the button is still down"));
        // A drag's release, of the press it made before: the same.
        let up = send_with(&[ev(Holds::Up(held))], &[held], |_| (0, 5));
        assert_eq!(release_answer(held, &up).unwrap_err().reason(), &errors::input::LEFT_DOWN);
    }

    /// A drag's sixteen steps go from the start to the end, and the first one Windows does not take
    /// ends them, as the answer; a button left down by the release is the drag's answer before it.
    #[test]
    fn a_drag_s_step_not_taken_ends_the_steps() {
        let mut at = Vec::new();
        assert_eq!(drag_steps((0, 0), (160, -32), |p| { at.push(p); Ok(()) }), Ok(()));
        assert_eq!(at.len(), 16);
        assert_eq!((at[0], at[15]), ((10, -2), (160, -32)));
        let refused = || HostError::with(&errors::input::NOT_TAKEN, "Windows did not take the input (error 5)".into());
        let mut n = 0;
        let r = drag_steps((0, 0), (160, 0), |_| {
            n += 1;
            if n == 3 { Err(refused()) } else { Ok(()) }
        });
        assert_eq!((r, n), (Err(refused()), 3), "no step after the one not taken");
        // The answer: a button left down first, whatever a step answered; then the first not taken.
        let left = || HostError::with(&errors::input::LEFT_DOWN, "Windows took the press but not its release (error 5): the button is still down".into());
        let up_alone = || HostError::with(&errors::input::NOT_TAKEN, "Windows did not take the input (error 6)".into());
        assert_eq!(drag_answer(Err(refused()), Err(left())), Err(left()));
        assert_eq!(drag_answer(Ok(()), Err(left())), Err(left()));
        assert_eq!(drag_answer(Err(refused()), Err(up_alone())), Err(refused()));
        assert_eq!(drag_answer(Ok(()), Err(up_alone())), Err(up_alone()));
        assert_eq!(drag_answer(Err(refused()), Ok(())), Err(refused()));
        assert_eq!(drag_answer(Ok(()), Ok(())), Ok(()));
    }

    /// The pointer calls' order, which no test can drive: placed first, and placed nowhere, nothing
    /// else is sent; a mouseUp's release is of the button held before it, so one not taken with the
    /// movement is sent again on its own; a drag's and a click's release answer through `released`.
    #[test]
    fn the_pointer_calls_place_first_and_owe_their_releases() {
        const SRC: &str = include_str!("windows.rs");
        let body = |sig: &str| crate::ocr_wiring_tests::body(SRC, sig);
        // The first statement after the `let`s is the placement, with `?`.
        let first = |b: &str| {
            b.lines()
                .skip(1)
                .map(str::trim)
                .find(|l| !l.is_empty() && !l.starts_with("//") && !l.starts_with("let "))
                .unwrap()
                .to_string()
        };
        for (sig, place) in [
            ("fn mouse_down(&self, x: i32, y: i32, button: MouseButton)", "place(x, y)?;"),
            ("fn mouse_scroll(&self, x: i32, y: i32, delta: i32)", "place(x, y)?;"),
            ("pub(super) fn click_input(x: i32, y: i32, button: MouseButton)", "place(x, y)?;"),
            ("fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton)", "place(x1, y1)?;"),
        ] {
            assert_eq!(first(body(sig)), place, "{sig}");
        }
        let down = body("fn mouse_down(&self, x: i32, y: i32, button: MouseButton)");
        assert!(down.contains("send_inputs(&[(mouse_input(down, 0), Holds::Down(button_code(button)))], &[]).answer()?;\n        note_button(button, true);"));
        let up = body("fn mouse_up(&self, x: i32, y: i32, button: MouseButton)");
        assert!(up.contains(
            "send_inputs(&[(motion_input(x, y, virtual_desk()), Holds::Neither), (mouse_input(up, 0), Holds::Up(held))], &[held]);"
        ));
        let drag = body("fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton)");
        assert!(drag.contains("let up = send_inputs(&[(mouse_input(up, 0), Holds::Up(held))], &[held]);\n        drag_answer(stepped, released(button, &up))"));
        assert!(drag.contains("let stepped = drag_steps((x1, y1), (x2, y2), |at| {"));
        let click = body("pub(super) fn click_input(x: i32, y: i32, button: MouseButton)");
        assert!(click.contains("if sent.taken == 0 {\n        return sent.answer();\n    }\n    released(button, &sent)"));
        assert!(body("fn released(button: MouseButton, sent: &Sent)").contains("note_button(button, down);"));
    }

    #[test]
    fn typed_text_counts_whole_characters() {
        assert_eq!(typed_of("abc", 6), (3, 3));
        assert_eq!(typed_of("abc", 5), (3, 3), "the third's down delivered it; its up is owed");
        assert_eq!(typed_of("abc", 3), (2, 3), "the second's down delivered it");
        assert_eq!(typed_of("abc", 2), (1, 3));
        assert_eq!(typed_of("abc", 1), (1, 3));
        assert_eq!(typed_of("abc", 0), (0, 3));
        // A character outside the basic plane is two units, four events: typed once the second
        // unit's down is taken, its first alone is half a character.
        assert_eq!(typed_of("a\u{1F600}b", 4), (1, 3));
        assert_eq!(typed_of("a\u{1F600}b", 5), (2, 3));
        assert_eq!(typed_of("a\u{1F600}b", 6), (2, 3));
        assert_eq!(typed_of("a\u{1F600}b", 7), (3, 3));
        let e = HostError::with(&errors::input::TEXT_NOT_TAKEN, "Windows did not take the input after 1 of 3 characters (error 5)".into());
        assert_eq!(e.code(), errors::Code::Failed);
    }

    #[test]
    fn a_pointer_place_windows_does_not_give_is_the_origin() {
        assert_eq!(cursor_or_origin(Some((12, -40)), || panic!("asked")), (12, -40));
        assert_eq!(cursor_or_origin(None, || 5), (0, 0));
    }

    /// Every call that goes through the system's input answers it: no `SendInput` or
    /// `SetCursorPos` whose answer is dropped is left, and the calls build their events with the
    /// builders that `send_inputs` sends.
    #[test]
    fn no_input_s_answer_is_dropped() {
        const SRC: &str = include_str!("windows.rs");
        const CLICK: &str = include_str!("click_win.rs");
        let code = &SRC[..SRC.find("\n#[cfg(test)]\nmod ").unwrap()];
        assert!(!code.contains("fn send_mouse_event(") && !code.contains("fn send_key_event(") && !code.contains("fn send_motion("));
        let sent = code.matches("SendInput(").count();
        // send_inputs, send_marked (the keys passed on, which reads its count) and the mask key.
        assert_eq!(sent, 3, "a SendInput whose answer is not read");
        assert!(code.contains("let took = unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) } as usize;"));
        // Every placement answers: the one in `place`, the one a posted click moves the pointer with.
        assert_eq!(code.matches("SetCursorPos(").count(), 1);
        assert!(code.contains("if unsafe { SetCursorPos(x, y) } != 0 {"));
        // A posted click's placement is read too: the click is posted either way, and its line says
        // whether the pointer moved.
        assert!(CLICK.contains("} else if SetCursorPos(x, y) != 0 {"));
        assert!(!CLICK.contains("SetCursorPos(x, y);"));
        let body = |sig: &str| crate::ocr_wiring_tests::body(SRC, sig);
        for sig in [
            "fn mouse_move(&self, x: i32, y: i32)",
            "fn mouse_drag(&self, x1: i32, y1: i32, x2: i32, y2: i32, button: MouseButton)",
            "fn mouse_down(&self, x: i32, y: i32, button: MouseButton)",
            "fn mouse_up(&self, x: i32, y: i32, button: MouseButton)",
            "fn mouse_scroll(&self, x: i32, y: i32, delta: i32)",
        ] {
            assert!(body(sig).contains(".answer()"), "{sig}");
        }
        // A send's answer, and what of it went out, are read in `key_sent`.
        assert!(body("fn key_send(&self, combo: &str)").contains("key_sent(&send_inputs(&events, &[]))"));
        assert!(body("fn key_sent(sent: &Sent)").contains("sent.answer()"));
        assert!(body("fn type_text(&self, text: &str)").contains("let sent = send_inputs(&events, &[]);"));
        // Placed nowhere, nothing is pressed or scrolled: the placement comes first, with `?`.
        for sig in ["fn mouse_down(&self, x: i32, y: i32, button: MouseButton)", "fn mouse_scroll(&self, x: i32, y: i32, delta: i32)", "pub(super) fn click_input("] {
            let b = body(sig);
            assert!(b.find("place(").unwrap() < b.find("send_inputs(").unwrap() && b.contains("?;"), "{sig}");
        }
    }
}
