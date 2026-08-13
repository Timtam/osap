//! Where the application keeps its own things: the log, `settings.toml`, and `modules/`.
//!
//! One rule, decided once: **beside the application**, not in a per-user system folder.
//! The whole folder can be copied to another machine or deleted to reset, which is what
//! "portable" buys — and for a user who cannot see, "delete this folder" is a far better
//! recovery instruction than "open Explorer and navigate to a hidden AppData path".
//!
//! macOS is the reason this is a module rather than three copies of `current_exe()`.
//! There, the executable is buried inside the application: `AutomationPlatform.app/
//! Contents/MacOS/automation-platform`. Writing beside *the executable* would write inside
//! the bundle — which breaks code signing, is read-only for an app in /Applications, and is
//! invisible to the person trying to send us their log. Beside *the application* means
//! beside the `.app`, which is the folder the user actually sees.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The folder the application lives in: next to the `.exe`, or next to the `.app`.
///
/// Computed once. It cannot change while the process runs, and every caller wants the same
/// answer — a disagreement between the log path and the settings path would be the kind of
/// bug that only shows up as "my settings did not stick".
pub fn base_dir() -> &'static Path {
    static BASE: OnceLock<PathBuf> = OnceLock::new();
    BASE.get_or_init(|| {
        let exe = std::env::current_exe().unwrap_or_default();
        let dir = exe.parent().unwrap_or(Path::new(".")).to_path_buf();
        strip_bundle(&dir).unwrap_or(dir)
    })
}

/// Given the directory an executable sits in, the directory the surrounding `.app` sits in.
///
/// Matched by shape (`…/Something.app/Contents/MacOS`) rather than by asking the OS,
/// because that is all a bundle is, and because it lets the rule be unit-tested on the
/// machine where the port is being written — which has no bundles at all.
///
/// Deliberately not gated to macOS: a rule that only exists on the platform nobody here can
/// run is a rule nobody can test.
fn strip_bundle(dir: &Path) -> Option<PathBuf> {
    let macos = dir.file_name()?;
    if macos != "MacOS" {
        return None;
    }
    let contents = dir.parent()?;
    if contents.file_name()? != "Contents" {
        return None;
    }
    let bundle = contents.parent()?;
    if !bundle.extension().is_some_and(|e| e == "app") {
        return None;
    }
    Some(bundle.parent()?.to_path_buf())
}

/// Can we actually write there?
///
/// Asked rather than assumed, and asked by *writing*: permissions on macOS are not a
/// property of the folder alone (a quarantined download, an app moved into /Applications,
/// and a sandboxed launch all fail differently), and the read-only metadata bit answers a
/// different question than "will my log line land".
pub fn is_writable(dir: &Path) -> bool {
    let probe = dir.join(".write-probe");
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Where to put something that MUST be writable, when the application's own folder is not.
///
/// Only the log really qualifies: settings and modules failing loudly beside the app is
/// honest and fixable ("move the folder somewhere you can write"), but a log that cannot be
/// written leaves a remote tester with nothing to send, which is the one failure that makes
/// every other failure unreportable.
pub fn fallback_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|h| h.join("Library").join("Application Support"));
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("HOME").map(PathBuf::from).map(|h| h.join(".local/share"));

    base.unwrap_or_else(std::env::temp_dir).join("AutomationPlatform")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_resolves_to_the_folder_holding_the_app() {
        let inside = PathBuf::from("/Users/t/Desktop/AutomationPlatform.app/Contents/MacOS");
        assert_eq!(strip_bundle(&inside), Some(PathBuf::from("/Users/t/Desktop")));
    }

    #[test]
    fn a_plain_directory_is_left_alone() {
        // The Windows and loose-binary cases: nothing to strip, and nothing must be
        // invented — returning None is what makes `base_dir` fall back to the exe folder.
        assert_eq!(strip_bundle(Path::new("C:/tools/platform")), None);
        assert_eq!(strip_bundle(Path::new("/usr/local/bin")), None);
    }

    #[test]
    fn a_near_miss_is_not_a_bundle() {
        // Every part has to match, or a folder that merely happens to be called MacOS
        // would send the log two levels up into somebody else's directory.
        assert_eq!(strip_bundle(Path::new("/x/Thing.app/Resources/MacOS")), None);
        assert_eq!(strip_bundle(Path::new("/x/Thing/Contents/MacOS")), None);
        assert_eq!(strip_bundle(Path::new("/x/Thing.app/Contents/Frameworks")), None);
    }
}
