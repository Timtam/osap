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

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

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
pub fn init() {
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
    line("host", &format!("version {}", env!("CARGO_PKG_VERSION")));
    line(
        "host",
        &format!(
            "os {} {} — {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            os_release()
        ),
    );
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

/// A human-readable OS version, best effort.
///
/// There is no portable way to ask, and the per-platform ways are worth their few lines:
/// on macOS especially, the version decides whether a capture API still works at all, and
/// asking the tester to read it out is asking them to navigate a settings pane by voice.
fn os_release() -> String {
    #[cfg(target_os = "macos")]
    {
        // sw_vers rather than an API call: this runs before the backend exists, and the
        // ProcessInfo route would drag AppKit into a module that is otherwise pure std.
        if let Ok(out) = std::process::Command::new("sw_vers").arg("-productVersion").output() {
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !v.is_empty() {
                return format!("macOS {v}");
            }
        }
        "macOS (version unknown)".to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        std::env::var("OS").unwrap_or_else(|_| "unknown".to_string())
    }
}

fn epoch_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Appends one line to the log file. No-op if logging wasn't initialized.
pub fn line(scope: &str, msg: &str) {
    if let Ok(mut guard) = LOG.lock() {
        if let Some(file) = guard.as_mut() {
            let _ = writeln!(file, "{} [{scope}] {msg}", epoch_secs());
            let _ = file.flush();
        }
    }
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
