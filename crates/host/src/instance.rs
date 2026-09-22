//! One running copy of the application per user (and, on Windows, per session).
//!
//! A second copy used to start a second host. Two hosts fight over every hotkey and write the
//! same log file into each other — both seen on a real machine — and each installs a keyboard
//! hook of its own. So a second start now brings the running copy's module manager forward and
//! exits, which is what somebody starting the application again almost always wants: the
//! window they could not find.
//!
//! Two parts, each an established mechanism rather than one written here:
//!
//! - **The lock** is wxWidgets' `wxSingleInstanceChecker`, which wxdragon has bound since
//!   0.9.15 (the lock file holds 0.9.16, so no bump was needed): a named mutex on Windows, a
//!   lock file holding the owner's pid on macOS. It is created in `gui.rs`, the one file that
//!   talks to wxdragon, and reaches this module through the [`Lock`] trait — which is also
//!   what lets `macos-check` borrow this file and type-check it for the Mac.
//! - **The request** travels over `interprocess` local sockets: a named pipe on Windows, a
//!   Unix-domain socket on macOS. wxWidgets has an IPC layer of its own, and it was checked and
//!   rejected: on Windows `wxServer`/`wxClient` are DDE, and starting a DDE conversation
//!   broadcasts `WM_DDE_INITIATE` with `SendMessage` to every top-level window on the desktop,
//!   so one hung window anywhere hangs the second start with it. Its alternative,
//!   `wxTCPServer`, is a TCP port on localhost: open to every user on the machine, and a port
//!   number that has to be agreed on and can already be taken.
//!
//! **Names** are per user and never shared between users: the user's SID and the session id on
//! Windows (the pipe namespace is machine-wide, and a user can be logged on in two sessions),
//! the uid on macOS. The macOS lock file and socket live in the user's own
//! `~/Library/Application Support/AutomationPlatform` — not beside the application, because
//! two copies in two folders are still two copies fighting over the same keys; not in `/tmp`,
//! which every user shares; and not in the per-user temporary folder either, which macOS
//! empties of anything nobody has touched for three days, a tray application's lock file and
//! socket included. That folder is only the fallback, for a path the socket's 104 bytes cannot
//! hold.
//!
//! **Who may talk to whom.** A name can be squatted — the pipe namespace is machine-wide — so
//! neither side trusts the name alone. The Windows pipe admits only this user and SYSTEM, and
//! the second start checks, before it sends anything, that the process answering is in its
//! own session and runs as its own user; it also connects at identification level, so a
//! process that squatted the name cannot act as the user. On macOS the socket's folder is the
//! user's own, and each side checks the other's uid.
//!
//! **Headless runs are exempt** and never take or ask the lock. CI and the development tools
//! start a headless host next to a running application on purpose; see `run` in `lib.rs`.
//!
//! **macOS, packaged:** Launch Services already keeps a `.app` to one copy and sends the
//! running one a reopen event instead, which `gui.rs` answers with the same request. The guard
//! matters there for the bare binary (`run-dev.sh`) and for anything that bypasses Launch
//! Services.
//!
//! **The Windows identity calls** (the SID and session of this process, the user a process runs
//! as, whether a mutex exists that we may not open) are a few windows-sys calls rather than a
//! crate. `sysinfo`, the established crate that answers the first three, was checked and
//! rejected: 0.38 needs `windows` 0.62 while the host is on 0.58 (a second generation of that
//! crate compiled in for three calls), and it answers a question about one process by
//! snapshotting every process on the machine (`CreateToolhelp32Snapshot`), on every poll of a
//! second start's wait.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use interprocess::local_socket::prelude::*;
use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, Listener, ListenerOptions, Name,
};

use crate::logging;

/// What the lock can say. Implemented in `gui.rs` for the lock it builds on wxdragon's
/// `SingleInstanceChecker`.
pub trait Lock {
    /// Whether another process held the lock when this one was created.
    fn another_running(&self) -> bool;
}

/// The one prefix every name here starts with.
const BASE: &str = "AutomationPlatform";

/// The second copy's request, and the running copy's three answers. One line each.
const REQUEST_SHOW: &str = "show-manager";
const ANSWER_SHOWN: &str = "shown";
const ANSWER_QUITTING: &str = "quitting";
const ANSWER_UNKNOWN: &str = "unknown-request";

/// Longest line either side reads. Both ends are ours, and a line longer than this is not.
const MAX_LINE: u64 = 256;

/// How long a second copy keeps trying before it gives up.
///
/// Long enough to cover the one case where waiting is right: the running copy was just told
/// to quit and is still shutting down (it answers `quitting` from the moment Quit is chosen),
/// so the person who chose Quit and started it again gets a fresh copy instead of nothing.
/// Shutting down drops every module and joins the OCR warmup thread, which is bounded by one
/// model load and one inference.
const WAIT: Duration = Duration::from_secs(10);

/// Pause between two attempts while waiting.
const POLL: Duration = Duration::from_millis(200);

/// How long one attempt to reach the running copy may take. The running copy answers from a
/// thread of its own, which never waits for its window loop, so a live copy answers at once;
/// the bound is for a copy that is frozen, not for one that is busy.
const CONTACT_TIMEOUT: Duration = Duration::from_secs(2);

/// How long a new running copy keeps trying to listen when the name is still taken.
///
/// The copy before it releases the lock only after its listener has stopped, but on Windows a
/// pipe name also stays taken while any instance of it is open anywhere — and the first
/// instance is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, which fails while another
/// exists. A previous copy whose process is still ending, or a connection it was still
/// answering, is gone within moments; three seconds is ample for that and short enough that a
/// name squatted for good costs a start very little.
const LISTEN_RETRY: Duration = Duration::from_secs(3);

/// How long quitting waits for the listener to stop and for answers in flight to finish.
const STOP_WAIT: Duration = Duration::from_secs(1);

/// At most this many connections are answered at once. A real second start writes its request
/// the moment it connects and is answered in microseconds; a connection that says nothing
/// keeps its thread until it goes away, and must not be able to pile up threads in the
/// process that owns the keyboard hook. Connections beyond this are closed unanswered, and a
/// real second start that meets one tries again at its next poll.
const MAX_ANSWERING: usize = 4;

/// Whether a held lock can be a leftover. Only a lock FILE can: wxWidgets decides that the
/// file's owner is alive with `kill(pid, 0)`, and after a crash and a restart that pid may
/// belong to an unrelated process. A Windows mutex dies with the last handle to it.
const STALE_LOCKS_POSSIBLE: bool = cfg!(unix);

/// Everything that names this user's running copy.
#[derive(Debug, Clone, PartialEq)]
pub struct Names {
    /// The lock's name: the mutex's on Windows, the lock file's on macOS.
    pub lock: String,
    /// The folder the lock file goes in (macOS). `None` on Windows, where the mutex is a
    /// kernel object and wxWidgets ignores the folder.
    pub lock_dir: Option<PathBuf>,
    /// Where the running copy listens.
    pub endpoint: Endpoint,
    /// How the log names the two. On Windows without the SID, which identifies the account
    /// and adds nothing a reader of the log needs: the log is attached to public issues. On
    /// macOS with the home folder as `~`, for the same reason.
    pub for_log: String,
    /// The user's SID (Windows): the one account the pipe admits besides SYSTEM, and the one a
    /// running copy has to run as for a second start to trust it. `None` where it could not be
    /// read, and always off Windows.
    pub owner: Option<String>,
}

/// Where the running copy listens for a second copy.
///
/// Both variants, and both name builders below, exist on both platforms although each uses
/// one: that way the tests of the other platform's names run here too, where they can be run.
#[derive(Debug, Clone, PartialEq)]
pub enum Endpoint {
    /// A named pipe, `\\.\pipe\<name>` (Windows).
    #[cfg_attr(not(windows), allow(dead_code))]
    Pipe(String),
    /// A Unix-domain socket file (macOS).
    #[cfg_attr(windows, allow(dead_code))]
    Socket(PathBuf),
}

impl Endpoint {
    fn name(&self) -> std::io::Result<Name<'static>> {
        match self {
            Endpoint::Pipe(n) => n.clone().to_ns_name::<GenericNamespaced>(),
            Endpoint::Socket(p) => p.clone().to_fs_name::<GenericFilePath>(),
        }
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Endpoint::Pipe(n) => write!(f, r"\\.\pipe\{n}"),
            Endpoint::Socket(p) => write!(f, "{}", p.display()),
        }
    }
}

/// Keeps what a mutex or pipe name may hold. A backslash would make the mutex name a
/// namespace path; everything else outside this set is replaced to keep names predictable.
#[cfg_attr(not(windows), allow(dead_code))]
fn sanitize(part: &str) -> String {
    part.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' })
        .collect()
}

/// The Windows names: one string for the mutex and the pipe, which live in different
/// namespaces and cannot collide. `user` is the SID string, `session` the Terminal Services
/// session id — the mutex namespace is per session already, the pipe namespace is not.
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_names(user: &str, session: u32) -> Names {
    let base = format!("{BASE}-{}-s{session}", sanitize(user));
    Names {
        lock: base.clone(),
        lock_dir: None,
        endpoint: Endpoint::Pipe(base),
        for_log: format!(r"the mutex and the pipe \\.\pipe\{BASE}-<user>-s{session}"),
        owner: None,
    }
}

/// The macOS names: a lock file and a socket beside it, in `dir`.
#[cfg_attr(windows, allow(dead_code))]
fn unix_names(uid: u32, dir: &Path) -> Names {
    let base = format!("{BASE}-{uid}");
    let lock = format!("{base}.lock");
    let socket = dir.join(format!("{base}.sock"));
    Names {
        for_log: format!(
            "the lock file {} and the socket {}",
            home_as_tilde(&dir.join(&lock)),
            home_as_tilde(&socket)
        ),
        lock,
        lock_dir: Some(dir.to_path_buf()),
        endpoint: Endpoint::Socket(socket),
        owner: None,
    }
}

/// A path for the log, with the home folder written as `~`: the home folder's name is the
/// account's, and the log is attached to public issues.
#[cfg_attr(windows, allow(dead_code))]
fn home_as_tilde(path: &Path) -> String {
    match std::env::var_os("HOME").map(PathBuf::from) {
        Some(home) if !home.as_os_str().is_empty() => match path.strip_prefix(&home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        },
        _ => path.display().to_string(),
    }
}

/// This user's names on this machine.
#[cfg(windows)]
pub fn names() -> Names {
    // The SID is what identifies a user; the name can be renamed, and two domains can each
    // have an "admin". The fallback is only for a token that cannot be read, which would be a
    // broken machine, and it is still per user and per session.
    let sid = win::user_sid();
    let user = sid.clone().unwrap_or_else(|| {
        format!("user-{}", std::env::var("USERNAME").unwrap_or_else(|_| "unknown".into()))
    });
    Names { owner: sid, ..windows_names(&user, win::session_id().unwrap_or(0)) }
}

/// This user's names on this machine.
#[cfg(target_os = "macos")]
pub fn names() -> Names {
    // SAFETY: getuid cannot fail and has no preconditions.
    let uid = unsafe { libc::getuid() };
    let socket_file = format!("{BASE}-{uid}.sock");
    unix_names(uid, &mac::instance_dir(uid, &socket_file))
}

/// Not a platform this application ships for; this is what a port would start from.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn names() -> Names {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0);
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(crate::portable::fallback_dir);
    unix_names(uid, &dir)
}

/// Whether a lock that could not be created exists all the same, held by a process whose lock
/// this one may not open — on Windows, a copy running as administrator: the mutex it created
/// carries an elevated token's default security, which (by Windows' defaults; not yet seen on
/// a real machine, see TODO.md) admits Administrators and SYSTEM, and a normal process's
/// Administrators membership is for denying only. wxWidgets reports that as a plain failure
/// and drops the error code, so this asks again, for no more than the right to wait on it.
///
/// Called by `gui::instance_lock` only after wxWidgets has refused. Always `false` off
/// Windows: a lock file wxWidgets refuses (someone else's, or the wrong permissions) is not a
/// running copy of ours.
pub fn held_where_we_cannot_open(names: &Names) -> bool {
    #[cfg(windows)]
    {
        win::mutex_exists_but_denied(&names.lock)
    }
    #[cfg(not(windows))]
    {
        let _ = names;
        false
    }
}

// ── The request flag the window reads ─────────────────────────────────────────────────────

/// Set when somebody asked for the module window from outside: a second start, or (macOS) a
/// reopen event. Read by the window's timer tick, which is the only place the window can be
/// touched from — the request arrives on another thread, or before the window exists.
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Asks for the module window to be shown at the window's next tick.
pub fn request_show() {
    SHOW_REQUESTED.store(true, Ordering::SeqCst);
}

/// Whether the window has been asked for since the last call; clears the request.
pub fn take_show_request() -> bool {
    SHOW_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Set the moment Quit is chosen: from then on a second start is told `quitting`.
///
/// Global rather than a field of [`Primary`], because Quit is chosen in the window code, which
/// does not hold the running copy's claim — and because the time between Quit and the end of
/// the window loop (wxWidgets tearing its windows down) is exactly when a second start must
/// not be told that a window will be shown.
static QUITTING: AtomicBool = AtomicBool::new(false);

/// Says that this copy is quitting. Called by the Quit items before they end the window loop.
pub fn begin_quit() {
    QUITTING.store(true, Ordering::SeqCst);
}

// ── Claiming ──────────────────────────────────────────────────────────────────────────────

/// What starting up found.
pub enum Claim<L> {
    /// This is the running copy. Keep it until the host has shut down: dropping it stops
    /// answering and then releases the lock.
    Primary(Primary<L>),
    /// Another copy is running and has been asked for its window. Exit without a word.
    Shown,
    /// Another copy holds the lock and did not show its window; this copy must not start.
    GaveUp(GaveUp),
}

/// Why a start gave up, said twice: once for the log and once for the person who started it.
#[derive(Debug, Clone, PartialEq)]
pub struct GaveUp {
    /// The one line worth leaving in the running copy's log.
    pub log_line: String,
    /// What the message box says. A start that ends in silence after ten seconds is, to
    /// somebody who cannot see the screen, an application that simply never appeared.
    pub message: String,
}

/// The running copy's hold on the lock and its listener.
pub struct Primary<L> {
    /// Dropped last (fields drop after `Drop::drop`), so the listener has stopped and the
    /// socket file is gone before the next copy can take the lock and listen itself.
    _lock: Option<L>,
    endpoint: Endpoint,
    quitting: Arc<AtomicBool>,
    listening: Option<Listening>,
    notes: Vec<String>,
}

impl<L> Primary<L> {
    /// Writes what claiming found. Separate from [`claim`] because claiming happens before the
    /// log is opened — a second copy must not open it at all.
    pub fn log_notes(&self) {
        for n in &self.notes {
            logging::line("instance", n);
        }
    }

    /// From here on a second start is told `quitting`, and waits for this copy to be gone
    /// instead of being told its request was delivered to a window loop that has ended.
    pub fn stop_serving(&self) {
        self.quitting.store(true, Ordering::SeqCst);
    }
}

impl<L> Drop for Primary<L> {
    fn drop(&mut self) {
        self.stop_serving();
        // The listener first, while the lock is still held: on Windows its pipe keeps the name
        // taken for as long as any instance of it is open, and a thread left blocked in accept
        // would keep one open until the process was gone — so the next copy, taking the lock
        // the moment it is released, could not listen, and nobody could ever reach it.
        if let Some(listening) = self.listening.take() {
            if !listening.stop(&self.endpoint) {
                logging::line(
                    "instance",
                    &format!(
                        "the listener for other starts did not stop within {} s; the next copy \
                         may have to wait for this process to end before it can listen",
                        STOP_WAIT.as_secs()
                    ),
                );
            }
        }
        // A socket file outlives its listener. Removed here, while the lock is still held, so
        // it can only be ours.
        if let Endpoint::Socket(p) = &self.endpoint {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// The lock's state after one attempt.
#[derive(Debug, Clone, Copy, PartialEq)]
enum LockState {
    /// Nobody held it: this copy has it now.
    Free,
    /// Another process holds it.
    Held,
    /// It could not be created at all (wxWidgets refused, or the folder is unusable).
    Unavailable,
}

/// What the running copy answered.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Answer {
    Shown,
    Quitting,
    /// It answered, and did not know the request: a different version of the application.
    Unknown,
    /// Something answered on the name that is not this user's copy in this session, or could
    /// not be confirmed to be (Windows: the process's user could not be read).
    Refused,
    /// No listener, a refused connection, silence past the timeout, or a line we do not know.
    Nothing,
}

/// What to do after one attempt.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Step {
    /// Run as the one copy.
    Proceed,
    /// The running copy has the request; exit.
    Handed,
    /// Try again after a pause.
    Wait,
    /// Remove a lock file that nothing answers for, and try again (once).
    TakeOver,
    /// Exit without starting.
    GiveUp,
}

/// The decision, separated from the waiting and the I/O so it can be tested on its own.
///
/// - A free lock is ours.
/// - A running copy that took the request: done.
/// - A copy of ours that answered and did not understand: a different version is running, and
///   no amount of waiting changes that. Starting beside it is the fight this guard prevents.
/// - A lock that could not be created is no reason to refuse to start — that would turn a
///   broken lock into an application that never appears — unless a running copy answers, or
///   says it is quitting (then it is waited for, like a held lock).
/// - Otherwise wait for the lock to be released, until `timed_out`. Then a lock that may be a
///   leftover is taken over once; a lock that cannot be (a Windows mutex, or a lock file whose
///   owner is running this application) means a copy is alive and not answering, and starting
///   a second host next to it is the fight this guard prevents.
fn step(
    lock: LockState,
    answer: Answer,
    timed_out: bool,
    may_be_stale: bool,
    took_over: bool,
) -> Step {
    match (lock, answer) {
        (LockState::Free, _) => Step::Proceed,
        (_, Answer::Shown) => Step::Handed,
        (_, Answer::Unknown) => Step::GiveUp,
        (LockState::Unavailable, Answer::Quitting) if !timed_out => Step::Wait,
        (LockState::Unavailable, _) => Step::Proceed,
        (LockState::Held, _) if !timed_out => Step::Wait,
        (LockState::Held, _) if may_be_stale && !took_over => Step::TakeOver,
        (LockState::Held, _) => Step::GiveUp,
    }
}

/// Takes the single-instance lock, or hands the start over to the copy that has it.
///
/// `make_lock` creates the platform lock for the names given (see `gui::instance_lock`). It is
/// called again on every attempt, because only a fresh lock can say whether the old owner has
/// gone: on Windows this process's own handle would otherwise keep the mutex alive.
///
/// Blocks for up to [`WAIT`] when a running copy does not answer. Writes nothing to the log:
/// the running copy logs the request in its own session, and the caller writes the one line
/// of [`Claim::GaveUp`].
pub fn claim<L: Lock>(make_lock: impl FnMut(&Names) -> Option<L>) -> Claim<L> {
    claim_with(&names(), make_lock, WAIT, POLL, CONTACT_TIMEOUT, STALE_LOCKS_POSSIBLE)
}

fn claim_with<L: Lock>(
    names: &Names,
    mut make_lock: impl FnMut(&Names) -> Option<L>,
    wait: Duration,
    poll: Duration,
    contact_timeout: Duration,
    stale_possible: bool,
) -> Claim<L> {
    let started = Instant::now();
    let mut took_over = false;
    loop {
        let lock = make_lock(names);
        let state = match &lock {
            None => LockState::Unavailable,
            Some(l) if l.another_running() => LockState::Held,
            Some(_) => LockState::Free,
        };
        // Asked only when somebody else might be there; a free lock needs no conversation.
        let answer =
            if state == LockState::Free { Answer::Nothing } else { contact(names, contact_timeout) };
        let timed_out = started.elapsed() >= wait;
        // Looked into only at the end, and only for a lock file: whether the pid in it is
        // running this application, which would make it a live copy that is not answering
        // rather than a leftover.
        let may_be_stale = stale_possible
            && state == LockState::Held
            && timed_out
            && lock_owner_is_this_app(names) != Some(true);
        match step(state, answer, timed_out, may_be_stale, took_over) {
            Step::Proceed => {
                return Claim::Primary(become_primary(names, lock, state, took_over.then_some(wait)))
            }
            Step::Handed => return Claim::Shown,
            Step::Wait => {
                drop(lock);
                std::thread::sleep(poll);
            }
            Step::TakeOver => {
                drop(lock);
                if let Some(dir) = &names.lock_dir {
                    let _ = std::fs::remove_file(dir.join(&names.lock));
                }
                took_over = true;
            }
            Step::GiveUp => return Claim::GaveUp(gave_up(names, answer, wait)),
        }
    }
}

/// The two texts for a start that gives up, by what the running copy last said.
fn gave_up(names: &Names, last: Answer, wait: Duration) -> GaveUp {
    let secs = wait.as_secs();
    let end_it = if cfg!(windows) { "in Task Manager" } else { "in Activity Monitor" };
    let tray = if cfg!(target_os = "macos") { "its menu-bar item" } else { "its tray icon" };
    let (why, message) = match last {
        Answer::Quitting => (
            format!("another copy was still shutting down after {secs} s"),
            format!(
                "Automation Platform is still closing after {secs} seconds, so it was not \
                 started again yet.\n\nWait a moment and start it again. If this keeps \
                 happening, end the old copy {end_it} and start the application again."
            ),
        ),
        Answer::Unknown => (
            "another copy answered but did not know the request to show its window (a \
             different version?)"
                .to_string(),
            format!(
                "A different version of Automation Platform is already running, and it did \
                 not understand the request to show its window.\n\nQuit it from {tray}, then \
                 start this one again."
            ),
        ),
        Answer::Refused => (
            format!(
                "the process answering on the name is not this user's copy in this session, \
                 or could not be confirmed to be (one started as administrator?), and no copy \
                 answered for {secs} s"
            ),
            format!(
                "Another copy of Automation Platform is running, and this start could not \
                 confirm that it is yours, so it did not ask it for its window. This happens \
                 when one copy was started as administrator and the other was not.\n\nQuit \
                 the running copy from {tray} and start the application again, or start this \
                 one the same way the running one was started."
            ),
        ),
        Answer::Shown | Answer::Nothing => (
            format!("another copy holds the lock and did not answer for {secs} s"),
            format!(
                "Automation Platform is already running, but it did not respond within {secs} \
                 seconds, so it was not started a second time.\n\nIf the running copy cannot \
                 be found, end it {end_it} and start the application again."
            ),
        ),
    };
    GaveUp {
        log_line: format!(
            "a second copy of the application (pid {}) did not start: {why}. The lock is {}. \
             It says so in a message box.",
            std::process::id(),
            names.for_log
        ),
        message,
    }
}

/// Shows the message of a start that gave up, and returns when it has been dismissed.
///
/// A native message box of the system's own (`MessageBoxW` on Windows, a Core Foundation
/// alert on macOS), because this runs before wxWidgets exists and the process ends right after:
/// the screen reader reads such a box when it appears, and nothing is spoken here.
pub fn tell(gave_up: &GaveUp) {
    #[cfg(windows)]
    win::tell("Automation Platform", &gave_up.message);
    #[cfg(target_os = "macos")]
    mac::tell("Automation Platform", &gave_up.message);
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = gave_up;
}

/// Starts listening, and records what the log should say about how this copy got here.
/// `took_over` is how long the stale lock's owner was waited for, when there was one.
fn become_primary<L>(
    names: &Names,
    lock: Option<L>,
    state: LockState,
    took_over: Option<Duration>,
) -> Primary<L> {
    let mut notes = Vec::new();
    if state == LockState::Unavailable {
        notes.push(format!(
            "the single-instance lock could not be created, so this copy runs unguarded; a \
             second start still finds it while it answers ({})",
            names.for_log
        ));
    } else {
        notes.push(format!("this is the one running copy for this user ({})", names.for_log));
    }
    if let Some(waited) = took_over {
        notes.push(format!(
            "took over a lock file nothing answered for in {} s: it was left behind by a copy \
             that ended without removing it, and the process it names is not this application",
            waited.as_secs()
        ));
    }
    let quitting = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    let mut failed = 0u32;
    let listening = loop {
        match listen(names, quitting.clone()) {
            Ok(l) => {
                if failed > 0 {
                    notes.push(format!(
                        "listening for other starts after {} ms: the name was still taken, \
                         most likely by the copy that had just quit and whose process was \
                         still ending",
                        started.elapsed().as_millis()
                    ));
                }
                break Some(l);
            }
            Err(_) if started.elapsed() < LISTEN_RETRY => {
                failed += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                notes.push(format!(
                    "cannot listen on {} for a second start (tried for {} s): {e}. One will \
                     wait and then give up instead of showing this copy's window",
                    names.endpoint_for_log(),
                    LISTEN_RETRY.as_secs()
                ));
                break None;
            }
        }
    };
    Primary { _lock: lock, endpoint: names.endpoint.clone(), quitting, listening, notes }
}

impl Names {
    /// The endpoint as the log may name it: without the SID on Windows, as `for_log` does.
    fn endpoint_for_log(&self) -> String {
        match &self.endpoint {
            Endpoint::Pipe(_) => "the pipe".to_string(),
            Endpoint::Socket(p) => home_as_tilde(p),
        }
    }
}

/// Whether the process a lock file names runs this application: `Some(true)` when it runs an
/// executable with this one's file name, `None` when that cannot be told (no lock file, no
/// such process, or not macOS).
///
/// wxWidgets writes the owner's pid into the file (as text, with a NUL after it) and trusts
/// `kill(pid, 0)` to say whether the owner lives, removing the file at once when it does not.
/// After a crash and a restart, though, that pid can belong to anything, and a lock file nobody
/// answers for would then keep the application from ever starting. Asking what the pid is
/// running tells that apart from a copy of ours that is alive but does not answer, which must
/// not get a second host beside it.
#[cfg(target_os = "macos")]
fn lock_owner_is_this_app(names: &Names) -> Option<bool> {
    let dir = names.lock_dir.as_ref()?;
    let text = std::fs::read(dir.join(&names.lock)).ok()?;
    let pid: i32 = String::from_utf8_lossy(&text)
        .trim_matches(|c: char| c == '\0' || c.is_whitespace())
        .parse()
        .ok()?;
    let theirs = mac::process_path(pid)?;
    let ours = std::env::current_exe().ok()?;
    Some(theirs.file_name() == ours.file_name())
}

#[cfg(not(target_os = "macos"))]
fn lock_owner_is_this_app(_names: &Names) -> Option<bool> {
    None
}

// ── The conversation ─────────────────────────────────────────────────────────────────────

/// The running copy's answer to one request line.
fn reply_to(request: &str, quitting: bool) -> &'static str {
    match request.trim() {
        REQUEST_SHOW if quitting => ANSWER_QUITTING,
        REQUEST_SHOW => ANSWER_SHOWN,
        _ => ANSWER_UNKNOWN,
    }
}

/// The second copy's reading of that answer.
fn parse_answer(line: &str) -> Answer {
    match line.trim() {
        ANSWER_SHOWN => Answer::Shown,
        ANSWER_QUITTING => Answer::Quitting,
        ANSWER_UNKNOWN => Answer::Unknown,
        _ => Answer::Nothing,
    }
}

/// The running copy's listener: a thread blocked in accept, and what it takes to stop it.
struct Listening {
    stop: Arc<AtomicBool>,
    /// Sent once the thread has returned, after its listener — every pipe instance it had
    /// open — has been dropped. Disconnected if the thread ended any other way.
    done: mpsc::Receiver<()>,
    thread: std::thread::JoinHandle<()>,
    /// Connections being answered right now.
    answering: Arc<AtomicUsize>,
}

impl Listening {
    /// Stops the listener and waits, for up to [`STOP_WAIT`], until it and the answers in
    /// flight have finished. Returns whether the listener stopped in time.
    ///
    /// The thread is blocked in accept, which nothing interrupts, so it is woken the way any
    /// client would wake it: by connecting. It sees the stop flag and returns without
    /// answering.
    ///
    /// The waking connection is held open until the thread has finished, and made again if it
    /// has not finished soon. On Windows a client that connects and closes again before the
    /// listener has reached `ConnectNamedPipe` — as happens when a copy quits moments after it
    /// started listening — leaves nothing to accept: interprocess clears such a connection and
    /// goes on waiting (a unit test met exactly that). And a connection can fail outright while
    /// every instance of the pipe is busy, for a moment between two accepts.
    fn stop(self, endpoint: &Endpoint) -> bool {
        self.stop.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + STOP_WAIT;
        let stopped = loop {
            let _waking = wake(endpoint);
            match self.done.recv_timeout(Duration::from_millis(50)) {
                Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => continue,
                Err(mpsc::RecvTimeoutError::Timeout) => break false,
                Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break true,
            }
        };
        if stopped {
            let _ = self.thread.join();
        }
        // An answer in flight holds a pipe instance of its own. A real one finishes in
        // microseconds; one waiting on a client that never speaks is left to the end of the
        // process (it can only be this user's, since nobody else may connect).
        while self.answering.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        stopped
    }
}

/// Counts one connection being answered, for as long as it lives.
struct AnswerSlot(Arc<AtomicUsize>);

impl Drop for AnswerSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Binds the endpoint and answers on a thread of its own until [`Listening::stop`].
///
/// Each connection gets a short-lived thread too, so a client that connects and then says
/// nothing holds up only itself — and at most [`MAX_ANSWERING`] of them at once. On macOS a
/// socket file left by a copy that crashed is replaced (`try_overwrite`): whoever holds the
/// lock owns the name. On Windows the pipe admits this user and SYSTEM only; the default
/// would let every account on the machine open it for reading, which is enough to connect.
fn listen(names: &Names, quitting: Arc<AtomicBool>) -> std::io::Result<Listening> {
    let options = ListenerOptions::new().name(names.endpoint.name()?);
    #[cfg(windows)]
    let options = match &names.owner {
        Some(sid) => {
            use interprocess::os::windows::local_socket::ListenerOptionsExt;
            options.security_descriptor(win::pipe_security(sid)?)
        }
        None => options,
    };
    #[cfg(unix)]
    let options = options.try_overwrite(true).max_spin_time(Duration::from_secs(1));
    let listener = options.create_sync()?;
    let stop = Arc::new(AtomicBool::new(false));
    let answering = Arc::new(AtomicUsize::new(0));
    let (done_tx, done) = mpsc::channel();
    let thread = {
        let (stop, answering) = (stop.clone(), answering.clone());
        std::thread::Builder::new().name("instance-listener".into()).spawn(move || {
            serve(listener, &stop, &quitting, &answering);
            let _ = done_tx.send(());
        })?
    };
    Ok(Listening { stop, done, thread, answering })
}

/// The accept loop. Takes the listener by value so that it is dropped — and with it every
/// pipe instance it holds — before the thread reports that it has finished.
fn serve(
    listener: Listener,
    stop: &AtomicBool,
    quitting: &Arc<AtomicBool>,
    answering: &Arc<AtomicUsize>,
) {
    let mut failures = 0u32;
    for conn in listener.incoming() {
        // The wake-up from `Listening::stop`, or a start that raced it: closed unanswered. A
        // start that raced it reads nothing, and asks again at its next poll.
        if stop.load(Ordering::SeqCst) {
            return;
        }
        match conn {
            Ok(conn) => {
                failures = 0;
                if answering.fetch_add(1, Ordering::SeqCst) >= MAX_ANSWERING {
                    answering.fetch_sub(1, Ordering::SeqCst);
                    continue; // dropped unanswered, see MAX_ANSWERING
                }
                let slot = AnswerSlot(answering.clone());
                let quitting = quitting.clone();
                // If the thread cannot be started, the closure is dropped and the slot with it.
                let _ = std::thread::Builder::new().name("instance-answer".into()).spawn(move || {
                    let _slot = slot;
                    answer(conn, &quitting);
                });
            }
            Err(e) => {
                // Not expected on a working machine. Stop rather than spin: a second start
                // then waits and gives up, which is the guard's safe side.
                failures += 1;
                if failures >= 20 {
                    logging::line(
                        "instance",
                        &format!("stopped answering other starts after repeated errors: {e}"),
                    );
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

/// One connection: read the request, set the flag, answer, and say so in the log.
fn answer(conn: LocalSocketStream, quitting: &AtomicBool) {
    // Belt and braces on macOS: the socket's folder is the user's own, so another user cannot
    // reach it — but a request that says it is from someone else is not answered.
    #[cfg(target_os = "macos")]
    {
        // SAFETY: getuid cannot fail and has no preconditions.
        let me = unsafe { libc::getuid() };
        if conn.peer_creds().ok().and_then(|c| c.euid()).is_some_and(|uid| uid != me) {
            return;
        }
    }
    #[cfg(windows)]
    let peer = conn.peer_creds().ok().and_then(|c| c.pid());
    #[cfg(not(windows))]
    let peer: Option<u32> = None;

    let mut reader = BufReader::new(conn);
    let mut request = String::new();
    if reader.by_ref().take(MAX_LINE).read_line(&mut request).is_err() {
        return;
    }
    let quitting = quitting.load(Ordering::SeqCst) || QUITTING.load(Ordering::SeqCst);
    let reply = reply_to(&request, quitting);
    if reply == ANSWER_SHOWN {
        request_show();
        logging::line(
            "instance",
            &format!(
                "another start of the application{} asked for the module window; showing it",
                peer.map(|p| format!(" (pid {p})")).unwrap_or_default()
            ),
        );
    }
    let mut conn = reader.into_inner();
    let _ = conn.write_all(format!("{reply}\n").as_bytes());
    // No flush: on Windows it would wait for the client to read, and interprocess finishes
    // sending an unflushed pipe in the background when the stream is dropped (its "limbo").
}

/// Asks the running copy for its window, giving up after `timeout`.
///
/// On a thread, because neither side of a named pipe has a read timeout in interprocess, and a
/// frozen copy must cost the second start a bounded wait rather than all of it.
fn contact(names: &Names, timeout: Duration) -> Answer {
    let names = names.clone();
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("instance-contact".into())
        .spawn(move || {
            let _ = tx.send(ask(&names));
        });
    if spawned.is_err() {
        return Answer::Nothing;
    }
    rx.recv_timeout(timeout).unwrap_or(Answer::Nothing)
}

/// Connects, checks who answered, and exchanges the one request and the one answer.
#[cfg(windows)]
fn ask(names: &Names) -> Answer {
    // Opened by hand rather than through interprocess's connect, for two things it does not
    // do: fail at once when every instance is busy (the caller polls anyway), and connect at
    // identification level, so a process that squatted the name cannot act as this user.
    let Ok(stream) = win::open_pipe(&names.endpoint.to_string()) else { return Answer::Nothing };
    // Before anything is sent, and before the foreground is handed over: the name is
    // machine-wide, and whoever created it first is who answers.
    if !win::server_is_ours(&stream, names.owner.as_deref()) {
        return Answer::Refused;
    }
    // Windows lets a background process take the foreground only with permission, and this
    // process has it — it was just started by the user — while the running copy, sitting in
    // the tray, does not. Handed over BEFORE the request, so it is there when the running copy
    // raises its window; without it the window opens behind the current one, and the screen
    // reader's focus stays where it was.
    if let Ok(pid) = stream.server_process_id() {
        win::allow_foreground(pid);
    }
    exchange(&stream)
}

/// Connects, checks who answered, and exchanges the one request and the one answer.
#[cfg(not(windows))]
fn ask(names: &Names) -> Answer {
    let Ok(name) = names.endpoint.name() else { return Answer::Nothing };
    // Fails at once when nothing listens: no socket, or a dead one.
    let Ok(conn) = LocalSocketStream::connect(name) else { return Answer::Nothing };
    // The socket's folder is the user's own; this is the same check `answer` makes, the
    // other way round.
    #[cfg(target_os = "macos")]
    {
        // SAFETY: getuid cannot fail and has no preconditions.
        let me = unsafe { libc::getuid() };
        if conn.peer_creds().ok().and_then(|c| c.euid()) != Some(me) {
            return Answer::Refused;
        }
    }
    exchange(&conn)
}

/// Sends the request line and reads the answer line.
fn exchange<S: Read + Write>(stream: S) -> Answer {
    let mut conn = BufReader::new(stream);
    if conn.get_mut().write_all(format!("{REQUEST_SHOW}\n").as_bytes()).is_err() {
        return Answer::Nothing;
    }
    let mut line = String::new();
    match conn.take(MAX_LINE).read_line(&mut line) {
        Ok(n) if n > 0 => parse_answer(&line),
        _ => Answer::Nothing,
    }
}

/// One connection to our own endpoint, to wake our own accept; it stays open while the result
/// is kept. See [`Listening::stop`].
fn wake(endpoint: &Endpoint) -> std::io::Result<Box<dyn std::any::Any>> {
    #[cfg(windows)]
    {
        Ok(Box::new(win::open_pipe_file(&endpoint.to_string())?))
    }
    #[cfg(not(windows))]
    {
        Ok(Box::new(LocalSocketStream::connect(endpoint.name()?)?))
    }
}

// ── Platform identity ────────────────────────────────────────────────────────────────────

#[cfg(windows)]
mod win {
    use interprocess::os::windows::named_pipe::{pipe_mode, DuplexPipeStream};
    use interprocess::os::windows::security_descriptor::SecurityDescriptor;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_ACCESS_DENIED, HANDLE,
    };
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OVERLAPPED, SECURITY_IDENTIFICATION};
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, OpenMutexW, OpenProcess, OpenProcessToken,
        PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZATION_SYNCHRONIZE,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AllowSetForegroundWindow, MessageBoxW, MB_ICONWARNING, MB_OK, MB_SETFOREGROUND,
    };

    /// The current user's SID as a string (`S-1-5-21-…`), from the process token.
    pub fn user_sid() -> Option<String> {
        // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
        token_user_sid(unsafe { GetCurrentProcess() })
    }

    /// The SID of the user a process runs as, or `None` when it cannot be read (no such
    /// process, or one this user may not ask — an elevated one, possibly).
    pub fn process_user_sid(pid: u32) -> Option<String> {
        // SAFETY: a plain call; the handle is closed below on every path that obtained it.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if process.is_null() {
            return None;
        }
        let sid = token_user_sid(process);
        // SAFETY: a handle this function opened and nothing else uses.
        unsafe { CloseHandle(process) };
        sid
    }

    /// The SID of the user whose token `process` has.
    fn token_user_sid(process: HANDLE) -> Option<String> {
        // SAFETY: every out-pointer points at a local of the right type; the token handle is
        // closed and the string LocalFree'd on every path that obtained them; the buffer is
        // u64-backed, so it is aligned for TOKEN_USER and the SID it points into.
        unsafe {
            let mut token: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
                return None;
            }
            let mut len = 0u32;
            GetTokenInformation(token, TokenUser, std::ptr::null_mut(), 0, &mut len);
            let mut buf = vec![0u64; (len as usize).div_ceil(8).max(1)];
            let ok = GetTokenInformation(
                token,
                TokenUser,
                buf.as_mut_ptr().cast(),
                (buf.len() * 8) as u32,
                &mut len,
            );
            CloseHandle(token);
            if ok == 0 {
                return None;
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut wide: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut wide) == 0 || wide.is_null() {
                return None;
            }
            let n = (0..).take_while(|&i| *wide.add(i) != 0).count();
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(wide, n));
            LocalFree(wide.cast());
            Some(s)
        }
    }

    /// The Terminal Services session this process runs in.
    pub fn session_id() -> Option<u32> {
        let mut id = 0u32;
        // SAFETY: a valid out-pointer to a local.
        (unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut id) } != 0).then_some(id)
    }

    /// Lets `pid` bring a window to the front. Harmless when it is not needed.
    pub fn allow_foreground(pid: u32) {
        // SAFETY: no pointers; a refusal is only a return value.
        unsafe { AllowSetForegroundWindow(pid) };
    }

    /// The pipe's security: full access for `sid` and for SYSTEM, nothing for anybody else,
    /// and nothing inherited (`P`). Without it the pipe gets the creator's default, which
    /// grants read access to Everyone and to Anonymous — enough to connect.
    ///
    /// Written as SDDL and parsed by interprocess (`ConvertStringSecurityDescriptorTo…`), the
    /// form Windows itself documents security descriptors in.
    pub fn pipe_security(sid: &str) -> std::io::Result<SecurityDescriptor> {
        let sddl = widestring::U16CString::from_str(pipe_sddl(sid))
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        SecurityDescriptor::deserialize(&sddl)
    }

    /// The SDDL behind [`pipe_security`], on its own for the tests.
    pub fn pipe_sddl(sid: &str) -> String {
        format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)")
    }

    /// Opens the pipe as a client: read and write, overlapped (which is how interprocess
    /// reads and writes it), and at identification level — the server may learn who connected
    /// but may not act as them. Fails at once when the pipe does not exist or every instance
    /// is busy.
    pub fn open_pipe_file(path: &str) -> std::io::Result<std::fs::File> {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OVERLAPPED)
            // std adds SECURITY_SQOS_PRESENT, without which this flag would be ignored.
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(path)
    }

    /// [`open_pipe_file`], handed to interprocess for the reading and writing, and for the
    /// questions only a pipe handle can answer (the server's process and session).
    pub fn open_pipe(path: &str) -> std::io::Result<DuplexPipeStream<pipe_mode::Bytes>> {
        let file = open_pipe_file(path)?;
        DuplexPipeStream::try_from(std::os::windows::io::OwnedHandle::from(file))
            .map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Whether the process serving `stream` is this user's, in this session: the session from
    /// the pipe itself, the user from the server process's token. With no SID of our own to
    /// compare (a token that could not be read), the session alone decides.
    pub fn server_is_ours(stream: &DuplexPipeStream<pipe_mode::Bytes>, owner: Option<&str>) -> bool {
        let (Ok(theirs), Some(ours)) = (stream.server_session_id(), session_id()) else {
            return false;
        };
        if theirs != ours {
            return false;
        }
        match owner {
            None => true,
            Some(sid) => stream
                .server_process_id()
                .ok()
                .and_then(process_user_sid)
                .is_some_and(|theirs| theirs.eq_ignore_ascii_case(sid)),
        }
    }

    /// Whether a mutex of this name exists that this process may not open. See
    /// `held_where_we_cannot_open`.
    pub fn mutex_exists_but_denied(name: &str) -> bool {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: a NUL-terminated name that outlives the call; the handle is closed when one
        // is returned, and GetLastError is read straight after the call that set it.
        unsafe {
            let handle = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, wide.as_ptr());
            if !handle.is_null() {
                CloseHandle(handle);
                return false;
            }
            GetLastError() == ERROR_ACCESS_DENIED
        }
    }

    /// A message box of the system's own, in front: this process was just started by the
    /// user, so it may take the foreground.
    pub fn tell(title: &str, message: &str) {
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        let (title, message) = (wide(title), wide(message));
        // SAFETY: both strings are NUL-terminated and outlive the call; no owner window.
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONWARNING | MB_SETFOREGROUND,
            )
        };
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::{Path, PathBuf};

    /// Longest path a Unix-domain socket may have on macOS: `sun_path` is 104 bytes, and the
    /// last of them holds the NUL.
    const SOCKET_PATH_MAX: usize = 103;

    /// The folder the lock file and the socket go in: `~/Library/Application Support/
    /// AutomationPlatform` (the application's own fallback folder, see `portable.rs`), created
    /// readable by this user alone when it does not exist yet.
    ///
    /// Not the per-user temporary folder, which was the first choice: macOS empties
    /// `/var/folders/…/T/` of whatever has not been accessed for about three days, and a
    /// running copy does not touch its lock file or its socket after creating them. Once they
    /// were gone, a new start would find no lock (and take one: two hosts) and no socket.
    ///
    /// That folder remains the fallback for the two cases the preferred one cannot serve: a
    /// socket path longer than macOS allows (a very long account name), and a folder that
    /// cannot be created or is not this user's (no home folder, so `fallback_dir` answered a
    /// shared place).
    pub fn instance_dir(uid: u32, socket_file: &str) -> PathBuf {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        let preferred = crate::portable::fallback_dir();
        let fits = preferred.join(socket_file).as_os_str().len() <= SOCKET_PATH_MAX;
        let ours = |dir: &Path| {
            std::fs::metadata(dir).map(|m| m.is_dir() && m.uid() == uid).unwrap_or(false)
        };
        if fits {
            let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&preferred);
            if ours(&preferred) {
                return preferred;
            }
        }
        user_temp_dir()
    }

    /// The user's own temporary folder, `/var/folders/…/T/`: created by the system for each
    /// user, readable by nobody else, and short enough for a socket path.
    ///
    /// Asked of `confstr` rather than `$TMPDIR`, which a launch from a script may not carry and
    /// whose absence would make `temp_dir()` answer the shared `/tmp`. Only if `confstr` fails
    /// too does it fall back to `~/Library/Caches`, still per user.
    pub fn user_temp_dir() -> PathBuf {
        let mut buf = vec![0u8; 1024];
        // SAFETY: the buffer and its length match; confstr writes at most `len` bytes.
        let n = unsafe {
            libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, buf.as_mut_ptr().cast(), buf.len())
        };
        if n > 1 && n <= buf.len() {
            buf.truncate(n - 1); // n counts the terminating NUL
            return PathBuf::from(OsString::from_vec(buf));
        }
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library").join("Caches"))
            .unwrap_or_else(crate::portable::fallback_dir)
    }

    /// The executable a process runs, or `None` when there is no such process or it is not
    /// ours to ask about.
    pub fn process_path(pid: i32) -> Option<PathBuf> {
        let mut buf = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        // SAFETY: the buffer and its length match; proc_pidpath writes at most that many bytes
        // and returns how many, or 0 or less on failure.
        let n = unsafe { libc::proc_pidpath(pid, buf.as_mut_ptr().cast(), buf.len() as u32) };
        if n <= 0 {
            return None;
        }
        buf.truncate(n as usize);
        Some(PathBuf::from(OsString::from_vec(buf)))
    }

    /// An alert of the system's own (`CFUserNotificationDisplayAlert`), which needs no
    /// application object: this runs before wxWidgets exists. Returns when it is dismissed.
    pub fn tell(title: &str, message: &str) {
        use objc2_core_foundation::{
            kCFUserNotificationCautionAlertLevel, CFOptionFlags, CFString, CFUserNotification,
        };
        let (title, message) = (CFString::from_str(title), CFString::from_str(message));
        let mut response: CFOptionFlags = 0;
        // SAFETY: the strings live until the call returns; every `None` is a documented
        // "use the default" (no icon, sound or localisation, the standard OK button, no other
        // buttons), and `response` is a valid out-pointer. A timeout of 0 means none.
        unsafe {
            CFUserNotification::display_alert(
                0.0,
                kCFUserNotificationCautionAlertLevel,
                None,
                None,
                None,
                Some(&*title),
                Some(&*message),
                None,
                None,
                None,
                &mut response,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_names_carry_the_user_and_the_session() {
        let n = windows_names("S-1-5-21-1004336348-1177238915-682003330-1001", 2);
        assert_eq!(n.lock, "AutomationPlatform-S-1-5-21-1004336348-1177238915-682003330-1001-s2");
        assert_eq!(n.lock_dir, None);
        assert_eq!(n.endpoint, Endpoint::Pipe(n.lock.clone()));
        // The log names them without the SID, which identifies the account.
        assert_eq!(n.for_log, r"the mutex and the pipe \\.\pipe\AutomationPlatform-<user>-s2");
        assert!(!n.endpoint_for_log().contains("S-1-5"), "{}", n.endpoint_for_log());
        // Another session of the same user is another name: the pipe namespace is shared.
        assert_ne!(windows_names("S-1-5-21-1-2-3-1001", 1), windows_names("S-1-5-21-1-2-3-1001", 2));
        // Another user in the same session is another name too.
        assert_ne!(windows_names("S-1-5-21-1-2-3-1001", 1), windows_names("S-1-5-21-1-2-3-1002", 1));
    }

    #[test]
    fn a_name_cannot_grow_a_namespace_or_odd_characters() {
        // A backslash would turn the mutex name into `Global\…`-style path; the fallback
        // identity is a user name, which may hold anything.
        let n = windows_names(r"user-DOMAIN\Some One Ä", 1);
        assert_eq!(n.lock, "AutomationPlatform-user-DOMAIN_Some_One__-s1");
        assert!(!n.lock.contains('\\'));
    }

    #[test]
    fn unix_names_put_lock_and_socket_in_the_folder_given() {
        let dir = Path::new("/var/folders/ab/xyz/T");
        let n = unix_names(501, dir);
        assert_eq!(n.lock, "AutomationPlatform-501.lock");
        assert_eq!(n.lock_dir.as_deref(), Some(dir));
        assert_eq!(n.endpoint, Endpoint::Socket(dir.join("AutomationPlatform-501.sock")));
        // Well inside the 104 bytes a macOS socket path may have.
        assert!(format!("{}", n.endpoint).len() < 104);
        assert!(n.for_log.contains("AutomationPlatform-501.lock"), "{}", n.for_log);
        assert!(n.for_log.contains("AutomationPlatform-501.sock"), "{}", n.for_log);
    }

    #[test]
    fn the_log_writes_the_home_folder_as_a_tilde() {
        // Whatever HOME is here (it may be unset on Windows), a path outside it is left alone.
        assert_eq!(home_as_tilde(Path::new("/var/folders/ab/T/x.sock")), "/var/folders/ab/T/x.sock");
        if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
            let inside = PathBuf::from(home).join("Library").join("x.sock");
            let shown = home_as_tilde(&inside);
            assert!(shown.starts_with("~/"), "{shown}");
            assert!(shown.ends_with("x.sock"), "{shown}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn this_users_names_are_built_from_the_sid() {
        let n = names();
        assert!(n.lock.starts_with("AutomationPlatform-S-1-"), "{}", n.lock);
        assert!(n.lock.contains("-s"), "{}", n.lock);
        // The SID the pipe is restricted to is the one in the name.
        let sid = n.owner.clone().expect("this process's SID");
        assert!(n.lock.contains(&sid), "{} / {sid}", n.lock);
    }

    #[cfg(windows)]
    #[test]
    fn the_pipe_admits_the_user_and_system_only() {
        assert_eq!(win::pipe_sddl("S-1-5-21-1-2-3-1001"), "D:P(A;;GA;;;S-1-5-21-1-2-3-1001)(A;;GA;;;SY)");
        // Windows accepts it.
        assert!(win::pipe_security("S-1-5-21-1-2-3-1001").is_ok());
    }

    #[test]
    fn the_decision() {
        use Answer::*;
        use LockState::*;
        // Ours.
        assert_eq!(step(Free, Nothing, false, false, false), Step::Proceed);
        // The running copy took the request, whatever the lock said.
        assert_eq!(step(Held, Shown, false, false, false), Step::Handed);
        assert_eq!(step(Unavailable, Shown, false, false, false), Step::Handed);
        // A different version answered: nothing to wait for, and nothing to start beside.
        assert_eq!(step(Held, Unknown, false, false, false), Step::GiveUp);
        assert_eq!(step(Unavailable, Unknown, false, false, false), Step::GiveUp);
        // A broken lock is not a reason to stay away…
        assert_eq!(step(Unavailable, Nothing, false, false, false), Step::Proceed);
        assert_eq!(step(Unavailable, Refused, false, false, false), Step::Proceed);
        // …but a copy that says it is quitting is waited for, and then started beside.
        assert_eq!(step(Unavailable, Quitting, false, false, false), Step::Wait);
        assert_eq!(step(Unavailable, Quitting, true, false, false), Step::Proceed);
        // Held and silent, not ours, or shutting down: wait for it.
        assert_eq!(step(Held, Nothing, false, true, false), Step::Wait);
        assert_eq!(step(Held, Refused, false, false, false), Step::Wait);
        assert_eq!(step(Held, Quitting, false, false, false), Step::Wait);
        // Timed out: a lock file may be a leftover, taken over once…
        assert_eq!(step(Held, Nothing, true, true, false), Step::TakeOver);
        assert_eq!(step(Held, Nothing, true, true, true), Step::GiveUp);
        // …a mutex cannot be, nor a lock file whose pid runs this application: its owner is
        // alive, and we must not start beside it.
        assert_eq!(step(Held, Nothing, true, false, false), Step::GiveUp);
        assert_eq!(step(Held, Quitting, true, false, false), Step::GiveUp);
        assert_eq!(step(Held, Refused, true, false, false), Step::GiveUp);
    }

    #[test]
    fn the_protocol_lines() {
        assert_eq!(reply_to("show-manager\n", false), ANSWER_SHOWN);
        assert_eq!(reply_to("show-manager\r\n", true), ANSWER_QUITTING);
        assert_eq!(reply_to("something else\n", false), ANSWER_UNKNOWN);
        assert_eq!(parse_answer("shown\n"), Answer::Shown);
        assert_eq!(parse_answer("quitting\n"), Answer::Quitting);
        assert_eq!(parse_answer("unknown-request\n"), Answer::Unknown);
        assert_eq!(parse_answer("gibberish\n"), Answer::Nothing);
        assert_eq!(parse_answer(""), Answer::Nothing);
    }

    #[test]
    fn a_start_that_gives_up_says_why_in_both_places() {
        let names = test_names("texts");
        let texts: Vec<GaveUp> = [Answer::Nothing, Answer::Quitting, Answer::Unknown, Answer::Refused]
            .into_iter()
            .map(|a| gave_up(&names, a, WAIT))
            .collect();
        for g in &texts {
            assert!(g.log_line.contains(&names.for_log), "{}", g.log_line);
            assert!(g.log_line.contains(&format!("pid {}", std::process::id())), "{}", g.log_line);
            assert!(g.message.contains("Automation Platform"), "{}", g.message);
        }
        // Four different accounts, in the log and in the message.
        for (i, a) in texts.iter().enumerate() {
            for b in &texts[i + 1..] {
                assert_ne!(a.log_line, b.log_line);
                assert_ne!(a.message, b.message);
            }
        }
        assert!(texts[0].message.contains("did not respond"), "{}", texts[0].message);
        assert!(texts[1].message.contains("still closing"), "{}", texts[1].message);
        assert!(texts[2].message.contains("different version"), "{}", texts[2].message);
        assert!(texts[3].message.contains("administrator"), "{}", texts[3].message);
    }

    /// Held by every test that asks a listener for the window: the request is one flag for
    /// the whole process, and tests run side by side, so one test's request would otherwise
    /// be another's surprise.
    static WINDOW_FLAG: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn window_flag() -> std::sync::MutexGuard<'static, ()> {
        WINDOW_FLAG.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A lock for the tests, held or free as it is told.
    struct FakeLock(bool);
    impl Lock for FakeLock {
        fn another_running(&self) -> bool {
            self.0
        }
    }

    /// Names no running application uses, so the tests never talk to a real copy. Short: a
    /// macOS socket path has 104 bytes, and the per-user temporary folder takes half.
    ///
    /// On Windows with this user's SID, as the application has it, so the pipe the tests
    /// listen on is restricted the way the real one is and the second start checks the first
    /// the way it would check a real one.
    fn test_names(tag: &str) -> Names {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let unique = format!("ap-test-{tag}-{}-{nanos}", std::process::id());
        #[cfg(windows)]
        {
            Names {
                lock: unique.clone(),
                lock_dir: None,
                endpoint: Endpoint::Pipe(unique.clone()),
                for_log: unique,
                owner: win::user_sid(),
            }
        }
        #[cfg(not(windows))]
        {
            let dir = std::env::temp_dir();
            Names {
                lock: format!("{unique}.lock"),
                lock_dir: Some(dir.clone()),
                endpoint: Endpoint::Socket(dir.join(format!("{unique}.sock"))),
                for_log: unique,
                owner: None,
            }
        }
    }

    #[test]
    fn a_lock_that_is_released_while_waiting_is_taken() {
        // Held twice (the owner is shutting down, nobody listens), then free.
        let names = test_names("released");
        let mut calls = 0;
        let claim = claim_with(
            &names,
            |_| {
                calls += 1;
                Some(FakeLock(calls <= 2))
            },
            Duration::from_secs(5),
            Duration::from_millis(10),
            Duration::from_millis(500),
            false,
        );
        assert!(matches!(claim, Claim::Primary(_)));
        assert_eq!(calls, 3);
    }

    #[test]
    fn a_held_mutex_that_never_answers_is_given_up_on() {
        let names = test_names("silent");
        let claim = claim_with(
            &names,
            |_| Some(FakeLock(true)),
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(200),
            false,
        );
        match claim {
            Claim::GaveUp(g) => {
                assert!(g.log_line.contains(&names.for_log), "{}", g.log_line);
                assert!(g.log_line.contains("did not answer"), "{}", g.log_line);
            }
            _ => panic!("expected GaveUp"),
        }
    }

    #[test]
    fn a_lock_file_nothing_answers_for_is_taken_over_once() {
        // The take-over branch, on every platform: a real lock file in a folder of its own,
        // naming a process that is not this application (pid 1: launchd on a Mac), and a lock
        // that is held for as long as the file exists — which is what wxWidgets reports.
        let mut names = test_names("stale");
        let dir = std::env::temp_dir().join(format!("{}-dir", names.lock));
        std::fs::create_dir_all(&dir).unwrap();
        names.lock_dir = Some(dir.clone());
        let file = dir.join(&names.lock);
        std::fs::write(&file, b"1\0").unwrap();
        let mut calls = 0;
        let claim = claim_with(
            &names,
            |n| {
                calls += 1;
                Some(FakeLock(n.lock_dir.as_ref().unwrap().join(&n.lock).exists()))
            },
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(200),
            true,
        );
        let primary = match claim {
            Claim::Primary(p) => p,
            _ => panic!("the leftover lock should have been taken over"),
        };
        assert!(!file.exists(), "the leftover was removed");
        assert!(primary.notes.iter().any(|n| n.contains("took over")), "{:?}", primary.notes);
        assert!(calls >= 3, "held, timed out and taken over, then free: {calls}");
        drop(primary);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A lock file whose pid runs this application is a live copy that does not answer, not
    /// a leftover: never taken over. Only a Mac can tell (`proc_pidpath`).
    #[cfg(target_os = "macos")]
    #[test]
    fn a_lock_file_owned_by_this_application_is_not_taken_over() {
        let mut names = test_names("live");
        let dir = std::env::temp_dir().join(format!("{}-dir", names.lock));
        std::fs::create_dir_all(&dir).unwrap();
        names.lock_dir = Some(dir.clone());
        let file = dir.join(&names.lock);
        std::fs::write(&file, format!("{}\0", std::process::id())).unwrap();
        let claim = claim_with(
            &names,
            |n| Some(FakeLock(n.lock_dir.as_ref().unwrap().join(&n.lock).exists())),
            Duration::from_millis(100),
            Duration::from_millis(10),
            Duration::from_millis(200),
            true,
        );
        assert!(matches!(claim, Claim::GaveUp(_)));
        assert!(file.exists(), "a live copy's lock file must stay");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_start_reaches_the_first_and_the_window_is_requested() {
        // The whole conversation over a real pipe (a socket on macOS), with a name nobody
        // else uses: the first copy claims a free lock and listens, the second finds it held
        // and asks. Nothing here opens a window; the request is only a flag.
        let _flag = window_flag();
        let names = test_names("conversation");
        let first = match claim_with(
            &names,
            |_| Some(FakeLock(false)),
            Duration::from_secs(5),
            Duration::from_millis(10),
            Duration::from_secs(2),
            false,
        ) {
            Claim::Primary(p) => p,
            _ => panic!("the first copy should have the lock"),
        };
        assert!(first.notes.iter().all(|n| !n.contains("cannot listen")), "{:?}", first.notes);
        let _ = take_show_request();
        let second = claim_with(
            &names,
            |_| Some(FakeLock(true)),
            Duration::from_secs(5),
            Duration::from_millis(10),
            Duration::from_secs(2),
            false,
        );
        assert!(matches!(second, Claim::Shown));
        assert!(take_show_request(), "the running copy should have been asked for its window");

        // Once the first copy is quitting, a second start is told so — which `step` turns into
        // waiting for the lock — and no window is requested from a loop that has ended.
        first.stop_serving();
        assert_eq!(contact(&names, Duration::from_secs(2)), Answer::Quitting);
        assert!(!take_show_request());
    }

    #[test]
    fn a_copy_that_quit_leaves_the_name_free_for_the_next() {
        // What a restart does: the running copy quits, and the next one must be able to
        // listen on the same name at once — on Windows that fails while any instance of the
        // pipe is still open, which a listener thread left blocked in accept used to keep.
        let _flag = window_flag();
        let names = test_names("restart");
        let claim_free = || {
            claim_with(
                &names,
                |_| Some(FakeLock(false)),
                Duration::from_secs(5),
                Duration::from_millis(10),
                Duration::from_secs(2),
                false,
            )
        };
        let first = match claim_free() {
            Claim::Primary(p) => p,
            _ => panic!("the first copy should have the lock"),
        };
        assert!(first.listening.is_some());
        drop(first);
        let started = Instant::now();
        let second = match claim_free() {
            Claim::Primary(p) => p,
            _ => panic!("the second copy should have the lock"),
        };
        assert!(second.listening.is_some(), "{:?}", second.notes);
        // At once, not after the retry: the first copy stopped its listener before it let go.
        assert!(started.elapsed() < Duration::from_millis(500), "{:?}", started.elapsed());
        assert!(second.notes.iter().all(|n| !n.contains("still taken")), "{:?}", second.notes);
        // And a third start finds the second.
        let _ = take_show_request();
        let third = claim_with(
            &names,
            |_| Some(FakeLock(true)),
            Duration::from_secs(5),
            Duration::from_millis(10),
            Duration::from_secs(2),
            false,
        );
        assert!(matches!(third, Claim::Shown));
        assert!(take_show_request());
    }

    /// A second start that finds the name served by somebody who is not this user sends
    /// nothing and is refused — here the server is this very process, and the second start is
    /// told to expect another account (SYSTEM's SID), which is what a squatter looks like from
    /// its side. No window is requested.
    #[cfg(windows)]
    #[test]
    fn a_server_that_is_not_this_user_is_not_asked() {
        let _flag = window_flag();
        let names = test_names("squatted");
        let first = match claim_with(
            &names,
            |_| Some(FakeLock(false)),
            Duration::from_secs(5),
            Duration::from_millis(10),
            Duration::from_secs(2),
            false,
        ) {
            Claim::Primary(p) => p,
            _ => panic!("the first copy should have the lock"),
        };
        let _ = take_show_request();
        let expecting_someone_else = Names { owner: Some("S-1-5-18".into()), ..names.clone() };
        assert_eq!(contact(&expecting_someone_else, Duration::from_secs(2)), Answer::Refused);
        assert!(!take_show_request(), "nothing may have been sent");
        // The same server, expected as what it is, is asked.
        assert_eq!(contact(&names, Duration::from_secs(2)), Answer::Shown);
        assert!(take_show_request());
        drop(first);
    }

    #[test]
    fn connections_beyond_the_cap_are_closed_unanswered() {
        // Clients that connect and say nothing hold an answer thread each; the listener
        // answers at most MAX_ANSWERING at once and a real start still gets through once they
        // are gone.
        let _flag = window_flag();
        let names = test_names("cap");
        let first = match claim_with(
            &names,
            |_| Some(FakeLock(false)),
            Duration::from_secs(5),
            Duration::from_millis(10),
            Duration::from_secs(2),
            false,
        ) {
            Claim::Primary(p) => p,
            _ => panic!("the first copy should have the lock"),
        };
        // Clients that connect and send nothing.
        let silent: Vec<_> = (0..MAX_ANSWERING + 2)
            .filter_map(|_| {
                let conn = wake(&names.endpoint).ok();
                std::thread::sleep(Duration::from_millis(30));
                conn
            })
            .collect();
        assert!(!silent.is_empty(), "the test clients should have connected");
        let answering = first.listening.as_ref().unwrap().answering.load(Ordering::SeqCst);
        assert!(answering <= MAX_ANSWERING, "{answering}");
        drop(silent);
        // The silent ones read EOF and end; a real request is answered again.
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut answer = Answer::Nothing;
        while Instant::now() < deadline {
            answer = contact(&names, Duration::from_secs(1));
            if answer == Answer::Shown {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(answer, Answer::Shown);
        let _ = take_show_request();
    }
}
