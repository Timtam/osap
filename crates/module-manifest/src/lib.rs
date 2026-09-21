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
    /// Ids of modules this one can USE if present but does not require. Each is
    /// loaded (and, for a code_module, evaluated into this module's VM) only when it
    /// is actually available; a missing one is skipped silently, and `host.require`
    /// of it returns nil so the module can adapt. Same spec grammar as `dependencies`.
    /// Unlike a hard dependency, an optional one never blocks the dependent from
    /// loading, and removing it never blocks uninstall. (e.g. Kontakt optionally uses
    /// Komplete Kontrol to detect itself hosted inside a standalone KK window.)
    #[serde(default)]
    pub optional_dependencies: Vec<String>,
    /// When true, this module's *code* is evaluated inside the VM of every module
    /// that depends on it, so its functions — not just serialized data — are
    /// reachable through `host.require`. Default (false) keeps the legacy
    /// data-only export path. Migration flag: the long-term model loads every
    /// dependency this way (one VM per dependency tree); see
    /// `docs/nested-overlays-design.md`.
    #[serde(default)]
    pub code_module: bool,
    /// Operating systems this module is written for, as `std::env::consts::OS` names —
    /// `["windows", "macos"]`. **Omitted means no claim, and no claim means everywhere.**
    ///
    /// That default is the whole design. A module is already gated at RUNTIME by its window
    /// matchers: one without a `macos` block never matches there and is inert. So this field
    /// is not what makes a module correct — it is what lets the application say something
    /// useful *before* running it: warn before installing something that cannot work here,
    /// and not pay for loading it.
    ///
    /// It is therefore a claim, and claims rot. Sforzando was Windows-only one day and
    /// worked on both the next; a manifest still saying `["windows"]` would have excluded a
    /// module that had just started working, and the reason would have sat in a file nobody
    /// reads. So the three defences against a stale claim are: absent is not "unsupported";
    /// every exclusion names itself in the log at every start, with the override that loads
    /// it anyway; and the install review says "it can be installed, but it will not be loaded
    /// here" before anyone commits to it.
    ///
    /// One gap, deliberately left rather than papered over: an excluded module does not
    /// appear in the manager's Installed list, because that list is built from what was
    /// loaded. Someone who installs past the warning and then looks for it will not find it.
    /// Synthesising a row for something with no VM means a row whose Settings, Reload and
    /// Uninstall buttons all have to refuse, in the one window a blind user depends on, and
    /// that is not a change to make for a cosmetic gain. See TODO.md.
    #[serde(default)]
    pub supported_os: Vec<String>,
    #[serde(default)]
    pub capabilities: Capabilities,
    /// `[screen]`: which picture this module's screen and OCR reads see. Absent for every
    /// module written before it existed, and absent means the standard way — see
    /// [`ScreenDecl`].
    #[serde(default)]
    pub screen: ScreenDecl,
}

/// `[screen]` block: how the screen is read for this module's VM.
///
/// Kept as the strings the author wrote rather than parsed into an enum here, and on purpose.
/// A value this crate does not know must not fail the whole manifest — a module written for a
/// newer host has to keep loading on an older one, reading the standard way — and the host is
/// the one that can say, in the log and by name, which value it did not understand. So the
/// judgement lives in the host (`capture_source.rs`), and this is only the carrier.
///
/// ```toml
/// [screen]
/// capture = "duplication"   # "standard" (the default) or "duplication"
/// fallback = "none"         # with "duplication": "standard" (the default) or "none"
/// ```
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
pub struct ScreenDecl {
    #[serde(default)]
    pub capture: Option<String>,
    #[serde(default)]
    pub fallback: Option<String>,
}

impl ModuleManifest {
    /// Does this module claim to run on `os` (an `std::env::consts::OS` name)?
    ///
    /// True when it makes no claim at all — see the field. Comparison is
    /// case-insensitive, because a manifest is written by hand and "Windows" is what a
    /// person types.
    pub fn runs_on(&self, os: &str) -> bool {
        self.supported_os.is_empty()
            || self.supported_os.iter().any(|s| s.eq_ignore_ascii_case(os))
    }
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

    fn manifest_with(os: &[&str]) -> ModuleManifest {
        let list = os.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ");
        let text = format!(
            "id = \"com.x.y\"
name = \"X\"
version = \"1.0.0\"
supported_os = [{list}]
"
        );
        toml::from_str(&text).expect("manifest parses")
    }

    #[test]
    fn a_module_that_makes_no_claim_runs_everywhere() {
        // The load-bearing default. Every module written before this field existed says
        // nothing, and every one of them must go on loading exactly as it did.
        let m: ModuleManifest =
            toml::from_str("id = \"com.x.y\"
name = \"X\"
version = \"1.0.0\"
")
                .expect("manifest parses");
        assert!(m.supported_os.is_empty());
        for os in ["windows", "macos", "linux", "something-new"] {
            assert!(m.runs_on(os), "no claim must mean {os} too");
        }
    }

    #[test]
    fn a_claim_includes_what_it_names_and_excludes_the_rest() {
        let m = manifest_with(&["windows", "macos"]);
        assert!(m.runs_on("windows"));
        assert!(m.runs_on("macos"));
        assert!(!m.runs_on("linux"));
    }

    #[test]
    fn a_manifest_without_a_screen_table_declares_nothing() {
        // Every module written before `[screen]` existed: it must parse exactly as before and
        // leave the choice to the host's default.
        let m: ModuleManifest = toml::from_str(
            "id = \"com.x.y\"
name = \"X\"
version = \"1.0.0\"
",
        )
        .expect("manifest parses");
        assert_eq!(m.screen, ScreenDecl::default());
        assert!(m.screen.capture.is_none() && m.screen.fallback.is_none());
    }

    #[test]
    fn a_screen_table_is_carried_as_written() {
        let m: ModuleManifest = toml::from_str(
            "id = \"com.x.y\"
name = \"X\"
version = \"1.0.0\"

[capabilities]
require = [\"screen\"]

[screen]
capture = \"duplication\"
fallback = \"none\"
",
        )
        .expect("manifest parses");
        assert_eq!(m.screen.capture.as_deref(), Some("duplication"));
        assert_eq!(m.screen.fallback.as_deref(), Some("none"));
        assert_eq!(m.capabilities.require, vec!["screen".to_string()]);
    }

    #[test]
    fn an_unknown_screen_value_still_parses() {
        // Refusing the manifest would make a module written for a newer host fail to load on
        // this one; the host names the value it does not know and reads the standard way.
        let m: ModuleManifest = toml::from_str(
            "id = \"com.x.y\"
name = \"X\"
version = \"1.0.0\"

[screen]
capture = \"window\"
some_later_key = 3
",
        )
        .expect("an unknown value and an unknown key must not fail the manifest");
        assert_eq!(m.screen.capture.as_deref(), Some("window"));
        assert!(m.screen.fallback.is_none());
    }

    #[test]
    fn a_claim_is_read_the_way_a_person_writes_it() {
        // Hand-written file, so "Windows" and "macOS" are what will actually appear.
        let m = manifest_with(&["Windows", "macOS"]);
        assert!(m.runs_on("windows"));
        assert!(m.runs_on("macos"));
    }

    /// Every Luau file that ships in this repository still COMPILES.
    ///
    /// Nothing checked that. A typo in a module was found by starting the application and
    /// pressing the reload key, and the person doing that is blind — the error was surfaced
    /// safely and accessibly, which is not the same as its being an acceptable way to learn
    /// about a missing `end`. This is the cheap half of what `macos-check` does for Rust:
    /// front-end only, no host bindings, no window, no `host` table. It cannot say a module
    /// WORKS; it can say the parser will accept it, which is the failure that costs a session.
    ///
    /// Compiled, not run: a module's top level calls `host.*` immediately, so running it here
    /// would fail for reasons that say nothing about the source.
    #[test]
    fn every_shipped_luau_module_parses() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("repository root")
            .to_path_buf();
        let lua = mlua::Lua::new();
        let mut checked = 0usize;
        // The dev tools and the API demos too: they are loaded by run-dev.ps1 and by whoever
        // is learning the API from them, so a broken one wastes exactly the same session.
        for set in ["modules", "examples", "tools"] {
            let dir = root.join(set);
            if !dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&dir).expect("read module set") {
                let src = entry.expect("dir entry").path().join("src");
                if !src.is_dir() {
                    continue;
                }
                for f in std::fs::read_dir(&src).expect("read module src") {
                    let path = f.expect("dir entry").path();
                    if path.extension().and_then(|e| e.to_str()) != Some("luau") {
                        continue;
                    }
                    let code = std::fs::read_to_string(&path).expect("read luau");
                    // `set_name` so a failure names the file rather than `[string "..."]`.
                    let rel = path.strip_prefix(&root).unwrap_or(&path).display().to_string();
                    if let Err(e) = lua.load(&code).set_name(&rel).into_function() {
                        panic!("{rel} does not compile: {e}");
                    }
                    checked += 1;
                }
            }
        }
        // Or the test would pass by finding nothing — the failure mode of every check that
        // walks a directory.
        assert!(checked >= 10, "expected the shipped modules, found {checked} Luau file(s)");
    }
}
