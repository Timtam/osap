//! Minimal file logging. Diagnostics go to `<exe_dir>/automation-platform.log`
//! (portable — next to the executable), **not** stdout/stderr: a screen reader
//! reads the focused terminal, so console output would be spoken aloud.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Log file path, next to the executable (falls back to the working directory).
fn log_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default()
        .join("automation-platform.log")
}

/// Opens the log file (append) next to the executable. Silent on failure.
pub fn init() {
    if let Ok(file) = OpenOptions::new().create(true).append(true).open(log_path()) {
        if let Ok(mut guard) = LOG.lock() {
            *guard = Some(file);
        }
        line("host", "session start");
    }
}

fn epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
