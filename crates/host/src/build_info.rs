//! Which build is running, so that a report can be matched to the download it came from.
//!
//! Two commits, because they can differ:
//!
//! * The **executable** knows the commit it was compiled from. `crates/app` compiles it in
//!   (the `git-version` crate) and hands it over first thing in `main`, through
//!   [`set_binary_commit`]. It lives there rather than here so that a new commit relinks the
//!   small launcher instead of rebuilding this crate.
//! * The **package** can name the commit it was made from, in [`FILE_NAME`] beside the
//!   application: beside the `.exe`, and beside the `.app` on macOS (the same folder as the
//!   log, see [`crate::portable`]). Read wherever it is found.
//!
//! They differ when the macOS CI job reuses an earlier run's executable because nothing under
//! `crates/` has changed since: the modules in that download are this commit's, the
//! executable is the earlier one's. That job writes the file, and it is the only thing that
//! does. The package's commit is the build the tester has (it is what the download is named
//! after), so that is the one given, and the executable's own is added when it is not the same.
//!
//! Every other package has no file and gives the compiled-in commit, like a development run
//! from `target/`: `package.ps1` and `package-macos.sh` package the executable of the checkout
//! they run in, so the executable's commit already is the package's, and they name it only in
//! the package's `README.txt`.
//!
//! The file is taken only when its `binary=` line names the commit this executable was compiled
//! from, which the job always writes. One that names another executable, or none, is left over
//! in the folder from an older download that a newer one was unpacked over, to keep the
//! settings and the log: until 2026-09-27 every package carried the file, with only `commit=`
//! in it. It names that older build, and taken at its word it would pass the new executable
//! off as the old one, in the log and in the window title. It is not used, and the log header
//! says why (see [`package_problem`]).
//!
//! Not inside the macOS bundle, although that is where it would travel with the `.app`. The
//! CI job writes this file into a package it has already signed when it reuses an executable,
//! and a file added to `Contents/` after signing breaks the seal. Signing again would give the
//! ad-hoc signed bundle a new identity, which costs a tester every permission he has granted,
//! on exactly the builds that would otherwise be the same application to macOS.

use std::path::Path;
use std::sync::OnceLock;

/// The package's record of its commit, in the application's folder, when it has one.
pub const FILE_NAME: &str = "build-info.txt";

/// What an executable compiled without git to ask says, and what one says before `main` has
/// told this module anything.
pub const UNKNOWN: &str = "unknown";

/// Appended by `git describe --dirty` when the working tree had uncommitted changes.
const MODIFIED: &str = "-modified";

static BINARY: OnceLock<&'static str> = OnceLock::new();

/// Records the commit the executable was compiled from. Called once, by `main`, before
/// anything can write the log header; a second call is ignored.
pub fn set_binary_commit(commit: &'static str) {
    let _ = BINARY.set(commit);
}

/// The commit the executable was compiled from, or [`UNKNOWN`].
pub fn binary_commit() -> &'static str {
    BINARY.get().copied().filter(|c| !c.trim().is_empty()).unwrap_or(UNKNOWN)
}

/// The package's file, read once: `Ok(None)` when there is none (a development run, or a
/// package made with its own executable), `Err` with the reason when there is one and it is
/// not used.
///
/// Read on first use, which is after `main` has called [`set_binary_commit`]: the file is
/// checked against that commit.
fn package() -> &'static Result<Option<String>, String> {
    static PACKAGE: OnceLock<Result<Option<String>, String>> = OnceLock::new();
    PACKAGE.get_or_init(|| read(&crate::portable::base_dir().join(FILE_NAME), binary_commit()))
}

/// The package's commit from the file at `path`, for an executable compiled from `binary`:
/// used only when the file's `binary=` line names that commit (see the module notes for why).
fn read(path: &Path, binary: &str) -> Result<Option<String>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} could not be read: {e}", path.display())),
    };
    let commit = match parse(&text) {
        // What a package made without git to ask said, when packages still carried the file.
        // The executable may well know its commit, and that is then the better answer than
        // "unknown".
        Some(commit) if commit.eq_ignore_ascii_case(UNKNOWN) => {
            return Err(format!("{} names no commit (it says {commit})", path.display()));
        }
        Some(commit) => commit,
        None => return Err(format!("{} names no commit", path.display())),
    };
    match value(&text, "binary") {
        Some(named) if same_commit(&named, binary) => Ok(Some(commit)),
        Some(named) => Err(format!(
            "{} is left over from another build: it names {named} as its executable, and this \
             one was built from {binary}",
            path.display()
        )),
        None => Err(format!(
            "{} is left over from an older download: it names no executable (binary=), and \
             this one was built from {binary}",
            path.display()
        )),
    }
}

/// The commit the package was made from, when it says.
pub fn package_commit() -> Option<&'static str> {
    package().as_ref().ok().and_then(|c| c.as_deref())
}

/// Why the package's file was not used, when there is one and it was not.
pub fn package_problem() -> Option<&'static str> {
    package().as_ref().err().map(String::as_str)
}

/// The log header's version line.
pub fn log_line(version: &str) -> String {
    describe(version, binary_commit(), package_commit())
}

/// The short form, for the manager window's title.
pub fn short(version: &str) -> String {
    summary(version, binary_commit(), package_commit())
}

/// `version 0.1.0, build 6c95c8b`, and ` (binary built from 1234567)` after it when the
/// executable is not the package's own commit.
pub fn describe(version: &str, binary: &str, package: Option<&str>) -> String {
    match package {
        Some(p) if !same_build(p, binary) => {
            format!("version {version}, build {p} (binary built from {binary})")
        }
        Some(p) => format!("version {version}, build {p}"),
        None => format!("version {version}, build {binary}"),
    }
}

/// `0.1.0, build 6c95c8b`: the build a tester has, without the executable's own story.
pub fn summary(version: &str, binary: &str, package: Option<&str>) -> String {
    format!("{version}, build {}", package.unwrap_or(binary))
}

/// Whether two builds name the same commit in the same state.
///
/// Compared as abbreviations: CI names a build by exactly seven characters, and `git
/// describe` gives more than seven when seven would be ambiguous, so one may be a longer
/// spelling of the other. Built from a modified tree is not the same build as the clean
/// commit, whatever the hash says.
fn same_build(a: &str, b: &str) -> bool {
    let (a, a_modified) = split_modified(a);
    let (b, b_modified) = split_modified(b);
    if a_modified != b_modified {
        return false;
    }
    let (a, b) = (a.to_ascii_lowercase(), b.to_ascii_lowercase());
    if a == b {
        return true;
    }
    let is_hash = |s: &str| s.len() >= 7 && s.bytes().all(|c| c.is_ascii_hexdigit());
    is_hash(&a) && is_hash(&b) && (a.starts_with(&b) || b.starts_with(&a))
}

/// Whether two builds name the same commit, modified or not: the executable a `binary=` line
/// names, whatever state its tree was in. That line gives the commit alone, and an executable
/// CI compiled from a checkout it had changed first is still that executable.
fn same_commit(a: &str, b: &str) -> bool {
    same_build(split_modified(a).0, split_modified(b).0)
}

fn split_modified(build: &str) -> (&str, bool) {
    match build.strip_suffix(MODIFIED) {
        Some(commit) => (commit, true),
        None => (build, false),
    }
}

/// The `commit=` line of a build-info file.
pub fn parse(text: &str) -> Option<String> {
    value(text, "commit")
}

/// The `key=` line of a build-info file.
///
/// Forgiving about what an editor or a PowerShell might do to the file (a byte-order mark,
/// CRLF, blank lines, `#` comments), strict about the value, which ends up in the log and in
/// a window title: letters, digits and `-._+`, at most 64 of them.
fn value(text: &str, key: &str) -> Option<String> {
    let value = text
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find_map(|line| {
            let (k, value) = line.split_once('=')?;
            (k.trim() == key).then(|| value.trim())
        })?;
    let valid = (1..=64).contains(&value.len())
        && value.bytes().all(|c| c.is_ascii_alphanumeric() || b"-._+".contains(&c));
    valid.then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_development_run_names_the_compiled_in_commit() {
        assert_eq!(describe("0.1.0", "6c95c8b", None), "version 0.1.0, build 6c95c8b");
        assert_eq!(
            describe("0.1.0", "6c95c8b-modified", None),
            "version 0.1.0, build 6c95c8b-modified"
        );
        assert_eq!(describe("0.1.0", UNKNOWN, None), "version 0.1.0, build unknown");
    }

    #[test]
    fn a_package_of_its_own_commit_says_it_once() {
        assert_eq!(
            describe("0.1.0", "6c95c8b", Some("6c95c8b")),
            "version 0.1.0, build 6c95c8b"
        );
        // Seven characters from CI, eight from `git describe` where seven were ambiguous.
        assert_eq!(
            describe("0.1.0", "6c95c8b1", Some("6c95c8b")),
            "version 0.1.0, build 6c95c8b"
        );
        assert_eq!(
            describe("0.1.0", "6C95C8B", Some("6c95c8b")),
            "version 0.1.0, build 6c95c8b"
        );
        // A local package of a modified tree: both say so, and it is still one build.
        assert_eq!(
            describe("0.1.0", "6c95c8b-modified", Some("6c95c8b-modified")),
            "version 0.1.0, build 6c95c8b-modified"
        );
    }

    #[test]
    fn a_reused_executable_is_named_beside_the_package() {
        assert_eq!(
            describe("0.1.0", "1234567", Some("6c95c8b")),
            "version 0.1.0, build 6c95c8b (binary built from 1234567)"
        );
        // Same commit, but the executable was built from a modified tree.
        assert_eq!(
            describe("0.1.0", "6c95c8b-modified", Some("6c95c8b")),
            "version 0.1.0, build 6c95c8b (binary built from 6c95c8b-modified)"
        );
        assert_eq!(
            describe("0.1.0", UNKNOWN, Some("6c95c8b")),
            "version 0.1.0, build 6c95c8b (binary built from unknown)"
        );
    }

    #[test]
    fn only_real_abbreviations_count_as_the_same_commit() {
        // Too short to be an abbreviation git would print, and not hexadecimal.
        assert!(!same_build("6c95", "6c95c8b"));
        assert!(!same_build("unknown", "unknow"));
        assert!(same_build("unknown", "unknown"));
        assert!(!same_build("6c95c8b", "6c95c8c"));
    }

    #[test]
    fn the_title_names_the_package_build() {
        assert_eq!(summary("0.1.0", "1234567", Some("6c95c8b")), "0.1.0, build 6c95c8b");
        assert_eq!(summary("0.1.0", "6c95c8b-modified", None), "0.1.0, build 6c95c8b-modified");
    }

    #[test]
    fn the_file_is_read_the_way_it_is_written() {
        // What the macOS CI job writes beside a reused executable: the package's commit, and
        // the executable's own under a key of its own, which is not this one.
        let written = "# The commit this package was made from.\ncommit=6c95c8b\nbinary=1234567\n";
        assert_eq!(parse(written).as_deref(), Some("6c95c8b"));
        assert_eq!(parse("binary=1234567\ncommit=6c95c8b\n").as_deref(), Some("6c95c8b"));
        assert_eq!(value(written, "binary").as_deref(), Some("1234567"));
        assert_eq!(value("commit=6c95c8b\n", "binary"), None);
        // Edited by hand: Windows PowerShell's UTF-8 has a byte-order mark; an editor may add
        // CRLF and spaces.
        assert_eq!(parse("\u{feff}commit = 6c95c8b \r\n").as_deref(), Some("6c95c8b"));
        assert_eq!(parse("commit=6c95c8b\n").as_deref(), Some("6c95c8b"));
        assert_eq!(parse("commit=6c95c8b-modified").as_deref(), Some("6c95c8b-modified"));
    }

    #[test]
    fn a_file_that_says_nothing_usable_is_not_used() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("# commit=6c95c8b\n"), None);
        assert_eq!(parse("6c95c8b\n"), None);
        assert_eq!(parse("commit=\n"), None);
        assert_eq!(parse("commit=6c95 c8b\n"), None);
        assert_eq!(parse("commit=<script>\n"), None);
        assert_eq!(parse(&format!("commit={}\n", "a".repeat(65))), None);
    }

    #[test]
    fn reading_tells_a_missing_file_from_a_useless_one() {
        let dir = std::env::temp_dir().join(format!("osap-build-info-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let _ = std::fs::remove_file(&path);
        assert_eq!(read(&path, "1234567"), Ok(None));
        std::fs::write(&path, "commit=6c95c8b\nbinary=1234567\n").unwrap();
        assert_eq!(read(&path, "1234567"), Ok(Some("6c95c8b".to_string())));
        std::fs::write(&path, "nothing here\n").unwrap();
        assert!(read(&path, "1234567").unwrap_err().contains("names no commit"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_is_used_only_beside_the_executable_it_names() {
        let dir =
            std::env::temp_dir().join(format!("osap-build-info-binary-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        let read_as = |text: &str, binary: &str| {
            std::fs::write(&path, text).unwrap();
            read(&path, binary)
        };
        let reused = "# The commit this package was made from.\ncommit=6c95c8b\nbinary=1234567\n";
        let this = Ok(Some("6c95c8b".to_string()));
        // What the reuse step writes, beside the executable it names.
        assert_eq!(read_as(reused, "1234567"), this);
        // `git describe` gave more than seven characters, or the tree was modified when CI
        // compiled it: still the executable the line names.
        assert_eq!(read_as(reused, "12345678"), this);
        assert_eq!(read_as(reused, "1234567-modified"), this);
        // A newer download unpacked over an older reused one: another executable.
        let other = read_as(reused, "7654321").unwrap_err();
        // And over any package from before 2026-09-27, whose file had no `binary=` at all.
        let older =
            read_as("# The commit this package was made from.\ncommit=6c95c8b\n", "7654321")
                .unwrap_err();
        // Even when it names this executable's own commit: nothing says it belongs to it.
        let same = read_as("commit=7654321\n", "7654321").unwrap_err();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(other.contains("left over from another build"), "{other}");
        assert!(other.contains("names 1234567 as its executable"), "{other}");
        assert!(other.contains("built from 7654321"), "{other}");
        assert!(older.contains("left over from an older download"), "{older}");
        assert!(same.contains("names no executable"), "{same}");
        // So the header and the title name the executable's own commit.
        assert_eq!(describe("0.1.0", "7654321", None), "version 0.1.0, build 7654321");
        assert_eq!(summary("0.1.0", "7654321", None), "0.1.0, build 7654321");
    }

    #[test]
    fn a_package_made_without_git_leaves_the_build_to_the_executable() {
        let dir =
            std::env::temp_dir().join(format!("osap-build-info-unknown-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(FILE_NAME);
        // A file an older package wrote where git could not be asked, which says so instead of
        // a commit; with or without a `binary=`, it names no build.
        std::fs::write(&path, "# The commit this package was made from.\ncommit=unknown\n").unwrap();
        let read_back = read(&path, "6c95c8b");
        std::fs::write(&path, "commit=unknown\nbinary=6c95c8b\n").unwrap();
        let read_back_named = read(&path, "6c95c8b");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(read_back.unwrap_err().contains("names no commit"));
        assert!(read_back_named.unwrap_err().contains("names no commit"));
        // So the package names nothing, and the header names the compiled-in commit.
        assert_eq!(describe("0.1.0", "6c95c8b", None), "version 0.1.0, build 6c95c8b");
        assert_eq!(summary("0.1.0", "6c95c8b", None), "0.1.0, build 6c95c8b");
    }
}
