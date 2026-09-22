//! Portable, unified settings store: per-module enabled-state + settings, kept
//! next to the executable (`<exe_dir>/settings.toml`) so the app stays portable
//! (no `%APPDATA%`). Supersedes the old `disabled-modules.txt` (auto-migrated).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A stored scalar setting value. Luau has only `number`; we keep `Int` vs
/// `Float` for clean TOML round-tripping but treat both as the `Number` kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

/// The pinned kind of a setting (derived from its `define` default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Number,
    Str,
}

impl Value {
    pub fn kind(&self) -> Kind {
        match self {
            Value::Bool(_) => Kind::Bool,
            Value::Int(_) | Value::Float(_) => Kind::Number,
            Value::Str(_) => Kind::Str,
        }
    }
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
}

/// A registered setting's schema (in-memory; not persisted). Drives validation
/// and the settings GUI.
#[derive(Debug, Clone)]
pub struct Field {
    pub kind: Kind,
    pub label: String,
    pub default: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub choices: Option<Vec<String>>,
}

impl Field {
    /// Validates a value against this field, returning an error message on fail.
    pub fn validate(&self, v: &Value) -> Result<(), String> {
        if v.kind() != self.kind {
            return Err(format!("expected {:?}, got {:?}", self.kind, v.kind()));
        }
        if let Some(n) = v.as_f64() {
            if let Some(min) = self.min {
                if n < min {
                    return Err(format!("must be >= {min}"));
                }
            }
            if let Some(max) = self.max {
                if n > max {
                    return Err(format!("must be <= {max}"));
                }
            }
        }
        if let (Value::Str(s), Some(choices)) = (v, &self.choices) {
            if !choices.iter().any(|c| c == s) {
                return Err(format!("must be one of {choices:?}"));
            }
        }
        Ok(())
    }
}

/// One module's record in the store: its enabled flag + persisted settings.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ModuleEntry {
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Left out of the file entirely when empty, rather than written as a bare
    /// `[modules."x".settings]` header with nothing under it.
    ///
    /// Most modules declare no settings at all, so the file was mostly section headers
    /// standing for nothing — and a person opening it to check one value had to read past
    /// eleven of them. An empty table and an absent one deserialize identically (`default`),
    /// so nothing is lost by not writing it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, Value>,
}

/// The whole portable store (keyed by module id; `BTreeMap` = diff-stable order).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "one")]
    pub schema_version: u32,
    /// The application's own settings — the ones that used to be environment variables.
    /// See [`crate::appcfg`] for what they are and why they moved.
    ///
    /// A map rather than a struct, so that a settings file written by a newer build keeps
    /// its unknown keys instead of losing them the next time an older build saves.
    ///
    /// Omitted when empty, for the same reason as a module's: with every switch off there is
    /// nothing to say, and a lone `[app]` heading says it at length.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub app: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modules: BTreeMap<String, ModuleEntry>,
}

fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}

fn dir() -> PathBuf {
    crate::portable::base_dir().to_path_buf()
}

fn store_path() -> PathBuf {
    dir().join("settings.toml")
}

/// Replaces `path` with `body` so that a crash at any moment leaves either the old file or
/// the new one, whole — never a truncated one, and never none.
///
/// This used to write a fixed `settings.toml.tmp` with `fs::write` and rename it over the
/// store, and both halves of that could lose every setting. Nothing was flushed, so after a
/// power cut or a crash of the machine the rename could be on disk before the data it
/// renamed was, and the store came back empty or cut short — which `Store::load` then
/// quarantines as corrupt and replaces with nothing. And the name was fixed, so a second
/// instance saving at the same moment wrote into the same temporary file as the first.
///
/// So: `tempfile`'s `NamedTempFile`, in the SAME folder (a rename is only atomic within one
/// volume), under a name of its own for every save; the data flushed to the disk with
/// `sync_all` before the rename; then the rename. A save that fails anywhere before the rename
/// is done deletes its temporary file on the way out and leaves the store as it was. A crash
/// in the middle can leave one behind, named `settings.*.toml.tmp`, beside the store; nothing
/// reads those, module discovery only looks in `modules/`, and `sweep_leftovers` removes them
/// at the next start.
///
/// The rename is `std::fs::rename`, NOT `tempfile`'s `persist`. On Windows `persist` is
/// `MoveFileExW` and nothing else, and `MoveFileExW` refuses to replace a file another program
/// has open — a second instance loading it, a virus scanner, the search indexer, a sync
/// client, an editor — so a save through it failed there ("access denied", measured) where
/// the old save had worked. `std::fs::rename` tries the same call and, when it is refused,
/// renames with POSIX semantics (`SetFileInformationByHandle`), which replaces the file while
/// it is open. `into_temp_path().keep()` closes our own handle first and, on Windows, clears
/// the temporary-file attribute `tempfile` creates the file with, as `persist` would have.
///
/// The flush is best effort: when it fails, the store is replaced anyway and the error comes
/// back as `Ok(Some(error))` for the caller to report. On macOS `sync_all` is
/// `fcntl(F_FULLFSYNC)`, which only some file systems support (HFS+, APFS, FAT, UDF; not an
/// SMB or NFS share), and failing the save there would have stopped a portable folder on a
/// network drive saving at all — where the old, unflushed save had worked. On Windows it is
/// `FlushFileBuffers`.
///
/// On a Unix the file is created with mode 0666 less the umask — the mode `fs::write` gave
/// the old store, and not `tempfile`'s own 0600, which would have made the store unreadable to
/// every other account on a Mac where the application sits in a shared folder (and
/// `Store::load` reads an unreadable store as no store). The folder is flushed as well once
/// the rename is done, because there the rename is an entry in the folder and is only durable
/// once the folder is; Windows has no such step for a directory.
fn replace_file(path: &Path, body: &[u8]) -> std::io::Result<Option<std::io::Error>> {
    use std::io::Write;
    let dir = match path.parent() {
        Some(d) if !d.as_os_str().is_empty() => d,
        _ => Path::new("."),
    };
    let mut builder = tempfile::Builder::new();
    builder.prefix(LEFTOVER_PREFIX).suffix(LEFTOVER_SUFFIX);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(std::fs::Permissions::from_mode(0o666));
    }
    let mut tmp = builder.tempfile_in(dir)?;
    tmp.write_all(body)?;
    let unflushed = tmp.as_file().sync_all().err();
    // From here the temporary file is no longer deleted on drop, so every way out below that
    // does not end in the rename deletes it by hand.
    let staged = tmp.into_temp_path().keep().map_err(|e| e.error)?;
    if let Err(e) = std::fs::rename(&staged, path) {
        let _ = std::fs::remove_file(&staged);
        return Err(e);
    }
    #[cfg(unix)]
    {
        // Best effort: the new file is already in place, and a folder that cannot be opened
        // for this is not a reason to report the save as failed.
        if let Ok(folder) = std::fs::File::open(dir) {
            let _ = folder.sync_all();
        }
    }
    Ok(unflushed)
}

/// Removes, once at start-up, the temporary files crashed saves left beside the store (see
/// `sweep_leftovers`) — those a minute old at least, so another instance's save in progress
/// is not one of them — and logs how many when there were any. Called by `run` once the log
/// is open, which the first `Store::load` is too early for.
pub fn sweep_leftovers_at_start() {
    let removed = sweep_leftovers(&dir(), std::time::Duration::from_secs(60));
    if removed > 0 {
        crate::logging::line(
            "settings",
            &format!("removed {removed} leftover temporary file(s) of interrupted saves"),
        );
    }
}

/// How `replace_file` names its temporary files: `settings.<random>.toml.tmp`. The fixed name
/// the save used before, `settings.toml.tmp`, fits the same pattern.
const LEFTOVER_PREFIX: &str = "settings.";
const LEFTOVER_SUFFIX: &str = ".toml.tmp";

/// Removes the temporary files that saves interrupted by a crash left in `dir`, and returns
/// how many it removed.
///
/// Every save uses a new random name, so nothing else ever overwrites such a leftover, and
/// without this they would pile up beside the store for good — one per crash in the middle
/// of a save, plus the fixed `settings.toml.tmp` an older build may have left. Only files at
/// least `older_than` old are removed: a younger one may belong to a save another instance is
/// making at this moment, and deleting it would fail that save.
fn sweep_leftovers(dir: &Path, older_than: std::time::Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    let now = std::time::SystemTime::now();
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !(name.starts_with(LEFTOVER_PREFIX) && name.ends_with(LEFTOVER_SUFFIX)) {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let old = meta
            .modified()
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|age| age >= older_than);
        if meta.is_file() && old && std::fs::remove_file(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

impl Store {
    fn empty() -> Store {
        Store {
            schema_version: 1,
            app: BTreeMap::new(),
            modules: BTreeMap::new(),
        }
    }

    /// One application setting as a bool, or `None` when it has never been set.
    pub fn app_flag(&self, key: &str) -> Option<bool> {
        match self.app.get(key) {
            Some(Value::Bool(b)) => Some(*b),
            _ => None,
        }
    }

    /// Records one application setting. Saving is the caller's business, as everywhere else
    /// in this store.
    pub fn set_app_flag(&mut self, key: &str, on: bool) {
        self.app.insert(key.to_string(), Value::Bool(on));
    }

    /// Reads the store and changes nothing: no quarantine of a file that does not parse (an
    /// empty store is answered instead), no rewrite into this build's form, no migration.
    ///
    /// For the one read that happens before this process knows whether it is the running copy
    /// (`run`, for the application settings `headless` among them). A second copy that went on
    /// to be turned away must not have renamed or rewritten the running copy's file — through
    /// the same temporary file its own saves use — on the way out.
    pub fn peek() -> Store {
        Store::peek_at(&store_path())
    }

    fn peek_at(path: &std::path::Path) -> Store {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_else(Store::empty)
    }

    /// Loads the store, migrating a legacy `disabled-modules.txt` if present,
    /// and quarantining a corrupt `settings.toml` rather than wiping it.
    pub fn load() -> Store {
        match std::fs::read_to_string(store_path()) {
            Ok(s) => {
                let store: Store = match toml::from_str(&s) {
                    Ok(v) => v,
                    Err(_) => {
                        quarantine();
                        return Store::empty();
                    }
                };
                // Rewrite once when the file on disk is not what this build would write.
                //
                // Otherwise a change in what gets serialized only reaches the file the next
                // time somebody happens to alter a setting — so the empty `[…settings]`
                // headings this build stopped writing would have sat there for weeks, and the
                // person who reported them would have had to take it on trust. Re-serializing
                // preserves unknown keys (both maps are open), so a file written by a newer
                // build survives this untouched.
                if toml::to_string_pretty(&store).map(|b| b != s).unwrap_or(false) {
                    store.save();
                }
                store
            }
            Err(_) => migrate_legacy().unwrap_or_else(Store::empty),
        }
    }

    /// Atomically replaces the store in the application's own folder — see `replace_file`.
    ///
    /// A failure here used to be swallowed entirely, which is survivable on a machine you
    /// can look at and not on one you cannot: an application folder that is not writable —
    /// the ordinary state of anything dragged into /Applications on macOS — means every
    /// setting and every enable/disable silently stops persisting, and the only symptom is
    /// that yesterday's choices are gone this morning. Logged once per session, because a
    /// setting change that fails will keep failing and the log is not a place to shout.
    pub fn save(&self) {
        use std::sync::atomic::{AtomicBool, Ordering};
        match self.save_to(&store_path()) {
            Ok(None) => {}
            // Saved, but not flushed first — see `replace_file`. Said once, like a failure: a
            // volume that cannot flush will not learn to.
            Ok(Some(flush)) => {
                static SAID: AtomicBool = AtomicBool::new(false);
                if !SAID.swap(true, Ordering::Relaxed) {
                    crate::logging::line(
                        "settings",
                        &format!(
                            "saved settings to {} without flushing them to disk first ({flush}); \
                             a power cut right after a save can lose that save",
                            store_path().display()
                        ),
                    );
                }
            }
            Err(what) => {
                static SAID: AtomicBool = AtomicBool::new(false);
                if !SAID.swap(true, Ordering::Relaxed) {
                    crate::logging::line(
                        "settings",
                        &format!("cannot save settings to {}: {what}", store_path().display()),
                    );
                }
            }
        }
    }

    /// `save`, to a path of the caller's choosing, saying what went wrong instead of logging
    /// it — the part of `save` a test can run. `Ok(Some(_))` is a save that went through
    /// without its flush (see `replace_file`).
    fn save_to(&self, path: &Path) -> Result<Option<String>, String> {
        let body =
            toml::to_string_pretty(self).map_err(|e| format!("could not be serialized ({e})"))?;
        replace_file(path, body.as_bytes())
            .map(|unflushed| unflushed.map(|e| e.to_string()))
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str, key: &str) -> Option<Value> {
        self.modules.get(id).and_then(|m| m.settings.get(key)).cloned()
    }

    /// Sets a setting value, returning the previous value (if any).
    pub fn set(&mut self, id: &str, key: &str, value: Value) -> Option<Value> {
        self.modules
            .entry(id.to_string())
            .or_default()
            .settings
            .insert(key.to_string(), value)
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) {
        self.modules.entry(id.to_string()).or_default().enabled = enabled;
    }

    /// Clones the current record for `id` (if any). Used to snapshot a module's
    /// persisted state before a fallible (re)load, so a partial load's settings
    /// writes can be rolled back without disturbing a prior successful run's values.
    pub fn snapshot(&self, id: &str) -> Option<ModuleEntry> {
        self.modules.get(id).cloned()
    }

    /// Restores a record captured by [`Store::snapshot`]: re-inserts the prior
    /// entry, or removes the module entirely if there was none — discarding any
    /// settings a failed load wrote (the store half of load rollback).
    pub fn restore(&mut self, id: &str, entry: Option<ModuleEntry>) {
        match entry {
            Some(e) => {
                self.modules.insert(id.to_string(), e);
            }
            None => {
                self.modules.remove(id);
            }
        }
    }

    /// Module ids present in the store and marked disabled.
    pub fn disabled_ids(&self) -> HashSet<String> {
        self.modules
            .iter()
            .filter(|(_, m)| !m.enabled)
            .map(|(id, _)| id.clone())
            .collect()
    }
}

/// Renames a corrupt store aside so a hand-edit isn't silently wiped.
fn quarantine() {
    let q = dir().join(format!("settings.toml.corrupt-{}", epoch_secs()));
    let _ = std::fs::rename(store_path(), q);
}

fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Migrates a legacy `disabled-modules.txt`: seed `enabled = false` for those
/// ids, write `settings.toml`, and rename the old file to `.bak`.
fn migrate_legacy() -> Option<Store> {
    let legacy = dir().join("disabled-modules.txt");
    let text = std::fs::read_to_string(&legacy).ok()?;
    let mut store = Store::empty();
    for line in text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        store.set_enabled(line, false);
    }
    store.save();
    let _ = std::fs::rename(&legacy, dir().join("disabled-modules.txt.bak"));
    Some(store)
}

#[cfg(test)]
mod store_rollback_tests {
    use super::*;

    #[test]
    fn snapshot_then_restore_rolls_back_a_partial_load() {
        let mut s = Store::empty();
        // A prior successful run persisted a value.
        s.set("m", "vol", Value::Int(7));
        let snap = s.snapshot("m");
        // A later (re)load partially runs: overwrites a key, adds another, then fails.
        s.set("m", "vol", Value::Int(99));
        s.set("m", "extra", Value::Bool(true));
        s.restore("m", snap);
        // The prior value is intact; the partial load's writes are discarded.
        assert_eq!(s.get("m", "vol"), Some(Value::Int(7)));
        assert_eq!(s.get("m", "extra"), None);
    }

    #[test]
    fn restore_removes_the_orphan_of_a_first_time_failed_load() {
        let mut s = Store::empty();
        let snap = s.snapshot("new"); // no prior entry
        assert!(snap.is_none());
        // A first load writes settings, then fails.
        s.set("new", "k", Value::Str("x".into()));
        s.restore("new", snap);
        // No orphan section left behind for a module that never finished loading.
        assert!(s.get("new", "k").is_none());
        assert!(s.snapshot("new").is_none());
    }

    /// A module with no settings writes no settings section — and the file still round-trips.
    ///
    /// Eleven modules ship here and two of them declare a setting, so the file was nine
    /// headings standing for nothing. Someone opening it to check one value had to read past
    /// them all, which for a file whose whole purpose is being readable by hand is the wrong
    /// trade for a byte.
    #[test]
    fn an_empty_settings_table_is_not_written_but_still_reads_back() {
        // Real ids, because they carry dots and TOML then quotes the key — the assertions
        // below would otherwise be testing a spelling this file never actually writes.
        let (plain, off, configured) =
            ("com.platform.plain", "com.platform.off", "com.platform.configured");
        let mut s = Store::empty();
        s.set_enabled(plain, true);
        s.set_enabled(off, false);
        s.set(configured, "vol", Value::Int(7));
        let text = toml::to_string_pretty(&s).expect("serialize");
        assert!(!text.contains(&format!("\"{plain}\".settings")), "empty table written:\n{text}");
        assert!(!text.contains(&format!("\"{off}\".settings")), "empty table written:\n{text}");
        assert!(
            text.contains(&format!("\"{configured}\".settings")),
            "real settings lost:\n{text}"
        );
        // The absent table and an empty one mean the same thing on the way back in, which is
        // what makes leaving it out safe rather than merely tidier.
        let back: Store = toml::from_str(&text).expect("deserialize");
        assert_eq!(back.modules.get(plain).map(|m| m.enabled), Some(true));
        assert_eq!(back.modules.get(off).map(|m| m.enabled), Some(false));
        assert!(back.modules[plain].settings.is_empty());
        assert_eq!(back.modules[configured].settings.get("vol"), Some(&Value::Int(7)));
    }

    /// With every application switch off, no `[app]` heading either.
    #[test]
    fn an_untouched_app_section_is_not_written() {
        let s = Store::empty();
        let text = toml::to_string_pretty(&s).expect("serialize");
        assert!(!text.contains("[app]"), "empty app table written:\n{text}");
        assert!(toml::from_str::<Store>(&text).is_ok());
    }

    /// The read a start makes before it knows whether it is the running copy leaves the file
    /// exactly as it found it: a broken one is neither renamed nor replaced, and one in an
    /// older form is not rewritten.
    #[test]
    fn peeking_changes_nothing_on_disk() {
        let dir = std::env::temp_dir().join(format!("ap-test-peek-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.toml");

        std::fs::write(&path, "this is [not toml").unwrap();
        let s = Store::peek_at(&path);
        assert!(s.app.is_empty() && s.modules.is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "this is [not toml");
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "nothing quarantined");

        // Valid, but not in the form this build writes (spacing, an empty heading).
        let old = "schema_version = 1\n[app]\nheadless   =   true\n[modules.\"a.b\"]\nenabled = false\n[modules.\"a.b\".settings]\n";
        std::fs::write(&path, old).unwrap();
        let s = Store::peek_at(&path);
        assert_eq!(s.app_flag("headless"), Some(true));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod store_save_tests {
    use super::*;

    /// Every name in `dir`, sorted — what a save left behind.
    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    /// A save replaces the store with the whole new file and leaves nothing else behind.
    #[test]
    fn a_save_replaces_the_store_and_leaves_no_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");

        let mut s = Store::empty();
        s.set("com.example.a", "vol", Value::Int(7));
        // `None`: flushed as well. A local temporary folder can always flush.
        assert_eq!(s.save_to(&path).unwrap(), None);
        let back: Store = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back.get("com.example.a", "vol"), Some(Value::Int(7)));

        // Over an existing store, which is every save but the first.
        s.set("com.example.a", "vol", Value::Int(9));
        s.set_enabled("com.example.b", false);
        s.save_to(&path).unwrap();
        let back: Store = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back.get("com.example.a", "vol"), Some(Value::Int(9)));
        assert!(back.disabled_ids().contains("com.example.b"));

        assert_eq!(names(dir.path()), vec!["settings.toml".to_string()]);
    }

    /// A save that cannot complete leaves the old store exactly as it was, says why, and
    /// cleans up after itself.
    ///
    /// The failure is made at the rename, the last step: the target is a FOLDER, which neither
    /// platform lets a file replace. Everything before it — the temporary file, its contents,
    /// the flush — has happened by then, so this is the case in which a leftover would appear.
    #[test]
    fn a_failed_save_leaves_the_old_file_and_no_temporary_one() {
        let dir = tempfile::tempdir().unwrap();
        let blocked = dir.path().join("settings.toml");
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("keep.txt"), "untouched").unwrap();

        let err = Store::empty().save_to(&blocked).unwrap_err();
        assert!(!err.is_empty());
        assert_eq!(std::fs::read_to_string(blocked.join("keep.txt")).unwrap(), "untouched");
        assert_eq!(names(dir.path()), vec!["settings.toml".to_string()], "a leftover remained");
    }

    /// A store another program has open is still replaced.
    ///
    /// Something has `settings.toml` open more often than one would think: a second instance
    /// loading it, a virus scanner, the search indexer, a sync client over a portable folder,
    /// an editor. On Windows `MoveFileExW` refuses to replace a file that is open ("access
    /// denied"), and `tempfile`'s `persist` is `MoveFileExW` and nothing else — so a save
    /// through it failed there, was logged once, and the change was lost. `std::fs::rename`
    /// falls back to a POSIX-semantics rename, which replaces the file while it is open, and
    /// that is what the old save used. Elsewhere a rename over an open file always works.
    #[test]
    fn a_save_replaces_a_store_another_program_has_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut s = Store::empty();
        s.set("com.example.a", "vol", Value::Int(1));
        s.save_to(&path).unwrap();

        // Opened the way a reader opens it: `File::open`, which on Windows shares read, write
        // and delete — and still blocks `MoveFileExW`.
        let reader = std::fs::File::open(&path).unwrap();
        s.set("com.example.a", "vol", Value::Int(2));
        let saved = s.save_to(&path);
        drop(reader);
        saved.expect("a save while another program has the store open");

        let back: Store = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back.get("com.example.a", "vol"), Some(Value::Int(2)));
        assert_eq!(names(dir.path()), vec!["settings.toml".to_string()]);
    }

    /// A folder that does not exist is an error, not a panic and not a file somewhere else.
    #[test]
    fn a_save_into_a_missing_folder_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone").join("settings.toml");
        assert!(Store::empty().save_to(&path).is_err());
        assert!(names(dir.path()).is_empty());
    }

    /// Two saves at once — two instances, say — each go through a temporary file of their
    /// own, so the store ends as one of them, whole. The fixed `settings.toml.tmp` this
    /// replaced let the second writer truncate the first one's half-written file.
    #[test]
    fn concurrent_saves_each_land_whole() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let stores: Vec<Store> = (0..8)
            .map(|i| {
                let mut s = Store::empty();
                // Big enough that a write is not one system call's worth of luck.
                for k in 0..200 {
                    s.set(&format!("com.example.m{i}"), &format!("key{k}"), Value::Int(k));
                }
                s
            })
            .collect();
        std::thread::scope(|scope| {
            for s in &stores {
                let path = path.clone();
                scope.spawn(move || {
                    // A rename that loses to another one at the wrong moment may still be
                    // refused on Windows, even with the fallback `std::fs::rename` has; that
                    // is a failed save, reported, and the store is still whole — which is
                    // what is asserted below.
                    let _ = s.save_to(&path);
                });
            }
        });
        let text = std::fs::read_to_string(&path).unwrap();
        let back: Store = toml::from_str(&text).expect("the store is whole");
        assert_eq!(back.modules.len(), 1, "exactly one writer's store");
        assert_eq!(back.modules.values().next().unwrap().settings.len(), 200);
        assert_eq!(names(dir.path()), vec!["settings.toml".to_string()]);
    }

    /// On a Unix the store gets the mode a plain `fs::write` gives a new file — 0666 less the
    /// umask, as the old save gave it — and not `tempfile`'s 0600, which would lock every other
    /// account out of it. Compared against a file written the old way in the same folder, so
    /// the test holds whatever the umask is.
    #[cfg(unix)]
    #[test]
    fn a_saved_store_has_the_mode_a_plain_write_gives() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        Store::empty().save_to(&path).unwrap();
        let plain = dir.path().join("plain");
        std::fs::write(&plain, "").unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), mode(&plain));
    }

    /// The sweep at start-up takes the temporary files interrupted saves left, old enough not
    /// to be another instance's save in progress, and nothing else.
    #[test]
    fn the_sweep_takes_old_leftovers_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(120);
        let make = |name: &str, aged: bool| {
            let p = dir.path().join(name);
            std::fs::write(&p, "x").unwrap();
            if aged {
                std::fs::File::options().write(true).open(&p).unwrap().set_modified(old).unwrap();
            }
        };
        make("settings.a1B2c3.toml.tmp", true); // a crashed save
        make("settings.toml.tmp", true); // the fixed name an older build used
        make("settings.d4E5f6.toml.tmp", false); // a save in progress, perhaps another instance's
        make("settings.toml", true);
        make("settings.toml.corrupt-1700000000", true); // `quarantine`'s, kept on purpose
        make("other.toml.tmp", true);
        std::fs::create_dir(dir.path().join("settings.folder.toml.tmp")).unwrap();

        assert_eq!(sweep_leftovers(dir.path(), std::time::Duration::from_secs(60)), 2);
        assert_eq!(
            names(dir.path()),
            vec![
                "other.toml.tmp",
                "settings.d4E5f6.toml.tmp",
                "settings.folder.toml.tmp",
                "settings.toml",
                "settings.toml.corrupt-1700000000",
            ]
        );
        // A folder that cannot be read is nothing to sweep, not a panic.
        assert_eq!(sweep_leftovers(&dir.path().join("gone"), std::time::Duration::ZERO), 0);
    }
}
