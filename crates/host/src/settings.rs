//! Portable, unified settings store: per-module enabled-state + settings, kept
//! next to the executable (`<exe_dir>/settings.toml`) so the app stays portable
//! (no `%APPDATA%`). Supersedes the old `disabled-modules.txt` (auto-migrated).

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

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

    /// Atomically writes the store (tmp + rename) in the application's own folder.
    ///
    /// A failure here used to be swallowed entirely, which is survivable on a machine you
    /// can look at and not on one you cannot: an application folder that is not writable —
    /// the ordinary state of anything dragged into /Applications on macOS — means every
    /// setting and every enable/disable silently stops persisting, and the only symptom is
    /// that yesterday's choices are gone this morning. Logged once per session, because a
    /// setting change that fails will keep failing and the log is not a place to shout.
    pub fn save(&self) {
        let path = store_path();
        let complain = |what: &str| {
            static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
                crate::logging::line(
                    "settings",
                    &format!("cannot save settings to {}: {what}", store_path().display()),
                );
            }
        };
        let body = match toml::to_string_pretty(self) {
            Ok(b) => b,
            Err(e) => return complain(&format!("could not be serialized ({e})")),
        };
        let tmp = path.with_extension("toml.tmp");
        if let Err(e) = std::fs::write(&tmp, body) {
            return complain(&format!("{e}"));
        }
        if let Err(e) = std::fs::rename(&tmp, &path) {
            complain(&format!("{e}"));
        }
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
}
