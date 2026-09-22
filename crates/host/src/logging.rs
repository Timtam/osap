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
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::portable;

static LOG: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Roll over at 8 MB. Chosen to stay attachable to an email or an issue: a trace-level
/// session can produce that in an afternoon, and the previous file is kept because the
/// interesting part is often the startup that happened before the symptom.
const MAX_BYTES: u64 = 8 * 1024 * 1024;

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
/// Rotation happens here rather than per line: the size only matters between sessions, and
/// checking it on every write would put a filesystem call in the pump.
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
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(path) {
        if let Ok(mut guard) = LOG.lock() {
            *guard = Some(file);
        }
    }
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
    if let Some(problem) = crate::build_info::package_problem() {
        line("host", &format!("{problem}; the build above is the executable's own"));
    } else if crate::build_info::package_commit().is_none() && portable::in_bundle() {
        // Not a development run (those are never inside a .app), so the package's file is
        // missing, and the executable's own commit is all there is. On macOS that can be an
        // earlier run's, when CI reused its executable, so it must not pass for the download's.
        line(
            "host",
            &format!(
                "no {} beside the .app (it was moved away from its folder, or macOS is running \
                 a translocated copy, which a `translocated` line below would say): the build above \
                 is the executable's own, which is older than the download when CI reused an \
                 earlier executable",
                crate::build_info::FILE_NAME
            ),
        );
    }
    line("host", &os_line(&os_info::get()));
    match std::env::current_exe() {
        Ok(p) => line("host", &format!("executable {}", p.display())),
        Err(e) => line("host", &format!("executable unknown: {e}")),
    }
    let base = portable::base_dir();
    line(
        "host",
        &format!(
            "app folder {} ({})",
            base.display(),
            if portable::is_writable(base) { "writable" } else { "NOT WRITABLE" }
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
pub fn line(scope: &str, msg: &str) {
    let text = format!("{} [{scope}] {msg}\n", epoch_secs());
    if let Ok(mut guard) = LOG.lock() {
        if let Some(file) = guard.as_mut() {
            let _ = file.write_all(text.as_bytes());
            let _ = file.flush();
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
    let text = format!("{} [{scope}] {msg}\n", epoch_secs());
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
