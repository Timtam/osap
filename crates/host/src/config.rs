//! Portable per-app config: the set of module ids the user disabled in the
//! manager. Stored as one id per line in `<exe_dir>/disabled-modules.txt` —
//! next to the executable, so the app stays portable (no `%APPDATA%`).

use std::collections::HashSet;
use std::path::PathBuf;

/// Config file path, next to the executable (falls back to the working dir).
fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default()
        .join("disabled-modules.txt")
}

/// Loads the set of disabled module ids (empty if no config / unreadable).
pub fn load_disabled() -> HashSet<String> {
    std::fs::read_to_string(config_path())
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Persists the set of disabled module ids (one per line).
pub fn save_disabled(disabled: &[String]) {
    let mut ids = disabled.to_vec();
    ids.sort();
    let body = format!(
        "# Modules disabled in the Automation Platform manager (one id per line).\n{}\n",
        ids.join("\n")
    );
    let _ = std::fs::write(config_path(), body);
}
