//! Module package manifest (`module.toml`) + loading a module from an unpacked
//! directory (dev mode). See `docs/module-package-format.md`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Contents of `module.toml`.
///
/// `PartialEq` so that an install can hold the manifest it unpacked to the one that was
/// reviewed, field for field (`registry.rs`, `differs`).
#[derive(Debug, PartialEq, Eq, Deserialize)]
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
    /// Parses a `module.toml` and checks the fields that end up in file paths — see
    /// [`ModuleManifest::validate`]. Every reader of a manifest goes through here, so a module
    /// whose id could name a folder outside the one it is written into is refused before any
    /// path is built from it.
    pub fn parse(text: &str) -> Result<Self> {
        let manifest: ModuleManifest =
            toml::from_str(text).context("module.toml is not valid TOML")?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Refuses an `id`, `version` or `entry` that could lead a path somewhere else.
    ///
    /// These three are joined onto directories: the id and the version name the cache folder a
    /// `.zip` package is unpacked into (`<temp>/automation-platform-modules/<id>/<version>-…`),
    /// and the entry is joined onto the module's own folder to find the file that runs. A
    /// manifest comes off the internet, so `id = "../../x"` would otherwise be a folder
    /// somebody else chose.
    ///
    /// The id and the version are single names and are held to a short list of characters
    /// (ASCII letters, digits, `.`, `-`, `_`, and `+` in a version) rather than to a list of
    /// forbidden ones: that rules out separators, `..`, drive letters, absolute paths and
    /// control characters in one rule, and a name made of these characters means the same
    /// folder on every file system. The entry is a relative path with `/` between its parts,
    /// and each part is a plain name.
    ///
    /// The id of every `dependencies` and `optional_dependencies` entry is held to the id rule
    /// too. It names no folder, but it is looked for — among the installed modules, and by an
    /// install among every module published under the topic — and an id no module can have,
    /// an empty one above all, would read every candidate's manifest to find nothing.
    pub fn validate(&self) -> Result<()> {
        check_single_name("id", &self.id, false)?;
        check_single_name("version", &self.version, true)?;
        check_relative_path("entry", &self.entry)?;
        for (field, specs) in [
            ("dependencies", &self.dependencies),
            ("optional_dependencies", &self.optional_dependencies),
        ] {
            for spec in specs {
                if dep_id(spec).is_empty() {
                    anyhow::bail!("module.toml: `{field}` has an empty entry");
                }
                check_single_name(field, dep_id(spec), false)?;
            }
        }
        Ok(())
    }

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
#[derive(Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Capabilities {
    #[serde(default)]
    pub require: Vec<String>,
}

fn default_entry() -> String {
    "src/main.luau".to_string()
}

/// Device names Windows reserves in every folder: a file called `aux` or `con.txt` is not a
/// file there but a device. Refused on every platform, so that a module which installs on one
/// system installs on the other. The superscript `COM¹`–`LPT³` are devices on older Windows
/// releases and ordinary names on current Windows 11; refused for the older ones.
const RESERVED_NAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com0", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
    "com8", "com9", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8",
    "lpt9", "com\u{b9}", "com\u{b2}", "com\u{b3}", "lpt\u{b9}", "lpt\u{b2}", "lpt\u{b3}",
];

/// Whether `name` is one of Windows' device names, with or without an extension (`nul`,
/// `NUL.luau`).
fn is_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end();
    RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(stem))
}

/// Checks a manifest field that becomes ONE path component: the id or the version.
fn check_single_name(field: &str, value: &str, is_version: bool) -> Result<()> {
    let allowed = |c: char| {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') || (is_version && c == '+')
    };
    if value.is_empty() {
        anyhow::bail!("module.toml: `{field}` is empty");
    }
    if value.len() > 128 {
        anyhow::bail!("module.toml: `{field}` is longer than 128 characters");
    }
    if let Some(bad) = value.chars().find(|c| !allowed(*c)) {
        anyhow::bail!(
            "module.toml: `{field}` = {value:?} contains {bad:?}; it may only use ASCII letters, \
             digits, '.', '-', '_'{}",
            if is_version { " and '+'" } else { "" }
        );
    }
    // A leading dot hides the folder on macOS and is how staging folders are named; a
    // trailing one is dropped by Windows, so two ids would share a folder; and `..` anywhere
    // is refused rather than reasoned about.
    if value.starts_with('.') || value.ends_with('.') || value.contains("..") {
        anyhow::bail!(
            "module.toml: `{field}` = {value:?} may not start or end with '.' or contain '..'"
        );
    }
    if is_reserved_name(value) {
        anyhow::bail!("module.toml: `{field}` = {value:?} is a device name on Windows");
    }
    Ok(())
}

/// Checks a manifest field that is a path RELATIVE to the module folder: the entry.
///
/// Parts are separated by `/` only. A `\` or a `:` is refused as text on every platform —
/// on Windows they are a separator and a drive letter, and a module that means the same thing
/// everywhere cannot use them.
fn check_relative_path(field: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        anyhow::bail!("module.toml: `{field}` is empty");
    }
    if value.starts_with('/') {
        anyhow::bail!("module.toml: `{field}` = {value:?} is an absolute path");
    }
    for part in value.split('/') {
        if let Some(why) = bad_path_part(part) {
            anyhow::bail!("module.toml: `{field}` = {value:?}: {why}");
        }
    }
    Ok(())
}

/// Why `part` is not a plain file or folder name, or `None` when it is one.
///
/// Shared by the manifest check and by the archive unpacker, which holds every member of an
/// archive to the same rule.
///
/// The rule is what Windows can create, applied on every platform: a name it cannot create
/// would otherwise pass here and fail only while being written, and a module that installs on
/// one system has to install on the other. So beside `\` and `:`, the characters `<>"|?*` are
/// refused, and so is a name ending in `.` or a space — Windows drops those, which makes
/// `.. ` a way back to `..` and `module.toml.` a second spelling of `module.toml`.
fn bad_path_part(part: &str) -> Option<String> {
    if part.is_empty() {
        return Some("it has an empty part (two '/' in a row, or one at the end)".to_string());
    }
    if part == "." || part == ".." {
        return Some(format!("it contains a {part:?} part"));
    }
    if let Some(c) = part
        .chars()
        .find(|c| c.is_control() || matches!(c, '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Some(format!("{part:?} contains {c:?}"));
    }
    if part.ends_with('.') || part.ends_with(' ') {
        return Some(format!(
            "{part:?} ends in '.' or a space, which Windows drops from a name"
        ));
    }
    if is_reserved_name(part) {
        return Some(format!("{part:?} is a device name on Windows"));
    }
    None
}

/// How the files sit inside a module archive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveLayout {
    /// Everything inside ONE top-level folder, which is dropped: GitHub's source archives,
    /// which wrap a repository in `{owner}-{repo}-{sha}/`.
    OneFolder,
    /// The module's files at the top of the archive: a `.zip` package, with `module.toml`
    /// beside nothing but its own files.
    AtTop,
}

/// The most one archive may unpack to, all its members together.
///
/// A module is Luau, images and sounds, and the largest shipped one is a few megabytes; this
/// is two orders of magnitude above that. It exists because the SIZE a member declares is the
/// archive's word, and a small download can declare anything and inflate to fill the disk —
/// so it is enforced on the bytes actually written, not on the declared sizes.
pub const MAX_UNPACKED_BYTES: u64 = 256 * 1024 * 1024;

/// The most members one archive may have. The largest shipped module has 34 files; this is
/// for an archive of a million empty ones, which the byte budget does not stop — at about a
/// millisecond per file created, it held the unpacking thread for a quarter of an hour.
pub const MAX_MEMBERS: usize = 10_000;
/// The most parts one member's name may have, its archive's own folder included (the deepest
/// shipped module goes four folders down). A name nested thousands deep would otherwise go to
/// `create_dir_all`, which recurses once per folder.
pub const MAX_NAME_PARTS: usize = 32;
/// The longest a member's name may be, in bytes as the archive stores it.
pub const MAX_NAME_BYTES: usize = 400;

/// Unpacks a module archive into `dest`, replacing whatever was there.
///
/// Our own loop, not `ZipArchive::extract`, and on purpose: `extract` in zip 8.6 writes a
/// member named `top/C:\evil.txt` to `C:\evil.txt`, and so does a loop that trusts
/// `ZipFile::enclosed_name` alone, because a `C:` in the MIDDLE of a name is an ordinary
/// component to its parser and a drive to `PathBuf::push` on Windows. So a name that starts
/// with `/` or `\` or holds a `:` is refused as written, and each member's `enclosed_name` is
/// taken apart again and held to [`bad_path_part`]: every part a plain name that Windows can
/// create. A `\` inside a name separates folders, as `enclosed_name` reads it — some Windows
/// tools write names that way. With [`ArchiveLayout::OneFolder`] the first part must also be
/// the one folder every member shares, and something must follow it.
///
/// Also refused before anything is written: two members that are one file to a file system
/// that ignores case — Windows, and a Mac's default volume — such as `MODULE.TOML` beside
/// `module.toml`, or a file and a folder of one name; an encrypted member, or one compressed
/// with anything but deflate or stored, which this build cannot read; more than
/// [`MAX_MEMBERS`] members; and a name longer than [`MAX_NAME_BYTES`] or deeper than
/// [`MAX_NAME_PARTS`].
///
/// Every name is checked BEFORE `dest` is touched, so an archive refused by a rule leaves what
/// was in `dest` as it was. What cannot be checked in advance is the size — see
/// [`MAX_UNPACKED_BYTES`] — and the disk itself: an archive that turns out larger than the
/// budget, or a write that fails, stops the unpacking part-way, with `dest` removed rather
/// than left half-written. So a caller that must not lose what is in `dest` unpacks somewhere
/// new and swaps, as installing and updating do (`registry.rs`, `install_one`).
///
/// A symbolic link in the archive is written as a plain file holding the link's target: no
/// link is ever created, so nothing written later can be steered through one.
pub fn unpack_zip(bytes: &[u8], dest: &Path, layout: ArchiveLayout) -> Result<()> {
    unpack_zip_with_budget(bytes, dest, layout, MAX_UNPACKED_BYTES)
}

fn unpack_zip_with_budget(
    bytes: &[u8],
    dest: &Path,
    layout: ArchiveLayout,
    budget: u64,
) -> Result<()> {
    use std::collections::HashMap;
    use std::io::Read;

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("the file is not a zip archive")?;
    if archive.len() > MAX_MEMBERS {
        anyhow::bail!(
            "the archive has {} members; a module may have at most {MAX_MEMBERS}",
            archive.len()
        );
    }

    // Pass 1: names and declared sizes only — nothing is decompressed and nothing written.
    let mut plan: Vec<(usize, PathBuf, bool)> = Vec::with_capacity(archive.len());
    let mut root: Option<String> = None;
    let mut declared: u64 = 0;
    // Every path the archive names — each member and each folder above it — as the file
    // system will compare it, and whether it is a folder. Lower case, because Windows and a
    // default macOS volume ignore case. Not folded further: a Mac volume also treats the
    // composed and decomposed spellings of an accented letter as one name, and Windows' case
    // table differs from Unicode's in a handful of letters, so two such members can still
    // land on one file. What that can replace is the module's own file with another of its
    // own files; the one file whose replacement would matter, `module.toml`, is compared
    // with the reviewed one after unpacking.
    let mut names: HashMap<String, bool> = HashMap::with_capacity(archive.len());
    for i in 0..archive.len() {
        let file = archive.by_index_raw(i)?;
        let raw = file.name().to_string();
        if raw.len() > MAX_NAME_BYTES {
            anyhow::bail!(
                "archive member {:?}… is refused: its name is longer than {MAX_NAME_BYTES} bytes",
                raw.chars().take(60).collect::<String>()
            );
        }
        // An absolute name or a drive is refused as written, not quietly made relative, which
        // is what `enclosed_name` does with a leading `/` or `C:`: a member that asks for a
        // place outside the folder has no business in a module at all.
        if raw.starts_with('/') || raw.starts_with('\\') || raw.contains(':') {
            anyhow::bail!("archive member {raw:?} is refused: it names an absolute path or a drive");
        }
        if file.encrypted() {
            anyhow::bail!("archive member {raw:?} is encrypted, and a module archive may not be");
        }
        if !matches!(file.compression(), zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated) {
            anyhow::bail!(
                "archive member {raw:?} is compressed with {:?}; only deflate and stored can be read",
                file.compression()
            );
        }
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| anyhow::anyhow!("archive member {raw:?} leaves the folder it is unpacked into"))?;
        let mut parts: Vec<String> = Vec::new();
        for c in enclosed.components() {
            let std::path::Component::Normal(s) = c else {
                anyhow::bail!("archive member {raw:?} leaves the folder it is unpacked into");
            };
            let s = s
                .to_str()
                .ok_or_else(|| anyhow::anyhow!("archive member {raw:?} is not valid text"))?;
            if let Some(why) = bad_path_part(s) {
                anyhow::bail!("archive member {raw:?} is refused: {why}");
            }
            parts.push(s.to_string());
        }
        if parts.len() > MAX_NAME_PARTS {
            anyhow::bail!(
                "archive member {raw:?} is refused: it is nested more than {MAX_NAME_PARTS} folders deep"
            );
        }
        let rel: Vec<String> = match layout {
            ArchiveLayout::AtTop => parts,
            ArchiveLayout::OneFolder => {
                let Some((first, rest)) = parts.split_first() else {
                    continue; // the archive's own "./" — names nothing
                };
                match &root {
                    None => root = Some(first.clone()),
                    Some(r) if r == first => {}
                    Some(r) => anyhow::bail!(
                        "archive member {raw:?} is outside the archive's one folder {r:?}"
                    ),
                }
                if rest.is_empty() {
                    if file.is_dir() {
                        continue; // the wrapper folder itself
                    }
                    anyhow::bail!("archive member {raw:?} is a file beside the archive's folder");
                }
                rest.to_vec()
            }
        };
        if rel.is_empty() {
            continue;
        }
        let is_dir = file.is_dir();
        let clash = || {
            anyhow::anyhow!(
                "archive member {raw:?} is refused: another member has the same name, compared \
                 the way Windows and macOS compare names (without case)"
            )
        };
        for depth in 1..rel.len() {
            // A folder above this member: a folder may be named any number of times, a file
            // of the same name may not exist.
            if names.insert(rel[..depth].join("/").to_lowercase(), true) == Some(false) {
                return Err(clash());
            }
        }
        match names.insert(rel.join("/").to_lowercase(), is_dir) {
            None => {}
            Some(true) if is_dir => {} // the same folder, listed again
            Some(_) => return Err(clash()),
        }
        declared = declared.saturating_add(file.size());
        if declared > budget {
            anyhow::bail!("the archive unpacks to more than {} MB", budget / (1024 * 1024));
        }
        plan.push((i, rel.iter().collect(), file.is_dir()));
    }

    // Pass 2: write. Every name has passed, so the old folder can go.
    if dest.exists() {
        std::fs::remove_dir_all(dest)
            .with_context(|| format!("removing the old {}", dest.display()))?;
    }
    std::fs::create_dir_all(dest)?;
    let written = (|| -> Result<()> {
        let mut remaining = budget;
        for (i, rel, is_dir) in &plan {
            let out = dest.join(rel);
            if *is_dir {
                std::fs::create_dir_all(&out)?;
                continue;
            }
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut file = archive.by_index(*i)?;
            let mode = file.unix_mode();
            let mut target = std::fs::File::create(&out)
                .with_context(|| format!("writing {}", out.display()))?;
            // One byte past the budget, so "exactly the budget" and "more" can be told apart.
            let n = std::io::copy(&mut (&mut file).take(remaining + 1), &mut target)
                .with_context(|| format!("unpacking {}", rel.display()))?;
            if n > remaining {
                anyhow::bail!("the archive unpacks to more than {} MB", budget / (1024 * 1024));
            }
            remaining -= n;
            // The executable bit, which a plain write does not carry over. It costs nothing on
            // Windows and matters on macOS the moment a module ships anything under
            // native/macos-arm64/ — a payload that arrives without it fails to run for a reason
            // that has nothing to do with the module. Permission bits only: never set-user-id,
            // and never the file-type bits of a link. The owner can always read and write what
            // was unpacked, whatever the archive says: a module file its own user cannot read
            // fails to load for a reason nobody would look for in a permission bit.
            #[cfg(unix)]
            if let Some(mode) = mode {
                use std::os::unix::fs::PermissionsExt;
                drop(target);
                let _ = std::fs::set_permissions(
                    &out,
                    std::fs::Permissions::from_mode((mode & 0o777) | 0o600),
                );
            }
            #[cfg(not(unix))]
            let _ = mode;
        }
        Ok(())
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_dir_all(dest);
        return Err(e);
    }
    Ok(())
}

/// The content address of a package: SHA-256 of its bytes, first 16 bytes as hex.
///
/// A cryptographic hash rather than std's `DefaultHasher`, whose output is allowed to change
/// between Rust releases — and did not have to change for this to matter: a cache folder named
/// by it is only reused by the build that named it.
fn content_key(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes)
        .iter()
        .take(16)
        .map(|b| format!("{b:02x}"))
        .collect()
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
        let manifest = ModuleManifest::parse(&text)?;
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
        let key = content_key(&bytes);
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes))
            .with_context(|| format!("not a valid module package: {}", path.display()))?;

        // Read and checked before any path is built from it: the id and the version name the
        // cache folder below.
        let manifest = {
            let mut entry = archive
                .by_name("module.toml")
                .context("module.toml missing in package root")?;
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text)?;
            ModuleManifest::parse(&text)?
        };

        // Content-addressed cache: <temp>/automation-platform-modules/<id>/<version>-<key>/
        let by_id = std::env::temp_dir().join("automation-platform-modules").join(&manifest.id);
        let cache = by_id.join(format!("{}-{key}", manifest.version));
        if !cache.join("module.toml").exists() {
            // Unpacked beside the cache folder and renamed into place, so a run that stops
            // half-way never leaves a folder that the check above would take for complete.
            let partial =
                by_id.join(format!(".{}-{key}.partial-{}", manifest.version, std::process::id()));
            unpack_zip(&bytes, &partial, ArchiveLayout::AtTop)
                .with_context(|| format!("failed to extract module package {}", path.display()))?;
            if std::fs::rename(&partial, &cache).is_err() {
                // Another process finished the same package first; its copy is identical.
                let _ = std::fs::remove_dir_all(&partial);
                if !cache.join("module.toml").exists() {
                    anyhow::bail!("could not move the unpacked package to {}", cache.display());
                }
            }
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

    fn manifest_text(id: &str, version: &str, entry: Option<&str>) -> String {
        let mut t = format!("id = {id:?}\nname = \"X\"\nversion = {version:?}\n");
        if let Some(e) = entry {
            t.push_str(&format!("entry = {e:?}\n"));
        }
        t
    }

    #[test]
    fn ids_and_versions_that_could_name_another_folder_are_refused() {
        for id in [
            "../x", "..", "a/b", "a\\b", "C:x", "C:", "/abs", "com.x\u{7}", "", ".hidden", "x.",
            "a..b", "con", "NUL.module", "com1", "lpt9.x", "com.x y", "mödul",
        ] {
            let text = manifest_text(id, "1.0.0", None);
            assert!(ModuleManifest::parse(&text).is_err(), "id {id:?} must be refused");
        }
        for version in ["../1", "1/0", "1\\0", "C:1", "1.0.0\n", ".1", ""] {
            let text = manifest_text("com.x.y", version, None);
            assert!(ModuleManifest::parse(&text).is_err(), "version {version:?} must be refused");
        }
        // What the shipped modules and ordinary semver look like.
        for id in ["com.platform.kontakt", "com.tool.hotkey-test-a", "My_Module-2", "com.x"] {
            ModuleManifest::parse(&manifest_text(id, "1.0.0", None))
                .unwrap_or_else(|e| panic!("id {id:?} must be accepted: {e:#}"));
        }
        for version in ["0.1.0", "1.2.3-rc.1", "1.0.0+build.5"] {
            ModuleManifest::parse(&manifest_text("com.x.y", version, None))
                .unwrap_or_else(|e| panic!("version {version:?} must be accepted: {e:#}"));
        }
    }

    #[test]
    fn an_entry_must_stay_inside_the_module() {
        for entry in [
            "../x.luau", "src/../../x.luau", "/abs.luau", "src\\main.luau", "C:/x.luau",
            "C:x.luau", "src//x.luau", "src/./x.luau", "src/", "", "src/con.luau", "src/\u{1}.luau",
            "src/main.luau.", "src /main.luau", "src/what?.luau", "src/a*.luau",
        ] {
            let text = manifest_text("com.x.y", "1.0.0", Some(entry));
            assert!(ModuleManifest::parse(&text).is_err(), "entry {entry:?} must be refused");
        }
        for entry in ["src/main.luau", "main.luau", "src/sub dir/entry file.luau"] {
            ModuleManifest::parse(&manifest_text("com.x.y", "1.0.0", Some(entry)))
                .unwrap_or_else(|e| panic!("entry {entry:?} must be accepted: {e:#}"));
        }
        // The default, which is what nearly every module uses.
        let m = ModuleManifest::parse(&manifest_text("com.x.y", "1.0.0", None)).unwrap();
        assert_eq!(m.entry, "src/main.luau");
    }

    #[test]
    fn a_dependency_id_follows_the_id_rule() {
        let with = |key: &str, list: &str| format!("{}{key} = [{list}]\n", manifest_text("com.x.y", "1.0.0", None));
        for key in ["dependencies", "optional_dependencies"] {
            // An empty id would have been looked for among every module under the topic.
            for list in [r#""""#, r#""   ""#, r#""../x""#, r#""a/b >= 1""#, r#""com.x""#] {
                assert!(
                    ModuleManifest::parse(&with(key, list)).is_err(),
                    "{key} = [{list}] must be refused"
                );
            }
            for list in [r#""com.platform.overlay""#, r#""com.x.z >= 1.2", "com.q""#] {
                ModuleManifest::parse(&with(key, list))
                    .unwrap_or_else(|e| panic!("{key} = [{list}] must be accepted: {e:#}"));
            }
        }
    }

    #[test]
    fn two_manifests_compare_field_for_field() {
        let a = ModuleManifest::parse(&manifest_text("com.x.y", "1.0.0", None)).unwrap();
        let b = ModuleManifest::parse(&manifest_text("com.x.y", "1.0.0", None)).unwrap();
        assert_eq!(a, b);
        let renamed = ModuleManifest::parse(
            &manifest_text("com.x.y", "1.0.0", None).replace("name = \"X\"", "name = \"Y\""),
        )
        .unwrap();
        assert_ne!(a, renamed);
    }

    /// A zip built in memory from (name, contents) pairs; a name ending in `/` is a folder.
    fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        for (name, data) in members {
            if name.ends_with('/') {
                w.add_directory(*name, opts).unwrap();
            } else {
                w.start_file(*name, opts).unwrap();
                w.write_all(data).unwrap();
            }
        }
        w.finish().unwrap().into_inner()
    }

    const MANIFEST: &[u8] = b"id = \"com.x.y\"\nname = \"X\"\nversion = \"1.0.0\"\n";

    #[test]
    fn a_github_source_archive_unpacks_without_its_wrapper_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("mod");
        // Something already installed there: it is replaced, not merged with.
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("stale.luau"), b"old").unwrap();
        let bytes = zip_of(&[
            ("owner-repo-0123abc/", b""),
            ("owner-repo-0123abc/module.toml", MANIFEST),
            ("owner-repo-0123abc/src/", b""),
            ("owner-repo-0123abc/src/main.luau", b"return {}"),
            ("owner-repo-0123abc/images/a b.png", b"\x89PNG"),
            // As some Windows tools write a name: a backslash is a folder separator.
            ("owner-repo-0123abc\\lib\\util.luau", b"return 2"),
        ]);
        unpack_zip(&bytes, &dest, ArchiveLayout::OneFolder).expect("a plain archive unpacks");
        assert_eq!(std::fs::read(dest.join("module.toml")).unwrap(), MANIFEST);
        assert_eq!(std::fs::read(dest.join("src/main.luau")).unwrap(), b"return {}");
        assert!(dest.join("images/a b.png").is_file());
        assert_eq!(std::fs::read(dest.join("lib").join("util.luau")).unwrap(), b"return 2");
        assert!(!dest.join("stale.luau").exists(), "the old folder is replaced");
        assert!(!dest.join("owner-repo-0123abc").exists(), "the wrapper folder is dropped");
    }

    #[test]
    fn a_member_that_names_a_drive_is_refused_and_writes_nothing() {
        // The escape zip's own `extract` lets through: a drive in the MIDDLE of the name.
        // Aimed at a folder inside this test's temp directory rather than at C:\, so that a
        // regression writes somewhere harmless — and the test still sees it.
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let target = outside.join("evil1.txt");
        let dest = tmp.path().join("mod");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("keep.luau"), b"installed").unwrap();
        let mut names = vec![
            "top/C:\\evil1.txt".to_string(),
            "top/C:evil1.txt".to_string(),
            "top/sub/C:/x.txt".to_string(),
        ];
        // The temp directory itself, as a drive path — only on Windows is it one. Elsewhere
        // `top//var/folders/…/evil1.txt` is an ordinary relative name that unpacks, rightly,
        // inside the module folder.
        if cfg!(windows) {
            names.push(format!("top/{}", target.display()));
        }
        for name in &names {
            let name = name.as_str();
            let bytes = zip_of(&[("top/module.toml", MANIFEST), (name, b"pwned")]);
            let err = unpack_zip(&bytes, &dest, ArchiveLayout::OneFolder)
                .expect_err(&format!("{name:?} must be refused"));
            assert!(err.to_string().contains("refused") || err.to_string().contains("leaves"), "{err:#}");
            assert!(!target.exists(), "{name:?} wrote outside the module folder");
            // Refused while the names were being checked, before the old folder was touched.
            assert_eq!(std::fs::read(dest.join("keep.luau")).unwrap(), b"installed");
        }
    }

    #[test]
    fn parent_folders_absolute_names_and_strays_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("mod");
        let cases: &[&[(&str, &[u8])]] = &[
            // Out through `..`, past the root and back into a sibling.
            &[("top/module.toml", MANIFEST), ("top/../../evil.txt", b"x")],
            &[("top/module.toml", MANIFEST), ("top/../other/evil.txt", b"x")],
            // Absolute, in either spelling.
            &[("top/module.toml", MANIFEST), ("/etc/evil.txt", b"x")],
            &[("top/module.toml", MANIFEST), ("\\\\server\\share\\evil.txt", b"x")],
            // Two roots, and a file beside the one folder.
            &[("a/module.toml", MANIFEST), ("b/main.luau", b"x")],
            &[("top/module.toml", MANIFEST), ("README", b"x")],
            // A Windows device name.
            &[("top/module.toml", MANIFEST), ("top/aux.luau", b"x")],
        ];
        for members in cases {
            let bytes = zip_of(members);
            assert!(
                unpack_zip(&bytes, &dest, ArchiveLayout::OneFolder).is_err(),
                "{:?} must be refused",
                members.iter().map(|(n, _)| *n).collect::<Vec<_>>()
            );
            assert!(!tmp.path().join("evil.txt").exists());
            assert!(!tmp.path().join("other").exists());
        }
    }

    #[test]
    fn an_archive_larger_than_the_budget_is_refused_and_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("mod");
        let big = vec![b'a'; 64 * 1024]; // compresses to almost nothing, as a bomb does
        let bytes = zip_of(&[("top/module.toml", MANIFEST), ("top/big.bin", &big)]);
        let err = unpack_zip_with_budget(&bytes, &dest, ArchiveLayout::OneFolder, 32 * 1024)
            .expect_err("over budget");
        assert!(err.to_string().contains("unpacks to more than"), "{err:#}");
        assert!(!dest.join("big.bin").exists());
        // The same archive fits a budget that holds it exactly.
        let exact = (MANIFEST.len() + big.len()) as u64;
        unpack_zip_with_budget(&bytes, &dest, ArchiveLayout::OneFolder, exact).expect("fits");
        assert_eq!(std::fs::read(dest.join("big.bin")).unwrap().len(), big.len());
    }

    /// Overwrites `value` into the local header (at offset `local`) and the central-directory
    /// record (at offset `central`) of the member named `name`: for archives that lie, which
    /// `ZipWriter` will not write.
    fn patch_member(bytes: &mut [u8], name: &str, local: usize, central: usize, value: &[u8]) {
        let find = |bytes: &[u8], sig: &[u8], name_at: usize| -> usize {
            (0..bytes.len() - name_at - name.len())
                .find(|&i| {
                    &bytes[i..i + 4] == sig && &bytes[i + name_at..i + name_at + name.len()] == name.as_bytes()
                })
                .unwrap_or_else(|| panic!("no header for {name:?}"))
        };
        let l = find(bytes, b"PK\x03\x04", 30);
        bytes[l + local..l + local + value.len()].copy_from_slice(value);
        let c = find(bytes, b"PK\x01\x02", 46);
        bytes[c + central..c + central + value.len()].copy_from_slice(value);
    }

    #[test]
    fn a_member_that_declares_less_than_it_holds_is_stopped_while_writing() {
        // Its headers say one byte, so the check on declared sizes passes; zip 8.6 then
        // inflates all 64 KiB. The count of bytes actually written is what stops it — and the
        // half-written folder is removed, the file that was there before with it.
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("mod");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("keep.luau"), b"installed").unwrap();
        let big = vec![b'a'; 64 * 1024];
        let mut bytes = zip_of(&[("top/module.toml", MANIFEST), ("top/big.bin", &big)]);
        // The uncompressed size: offset 22 in the local header, 24 in the central directory.
        patch_member(&mut bytes, "top/big.bin", 22, 24, &1u32.to_le_bytes());
        let err = unpack_zip_with_budget(&bytes, &dest, ArchiveLayout::OneFolder, 32 * 1024)
            .expect_err("over budget while writing");
        assert!(err.to_string().contains("unpacks to more than"), "{err:#}");
        assert!(!dest.exists(), "the half-written folder is removed");
    }

    #[test]
    fn encrypted_and_unreadable_members_are_refused_before_anything_is_written() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("mod");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("keep.luau"), b"installed").unwrap();
        let plain = zip_of(&[("top/module.toml", MANIFEST), ("top/a.luau", b"return 1")]);
        // General-purpose flag bit 0 (encrypted): offset 6 local, 8 central.
        let mut encrypted = plain.clone();
        patch_member(&mut encrypted, "top/a.luau", 6, 8, &1u16.to_le_bytes());
        // Compression method 12, bzip2, which this build does not read: offset 8 local, 10
        // central.
        let mut bzip2 = plain.clone();
        patch_member(&mut bzip2, "top/a.luau", 8, 10, &12u16.to_le_bytes());
        for (what, bytes) in [("encrypted", encrypted), ("compressed with", bzip2)] {
            let err = unpack_zip(&bytes, &dest, ArchiveLayout::OneFolder).expect_err(what);
            assert!(err.to_string().contains(what), "{err:#}");
            assert_eq!(std::fs::read(dest.join("keep.luau")).unwrap(), b"installed");
        }
    }

    #[test]
    fn names_windows_would_change_or_merge_are_refused_before_anything_is_written() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("mod");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("keep.luau"), b"installed").unwrap();
        let cases: &[&[(&str, &[u8])]] = &[
            // A trailing dot or space is dropped by Windows: `.. ` is `..`, and `x.` is `x`.
            &[("top/module.toml", MANIFEST), ("top/.. /evil.txt", b"x")],
            &[("top/module.toml", MANIFEST), ("top/... /x", b"x")],
            &[("top/module.toml", MANIFEST), ("top/x/.. ", b"x")],
            &[("top/module.toml", MANIFEST), ("top/module.toml.", b"pwned")],
            // One file to a file system that ignores case.
            &[("top/module.toml", MANIFEST), ("top/MODULE.TOML", b"pwned")],
            &[("top/src/a.luau", b"1"), ("top/SRC/A.luau", b"2"), ("top/module.toml", MANIFEST)],
            // A file and a folder of one name, in either order.
            &[("top/module.toml", MANIFEST), ("top/a", b"x"), ("top/A/b.luau", b"x")],
            &[("top/module.toml", MANIFEST), ("top/a/b.luau", b"x"), ("top/a", b"x")],
            // Characters Windows cannot put in a name.
            &[("top/module.toml", MANIFEST), ("top/what?.png", b"x")],
            &[("top/module.toml", MANIFEST), ("top/a|b", b"x")],
            &[("top/module.toml", MANIFEST), ("top/COM\u{b9}.luau", b"x")],
        ];
        for members in cases {
            let names: Vec<&str> = members.iter().map(|(n, _)| *n).collect();
            let err = unpack_zip(&zip_of(members), &dest, ArchiveLayout::OneFolder)
                .expect_err(&format!("{names:?} must be refused"));
            assert!(err.to_string().contains("refused"), "{names:?}: {err:#}");
            assert_eq!(
                std::fs::read(dest.join("keep.luau")).unwrap(),
                b"installed",
                "{names:?} touched the folder before it was refused"
            );
        }
        // A folder named twice — here once in another case — and named again by the files in
        // it is one folder, and fine.
        let fine = zip_of(&[
            ("top/src/", b""),
            ("top/src/a.luau", b"1"),
            ("top/SRC/", b""),
            ("top/module.toml", MANIFEST),
        ]);
        unpack_zip(&fine, &dest, ArchiveLayout::OneFolder).expect("one folder, named twice");
        assert_eq!(std::fs::read(dest.join("src/a.luau")).unwrap(), b"1");
    }

    #[test]
    fn too_many_members_and_names_too_deep_or_long_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("mod");
        let many: Vec<String> = (0..=MAX_MEMBERS).map(|i| format!("top/f{i}")).collect();
        let members: Vec<(&str, &[u8])> = many.iter().map(|n| (n.as_str(), &b""[..])).collect();
        let err = unpack_zip(&zip_of(&members), &dest, ArchiveLayout::OneFolder)
            .expect_err("too many members");
        assert!(err.to_string().contains("at most"), "{err:#}");

        let deep = format!("top/{}f.luau", "d/".repeat(MAX_NAME_PARTS));
        let long = format!("top/{}.luau", "a".repeat(MAX_NAME_BYTES));
        for name in [deep.as_str(), long.as_str()] {
            let bytes = zip_of(&[("top/module.toml", MANIFEST), (name, b"x")]);
            let err = unpack_zip(&bytes, &dest, ArchiveLayout::OneFolder)
                .expect_err("too deep or too long");
            assert!(err.to_string().contains("refused"), "{err:#}");
        }
        assert!(!dest.exists(), "nothing was written");
        // At the limit is fine.
        let at_limit = format!("top/{}f.luau", "d/".repeat(MAX_NAME_PARTS - 2));
        unpack_zip(
            &zip_of(&[("top/module.toml", MANIFEST), (at_limit.as_str(), b"x")]),
            &dest,
            ArchiveLayout::OneFolder,
        )
        .expect("32 parts");
    }

    #[test]
    fn a_package_zip_loads_from_a_content_addressed_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("pkg.zip");
        // An id no other test or run uses, so the shared temp cache cannot answer for it.
        let id = format!("com.test.pkg-{}", std::process::id());
        let manifest = format!("id = {id:?}\nname = \"P\"\nversion = \"1.0.0\"\n");
        std::fs::write(
            &pkg,
            zip_of(&[("module.toml", manifest.as_bytes()), ("src/main.luau", b"return 1")]),
        )
        .unwrap();
        let first = LoadedModule::load(&pkg).expect("package loads");
        assert_eq!(first.manifest.id, id);
        assert!(first.entry_path().is_file());
        // The same bytes land in the same folder, and no half-written copy is left beside it.
        let again = LoadedModule::load(&pkg).expect("package loads again");
        assert_eq!(first.root, again.root);
        let by_id = first.root.parent().unwrap().to_path_buf();
        let leftovers: Vec<_> = std::fs::read_dir(&by_id)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().contains("partial"))
            .collect();
        assert!(leftovers.is_empty());
        let _ = std::fs::remove_dir_all(by_id);
    }

    #[test]
    fn a_package_whose_id_names_another_folder_is_refused_before_unpacking() {
        let tmp = tempfile::tempdir().unwrap();
        let pkg = tmp.path().join("evil.zip");
        let manifest = b"id = \"../../evil\"\nname = \"E\"\nversion = \"1.0.0\"\n";
        std::fs::write(&pkg, zip_of(&[("module.toml", manifest), ("src/main.luau", b"")])).unwrap();
        let err = LoadedModule::load(&pkg).expect_err("an id with '..' is refused");
        assert!(format!("{err:#}").contains("`id`"), "{err:#}");
    }

    #[test]
    fn every_shipped_manifest_passes_the_name_rules() {
        // The rules are new; a shipped module they refused would silently vanish from the
        // next start (a folder whose manifest fails is skipped without a message).
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("repository root")
            .to_path_buf();
        let mut checked = 0usize;
        for set in ["modules", "examples", "tools"] {
            let Ok(entries) = std::fs::read_dir(root.join(set)) else { continue };
            for entry in entries.flatten() {
                let toml = entry.path().join("module.toml");
                if let Ok(text) = std::fs::read_to_string(&toml) {
                    ModuleManifest::parse(&text)
                        .unwrap_or_else(|e| panic!("{}: {e:#}", toml.display()));
                    checked += 1;
                }
            }
        }
        assert!(checked >= 10, "expected the shipped modules, found {checked} manifest(s)");
    }

    #[test]
    fn the_content_key_is_sha256() {
        // FIPS 180-2's first example, so the key cannot drift with a toolchain or a crate.
        assert_eq!(content_key(b"abc"), "ba7816bf8f01cfea414140de5dae2223");
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
