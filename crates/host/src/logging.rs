//! Diagnostics to `<app folder>/automation-platform.log` (see [`crate::portable`]) —
//! **not** stdout/stderr: a screen reader reads the focused terminal, so console output
//! would be spoken aloud.
//!
//! The log is the only instrument we have on a machine we cannot touch. A tester on macOS
//! is blind, remote, and cannot describe a screen; whatever is not in this file did not
//! happen as far as we are concerned. That shapes three things here that a smaller logger
//! would not have: a **session header** that states the ground truth of the machine before
//! anything can go wrong, a **trace level** that can be turned on without a new build, and
//! **rotation**, because a log nobody can send is a log nobody has.
//!
//! It is also where the libraries we are built on report. They speak through the `log`
//! facade (ureq, rustls, rusty-xinput, wxdragon, symphonia under rodio, os_info, among others)
//! or through `tracing` (ort, and ONNX Runtime's own messages, which ort forwards into it), and
//! until something listens both are thrown away: a TLS alert from rustls, or a panic wxdragon
//! caught in its init callback, was written nowhere. [`init`] installs the listener; see
//! [`DependencyLog`] for what it lets through and how it keeps one library from flooding the
//! file.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::portable;

static LOG: Mutex<Option<Sink>> = Mutex::new(None);

/// Roll over at 8 MB. Chosen to stay attachable to an email or an issue: a trace-level
/// session can produce that in an afternoon, and the previous file is kept because the
/// interesting part is often the startup that happened before the symptom.
const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// When this session's header was written (seconds since 1970, the log's own clock), for the
/// line that opens a file the session continues in after a rotation. 0 before [`init`].
static SESSION_STARTED: AtomicU64 = AtomicU64::new(0);

/// The header's `app folder` line, kept for the files the session continues in: written once
/// by [`header`], so repeating it costs no second look at whether the folder can be written.
static APP_FOLDER_LINE: OnceLock<String> = OnceLock::new();

/// The open log file, and how many bytes it holds.
///
/// **Rotated within the session, by the bytes written.** It used to be rotated only by
/// [`init`], at start, on the reasoning that its size only matters between sessions — which
/// stops being true once a session lasts days, and this application is left running for days
/// by users who cannot see that a log has grown to hundreds of megabytes. So the count is kept
/// here, seeded from the file's length when it is opened and raised by every line written, and
/// no filesystem call is made per line to learn it. Past [`MAX_BYTES`] the file is renamed to
/// `.log.1` — the one generation kept, as at start — and a new one is opened, whose first lines
/// say where the rest of the session is and which build wrote it ([`continuation`]).
///
/// **A rename that fails costs no line.** The rename runs with our handle closed. When it is
/// refused — another program has the file open without sharing its deletion, a viewer, a
/// backup tool, a `Get-Content -Wait` — the same file is opened again for appending, the line
/// that could not be written into a new file goes into it, and the rename is tried again after
/// another sixteenth of the limit, with one line saying so at every try. For as long as such a
/// program keeps the file open the file grows past the limit: nothing is lost, and the next try
/// after it lets go moves it. (Copying the file away and emptying it instead was not taken: a
/// line another copy of the application appends between the copy and the emptying would be
/// lost, and a reader that follows the file is not told it was emptied.)
///
/// **An opening that fails is tried again later, not at every line.** When the new file (or the
/// old one again) cannot be opened, nothing is written until it can, and the next try comes
/// after another sixteenth of the limit's worth of lines — counted though not written — rather
/// than at every line, which would put two failing filesystem calls on every thread that logs.
/// A rename that had gone through is remembered, so the file that finally opens starts with the
/// continuation lines rather than with a refusal that did not happen.
///
/// **Another copy writing the same file does not lose this one's lines.** A headless run beside
/// the application — which the single-instance guard lets through, for CI and the development
/// tools — opens the same file and counts only its own lines, so it can move the file first;
/// this copy's handle then follows the file to `.log.1` and goes on writing there. Renaming the
/// path at this copy's own count would then move the other copy's new file over `.log.1` — the
/// very file this copy has been writing into since, and the generation before it. So before the
/// rename, the path is checked to still name the open file ([`Fs::still_names`], through the
/// established `same-file` crate); when it does not, nothing is moved, and this copy goes on in
/// the file the path names now, after a line saying so and the continuation lines. The other
/// way round stays possible — the other copy moving the file this one writes into, and later
/// moving its own over it — but only after it has written 8 MB of its own, and a headless run
/// seldom writes that much.
///
/// **Where it runs.** On the thread whose line took the file past the limit, under the log's
/// lock: one check, one rename, one open and the continuation lines (five at most, and one line
/// more when another copy had moved the file), once per 8 MB.
///
/// **Why not `file-rotate`.** `file-rotate` (0.8, MIT, the established crate for this) was
/// checked against the first rule above and does not meet it: its `write` drops the handle and
/// returns the rename's error before it writes, so every line is lost for as long as the
/// rename fails; it `assert!`s on the files it expects during a rotation, and a panic inside
/// the logger would poison its lock and silence the log for the rest of the session; it has no
/// hook for the continuation lines; and it brings `chrono`, which nothing else here needs.
struct Sink {
    path: PathBuf,
    /// `None` only between closing the file for a rotation and opening the next one, or when
    /// opening it again failed (then nothing is written until a later rotation succeeds).
    file: Option<File>,
    /// Bytes in the file: its length when it was opened, plus every line written since. While
    /// no file is open, the lines that could not be written, so that the next try to open one
    /// comes a sixteenth of the limit later.
    bytes: u64,
    /// Rotate once `bytes` reaches this: `limit`, or later after a rename was refused or an
    /// opening failed.
    rotate_at: u64,
    limit: u64,
    /// The file was renamed to `.log.1` and the new one could not be opened yet: the next
    /// rotation only opens, and the file it opens begins with the continuation lines.
    moved: bool,
}

/// How a rotation reaches the filesystem: the real calls, or a test's.
trait Fs {
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()>;
    fn open(&mut self, path: &Path) -> std::io::Result<File>;
    /// Whether `path` still names the open `file` — `false` once something else moved it away.
    fn still_names(&mut self, path: &Path, file: &File) -> bool;
}

struct RealFs;

impl Fs for RealFs {
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }
    fn open(&mut self, path: &Path) -> std::io::Result<File> {
        OpenOptions::new().create(true).append(true).open(path)
    }
    /// The volume and file index on Windows, the device and inode elsewhere. Whatever cannot be
    /// asked counts as still ours, so the file is moved as it always was; nothing at the path
    /// counts as moved away.
    fn still_names(&mut self, path: &Path, file: &File) -> bool {
        let Ok(ours) = file.try_clone().and_then(same_file::Handle::from_file) else {
            return true;
        };
        match same_file::Handle::from_path(path) {
            Ok(there) => there == ours,
            Err(e) => e.kind() != std::io::ErrorKind::NotFound,
        }
    }
}

impl Sink {
    /// Opens `path` for appending, counting what it already holds. `None` when it cannot be
    /// opened at all.
    fn open(path: PathBuf, limit: u64) -> Option<Sink> {
        let file = RealFs.open(&path).ok()?;
        let bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
        Some(Sink { path, file: Some(file), bytes, rotate_at: limit, limit, moved: false })
    }

    /// Writes one line — already composed, newline included — in ONE `write_all` (see
    /// [`line`]), and says whether the file is now due for rotation. With no file open the line
    /// is lost, and counted towards the next try to open one.
    fn write(&mut self, text: &[u8]) -> bool {
        match self.file.as_mut() {
            Some(file) => {
                if file.write_all(text).is_ok() {
                    self.bytes += text.len() as u64;
                }
                let _ = file.flush();
            }
            None => self.bytes += text.len() as u64,
        }
        self.due()
    }

    /// A sixteenth of the limit: how much later a refused rename or a failed opening is tried
    /// again.
    fn step(&self) -> u64 {
        (self.limit / 16).max(1)
    }

    fn due(&self) -> bool {
        self.bytes >= self.rotate_at
    }

    /// Where the previous file goes: `automation-platform.log.1` beside it.
    fn previous(&self) -> PathBuf {
        self.path.with_extension("log.1")
    }

    /// Renames the file to `.log.1` (replacing an older one) and opens a new one, which starts
    /// with `continued`. When the rename is refused, keeps appending to the same file and tries
    /// again after another sixteenth of the limit. When no file can be opened, tries again after
    /// another sixteenth of the limit, remembering whether the rename went through. The lines of
    /// `continued` go into the new file once one is open. When the path no longer names the open
    /// file — another copy writing the same log moved it already — nothing is renamed: the file
    /// the path names now is opened, and `continued` goes into it after a line saying why.
    fn rotate(&mut self, continued: &[String]) {
        self.rotate_with(continued, &mut RealFs);
    }

    fn rotate_with(&mut self, continued: &[String], fs: &mut dyn Fs) {
        let step = self.step();
        let mut refused = None;
        let mut elsewhere = false;
        if !self.moved {
            elsewhere = self.file.as_ref().is_some_and(|f| !fs.still_names(&self.path, f));
            self.file = None;
            if elsewhere {
                // Moved already, by the other copy (see [`Sink`]): moving the path again would
                // put that copy's new file over the one this copy's lines went into.
                self.moved = true;
            } else {
                match fs.rename(&self.path, &self.previous()) {
                    Ok(()) => self.moved = true,
                    Err(e) => refused = Some(e),
                }
            }
        }
        match fs.open(&self.path) {
            Ok(file) => {
                self.bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
                self.file = Some(file);
            }
            Err(_) => {
                // Nothing to write into: the lines until the next try are lost, and counted.
                self.file = None;
                self.rotate_at = self.bytes.saturating_add(step);
                return;
            }
        }
        if std::mem::take(&mut self.moved) {
            self.rotate_at = self.limit;
            if elsewhere {
                let text = stamped(
                    "host",
                    &format!(
                        "the log had been moved away before this session moved it — to {} by \
                         another copy of the application writing to it too (a headless run beside \
                         this one), or by hand — so it is not moved again: this session goes on \
                         here",
                        self.previous().display()
                    ),
                );
                self.write(text.as_bytes());
            }
            for l in continued {
                self.write(l.as_bytes());
            }
        } else if let Some(e) = refused {
            self.rotate_at = self.bytes.saturating_add(step);
            let text = stamped(
                "host",
                &format!(
                    "the log could not be moved to {} to start a new file ({e}); it goes on \
                     growing here, and the move is tried again after another {} KB",
                    self.previous().display(),
                    step / 1024
                ),
            );
            self.write(text.as_bytes());
        }
        // Neither is impossible: a rotation that finds the file not moved either moves it or is
        // refused.
    }
}

/// The lines a file the session continues in begins with, in this order: where the rest of the
/// session is and when it started; the build, so a file that is sent on its own can still be
/// matched to it; on macOS from an `.app`, the header's `translocated` line, which says why the
/// file is in the folder it is in (beside the original `.app` of a translocated copy, or in
/// Application Support when that original was not found); the header's `app folder` line, which
/// that one refers to and which says whether the folder can be written; and the settings that
/// are on. Five lines at most.
fn continuation(previous: &Path) -> Vec<String> {
    let started = SESSION_STARTED.load(Ordering::Relaxed);
    let mut lines = vec![
        stamped(
            "host",
            &format!(
                "log continued: this session started at {started} (the clock these lines begin \
                 with), and what it wrote before this line is in {} — a file is started afresh \
                 every {} MB",
                previous.display(),
                MAX_BYTES / (1024 * 1024)
            ),
        ),
        stamped("host", &crate::build_info::log_line(env!("CARGO_PKG_VERSION"))),
    ];
    // Decided once, before the log was opened (portable::base_dir), so reading it here costs
    // nothing and cannot differ from the header's.
    if let Some(t) = portable::translocation().report() {
        lines.push(stamped("host", &format!("translocated: {t}")));
    }
    if let Some(folder) = APP_FOLDER_LINE.get() {
        lines.push(stamped("host", folder));
    }
    let on = crate::appcfg::active();
    if !on.is_empty() {
        lines.push(stamped("host", &format!("settings on: {}", on.join(", "))));
    }
    lines
}

/// One line as the file holds it: `<seconds since 1970> [scope] message`, newline included.
fn stamped(scope: &str, msg: &str) -> String {
    format!("{} [{scope}] {msg}\n", epoch_secs())
}

/// Log file path. Beside the application, or — only if that is not writable — in the
/// per-user fallback, because a tester with no log has nothing to send.
pub fn log_path() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let base = portable::base_dir();
        let dir = if portable::is_writable(base) {
            base.to_path_buf()
        } else {
            let alt = portable::fallback_dir();
            let _ = std::fs::create_dir_all(&alt);
            alt
        };
        dir.join("automation-platform.log")
    })
}

/// Is trace logging on?
///
/// Owned by [`crate::appcfg`], which loads it from the settings file and lets the settings
/// dialog change it while the application runs. This is the read every `trace` call makes,
/// so it has to stay a plain atomic load.
pub fn is_trace() -> bool {
    crate::appcfg::trace()
}

/// Opens the log file (append) and writes the session header. Silent on failure.
///
/// A file past [`MAX_BYTES`] is rotated here before the session starts, so a session begins
/// in a file of its own when the last one filled it; within the session [`Sink`] rotates by
/// the bytes it writes, without a filesystem call per line.
///
/// The dependency listener goes in first, so that anything a library says while the header
/// is being gathered (os_info, when it cannot read the system version) lands in the log too.
/// Installing it twice is harmless — `log` takes the first logger and refuses the rest.
///
/// Does nothing while this process is [outside](set_outside): a start that may turn out to be
/// a second copy must not write a session header into the running copy's session, nor rotate
/// the file under it.
pub fn init() {
    if OUTSIDE.load(Ordering::SeqCst) {
        return;
    }
    install_dependency_log();
    let path = log_path();
    if std::fs::metadata(path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        // One generation back, overwritten. Two files is enough to see "it worked
        // yesterday"; more is a folder the user has to explain.
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    if let Some(sink) = Sink::open(path.to_path_buf(), MAX_BYTES) {
        if let Ok(mut guard) = LOG.lock() {
            *guard = Some(sink);
        }
    }
    SESSION_STARTED.store(epoch_secs(), Ordering::Relaxed);
    header();
}

/// The state of the machine, written before anything can go wrong.
///
/// Every line here has been the answer to a support question at least once in this
/// project's short life: which build is running, where it is writing, whether it is even
/// looking in the right place for modules.
fn header() {
    line("host", "session start");
    // `version 0.1.0, build 6c95c8b`: the build is the commit the download was made from, so a
    // report can be matched to it. See build_info for when a second commit follows it.
    line("host", &crate::build_info::log_line(env!("CARGO_PKG_VERSION")));
    // Nothing is said when there is no file: a package made with its own executable has none,
    // and that executable names its build itself. Only a macOS download whose executable CI
    // reused carries the file; a copy of one that cannot find it (moved away from its folder,
    // or run translocated with no original found, which the `translocated` line below says)
    // names its executable's older commit, and its README.txt still names the download's. A
    // file that is there and not used is said, with why: most often one left over from an
    // older download that this one was unpacked over, which names another executable (see
    // build_info::read).
    if let Some(problem) = crate::build_info::package_problem() {
        line("host", &format!("{problem}; the build above is the executable's own"));
    }
    line("host", &os_line(&os_info::get()));
    match std::env::current_exe() {
        Ok(p) => line("host", &format!("executable {}", p.display())),
        Err(e) => line("host", &format!("executable unknown: {e}")),
    }
    // Whether macOS runs a translocated copy of the .app, and which folder that made the app
    // folder below. Decided before anything else read the folder (see portable::base_dir) and
    // only written now, because the log's own path depends on it. On macOS from an .app only;
    // elsewhere the question does not arise and nothing is said.
    if let Some(t) = portable::translocation().report() {
        line("host", &format!("translocated: {t}"));
    }
    let base = portable::base_dir();
    let folder = format!(
        "app folder {} ({})",
        base.display(),
        if portable::is_writable(base) { "writable" } else { "NOT WRITABLE" }
    );
    line("host", &folder);
    // Repeated by every file the session continues in (see `continuation`).
    let _ = APP_FOLDER_LINE.set(folder);
    // Where installed modules are looked for, and what is there. An application moved away
    // from the folder it came in, or one run from a copy whose original was not found, reads
    // a folder with nothing in it — and loading nothing is otherwise a silent, legitimate state.
    // Only the state: whether this session reads the folder at all is not known yet (module
    // folders on the command line replace it), so the `no modules to load` line says what an
    // absent one means.
    let modules = portable::modules_dir();
    line(
        "host",
        &format!(
            "modules folder {}: {}",
            modules.display(),
            portable::look_into(&modules).modules_state()
        ),
    );
    line("host", &format!("log {}", log_path().display()));
    // Every application setting that is on, and where each came from — the settings file
    // or an environment variable forcing it. It belongs in the header because it changes how
    // the rest of the log should be read, and because "why is this log enormous" and "why is
    // there no detail in it" are both answered here.
    let on = crate::appcfg::active();
    if !on.is_empty() {
        line("host", &format!("settings on: {}", on.join(", ")));
    }
}

/// The header's OS line: `os windows x86_64 — Windows 10.0.26220 (Windows 11 Professional)
/// [64-bit]`.
///
/// The first two words are what this executable was built for, and stay as they were because
/// CI greps for `os macos`. After the dash is what the machine says it is, from `os_info`:
/// the version on both systems (on macOS it decides whether a capture API still works at all,
/// and asking the tester to read it out is asking them to navigate a settings pane by voice),
/// and on Windows the edition too. This used to be the `OS` environment variable, which on
/// every Windows machine ever made says `Windows_NT` and nothing else.
///
/// os_info reads the version through `RtlGetVersion` on Windows, which does not lie the way
/// `GetVersionEx` does to an executable without a compatibility manifest, and from
/// SystemVersion.plist on macOS (with `sw_vers` behind it), which is what the hand-written
/// version here used to ask.
///
/// One more word when os_info names another architecture than the one the executable was
/// built for, because capture and input timings measured under emulation are not the
/// machine's own. That is less often than it sounds. os_info asks `GetNativeSystemInfo` on
/// Windows, which tells a 32-bit build under WOW64 the truth (`, machine x86_64`) but, by
/// Microsoft's account of x64 emulation, shows an x86_64 build emulated on Windows on ARM the
/// x64 machine it emulates (only `IsWow64Process2` names the ARM64 machine; never tried here,
/// for want of such a machine). And it asks `uname` on macOS, which tells a process translated
/// by Rosetta that the machine is x86_64. The two emulated cases this project could meet
/// therefore most likely say nothing here.
fn os_line(info: &os_info::Info) -> String {
    let mut s = format!("os {} {} — {info}", std::env::consts::OS, std::env::consts::ARCH);
    if let Some(machine) = info.architecture() {
        if !same_arch(machine, std::env::consts::ARCH) {
            s.push_str(&format!(", machine {machine}"));
        }
    }
    s
}

/// Whether os_info's name for an architecture and Rust's name for one mean the same thing.
///
/// They spell two of them differently: os_info says `arm64` and `i386`/`i686` where Rust says
/// `aarch64` and `x86`. Anything else is compared as written, so an unknown spelling reports
/// a difference rather than hiding one.
fn same_arch(os_info_arch: &str, rust_arch: &str) -> bool {
    let norm = |a: &str| match a.to_ascii_lowercase().as_str() {
        "arm64" | "aarch64" => "aarch64".to_string(),
        "i386" | "i686" | "x86" => "x86".to_string(),
        "amd64" | "x86_64" | "x64" => "x86_64".to_string(),
        other => other.to_string(),
    };
    norm(os_info_arch) == norm(rust_arch)
}

fn epoch_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Appends one line to the log file. No-op if logging wasn't initialized.
///
/// Composed first and written with ONE `write_all`. `writeln!` straight into the file wrote
/// each piece of the format separately — the time, the scope, the message, the newline — and
/// a file opened for appending is only atomic per write. Within this process the mutex keeps
/// lines whole either way; the single write is what keeps them whole against a second process
/// appending to the same file, which is what [`append_from_outside`] does.
///
/// Past [`MAX_BYTES`] the file is rotated after the line — see [`Sink`]. The continuation
/// lines are composed with the lock released (they read the settings), and the rotation is
/// made under it only if no other thread made it meanwhile.
pub fn line(scope: &str, msg: &str) {
    let text = stamped(scope, msg);
    let due = match LOG.lock() {
        Ok(mut guard) => guard.as_mut().is_some_and(|sink| sink.write(text.as_bytes())),
        Err(_) => false,
    };
    if !due {
        return;
    }
    let previous = LOG.lock().ok().and_then(|g| g.as_ref().map(Sink::previous));
    let Some(previous) = previous else { return };
    let continued = continuation(&previous);
    if let Ok(mut guard) = LOG.lock() {
        if let Some(sink) = guard.as_mut().filter(|s| s.due()) {
            sink.rotate(&continued);
        }
    }
}

/// One line, from a process that must not start a session of its own in this log.
///
/// For the second copy of the application that the single-instance guard turned away (see
/// `crate::instance`). It never calls [`init`] — that would write a session header into the
/// middle of the running copy's session, and rotate the file under it — so it has no open
/// log. This opens the file for appending, writes the line in one `write_all` (a single append,
/// which lands whole between the running copy's lines, since those are single appends too),
/// and closes it again.
///
/// Used for the one case the running copy cannot write itself: when it did not answer. When it
/// does answer, it logs the request in its own session and the second copy writes nothing.
/// And for a panic while this process is [outside](set_outside), see [`panic_line`].
pub fn append_from_outside(scope: &str, msg: &str) {
    append_line_to(log_path(), scope, msg);
}

/// [`append_from_outside`] for any file, for the tests.
fn append_line_to(path: &Path, scope: &str, msg: &str) {
    let text = stamped(scope, msg);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(text.as_bytes());
    }
}

/// Whether this process has yet to be confirmed as the running copy. See [`set_outside`].
static OUTSIDE: AtomicBool = AtomicBool::new(false);

/// Marks this process as one that must not start a session in the log, or clears the mark.
///
/// Set by `run` while the single-instance guard decides whether this start is the running copy
/// or a second one, and cleared once it is the running copy. While it is set, [`init`] does
/// nothing and a panic is appended as one line ([`panic_line`]); a start that was turned away
/// keeps it until it exits.
pub fn set_outside(outside: bool) {
    OUTSIDE.store(outside, Ordering::SeqCst);
}

/// Writes a panic, for the panic hook (`main.rs`), which has no other place to put it.
///
/// Opens the log first when it is not open yet — a panic before `run` got that far — but never
/// a second time: a panic in a running session is one more line in it, not a new session
/// header. While this process is [outside](set_outside) the panic is appended as a single line
/// and the log is not opened at all.
pub fn panic_line(msg: &str) {
    if OUTSIDE.load(Ordering::SeqCst) {
        append_from_outside("panic", msg);
        return;
    }
    let open = LOG.lock().map(|g| g.is_some()).unwrap_or(false);
    if !open {
        init();
    }
    line("panic", msg);
}

/// Appends a line only when trace logging is on.
///
/// For the detail that is too much for normal running and exactly right when a remote
/// machine is misbehaving: every OS call that failed, every permission that was refused,
/// every element a walk did not find. `msg` is a closure so composing that detail costs
/// nothing when the level is off — some of these sit in the pump.
pub fn trace(scope: &str, msg: impl FnOnce() -> String) {
    if is_trace() {
        line(scope, &msg());
    }
}

/// Writes a labelled block of name/value pairs — a backend's account of the machine.
///
/// One line each rather than one long line: this is what a tester copies out, and what we
/// grep. See `Backend::environment`.
pub fn report(scope: &str, pairs: &[(String, String)]) {
    for (k, v) in pairs {
        line(scope, &format!("{k}: {v}"));
    }
}

// ── What our dependencies say ────────────────────────────────────────────────────────────

/// How many lines one library may write per [`DEP_WINDOW`] before the rest are counted
/// instead of written.
///
/// Per library rather than for all of them together, so a library that repeats itself cannot
/// crowd out a different one's single error. Twenty holds the useful part of the longest
/// account seen: with trace on, ONNX Runtime's session start (the OCR warmup) begins with the
/// hardware it found, its options and its threads, and then lists every graph optimisation
/// it made — dozens of lines within a second, of which the first twenty are the useful ones.
/// It also stops the case that is not an account at all: symphonia warns once per damaged
/// MP3 frame, which is dozens of lines a second for as long as a sound plays.
const DEP_BUDGET: u32 = 20;

/// The window [`DEP_BUDGET`] is counted over.
const DEP_WINDOW: Duration = Duration::from_secs(60);

/// A library's line is cut here. ONNX Runtime can put a whole graph dump into one message,
/// and one message must not become the file.
const DEP_MAX_CHARS: usize = 1000;

/// The `log` backend: every record from a dependency, filtered, rate-limited and written as
/// `[dep:<crate>] <level>: <message>`.
///
/// **Level.** Warnings and errors always — those are what was being thrown away, and each is
/// something a library thought worth raising. Info and debug only while trace logging is on,
/// which is what that switch is for: chasing one problem, with a large log accepted. Trace
/// never: that is where rustls narrates the steps of a TLS handshake (whole structures
/// pretty-printed among them) and ONNX Runtime its verbose level, which is noise even to
/// somebody who asked for detail. The switch can change while the process runs, so the level
/// is decided per record, from the same atomic every `trace` call reads.
///
/// **Rate.** At most [`DEP_BUDGET`] lines per library per [`DEP_WINDOW`]. The first line over
/// says so at once; the rest are counted, and the library's next line after the window is
/// preceded by how many were not written — so an absence in the log is never mistaken for
/// silence. A fixed window rather than a rate-limiter crate:
/// governor, the established one, answers "not yet" and forgets, and the count of what was
/// dropped — which is the part a reader needs — would still be ours to keep; what the crate
/// would replace is one comparison of two instants.
///
/// **Crate** is the first segment of the record's target (`rustls::client::hs` → `rustls`),
/// which is the module path of the call for both `log` and `tracing` records.
pub struct DependencyLog {
    gate: Mutex<Gate>,
}

static DEPENDENCY_LOG: DependencyLog = DependencyLog { gate: Mutex::new(Gate::new()) };

/// Routes the `log` facade — and, through tracing's `log` feature, ort's `tracing` events —
/// into this file. Idempotent: `set_logger` accepts one logger per process and refuses the rest.
///
/// The facade's own maximum is set to `Debug` once and for all, and the trace switch is
/// applied per record in [`DependencyLog::enabled`]. The facade's level is a filter in front
/// of the logger that the macros check before they format anything, so `Trace` stays out at
/// no cost; letting `Debug` through to a logger that then says no costs one atomic load per
/// debug line, and saves a hook into the settings code for the moment the switch is flipped.
pub fn install_dependency_log() {
    if log::set_logger(&DEPENDENCY_LOG).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
}

/// The most detailed level written for a dependency, given the trace switch.
fn dep_level(trace: bool) -> log::LevelFilter {
    if trace {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Warn
    }
}

impl log::Log for DependencyLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= dep_level(is_trace())
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let krate = dep_crate(record.target()).to_string();
        let admitted = match self.gate.lock() {
            Ok(mut gate) => gate.admit(&krate, Instant::now()),
            Err(_) => return,
        };
        let scope = format!("dep:{krate}");
        match admitted {
            Admit::Drop => {}
            // Said at the moment it starts, not only afterwards: the count is written before
            // the library's next line, and a library that went quiet has no next line.
            Admit::Notice => line(
                &scope,
                &format!(
                    "more than {DEP_BUDGET} lines from {krate} within {} s: the rest are counted \
                     rather than written, and the count comes before its next line",
                    DEP_WINDOW.as_secs()
                ),
            ),
            Admit::Write { dropped } => {
                if dropped > 0 {
                    line(&scope, &format!("{dropped} line(s) from {krate} were not written"));
                }
                line(&scope, &dep_message(record.level(), &record.args().to_string()));
            }
        }
    }

    fn flush(&self) {}
}

/// The crate a record came from: the first segment of its target.
fn dep_crate(target: &str) -> &str {
    let first = target.split("::").next().unwrap_or(target);
    if first.is_empty() {
        "unknown"
    } else {
        first
    }
}

/// `warn: <message>`, on one line and at most [`DEP_MAX_CHARS`] characters.
///
/// One line because the log is read line by line, by a screen reader and by grep; a library's
/// line break becomes ` | `.
fn dep_message(level: log::Level, message: &str) -> String {
    let flat: String =
        message.trim_end().replace("\r\n", "\n").replace(|c| c == '\n' || c == '\r', " | ");
    let total = flat.chars().count();
    let body = if total > DEP_MAX_CHARS {
        let cut: String = flat.chars().take(DEP_MAX_CHARS).collect();
        format!("{cut}… ({} more characters)", total - DEP_MAX_CHARS)
    } else {
        flat
    };
    format!("{}: {body}", level.as_str().to_ascii_lowercase())
}

/// The per-library budget behind [`DependencyLog`], kept apart from it so the arithmetic can
/// be tested with instants the test chooses.
struct Gate {
    windows: Option<HashMap<String, Window>>,
}

struct Window {
    started: Instant,
    written: u32,
    /// Whether this window's first dropped line has been announced.
    noticed: bool,
    /// Dropped since the last line written; carried across windows until it is reported.
    dropped: u64,
}

/// What [`Gate::admit`] decided for one record.
#[derive(Debug, PartialEq)]
enum Admit {
    /// Write it, after saying that `dropped` lines were not written before it.
    Write { dropped: u64 },
    /// Over the budget, for the first time in this window: counted, and announced.
    Notice,
    /// Over the budget: counted only.
    Drop,
}

impl Gate {
    const fn new() -> Self {
        // `None` until the first record: a HashMap cannot be built in a const, and the static
        // above needs one.
        Gate { windows: None }
    }

    fn admit(&mut self, krate: &str, now: Instant) -> Admit {
        let windows = self.windows.get_or_insert_with(HashMap::new);
        let w = windows
            .entry(krate.to_string())
            .or_insert(Window { started: now, written: 0, noticed: false, dropped: 0 });
        if now.duration_since(w.started) >= DEP_WINDOW {
            w.started = now;
            w.written = 0;
            w.noticed = false;
        }
        if w.written >= DEP_BUDGET {
            w.dropped += 1;
            if w.noticed {
                return Admit::Drop;
            }
            w.noticed = true;
            return Admit::Notice;
        }
        w.written += 1;
        Admit::Write { dropped: std::mem::take(&mut w.dropped) }
    }
}

// ── The same line again and again ────────────────────────────────────────────────────────

/// How often a line that keeps coming is summed up, at most: once a minute.
pub const REPEAT_WINDOW: Duration = Duration::from_secs(60);

/// After this long without a repeat, a line is forgotten, and its next occurrence is written in
/// full again as news.
pub const REPEAT_FORGET: Duration = Duration::from_secs(600);

/// The most lines [`Repeats`] keeps count of at once. A line whose text changes every time (a
/// counter or a time in an error message) is news every time and is written every time; past
/// this many lines, a new one takes the place of the line seen longest ago — one with nothing
/// counted if there is one — so a line that keeps coming is always among those kept, however
/// many changing ones come between its occurrences.
///
/// Why the texts are not made alike (digits taken out, say) so that a changing message counts
/// as one: the digits are often the point — `main.luau:42:` and `main.luau:57:` are two
/// different errors, and a count of "the same" error across them would hide the second.
pub const REPEAT_MAX_LINES: usize = 512;

/// A line the caller writes in full the first time and counts after that, with at most one
/// summary per [`REPEAT_WINDOW`] — the rule [`DependencyLog`] keeps per library, kept here per
/// line: a module's poll that fails every 500 ms would otherwise write 170 000 identical lines a
/// day. Keyed by whatever the caller says makes two lines the same (for a module error: the
/// module, the context and the message).
///
/// Pure: the caller hands in the time, so the tests choose it. What it holds is bounded by the
/// distinct lines of the last [`REPEAT_FORGET`], since [`Repeats::due`] forgets the quiet ones,
/// and by [`REPEAT_MAX_LINES`].
#[derive(Default, Debug)]
pub struct Repeats {
    lines: HashMap<String, Repeat>,
}

#[derive(Debug)]
struct Repeat {
    /// When the line was last written in full or summed up.
    since: Instant,
    /// Occurrences since then, not written.
    count: u64,
    /// The latest occurrence.
    last: Instant,
}

/// What [`Repeats::note`] says to do with one occurrence.
#[derive(Debug, PartialEq, Eq)]
pub enum Said {
    /// Write the line: it is news.
    Line,
    /// Counted; write nothing.
    Counted,
    /// Write the summary: it came this many times in the window that just ended, this one
    /// included.
    Summary { count: u64, over: Duration },
}

impl Repeats {
    pub fn note(&mut self, key: &str, now: Instant) -> Said {
        match self.lines.get_mut(key) {
            None => {
                if self.lines.len() >= REPEAT_MAX_LINES {
                    self.evict_one();
                }
                self.lines.insert(key.to_string(), Repeat { since: now, count: 0, last: now });
                Said::Line
            }
            Some(r) => {
                r.count += 1;
                r.last = now;
                let over = now.saturating_duration_since(r.since);
                if over >= REPEAT_WINDOW {
                    let count = std::mem::take(&mut r.count);
                    r.since = now;
                    Said::Summary { count, over }
                } else {
                    Said::Counted
                }
            }
        }
    }

    /// The summaries owed now — lines counted in a window that has ended, whose next
    /// occurrence has not come to write it — as (key, count, window), and forgets the lines
    /// quiet for [`REPEAT_FORGET`]. Asked from the event loop's tick, so a line that stops
    /// repeating still has its count written.
    pub fn due(&mut self, now: Instant) -> Vec<(String, u64, Duration)> {
        let mut out = Vec::new();
        for (k, r) in self.lines.iter_mut() {
            let over = now.saturating_duration_since(r.since);
            if r.count > 0 && over >= REPEAT_WINDOW {
                out.push((k.clone(), std::mem::take(&mut r.count), over));
                r.since = now;
            }
        }
        self.lines.retain(|_, r| r.count > 0 || now.saturating_duration_since(r.last) < REPEAT_FORGET);
        out.sort();
        out
    }

    /// Every count still owed, as (key, count, from the line written last to the latest
    /// occurrence), and forgets every line, so the next occurrence of each is news again. For
    /// when the clock the counts run on cannot be trusted across what just happened: on macOS
    /// the process's clock stops while the Mac sleeps (`system_events::Since::plan`, at a wake).
    pub fn take_all(&mut self) -> Vec<(String, u64, Duration)> {
        let mut out: Vec<(String, u64, Duration)> = self
            .lines
            .drain()
            .filter(|(_, r)| r.count > 0)
            .map(|(k, r)| (k, r.count, r.last.saturating_duration_since(r.since)))
            .collect();
        out.sort();
        out
    }

    /// Forgets every line whose key starts with `prefix`, so its next occurrence is news again
    /// (a module switched off and on, or reloaded).
    pub fn forget_prefix(&mut self, prefix: &str) {
        self.lines.retain(|k, _| !k.starts_with(prefix));
    }

    /// Makes room for one more line: forgets the one seen longest ago among those with nothing
    /// counted, or — when every line kept has a count waiting — the one seen longest ago, whose
    /// count is then not written. One pass over [`REPEAT_MAX_LINES`] entries, only when a new
    /// line comes to a full table.
    fn evict_one(&mut self) {
        let oldest = |quiet_only: bool| {
            self.lines
                .iter()
                .filter(|(_, r)| !quiet_only || r.count == 0)
                .min_by_key(|(_, r)| r.last)
                .map(|(k, _)| k.clone())
        };
        if let Some(k) = oldest(true).or_else(|| oldest(false)) {
            self.lines.remove(&k);
        }
    }
}

// ── Panics that are caught and reported by the code they happen in ──────────────────────

thread_local! {
    /// Whether this thread is inside [`contain`].
    static CONTAINING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The panic hook's report of a panic inside `contain`, kept for `contain` to answer with.
    static HELD: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Runs `f`, catching a panic in it and answering with the panic's report — message and
/// place, as the application's panic hook would have written it — instead.
///
/// For code that survives its own panics and reports them itself, at a rate it chooses: the
/// image worker, where one template that trips something would otherwise panic on every
/// 500 ms poll. The application's panic hook asks [`hold_contained_panic`] first and writes
/// nothing for these. Without that, the hook logged every one of them in full, so a fault the
/// worker had survived cost a log line per poll for as long as it lasted, whatever the worker's
/// own throttle said.
pub fn contain<R>(f: impl FnOnce() -> R) -> Result<R, String> {
    let was = CONTAINING.with(|c| c.replace(true));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    CONTAINING.with(|c| c.set(was));
    // Taken either way, so a report can never be left over for a later `contain` to answer.
    let held = HELD.with(|h| h.borrow_mut().take());
    r.map_err(|p| {
        held.unwrap_or_else(|| {
            p.downcast_ref::<&str>()
                .map(|s| (*s).to_string())
                .or_else(|| p.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "a panic with no message".into())
        })
    })
}

/// For the application's panic hook: `true` when the panicking thread is inside [`contain`],
/// which will report the panic itself. The report (`report` is called only then) is kept for
/// it, and the hook should write nothing.
pub fn hold_contained_panic(report: impl FnOnce() -> String) -> bool {
    // `try_with`: a panic during thread teardown must not panic again in the hook.
    if !CONTAINING.try_with(|c| c.get()).unwrap_or(false) {
        return false;
    }
    let _ = HELD.try_with(|h| *h.borrow_mut() = Some(report()));
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_os_line_keeps_its_prefix_and_names_the_system() {
        // `os <os> <arch> — ` is what CI greps for; what follows is os_info's own account.
        let line = os_line(&os_info::Info::with_type(os_info::Type::Windows));
        assert!(
            line.starts_with(&format!(
                "os {} {} — Windows",
                std::env::consts::OS,
                std::env::consts::ARCH
            )),
            "{line}"
        );
        // No architecture known: nothing is said about the machine.
        assert!(!line.contains("machine"), "{line}");
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_the_os_line_is_a_version_rather_than_windows_nt() {
        // What the header said before: the `OS` variable, which is `Windows_NT` everywhere.
        let info = os_info::get();
        let line = os_line(&info);
        assert!(!line.contains("Windows_NT"), "{line}");
        assert_ne!(info.version(), &os_info::Version::Unknown, "{line}");
        // RtlGetVersion's major.minor.build, which is 10.0.x on Windows 10 and 11 alike.
        assert!(line.contains("Windows 10.0."), "{line}");
    }

    #[test]
    fn architectures_are_compared_across_the_two_spellings() {
        assert!(same_arch("aarch64", "aarch64"));
        assert!(same_arch("arm64", "aarch64"));
        assert!(same_arch("i386", "x86"));
        assert!(same_arch("x86_64", "x86_64"));
        // A different machine, whenever os_info does name one, is a difference worth a word.
        assert!(!same_arch("aarch64", "x86_64"));
        // An unknown spelling reports a difference rather than hiding one.
        assert!(!same_arch("riscv64gc", "x86_64"));
    }

    #[test]
    fn dependency_levels_follow_the_trace_switch_and_never_reach_trace() {
        use log::Level::*;
        let off = dep_level(false);
        assert!(Error <= off && Warn <= off);
        assert!(Info > off && Debug > off);
        let on = dep_level(true);
        assert!(Info <= on && Debug <= on);
        // Handshake narration from rustls, ONNX Runtime's verbose level.
        assert!(Trace > on);
    }

    #[test]
    fn a_record_is_filed_under_its_crate() {
        assert_eq!(dep_crate("rustls::client::hs"), "rustls");
        assert_eq!(dep_crate("ureq"), "ureq");
        assert_eq!(dep_crate("ort::logging"), "ort");
        assert_eq!(dep_crate(""), "unknown");
    }

    #[test]
    fn a_dependency_message_is_one_bounded_line() {
        assert_eq!(dep_message(log::Level::Warn, "plain"), "warn: plain");
        assert_eq!(
            dep_message(log::Level::Error, "first\r\nsecond\nthird\n"),
            "error: first | second | third"
        );
        let long = "é".repeat(DEP_MAX_CHARS + 5);
        let msg = dep_message(log::Level::Debug, &long);
        assert!(msg.starts_with("debug: "));
        // Cut by characters, not bytes: `é` is two bytes and must not be split.
        assert!(msg.ends_with("… (5 more characters)"), "{msg}");
        assert_eq!(msg.chars().filter(|&c| c == 'é').count(), DEP_MAX_CHARS);
    }

    #[test]
    fn a_line_from_outside_is_appended_whole_and_opens_no_session() {
        // What a turned-away second start leaves in the running copy's log: its line after the
        // lines already there, complete, and no session header of its own.
        let dir = std::env::temp_dir().join(format!("ap-test-outside-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("automation-platform.log");
        std::fs::write(&path, "1 [host] session start\n").unwrap();
        append_line_to(&path, "instance", "a second copy did not start");
        append_line_to(&path, "panic", "two");
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert_eq!(lines[0], "1 [host] session start");
        assert!(lines[1].ends_with(" [instance] a second copy did not start"), "{text}");
        assert!(lines[2].ends_with(" [panic] two"), "{text}");
        assert!(text.ends_with('\n'));
        assert_eq!(text.matches("session start").count(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn temp_log(name: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ap-test-rotate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("automation-platform.log");
        (dir, path)
    }

    /// The session's log starts a new file once it has written past the limit: the old file is
    /// `.log.1`, the new one opens with the continuation lines, and a line appended from outside
    /// (a second start the guard turned away) still lands whole in the current file.
    #[test]
    fn the_log_rotates_within_the_session_by_the_bytes_it_wrote() {
        let (dir, path) = temp_log("within");
        std::fs::write(&path, "1 [host] session start\n").unwrap();
        let mut sink = Sink::open(path.clone(), 100).unwrap();
        assert_eq!(sink.bytes, 23, "seeded from the file's length");
        let continued = vec!["2 [host] log continued\n".to_string()];
        let mut rotations = 0;
        for i in 0..30 {
            if sink.write(stamped("t", &format!("line {i:02}")).as_bytes()) {
                sink.rotate(&continued);
                rotations += 1;
            }
        }
        assert!(rotations >= 2, "{rotations}");
        let old = std::fs::read_to_string(path.with_extension("log.1")).unwrap();
        let new = std::fs::read_to_string(&path).unwrap();
        // `.1` is overwritten at every rotation: it holds the file just before the last one,
        // which begins with the continuation line, not the session start.
        assert!(old.starts_with("2 [host] log continued\n"), "{old}");
        assert!(!old.contains("session start"), "{old}");
        assert!(new.starts_with("2 [host] log continued\n"), "{new}");
        // Every line of every file whole, and none lost: 30 lines across the two files that are
        // left and the ones rotated away before them, in order.
        for text in [&old, &new] {
            assert!(text.ends_with('\n'));
            for l in text.lines().filter(|l| l.contains("[t]")) {
                assert!(l.contains("[t] line "), "{l}");
                assert_eq!(l.len(), l.find("[t]").unwrap() + "[t] line 00".len(), "{l}");
            }
        }
        assert!(new.contains("[t] line 29"), "the last line is in the new file: {new}");
        // What a turned-away second start appends lands whole, after our lines.
        append_line_to(&path, "instance", "a second copy did not start");
        sink.write(stamped("t", "after").as_bytes());
        let new = std::fs::read_to_string(&path).unwrap();
        let tail: Vec<&str> = new.lines().rev().take(2).collect();
        assert!(tail[0].ends_with(" [t] after"), "{new}");
        assert!(tail[1].ends_with(" [instance] a second copy did not start"), "{new}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A rename that is refused costs no line: the file goes on growing, the refusal is said
    /// once, and the rename is tried again a sixteenth of the limit later.
    #[test]
    fn a_refused_rotation_keeps_writing_and_tries_again_later() {
        let (dir, path) = temp_log("refused");
        // A directory where `.log.1` would go: the rename cannot replace it.
        std::fs::create_dir_all(path.with_extension("log.1")).unwrap();
        std::fs::write(path.with_extension("log.1").join("keep"), "x").unwrap();
        let mut sink = Sink::open(path.clone(), 160).unwrap();
        let continued = vec!["9 [host] log continued\n".to_string()];
        let mut attempts = 0;
        for i in 0..20 {
            if sink.write(stamped("t", &format!("line {i:02}")).as_bytes()) {
                sink.rotate(&continued);
                attempts += 1;
            }
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for i in 0..20 {
            assert!(text.contains(&format!("[t] line {i:02}")), "line {i} lost: {text}");
        }
        assert!(attempts >= 2, "tried again: {attempts}");
        assert_eq!(text.matches("could not be moved").count(), attempts, "said at every try: {text}");
        assert!(!text.contains("log continued"), "nothing continued: it is the same file");
        assert_eq!(sink.bytes, text.len() as u64, "the count follows the file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A filesystem whose answers the test chooses: each call takes the next scripted answer
    /// (`true` succeeds) and is counted.
    struct Scripted {
        renames: Vec<bool>,
        opens: Vec<bool>,
        calls: Vec<&'static str>,
    }

    impl Fs for Scripted {
        fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
            self.calls.push("rename");
            if self.renames.remove(0) {
                std::fs::rename(from, to)
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "refused"))
            }
        }
        fn open(&mut self, path: &Path) -> std::io::Result<File> {
            self.calls.push("open");
            if self.opens.remove(0) {
                RealFs.open(path)
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no"))
            }
        }
        fn still_names(&mut self, path: &Path, file: &File) -> bool {
            RealFs.still_names(path, file)
        }
    }

    /// The new file cannot be opened after the old one was moved: nothing is tried again until
    /// another sixteenth of the limit's worth of lines went by — not at every line — the rename
    /// is not made twice, and the file that finally opens starts with the continuation lines.
    #[test]
    fn a_failed_opening_is_tried_again_later_and_the_new_file_still_begins_as_one() {
        let (dir, path) = temp_log("reopen");
        let mut sink = Sink::open(path.clone(), 1600).unwrap();
        let continued = vec!["9 [host] log continued\n".to_string()];
        let mut fs = Scripted { renames: vec![true], opens: vec![false, true], calls: Vec::new() };
        let line = |i: usize| stamped("t", &format!("line {i:02}"));
        let mut i = 0;
        while !sink.write(line(i).as_bytes()) {
            i += 1;
        }
        sink.rotate_with(&continued, &mut fs);
        assert_eq!(fs.calls, vec!["rename", "open"]);
        assert!(sink.file.is_none() && sink.moved);
        // The lines until the next try are lost, but they are counted, so the try comes after a
        // sixteenth of the limit (100 bytes here, four or five lines), not at every line.
        let mut lost = 0;
        loop {
            i += 1;
            lost += 1;
            if sink.write(line(i).as_bytes()) {
                sink.rotate_with(&continued, &mut fs);
                break;
            }
            assert!(lost < 50, "never tried again");
        }
        assert!(lost >= 4, "tried again after {lost} line(s)");
        assert_eq!(fs.calls, vec!["rename", "open", "open"], "opened, not renamed again");
        assert!(sink.file.is_some() && !sink.moved);
        let new = std::fs::read_to_string(&path).unwrap();
        assert!(new.starts_with("9 [host] log continued\n"), "{new}");
        assert!(!new.contains("could not be moved"), "no refusal that did not happen: {new}");
        assert!(std::fs::read_to_string(path.with_extension("log.1")).unwrap().contains("[t] line 00"));
        // The next line goes into the new file, and rotation is back to the limit.
        i += 1;
        assert!(!sink.write(line(i).as_bytes()));
        assert_eq!(sink.rotate_at, 1600);
        drop(sink);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file the session continues in begins with where the rest is and the build, repeats
    /// the header's `translocated` line exactly when the header has one — both are written from
    /// the one answer `portable` keeps — and then the header's `app folder` line, which the
    /// translocated one refers to, so a file sent on its own still says why it lies where it
    /// does. (Off macOS there is no translocated line, and none here either.) The settings come
    /// last, as `docs/api/log.md` lists them.
    #[test]
    fn a_continued_file_repeats_what_a_file_sent_alone_needs() {
        // What the header kept, or — in a test process, which writes no header — a stand-in.
        let folder = APP_FOLDER_LINE.get_or_init(|| "app folder /test (writable)".to_string()).clone();
        let lines = continuation(Path::new("automation-platform.log.1"));
        assert!(lines[0].contains(" [host] log continued: this session started at "), "{lines:?}");
        assert!(lines[0].contains("automation-platform.log.1"), "{lines:?}");
        let build = crate::build_info::log_line(env!("CARGO_PKG_VERSION"));
        assert!(lines[1].ends_with(&format!(" [host] {build}\n")), "{lines:?}");
        let translocated: Vec<&String> = lines.iter().filter(|l| l.contains(" [host] translocated: ")).collect();
        let after = match portable::translocation().report() {
            Some(t) => {
                assert_eq!(translocated.len(), 1, "{lines:?}");
                assert!(translocated[0].ends_with(&format!("translocated: {t}\n")), "{lines:?}");
                assert_eq!(lines[2], *translocated[0], "right after the build: {lines:?}");
                3
            }
            None => {
                assert!(translocated.is_empty(), "{lines:?}");
                2
            }
        };
        assert!(lines[after].ends_with(&format!(" [host] {folder}\n")), "the app folder next: {lines:?}");
        assert!(lines.len() <= after + 2 && lines.len() <= 5, "{lines:?}");
        if let Some(settings) = lines.get(after + 1) {
            assert!(settings.contains(" [host] settings on: "), "the settings last: {lines:?}");
        }
        for l in &lines {
            assert!(l.ends_with('\n') && l.matches('\n').count() == 1, "one line each: {l:?}");
        }
        // The header writes the same lines from the same calls, and keeps the folder's.
        let src = include_str!("logging.rs");
        let header = &src[src.find("fn header()").unwrap()..];
        let header = &header[..header.find("\n}\n").unwrap()];
        assert!(header.contains("portable::translocation().report()"), "the header's line moved");
        assert!(header.contains("line(\"host\", &folder);") && header.contains("APP_FOLDER_LINE.set(folder)"));
    }

    /// Another copy writing the same log (a headless run beside the application) moved the file
    /// first, and this copy's handle followed it to `.log.1`. At this copy's own limit the path
    /// names the other copy's new file: it is not moved over `.log.1` — which would lose this
    /// copy's lines there — and this copy goes on in it, saying so, with the continuation lines.
    #[test]
    fn a_log_another_copy_moved_is_not_moved_again() {
        let (dir, path) = temp_log("elsewhere");
        let previous = path.with_extension("log.1");
        let mut sink = Sink::open(path.clone(), 400).unwrap();
        sink.write(stamped("t", "before the other copy moved it").as_bytes());
        // The other copy's start-up or in-session rotation, with this copy's file still open.
        std::fs::rename(&path, &previous).unwrap();
        std::fs::write(&path, "1 [host] the other copy's session start\n").unwrap();
        sink.write(stamped("t", "after it moved it").as_bytes());
        let continued = vec!["9 [host] log continued\n".to_string()];
        let mut i = 0;
        while !sink.write(stamped("t", &format!("line {i:02}")).as_bytes()) {
            i += 1;
        }
        sink.rotate(&continued);
        sink.write(stamped("t", "in the other copy's file").as_bytes());
        drop(sink);
        let old = std::fs::read_to_string(&previous).unwrap();
        let new = std::fs::read_to_string(&path).unwrap();
        assert!(old.contains("[t] before the other copy moved it") && old.contains("[t] after it moved it"), "{old}");
        assert!(old.contains(&format!("[t] line {i:02}")), "every line up to the limit is kept: {old}");
        assert!(!old.contains("the other copy's session start"), "the other copy's file was moved over ours: {old}");
        assert!(new.starts_with("1 [host] the other copy's session start\n"), "{new}");
        let said = new.find("the log had been moved away before this session moved it").expect(&new);
        assert!(said < new.find("9 [host] log continued").unwrap(), "{new}");
        assert!(new.ends_with(" [t] in the other copy's file\n"), "{new}");
        assert!(!new.contains("could not be moved"), "{new}");
        // A file that is still the one it opened is moved as before.
        let mut sink = Sink::open(path.clone(), 1).unwrap();
        assert!(sink.write(b"x\n"));
        sink.rotate(&continued);
        drop(sink);
        assert!(std::fs::read_to_string(&previous).unwrap().ends_with("x\n"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "9 [host] log continued\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The first occurrence is written, then a count, summed up at most once a minute — by the
    /// next occurrence, or by the tick when the line has stopped coming — and a line quiet for
    /// ten minutes is news again.
    #[test]
    fn a_repeated_line_is_written_once_and_then_counted_a_minute_at_a_time() {
        let mut r = Repeats::default();
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_secs(n);
        assert_eq!(r.note("a", s(0)), Said::Line);
        for i in 1..=59 {
            assert_eq!(r.note("a", s(i)), Said::Counted);
        }
        // Another line has a count of its own.
        assert_eq!(r.note("b", s(10)), Said::Line);
        assert_eq!(r.note("a", s(60)), Said::Summary { count: 60, over: Duration::from_secs(60) });
        assert_eq!(r.note("a", s(61)), Said::Counted);
        // It stops: the tick writes the count once the minute is up, and only then.
        assert!(r.due(s(100)).is_empty());
        assert_eq!(r.due(s(121)), vec![("a".to_string(), 1, Duration::from_secs(61))]);
        assert!(r.due(s(200)).is_empty(), "said once");
        // Quiet for ten minutes: forgotten, and news again.
        assert!(r.due(s(61 + 600)).is_empty());
        assert!(!r.lines.contains_key("a") && !r.lines.contains_key("b"));
        assert_eq!(r.note("a", s(700)), Said::Line);
        r.forget_prefix("a");
        assert_eq!(r.note("a", s(701)), Said::Line, "forgotten by prefix");
        // Everything owed at once, and everything forgotten: the span is from the line written
        // last to the latest occurrence, whatever the clock says now.
        let mut r = Repeats::default();
        r.note("a", s(0));
        r.note("a", s(10));
        r.note("a", s(25));
        r.note("b", s(5));
        assert_eq!(r.take_all(), vec![("a".to_string(), 2, Duration::from_secs(25))], "b had nothing counted");
        assert!(r.lines.is_empty());
        assert_eq!(r.note("a", s(26)), Said::Line, "news again");
        // A message that changes every time is written every time, and the table stays at its cap.
        let mut r = Repeats::default();
        for i in 0..REPEAT_MAX_LINES + 10 {
            assert_eq!(r.note(&format!("tick {i}"), s(0)), Said::Line);
        }
        assert_eq!(r.lines.len(), REPEAT_MAX_LINES);
    }

    /// A full table makes room by forgetting the line seen longest ago, so a steady line is
    /// still counted however many changing ones come between its occurrences — once the table
    /// had filled, a new line used to be written every time it came, for everything after it.
    #[test]
    fn a_full_table_still_counts_a_line_that_keeps_coming() {
        let mut r = Repeats::default();
        let t0 = Instant::now();
        let ms = |n: u64| t0 + Duration::from_millis(n);
        // Fill it with an error whose text changes every time, twice a second.
        for i in 0..REPEAT_MAX_LINES as u64 {
            assert_eq!(r.note(&format!("mod\u{1}timer\u{1}tick {i}"), ms(i * 500)), Said::Line);
        }
        assert_eq!(r.lines.len(), REPEAT_MAX_LINES);
        // Another module's error starts now, between more of the changing ones: written once,
        // then counted, and never pushed out by them.
        let mut t = REPEAT_MAX_LINES as u64 * 500;
        assert_eq!(r.note("other\u{1}timer\u{1}steady", ms(t)), Said::Line);
        for i in 0..2000u64 {
            t += 250;
            assert_eq!(r.note(&format!("mod\u{1}timer\u{1}tick x{i}"), ms(t)), Said::Line);
            if i % 4 == 0 {
                let said = r.note("other\u{1}timer\u{1}steady", ms(t));
                assert!(matches!(said, Said::Counted | Said::Summary { .. }), "{i}: {said:?}");
            }
            assert!(r.lines.len() <= REPEAT_MAX_LINES);
        }
        // A line with a count waiting is kept over a quiet one that is older still.
        let mut r = Repeats::default();
        r.note("counted", ms(0));
        r.note("counted", ms(1));
        for i in 0..REPEAT_MAX_LINES as u64 - 1 {
            r.note(&format!("once {i}"), ms(2 + i));
        }
        r.note("new", ms(10_000));
        assert!(r.lines.contains_key("counted") && r.lines.contains_key("new"));
        assert!(!r.lines.contains_key("once 0"), "the quiet line seen longest ago went");
    }

    #[test]
    fn a_library_over_its_budget_is_announced_counted_and_the_count_reported_once() {
        let mut gate = Gate::new();
        let t0 = Instant::now();
        for _ in 0..DEP_BUDGET {
            assert_eq!(gate.admit("symphonia", t0), Admit::Write { dropped: 0 });
        }
        // Over budget: announced once, then only counted.
        assert_eq!(gate.admit("symphonia", t0), Admit::Notice);
        assert_eq!(gate.admit("symphonia", t0 + Duration::from_secs(30)), Admit::Drop);
        // Another library has a budget of its own.
        assert_eq!(gate.admit("rustls", t0), Admit::Write { dropped: 0 });
        // The next window: written again, with the two dropped lines reported first, once.
        let t1 = t0 + DEP_WINDOW;
        assert_eq!(gate.admit("symphonia", t1), Admit::Write { dropped: 2 });
        assert_eq!(gate.admit("symphonia", t1), Admit::Write { dropped: 0 });
        // And a new window announces its own overflow again.
        for _ in 2..DEP_BUDGET {
            gate.admit("symphonia", t1);
        }
        assert_eq!(gate.admit("symphonia", t1), Admit::Notice);
    }
}
