//! What a call answers when something it was asked to do was not done: an **error value**,
//! `{ code, message, detail? }` (docs/api/errors.md). This file holds what every one of them is
//! made from, pure: the codes ([`Code`]), every reason the host can give ([`Reason`], one constant
//! each, all of them in [`REASONS`]), the groups the docs name them by ([`GROUPS`]), and the error
//! the host carries until it reaches a VM ([`HostError`]). The table a module gets, `host.errors`
//! and the text calls that take an error value are errors_lua.rs's.
//!
//! **The Reason table is the single source.** A [`HostError`] can only be made from a Reason of
//! [`REASONS`], so a reason no table names cannot reach a module, and tools/api-index.py compares
//! every docs/api entry's Errors table with the Reasons whose `entries` name it, and each group of
//! [`GROUPS`] with its own section, in both directions: a row with no Reason behind it fails, and
//! so does a Reason with no row. Three
//! things keep every Reason in the table: a Reason is written only in this file (a test fails on
//! one written anywhere else), every one written here is in [`REASONS`] (a test counts them,
//! whatever their visibility), and [`HostError::of`] and [`HostError::with`] assert in a debug
//! build that theirs is. The tool reads this file as text, so a Reason is written the way it reads
//! one — `pub const NAME: Reason = Reason { code: Code::X, message: "…", entries: … };`, the
//! message on one line, the entries a list of strings or the name of a `const` list of this file —
//! and a [`Code`] is named in `as_str` and `constant` by one arm each.
//!
//! **A message is word for word what modules get today** (D49 of the typed-errors plan), with
//! `…` for each part the producer formats: a name, a size, an error number, the system's own
//! text. A number fixed by a constant is written out, so the docs can show it; the producer's
//! debug assertion ([`HostError::with`]) keeps the two together.
//!
//! **Every namespace answers with HostError**: host.gui, host.window.focus, host.element,
//! host.input, host.keys, host.hotkey, host.screen, host.ocr, host.speech.use, host.sound.play and
//! host.gamepad. A Reason whose `entries` are empty is one no module is handed; it says why.

use std::borrow::Cow;
use std::fmt;

/// What kind of failure an error value reports: what a module compares (`err.code ==
/// host.errors.NOT_READY`), never the message. A code never names a platform; the message may.
///
/// Three tell the module what the user can do: [`NotSupported`](Code::NotSupported) (nothing, here),
/// [`Unavailable`](Code::Unavailable) (install it or switch it on) and [`Denied`](Code::Denied)
/// (grant this application a right). The others describe the moment or the target. Each one's
/// meaning is docs/api/errors.md's, which a check holds to this list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Code {
    NotReady,
    NotSupported,
    Unavailable,
    Denied,
    Refused,
    Busy,
    TooMany,
    TooLarge,
    Replaced,
    Timeout,
    Gone,
    Hidden,
    Outside,
    Covered,
    NotFound,
    Taken,
    Invalid,
    Cancelled,
    Failed,
}

impl Code {
    /// Every code, in the order docs/api/errors.md lists them.
    pub const ALL: [Code; 19] = [
        Code::NotReady,
        Code::NotSupported,
        Code::Unavailable,
        Code::Denied,
        Code::Refused,
        Code::Busy,
        Code::TooMany,
        Code::TooLarge,
        Code::Replaced,
        Code::Timeout,
        Code::Gone,
        Code::Hidden,
        Code::Outside,
        Code::Covered,
        Code::NotFound,
        Code::Taken,
        Code::Invalid,
        Code::Cancelled,
        Code::Failed,
    ];

    /// The code as an error value carries it: lower-case snake_case.
    pub const fn as_str(self) -> &'static str {
        match self {
            Code::NotReady => "not_ready",
            Code::NotSupported => "not_supported",
            Code::Unavailable => "unavailable",
            Code::Denied => "denied",
            Code::Refused => "refused",
            Code::Busy => "busy",
            Code::TooMany => "too_many",
            Code::TooLarge => "too_large",
            Code::Replaced => "replaced",
            Code::Timeout => "timeout",
            Code::Gone => "gone",
            Code::Hidden => "hidden",
            Code::Outside => "outside",
            Code::Covered => "covered",
            Code::NotFound => "not_found",
            Code::Taken => "taken",
            Code::Invalid => "invalid",
            Code::Cancelled => "cancelled",
            Code::Failed => "failed",
        }
    }

    /// The name of its constant in `host.errors` and the page's `platform.errors`: the code in
    /// upper case. Written out, one arm each, because check-docs.ps1 reads these arms to know
    /// which `host.errors.*` names exist.
    pub const fn constant(self) -> &'static str {
        match self {
            Code::NotReady => "NOT_READY",
            Code::NotSupported => "NOT_SUPPORTED",
            Code::Unavailable => "UNAVAILABLE",
            Code::Denied => "DENIED",
            Code::Refused => "REFUSED",
            Code::Busy => "BUSY",
            Code::TooMany => "TOO_MANY",
            Code::TooLarge => "TOO_LARGE",
            Code::Replaced => "REPLACED",
            Code::Timeout => "TIMEOUT",
            Code::Gone => "GONE",
            Code::Hidden => "HIDDEN",
            Code::Outside => "OUTSIDE",
            Code::Covered => "COVERED",
            Code::NotFound => "NOT_FOUND",
            Code::Taken => "TAKEN",
            Code::Invalid => "INVALID",
            Code::Cancelled => "CANCELLED",
            Code::Failed => "FAILED",
        }
    }
}

/// One reason the host can give: its code, its message, and the docs entries that give it.
#[derive(Debug, PartialEq, Eq)]
pub struct Reason {
    pub code: Code,
    /// The message, word for word; `…` stands for each part the producer formats.
    pub message: &'static str,
    /// The docs entries that give it — entry anchors such as `"host-screen-pixel"` — or a group
    /// of [`GROUPS`] such as `"capture-errors"`. Empty for a reason no module can be handed,
    /// which each such Reason says.
    pub entries: &'static [&'static str],
}

// ── The reasons, by namespace ────────────────────────────────────────────────────────────────

/// Reasons more than one namespace gives, in the same words.
pub mod common {
    use super::{Code, Reason};

    /// A platform, run mode or system that has no way to do it: the stub build of other systems,
    /// a page or a press on a Mac before it is built, a click "post" or "noactivate" there.
    pub const NOT_YET: Reason = Reason {
        code: Code::NotSupported,
        message: "not supported on this platform yet",
        entries: &["page-show", "page-prepare", "page-focus", "page-post", "host-window-focus", "host-element-press", "host-input-click", "host-input-send"],
    };
    /// The system would not bring the window to the front.
    pub const DECLINED: Reason = Reason {
        code: Code::Refused,
        message: "declined by the system",
        entries: &["page-show", "page-focus", "host-window-focus"],
    };
    /// The window the call was given is no longer there.
    pub const WINDOW_GONE: Reason = Reason {
        code: Code::Gone,
        message: "the window is gone",
        entries: &["host-element-press", "host-input-click", "host-input-post"],
    };
}

/// `host.gui`: a page, and the page's own side.
pub mod gui {
    use super::{Code, Reason};

    pub const HEADLESS: Reason = Reason { code: Code::NotSupported, message: "headless", entries: &["page-show", "page-prepare", "page-focus", "page-post"] };
    pub const RUNTIME_MISSING: Reason = Reason { code: Code::Unavailable, message: "WebView2 runtime not installed", entries: &["page-show", "page-prepare"] };
    pub const NOT_READY: Reason = Reason { code: Code::NotReady, message: "not ready yet", entries: &["page-show", "page-prepare"] };
    pub const BUILD_FAILED: Reason = Reason { code: Code::Failed, message: "could not be built", entries: &["page-show", "page-prepare"] };
    /// A show without `focus`: the page is hidden.
    pub const NO_ROOM: Reason = Reason { code: Code::TooLarge, message: "no room beside the rectangle to avoid", entries: &["page-show"] };
    /// A show with `focus` that ended in front, over part of `avoid`: the partial answer `true, err`.
    pub const OVER: Reason = Reason { code: Code::TooLarge, message: "over the rectangle to avoid", entries: &["page-show"] };
    pub const GONE: Reason = Reason {
        code: Code::Gone,
        message: "this page no longer exists: its module was reloaded",
        entries: &["page-show", "page-prepare", "page-focus", "page-post"],
    };
    pub const NOT_BUILT: Reason = Reason { code: Code::NotReady, message: "the page is not built yet", entries: &["page-focus"] };
    pub const HIDDEN: Reason = Reason { code: Code::Hidden, message: "the page is hidden", entries: &["page-focus"] };
    pub const NOT_HANDED: Reason = Reason { code: Code::Failed, message: "a message could not be handed to the page: …", entries: &["page-post"] };
    /// onClose's `"failed"`, beside it.
    pub const FAILED: Reason = Reason { code: Code::Failed, message: "the page failed", entries: &["host-gui-webview"] };
    /// What the page's own `platform.post` answers for a message past `webview::MAX_MESSAGE_BYTES`.
    pub const PAGE_MESSAGE_TOO_LARGE: Reason = Reason {
        code: Code::TooLarge,
        message: "a message of … bytes was not sent — the most one may be is 1048576",
        entries: &["platform-post"],
    };
}

/// `host.window`.
pub mod window {
    use super::{Code, Reason};

    pub const NO_SUCH_WINDOW: Reason = Reason { code: Code::Gone, message: "no such window", entries: &["host-window-focus"] };
    /// A hidden window of this application.
    pub const HIDDEN: Reason = Reason { code: Code::Hidden, message: "the window is hidden", entries: &["host-window-focus"] };
}

/// Every call of `host.element` that puts a question to a program on Windows: the members of the
/// group `question-errors`, the reasons every such question can give. `focusWithin` is asked on the
/// accessibility thread too, but asks the program nothing there (uia.rs answers it `None`), so it
/// gives only the host's own of them, which name it.
pub const QUESTIONS: &[&str] = &[
    "host-element-find",
    "host-element-locate",
    "host-element-findany",
    "host-element-locatevia",
    "host-element-classnavpoint",
    "host-element-focusstep",
    "host-element-pluginlocate",
    "host-element-stateprobe",
    "host-element-dump",
    "host-element-rawdump",
    "host-element-press",
];

/// The questions that walk the tree within the 4000-node and 5-second bounds on Windows.
pub const WALKS: &[&str] = &["host-element-pluginlocate", "host-element-stateprobe", "host-element-rawdump"];

/// The questions a Mac asks an application's tree: every one but `press`, which is not there yet.
pub const MAC_QUESTIONS: &[&str] = &[
    "host-element-find",
    "host-element-locate",
    "host-element-findany",
    "host-element-locatevia",
    "host-element-classnavpoint",
    "host-element-focusstep",
    "host-element-focuswithin",
    "host-element-pluginlocate",
    "host-element-stateprobe",
    "host-element-dump",
    "host-element-rawdump",
];

/// The Mac's searches, each walk of them bounded at 1500 elements and 300 ms (macos/ax.rs,
/// `QUERY_NODES` and `HOT_DEADLINE`).
pub const MAC_SEARCHES: &[&str] = &[
    "host-element-find",
    "host-element-locate",
    "host-element-findany",
    "host-element-locatevia",
    "host-element-classnavpoint",
    "host-element-pluginlocate",
    "host-element-stateprobe",
];

/// The Mac's walks bounded at 300 ms: its searches, and the focus's two at 800 elements
/// (`FOCUS_NODES`).
pub const MAC_HOT_WALKS: &[&str] = &[
    "host-element-find",
    "host-element-locate",
    "host-element-findany",
    "host-element-locatevia",
    "host-element-classnavpoint",
    "host-element-pluginlocate",
    "host-element-stateprobe",
    "host-element-focusstep",
    "host-element-focuswithin",
];

/// `host.element`.
pub mod element {
    use super::{Code, Reason, MAC_HOT_WALKS, MAC_QUESTIONS, MAC_SEARCHES, WALKS};

    // The reasons every question can give on Windows: the group `question-errors` — and those the
    // host gives without asking the program, `focusWithin` too.
    pub const TIMEOUT: Reason = Reason { code: Code::Timeout, message: "the program did not answer within 1000 ms", entries: &["question-errors"] };
    pub const DENIED: Reason = Reason { code: Code::Denied, message: "the program runs with higher rights", entries: &["question-errors"] };
    pub const PROGRAM_GONE: Reason = Reason { code: Code::Gone, message: "the program is gone", entries: &["question-errors"] };
    /// A question behind one the program did not answer, with that one's code: its timeout.
    pub const NOT_ASKED_TIMEOUT: Reason = Reason {
        code: Code::Timeout,
        message: "… to an earlier question, so this one was not asked",
        entries: &["question-errors", "host-element-focuswithin"],
    };
    /// The same, behind one refused for higher rights: never given, since only a timeout leaves the
    /// questions behind it unasked (element.rs, `Queue::finish`, and uia.rs, `noted`).
    pub const NOT_ASKED_DENIED: Reason = Reason { code: Code::Denied, message: "… to an earlier question, so this one was not asked", entries: &[] };
    /// The same, behind one whose program went: never given, for the same reason.
    pub const NOT_ASKED_GONE: Reason = Reason { code: Code::Gone, message: "… to an earlier question, so this one was not asked", entries: &[] };
    /// With `opts.key` and a callback only.
    pub const REPLACED: Reason = Reason {
        code: Code::Replaced,
        message: "a newer question with the same key was asked before this one",
        entries: &["question-errors", "host-element-focuswithin"],
    };
    /// Without a callback, asked inside another one's wait.
    pub const NESTED: Reason = Reason { code: Code::Busy, message: "another accessibility question is being waited for", entries: &["question-errors", "host-element-focuswithin"] };
    pub const THREAD_NOT_STARTED: Reason = Reason {
        code: Code::Failed,
        message: "the host could not start its accessibility thread: …",
        entries: &["question-errors", "host-element-focuswithin"],
    };
    pub const OWN_WINDOW: Reason = Reason {
        code: Code::Refused,
        message: "the window is this application's own, which is not asked about",
        entries: &["question-errors", "host-element-focuswithin"],
    };
    pub const NO_SIGNAL: Reason = Reason {
        code: Code::Failed,
        message: "the event the host waits on for the answer could not be made",
        entries: &["question-errors", "host-element-focuswithin"],
    };
    pub const LOST: Reason = Reason { code: Code::Failed, message: "the answer was lost", entries: &["question-errors", "host-element-focuswithin"] };
    pub const THREAD_FAILED: Reason = Reason { code: Code::Failed, message: "the host's accessibility thread failed: …", entries: &["question-errors", "host-element-focuswithin"] };
    /// UI Automation's object could not be made on the accessibility thread, or a condition of the
    /// question could not be built: nothing can be asked.
    pub const NO_INTERFACE: Reason = Reason {
        code: Code::Failed,
        message: "this system's accessibility interface could not be reached",
        entries: &["question-errors"],
    };
    /// The Windows backend's own answer on the event loop, where no question is asked: never
    /// reaches a module.
    pub const WRONG_THREAD: Reason = Reason { code: Code::Failed, message: "accessibility questions are asked on the host's accessibility thread", entries: &[] };
    pub const TOO_MANY_NODES: Reason = Reason { code: Code::TooLarge, message: "the tree holds more than the 4000 elements a search reads", entries: WALKS };
    /// A Windows walk past its 5 s, and a Mac's dump past its own 5 s (macos/ax.rs, `DUMP_DEADLINE`).
    pub const TOO_SLOW: Reason = Reason {
        code: Code::Timeout,
        message: "the search took longer than 5 s",
        entries: &["host-element-pluginlocate", "host-element-stateprobe", "host-element-rawdump", "host-element-dump"],
    };
    /// A dump's list so far beside it: a partial answer.
    pub const DUMP_CUT: Reason = Reason { code: Code::TooLarge, message: "the tree holds … elements, more than the 2000 a dump lists", entries: &["host-element-dump"] };

    // A Mac's (macos/ax.rs): a question it did not get to the end of. Each answers `nil, err` where
    // the question found nothing, and a dump its list so far beside it.
    /// A read of the question's that the application did not answer: it is busy, starting or not
    /// responding (`kAXErrorCannotComplete`).
    pub const NOT_ANSWERING: Reason = Reason {
        code: Code::Timeout,
        message: "the program did not answer: it is busy, starting or not responding",
        entries: MAC_QUESTIONS,
    };
    /// An application that did not answer a moment ago is not asked again for 5 s (macos/ax.rs,
    /// `BUSY_PENALTY`): the question is not put to it.
    pub const QUARANTINED: Reason = Reason {
        code: Code::Timeout,
        message: "the program did not answer a moment ago, so it is not asked for 5 s",
        entries: MAC_QUESTIONS,
    };
    pub const SEARCH_CUT: Reason = Reason { code: Code::TooLarge, message: "the tree holds more than the 1500 elements a search reads", entries: MAC_SEARCHES };
    pub const FOCUS_CUT: Reason = Reason {
        code: Code::TooLarge,
        message: "the window holds more than the 800 elements a focus search reads",
        entries: &["host-element-focusstep", "host-element-focuswithin"],
    };
    pub const HOT_TOO_SLOW: Reason = Reason { code: Code::Timeout, message: "the search took longer than 300 ms", entries: MAC_HOT_WALKS };
    /// Beside the list read so far: a partial answer.
    pub const MAC_DUMP_CUT: Reason = Reason {
        code: Code::TooLarge,
        message: "the tree holds more than the 2000 elements a dump reads",
        entries: &["host-element-dump", "host-element-rawdump"],
    };
    pub const MOVES_FOCUS: Reason = Reason {
        code: Code::NotSupported,
        message: "this system cannot press an element without moving the keyboard focus to it",
        entries: &["host-element-press"],
    };
    pub const NO_ELEMENT: Reason = Reason { code: Code::NotFound, message: "no such element", entries: &["host-element-press"] };
    pub const CANNOT_PRESS: Reason = Reason { code: Code::Refused, message: "the element cannot be pressed", entries: &["host-element-press"] };
    pub const NOT_ENABLED: Reason = Reason { code: Code::Refused, message: "the element is not enabled", entries: &["host-element-press"] };
    pub const ELEMENT_GONE: Reason = Reason { code: Code::Gone, message: "the element is gone", entries: &["host-element-press"] };
    pub const NOT_TAKEN: Reason = Reason { code: Code::Failed, message: "the program did not take the action (0x…)", entries: &["host-element-press"] };
}

/// `host.input`. A click's reasons come at the call or, after a "noactivate" hand-over, to the
/// callback only; which is which is the docs' "arrives".
pub mod input {
    use super::{Code, Reason};

    pub const OUTSIDE: Reason = Reason { code: Code::Outside, message: "the point is not in the window", entries: &["host-input-click"] };
    pub const COVERED: Reason = Reason { code: Code::Covered, message: "something else is drawn over the point", entries: &["host-input-click"] };
    pub const FRAME: Reason = Reason { code: Code::Outside, message: "the point is on the window's frame, not in its client area", entries: &["host-input-click"] };
    /// Windows' error 5: a click, a style change or a posted key to a program of higher rights.
    pub const HIGHER: Reason = Reason { code: Code::Denied, message: "the window's program runs with higher rights", entries: &["host-input-click", "host-input-post"] };
    pub const MESSAGE_NOT_TAKEN: Reason = Reason { code: Code::Failed, message: "the window did not take the message (error …)", entries: &["host-input-click", "host-input-post"] };
    pub const STYLE_NOT_TAKEN: Reason = Reason { code: Code::Failed, message: "the window did not take the style (error …)", entries: &["host-input-click"] };
    pub const MAIN_WINDOW: Reason = Reason {
        code: Code::Refused,
        message: "the window belongs to a program's main window, which never gets the no-activate style: only a window another window owns — a plug-in's floating window — does",
        entries: &["host-input-click"],
    };
    pub const ENDING: Reason = Reason { code: Code::Cancelled, message: "the application is ending", entries: &["host-input-click"] };
    pub const NO_ANSWER_WITHIN: Reason = Reason { code: Code::Timeout, message: "the window's program did not answer within … ms", entries: &["host-input-click"] };
    pub const NO_ANSWER_ERROR: Reason = Reason { code: Code::Failed, message: "the window's program did not answer (error …)", entries: &["host-input-click"] };
    pub const NOT_KEPT: Reason = Reason { code: Code::Refused, message: "the window did not keep the no-activate style", entries: &["host-input-click"] };
    pub const RESTORE_LIST: Reason = Reason {
        code: Code::Failed,
        message: "the list of windows to give their style back could not be written (…)",
        entries: &["host-input-click"],
    };
    pub const STYLE_THREAD_GONE: Reason = Reason { code: Code::Failed, message: "the host's window-style thread is gone", entries: &["host-input-click"] };
    pub const STYLE_THREAD_NOT_STARTED: Reason = Reason { code: Code::Failed, message: "the host could not start its window-style thread: …", entries: &["host-input-click"] };
    pub const STYLE_THREAD_FAILED: Reason = Reason { code: Code::Failed, message: "the host's thread for it failed: …", entries: &["host-input-click"] };
    pub const NO_POST_ACCESS: Reason = Reason {
        code: Code::Denied,
        message: "this application may not post events: Accessibility is not granted to it (System Settings > Privacy & Security > Accessibility)",
        entries: &["host-input-click"],
    };
    pub const NOT_PAIRED: Reason = Reason { code: Code::Hidden, message: "the window has no number in the window server's list", entries: &["host-input-click"] };
    pub const NOT_ON_SCREEN: Reason = Reason { code: Code::Hidden, message: "the window is not on screen", entries: &["host-input-click"] };
    pub const NOT_ANSWERING: Reason = Reason { code: Code::Timeout, message: "the window's program did not answer", entries: &["host-input-click"] };
    pub const NO_WINDOW_LOCATION: Reason = Reason {
        code: Code::NotSupported,
        message: "this macOS cannot give a posted click its place in the window (CGEventSetWindowLocation is missing)",
        entries: &["host-input-click"],
    };
    pub const NULL_MOUSE_EVENT: Reason = Reason { code: Code::Failed, message: "CGEventCreateMouseEvent(type …) returned NULL; nothing was posted", entries: &["host-input-click"] };
    /// The macOS backend's answer off the event loop's thread, where no module asks: never reaches one.
    pub const OFF_MAIN_THREAD: Reason = Reason { code: Code::Failed, message: "a posted click is made on the event loop's thread only", entries: &[] };
    /// A key no key of the layout types, or one macOS has no key code for.
    pub const POST_NO_KEYCODE: Reason = Reason { code: Code::NotSupported, message: "key '…' (VK …): …", entries: &["host-input-post"] };
    pub const POST_NO_PROCESS: Reason = Reason {
        code: Code::Gone,
        message: "key_post: … is not a handle this backend issued, so there is no process to post '…' to",
        entries: &["host-input-post"],
    };
    pub const NULL_KEY_EVENT: Reason = Reason {
        code: Code::Failed,
        message: "CGEventCreateKeyboardEvent returned NULL for '…' (keycode …)",
        entries: &["host-input-post", "host-input-send"],
    };
    pub const SEND_NO_KEYCODE: Reason = Reason { code: Code::NotSupported, message: "'…' (VK …): …", entries: &["host-input-send"] };
    /// Windows' `SendInput` took fewer of a call's events than it was given, with its last error.
    pub const NOT_TAKEN: Reason = Reason {
        code: Code::Failed,
        message: "Windows did not take the input (error …)",
        entries: &[
            "host-input-move",
            "host-input-click",
            "host-input-drag",
            "host-input-mousedown",
            "host-input-mouseup",
            "host-input-scroll",
            "host-input-send",
        ],
    };
    /// Windows took a click's or a drag's press and refused its release, sent with the call and
    /// again on its own: the button is down, and the host holds it for the module, as a
    /// mouseDown's (lib.rs, `held_after`).
    pub const LEFT_DOWN: Reason = Reason {
        code: Code::Failed,
        message: "Windows took the press but not its release (error …): the button is still down",
        entries: &["host-input-click", "host-input-drag"],
    };
    /// Windows' `SetCursorPos` refused, with its last error: nothing after it is sent.
    pub const NOT_PLACED: Reason = Reason {
        code: Code::Failed,
        message: "Windows did not put the pointer there (error …)",
        entries: &["host-input-move", "host-input-click", "host-input-drag", "host-input-mousedown", "host-input-scroll"],
    };
    /// `host.input.text` on Windows, typed as far as `SendInput` took its events.
    pub const TEXT_NOT_TAKEN: Reason = Reason {
        code: Code::Failed,
        message: "Windows did not take the input after … of … characters (error …)",
        entries: &["host-input-text"],
    };
    /// macOS: a mouse event of the call the system would not make — every one is made before the
    /// first is posted, so nothing of the call was sent.
    pub const NULL_MOUSE_SENT: Reason = Reason {
        code: Code::Failed,
        message: "CGEventCreateMouseEvent(type …) returned NULL; nothing was sent",
        entries: &["host-input-move", "host-input-click", "host-input-drag", "host-input-mousedown", "host-input-mouseup", "host-input-scroll"],
    };
    pub const NULL_SCROLL_EVENT: Reason = Reason { code: Code::Failed, message: "CGEventCreateScrollWheelEvent2 returned NULL; nothing was sent", entries: &["host-input-scroll"] };
    /// macOS: `host.input.text`, typed up to the character whose event the system would not make.
    pub const TEXT_CUT: Reason = Reason {
        code: Code::Failed,
        message: "CGEventCreateKeyboardEvent returned NULL while typing; … of … characters were sent",
        entries: &["host-input-text"],
    };
}

/// `host.keys.capture`, without the hook or the tap.
pub mod keys {
    use super::{Code, Reason};

    pub const HOOK_THREAD: Reason = Reason { code: Code::Failed, message: "could not start the keyboard hook's thread: …", entries: &["host-keys-capture"] };
    pub const HOOK_ENDED: Reason = Reason { code: Code::Failed, message: "the keyboard hook's thread ended before it answered", entries: &["host-keys-capture"] };
    pub const HOOK_REFUSED: Reason = Reason { code: Code::Failed, message: "SetWindowsHookExW(WH_KEYBOARD_LL) failed (error …)", entries: &["host-keys-capture"] };
    pub const TAP_REFUSED: Reason = Reason {
        code: Code::Denied,
        message: "CGEventTapCreate refused (AXIsProcessTrusted = …) — grant this application Accessibility in System Settings > Privacy & Security, and Input Monitoring too if keys are still not captured after that; until then no key can be captured or suppressed",
        entries: &["host-keys-capture"],
    };
    pub const TAP_SOURCE: Reason = Reason { code: Code::Failed, message: "CFMachPortCreateRunLoopSource failed for the event tap", entries: &["host-keys-capture"] };
    pub const TAP_NO_RUN_LOOP: Reason = Reason { code: Code::Failed, message: "no CFRunLoop to attach the event tap to", entries: &["host-keys-capture"] };
}

/// `host.hotkey.register`. The first two answer `nil, err`; the claims made but not held now
/// answer `id, err`, a partial answer.
pub mod hotkey {
    use super::{Code, Reason};

    pub const KEPT_BY_SYSTEM: Reason = Reason {
        code: Code::Taken,
        message: "the hotkey '…' (…) is kept by the system and cannot be registered: …. Choose another combination",
        entries: &["host-hotkey-register"],
    };
    pub const NO_KEYCODE: Reason = Reason {
        code: Code::NotSupported,
        message: "the hotkey '…' (…) cannot be registered: macOS has no key code for …. Choose another key",
        entries: &["host-hotkey-register"],
    };
    /// Windows: the keypad's Enter, which has no virtual key of its own for `RegisterHotKey`.
    pub const NO_VK: Reason = Reason {
        code: Code::NotSupported,
        message: "the hotkey '…' (…) cannot be registered: Windows has no virtual key for the keypad's Enter apart from Enter's, and RegisterHotKey would take both. Capture it with host.keys.capture, or choose another key",
        entries: &["host-hotkey-register"],
    };
    pub const MODULE_HOLDS: Reason = Reason { code: Code::Taken, message: "module '…' holds this combination", entries: &["host-hotkey-register"] };
    pub const REGISTER_REFUSED: Reason = Reason { code: Code::Taken, message: "RegisterHotKey failed for '…' (already in use by another app?)", entries: &["host-hotkey-register"] };
    pub const MAC_REFUSED: Reason = Reason {
        code: Code::Taken,
        message: "macOS refused the shortcut '…' (RegisterEventHotKey returned …); another application probably already uses it system-wide",
        entries: &["host-hotkey-register"],
    };
    /// A macOS letter the current layout does not type: parked until one does.
    pub const PARKED: Reason = Reason { code: Code::NotSupported, message: "no key of the current keyboard layout types this letter", entries: &["host-hotkey-register"] };
    pub const STUB: Reason = Reason { code: Code::NotSupported, message: "hotkeys are not implemented on this platform yet (spec '…')", entries: &["host-hotkey-register"] };
    /// macOS: the Carbon handler every hotkey is delivered through could not be installed, so
    /// no claim is held until one installs it.
    pub const NO_HANDLER: Reason = Reason {
        code: Code::Failed,
        message: "could not install the Carbon hotkey handler (InstallEventHandler returned …); no global shortcut will work in this session",
        entries: &["host-hotkey-register"],
    };
}

/// `host.screen`. Most reasons are shared by many calls, so the docs name five groups once
/// (capture, window form, snapshot source, request limits, image worker), and a sixth for desktop
/// duplication's, which a module meets only under `[screen] fallback = "none"`. A reason only some
/// of a group's calls give names those calls instead: the `screen-capture` thread's own failures,
/// desktop duplication's 40 million pixels, and the snapshot reason of the calls that need the
/// whole region inside it.
pub mod screen {
    use super::{Code, Reason};

    // Capture errors: any call of this namespace that captures a region; and a text read's.
    pub const CAPTURE_FAILED: Reason = Reason {
        code: Code::Failed,
        message: "screen capture failed",
        entries: &["capture-errors", "host-screen-pixels", "host-ocr-recognize"],
    };
    /// Desktop duplication's 40 million pixels (dxgi.rs, `fits`, per region): only the calls that
    /// do not hold their region to that or less before they capture — the searches, `save` and
    /// `saveMarked`. A snapshot's and a profile's region is held to `frame::MAX_FRAME_PIXELS`, a
    /// grid's to `cells::MAX_REGION_PIXELS`, a captured template's to `template::MAX_PIXELS`, and a
    /// text read reads at most that many in all (`ocr::CALL_TOO_LARGE` comes first): windows.rs's
    /// `only_the_calls_that_do_not_hold_their_region_meet_the_40_million` holds the limits to it.
    pub const CAPTURE_TOO_LARGE: Reason = Reason {
        code: Code::TooLarge,
        message: "screen capture failed: the region is larger than 40 million pixels",
        entries: &["host-screen-imagesearch", "host-screen-imagesearchall", "host-screen-imagesearcheach", "host-screen-save", "host-screen-savemarked"],
    };
    /// The `screen-capture` thread's: its pictures and its points, never the image worker's.
    pub const CAPTURE_PANICKED: Reason = Reason {
        code: Code::Failed,
        message: "the screen capture failed inside the application; the log has the details",
        entries: &["host-screen-snapshot", "host-screen-profile", "host-ocr-recognize", "host-screen-pixel", "host-screen-pixels"],
    };
    /// A snapshot round's (ocr/snap_queue.rs): a snapshot's or a profile's.
    pub const ROUND_FAILED: Reason = Reason {
        code: Code::Failed,
        message: "the snapshot failed inside the application; the log has the details",
        entries: &["host-screen-snapshot", "host-screen-profile"],
    };

    // Duplication errors, Windows only: the whole sentence, its code from its tail (dxgi.rs, `Fallback::describe`).
    pub const DUPLICATION_SWITCHED_OFF: Reason = Reason {
        code: Code::Unavailable,
        message: "screen capture failed: desktop duplication could not answer — it is switched off in Application settings",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_DISABLED: Reason = Reason {
        code: Code::Unavailable,
        message: "screen capture failed: desktop duplication could not answer — it stopped after three reads hung within ten minutes; it is tried again after a while, after a resume, an unlock or a display change, or when it is switched off and on again in Application settings",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_CRASHED: Reason = Reason {
        code: Code::Failed,
        message: "screen capture failed: desktop duplication could not answer — its thread stopped on an internal error; switching it off and on again in Application settings starts it afresh",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_NO_THREAD: Reason = Reason {
        code: Code::Failed,
        message: "screen capture failed: desktop duplication could not answer — its thread could not be started",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_CLOSING: Reason = Reason {
        code: Code::Cancelled,
        message: "screen capture failed: desktop duplication could not answer — the application is closing",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_OVERDUE: Reason = Reason {
        code: Code::Busy,
        message: "screen capture failed: desktop duplication could not answer — an earlier read has not come back yet",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_UNSUPPORTED: Reason = Reason {
        code: Code::NotSupported,
        message: "screen capture failed: desktop duplication could not answer — Windows does not support it here (0x…): over Remote Desktop, or on a laptop with two graphics chips where this application runs on the one that does not drive the display — Windows Settings, System, Display, Graphics, this application, Power saving",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_TOO_MANY_RECORDERS: Reason = Reason {
        code: Code::Unavailable,
        message: "screen capture failed: desktop duplication could not answer — four other programs already record the screen — a screen recorder, a screen-sharing call, another reader",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_SECURE_DESKTOP: Reason = Reason {
        code: Code::Denied,
        message: "screen capture failed: desktop duplication could not answer — the secure desktop is showing (a UAC prompt or the lock screen)",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_ACCESS_LOST: Reason = Reason {
        code: Code::Failed,
        message: "screen capture failed: desktop duplication could not answer — the picture was lost to a display mode change, a fullscreen switch or a desktop switch",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_DEVICE_REMOVED: Reason = Reason {
        code: Code::Failed,
        message: "screen capture failed: desktop duplication could not answer — the graphics driver was reset or updated",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_ROTATED: Reason = Reason {
        code: Code::NotSupported,
        message: "screen capture failed: desktop duplication could not answer — the region touches a rotated monitor, which this version reads only the standard way",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_TOPOLOGY: Reason = Reason {
        code: Code::NotSupported,
        message: "screen capture failed: desktop duplication could not answer — the region touches a monitor desktop duplication does not report",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_NO_FRAME_YET: Reason = Reason {
        code: Code::NotReady,
        message: "screen capture failed: desktop duplication could not answer — no picture has arrived since it was opened",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_NO_OUTPUT: Reason = Reason {
        code: Code::NotSupported,
        message: "screen capture failed: desktop duplication could not answer — no display output was found",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_FORMAT: Reason = Reason {
        code: Code::NotSupported,
        message: "screen capture failed: desktop duplication could not answer — the display delivers pixel format …, which this version cannot read",
        entries: &["duplication-errors"],
    };
    pub const DUPLICATION_FAILED: Reason = Reason {
        code: Code::Failed,
        message: "screen capture failed: desktop duplication could not answer — it failed with 0x…",
        entries: &["duplication-errors"],
    };

    // Window-form errors: any call given a window region or point.
    pub const CLIENT_EMPTY: Reason = Reason { code: Code::Hidden, message: "the window's client area is empty (…x…)", entries: &["window-form-errors"] };
    pub const OUT_OF_RANGE: Reason = Reason { code: Code::Outside, message: "the region lies outside the screen's coordinate range", entries: &["window-form-errors"] };

    // Snapshot-source errors: a call given `opts.snapshot` that reads the part of its region inside
    // it; and those that need the whole region inside, a snapshot's `crop` among them.
    pub const NOT_INSIDE: Reason = Reason {
        code: Code::Outside,
        message: "the region is not inside the snapshot",
        entries: &["host-screen-cells", "host-screen-matchcells", "host-screen-template", "snapshot-crop"],
    };
    pub const NO_OVERLAP: Reason = Reason { code: Code::Outside, message: "the region does not overlap the snapshot", entries: &["snapshot-source-errors"] };
    pub const POINT_NOT_INSIDE: Reason = Reason { code: Code::Outside, message: "the point is not inside the snapshot", entries: &["host-screen-pixel", "host-screen-pixels"] };

    // Request-limit errors: the requests of the screen-capture thread; and the image worker's.
    pub const REPLACED: Reason = Reason {
        code: Code::Replaced,
        message: "a newer request with the same key replaced this one",
        entries: &["request-limit-errors", "image-worker-errors"],
    };
    pub const OWNER_REQUESTS_ENDED: Reason = Reason {
        code: Code::TooMany,
        message: "too many snapshot requests waiting for this module (16); the oldest was ended",
        entries: &["request-limit-errors"],
    };
    pub const ALL_REQUESTS_REFUSED: Reason = Reason {
        code: Code::TooMany,
        message: "too many snapshot requests waiting in the whole application (64); this one was not taken",
        entries: &["request-limit-errors"],
    };
    /// The capture thread stopped the request, which no path of the host does.
    pub const CANCELLED: Reason = Reason { code: Code::Cancelled, message: "the request was cancelled", entries: &["request-limit-errors"] };

    // Image-worker errors: its callbacks.
    pub const IMAGE_TOO_MANY: Reason = Reason {
        code: Code::TooMany,
        message: "too many image searches waiting for this module (64); the oldest was ended",
        entries: &["image-worker-errors", "snapshot-crop"],
    };

    // A change wait's.
    pub const OWNER_WAITS_ENDED: Reason = Reason { code: Code::TooMany, message: "too many change waits for this module (4); the oldest was ended", entries: &["host-screen-snapshot"] };
    pub const ALL_WAITS_REFUSED: Reason = Reason {
        code: Code::TooMany,
        message: "too many change waits in the whole application (16); this one was not started",
        entries: &["host-screen-snapshot"],
    };
    pub const WATCH_MISS: Reason = Reason { code: Code::Outside, message: "a watch region does not overlap the region", entries: &["host-screen-snapshot"] };
    pub const TOO_FEW_WATCHED: Reason = Reason {
        code: Code::Outside,
        message: "at the window's current size the change wait watches … pixels, fewer than its minPixels …",
        entries: &["host-screen-snapshot"],
    };
    pub const NO_PICTURE_BEFORE_TIMEOUT: Reason = Reason { code: Code::Timeout, message: "no picture was taken before the change wait's timeout", entries: &["host-screen-snapshot"] };

    // Sizes and budgets. A window region's size is answered with the prefix; the budgets are
    // answered for every form (D2), corners without it — hence the leading `…`.
    pub const REGION_TOO_LARGE: Reason = Reason {
        code: Code::TooLarge,
        message: "at the window's current size the region is …x…, … pixels; the limit is 40000000",
        entries: &["host-screen-snapshot", "host-screen-profile", "host-screen-cells", "host-screen-matchcells"],
    };
    pub const CHANGE_REGION_TOO_LARGE: Reason = Reason {
        code: Code::TooLarge,
        message: "at the window's current size the region is …x…, … pixels; the limit is 8294400",
        entries: &["host-screen-snapshot"],
    };
    pub const MODULE_SNAPSHOTS: Reason = Reason {
        code: Code::TooLarge,
        message: "…this module's snapshots would hold … MiB; the limit is 128 MiB per module VM (512 MiB in the whole application). Release the ones you no longer need with s:release()",
        entries: &["host-screen-snapshot", "snapshot-crop", "host-screen-profile"],
    };
    pub const APPLICATION_SNAPSHOTS: Reason = Reason {
        code: Code::TooLarge,
        message: "…the application's snapshots would hold … MiB; the limit is 512 MiB in the whole application (128 MiB per module VM), and other modules' snapshots count toward it. Release the ones this module no longer needs with s:release()",
        entries: &["host-screen-snapshot", "snapshot-crop", "host-screen-profile"],
    };
    pub const TEMPLATE_SIDE: Reason = Reason { code: Code::TooLarge, message: "at the window's current size …x… is too large; no side may exceed 4096", entries: &["host-screen-template"] };
    pub const TEMPLATE_PIXELS: Reason = Reason { code: Code::TooLarge, message: "at the window's current size …x… is … pixels; the limit is 1048576", entries: &["host-screen-template"] };
    pub const MODULE_TEMPLATES: Reason = Reason {
        code: Code::TooLarge,
        message: "…this module's templates would hold … MiB; the limit is 32 MiB per module VM. Build templates once, when the module loads, rather than on every poll.",
        entries: &["host-screen-template"],
    };
    /// profile's empty corners (D5); host.ocr.recognize's too.
    pub const EMPTY_REGION: Reason = Reason { code: Code::Outside, message: "the region is empty (…x…)", entries: &["host-screen-profile", "host-ocr-recognize"] };

    // A call's own work.
    pub const PROFILE_FAILED: Reason = Reason { code: Code::Failed, message: "internal error: the profile could not be made", entries: &["host-screen-profile"] };
    pub const SHORT_CAPTURE: Reason = Reason {
        code: Code::Failed,
        message: "the capture came back shorter than its own size",
        entries: &["host-screen-profile", "host-screen-cells", "host-screen-matchcells", "host-screen-template", "host-screen-save", "host-screen-savemarked"],
    };
    pub const CAPTURE_SIZE: Reason = Reason { code: Code::Failed, message: "the capture came back …x…, not the region's …x…", entries: &["host-screen-cells", "host-screen-matchcells"] };
    /// cells.rs's own checks, which the calls before them make unreachable: never documented.
    pub const CELLS_OUT_OF_IMAGE: Reason = Reason { code: Code::Failed, message: "the region is not inside the captured image", entries: &[] };
    /// The same.
    pub const CELLS_EMPTY: Reason = Reason { code: Code::Failed, message: "the region is empty", entries: &[] };
    /// The same.
    pub const CELLS_TOO_LARGE: Reason = Reason { code: Code::Failed, message: "the region is too large to count", entries: &[] };
    /// The same.
    pub const NO_STATES: Reason = Reason { code: Code::Failed, message: "no states to compare", entries: &[] };
    pub const CELLS_FAILED: Reason = Reason { code: Code::Failed, message: "internal error: the cells could not be computed", entries: &["host-screen-cells", "host-screen-matchcells"] };
    pub const SEARCH_FAILED: Reason = Reason {
        code: Code::Failed,
        message: "internal error: the search could not be made",
        entries: &["host-screen-imagesearch", "host-screen-imagesearchall", "host-screen-imagesearcheach"],
    };
    /// A snapshot's `crop`.
    pub const CROP_FAILED: Reason = Reason { code: Code::Failed, message: "internal error: the crop could not be made", entries: &["snapshot-crop"] };
    pub const TEMPLATE_FAILED: Reason = Reason {
        code: Code::Failed,
        message: "internal error: the template could not be made",
        entries: &["host-screen-template", "host-screen-imagesearch", "host-screen-imagesearchall", "host-screen-imagesearcheach"],
    };
    pub const SAVE_FAILED: Reason = Reason { code: Code::Failed, message: "internal error: the picture could not be saved", entries: &["host-screen-save", "host-screen-savemarked"] };

    // Files (D3): a template outside the module's package, or one that changed after it was first read.
    pub const TEMPLATE_NOT_FOUND: Reason = Reason {
        code: Code::NotFound,
        message: "cannot open template '…': …",
        entries: &["host-screen-template", "host-screen-imagesearch", "host-screen-imagesearchall", "host-screen-imagesearcheach"],
    };
    pub const TEMPLATE_INVALID: Reason = Reason {
        code: Code::Invalid,
        message: "cannot open template '…': …",
        entries: &["host-screen-template", "host-screen-imagesearch", "host-screen-imagesearchall", "host-screen-imagesearcheach"],
    };
    /// A file there that could not be read for another reason: held by another program, or
    /// without the right to read it.
    pub const TEMPLATE_UNREADABLE: Reason = Reason {
        code: Code::Failed,
        message: "cannot open template '…': …",
        entries: &["host-screen-template", "host-screen-imagesearch", "host-screen-imagesearchall", "host-screen-imagesearcheach"],
    };
    pub const SAVE_UNWRITTEN: Reason = Reason { code: Code::Failed, message: "screen.save '…': …", entries: &["host-screen-save"] };
    pub const SAVE_MARKED_UNWRITTEN: Reason = Reason { code: Code::Failed, message: "screen.saveMarked '…': …", entries: &["host-screen-savemarked"] };

    // Never handed to a module.
    /// The answer of a branch no request takes — a picture for a points request, a profile's
    /// reduction for a snapshot: never reaches a module.
    pub const INTERNAL: Reason = Reason { code: Code::Failed, message: "internal error", entries: &[] };
    /// A shot whose module, or handler, went before it was written: nobody is answered.
    pub const NOT_WAITED: Reason = Reason { code: Code::Cancelled, message: "nobody waits for the shot any more", entries: &[] };
}

/// `host.ocr`. A reading's `error`, and resolveLanguage's answer.
pub mod ocr {
    use super::{Code, Reason};

    /// Windows' crop of a region out of the bounding box it was captured in: never given, since the
    /// box is made from the same rectangles (`plan::offset_in` admits only a region inside it, and a
    /// capture of another size than the box is not cropped) — kept for the crop's own check
    /// (windows.rs, `capture_for_read`).
    pub const CROP_OUTSIDE: Reason = Reason { code: Code::Failed, message: "region outside the captured area", entries: &[] };
    pub const MAC_CAPTURE_FAILED: Reason = Reason {
        code: Code::Failed,
        message: "screen capture failed — if every read fails, grant this application Screen Recording and restart it",
        entries: &["host-ocr-recognize"],
    };
    pub const NO_DISPLAY_PICTURE: Reason = Reason {
        code: Code::Failed,
        message: "this snapshot kept no picture at the display's resolution to read text from",
        entries: &["host-ocr-recognize"],
    };
    pub const NOT_CUT: Reason = Reason { code: Code::Failed, message: "could not cut this region out of the shared capture", entries: &["host-ocr-recognize"] };
    pub const RECOGNITION_FAILED: Reason = Reason { code: Code::Failed, message: "OCR failed: …", entries: &["host-ocr-recognize"] };
    pub const RECOGNISER_PANICKED: Reason = Reason {
        code: Code::Failed,
        message: "the text recogniser failed inside the application; the log has the details",
        entries: &["host-ocr-recognize"],
    };
    pub const NO_RECOGNISER: Reason = Reason { code: Code::NotSupported, message: "no text recogniser on this platform", entries: &["host-ocr-recognize"] };
    pub const NO_ANSWER: Reason = Reason { code: Code::Failed, message: "the recogniser gave no answer for this region", entries: &["host-ocr-recognize"] };
    /// A region not started because the application closed: no callback runs after that, so it
    /// never reaches a module.
    pub const CLOSING: Reason = Reason { code: Code::Cancelled, message: "the application is closing", entries: &[] };
    pub const TOO_MANY: Reason = Reason {
        code: Code::TooMany,
        message: "too many reads waiting — put regions that belong together in one call (up to 64)",
        entries: &["host-ocr-recognize"],
    };
    pub const ALL_TOO_MANY: Reason = Reason {
        code: Code::TooMany,
        message: "too many reads waiting in the whole application (64); this one was not taken",
        entries: &["host-ocr-recognize"],
    };
    pub const HANG: Reason = Reason { code: Code::Timeout, message: "the text recogniser has not answered a region for … s", entries: &["host-ocr-recognize"] };
    pub const CALL_TOO_LARGE: Reason = Reason {
        code: Code::TooLarge,
        message: "not read: at the window's current size this region is …x…, which would take the call past 40000000 pixels",
        entries: &["host-ocr-recognize"],
    };
    pub const NO_LANGUAGE: Reason = Reason {
        code: Code::Unavailable,
        message: "language: no text recognition language is available here. …",
        entries: &["host-ocr-recognize", "host-ocr-resolvelanguage"],
    };
    pub const LANGUAGE_UNAVAILABLE: Reason = Reason {
        code: Code::Unavailable,
        message: "language: … is not available here (available: …). …",
        entries: &["host-ocr-recognize", "host-ocr-resolvelanguage"],
    };
    pub const LANGUAGES_UNAVAILABLE: Reason = Reason {
        code: Code::Unavailable,
        message: "language: none of … is available here (available: …). …",
        entries: &["host-ocr-recognize", "host-ocr-resolvelanguage"],
    };
    /// macOS: pixels CoreGraphics could not read back (D33).
    pub const NOT_READ_BACK: Reason = Reason { code: Code::Failed, message: "the picture could not be read back", entries: &["host-ocr-recognize"] };
    /// macOS: a small region's content crop CoreGraphics could not draw for Vision.
    pub const NOT_DRAWN: Reason = Reason { code: Code::Failed, message: "the picture could not be drawn for the recogniser", entries: &["host-ocr-recognize"] };
    /// macOS: a request Vision refused (D33).
    pub const VISION_REFUSED: Reason = Reason { code: Code::Refused, message: "the text recogniser refused the request: …", entries: &["host-ocr-recognize"] };
    pub const LANGUAGES_NOT_KNOWN: Reason = Reason { code: Code::NotReady, message: "the recognition languages are not known yet", entries: &["host-ocr-resolvelanguage"] };
}

/// `host.speech.use`.
pub mod speech {
    use super::{Code, Reason};

    pub const NO_ENGINE: Reason = Reason { code: Code::NotFound, message: "no speech engine '…' in this build", entries: &["host-speech-use"] };
    pub const NOT_KNOWN_YET: Reason = Reason { code: Code::NotReady, message: "the speech engines are not known yet", entries: &["host-speech-use"] };
    pub const NOT_RUNNING: Reason = Reason { code: Code::Unavailable, message: "'…' is not running", entries: &["host-speech-use"] };
}

/// `host.sound.play`. A file outside the module's package is answered (D3); one inside it raises.
pub mod sound {
    use super::{Code, Reason};

    pub const NO_DEVICE: Reason = Reason { code: Code::Unavailable, message: "no audio output device: …", entries: &["host-sound-play"] };
    pub const NOT_FOUND: Reason = Reason { code: Code::NotFound, message: "cannot play '…': …", entries: &["host-sound-play"] };
    pub const INVALID: Reason = Reason { code: Code::Invalid, message: "cannot play '…': …", entries: &["host-sound-play"] };
    pub const FAILED: Reason = Reason { code: Code::Failed, message: "cannot play '…': …", entries: &["host-sound-play"] };
}

/// The gamepad calls that answer the watcher's failures: list, state and on.
pub const PADS: &[&str] = &["host-gamepad-list", "host-gamepad-state", "host-gamepad-on"];

/// `host.gamepad`: the watcher, at its start and after it.
pub mod gamepad {
    use super::{Code, Reason, PADS};

    pub const NOT_SUPPORTED: Reason = Reason { code: Code::NotSupported, message: "game controllers are not supported on …", entries: PADS };
    pub const THREAD_NOT_STARTED: Reason = Reason { code: Code::Failed, message: "could not start the gamepad thread: …", entries: PADS };
    pub const THREAD_NOT_READY: Reason = Reason { code: Code::Failed, message: "the gamepad thread ended before it was ready", entries: PADS };
    pub const NO_WINDOW: Reason = Reason { code: Code::Failed, message: "could not create the gamepad thread's window (error …)", entries: PADS };
    pub const START_PANICKED: Reason = Reason { code: Code::Failed, message: "the source panicked while starting: …", entries: PADS };
    /// After the 250 ms wait: the thread's window not made (D36).
    pub const FAILED_AFTER_START: Reason = Reason { code: Code::Failed, message: "gamepad watcher failed after starting: …", entries: PADS };
    /// After the 250 ms wait: a panic (D36).
    pub const STOPPED_AFTER_PANIC: Reason = Reason { code: Code::Failed, message: "the gamepad thread stopped after a panic: …", entries: PADS };
    /// No XInput DLL: the watcher runs, and no Xbox-type pad is ever listed (D36).
    pub const NO_XINPUT: Reason = Reason { code: Code::Unavailable, message: "XInput is unavailable, so no Xbox-type pad will be seen: …", entries: PADS };
}

/// Every Reason, by namespace. A test holds this list to the constants above, so a Reason left out
/// of it — which no docs check would see — fails.
pub static REASONS: &[&Reason] = &[
    &common::NOT_YET,
    &common::DECLINED,
    &common::WINDOW_GONE,
    &gui::HEADLESS,
    &gui::RUNTIME_MISSING,
    &gui::NOT_READY,
    &gui::BUILD_FAILED,
    &gui::NO_ROOM,
    &gui::OVER,
    &gui::GONE,
    &gui::NOT_BUILT,
    &gui::HIDDEN,
    &gui::NOT_HANDED,
    &gui::FAILED,
    &gui::PAGE_MESSAGE_TOO_LARGE,
    &window::NO_SUCH_WINDOW,
    &window::HIDDEN,
    &element::TIMEOUT,
    &element::DENIED,
    &element::PROGRAM_GONE,
    &element::NOT_ASKED_TIMEOUT,
    &element::NOT_ASKED_DENIED,
    &element::NOT_ASKED_GONE,
    &element::REPLACED,
    &element::NESTED,
    &element::THREAD_NOT_STARTED,
    &element::OWN_WINDOW,
    &element::NO_SIGNAL,
    &element::LOST,
    &element::THREAD_FAILED,
    &element::WRONG_THREAD,
    &element::NO_INTERFACE,
    &element::TOO_MANY_NODES,
    &element::TOO_SLOW,
    &element::DUMP_CUT,
    &element::NOT_ANSWERING,
    &element::QUARANTINED,
    &element::SEARCH_CUT,
    &element::FOCUS_CUT,
    &element::HOT_TOO_SLOW,
    &element::MAC_DUMP_CUT,
    &element::MOVES_FOCUS,
    &element::NO_ELEMENT,
    &element::CANNOT_PRESS,
    &element::NOT_ENABLED,
    &element::ELEMENT_GONE,
    &element::NOT_TAKEN,
    &input::OUTSIDE,
    &input::COVERED,
    &input::FRAME,
    &input::HIGHER,
    &input::MESSAGE_NOT_TAKEN,
    &input::STYLE_NOT_TAKEN,
    &input::MAIN_WINDOW,
    &input::ENDING,
    &input::NO_ANSWER_WITHIN,
    &input::NO_ANSWER_ERROR,
    &input::NOT_KEPT,
    &input::RESTORE_LIST,
    &input::STYLE_THREAD_GONE,
    &input::STYLE_THREAD_NOT_STARTED,
    &input::STYLE_THREAD_FAILED,
    &input::NO_POST_ACCESS,
    &input::NOT_PAIRED,
    &input::NOT_ON_SCREEN,
    &input::NOT_ANSWERING,
    &input::NO_WINDOW_LOCATION,
    &input::NULL_MOUSE_EVENT,
    &input::OFF_MAIN_THREAD,
    &input::POST_NO_KEYCODE,
    &input::POST_NO_PROCESS,
    &input::NULL_KEY_EVENT,
    &input::SEND_NO_KEYCODE,
    &input::NOT_TAKEN,
    &input::LEFT_DOWN,
    &input::NOT_PLACED,
    &input::TEXT_NOT_TAKEN,
    &input::NULL_MOUSE_SENT,
    &input::NULL_SCROLL_EVENT,
    &input::TEXT_CUT,
    &keys::HOOK_THREAD,
    &keys::HOOK_ENDED,
    &keys::HOOK_REFUSED,
    &keys::TAP_REFUSED,
    &keys::TAP_SOURCE,
    &keys::TAP_NO_RUN_LOOP,
    &hotkey::KEPT_BY_SYSTEM,
    &hotkey::NO_KEYCODE,
    &hotkey::NO_VK,
    &hotkey::MODULE_HOLDS,
    &hotkey::REGISTER_REFUSED,
    &hotkey::MAC_REFUSED,
    &hotkey::PARKED,
    &hotkey::STUB,
    &hotkey::NO_HANDLER,
    &screen::CAPTURE_FAILED,
    &screen::CAPTURE_TOO_LARGE,
    &screen::CAPTURE_PANICKED,
    &screen::ROUND_FAILED,
    &screen::DUPLICATION_SWITCHED_OFF,
    &screen::DUPLICATION_DISABLED,
    &screen::DUPLICATION_CRASHED,
    &screen::DUPLICATION_NO_THREAD,
    &screen::DUPLICATION_CLOSING,
    &screen::DUPLICATION_OVERDUE,
    &screen::DUPLICATION_UNSUPPORTED,
    &screen::DUPLICATION_TOO_MANY_RECORDERS,
    &screen::DUPLICATION_SECURE_DESKTOP,
    &screen::DUPLICATION_ACCESS_LOST,
    &screen::DUPLICATION_DEVICE_REMOVED,
    &screen::DUPLICATION_ROTATED,
    &screen::DUPLICATION_TOPOLOGY,
    &screen::DUPLICATION_NO_FRAME_YET,
    &screen::DUPLICATION_NO_OUTPUT,
    &screen::DUPLICATION_FORMAT,
    &screen::DUPLICATION_FAILED,
    &screen::CLIENT_EMPTY,
    &screen::OUT_OF_RANGE,
    &screen::NOT_INSIDE,
    &screen::NO_OVERLAP,
    &screen::POINT_NOT_INSIDE,
    &screen::REPLACED,
    &screen::OWNER_REQUESTS_ENDED,
    &screen::ALL_REQUESTS_REFUSED,
    &screen::CANCELLED,
    &screen::IMAGE_TOO_MANY,
    &screen::OWNER_WAITS_ENDED,
    &screen::ALL_WAITS_REFUSED,
    &screen::WATCH_MISS,
    &screen::TOO_FEW_WATCHED,
    &screen::NO_PICTURE_BEFORE_TIMEOUT,
    &screen::REGION_TOO_LARGE,
    &screen::CHANGE_REGION_TOO_LARGE,
    &screen::MODULE_SNAPSHOTS,
    &screen::APPLICATION_SNAPSHOTS,
    &screen::TEMPLATE_SIDE,
    &screen::TEMPLATE_PIXELS,
    &screen::MODULE_TEMPLATES,
    &screen::EMPTY_REGION,
    &screen::PROFILE_FAILED,
    &screen::SHORT_CAPTURE,
    &screen::CAPTURE_SIZE,
    &screen::CELLS_OUT_OF_IMAGE,
    &screen::CELLS_EMPTY,
    &screen::CELLS_TOO_LARGE,
    &screen::NO_STATES,
    &screen::CELLS_FAILED,
    &screen::SEARCH_FAILED,
    &screen::CROP_FAILED,
    &screen::TEMPLATE_FAILED,
    &screen::SAVE_FAILED,
    &screen::TEMPLATE_NOT_FOUND,
    &screen::TEMPLATE_INVALID,
    &screen::TEMPLATE_UNREADABLE,
    &screen::SAVE_UNWRITTEN,
    &screen::SAVE_MARKED_UNWRITTEN,
    &screen::INTERNAL,
    &screen::NOT_WAITED,
    &ocr::CROP_OUTSIDE,
    &ocr::MAC_CAPTURE_FAILED,
    &ocr::NO_DISPLAY_PICTURE,
    &ocr::NOT_CUT,
    &ocr::RECOGNITION_FAILED,
    &ocr::RECOGNISER_PANICKED,
    &ocr::NO_RECOGNISER,
    &ocr::NO_ANSWER,
    &ocr::CLOSING,
    &ocr::TOO_MANY,
    &ocr::ALL_TOO_MANY,
    &ocr::HANG,
    &ocr::CALL_TOO_LARGE,
    &ocr::NO_LANGUAGE,
    &ocr::LANGUAGE_UNAVAILABLE,
    &ocr::LANGUAGES_UNAVAILABLE,
    &ocr::NOT_READ_BACK,
    &ocr::NOT_DRAWN,
    &ocr::VISION_REFUSED,
    &ocr::LANGUAGES_NOT_KNOWN,
    &speech::NO_ENGINE,
    &speech::NOT_KNOWN_YET,
    &speech::NOT_RUNNING,
    &sound::NO_DEVICE,
    &sound::NOT_FOUND,
    &sound::INVALID,
    &sound::FAILED,
    &gamepad::NOT_SUPPORTED,
    &gamepad::THREAD_NOT_STARTED,
    &gamepad::THREAD_NOT_READY,
    &gamepad::NO_WINDOW,
    &gamepad::START_PANICKED,
    &gamepad::FAILED_AFTER_START,
    &gamepad::STOPPED_AFTER_PANIC,
    &gamepad::NO_XINPUT,
];

/// Each group a Reason may name instead of an entry, and the entries that give every reason of
/// it. The docs write a group's reasons once, at the group's anchor, and an entry's Errors table
/// links there ("any of the capture errors"): a group row stands for all of them, so every entry of
/// a group gives every reason of it — a reason that only some give names those entries instead.
/// The members are a list of strings or the name of a `const` list of this file. Read by the tests
/// and, as text, by tools/api-index.py; the host itself never needs it.
#[cfg_attr(not(test), allow(dead_code))]
pub static GROUPS: &[(&str, &[&str])] = &[
    ("question-errors", QUESTIONS),
    (
        "capture-errors",
        &[
            "host-screen-snapshot",
            "host-screen-profile",
            "host-screen-cells",
            "host-screen-matchcells",
            "host-screen-template",
            "host-screen-imagesearch",
            "host-screen-imagesearchall",
            "host-screen-imagesearcheach",
            "host-screen-save",
            "host-screen-savemarked",
        ],
    ),
    (
        "duplication-errors",
        &[
            "host-screen-pixel",
            "host-screen-pixels",
            "host-screen-snapshot",
            "host-screen-profile",
            "host-screen-cells",
            "host-screen-matchcells",
            "host-screen-template",
            "host-screen-imagesearch",
            "host-screen-imagesearchall",
            "host-screen-imagesearcheach",
            "host-screen-save",
            "host-screen-savemarked",
            "host-ocr-recognize",
        ],
    ),
    (
        "window-form-errors",
        &[
            "host-screen-pixel",
            "host-screen-pixels",
            "host-screen-snapshot",
            "snapshot-crop",
            "host-screen-profile",
            "host-screen-cells",
            "host-screen-matchcells",
            "host-screen-template",
            "host-screen-imagesearch",
            "host-screen-imagesearchall",
            "host-screen-imagesearcheach",
            "host-screen-save",
            "host-screen-savemarked",
            "host-ocr-recognize",
        ],
    ),
    (
        "snapshot-source-errors",
        &[
            "host-screen-profile",
            "host-screen-imagesearch",
            "host-screen-imagesearchall",
            "host-screen-imagesearcheach",
            "host-screen-save",
            "host-screen-savemarked",
            "host-ocr-recognize",
        ],
    ),
    ("request-limit-errors", &["host-screen-pixel", "host-screen-pixels", "host-screen-snapshot", "host-screen-profile"]),
    (
        "image-worker-errors",
        &[
            "host-screen-cells",
            "host-screen-matchcells",
            "host-screen-template",
            "host-screen-imagesearch",
            "host-screen-imagesearchall",
            "host-screen-imagesearcheach",
            "host-screen-save",
            "host-screen-savemarked",
        ],
    ),
];

// ── The error the host carries ───────────────────────────────────────────────────────────────

/// A reason, as the host carries it until it reaches a module: `Send`, so an answer can cross
/// threads in one where it carries a `String` today, and turned into an error value only in the
/// VM that receives it (errors_lua.rs, `to_lua`).
///
/// Made only from a [`Reason`] of [`REASONS`]: [`of`](HostError::of) and [`with`](HostError::with)
/// are the only constructors — its fields are private, and a test fails on a literal of one
/// anywhere but here — and each asserts in a debug build that its Reason is in the table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostError {
    reason: &'static Reason,
    message: Cow<'static, str>,
    detail: Option<String>,
}

impl HostError {
    /// `reason`, with its message as it is.
    pub fn of(reason: &'static Reason) -> Self {
        listed(reason);
        HostError { reason, message: Cow::Borrowed(reason.message), detail: None }
    }

    /// `reason`, with its formatted message. In a debug build, a message that is not the
    /// reason's pattern — its pieces between `…`, in order — panics, so every test run checks
    /// every formatted message it makes.
    pub fn with(reason: &'static Reason, message: String) -> Self {
        listed(reason);
        debug_assert!(fits(reason.message, &message), "the message {message:?} is not the reason's {:?}", reason.message);
        HostError { reason, message: Cow::Owned(message), detail: None }
    }

    /// The same, with `detail` for the log: what the module would otherwise get less of than the
    /// log has. Never compared.
    pub fn with_detail(self, detail: String) -> Self {
        HostError { detail: Some(detail), ..self }
    }

    pub fn reason(&self) -> &'static Reason {
        self.reason
    }

    pub fn code(&self) -> Code {
        self.reason.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

/// In a debug build, a Reason that is not in [`REASONS`] panics: no docs check would see it.
/// Compared by value, since a constant has no one address.
fn listed(reason: &'static Reason) {
    debug_assert!(REASONS.contains(&reason), "{:?} is no Reason of REASONS", reason.message);
}

/// The message, so a log line that formats a reason reads as it did.
impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Whether `message` is `pattern` with each `…` filled in: the pieces between them appear in it in
/// order, the first at its start and the last at its end. A pattern without `…` fits only itself.
pub fn fits(pattern: &str, message: &str) -> bool {
    let pieces: Vec<&str> = pattern.split('…').collect();
    let [first, middle @ .., last] = pieces.as_slice() else {
        return pattern == message;
    };
    let Some(mut rest) = message.strip_prefix(first) else { return false };
    for piece in middle {
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use std::path::{Path, PathBuf};

    /// The repository's root, from this crate's: the same in the host and in macos-check, which
    /// borrows this file.
    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
    }

    #[test]
    fn every_code_is_snake_case_and_its_constant_its_upper_case() {
        let mut seen = HashSet::new();
        for code in Code::ALL {
            let s = code.as_str();
            assert!(s.starts_with(|c: char| c.is_ascii_lowercase()), "{s}");
            assert!(s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'), "{s}");
            assert_eq!(code.constant(), s.to_ascii_uppercase());
            assert!(seen.insert(s), "{s} twice");
        }
        assert_eq!(seen.len(), 19);
    }

    #[test]
    fn a_pattern_fits_its_formatted_messages_only() {
        assert!(fits("screen capture failed", "screen capture failed"));
        assert!(!fits("screen capture failed", "screen capture failed: more"));
        let p = "the window's client area is empty (…x…)";
        assert!(fits(p, "the window's client area is empty (0x0)"));
        assert!(fits(p, "the window's client area is empty (1200x30)"));
        assert!(!fits(p, "the window's client area is empty 0x0"));
        assert!(!fits(p, "a window's client area is empty (0x0)"));
        let lead = "…this module's snapshots would hold … MiB; the limit";
        assert!(fits(lead, "this module's snapshots would hold 1.0 MiB; the limit"), "an empty part");
        assert!(fits(lead, "at the window's current size this module's snapshots would hold 1.0 MiB; the limit"));
        assert!(fits("… to an earlier question, so this one was not asked", "the program is gone to an earlier question, so this one was not asked"));
        assert!(fits("OCR failed: …", "OCR failed: "), "a trailing part may be empty");
        assert!(!fits("OCR failed: …", "OCR failed"));
    }

    #[test]
    fn a_host_error_is_its_reasons_and_says_its_message() {
        let e = HostError::of(&input::COVERED);
        assert_eq!((e.code(), e.message(), e.detail()), (Code::Covered, "something else is drawn over the point", None));
        assert_eq!(e.to_string(), "something else is drawn over the point");
        assert_eq!(e.reason(), &input::COVERED, "by value: a constant has no one address");
        let e = HostError::with(&screen::CLIENT_EMPTY, "the window's client area is empty (0x12)".into()).with_detail("seen at the call".into());
        assert_eq!((e.code(), e.message(), e.detail()), (Code::Hidden, "the window's client area is empty (0x12)", Some("seen at the call")));
        fn sendable<T: Send + Sync + Clone + 'static>(_: &T) {}
        sendable(&e);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "is not the reason's")]
    fn a_formatted_message_that_is_not_its_reasons_panics_in_a_debug_build() {
        let _ = HostError::with(&screen::CLIENT_EMPTY, "the client area is empty".into());
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "is no Reason of REASONS")]
    fn a_reason_that_is_not_in_the_list_panics_in_a_debug_build() {
        static UNLISTED: Reason = Reason { code: Code::Busy, message: "one at a time", entries: &[] };
        let _ = HostError::of(&UNLISTED);
    }

    /// How many struct literals of `ty` `text` holds outside its comments: `ty`, `{`, then one of
    /// `fields` — `Reason { code: …` — but not `-> Reason {` or `impl … for Reason {`, which open
    /// a block.
    fn literals(text: &str, ty: &str, fields: &[&str]) -> usize {
        let code: String = text.lines().filter(|l| !l.trim_start().starts_with("//")).flat_map(|l| [l, "\n"]).collect();
        let ident = |c: char| c.is_alphanumeric() || c == '_';
        code.match_indices(ty)
            .filter(|&(at, _)| !code[..at].ends_with(ident))
            .filter(|&(at, _)| {
                let Some(rest) = code[at + ty.len()..].trim_start().strip_prefix('{') else { return false };
                let rest = rest.trim_start();
                fields.iter().any(|f| rest.strip_prefix(f).is_some_and(|after| !after.starts_with(ident)))
            })
            .count()
    }

    const REASON_FIELDS: &[&str] = &["code", "message", "entries"];

    /// Every Reason written in this file is in [`REASONS`], once — `const` or `static`, whatever
    /// its visibility — and no two have the same code and message: the docs check matches a row
    /// by both.
    #[test]
    fn the_list_holds_every_reason_once() {
        let src = std::fs::read_to_string(root().join("crates/host/src/errors.rs")).unwrap();
        let src = &src[..src.find("\nmod tests {").unwrap()];
        assert_eq!(literals(src, "Reason", REASON_FIELDS), REASONS.len(), "a Reason is missing from REASONS, or listed twice");
        let mut pairs = HashSet::new();
        for (i, r) in REASONS.iter().enumerate() {
            assert!(!REASONS[..i].contains(r), "{:?} listed twice", r.message);
            assert!(pairs.insert((r.code, r.message)), "{:?} {:?} twice", r.code, r.message);
        }
    }

    /// No Reason and no HostError is written anywhere but here, so none bypasses the table: a
    /// Reason written elsewhere is one no test counts, and HostError's fields are private, so a
    /// literal of one compiles only here anyway.
    #[test]
    fn no_reason_or_host_error_is_written_outside_errors_rs() {
        fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(&p, out);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    out.push(p);
                }
            }
        }
        let sample = "fn rogue() -> Reason {\n    Reason { code: Code::Busy, message: \"m\", entries: &[] }\n}\n// Reason { code: … }";
        assert_eq!(literals(sample, "Reason", REASON_FIELDS), 1, "a literal, not a return type or a comment");
        let mut files = Vec::new();
        walk(&root().join("crates/host/src"), &mut files);
        assert!(files.len() > 50);
        for f in files {
            if f.file_name().is_some_and(|n| n == "errors.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&f).unwrap();
            assert_eq!(literals(&text, "Reason", REASON_FIELDS), 0, "{} writes a Reason: it belongs in errors.rs", f.display());
            assert_eq!(literals(&text, "HostError", &["reason", "message", "detail"]), 0, "{} builds a HostError by hand", f.display());
        }
    }

    /// A group gives each of its reasons to every call it names, so a reason only some of them give
    /// names those calls instead: the `screen-capture` thread's own two its calls only, desktop
    /// duplication's 40 million pixels only the calls that do not hold their region to fewer
    /// first, and a call given a snapshot the one reason
    /// of its rule. And the reasons no path gives name nothing: a question not asked behind one
    /// refused or gone (only a timeout leaves those behind it unasked), and Windows' crop of a read
    /// out of its bounding box (windows.rs, `ocr_crop_tests`).
    #[test]
    fn a_group_gives_its_reasons_to_every_call_it_names() {
        let members = |e: &'static str| GROUPS.iter().find(|(g, _)| *g == e).map_or_else(|| vec![e], |(_, m)| m.to_vec());
        let of = |r: &Reason| -> HashSet<&'static str> { r.entries.iter().flat_map(|e| members(e)).collect() };
        let set = |s: &[&'static str]| -> HashSet<&'static str> { s.iter().copied().collect() };
        let thread = set(&["host-screen-snapshot", "host-screen-profile", "host-ocr-recognize", "host-screen-pixel", "host-screen-pixels"]);
        assert_eq!(of(&screen::CAPTURE_PANICKED), thread);
        assert_eq!(of(&screen::ROUND_FAILED), set(&["host-screen-snapshot", "host-screen-profile"]));
        assert_eq!(of(&screen::CAPTURE_TOO_LARGE), set(&["host-screen-imagesearch", "host-screen-imagesearchall", "host-screen-imagesearcheach", "host-screen-save", "host-screen-savemarked"]));
        assert!(of(&screen::CAPTURE_FAILED).contains("host-ocr-recognize"));
        let inside = of(&screen::NOT_INSIDE);
        assert_eq!(inside, set(&["host-screen-cells", "host-screen-matchcells", "host-screen-template", "snapshot-crop"]));
        assert!(inside.is_disjoint(&of(&screen::NO_OVERLAP)), "a call gives the one its rule gives");
        for never in [&element::NOT_ASKED_DENIED, &element::NOT_ASKED_GONE, &ocr::CROP_OUTSIDE, &element::WRONG_THREAD] {
            assert!(never.entries.is_empty(), "{:?}", never.message);
        }
        // The questions: every one gives the Windows reasons through the group, and a Mac's to all but
        // the press, which is not there yet. focusWithin asks a Windows program nothing, so only the
        // host's own reach it there: what the program answers, or the automation object, never.
        for program in [&element::TIMEOUT, &element::DENIED, &element::PROGRAM_GONE, &element::NO_INTERFACE] {
            assert_eq!(of(program), set(QUESTIONS), "{:?}", program.message);
        }
        assert!(!QUESTIONS.contains(&"host-element-focuswithin"));
        let mut asked = set(QUESTIONS);
        asked.insert("host-element-focuswithin");
        for host in [
            &element::NOT_ASKED_TIMEOUT,
            &element::REPLACED,
            &element::NESTED,
            &element::THREAD_NOT_STARTED,
            &element::OWN_WINDOW,
            &element::NO_SIGNAL,
            &element::LOST,
            &element::THREAD_FAILED,
        ] {
            assert_eq!(of(host), asked, "{:?}", host.message);
        }
        let mac = of(&element::NOT_ANSWERING);
        let mut all_but_press = asked.clone();
        all_but_press.remove("host-element-press");
        assert_eq!(mac, all_but_press);
        assert!(mac == of(&element::QUARANTINED));
    }

    /// An entry is a level-2 heading of an API page, as tools/api-index.py reads one: a Reason
    /// naming a level-3 anchor (`host-window-focus-errors`) or a page's box would be in no
    /// entry's table, and no check would compare it with one.
    #[test]
    fn every_reason_has_a_code_a_message_and_entries_that_exist() {
        let mut anchors = HashSet::new();
        for page in std::fs::read_dir(root().join("docs/api")).unwrap() {
            let text = std::fs::read_to_string(page.unwrap().path()).unwrap();
            for head in text.lines().filter_map(|l| l.strip_prefix("## ")) {
                if let Some((_, id)) = head.trim_end().rsplit_once("{#") {
                    anchors.insert(id.trim_end_matches('}').to_string());
                }
            }
        }
        anchors.remove("declare");
        let groups: HashMap<&str, &[&str]> = GROUPS.iter().copied().collect();
        assert_eq!(groups.len(), GROUPS.len(), "a group is listed twice");
        let mut used = HashSet::new();
        for r in REASONS {
            assert!(Code::ALL.contains(&r.code));
            assert!(!r.message.trim().is_empty() && r.message.trim() == r.message, "{:?}", r.message);
            assert!(!r.message.contains("……"), "{:?}", r.message);
            for e in r.entries {
                if groups.contains_key(e) {
                    used.insert(*e);
                } else {
                    assert!(anchors.contains(*e), "{:?} names {e}, which is neither an entry nor a group", r.message);
                }
            }
        }
        for (name, entries) in GROUPS {
            assert!(used.contains(name), "no reason is in the group {name}");
            for e in *entries {
                assert!(anchors.contains(*e), "the group {name} names {e}, which is no entry");
            }
        }
    }
}
