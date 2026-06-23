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
    #[serde(default)]
    pub settings: BTreeMap<String, Value>,
}

/// The whole portable store (keyed by module id; `BTreeMap` = diff-stable order).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default = "one")]
    pub schema_version: u32,
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleEntry>,
}

fn one() -> u32 {
    1
}
fn yes() -> bool {
    true
}

fn dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_default()
}

fn store_path() -> PathBuf {
    dir().join("settings.toml")
}

impl Store {
    fn empty() -> Store {
        Store {
            schema_version: 1,
            modules: BTreeMap::new(),
        }
    }

    /// Loads the store, migrating a legacy `disabled-modules.txt` if present,
    /// and quarantining a corrupt `settings.toml` rather than wiping it.
    pub fn load() -> Store {
        match std::fs::read_to_string(store_path()) {
            Ok(s) => toml::from_str(&s).unwrap_or_else(|_| {
                quarantine();
                Store::empty()
            }),
            Err(_) => migrate_legacy().unwrap_or_else(Store::empty),
        }
    }

    /// Atomically writes the store (tmp + rename) next to the executable.
    pub fn save(&self) {
        if let Ok(body) = toml::to_string_pretty(self) {
            let path = store_path();
            let tmp = path.with_extension("toml.tmp");
            if std::fs::write(&tmp, body).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
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
}
