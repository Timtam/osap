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
//! log says so every time. It is first again, too, whenever it comes back after a window of this
//! process (below).
//!
//! **Out of the chain while a window of this process is in front** ([`Place`]). Windows does not
//! only skip the hook for keys going to this process's own windows: it skips the hooks behind it
//! in the chain as well, and Microsoft documents neither. Seen live on 2026-10-10 with a screen
//! reader, whose hook reads out the keys typed into the module manager. Started after the
//! application, its hook ahead of ours, it read them; with only the application started again,
//! ours ahead, it read none there, and every key in other programs' windows. So the watch has the
//! hook taken out of the chain while one of this process's windows is in front — it does nothing
//! there anyway — and installed again while a window of another program is, first in the chain as
//! at start. Raw input stays registered: it was registered in both runs, and the screen reader
//! read the keys in the first, so it cuts nobody's hook off. While the hook is out the witness
//! counts no key, and a re-install for a resume or an unlock waits for the next window of another
//! program, where the hook is installed afresh anyway.
//!
//! Where the hook belongs is decided by the window in front now, never by the window a foreground
//! event names, and compared with where the hook's thread says the hook is, one move at a time
//! ([`Place`]). On 2026-10-11 the events and the window in front parted around a page of ours
//! whose show Windows had declined: an event named the page while another program's window kept
//! the front, and when F7 brought the page to the front no event came, so the hook stayed in the
//! chain there — and a screen reader behind it missed the key-ups of that very combination, and
//! was disturbed from then on. So the watch looks at the window in front at every foreground and
//! focus event, at every answer of the hook's thread, at every key raw input reports from the side
//! the hook does not belong to — key-ups included — and at a resume or an unlock while the hook is
//! out ([`Place::looks_at_key`], [`Place::looks_at_change`]). The captured set and the granted
//! hotkeys stay as they are; what the hook recorded as held is forgotten as it goes out, since it
//! sees no key-up until it is back.
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
    /// A window of another program came to the front after one of this process's own: the hook,
    /// out of the chain while that window was in front, goes back in ([`Place`]).
    Back,
    /// One of this process's own windows came to the front: the hook is taken out of the chain
    /// ([`Place`]). Not a re-install: the hook's thread takes the hook out and installs none
    /// ([`take_out`]).
    Out,
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
            Reason::Back => 6,
            Reason::Out => 7,
        }
    }

    pub(crate) fn decode(w: usize) -> Option<Reason> {
        match w & 0xFF {
            1 => Some(Reason::Resumed),
            2 => Some(Reason::Unlocked),
            3 => Some(Reason::ConsoleConnected),
            4 => Some(Reason::RemoteConnected),
            5 => Some(Reason::Missed { downs: (w >> 8) as u32 }),
            6 => Some(Reason::Back),
            7 => Some(Reason::Out),
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
            Reason::Back => {
                "a window of another program came to the front after one of this application's own"
                    .to_string()
            }
            Reason::Out => "one of this application's own windows came to the front".to_string(),
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
    /// later re-install ([`swap`]'s `stale`). Taking the hook out ([`take_out`]): it stays in the
    /// chain, current, and the next re-install swaps it.
    Failed(u32),
    /// There was no hook of ours in the chain to take out: it was out while one of this
    /// process's windows was in front ([`Place`]), or putting it back after one had failed.
    WasOut,
}

/// What the hook's thread did with a request — a re-install, or taking the hook out of the chain
/// — as it reports it.
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
    /// The hook was asked out of the chain ([`Reason::Out`]); `old` is what became of the one in
    /// it: taken out, removed by Windows already, none there ([`Old::WasOut`]), or still in
    /// ([`Old::Failed`]).
    TakenOut { old: Old },
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
            Outcome::Installed { old: Old::WasOut } => (6, 0),
            Outcome::TakenOut { old: Old::Removed } => (7, 0),
            Outcome::TakenOut { old: Old::AlreadyGone } => (8, 0),
            Outcome::TakenOut { old: Old::Failed(e) } => (9, e),
            Outcome::TakenOut { old: Old::WasOut } => (10, 0),
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
            6 => Some(Outcome::Installed { old: Old::WasOut }),
            7 => Some(Outcome::TakenOut { old: Old::Removed }),
            8 => Some(Outcome::TakenOut { old: Old::AlreadyGone }),
            9 => Some(Outcome::TakenOut { old: Old::Failed(code) }),
            10 => Some(Outcome::TakenOut { old: Old::WasOut }),
            _ => None,
        }
    }

    /// Whether a new hook is in the chain now.
    pub(crate) fn installed(self) -> bool {
        matches!(self, Outcome::Installed { .. })
    }

    /// Whether what the hook remembered of keys going by has to be forgotten: when Windows had
    /// removed the old hook, which then missed their key-ups, and when the hook was taken out of
    /// the chain, which from then on sees no key-up until it is back. A hook still installed saw
    /// every key the new one would have, so its record is right, and forgetting it would drop
    /// a screen reader's modifier held across the swap; a hook that comes back from out of the
    /// chain has recorded nothing since it went.
    pub(crate) fn forget_seen_keys(self) -> bool {
        matches!(
            self,
            Outcome::Installed { old: Old::AlreadyGone }
                | Outcome::TakenOut { old: Old::Removed | Old::AlreadyGone }
        )
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
/// out (`UnhookWindowsHookEx`; `Err` is `GetLastError`). `current` is the hook installed now —
/// `None` while it is out of the chain ([`take_out`]) — and `stale` the hooks of ours an earlier
/// swap could not take out. Returns the hook that is current afterwards, and the outcome.
///
/// **New first, then old out.** The caller handles no message between the calls, so a key
/// arriving meanwhile waits for the new hook, which is first in the chain, and by the time it
/// goes on the old one is out: no key handled twice, none without a hook of ours.
///
/// - `set` fails: nothing is taken out; the old hook stays, and it may still work.
/// - There is no old one: the new one is simply in ([`Old::WasOut`]).
/// - The old one answers `ERROR_INVALID_HOOK_HANDLE`: Windows had removed it.
/// - The old one fails otherwise: it is still in the chain, and two hooks of ours would handle
///   every key the first lets through a second time. So the new one is taken out again and the
///   state is what it was ([`Outcome::Reverted`]). Only if that fails too do both stay; the old
///   one is then kept in `stale` and tried again at every later swap ([`Old::Failed`]).
pub(crate) fn swap<H: Copy>(
    current: Option<H>,
    stale: &mut Vec<H>,
    set: impl FnOnce() -> Result<H, u32>,
    mut unhook: impl FnMut(H) -> Result<(), u32>,
) -> (Option<H>, Outcome) {
    let new = match set() {
        Ok(new) => new,
        Err(error) => return (current, Outcome::Failed { error }),
    };
    let earlier = std::mem::take(stale);
    let (after, outcome) = match current {
        None => (new, Outcome::Installed { old: Old::WasOut }),
        Some(current) => match unhook(current) {
            Ok(()) => (new, Outcome::Installed { old: Old::Removed }),
            Err(ERROR_INVALID_HOOK_HANDLE) => (new, Outcome::Installed { old: Old::AlreadyGone }),
            Err(error) => match unhook(new) {
                Ok(()) | Err(ERROR_INVALID_HOOK_HANDLE) => (current, Outcome::Reverted { error }),
                Err(_) => {
                    stale.push(current);
                    (new, Outcome::Installed { old: Old::Failed(error) })
                }
            },
        },
    };
    take_stale_out(earlier, stale, &mut unhook);
    (Some(after), outcome)
}

/// Takes the hook out of the chain while one of this process's windows is in front
/// ([`Reason::Out`], [`Place`]), with `unhook` passed in as for [`swap`]. `current` is the hook in
/// the chain now, `None` when none is; the hooks in `stale` are tried again here as well. Returns
/// the hook in the chain afterwards — `None`, unless `UnhookWindowsHookEx` refused it
/// ([`Old::Failed`]: it stays current, and the next re-install swaps it) — and the outcome,
/// [`Outcome::TakenOut`].
pub(crate) fn take_out<H: Copy>(
    current: Option<H>,
    stale: &mut Vec<H>,
    mut unhook: impl FnMut(H) -> Result<(), u32>,
) -> (Option<H>, Outcome) {
    let earlier = std::mem::take(stale);
    let (after, old) = match current {
        None => (None, Old::WasOut),
        Some(hook) => match unhook(hook) {
            Ok(()) => (None, Old::Removed),
            Err(ERROR_INVALID_HOOK_HANDLE) => (None, Old::AlreadyGone),
            Err(error) => (Some(hook), Old::Failed(error)),
        },
    };
    take_stale_out(earlier, stale, &mut unhook);
    (after, Outcome::TakenOut { old })
}

/// Hooks an earlier swap could not take out: out now, gone by now, or still refusing — and then
/// back in `stale` for the next try.
fn take_stale_out<H: Copy>(earlier: Vec<H>, stale: &mut Vec<H>, unhook: &mut impl FnMut(H) -> Result<(), u32>) {
    for h in earlier {
        if let Err(e) = unhook(h) {
            if e != ERROR_INVALID_HOOK_HANDLE {
                stale.push(h);
            }
        }
    }
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
            let before = old_words(old);
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
        Outcome::TakenOut { old } => {
            format!("the keyboard hook was taken out of the chain ({why}): {}", old_words(old))
        }
    }
}

/// What became of the old hook, as the lines say it.
fn old_words(old: Old) -> String {
    match old {
        Old::Removed => "the old hook was still installed and has been taken out".to_string(),
        Old::AlreadyGone => "Windows had already removed the old hook — it had stopped being \
                             called, so captured keys and the hotkeys the hook matches were not \
                             working until now"
            .to_string(),
        Old::Failed(e) => format!(
            "taking the old hook out failed (UnhookWindowsHookEx error {e}), and so did taking the \
             new one out again: both are in the chain, and a key the new one lets through reaches \
             the old one too and is handled a second time, until the old one can be taken out — \
             tried again at every re-install"
        ),
        Old::WasOut => "no hook of ours was in the chain: it had been taken out while one of our \
                        windows was in front, and putting it back had failed"
            .to_string(),
    }
}

/// Where the hook's thread left the hook: how many of the watch's requests it has handled — moves
/// and re-installs alike, in the order they were posted, counted with wrapping — and whether a
/// hook of ours is in the chain after them. The hook's thread writes it at its first install
/// ([`HookAt::START`]) and after every request, before it answers; the watch reads it before it
/// asks for a move ([`Place::converge`]). So where the hook is never depends on an answer that
/// arrived: a reply lost to a full queue costs its line, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HookAt {
    pub(crate) handled: u32,
    pub(crate) in_chain: bool,
}

impl HookAt {
    /// The hook as its thread installs it at start, before any request.
    pub(crate) const START: HookAt = HookAt { handled: 0, in_chain: true };

    /// Packed into one word for an atomic: the count above, the hook's place in the lowest bit.
    pub(crate) const fn encode(self) -> u64 {
        ((self.handled as u64) << 1) | self.in_chain as u64
    }

    pub(crate) fn decode(w: u64) -> HookAt {
        HookAt { handled: (w >> 1) as u32, in_chain: w & 1 == 1 }
    }

    /// After one more request, which left a hook of ours in the chain or not (`in_chain`).
    pub(crate) fn after(self, in_chain: bool) -> HookAt {
        HookAt { handled: self.handled.wrapping_add(1), in_chain }
    }
}

/// Whether the window in front belongs to this process (`own`) as far as the hook goes: the
/// window's own process (`window`) is this one, or the process of its root owner (`root_owner`,
/// `GetAncestor` with `GA_ROOTOWNER`: the top of its chain of parents and owners) is. A page's
/// WebView2 content is drawn by `msedgewebview2.exe` in a child window of the page's frame, and a
/// popup of it — a select list's — is a window of that process owned by the frame: both are this
/// application's as the keys go. `None` is a process that could not be read, a window gone.
pub(crate) fn front_is_ours(own: u32, window: Option<u32>, root_owner: Option<u32>) -> bool {
    window == Some(own) || root_owner == Some(own)
}

/// Where the hook belongs and what the watch has asked of the hook's thread to get it there: out
/// of the chain of low-level keyboard hooks while one of this process's own windows is in front,
/// in it otherwise. See the top of this file for why.
///
/// **Level, not edges.** The watch does not move the hook by the window a foreground event names:
/// around a show Windows declined, one named a page of ours that did not keep the front, and when
/// that page did come to the front a few seconds later, no foreground event reached the watch at
/// all (2026-10-11, TODO.md). It compares where the hook belongs — by the window in front now,
/// `GetForegroundWindow` — with where the hook's thread says the hook is ([`HookAt`]), and asks
/// for the move that brings the two together ([`Place::converge`]), at every foreground and focus
/// event, every answer of the hook's thread, and every key raw input reports from the other side
/// ([`Place::looks_at_key`]). One move at a time: while a request is on its way no move is
/// asked, and its answer looks again, so a switch made meanwhile is caught there and no stale move
/// is queued behind it. A re-install the witness or a resume asks for may be posted beside a move
/// on its way; the look waits for the answers to all of them. No clock decides anything.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Place {
    /// One of this process's own windows was in front the last time the watch looked: the hook
    /// belongs out of the chain.
    out: bool,
    /// The requests posted to the hook's thread, moves and re-installs alike, counted with
    /// wrapping: one is on its way while [`HookAt::handled`] has not reached this.
    posted: u32,
    /// The last request posted, as the move it is: a re-install puts the hook in the chain as
    /// [`Move::In`] does. Once it is handled and the hook is still not where it would have put
    /// it, the hook's thread could not make it, and it is not asked for again — at every key, at
    /// every focus change — until the window in front changes sides or a window comes to the front
    /// ([`Place::foreground_event`]).
    last: Option<Move>,
    /// The last look found nothing to do: the hook is where it belongs, or where a move that could
    /// not be made left it. Until then every key looks ([`Place::looks_at_key`]).
    settled: bool,
}

/// What the watch asks the hook's thread for as a window comes to the front.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Move {
    /// Take the hook out of the chain: one of this process's own windows is in front.
    Out,
    /// Install it again, first in the chain: a window of another program is in front.
    In,
}

impl Move {
    /// The reason the request carries to the hook's thread and back.
    pub(crate) fn reason(self) -> Reason {
        match self {
            Move::Out => Reason::Out,
            Move::In => Reason::Back,
        }
    }
}

impl Place {
    /// The window in front now is one of this process's own (`ours`, [`front_is_ours`]) or another
    /// program's, and the hook's thread left the hook as `at` says: the move that brings the hook
    /// where it belongs, if one is needed, is handed to `post`, which asks the hook's thread and
    /// says whether it could. Nothing while a request is on its way — its answer looks again — and
    /// nothing for a move the hook's thread has just failed to make (see `last`) until the
    /// window in front changes sides or a window comes to the front. A move that could not be
    /// posted is asked for again at the next look. Windows of this process one after another, or
    /// of other programs one after another, move nothing: the hook is where they want it already.
    pub(crate) fn converge(&mut self, ours: bool, at: HookAt, post: impl FnOnce(Move) -> bool) -> Option<Move> {
        if ours != self.out {
            self.out = ours;
            self.last = None;
        }
        self.settled = false;
        if at.handled != self.posted {
            return None;
        }
        let wanted = match (ours, at.in_chain) {
            (true, true) => Move::Out,
            (false, false) => Move::In,
            _ => {
                self.settled = true;
                return None;
            }
        };
        if self.last == Some(wanted) {
            self.settled = true;
            return None;
        }
        if !post(wanted) {
            return None;
        }
        self.posted = self.posted.wrapping_add(1);
        self.last = Some(wanted);
        Some(wanted)
    }

    /// A re-install was posted to the hook's thread: counted with the moves, since that thread
    /// handles them all in order. It puts the hook in the chain as a move back does, so one that
    /// fails is not followed at its answer by a move back that would fail the same way: the
    /// witness, whose bar the failure raised, asks again after the key-downs the hook misses, and
    /// the next window that comes to the front does. Posted only while the hook belongs in the
    /// chain: the witness judges no key while it belongs out, and a resume or an unlock then asks
    /// for none ([`Place::key_down`], [`Place::change`]).
    pub(crate) fn posted(&mut self) {
        self.posted = self.posted.wrapping_add(1);
        self.last = Some(Move::In);
    }

    /// A window came to the front (`EVENT_SYSTEM_FOREGROUND`), whichever it is: a move the hook's
    /// thread could not make is asked for again at the look that follows. So a hook that could not
    /// be put back is tried again at the next window of another program the user goes to, and not
    /// only once the witness has counted the key-downs it misses there. A focus change or a key
    /// does not ask again: they come many to a window.
    pub(crate) fn foreground_event(&mut self) {
        self.last = None;
    }

    /// Whether one of this process's own windows was in front at the last look: the hook belongs
    /// out of the chain.
    #[cfg(test)]
    fn is_out(self) -> bool {
        self.out
    }

    /// A physical key-down for the witness ([`Witness::key_down`]), judged only while the hook
    /// belongs in the chain. While one of this process's windows is in front no key is the hook's
    /// to have seen, so none is counted, none moves the witness's reference, and nothing is asked
    /// about it: `None`. Raw input still reports them — keys going to other programs too, in the
    /// moment before the window in front is another program's; once it is, the key makes the watch
    /// put the hook back first ([`Place::looks_at_key`]) and is the first of the fresh count. A hook
    /// that belongs in the chain and could not be put back is judged as a removed one is: the
    /// witness installs it again after the key-downs it misses.
    pub(crate) fn key_down(
        self,
        witness: &mut Witness,
        time: u32,
        last_call: Option<u32>,
        visible: impl FnOnce() -> bool,
    ) -> Option<Verdict> {
        (!self.out).then(|| witness.key_down(time, last_call, visible))
    }

    /// What a session change or a suspend/resume ([`on_session_change`], [`on_power`]) asks of the
    /// hook here. While it belongs out, a re-install becomes a fresh count only: installing the
    /// hook now would put it back in the chain in front of one of this process's own windows, and
    /// it is installed afresh, first in the chain, as soon as a window of another program is in
    /// front.
    pub(crate) fn change(self, change: Change) -> Change {
        match change {
            Change::Rehook(_) if self.out => Change::Restart,
            other => other,
        }
    }

    /// Whether a physical key event — a key-down or a key-up ([`physical_key`]) — makes the watch
    /// look at the window in front again ([`Place::converge`]). Raw input says with each key which
    /// side had the front when it was pressed or let go: this process (`to_this_process`,
    /// `RIM_INPUT`) or another. A key from the side the hook does not belong to means the window
    /// in front changed without the watch looking — a foreground event Windows did not send, or
    /// one the watch never got — and is looked at; so is every key while the last look left
    /// something to do (a request on its way, a move that could not be posted). The key-ups count:
    /// the keys of the combination that brings one of this process's windows to the front go up
    /// there, and a screen reader whose hook is behind ours misses each of them until the hook is
    /// out. Looking costs a few queries, and asks the hook's thread for nothing when the hook is
    /// where it belongs.
    pub(crate) fn looks_at_key(self, to_this_process: bool) -> bool {
        to_this_process != self.out || !self.settled
    }

    /// Whether a session change or a suspend/resume makes the watch look at the window in front
    /// again: one that asks for a re-install ([`Change::Rehook`]: a resume, an unlock, a connect)
    /// while the hook belongs out. The keyboard comes back then from where no foreground event
    /// reaches the watch, and a window of another program may be in front by now. While it belongs
    /// in, the re-install is asked for, and its answer looks.
    pub(crate) fn looks_at_change(self, change: Change) -> bool {
        self.out && matches!(change, Change::Rehook(_))
    }
}

/// A window that came to the front, as the lines of [`move_line`] name it: its title in quotes,
/// or its class when it has none, and for another program's window the program (`exe`, empty
/// when it could not be read) — `'Modules'`, `'REAPER v7.22' (reaper.exe)`. A window with neither
/// a title nor a class to read is gone by now: `a window that is gone`.
pub(crate) fn window_words(title: &str, class: &str, exe: Option<&str>) -> String {
    let name = match (title.is_empty(), class.is_empty()) {
        (false, _) => format!("'{title}'"),
        (true, false) => format!("untitled, class '{class}'"),
        (true, true) => "a window that is gone".to_string(),
    };
    match exe {
        Some(exe) => format!("{name} ({})", if exe.is_empty() { "?" } else { exe }),
        None => name,
    }
}

/// What a move's line says after the window that came to the front ([`move_line`]): for a move
/// out, the window of another program that was in front before it (`before`, [`window_words`]);
/// for a move back, how long this process's windows were in front (`ours_for_ms`, from the move
/// out to this one). Empty when there is nothing to say: the hook went out with no other program's
/// window seen before, at the start.
pub(crate) fn move_after(reason: Reason, before: Option<&str>, ours_for_ms: Option<u32>) -> String {
    match (reason, before, ours_for_ms) {
        (Reason::Out, Some(before), _) => format!(", after {before}"),
        (Reason::Back, _, Some(ms)) => {
            format!(", after {} with our windows in front", crate::modal_spans::seconds(u64::from(ms)))
        }
        _ => String::new(),
    }
}

/// The one line a move of the hook writes ([`Reason::Out`], [`Reason::Back`]), once its outcome
/// is in; `window` names the window that came to the front ([`window_words`]), and `after` is what
/// [`move_after`] says about the windows before it. Quiet: a few words for each change, more only
/// when something went wrong.
pub(crate) fn move_line(reason: Reason, outcome: Outcome, window: &str, after: &str) -> String {
    match (reason, outcome) {
        (Reason::Out, Outcome::TakenOut { old: Old::Removed | Old::WasOut }) => {
            format!("the keyboard hook is out while one of our windows is in front ({window}){after}")
        }
        (Reason::Out, Outcome::TakenOut { old: Old::AlreadyGone }) => format!(
            "the keyboard hook is out while one of our windows is in front ({window}){after}; \
             Windows had already removed it — it had stopped being called"
        ),
        (Reason::Out, Outcome::TakenOut { old: Old::Failed(e) }) => format!(
            "the keyboard hook could not be taken out while one of our windows is in front \
             ({window}){after}: UnhookWindowsHookEx error {e}, so it stays in the chain while our \
             windows are in front; it is asked out again at the next window that comes to the front"
        ),
        (Reason::Back, Outcome::Installed { old: Old::WasOut | Old::Removed }) => {
            format!("the keyboard hook is back, first in the chain ({window} in front){after}")
        }
        (Reason::Back, Outcome::Installed { old }) => format!(
            "the keyboard hook is back, first in the chain ({window} in front){after}; {}",
            old_words(old)
        ),
        (Reason::Back, Outcome::Failed { error }) => format!(
            "the keyboard hook could not be put back ({window} in front){after}: SetWindowsHookExW \
             error {error}. Captured keys and the hotkeys the hook matches do not work until it is \
             (RegisterHotKey still delivers hotkeys); tried again when it misses key-downs, and \
             at the next window that comes to the front"
        ),
        (Reason::Back, Outcome::Reverted { error }) => format!(
            "the keyboard hook stays where it was ({window} in front){after}: it could not be taken \
             out while one of our windows was in front, and now the new one went in but the old \
             one could not be taken out (UnhookWindowsHookEx error {error}), so the new one was \
             taken out again rather than leave two hooks of ours handling every key twice"
        ),
        // Not a pair the hook's thread answers with; said as a re-install would be.
        _ => line(reason, outcome, None),
    }
}

/// How late a key-down may reach the hook before the watch writes a line about it
/// ([`late_key_said`]).
pub(crate) const LATE_KEY_MS: u32 = 250;

/// Whether a key-down that reached the hook `late` ms after it was pressed (as
/// `hotkey_hook::lateness` measures it: 0 for an injected one, whose stamp is its sender's) is
/// said in the log ([`late_key_line`]): at [`LATE_KEY_MS`] or more. Key-ups are not said; their
/// key-down was.
pub(crate) fn late_key_said(is_down: bool, late: u32) -> bool {
    is_down && late >= LATE_KEY_MS
}

/// The line for key-downs that reached the hook [`LATE_KEY_MS`] or more after they were pressed
/// since the line before ([`LateKeys`]): the one most late of them (`late` ms, virtual key `vk`),
/// with the window that was in front at the first (`front`, [`window_words`]), and how many more
/// there were (`more`). The hook cannot tell what held them: a hook ahead of ours in the chain is
/// called first and keeps every hook after it waiting, and a machine too busy to run our hook's
/// thread does the same.
pub(crate) fn late_key_line(late: u32, vk: u32, front: &str, more: u32) -> String {
    let mut text = format!(
        "a key reached our keyboard hook {late} ms after it was pressed (vk {vk:#04x}, {front} in \
         front): it was held that long before our hook was called — by a hook ahead of ours in \
         the chain, or by a machine too busy to run our hook's thread"
    );
    if more > 0 {
        text.push_str(&format!(
            "; {more} more key-down(s) came {LATE_KEY_MS} ms or more late since the line before \
             (this one names the most late; a line at most once a minute)"
        ));
    }
    text
}

/// The least time between two lines about late key-downs ([`LateKeys`]).
pub(crate) const LATE_LINE_EVERY_MS: u32 = 60_000;

/// Key-downs that reached the hook late, as the watch hears of them: how many (`count`), the one
/// most late of them (`late` ms, virtual key `vk`), and the window in front at the first, as its
/// line names it (`front`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LateBatch {
    pub(crate) count: u32,
    pub(crate) late: u32,
    pub(crate) vk: u32,
    pub(crate) front: String,
}

impl LateBatch {
    /// This batch and `later` as one: both counts, the most late of the two, the first's window.
    fn and(self, later: LateBatch) -> LateBatch {
        let (late, vk) = if later.late > self.late { (later.late, later.vk) } else { (self.late, self.vk) };
        LateBatch { count: self.count.saturating_add(later.count), late, vk, front: self.front }
    }
}

/// What the watch does with late key-downs that came ([`LateKeys::came`]).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LateSay {
    /// Write their line now ([`late_key_line`], `count - 1` more).
    Now(LateBatch),
    /// Keep them for a line once a minute has passed since the last; `Some(ms)` for the first kept
    /// since then: a timer of that many milliseconds is to call [`LateKeys::due`].
    Kept(Option<u32>),
}

/// How the lines about late key-downs are spaced, so that keys late one after another — a key held
/// with auto-repeat while a hook ahead of ours is slow, a busy machine — do not write a line
/// each: the first after [`LATE_LINE_EVERY_MS`] without a line is said at once, and the ones after
/// it are counted and said in one line when that time has passed since it, then again a minute
/// later, as long as they come. Ticks are `GetTickCount`'s, wrapping.
#[derive(Debug, Default)]
pub(crate) struct LateKeys {
    /// When the last line was written.
    last_line: Option<u32>,
    /// The key-downs since it, not said yet.
    kept: Option<LateBatch>,
}

impl LateKeys {
    /// Late key-downs came (`batch`) at `now`: said at once, with any kept, when no line was
    /// written in the last [`LATE_LINE_EVERY_MS`]; else kept until then.
    pub(crate) fn came(&mut self, now: u32, batch: LateBatch) -> LateSay {
        let first_kept = self.kept.is_none();
        let all = match self.kept.take() {
            Some(kept) => kept.and(batch),
            None => batch,
        };
        let since = self.last_line.map(|at| now.wrapping_sub(at));
        match since {
            Some(since) if since < LATE_LINE_EVERY_MS => {
                self.kept = Some(all);
                LateSay::Kept(first_kept.then_some(LATE_LINE_EVERY_MS - since))
            }
            _ => {
                self.last_line = Some(now);
                LateSay::Now(all)
            }
        }
    }

    /// The timer [`LateKeys::came`] asked for ran out, at `now`: the key-downs kept since the last
    /// line, for a line now, if any are.
    pub(crate) fn due(&mut self, now: u32) -> Option<LateBatch> {
        let kept = self.kept.take()?;
        self.last_line = Some(now);
        Some(kept)
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
    physical_key(has_device, vkey, make_code) && flags & RI_KEY_BREAK == 0
}

/// Whether a raw keyboard report is a physical key event, a key-down or a key-up: from a device,
/// and a real key rather than half of an escape sequence. What makes the watch look at the window
/// in front ([`Place::looks_at_key`]); only the key-downs among them are counted
/// ([`physical_down`]).
pub(crate) fn physical_key(has_device: bool, vkey: u16, make_code: u16) -> bool {
    has_device && vkey != NO_KEY && vkey != 0 && make_code != NO_KEY
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
            Reason::Back,
            Reason::Out,
        ] {
            assert_eq!(Reason::decode(r.encode()), Some(r));
        }
        assert_eq!(Reason::decode(0), None);
        for o in [
            Outcome::Installed { old: Old::Removed },
            Outcome::Installed { old: Old::AlreadyGone },
            Outcome::Installed { old: Old::Failed(5) },
            Outcome::Installed { old: Old::WasOut },
            Outcome::Failed { error: 1428 },
            Outcome::Reverted { error: 5 },
            Outcome::TakenOut { old: Old::Removed },
            Outcome::TakenOut { old: Old::AlreadyGone },
            Outcome::TakenOut { old: Old::Failed(5) },
            Outcome::TakenOut { old: Old::WasOut },
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
        /// `SetWindowsHookExW`, as this chain answers it.
        fn set(&mut self) -> Result<u32, u32> {
            self.calls.push("set".to_string());
            if let Some(e) = self.set_fails {
                return Err(e);
            }
            let h = self.next;
            self.next += 1;
            self.installed.insert(0, h); // newest first, as Windows calls them
            Ok(h)
        }
        /// `UnhookWindowsHookEx`, as this chain answers it.
        fn unhook(&mut self, h: u32) -> Result<(), u32> {
            self.calls.push(format!("unhook {h}"));
            if let Some(&(_, e)) = self.refuse.iter().find(|(r, _)| *r == h) {
                return Err(e);
            }
            if self.removed_by_windows.contains(&h) || !self.installed.contains(&h) {
                return Err(ERROR_INVALID_HOOK_HANDLE);
            }
            self.installed.retain(|&x| x != h);
            Ok(())
        }
        fn swap(&mut self, current: u32, stale: &mut Vec<u32>) -> (u32, Outcome) {
            let (after, outcome) = self.swap_from(Some(current), stale);
            (after.expect("a swap with a hook in the chain leaves one in it"), outcome)
        }
        fn swap_from(&mut self, current: Option<u32>, stale: &mut Vec<u32>) -> (Option<u32>, Outcome) {
            let this = std::cell::RefCell::new(self);
            swap(current, stale, || this.borrow_mut().set(), |h| this.borrow_mut().unhook(h))
        }
        fn take_out(&mut self, current: Option<u32>, stale: &mut Vec<u32>) -> (Option<u32>, Outcome) {
            take_out(current, stale, |h| self.unhook(h))
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

    /// The module manager comes to the front: nothing of ours is left in the chain, ahead of the
    /// hook another program installed before ours, and what the hook recorded as held is
    /// forgotten. Another program's window comes to the front: the hook is installed, nothing is
    /// taken out on the way, and it is first in the chain again — ahead of a hook installed while
    /// it was out as well, as when the application starts — with nothing forgotten, since nothing
    /// was recorded while it was out.
    #[test]
    fn out_of_the_chain_for_our_windows_and_back_first_in_it() {
        let mut c = Chain::with(1);
        c.installed.push(90); // installed before ours, so behind it
        let mut stale = Vec::new();
        let (now, outcome) = c.take_out(Some(1), &mut stale);
        assert_eq!((now, outcome), (None, Outcome::TakenOut { old: Old::Removed }));
        assert_eq!(c.installed, [90], "nothing of ours ahead of it");
        assert!(outcome.forget_seen_keys(), "out of the chain, the hook sees no key-up");
        // A program that installs a hook meanwhile is first, for now.
        c.installed.insert(0, 91);
        let (now, outcome) = c.swap_from(now, &mut stale);
        assert_eq!((now, outcome), (Some(2), Outcome::Installed { old: Old::WasOut }));
        assert_eq!(c.installed, [2, 91, 90], "first in the chain, as at start");
        assert_eq!(c.calls, ["unhook 1", "set"], "nothing taken out on the way back");
        assert!(!outcome.forget_seen_keys(), "nothing was recorded while it was out");
        assert!(outcome.installed());
    }

    #[test]
    fn taking_out_a_hook_windows_removed_one_that_refuses_and_none() {
        // Removed by Windows already: out all the same, and its record forgotten.
        let mut c = Chain::with(1);
        c.installed.clear();
        c.removed_by_windows.push(1);
        let (now, outcome) = c.take_out(Some(1), &mut Vec::new());
        assert_eq!((now, outcome), (None, Outcome::TakenOut { old: Old::AlreadyGone }));
        assert!(outcome.forget_seen_keys());
        // Refused: it stays in the chain and current, its record stays right, and the way back
        // swaps it — new first, then old out.
        let mut c = Chain::with(1);
        c.refuse.push((1, 5));
        let mut stale = Vec::new();
        let (now, outcome) = c.take_out(Some(1), &mut stale);
        assert_eq!((now, outcome), (Some(1), Outcome::TakenOut { old: Old::Failed(5) }));
        assert!(!outcome.forget_seen_keys(), "still in the chain: what it recorded is right");
        assert!(stale.is_empty(), "current, not stale");
        c.refuse.clear();
        assert_eq!(c.swap_from(now, &mut stale), (Some(2), Outcome::Installed { old: Old::Removed }));
        assert_eq!(c.installed, [2]);
        // None in the chain — a way back that failed — and one left over from an earlier swap:
        // that one is tried, and nothing else.
        let mut c = Chain::default();
        c.installed.push(7);
        let mut stale = vec![7];
        assert_eq!(c.take_out(None, &mut stale), (None, Outcome::TakenOut { old: Old::WasOut }));
        assert_eq!(c.calls, ["unhook 7"]);
        assert!(stale.is_empty() && c.installed.is_empty());
        // A way back that fails leaves the hook out; the next one installs it.
        let mut c = Chain { set_fails: Some(8), ..Chain::default() };
        let (now, outcome) = c.swap_from(None, &mut Vec::new());
        assert_eq!((now, outcome), (None, Outcome::Failed { error: 8 }));
        c.set_fails = None;
        assert_eq!(c.swap_from(now, &mut Vec::new()).1, Outcome::Installed { old: Old::WasOut });
    }

    /// What the watch posts to the hook's thread.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Request {
        Move(Move),
        Reinstall,
    }

    /// The hook's thread as the watch sees it, for the tests of [`Place`]: the requests posted to
    /// it, handled one at a time in order when a test says so, and where each left the hook — as
    /// [`take_out`] and [`swap`] leave it, the system calls refusing when a test says so.
    struct HookThread {
        at: HookAt,
        queue: std::collections::VecDeque<Request>,
        /// The most requests that were ever waiting at once.
        most_waiting: usize,
        /// `SetWindowsHookExW` refuses.
        set_fails: bool,
        /// `UnhookWindowsHookEx` refuses.
        unhook_fails: bool,
    }

    impl HookThread {
        fn new() -> HookThread {
            HookThread {
                at: HookAt::START,
                queue: Default::default(),
                most_waiting: 0,
                set_fails: false,
                unhook_fails: false,
            }
        }

        /// A window came to the front — one of this process's (`ours`) or not — and the watch
        /// looks, as at a foreground event.
        fn foreground(&mut self, place: &mut Place, ours: bool) -> Option<Move> {
            place.foreground_event();
            self.look(place, ours)
        }

        /// The watch looks at the window in front — one of this process's (`ours`) or not — and
        /// posts what it asks for: as at a focus change or a key.
        fn look(&mut self, place: &mut Place, ours: bool) -> Option<Move> {
            let queue = &mut self.queue;
            let m = place.converge(ours, self.at, |m| {
                queue.push_back(Request::Move(m));
                true
            });
            self.most_waiting = self.most_waiting.max(self.queue.len());
            m
        }

        /// The witness asks for a re-install.
        fn reinstall(&mut self, place: &mut Place) {
            place.posted();
            self.queue.push_back(Request::Reinstall);
            self.most_waiting = self.most_waiting.max(self.queue.len());
        }

        /// The hook's thread handles the oldest request and answers, and the watch looks again at
        /// the answer, with the window in front then (`ours`).
        fn answer(&mut self, place: &mut Place, ours: bool) -> Option<Move> {
            let r = self.queue.pop_front().expect("a request to handle");
            let in_chain = match r {
                // Taken out, unless it refuses; then it stays where it was.
                Request::Move(Move::Out) => self.unhook_fails && self.at.in_chain,
                // A new one in, unless that is refused; then the old one stays where it was.
                Request::Move(Move::In) | Request::Reinstall => !self.set_fails || self.at.in_chain,
            };
            self.at = self.at.after(in_chain);
            self.look(place, ours)
        }
    }

    /// The module manager, a module's dialog over it, the manager again: out once. Another
    /// program's window, then another: in once. Each answer looks again and finds the hook where
    /// it belongs.
    #[test]
    fn out_at_the_first_of_our_windows_and_in_once_at_another_program_s() {
        let mut t = HookThread::new();
        let mut p = Place::default();
        assert_eq!(t.look(&mut p, false), None, "another program's: the hook stays in");
        assert_eq!(t.look(&mut p, true), Some(Move::Out));
        assert!(p.is_out());
        assert_eq!(t.answer(&mut p, true), None);
        assert!(!t.at.in_chain);
        assert_eq!(t.look(&mut p, true), None, "a module's dialog");
        assert_eq!(t.look(&mut p, true), None, "the module manager again");
        assert_eq!(t.look(&mut p, false), Some(Move::In));
        assert!(!p.is_out());
        assert_eq!(t.answer(&mut p, false), None);
        assert!(t.at.in_chain);
        assert_eq!(t.look(&mut p, false), None, "another program's window after it");
        assert_eq!(Move::Out.reason(), Reason::Out);
        assert_eq!(Move::In.reason(), Reason::Back);
    }

    /// The session of 2026-10-11 (the HTML probe, REAPER's FX window): the hook was out while the
    /// page was taken to be in front, and the first key of Ctrl+Alt+Shift+Win+F7, pressed in
    /// REAPER's FX window, brought it back. F7 then brought the page to the front. With the move
    /// back still on its way nothing more is asked, and its answer looks again: out, with the page
    /// in front. One request waits at a time.
    #[test]
    fn back_in_on_its_way_then_one_of_our_windows_ends_out() {
        let mut t = HookThread::new();
        let mut p = Place::default();
        assert_eq!(t.look(&mut p, true), Some(Move::Out));
        assert_eq!(t.answer(&mut p, true), None);
        // The FX window in front at the first key: back in.
        assert_eq!(t.look(&mut p, false), Some(Move::In));
        // The page comes to the front before the hook's thread has answered.
        assert_eq!(t.look(&mut p, true), None, "nothing more while the move back is on its way");
        assert_eq!(t.answer(&mut p, true), Some(Move::Out), "its answer looks again");
        assert_eq!(t.answer(&mut p, true), None);
        assert!(!t.at.in_chain, "out, with the page in front");
        assert_eq!(t.most_waiting, 1);
    }

    /// The same session, as it went: the move back answered while the FX window was still in
    /// front, then the page in front with no foreground event at all. The keys of the combination
    /// go up in the page, and raw input reports each of them from this process's side: the first
    /// key-up looks, and the hook goes out — before the modifiers' key-ups, which a screen reader
    /// whose hook is behind ours would otherwise miss and go on taking as held.
    #[test]
    fn our_window_in_front_with_no_foreground_event_is_found_at_the_first_key_up_there() {
        let mut t = HookThread::new();
        let mut p = Place::default();
        t.look(&mut p, true);
        t.answer(&mut p, true);
        assert_eq!(t.look(&mut p, false), Some(Move::In));
        assert_eq!(t.answer(&mut p, false), None, "the FX window still in front");
        assert!(t.at.in_chain);
        // F7 goes up, then Shift, Win, Ctrl, Alt: key-ups (RI_KEY_BREAK), from a keyboard.
        assert!(physical_key(true, 0x76, 0x41), "F7's key-up is a physical key event");
        assert!(!physical_down(true, 1, 0x76, 0x41), "but not a key-down");
        assert!(!p.looks_at_key(false), "a key from the FX window's side: nothing to look at");
        assert!(p.looks_at_key(true), "a key from this process's side: looked at");
        assert_eq!(t.look(&mut p, true), Some(Move::Out));
        assert!(p.looks_at_key(true), "every key looks while the move is on its way");
        assert_eq!(t.answer(&mut p, true), None);
        assert!(!t.at.in_chain);
        assert!(!p.looks_at_key(true), "out: the keys in the page look no more");
    }

    /// The other way round: the move out on its way, and a window of another program in front
    /// before it is answered. The answer looks again: back in.
    #[test]
    fn out_on_its_way_then_another_program_s_window_ends_in() {
        let mut t = HookThread::new();
        let mut p = Place::default();
        assert_eq!(t.look(&mut p, true), Some(Move::Out));
        assert_eq!(t.look(&mut p, false), None, "nothing more while the move out is on its way");
        assert_eq!(t.answer(&mut p, false), Some(Move::In));
        assert_eq!(t.answer(&mut p, false), None);
        assert!(t.at.in_chain);
        assert_eq!(t.most_waiting, 1);
    }

    /// Every order of up to eight steps — a window of ours in front, another program's, the hook's
    /// thread answering — ends with the hook where the last window in front wants it once the
    /// answers are in, and never with more than one request waiting.
    #[test]
    fn rapid_alternation_ends_where_the_last_window_in_front_wants_the_hook() {
        for n in 0..=8u32 {
            for code in 0..3u32.pow(n) {
                let mut t = HookThread::new();
                let mut p = Place::default();
                // The look as the watch is armed, another program's window in front.
                assert_eq!(t.look(&mut p, false), None);
                let mut ours = false;
                let mut c = code;
                for _ in 0..n {
                    match c % 3 {
                        0 | 1 => {
                            ours = c % 3 == 0;
                            t.look(&mut p, ours);
                        }
                        _ => {
                            if !t.queue.is_empty() {
                                t.answer(&mut p, ours);
                            }
                        }
                    }
                    c /= 3;
                }
                while !t.queue.is_empty() {
                    t.answer(&mut p, ours);
                }
                assert_eq!(t.at.in_chain, !ours, "steps {code} of {n}");
                assert!(t.most_waiting <= 1, "steps {code} of {n}: {} waiting", t.most_waiting);
                assert!(!p.looks_at_key(ours), "steps {code} of {n}: settled");
            }
        }
    }

    /// A move the hook's thread could not make is not asked for again at every focus change or key
    /// — nor does every key look — until the window in front changes sides (or a window comes to
    /// the front: the next test). `UnhookWindowsHookEx` refusing leaves the hook in the chain while
    /// our windows are in front; `SetWindowsHookExW` refusing leaves it out in front of another
    /// program, where the witness counts the keys it misses and its re-install puts it back.
    #[test]
    fn a_move_that_could_not_be_made_is_asked_for_again_only_once_the_window_changes_sides() {
        let mut t = HookThread::new();
        let mut p = Place::default();
        t.unhook_fails = true;
        assert_eq!(t.look(&mut p, true), Some(Move::Out));
        assert_eq!(t.answer(&mut p, true), None, "not again at its own answer");
        assert!(t.at.in_chain);
        assert_eq!(t.look(&mut p, true), None, "nor at the next look");
        assert!(!p.looks_at_key(true), "nor does a key in our window look");
        assert_eq!(t.look(&mut p, false), None, "another program's: the hook is where it belongs");
        t.unhook_fails = false;
        assert_eq!(t.look(&mut p, true), Some(Move::Out), "our window again: asked again");
        t.answer(&mut p, true);
        assert!(!t.at.in_chain);
        // The way back refused.
        t.set_fails = true;
        assert_eq!(t.look(&mut p, false), Some(Move::In));
        assert_eq!(t.answer(&mut p, false), None);
        assert!(!t.at.in_chain);
        assert_eq!(t.look(&mut p, false), None);
        assert!(!p.looks_at_key(false));
        let mut w = Witness::default();
        let judged: Vec<_> = (0..=MISSES_TO_REHOOK)
            .map(|i| p.key_down(&mut w, 20_000 + i * 1_500, Some(1_000), || true))
            .collect();
        assert_eq!(judged.last(), Some(&Some(Verdict::Rehook { downs: MISSES_TO_REHOOK })), "{judged:?}");
        t.set_fails = false;
        t.reinstall(&mut p);
        assert_eq!(t.look(&mut p, false), None, "the re-install is on its way");
        assert_eq!(t.answer(&mut p, false), None);
        assert!(t.at.in_chain, "back in");
    }

    /// The hook could not be put back in front of REAPER, and the user goes to another program's
    /// window: that window coming to the front asks again, and the hook is back — not only once
    /// the witness has counted the key-downs it misses there. A focus change or a key in REAPER
    /// asks nothing. A hook that could not be taken out is asked out again at the next window that
    /// comes to the front the same way: a module's dialog over the module manager.
    #[test]
    fn a_move_that_could_not_be_made_is_asked_for_again_when_a_window_comes_to_the_front() {
        let mut t = HookThread::new();
        let mut p = Place::default();
        assert_eq!(t.foreground(&mut p, true), Some(Move::Out));
        t.answer(&mut p, true);
        t.set_fails = true;
        assert_eq!(t.foreground(&mut p, false), Some(Move::In), "REAPER");
        assert_eq!(t.answer(&mut p, false), None);
        assert!(!t.at.in_chain);
        assert_eq!(t.look(&mut p, false), None, "a focus change in REAPER");
        assert!(!p.looks_at_key(false), "nor does a key there look");
        t.set_fails = false;
        assert_eq!(t.foreground(&mut p, false), Some(Move::In), "another program's window");
        assert_eq!(t.answer(&mut p, false), None);
        assert!(t.at.in_chain);
        assert_eq!(t.foreground(&mut p, false), None, "in already: nothing to ask");
        t.unhook_fails = true;
        assert_eq!(t.foreground(&mut p, true), Some(Move::Out), "the module manager");
        assert_eq!(t.answer(&mut p, true), None);
        assert!(t.at.in_chain);
        t.unhook_fails = false;
        assert_eq!(t.look(&mut p, true), None, "a focus change in the module manager");
        assert_eq!(t.foreground(&mut p, true), Some(Move::Out), "a module's dialog over it");
        assert_eq!(t.answer(&mut p, true), None);
        assert!(!t.at.in_chain);
        assert_eq!(t.most_waiting, 1);
    }

    /// The hook could not be put back, and the re-install the witness asks for after the key-downs
    /// it misses is refused as well: its answer asks for no move back, which would be refused the
    /// same way and raise the witness's bar a second time. One attempt a round. A re-install that
    /// puts it back moves nothing after it, and one answered with one of our windows in front by
    /// then is followed by the move out.
    #[test]
    fn a_refused_reinstall_is_not_followed_by_a_move_back() {
        let mut t = HookThread::new();
        let mut p = Place::default();
        t.foreground(&mut p, true);
        t.answer(&mut p, true);
        t.set_fails = true;
        assert_eq!(t.foreground(&mut p, false), Some(Move::In));
        assert_eq!(t.answer(&mut p, false), None);
        t.reinstall(&mut p);
        assert_eq!(t.answer(&mut p, false), None, "no move back after the refused re-install");
        assert!(t.queue.is_empty());
        assert!(!t.at.in_chain);
        assert!(!p.looks_at_key(false), "settled: the keys go to the witness only");
        t.set_fails = false;
        t.reinstall(&mut p);
        assert_eq!(t.answer(&mut p, false), None);
        assert!(t.at.in_chain);
        t.reinstall(&mut p);
        assert_eq!(t.foreground(&mut p, true), None, "the re-install is on its way");
        assert_eq!(t.answer(&mut p, true), Some(Move::Out));
        assert_eq!(t.answer(&mut p, true), None);
        assert!(!t.at.in_chain);
    }

    /// A move that could not be posted is asked for at the next look, and until then every key
    /// looks — whichever side it comes from.
    #[test]
    fn a_move_that_could_not_be_posted_is_asked_for_at_the_next_key() {
        let mut p = Place::default();
        assert_eq!(p.converge(true, HookAt::START, |_| false), None);
        assert!(p.is_out());
        assert!(p.looks_at_key(true), "a key in our window, the move still owed");
        assert!(p.looks_at_key(false));
        assert_eq!(p.converge(true, HookAt::START, |_| true), Some(Move::Out));
        assert_eq!(p.converge(true, HookAt::START.after(false), |_| panic!("out already")), None);
        assert!(!p.looks_at_key(true));
        assert!(p.looks_at_key(false), "a key from another program's side");
    }

    /// The span with the hook out: raw input goes on reporting keys — to this application's
    /// windows, and to another program's in the moment before the window in front is its — and
    /// the hook, out, is called for none of them. None is counted or asked about, and no
    /// re-install comes of them, however many; an unlock or a resume meanwhile only starts the
    /// count afresh (the window in front is looked at then: the next test). With the hook back,
    /// the count starts afresh and the keys are judged again.
    /// The same keys with the hook meant to be in the chain are what a removed hook looks like.
    #[test]
    fn while_the_hook_is_out_no_key_is_counted_and_nothing_installs_it_again() {
        let mut w = Witness::default();
        let (_, last) = run(&mut w, 10_000, 300, 5, true, None);
        let mut t = HookThread::new();
        let mut place = Place::default();
        assert_eq!(t.look(&mut place, true), Some(Move::Out));
        t.answer(&mut place, true);
        for i in 0..60 {
            let judged = place.key_down(&mut w, 20_000 + i * 1_500, last, || {
                panic!("asked about a key while the hook is out")
            });
            assert_eq!(judged, None);
        }
        assert_eq!((w.prev, w.streak), (Some(11_200), 0), "the witness untouched");
        for change in [on_session_change(8), on_session_change(1), on_session_change(3), on_power(0x12)] {
            assert_eq!(place.change(change), Change::Restart, "{change:?} while out");
        }
        assert_eq!(place.change(on_session_change(7)), Change::Restart);
        assert_eq!(place.change(Change::Nothing), Change::Nothing);
        // Back in: the watch starts the count afresh, the hook is called again.
        assert_eq!(t.look(&mut place, false), Some(Move::In));
        w.restart();
        assert_eq!(place.change(on_session_change(8)), Change::Rehook(Reason::Unlocked), "in the chain again");
        assert_eq!(place.key_down(&mut w, 200_000, Some(200_001), || panic!("not asked")), Some(Verdict::First));
        assert_eq!(place.key_down(&mut w, 200_300, Some(200_301), || panic!("not asked")), Some(Verdict::Seen));
        // The same keys, the hook meant to be in and never called: installed again after three.
        let mut w = Witness::default();
        let v: Vec<_> = (0..=MISSES_TO_REHOOK)
            .map(|i| Place::default().key_down(&mut w, 20_000 + i * 1_500, last, || true))
            .collect();
        assert_eq!(v.last(), Some(&Some(Verdict::Rehook { downs: MISSES_TO_REHOOK })), "{v:?}");
    }

    /// A switch the watch never hears of is found at the next key raw input reports from the
    /// other side: while the hook is out, a key gone to another program's window; while it is in,
    /// a key gone to one of this process's windows. A key from the side the hook is on looks at
    /// nothing. A resume, an unlock or a connect while the hook is out looks as well.
    #[test]
    fn a_switch_never_heard_of_is_found_at_the_next_key_from_the_other_side_and_at_an_unlock() {
        let mut t = HookThread::new();
        let mut place = Place::default();
        assert_eq!(t.look(&mut place, false), None);
        assert!(!place.looks_at_key(false), "in, a key to another program: nothing to look at");
        assert!(place.looks_at_key(true), "in, a key to one of our windows");
        assert_eq!(t.look(&mut place, true), Some(Move::Out));
        t.answer(&mut place, true);
        assert!(!place.looks_at_key(true), "out, a key to one of our windows: nothing to look at");
        assert_eq!(t.look(&mut place, true), None);
        // The switch to another program unheard: its first key brings the hook back.
        assert!(place.looks_at_key(false));
        assert_eq!(t.look(&mut place, false), Some(Move::In));
        t.answer(&mut place, false);
        assert!(!place.looks_at_key(false));
        // The keyboard back from a lock or a sleep while the hook is out: looked at. On the way
        // there, or with the hook in (which a re-install of its own answers), not.
        assert_eq!(t.look(&mut place, true), Some(Move::Out));
        for change in [on_session_change(8), on_session_change(1), on_session_change(3), on_power(0x12)] {
            assert!(place.looks_at_change(change), "{change:?} while out");
        }
        for change in [on_session_change(7), on_session_change(2), on_session_change(4), on_power(0x4)] {
            assert!(!place.looks_at_change(change), "{change:?} while out");
        }
        assert!(!place.looks_at_change(Change::Nothing));
        t.answer(&mut place, true);
        assert_eq!(t.look(&mut place, false), Some(Move::In));
        assert!(!place.looks_at_change(on_session_change(8)), "in: a re-install of its own");
    }

    /// Where the hook's thread left the hook survives the trip through the atomic, the count
    /// wrapping as the watch's count of posted requests does.
    #[test]
    fn where_the_hook_is_survives_the_atomic_and_the_counts_wrap_together() {
        for at in [HookAt::START, HookAt { handled: 7, in_chain: false }, HookAt { handled: u32::MAX, in_chain: true }] {
            assert_eq!(HookAt::decode(at.encode()), at);
        }
        assert_eq!(HookAt { handled: u32::MAX, in_chain: true }.after(false), HookAt { handled: 0, in_chain: false });
        let mut p = Place { posted: u32::MAX, ..Place::default() };
        let at = HookAt { handled: u32::MAX, in_chain: true };
        assert_eq!(p.converge(true, at, |_| true), Some(Move::Out));
        assert_eq!(p.posted, 0);
        assert_eq!(p.converge(true, at, |_| panic!("on its way")), None);
        assert_eq!(p.converge(true, at.after(false), |_| panic!("out already")), None);
    }

    /// A page's WebView2 content is drawn by another process in a child window of the page's
    /// frame, and its popups are that process's windows owned by the frame: the window in front
    /// is this application's when it is, or when its root owner is.
    #[test]
    fn the_window_in_front_is_ours_when_it_or_its_root_owner_is() {
        const OWN: u32 = 1234;
        assert!(front_is_ours(OWN, Some(OWN), Some(OWN)), "the page's frame, the module manager");
        assert!(front_is_ours(OWN, Some(OWN), None), "its root owner unread");
        assert!(front_is_ours(OWN, Some(5678), Some(OWN)), "a WebView2 window owned by a page of ours");
        assert!(!front_is_ours(OWN, Some(5678), Some(5678)), "REAPER's FX window");
        assert!(!front_is_ours(OWN, Some(5678), Some(9)), "another program's window owned by a third");
        assert!(!front_is_ours(OWN, None, None), "gone");
    }

    #[test]
    fn the_move_lines_name_the_window_and_say_more_only_when_something_went_wrong() {
        assert_eq!(window_words("Modules", "wxWindowNR", None), "'Modules'");
        assert_eq!(window_words("REAPER", "REAPERwnd", Some("reaper.exe")), "'REAPER' (reaper.exe)");
        assert_eq!(window_words("", "#32770", Some("")), "untitled, class '#32770' (?)");
        assert_eq!(
            move_line(Reason::Out, Outcome::TakenOut { old: Old::Removed }, "'Modules'", ""),
            "the keyboard hook is out while one of our windows is in front ('Modules')"
        );
        assert_eq!(
            move_line(Reason::Back, Outcome::Installed { old: Old::WasOut }, "'REAPER' (reaper.exe)", ""),
            "the keyboard hook is back, first in the chain ('REAPER' (reaper.exe) in front)"
        );
        let l = move_line(Reason::Out, Outcome::TakenOut { old: Old::AlreadyGone }, "'Modules'", "");
        assert!(l.ends_with("('Modules'); Windows had already removed it — it had stopped being called"), "{l}");
        let l = move_line(Reason::Out, Outcome::TakenOut { old: Old::Failed(5) }, "'Modules'", "");
        assert!(l.contains("could not be taken out") && l.contains("UnhookWindowsHookEx error 5"), "{l}");
        let l = move_line(Reason::Back, Outcome::Failed { error: 8 }, "'REAPER' (reaper.exe)", "");
        assert!(l.contains("could not be put back") && l.contains("SetWindowsHookExW error 8"), "{l}");
        let l = move_line(Reason::Back, Outcome::Installed { old: Old::AlreadyGone }, "'REAPER' (reaper.exe)", "");
        assert!(l.starts_with("the keyboard hook is back") && l.contains("had already removed"), "{l}");
        let l = move_line(Reason::Back, Outcome::Reverted { error: 5 }, "'REAPER' (reaper.exe)", "");
        assert!(l.contains("stays where it was") && l.contains("error 5"), "{l}");
        // A re-install for missed keys after a way back that failed.
        let l = line(Reason::Missed { downs: 3 }, Outcome::Installed { old: Old::WasOut }, None);
        assert!(l.contains("no hook of ours was in the chain"), "{l}");
    }

    /// A move out names the other program's window that was in front before ours, a move back how
    /// long ours were in front; a window that is gone by the time it is named says so.
    #[test]
    fn the_move_lines_say_what_was_in_front_before_and_for_how_long() {
        let after = move_after(Reason::Out, Some("'REAPER' (reaper.exe)"), None);
        assert_eq!(
            move_line(Reason::Out, Outcome::TakenOut { old: Old::Removed }, "'Modules'", &after),
            "the keyboard hook is out while one of our windows is in front ('Modules'), after \
             'REAPER' (reaper.exe)"
        );
        let after = move_after(Reason::Back, None, Some(12_345));
        assert_eq!(
            move_line(Reason::Back, Outcome::Installed { old: Old::WasOut }, "'REAPER' (reaper.exe)", &after),
            "the keyboard hook is back, first in the chain ('REAPER' (reaper.exe) in front), after \
             12.3 s with our windows in front"
        );
        assert_eq!(move_after(Reason::Back, None, Some(80)), ", after 0.0 s with our windows in front");
        assert_eq!(move_after(Reason::Out, None, None), "", "armed with ours in front: none before");
        assert_eq!(move_after(Reason::Back, None, None), "");
        assert_eq!(move_after(Reason::Resumed, Some("'x'"), Some(5)), "");
        let l = move_line(Reason::Out, Outcome::TakenOut { old: Old::AlreadyGone }, "'Modules'", ", after 'x'");
        assert!(l.contains("('Modules'), after 'x'; Windows had already removed it"), "{l}");
        assert_eq!(window_words("", "", Some("")), "a window that is gone (?)");
        assert_eq!(window_words("", "", None), "a window that is gone");
    }

    /// Late key-downs one after another write a line a minute at most: the first after a quiet
    /// minute at once, the ones after it counted, with the most late of them and the window in
    /// front at the first, into one line a minute after the last — and nothing when none came.
    #[test]
    fn late_key_downs_write_a_line_a_minute_at_most() {
        let b = |count, late, vk, front: &str| LateBatch { count, late, vk, front: front.to_string() };
        let mut l = LateKeys::default();
        assert_eq!(l.came(1_000, b(1, 300, 0x41, "'A'")), LateSay::Now(b(1, 300, 0x41, "'A'")));
        // Within the minute: kept, and the first asks for the timer of what is left of it.
        assert_eq!(l.came(11_000, b(2, 900, 0x42, "'B'")), LateSay::Kept(Some(50_000)));
        assert_eq!(l.came(12_000, b(1, 400, 0x43, "'C'")), LateSay::Kept(None));
        assert_eq!(l.due(61_000), Some(b(3, 900, 0x42, "'B'")));
        assert_eq!(late_key_line(900, 0x42, "'B'", 2).matches("2 more key-down(s)").count(), 1);
        // The timer for nothing kept: no line.
        assert_eq!(l.due(61_500), None);
        // A minute from that line on, the next is said at once.
        assert_eq!(l.came(70_000, b(1, 260, 0x44, "'D'")), LateSay::Kept(Some(51_000)));
        assert_eq!(l.came(121_000, b(1, 270, 0x45, "'E'")), LateSay::Now(b(2, 270, 0x45, "'D'")));
        // Ticks wrap.
        let mut l = LateKeys::default();
        assert!(matches!(l.came(u32::MAX - 10, b(1, 300, 0x41, "'A'")), LateSay::Now(_)));
        assert_eq!(l.came(20, b(1, 300, 0x41, "'A'")), LateSay::Kept(Some(LATE_LINE_EVERY_MS - 31)));
    }

    /// A key-down that reached the hook 250 ms or more after it was pressed is said, with the
    /// window in front and how many more there were; a key-up, or one less late, is not.
    #[test]
    fn a_key_down_that_reached_the_hook_late_is_said_once_with_how_many_more() {
        assert!(late_key_said(true, 250));
        assert!(late_key_said(true, 5_000));
        assert!(!late_key_said(true, 249));
        assert!(!late_key_said(false, 900), "a key-up: its key-down was said");
        assert!(!late_key_said(true, 0), "on time, or injected");
        let l = late_key_line(812, 0x09, "'REAPER' (reaper.exe)", 0);
        assert_eq!(
            l,
            "a key reached our keyboard hook 812 ms after it was pressed (vk 0x09, 'REAPER' \
             (reaper.exe) in front): it was held that long before our hook was called — by a hook \
             ahead of ours in the chain, or by a machine too busy to run our hook's thread"
        );
        let l = late_key_line(300, 0x41, "'Modules'", 3);
        assert!(
            l.ends_with(
                "; 3 more key-down(s) came 250 ms or more late since the line before (this one \
                 names the most late; a line at most once a minute)"
            ),
            "{l}"
        );
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
