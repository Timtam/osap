//! Module package manifest (`module.toml`) + loading a module from an unpacked
//! directory (dev mode). See `docs/module-package-format.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Contents of `module.toml`.
#[derive(Debug, Deserialize)]
pub struct ModuleManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    /// Entry point relative to the package root. Default: `src/main.luau`.
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub engine_api: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// Ids of other modules this one depends on (their exports become available
    /// via `host.require`). Loaded first; auto-discovered among sibling modules.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// When true, this module's *code* is evaluated inside the VM of every module
    /// that depends on it, so its functions — not just serialized data — are
    /// reachable through `host.require`. Default (false) keeps the legacy
    /// data-only export path. Migration flag: the long-term model loads every
    /// dependency this way (one VM per dependency tree); see
    /// `docs/nested-overlays-design.md`.
    #[serde(default)]
    pub code_module: bool,
    #[serde(default)]
    pub capabilities: Capabilities,
}

/// `[capabilities]` block: which host capabilities the module requests (default-deny).
#[derive(Debug, Default, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub require: Vec<String>,
}

fn default_entry() -> String {
    "src/main.luau".to_string()
}

/// The module id of a dependency spec. A spec is either a bare id (`"com.x.y"`)
/// or an id followed by a space-separated semver requirement (`"com.x.y >= 1.2"`);
/// the id is everything up to the first whitespace.
pub fn dep_id(spec: &str) -> &str {
    spec.split_whitespace().next().unwrap_or("")
}

/// The version-constraint part of a dependency spec (the text after the id), or
/// `None` for a bare id. e.g. `"com.x >= 1.2"` → `Some(">= 1.2")`.
pub fn dep_constraint(spec: &str) -> Option<&str> {
    let trimmed = spec.trim();
    let ws = trimmed.find(char::is_whitespace)?;
    let rest = trimmed[ws..].trim();
    (!rest.is_empty()).then_some(rest)
}

/// A loaded module: package root + parsed manifest.
#[derive(Debug)]
pub struct LoadedModule {
    pub root: PathBuf,
    pub manifest: ModuleManifest,
}

impl LoadedModule {
    /// Loads a module from either an unpacked directory or a `.zip` package
    /// (extracted on demand into a content-addressed cache).
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if path.is_dir() {
            Self::load_dir(path)
        } else {
            Self::load_zip(path)
        }
    }

    /// Loads a module from an unpacked directory (dev mode).
    pub fn load_dir(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let manifest_path = root.join("module.toml");
        let text = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("module.toml not readable: {}", manifest_path.display()))?;
        let manifest: ModuleManifest =
            toml::from_str(&text).context("module.toml is not valid TOML")?;
        Ok(Self { root, manifest })
    }

    /// Absolute path to the entry point.
    pub fn entry_path(&self) -> PathBuf {
        self.root.join(&self.manifest.entry)
    }

    /// Resolves a package-relative resource path against the package root
    /// (basis for `host.path()` / `host.resource.read()`).
    pub fn resolve(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    /// Extracts a `.zip` package into a content-addressed cache dir and loads it.
    fn load_zip(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("module package not readable: {}", path.display()))?;
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut h);
            h.finish()
        };
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes))
            .with_context(|| format!("not a valid module package: {}", path.display()))?;

        let manifest: ModuleManifest = {
            let mut entry = archive
                .by_name("module.toml")
                .context("module.toml missing in package root")?;
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text)?;
            toml::from_str(&text).context("module.toml is not valid TOML")?
        };

        // Content-addressed cache: <temp>/automation-platform-modules/<id>/<version>-<hash>/
        let cache = std::env::temp_dir()
            .join("automation-platform-modules")
            .join(&manifest.id)
            .join(format!("{}-{hash:016x}", manifest.version));
        if !cache.join("module.toml").exists() {
            std::fs::create_dir_all(&cache)?;
            archive
                .extract(&cache)
                .context("failed to extract module package")?;
        }
        Self::load_dir(cache)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dependency_spec_parsing() {
        // Bare id (backward compatible): no constraint.
        assert_eq!(dep_id("com.x.y"), "com.x.y");
        assert_eq!(dep_constraint("com.x.y"), None);
        // Id + space-separated semver requirement.
        assert_eq!(dep_id("com.x.y >= 1.2"), "com.x.y");
        assert_eq!(dep_constraint("com.x.y >= 1.2"), Some(">= 1.2"));
        // Surrounding / extra whitespace is tolerated.
        assert_eq!(dep_id("  com.x.y   ^2  "), "com.x.y");
        assert_eq!(dep_constraint("  com.x.y   ^2  "), Some("^2"));
    }
}
