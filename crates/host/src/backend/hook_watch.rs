//! Keeping the low-level keyboard hook alive for days — the pure half.
//!
//! **Why.** Windows removes a `WH_KEYBOARD_LL` hook without telling anybody: "If the hook
//! procedure times out, the system passes the message to the next hook. However, on Windows 7
//! and later, the hook is silently removed without being called. There is no way for the
//! application to know whether the hook is removed." (Microsoft's `LowLevelKeyboardProc`
//! page.) Nothing re-installed the hook, and nothing could have noticed. On 2026-09-27 the
//! application was started at about three in the morning, an overlay activated normally ten
//! hours later and registered its captured keys, and not one Tab reached it — while every
//! `RegisterHotKey` hotkey still worked. A restart fixed it. A removed hook fits that log, but
//! it does not prove it: no hotkey arrived through `RegisterHotKey` alone, which a removed hook
//! would have shown if one was pressed, and a screen reader's modifier the hook had recorded as
//! held — its key-up gone to a screen reader's own dialog, where the hook is not called — fits
//! every line as well. The backend now forgets that record when the keyboard goes where the
//! hook cannot follow, and says in the log why a captured key was let through.
//!
//! **Two ways back.** The hook is installed again
//!
//! - after the events around which hooks are reported lost (by other programs that re-install
//!   theirs; Microsoft documents none): the machine resumed from sleep or hibernation, the
//!   session was unlocked, or it was connected to a console or a remote client again
//!   ([`on_session_change`], [`on_power`]). Around those the hook's thread is the likeliest to
//!   answer late — its pages faulted back in, the machine busy — which is what makes a hook
//!   time out. Unconditionally: whether the old hook was still there is said in the log, which
//!   is how a live test answers the question; and
//! - when a witness that no timeout can remove says so. Raw Input (`RIDEV_INPUTSINK` on a
//!   message-only window of its own thread) is posted to its window, not called and waited
//!   for, so there is nothing for Windows to time out. It sees the physical key-downs the system
//!   delivered, and [`Witness`] compares them with when the hook was last called. Several in a
//!   row that the hook was not called for, in front of a window the hook can see, means the
//!   hook is gone. Counted in key-downs, never decided by a clock.
//!
//! **What the comparison has to leave alone.** Every case in which the hook legitimately sees
//! nothing while keys are pressed:
//!
//! - *A window of a higher integrity level in front* — an elevated program, and the dialogs of
//!   a UIAccess program such as a screen reader, which runs one step above an ordinary one. User
//!   Interface Privilege Isolation keeps input to those from the low-level hooks of an ordinary
//!   process, and — Microsoft names both together, as what a UIAccess process gains — from its
//!   raw input as well. So the witness is normally deaf there too; [`hook_sees`] makes sure a
//!   key it does hear there is not counted.
//! - *Another desktop* — the lock screen, the secure desktop of a UAC prompt or Ctrl+Alt+Del. A
//!   hook sees the desktop it was installed on, and so does raw input; the backend asks which
//!   desktop has the keyboard before it counts a key.
//! - *A window of this process in front* — the module manager, a module's dialog. Windows does not
//!   call a process's low-level keyboard hook for keys going to that process's own windows, and
//!   Microsoft documents nothing of it: on 2026-10-05 Tab pressed in the module manager installed
//!   the hook again, the lines saying 3, then 6, 12 and 24 key-downs in a row had reached raw input
//!   and not the hook, with no call in between, and the HTML probe page had shown the same before.
//!   Raw input says with each key whether this process had the foreground when it was pressed
//!   (`RIM_INPUT`), so [`counts`] leaves such a key out by the key's own report — not by asking
//!   which window is in front once the witness gets to it, by when the user may have switched.
//! - *Injected input* — the hook sees it (`LLKHF_INJECTED`), raw input reports it with no
//!   device handle. [`physical_down`] counts only keys with a device, so a program that types
//!   keys can neither make the hook look dead nor alive.
//! - *A hook ahead of ours that swallows a key* — a screen reader's hook installed after ours,
//!   in browse mode, where it takes the arrows. Microsoft does not document whether raw input
//!   is generated before or after the low-level hooks. One developer's test found that a key a
//!   low-level hook swallows never reaches raw input, and then nothing is counted. If it does
//!   reach it, the hook's calls for every key that hook lets through still reset the count, and
//!   at worst one run of five swallowed keys installs our hook again, once — which puts it ahead
//!   of the screen reader's, the order the application has anyway when it starts after the
//!   screen reader, after which it sees every key and nothing is missed again. The same holds
//!   for FilterKeys' slow keys, if raw input sees a press they drop. TODO.md's live checks
//!   settle which it is.
//! - *A hook running late* — the hook is judged by whether it was called at all since the
//!   PREVIOUS key-down the witness saw, with a second of slack, not by the key in hand. A late
//!   call counts as soon as it runs; only several key-downs in a row with no call at all, each
//!   meaning the system gave up waiting for the hook, make a re-install.
//! - *The witness running late* — a key-down is judged against its own timestamp, not against
//!   the moment the witness got round to it, so a busy witness only ever sees the hook as more
//!   alive than it was.
//!
//! **What a re-install carries over, and what it forgets** — see the backend's `reinstall`:
//! the captured set, the granted hotkeys, the scope and the menu flags are the application's
//! and stay as they are. What the hook remembered of keys going by (a screen reader's modifier
//! held, a pending modifier tap, the modifiers a late call is judged by) is forgotten only when
//! Windows had removed the old hook, because only then did it miss their key-ups; a hook that
//! was still there saw them. The keys owed a key-up for a screen reader are kept either way: a
//! stray key-up is harmless, a swallowed owed one leaves a key held. Independently of any
//! re-install, the watch has the backend forget the first two whenever the keyboard goes where
//! the hook cannot follow — a window of a higher integrity level or one of this process's own in
//! front, a locked or disconnected session, a suspend ([`front_unseen`]).
//!
//! **The chain.** Windows calls low-level keyboard hooks newest first, so a hook installed again
//! is first again: ahead of every hook installed since the application started — a screen
//! reader started or restarted since then included. That is the order the application has
//! whenever it is started after the screen reader, which is the usual order, and the line in the
//! log says so every time.
//!
//! Everything here is integers and decisions: no OS call, so the tests run wherever Windows
//! builds.

/// How many key-downs in a row the hook must miss before it is installed again, while the last
/// re-install is confirmed (or there was none). Enough that a stray key the hook legitimately
/// missed — the Del of Ctrl+Alt+Del, a key pressed as a desktop switches — never adds up to one;
/// few enough that a user pressing Tab in an overlay gets it back after a handful of presses.
/// Three, decided by the maintainer (2026-09-27): with the desktop and integrity guards a
/// false re-install is harmless, and every miss is a Tab or a Return the user loses.
pub(crate) const MISSES_TO_REHOOK: u32 = 3;

/// The most key-downs a streak has to reach, however often a re-install went unconfirmed.
pub(crate) const MISSES_CAP: u32 = 320;

/// How far before the previous counted key-down a hook call may lie and still count as a call
/// for it or after it: the tick clock's resolution, the order in which the system stamps, hooks
/// and posts one event, and a hook answering late by up to the system's own timeout (at most a
/// second since Windows 10 1709). A generous figure costs nothing: a hook that is gone stops
/// being called for good, so the slack delays a verdict by one second at most.
pub(crate) const CALL_SLACK_MS: u32 = 1_000;

/// Why the hook is being installed again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The machine resumed from sleep or hibernation (`PBT_APMRESUMEAUTOMATIC`).
    Resumed,
    /// The session was unlocked (`WTS_SESSION_UNLOCK`).
    Unlocked,
    /// The session was connected to the physical console again (`WTS_CONSOLE_CONNECT`).
    ConsoleConnected,
    /// The session was connected to a remote client (`WTS_REMOTE_CONNECT`).
    RemoteConnected,
    /// The witness saw `downs` physical key-downs in a row that the hook was not called for.
    Missed { downs: u32 },
}

impl Reason {
    /// Packed into a message's `WPARAM` for the trip to the hook's thread and back: the kind in
    /// the low byte, the count above it.
    pub(crate) fn encode(self) -> usize {
        match self {
            Reason::Resumed => 1,
            Reason::Unlocked => 2,
            Reason::ConsoleConnected => 3,
            Reason::RemoteConnected => 4,
            Reason::Missed { downs } => 5 | ((downs as usize) << 8),
        }
    }

    pub(crate) fn decode(w: usize) -> Option<Reason> {
        match w & 0xFF {
            1 => Some(Reason::Resumed),
            2 => Some(Reason::Unlocked),
            3 => Some(Reason::ConsoleConnected),
            4 => Some(Reason::RemoteConnected),
            5 => Some(Reason::Missed { downs: (w >> 8) as u32 }),
            _ => None,
        }
    }

    /// The words the log uses. The first three are the ones the maintainer asked to find there.
    pub(crate) fn words(self) -> String {
        match self {
            Reason::Resumed => "resumed: the machine came back from sleep or hibernation".to_string(),
            Reason::Unlocked => "unlocked: the session was unlocked".to_string(),
            Reason::ConsoleConnected => {
                "the session was connected to the console again".to_string()
            }
            Reason::RemoteConnected => "the session was connected to a remote client".to_string(),
            Reason::Missed { downs } => format!(
                "the hook stopped seeing keys the system delivered: {downs} key-downs in a row \
                 reached raw input and not the hook"
            ),
        }
    }
}

/// What became of the old hook once the new one was in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Old {
    /// `UnhookWindowsHookEx` took it out: it was still installed.
    Removed,
    /// `UnhookWindowsHookEx` answered `ERROR_INVALID_HOOK_HANDLE`: Windows had removed it.
    AlreadyGone,
    /// `UnhookWindowsHookEx` failed otherwise, with this error — and taking the NEW one out
    /// again failed as well, so both are in the chain. The old one is tried again at every
    /// later re-install ([`swap`]'s `stale`).
    Failed(u32),
}

/// A re-install's outcome, as the hook's thread reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The new hook is in; `old` is what happened to the one before it.
    Installed { old: Old },
    /// `SetWindowsHookExW` failed with `error`. The old hook was left where it was.
    Failed { error: u32 },
    /// The new hook went in, but the old one could not be taken out (`UnhookWindowsHookEx`
    /// error `error`, not `ERROR_INVALID_HOOK_HANDLE`, so it is still in the chain): the new one
    /// was taken out again rather than leave two of ours, each handling every key the other
    /// lets through. Everything is as it was before the re-install.
    Reverted { error: u32 },
}

/// `ERROR_INVALID_HOOK_HANDLE`, from WinError.h.
pub(crate) const ERROR_INVALID_HOOK_HANDLE: u32 = 1404;

impl Outcome {
    /// Packed into a message's `LPARAM`: the kind in the top bits of the low 32, an error code
    /// below (Win32 error codes fit in 16 bits; a larger one is clipped, never misread). Never
    /// 0, which [`pack_reply`] relies on.
    pub(crate) fn encode(self) -> isize {
        let (kind, code): (u32, u32) = match self {
            Outcome::Installed { old: Old::Removed } => (1, 0),
            Outcome::Installed { old: Old::AlreadyGone } => (2, 0),
            Outcome::Installed { old: Old::Failed(e) } => (3, e),
            Outcome::Failed { error } => (4, error),
            Outcome::Reverted { error } => (5, error),
        };
        ((kind << 24) | (code & 0x00FF_FFFF)) as isize
    }

    pub(crate) fn decode(l: isize) -> Option<Outcome> {
        let l = l as u32;
        let code = l & 0x00FF_FFFF;
        match l >> 24 {
            1 => Some(Outcome::Installed { old: Old::Removed }),
            2 => Some(Outcome::Installed { old: Old::AlreadyGone }),
            3 => Some(Outcome::Installed { old: Old::Failed(code) }),
            4 => Some(Outcome::Failed { error: code }),
            5 => Some(Outcome::Reverted { error: code }),
            _ => None,
        }
    }

    /// Whether a new hook is in the chain now.
    pub(crate) fn installed(self) -> bool {
        matches!(self, Outcome::Installed { .. })
    }

    /// Whether what the hook remembered of keys going by has to be forgotten: only when Windows
    /// had removed the old hook, which then missed their key-ups. A hook still installed saw
    /// every key the new one would have, so its record is right, and forgetting it would drop
    /// a screen reader's modifier held across the swap.
    pub(crate) fn forget_seen_keys(self) -> bool {
        matches!(self, Outcome::Installed { old: Old::AlreadyGone })
    }
}

/// A reply the hook's thread could not post to the watch, packed into one word for an atomic:
/// the [`Reason`] as it was asked above, the [`Outcome`] below. Never 0 (an outcome never
/// encodes to 0), so 0 is "nothing waiting".
pub(crate) fn pack_reply(reason: usize, outcome: Outcome) -> u64 {
    ((reason as u64 & 0xFFFF_FFFF) << 32) | (outcome.encode() as u32 as u64)
}

pub(crate) fn unpack_reply(packed: u64) -> Option<(Reason, Outcome)> {
    let reason = Reason::decode((packed >> 32) as usize)?;
    let outcome = Outcome::decode((packed & 0xFFFF_FFFF) as u32 as isize)?;
    Some((reason, outcome))
}

/// The swap itself, with the two system calls passed in so that its order can be tested:
/// `set` installs a new hook (`SetWindowsHookExW`; `Err` is `GetLastError`), `unhook` takes one
/// out (`UnhookWindowsHookEx`; `Err` is `GetLastError`). `current` is the hook installed now,
/// `stale` the hooks of ours an earlier swap could not take out. Returns the hook that is
/// current afterwards, and the outcome.
///
/// **New first, then old out.** The caller handles no message between the calls, so a key
/// arriving meanwhile waits for the new hook, which is first in the chain, and by the time it
/// goes on the old one is out: no key handled twice, none without a hook of ours.
///
/// - `set` fails: nothing is taken out; the old hook stays, and it may still work.
/// - The old one answers `ERROR_INVALID_HOOK_HANDLE`: Windows had removed it.
/// - The old one fails otherwise: it is still in the chain, and two hooks of ours would handle
///   every key the first lets through a second time. So the new one is taken out again and the
///   state is what it was ([`Outcome::Reverted`]). Only if that fails too do both stay; the old
///   one is then kept in `stale` and tried again at every later swap ([`Old::Failed`]).
pub(crate) fn swap<H: Copy>(
    current: H,
    stale: &mut Vec<H>,
    set: impl FnOnce() -> Result<H, u32>,
    mut unhook: impl FnMut(H) -> Result<(), u32>,
) -> (H, Outcome) {
    let new = match set() {
        Ok(new) => new,
        Err(error) => return (current, Outcome::Failed { error }),
    };
    let earlier = std::mem::take(stale);
    let (after, outcome) = match unhook(current) {
        Ok(()) => (new, Outcome::Installed { old: Old::Removed }),
        Err(ERROR_INVALID_HOOK_HANDLE) => (new, Outcome::Installed { old: Old::AlreadyGone }),
        Err(error) => match unhook(new) {
            Ok(()) | Err(ERROR_INVALID_HOOK_HANDLE) => (current, Outcome::Reverted { error }),
            Err(_) => {
                stale.push(current);
                (new, Outcome::Installed { old: Old::Failed(error) })
            }
        },
    };
    // Hooks an earlier swap could not take out: out now, gone by now, or still refusing.
    for h in earlier {
        if let Err(e) = unhook(h) {
            if e != ERROR_INVALID_HOOK_HANDLE {
                stale.push(h);
            }
        }
    }
    (after, outcome)
}

/// What the watch saw of a run of missed key-downs, for the re-install line the run asked for.
/// Diagnostics only: none of it decides anything.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MissedRun {
    /// How many of the run's key-downs raw input reported with `RIM_INPUT`: this process had the
    /// foreground when the key was pressed (`RIM_INPUTSINK` otherwise). Such a key is never
    /// counted ([`counts`]), so only the run's first can be one: the key the first miss was judged
    /// from, which the hook need not have seen.
    pub foreground: u32,
    /// The window in front when the watch asked for the re-install; `None` when there was none.
    pub front: Option<FrontWindow>,
    /// How often the hook procedure was entered, for any code, since raw input's report of the
    /// first missed key-down — the key before the one the witness first found the hook silent at —
    /// and since the process started.
    pub entered: u32,
    pub entered_total: u32,
    /// The run's first key-down, and the hook's last call (`None`: never called), as ticks.
    pub first_at: u32,
    pub last_call: Option<u32>,
}

/// A window in front, as [`MissedRun`] names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FrontWindow {
    pub hwnd: isize,
    pub class: String,
    /// The process's image name, empty when it could not be read.
    pub exe: String,
    pub pid: u32,
    /// Whether it is this process's own window.
    pub ours: bool,
}

/// The words a missed run adds to the reason in the re-install line.
pub(crate) fn missed_words(run: &MissedRun) -> String {
    let front = match &run.front {
        Some(w) => format!(
            "window in front {:#x} class '{}' {} pid {} (this application: {})",
            w.hwnd,
            w.class,
            if w.exe.is_empty() { "?" } else { &w.exe },
            w.pid,
            if w.ours { "yes" } else { "no" }
        ),
        None => "no window in front".to_string(),
    };
    let last = match run.last_call {
        None => "never called".to_string(),
        Some(call) => {
            let before = run.first_at.wrapping_sub(call) as i32;
            if before >= 0 {
                format!("last called {before} ms before it")
            } else {
                format!("last called {} ms after it", -(before as i64))
            }
        }
    };
    format!(
        "this process in the foreground for {} of them; {front}; hook entered {} times since the \
         first missed key ({} in all), {last}",
        run.foreground, run.entered, run.entered_total
    )
}

/// The one line a re-install writes to the log. `run` is what the watch saw of the missed
/// key-downs that asked for it, when they did.
pub(crate) fn line(reason: Reason, outcome: Outcome, run: Option<&MissedRun>) -> String {
    let why = match run {
        Some(run) => format!("{}; {}", reason.words(), missed_words(run)),
        None => reason.words(),
    };
    match outcome {
        Outcome::Installed { old } => {
            let before = match old {
                Old::Removed => "the old hook was still installed and has been taken out".to_string(),
                Old::AlreadyGone => "Windows had already removed the old hook — it had stopped \
                                     being called, so captured keys and the hotkeys the hook \
                                     matches were not working until now"
                    .to_string(),
                Old::Failed(e) => format!(
                    "taking the old hook out failed (UnhookWindowsHookEx error {e}), and so did \
                     taking the new one out again: both are in the chain, and a key the new one \
                     lets through reaches the old one too and is handled a second time, until \
                     the old one can be taken out — tried again at every re-install"
                ),
            };
            let forgot = if outcome.forget_seen_keys() {
                "; what the old hook had recorded as held (a screen reader's modifier, a pending \
                 modifier tap) was forgotten, since it missed the key-ups"
            } else {
                ""
            };
            format!(
                "the keyboard hook was installed again ({why}); {before}. It is first in the chain \
                 of low-level keyboard hooks again, ahead of every hook installed since the \
                 application started — a screen reader started or restarted since then included \
                 — as it is when the application starts after the screen reader; the captured \
                 keys and granted hotkeys carried over{forgot}"
            )
        }
        Outcome::Failed { error } => format!(
            "installing the keyboard hook again failed (SetWindowsHookExW error {error}) after \
             this: {why}. The old hook stays as it was — dead, if Windows removed it, and then \
             captured keys and the hotkeys the hook matches do not work (RegisterHotKey still \
             delivers hotkeys). Tried again when the hook misses more key-downs, and at the \
             next resume or unlock"
        ),
        Outcome::Reverted { error } => format!(
            "the keyboard hook was not installed again after this: {why}. The new hook went in, \
             but the old one could not be taken out (UnhookWindowsHookEx error {error}), so the \
             new one was taken out again rather than leave two hooks of ours handling every key \
             twice; the old hook stays as it was. Tried again when the hook misses more \
             key-downs, and at the next resume or unlock"
        ),
    }
}

/// What a session change means for the hook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// Install the hook again, for this reason; the witness starts counting afresh.
    Rehook(Reason),
    /// The keyboard is going elsewhere for a while (locked, disconnected, suspending): the
    /// witness starts counting afresh, and nothing else happens.
    Restart,
    /// Nothing to do.
    Nothing,
}

/// `WM_WTSSESSION_CHANGE`'s `wParam` (WinUser.h's `WTS_*` codes) as a [`Change`].
///
/// Only a restart of the count on the way out, never a state the witness keeps: a lock whose
/// unlock notification got lost must not switch the witness off for the rest of the session.
/// Whether a key-down can be counted is asked again for each one (the desktop that has the
/// keyboard, the window in front).
pub(crate) fn on_session_change(code: u32) -> Change {
    match code {
        1 => Change::Rehook(Reason::ConsoleConnected), // WTS_CONSOLE_CONNECT
        2 => Change::Restart,                          // WTS_CONSOLE_DISCONNECT
        3 => Change::Rehook(Reason::RemoteConnected),  // WTS_REMOTE_CONNECT
        4 => Change::Restart,                          // WTS_REMOTE_DISCONNECT
        7 => Change::Restart,                          // WTS_SESSION_LOCK
        8 => Change::Rehook(Reason::Unlocked),         // WTS_SESSION_UNLOCK
        _ => Change::Nothing,
    }
}

/// A suspend/resume notification's event type (`PBT_*`) as a [`Change`]. Resuming is answered
/// on `PBT_APMRESUMEAUTOMATIC`, which is sent on every resume; `PBT_APMRESUMESUSPEND`, sent
/// after it when a person woke the machine, would only install the hook a second time.
pub(crate) fn on_power(pbt: u32) -> Change {
    match pbt {
        0x4 => Change::Restart,                 // PBT_APMSUSPEND
        0x12 => Change::Rehook(Reason::Resumed), // PBT_APMRESUMEAUTOMATIC
        _ => Change::Nothing,
    }
}

/// `RI_KEY_BREAK` in `RAWKEYBOARD::Flags`: the report is a key-up.
const RI_KEY_BREAK: u16 = 1;
/// `KEYBOARD_OVERRUN_MAKE_CODE`, and the virtual key raw input reports for the half of an
/// escaped scan-code sequence that is no key (the fake Shift around Print Screen or the
/// navigation keys on some keyboards): no low-level hook is called for those.
const NO_KEY: u16 = 0xFF;

/// Whether a raw keyboard report is a physical key-down the hook would have been called for:
/// from a device (injected input carries no device handle), a make rather than a break, and a
/// real key rather than half of an escape sequence.
pub(crate) fn physical_down(has_device: bool, flags: u16, vkey: u16, make_code: u16) -> bool {
    has_device && flags & RI_KEY_BREAK == 0 && vkey != NO_KEY && vkey != 0 && make_code != NO_KEY
}

/// Whether the hook of a process at integrity level `own` is called for input going to a
/// window of a process at `foreground` (the RIDs of their mandatory labels: medium is 0x2000,
/// a UIAccess program 0x2010, an elevated one 0x3000). `None` is a level that could not be
/// read, and then the key is not counted: the witness only ever counts a key it is sure the
/// hook should have seen.
pub(crate) fn hook_sees(own: Option<u32>, foreground: Option<u32>) -> bool {
    matches!((own, foreground), (Some(own), Some(fg)) if fg <= own)
}

/// Whether the keys going to a window of a process at `foreground` go by unseen by the hook of
/// a process at `own`, so that the hook can miss the key-up of a key it saw go down — and what
/// it recorded as held (a screen reader's modifier, a modifier waiting to be a tap) has to be
/// forgotten. NVDA+N is the everyday case: Insert goes down in front of an ordinary window,
/// NVDA's menu comes to the front, and Insert goes up to the menu.
///
/// The other side of [`hook_sees`], for the opposite purpose, and so with the opposite answer
/// for a level that could not be read: a window whose level is unknown is most likely a
/// protected or a system process's, above ours, and forgetting costs at most a screen reader's
/// modifier held across the switch. With our own level unknown nothing is forgotten, or every
/// foreground change would forget.
pub(crate) fn keys_go_unseen(own: Option<u32>, foreground: Option<u32>) -> bool {
    own.is_some() && !hook_sees(own, foreground)
}

/// Why the keys going to a window that came to the front go by unseen by the hook — see
/// [`front_unseen`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unseen {
    /// One of this process's own windows: Windows does not call the hook for keys going to them.
    OwnWindow,
    /// A window of a process above ours, at this level; `None` when its level could not be read.
    Above(Option<u32>),
}

impl Unseen {
    /// The words the log uses for the window that came to the front.
    pub(crate) fn words(self) -> String {
        let which = match self {
            Unseen::OwnWindow => {
                "one of this application's own: Windows does not call its hook for keys going to them"
                    .to_string()
            }
            Unseen::Above(Some(level)) => format!("integrity level {level:#06x}, above this process's"),
            Unseen::Above(None) => "its integrity level could not be read".to_string(),
        };
        format!("a window the keyboard hook is not called for came to the front ({which})")
    }
}

/// Whether the keys going to the window that came to the front go by unseen by the hook of a
/// process at `own`, and why: the window is one of this process's own (`ours`) — whatever the
/// levels say — or one of a process at `foreground` that [`keys_go_unseen`] says the hook is not
/// called for. `None` when the hook is called for them, and nothing has to be forgotten.
pub(crate) fn front_unseen(own: Option<u32>, ours: bool, foreground: Option<u32>) -> Option<Unseen> {
    if ours {
        Some(Unseen::OwnWindow)
    } else if keys_go_unseen(own, foreground) {
        Some(Unseen::Above(foreground))
    } else {
        None
    }
}

/// `RIM_INPUT`: the input code (`GET_RAWINPUT_CODE_WPARAM`, the low byte of a `WM_INPUT`'s
/// `wParam`) of input that came while this process had the foreground. `RIM_INPUTSINK`, 1, is
/// every other.
const RIM_INPUT: usize = 0;

/// Whether raw input reported a key with `RIM_INPUT` — `wparam` is its `WM_INPUT`'s: this process
/// had the foreground when the key was pressed, so the key went to one of its windows. Decided by
/// the system as the key came, not when the watch gets to it.
pub(crate) fn to_this_process(wparam: usize) -> bool {
    wparam & 0xFF == RIM_INPUT
}

/// Whether a key-down the hook was not called for counts as one it missed. Never one that went to
/// a window of this process ([`to_this_process`]): Windows does not call the process's own hook
/// for those. Otherwise `hook_could_see` decides — the desktop that has the keyboard and the window
/// in front ([`hook_sees`]) — asked only then, since it costs system calls.
pub(crate) fn counts(to_this_process: bool, hook_could_see: impl FnOnce() -> bool) -> bool {
    !to_this_process && hook_could_see()
}

/// Whether a registration Windows refused is tried again at the `n`th physical key-down since
/// (1-based): the 1st, 2nd, 4th, 8th and so on. A service that starts late — the Remote Desktop
/// services behind `WTSRegisterSessionNotification`, early after sign-in — is caught within a
/// few keys, and one that never runs costs one refused call per doubling of the keys typed,
/// about twenty in a million.
pub(crate) fn retry_at(n: u64) -> bool {
    n.is_power_of_two()
}

/// The timestamp a raw key-down is judged by: its message time (`GetMessageTime` while its
/// `WM_INPUT` is dispatched — the input's own time, on the tick clock the hook's calls are
/// stamped with), unless that is not a time this key-down can have — ahead of `now`, or more
/// than [`MESSAGE_TIME_PLAUSIBLE_MS`] behind it — and then `now`, the tick at which the witness
/// got to it. A message time of 0, say, would otherwise read as a key-down from the day the
/// machine started, which every hook call since would count as "after".
pub(crate) fn event_time(message_time: u32, now: u32) -> u32 {
    let age = now.wrapping_sub(message_time) as i32;
    if (0..=MESSAGE_TIME_PLAUSIBLE_MS as i32).contains(&age) {
        message_time
    } else {
        now
    }
}

/// How far behind the tick clock a raw key-down's message time may be and still be believed:
/// far longer than the watch's thread is ever kept waiting, far shorter than anything wrong.
pub(crate) const MESSAGE_TIME_PLAUSIBLE_MS: u32 = 60_000;

/// Whether the hook was called at or after `time`, with [`CALL_SLACK_MS`] of slack.
/// `last_call` is `None` until the hook has been called at all. Tick-clock arithmetic: correct
/// across the clock's wrap, for times less than 24 days apart.
fn called_since(last_call: Option<u32>, time: u32) -> bool {
    match last_call {
        None => false,
        Some(call) => (call.wrapping_sub(time) as i32) >= -(CALL_SLACK_MS as i32),
    }
}

/// What the witness makes of one physical key-down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The first key-down since the count started; nothing to judge it against yet.
    First,
    /// The hook was called since the previous key-down: it is alive.
    Seen,
    /// The hook could not have seen this key (see [`counts`]: a window of this process, one
    /// above it, another desktop); it is not counted.
    NotCounted,
    /// The hook was not called since the previous key-down; `streak` in a row now.
    Missed { streak: u32 },
    /// That makes enough: install the hook again. The count starts afresh, and the next
    /// verdict takes twice as many until the hook confirms the re-install by being called.
    Rehook { downs: u32 },
}

/// The comparison between the key-downs raw input delivered and the calls the hook got.
#[derive(Clone, Debug)]
pub(crate) struct Witness {
    /// The timestamp of the previous counted key-down.
    prev: Option<u32>,
    /// Counted key-downs in a row with no hook call since the one before.
    streak: u32,
    /// How many make a re-install: [`MISSES_TO_REHOOK`], doubled at every re-install the
    /// witness asks for (and at every one that failed) until the hook confirms one by being
    /// called, up to [`MISSES_CAP`].
    needed: u32,
    /// The time from which a hook call confirms the last raise of the bar: the key-down that
    /// made the witness ask, or the tick a failed re-install was reported at. `None` once the
    /// hook has been called since.
    unconfirmed: Option<u32>,
}

impl Default for Witness {
    fn default() -> Self {
        Witness { prev: None, streak: 0, needed: MISSES_TO_REHOOK, unconfirmed: None }
    }
}

impl Witness {
    /// A physical key-down stamped `time`, with the hook last called at `last_call`. `visible`
    /// is asked only when the key would count as missed, and says whether the hook could have
    /// seen it at all ([`counts`]: where the key went, the window in front, the desktop that has
    /// the keyboard): those questions cost system calls, and a hook that was called needs none of
    /// them.
    pub(crate) fn key_down(
        &mut self,
        time: u32,
        last_call: Option<u32>,
        visible: impl FnOnce() -> bool,
    ) -> Verdict {
        if let Some(at) = self.unconfirmed {
            if called_since(last_call, at) {
                self.unconfirmed = None;
                self.needed = MISSES_TO_REHOOK;
            }
        }
        let Some(prev) = self.prev else {
            self.prev = Some(time);
            return Verdict::First;
        };
        if called_since(last_call, prev) {
            self.streak = 0;
            self.prev = Some(time);
            return Verdict::Seen;
        }
        if !visible() {
            // Not counted, and not remembered either: the next key is still judged against the
            // last one the hook should have seen.
            return Verdict::NotCounted;
        }
        self.prev = Some(time);
        self.streak += 1;
        if self.streak < self.needed {
            return Verdict::Missed { streak: self.streak };
        }
        let downs = self.streak;
        self.restart();
        // Raised here, when the witness asks, and not when the answer comes: an answer that
        // never arrives must not leave the next request as cheap as this one, or a hook thread
        // that cannot answer would be asked again every few key-downs for days.
        self.raise(time);
        Verdict::Rehook { downs }
    }

    /// Start the count afresh: the keyboard went elsewhere for a while, or the hook is being
    /// installed again for another reason.
    pub(crate) fn restart(&mut self) {
        self.prev = None;
        self.streak = 0;
    }

    /// Doubles the bar, up to the cap, until the hook is called at or after `at`.
    fn raise(&mut self, at: u32) {
        self.needed = (self.needed.saturating_mul(2)).min(MISSES_CAP);
        self.unconfirmed = Some(at);
    }

    /// A re-install's outcome is in. `asked_by_witness` is whether it was [`Verdict::Rehook`]'s;
    /// `at` the tick it was reported at. The count starts afresh. The bar was raised when the
    /// witness asked, and stays raised until the hook confirms the re-install by being called —
    /// so a hook that stays deaf for a reason nobody foresaw is installed again after 5, 10,
    /// 20… missed key-downs rather than after every five. A failed re-install for another reason
    /// (resume, unlock) raises it the same way; a successful one leaves it where it is.
    pub(crate) fn rehooked(&mut self, asked_by_witness: bool, installed: bool, at: u32) {
        self.restart();
        if !asked_by_witness && !installed {
            self.raise(at);
        }
    }

    /// How many missed key-downs the next re-install takes.
    pub(crate) fn needed(&self) -> u32 {
        self.needed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Key-downs `gap` ms apart from `start`, with the hook called for each (`alive`) or never.
    fn run(
        w: &mut Witness,
        start: u32,
        gap: u32,
        n: u32,
        alive: bool,
        mut last: Option<u32>,
    ) -> (Vec<Verdict>, Option<u32>) {
        let mut out = Vec::new();
        for i in 0..n {
            let t = start.wrapping_add(i * gap);
            if alive {
                last = Some(t.wrapping_add(1));
            }
            out.push(w.key_down(t, last, || true));
        }
        (out, last)
    }

    #[test]
    fn a_hook_that_is_called_is_never_reinstalled() {
        let mut w = Witness::default();
        let (v, _) = run(&mut w, 10_000, 30, 500, true, None);
        assert_eq!(v[0], Verdict::First);
        assert!(v[1..].iter().all(|v| *v == Verdict::Seen), "{v:?}");
    }

    #[test]
    fn a_hook_that_stopped_being_called_is_reinstalled_after_the_threshold_of_missed_key_downs() {
        const N: u32 = MISSES_TO_REHOOK;
        let mut w = Witness::default();
        // Alive for a while, then dead at the last call below.
        let (_, last) = run(&mut w, 10_000, 200, 10, true, None);
        // Hours later.
        let later = 10_000 + 36_000_000;
        let (v, _) = run(&mut w, later, 250, N + 1, false, last);
        // The first key after the gap is judged against the previous one, which the hook saw.
        assert_eq!(v[0], Verdict::Seen);
        let mut expected: Vec<Verdict> = (1..N).map(|streak| Verdict::Missed { streak }).collect();
        expected.push(Verdict::Rehook { downs: N });
        assert_eq!(&v[1..], expected.as_slice());
    }

    #[test]
    fn a_hook_answering_late_is_not_taken_for_a_dead_one() {
        // Every call runs 900 ms after its key, keys come every 150 ms, and the witness judges
        // each key 5 ms after it — so it always judges with calls that are several keys behind.
        const LATE: u32 = 900;
        let mut w = Witness::default();
        let key = |i: u32| 10_000 + i * 150;
        let mut verdicts = Vec::new();
        for i in 0..60u32 {
            let judged_at = key(i) + 5;
            // The latest call that has run by then: for the latest key at least LATE ms old,
            // or the one the hook had answered just before this run of keys.
            let last = (0..=i)
                .rev()
                .map(|j| key(j) + LATE)
                .find(|&call| call <= judged_at)
                .unwrap_or(9_990);
            verdicts.push(w.key_down(key(i), Some(last), || true));
        }
        assert!(verdicts.iter().all(|v| matches!(v, Verdict::First | Verdict::Seen)), "{verdicts:?}");
    }

    #[test]
    fn a_witness_running_late_sees_the_hook_as_more_alive_not_less() {
        // The witness got to five queued key-downs at once, long after the hook had been
        // called for all of them: each is judged by its own timestamp.
        let mut w = Witness::default();
        let times = [5_000u32, 5_030, 5_060, 5_090, 5_120, 5_150];
        let last = Some(5_151);
        let v: Vec<_> = times.iter().map(|&t| w.key_down(t, last, || true)).collect();
        assert_eq!(v[0], Verdict::First);
        assert!(v[1..].iter().all(|v| *v == Verdict::Seen));
    }

    #[test]
    fn keys_the_hook_cannot_see_are_not_counted_and_do_not_move_the_reference() {
        let mut w = Witness::default();
        assert_eq!(w.key_down(1_000, Some(1_001), || true), Verdict::First);
        assert_eq!(w.key_down(1_500, Some(1_501), || true), Verdict::Seen);
        // An elevated window in front from 5 s on: the hook is not called any more. The first
        // key there is judged by the key before it, which the hook saw; the rest are not
        // counted, however many there are.
        assert_eq!(w.key_down(5_000, Some(1_501), || false), Verdict::Seen);
        for i in 1..40 {
            assert_eq!(w.key_down(5_000 + i * 100, Some(1_501), || false), Verdict::NotCounted);
        }
        assert_eq!(w.prev, Some(5_000), "the last key the hook could see is still the reference");
        assert_eq!(w.streak, 0);
        // Back in an ordinary window, the hook called for the key: alive.
        assert_eq!(w.key_down(9_000, Some(9_001), || true), Verdict::Seen);
    }

    #[test]
    fn visibility_is_asked_only_when_the_hook_was_not_called() {
        let mut w = Witness::default();
        w.key_down(1_000, Some(1_001), || panic!("asked for the first key"));
        w.key_down(1_100, Some(1_101), || panic!("asked for a key the hook saw"));
    }

    #[test]
    fn a_hit_breaks_the_streak() {
        let mut w = Witness::default();
        assert_eq!(w.key_down(0, None, || true), Verdict::First);
        for i in 1..MISSES_TO_REHOOK {
            assert_eq!(w.key_down(i * 5_000, None, || true), Verdict::Missed { streak: i });
        }
        // Called again since the last key: the streak is over.
        assert_eq!(w.key_down(30_000, Some(28_000), || true), Verdict::Seen);
        // And not since this one (more than the slack before it): a new streak.
        assert_eq!(w.key_down(35_000, Some(28_000), || true), Verdict::Missed { streak: 1 });
    }

    #[test]
    fn a_reinstall_the_hook_never_confirms_raises_the_bar_and_a_confirmed_one_lowers_it() {
        let mut w = Witness::default();
        w.key_down(0, None, || true);
        let mut t = 0;
        let mut downs_seen = Vec::new();
        for _ in 0..4 {
            loop {
                t += 1_500;
                if let Verdict::Rehook { downs } = w.key_down(t, None, || true) {
                    downs_seen.push(downs);
                    w.rehooked(true, true, t);
                    break;
                }
            }
        }
        let n = MISSES_TO_REHOOK;
        assert_eq!(downs_seen, vec![n, 2 * n, 4 * n, 8 * n]);
        // The hook is called at last: the bar is back where it started.
        t += 1_500;
        assert_eq!(w.key_down(t, Some(t), || true), Verdict::First);
        assert_eq!(w.needed(), MISSES_TO_REHOOK);
    }

    #[test]
    fn the_bar_stops_at_its_cap() {
        let mut w = Witness::default();
        let mut t = 0u32;
        w.key_down(t, None, || true);
        let mut verdicts = 0;
        while verdicts < 20 {
            t += 1_500;
            if let Verdict::Rehook { .. } = w.key_down(t, None, || true) {
                verdicts += 1;
                // The answer never comes: the bar goes on rising all the same.
            }
        }
        assert_eq!(w.needed(), MISSES_CAP);
    }

    /// The case that matters: the dead hook's last call is a real, old one, not "never". It
    /// lies before every key-down the witness counted, so it must not read as the new hook
    /// confirming the re-install.
    #[test]
    fn a_call_from_before_the_reinstall_does_not_confirm_it() {
        let mut w = Witness::default();
        // Alive until its last call at 10_001.
        let (_, last) = run(&mut w, 10_000, 200, 1, true, None);
        assert_eq!(last, Some(10_001));
        const N: u32 = MISSES_TO_REHOOK;
        let (v, _) = run(&mut w, 20_000, 1_500, N + 1, false, last);
        assert_eq!(v.last(), Some(&Verdict::Rehook { downs: N }));
        w.rehooked(true, true, 20_000 + (N + 1) * 1_500);
        // Still never called: the bar stays raised, and the next verdict takes twice as many.
        let (v, _) = run(&mut w, 60_000, 1_500, 2 * N + 1, false, last);
        assert_eq!(w.needed(), N * 4, "raised again at the second verdict");
        assert_eq!(v.last(), Some(&Verdict::Rehook { downs: 2 * N }), "{v:?}");
    }

    #[test]
    fn a_verdict_starts_the_count_afresh() {
        let mut w = Witness::default();
        w.key_down(0, None, || true);
        let mut t = 0;
        loop {
            t += 1_500;
            if let Verdict::Rehook { .. } = w.key_down(t, None, || true) {
                break;
            }
        }
        // Before any answer: the next key-down is the first of a new count, not a sixth miss.
        assert_eq!(w.key_down(t + 1_500, None, || true), Verdict::First);
        assert_eq!(w.key_down(t + 3_000, None, || true), Verdict::Missed { streak: 1 });
    }

    #[test]
    fn a_miss_moves_the_reference_and_an_uncounted_key_does_not() {
        let mut w = Witness::default();
        assert_eq!(w.key_down(1_000, Some(1_001), || true), Verdict::First);
        assert_eq!(w.key_down(5_000, Some(1_001), || true), Verdict::Seen);
        assert_eq!(w.key_down(9_000, Some(1_001), || true), Verdict::Missed { streak: 1 });
        assert_eq!(w.prev, Some(9_000), "a counted miss is the next key's reference");
        assert_eq!(w.key_down(13_000, Some(1_001), || false), Verdict::NotCounted);
        assert_eq!(w.prev, Some(9_000), "a key the hook could not see is nobody's reference");
        assert_eq!(w.streak, 1);
    }

    #[test]
    fn a_reinstall_for_resume_or_unlock_restarts_the_count_and_keeps_the_bar() {
        let mut w = Witness::default();
        w.key_down(0, None, || true);
        w.key_down(1_500, None, || true);
        w.rehooked(false, true, 2_000);
        assert_eq!(w.needed(), MISSES_TO_REHOOK);
        assert_eq!(w.key_down(3_000, None, || true), Verdict::First);
        // A failed one raises the bar, whoever asked for it.
        w.rehooked(false, false, 4_000);
        assert_eq!(w.needed(), MISSES_TO_REHOOK * 2);
    }

    #[test]
    fn the_comparison_survives_the_tick_clock_wrapping() {
        let mut w = Witness::default();
        let start = u32::MAX - 200;
        let (v, last) = run(&mut w, start, 100, 6, true, None);
        assert_eq!(v[0], Verdict::First);
        assert!(v[1..].iter().all(|v| *v == Verdict::Seen), "{v:?}");
        let (v, _) = run(&mut w, start.wrapping_add(5_000), 3_000, MISSES_TO_REHOOK + 1, false, last);
        assert_eq!(v.last(), Some(&Verdict::Rehook { downs: MISSES_TO_REHOOK }));
    }

    #[test]
    fn a_key_down_is_judged_by_its_own_time_when_that_is_believable() {
        assert_eq!(event_time(9_990, 10_000), 9_990);
        assert_eq!(event_time(10_000, 10_000), 10_000);
        // Late by a stall of the watch's thread: still its own time.
        assert_eq!(event_time(60_000 - 40_000, 60_000), 60_000 - 40_000);
        // Across the tick clock's wrap.
        assert_eq!(event_time(u32::MAX - 5, 10), u32::MAX - 5);
        // Not a time this key-down can have: 0 on a machine up for days, or ahead of now.
        assert_eq!(event_time(0, 400_000_000), 400_000_000);
        assert_eq!(event_time(10_500, 10_000), 10_000);
    }

    #[test]
    fn only_physical_key_downs_count() {
        assert!(physical_down(true, 0, 0x09, 0x0F)); // Tab from a keyboard
        assert!(physical_down(true, 2, 0x2D, 0x52)); // Insert, extended (RI_KEY_E0)
        assert!(!physical_down(false, 0, 0x09, 0x0F)); // injected: no device
        assert!(!physical_down(true, 1, 0x09, 0x0F)); // the key-up
        assert!(!physical_down(true, 2, 0xFF, 0x2A)); // the fake Shift of an escape sequence
        assert!(!physical_down(true, 0, 0x41, 0xFF)); // overrun
        assert!(!physical_down(true, 0, 0, 0x1E)); // no virtual key
    }

    #[test]
    fn the_hook_sees_only_windows_at_its_own_integrity_level_or_below() {
        const MEDIUM: u32 = 0x2000;
        const UIACCESS: u32 = 0x2010;
        const HIGH: u32 = 0x3000;
        const LOW: u32 = 0x1000;
        assert!(hook_sees(Some(MEDIUM), Some(MEDIUM)));
        assert!(hook_sees(Some(MEDIUM), Some(LOW)));
        assert!(!hook_sees(Some(MEDIUM), Some(HIGH)), "an elevated window");
        assert!(!hook_sees(Some(MEDIUM), Some(UIACCESS)), "a screen reader's own dialog");
        assert!(hook_sees(Some(HIGH), Some(HIGH)), "the application itself elevated");
        assert!(!hook_sees(Some(MEDIUM), None), "a level that could not be read");
        assert!(!hook_sees(None, Some(MEDIUM)));
    }

    #[test]
    fn session_and_power_changes_map_to_what_they_mean() {
        assert_eq!(on_session_change(8), Change::Rehook(Reason::Unlocked));
        assert_eq!(on_session_change(1), Change::Rehook(Reason::ConsoleConnected));
        assert_eq!(on_session_change(3), Change::Rehook(Reason::RemoteConnected));
        for code in [2, 4, 7] {
            assert_eq!(on_session_change(code), Change::Restart, "code {code}");
        }
        for code in [5, 6, 9, 10, 11, 0, 99] {
            assert_eq!(on_session_change(code), Change::Nothing, "code {code}");
        }
        assert_eq!(on_power(0x12), Change::Rehook(Reason::Resumed));
        assert_eq!(on_power(0x4), Change::Restart);
        assert_eq!(on_power(0x7), Change::Nothing, "PBT_APMRESUMESUSPEND follows the automatic one");
        assert_eq!(on_power(0xA), Change::Nothing);
    }

    #[test]
    fn reasons_and_outcomes_survive_the_trip_through_a_message() {
        for r in [
            Reason::Resumed,
            Reason::Unlocked,
            Reason::ConsoleConnected,
            Reason::RemoteConnected,
            Reason::Missed { downs: 5 },
            Reason::Missed { downs: 320 },
        ] {
            assert_eq!(Reason::decode(r.encode()), Some(r));
        }
        assert_eq!(Reason::decode(0), None);
        for o in [
            Outcome::Installed { old: Old::Removed },
            Outcome::Installed { old: Old::AlreadyGone },
            Outcome::Installed { old: Old::Failed(5) },
            Outcome::Failed { error: 1428 },
            Outcome::Reverted { error: 5 },
        ] {
            assert_eq!(Outcome::decode(o.encode()), Some(o));
            assert_ne!(o.encode(), 0);
            let r = Reason::Missed { downs: MISSES_CAP };
            assert_eq!(unpack_reply(pack_reply(r.encode(), o)), Some((r, o)));
            assert_ne!(pack_reply(r.encode(), o), 0);
        }
        assert_eq!(Outcome::decode(0), None);
        assert_eq!(unpack_reply(0), None, "0 is nothing waiting");
    }

    /// A chain of fake hooks and a record of every call, in order.
    #[derive(Default)]
    struct Chain {
        installed: Vec<u32>,
        calls: Vec<String>,
        next: u32,
        set_fails: Option<u32>,
        /// Handles `unhook` refuses, with the error it gives.
        refuse: Vec<(u32, u32)>,
        /// Handles Windows removed on its own.
        removed_by_windows: Vec<u32>,
    }

    impl Chain {
        fn with(first: u32) -> Chain {
            Chain { installed: vec![first], next: first + 1, ..Chain::default() }
        }
        fn swap(&mut self, current: u32, stale: &mut Vec<u32>) -> (u32, Outcome) {
            let this = std::cell::RefCell::new(self);
            swap(
                current,
                stale,
                || {
                    let mut c = this.borrow_mut();
                    c.calls.push("set".to_string());
                    if let Some(e) = c.set_fails {
                        return Err(e);
                    }
                    let h = c.next;
                    c.next += 1;
                    c.installed.insert(0, h); // newest first, as Windows calls them
                    Ok(h)
                },
                |h| {
                    let mut c = this.borrow_mut();
                    c.calls.push(format!("unhook {h}"));
                    if let Some(&(_, e)) = c.refuse.iter().find(|(r, _)| *r == h) {
                        return Err(e);
                    }
                    if c.removed_by_windows.contains(&h) || !c.installed.contains(&h) {
                        return Err(ERROR_INVALID_HOOK_HANDLE);
                    }
                    c.installed.retain(|&x| x != h);
                    Ok(())
                },
            )
        }
    }

    #[test]
    fn the_new_hook_goes_in_before_the_old_one_comes_out() {
        let mut c = Chain::with(1);
        let mut stale = Vec::new();
        assert_eq!(c.swap(1, &mut stale), (2, Outcome::Installed { old: Old::Removed }));
        assert_eq!(c.calls, ["set", "unhook 1"]);
        assert_eq!(c.installed, [2]);
        assert!(stale.is_empty());
    }

    #[test]
    fn an_old_hook_windows_removed_is_named_so() {
        let mut c = Chain::with(1);
        c.installed.clear();
        c.removed_by_windows.push(1);
        let mut stale = Vec::new();
        let (now, outcome) = c.swap(1, &mut stale);
        assert_eq!((now, outcome), (2, Outcome::Installed { old: Old::AlreadyGone }));
        assert!(outcome.forget_seen_keys());
        assert!(!Outcome::Installed { old: Old::Removed }.forget_seen_keys());
        assert_eq!(c.installed, [2]);
    }

    #[test]
    fn a_failed_install_leaves_the_old_hook_alone() {
        let mut c = Chain::with(1);
        c.set_fails = Some(8);
        let mut stale = vec![7];
        assert_eq!(c.swap(1, &mut stale), (1, Outcome::Failed { error: 8 }));
        assert_eq!(c.calls, ["set"], "nothing taken out");
        assert_eq!(c.installed, [1]);
        assert_eq!(stale, [7], "kept for the next swap");
    }

    #[test]
    fn an_old_hook_that_will_not_come_out_takes_the_new_one_out_again() {
        let mut c = Chain::with(1);
        c.refuse.push((1, 5));
        let mut stale = Vec::new();
        assert_eq!(c.swap(1, &mut stale), (1, Outcome::Reverted { error: 5 }));
        assert_eq!(c.calls, ["set", "unhook 1", "unhook 2"]);
        assert_eq!(c.installed, [1], "as it was: one hook of ours");
        assert!(stale.is_empty());
    }

    #[test]
    fn two_hooks_that_will_not_come_out_are_both_kept_and_the_old_one_is_tried_again() {
        let mut c = Chain::with(1);
        c.refuse.push((1, 5));
        c.refuse.push((2, 5));
        let mut stale = Vec::new();
        assert_eq!(c.swap(1, &mut stale), (2, Outcome::Installed { old: Old::Failed(5) }));
        assert_eq!(stale, [1]);
        assert_eq!(c.installed, [2, 1]);
        // Later, both come out again: the next swap takes the current one out and the stale
        // one with it.
        c.refuse.clear();
        c.calls.clear();
        assert_eq!(c.swap(2, &mut stale), (3, Outcome::Installed { old: Old::Removed }));
        assert_eq!(c.calls, ["set", "unhook 2", "unhook 1"]);
        assert_eq!(c.installed, [3]);
        assert!(stale.is_empty());
    }

    #[test]
    fn a_stale_hook_is_dropped_once_it_is_out_or_gone_and_kept_while_it_refuses() {
        let mut c = Chain::with(3);
        // An earlier swap left hook 1 behind, and Windows has removed it since.
        let mut stale = vec![1];
        c.removed_by_windows.push(1);
        assert_eq!(c.swap(3, &mut stale), (4, Outcome::Installed { old: Old::Removed }));
        assert!(stale.is_empty(), "nothing left to take out");
        // One that still refuses stays for the next swap.
        c.installed.push(9);
        c.refuse.push((9, 5));
        let mut stale = vec![9];
        assert_eq!(c.swap(4, &mut stale), (5, Outcome::Installed { old: Old::Removed }));
        assert_eq!(stale, [9]);
        assert_eq!(c.installed, [5, 9]);
    }

    #[test]
    fn keys_go_unseen_in_front_of_a_higher_window_and_one_whose_level_is_unknown() {
        const MEDIUM: u32 = 0x2000;
        assert!(keys_go_unseen(Some(MEDIUM), Some(0x2010)), "NVDA's menu");
        assert!(keys_go_unseen(Some(MEDIUM), Some(0x3000)), "an elevated window");
        assert!(keys_go_unseen(Some(MEDIUM), None), "a protected process");
        assert!(!keys_go_unseen(Some(MEDIUM), Some(MEDIUM)));
        assert!(!keys_go_unseen(Some(MEDIUM), Some(0x1000)));
        assert!(!keys_go_unseen(None, Some(0x3000)), "our own level unknown: never");
        assert!(!keys_go_unseen(None, None));
    }

    /// A window of this application coming to the front is one whose keys go by unseen, whatever
    /// the levels say, so what the hook recorded as held is forgotten as for NVDA's menu; another
    /// program's ordinary window is not.
    #[test]
    fn keys_go_unseen_in_front_of_a_window_of_this_application_too() {
        const MEDIUM: u32 = 0x2000;
        assert_eq!(front_unseen(Some(MEDIUM), true, Some(MEDIUM)), Some(Unseen::OwnWindow));
        assert_eq!(front_unseen(None, true, None), Some(Unseen::OwnWindow), "levels unread");
        assert_eq!(front_unseen(Some(MEDIUM), false, Some(0x2010)), Some(Unseen::Above(Some(0x2010))));
        assert_eq!(front_unseen(Some(MEDIUM), false, None), Some(Unseen::Above(None)));
        assert_eq!(front_unseen(Some(MEDIUM), false, Some(MEDIUM)), None, "another program's window");
        assert_eq!(front_unseen(None, false, Some(0x3000)), None, "our own level unknown");
        assert_eq!(
            Unseen::OwnWindow.words(),
            "a window the keyboard hook is not called for came to the front (one of this \
             application's own: Windows does not call its hook for keys going to them)"
        );
        assert_eq!(
            Unseen::Above(Some(0x2010)).words(),
            "a window the keyboard hook is not called for came to the front (integrity level \
             0x2010, above this process's)"
        );
        assert!(Unseen::Above(None).words().ends_with("(its integrity level could not be read)"));
    }

    /// Raw input's own report says where a key went: `RIM_INPUT` (0) in the low byte of
    /// `WM_INPUT`'s `wParam` is a key pressed while this process had the foreground. Such a key is
    /// never counted, and the system is not asked about it.
    #[test]
    fn a_key_that_went_to_this_process_is_not_counted_and_nothing_is_asked_about_it() {
        assert!(to_this_process(0), "RIM_INPUT");
        assert!(!to_this_process(1), "RIM_INPUTSINK");
        assert!(to_this_process(0x100), "only the low byte is the code");
        assert!(!counts(true, || panic!("asked the system about a key that went to this process")));
        assert!(counts(false, || true));
        assert!(!counts(false, || false), "a window above ours, or another desktop");
    }

    /// The session of 2026-10-05: the hook called for every key in another program's window, then
    /// Tab pressed again and again in the module manager — keys Windows does not call this
    /// process's hook for — then back. No re-install, however many. The first key there is judged
    /// by the one before it, which the hook saw; the rest are not counted and move no reference.
    /// The same keys reported as gone to another program's window (`RIM_INPUTSINK`), with no call,
    /// are what a removed hook looks like, and install it again after three.
    #[test]
    fn tab_in_this_application_s_own_window_never_installs_the_hook_again() {
        let mut w = Witness::default();
        let (v, last) = run(&mut w, 10_000, 300, 5, true, None);
        assert!(v[1..].iter().all(|v| *v == Verdict::Seen), "{v:?}");
        // Key-downs 1.5 s apart from `start`, the hook last called at `last`, each reported with
        // the input code `code`.
        let keys = |w: &mut Witness, start: u32, n: u32, last: Option<u32>, code: usize| -> Vec<Verdict> {
            (0..n)
                .map(|i| w.key_down(start + i * 1_500, last, || counts(to_this_process(code), || true)))
                .collect()
        };
        let v = keys(&mut w, 20_000, 60, last, 0);
        assert_eq!(v[0], Verdict::Seen, "the hook was called for the key before it");
        assert!(v[1..].iter().all(|v| *v == Verdict::NotCounted), "{v:?}");
        assert_eq!(w.streak, 0);
        assert_eq!(w.prev, Some(20_000), "the keys not counted move no reference");
        // Back in the other program, the hook called for the key: alive.
        assert_eq!(w.key_down(200_000, Some(200_001), || panic!("not asked")), Verdict::Seen);
        // The same keys gone elsewhere, with no call since: the hook is gone, and installed again.
        let v = keys(&mut w, 210_000, MISSES_TO_REHOOK + 1, Some(200_001), 1);
        assert_eq!(v.last(), Some(&Verdict::Rehook { downs: MISSES_TO_REHOOK }), "{v:?}");
    }

    #[test]
    fn a_refused_registration_is_tried_again_at_every_doubling() {
        let tried: Vec<u64> = (1..=100).filter(|&n| retry_at(n)).collect();
        assert_eq!(tried, [1, 2, 4, 8, 16, 32, 64]);
        assert_eq!((1..=1_000_000u64).filter(|&n| retry_at(n)).count(), 20);
    }

    #[test]
    fn the_log_line_names_the_reason_the_old_hook_and_the_chain() {
        let l = line(Reason::Resumed, Outcome::Installed { old: Old::Removed }, None);
        assert!(l.contains("resumed"), "{l}");
        assert!(l.contains("still installed"), "{l}");
        assert!(l.contains("first in the chain"), "{l}");
        assert!(!l.contains("forgotten"), "a hook still installed saw the key-ups: {l}");
        let l = line(Reason::Unlocked, Outcome::Installed { old: Old::AlreadyGone }, None);
        assert!(l.contains("unlocked"), "{l}");
        assert!(l.contains("had already removed"), "{l}");
        assert!(l.contains("was forgotten"), "{l}");
        let l = line(Reason::Resumed, Outcome::Reverted { error: 5 }, None);
        assert!(l.contains("was not installed again"), "{l}");
        assert!(l.contains("UnhookWindowsHookEx error 5"), "{l}");
        let l = line(Reason::Missed { downs: 5 }, Outcome::Installed { old: Old::AlreadyGone }, None);
        assert!(l.contains("the hook stopped seeing keys the system delivered: 5 key-downs"), "{l}");
        let l = line(Reason::Missed { downs: 10 }, Outcome::Failed { error: 8 }, None);
        assert!(l.contains("failed (SetWindowsHookExW error 8)"), "{l}");
        assert!(l.contains("10 key-downs"), "{l}");
    }

    #[test]
    fn a_missed_run_says_where_the_keys_went_and_how_often_the_hook_was_entered() {
        let run = MissedRun {
            foreground: 0,
            front: Some(FrontWindow {
                hwnd: 0x72195a,
                class: "REAPERwnd".to_string(),
                exe: "reaper.exe".to_string(),
                pid: 4321,
                ours: false,
            }),
            entered: 0,
            entered_total: 51_234,
            first_at: 20_000,
            last_call: Some(15_988),
        };
        let l = line(Reason::Missed { downs: 3 }, Outcome::Installed { old: Old::Removed }, Some(&run));
        assert!(
            l.contains(
                "3 key-downs in a row reached raw input and not the hook; this process in the \
                 foreground for 0 of them; window in front 0x72195a class 'REAPERwnd' reaper.exe \
                 pid 4321 (this application: no); hook entered 0 times since the first missed key \
                 (51234 in all), last called 4012 ms before it); the old hook was still installed"
            ),
            "{l}"
        );
        let other = MissedRun { front: None, last_call: None, ..run.clone() };
        assert!(missed_words(&other).contains("; no window in front; "), "{}", missed_words(&other));
        assert!(missed_words(&other).ends_with("(51234 in all), never called"));
        let late = MissedRun { last_call: Some(20_050), ..run.clone() };
        assert!(missed_words(&late).ends_with("last called 50 ms after it"));
        // The run's first key, judged from, can be one that went to this process; and the window
        // in front when the watch asks can be one of its own, the user having switched since.
        let ours = MissedRun {
            foreground: 1,
            front: Some(FrontWindow {
                hwnd: 0x3305ae,
                class: "wxWindowNR".to_string(),
                exe: String::new(),
                pid: 1234,
                ours: true,
            }),
            ..run
        };
        assert!(missed_words(&ours).starts_with("this process in the foreground for 1 of them;"));
        assert!(missed_words(&ours).contains("0x3305ae class 'wxWindowNR' ? pid 1234 (this application: yes)"));
    }
}
