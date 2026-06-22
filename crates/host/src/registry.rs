//! Module registry: discover, install and update modules published as GitHub
//! repositories tagged with [`MODULE_TOPIC`] — no central registry, devs just
//! tag their repo (HFS-style). Unauthenticated GitHub REST; installs the
//! default branch as a ZIP into a portable modules directory next to the exe.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use module_manifest::{LoadedModule, ModuleManifest};
use serde::Deserialize;

/// GitHub topic that module repositories tag themselves with. Central + a
/// working-title placeholder — change this one line to rebrand.
pub const MODULE_TOPIC: &str = "osap-module";

const USER_AGENT: &str = "automation-platform";

/// A module available to install (a GitHub repo tagged [`MODULE_TOPIC`]).
pub struct RemoteModule {
    pub full_name: String, // owner/repo
    pub description: String,
    pub stars: u64,
    pub default_branch: String,
    pub updated_at: String,
}

/// An installed module (in the portable modules directory).
pub struct InstalledModule {
    pub id: String,
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
    /// (repo, branch, commit sha) it was installed from, if installed remotely.
    pub source: Option<Source>,
}

#[derive(Clone)]
pub struct Source {
    pub repo: String,
    pub branch: String,
    pub sha: String,
}

/// The portable modules directory (next to the executable).
pub fn modules_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("modules")))
        .unwrap_or_else(|| PathBuf::from("modules"))
}

// --- HTTP (ureq 3.3; 4xx/5xx surface as Err(StatusCode), redirects automatic) --

fn map_err(url: &str) -> impl Fn(ureq::Error) -> anyhow::Error + '_ {
    move |e| match e {
        ureq::Error::StatusCode(code) => anyhow::anyhow!("GET {url}: HTTP {code}"),
        other => anyhow::anyhow!(other).context(format!("GET {url}")),
    }
}

fn get_string(url: &str, accept: &str) -> Result<String> {
    let mut resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", accept)
        .call()
        .map_err(map_err(url))?;
    resp.body_mut()
        .with_config()
        .limit(16 * 1024 * 1024)
        .read_to_string()
        .with_context(|| format!("reading {url}"))
}

fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let resp = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(map_err(url))?;
    let (_parts, body) = resp.into_parts();
    body.into_with_config()
        .limit(200 * 1024 * 1024)
        .read_to_vec()
        .with_context(|| format!("downloading {url}"))
}

// --- GitHub API ---------------------------------------------------------------

#[derive(Deserialize)]
struct SearchResponse {
    items: Vec<RepoJson>,
}

#[derive(Deserialize)]
struct RepoJson {
    full_name: String,
    description: Option<String>,
    stargazers_count: u64,
    default_branch: String,
    updated_at: String,
}

/// Searches GitHub for modules (repos tagged [`MODULE_TOPIC`]), optionally
/// narrowed by free-text `query`.
pub fn search(query: &str) -> Result<Vec<RemoteModule>> {
    let q = if query.trim().is_empty() {
        format!("topic:{MODULE_TOPIC}")
    } else {
        format!("topic:{MODULE_TOPIC} {}", query.trim())
    };
    let url = format!(
        "https://api.github.com/search/repositories?q={}&sort=stars",
        q.replace(' ', "+")
    );
    let json = get_string(&url, "application/vnd.github+json")?;
    let parsed: SearchResponse = serde_json::from_str(&json).context("parsing search response")?;
    Ok(parsed
        .items
        .into_iter()
        .map(|r| RemoteModule {
            full_name: r.full_name,
            description: r.description.unwrap_or_default(),
            stars: r.stargazers_count,
            default_branch: r.default_branch,
            updated_at: r.updated_at,
        })
        .collect())
}

/// The default branch of `owner/repo` (for installing by name).
pub fn default_branch(full_name: &str) -> Result<String> {
    #[derive(Deserialize)]
    struct R {
        default_branch: String,
    }
    let url = format!("https://api.github.com/repos/{full_name}");
    let json = get_string(&url, "application/vnd.github+json")?;
    Ok(serde_json::from_str::<R>(&json).context("parsing repo info")?.default_branch)
}

/// Fetches + parses a repo's `module.toml` without cloning — to review the
/// requested capabilities before installing.
pub fn fetch_manifest(full_name: &str, branch: &str) -> Result<ModuleManifest> {
    let url = format!("https://raw.githubusercontent.com/{full_name}/{branch}/module.toml");
    let text = get_string(&url, "text/plain")?;
    toml::from_str(&text).with_context(|| format!("module.toml of {full_name} is not valid TOML"))
}

/// Latest commit SHA of a branch (returned as plain text), for update checks.
pub fn latest_sha(full_name: &str, branch: &str) -> Result<String> {
    let url = format!("https://api.github.com/repos/{full_name}/commits/{branch}");
    Ok(get_string(&url, "application/vnd.github.sha")?.trim().to_string())
}

/// Installs (or reinstalls) `owner/repo` into the portable modules directory.
/// Returns the installed manifest. The caller is expected to have reviewed the
/// capabilities (via [`fetch_manifest`]) and confirmed.
pub fn install(full_name: &str) -> Result<ModuleManifest> {
    let branch = default_branch(full_name)?;
    let sha = latest_sha(full_name, &branch).unwrap_or_default();
    let bytes = get_bytes(&format!("https://api.github.com/repos/{full_name}/zipball/{branch}"))?;

    let repo = full_name.rsplit('/').next().unwrap_or(full_name);
    let dest = modules_dir().join(repo);
    extract_zip_stripped(&bytes, &dest)?;

    let source = format!("repo = \"{full_name}\"\nbranch = \"{branch}\"\nsha = \"{sha}\"\n");
    let _ = std::fs::write(dest.join(".source.toml"), source);

    Ok(LoadedModule::load_dir(&dest)
        .with_context(|| format!("{full_name} is not a valid module after install"))?
        .manifest)
}

/// Lists modules installed in the portable modules directory.
pub fn installed() -> Vec<InstalledModule> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(modules_dir()) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        if let Ok(m) = LoadedModule::load_dir(&dir) {
            out.push(InstalledModule {
                id: m.manifest.id,
                name: m.manifest.name,
                version: m.manifest.version,
                source: read_source(&dir),
                dir,
            });
        }
    }
    out
}

/// Removes an installed module by id (returns whether one was removed).
pub fn uninstall(id: &str) -> Result<bool> {
    for m in installed() {
        if m.id == id {
            std::fs::remove_dir_all(&m.dir)
                .with_context(|| format!("removing {}", m.dir.display()))?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// If a remotely-installed module has a newer commit upstream, returns the new
/// SHA; otherwise `None`.
pub fn update_available(m: &InstalledModule) -> Option<String> {
    let src = m.source.as_ref()?;
    let latest = latest_sha(&src.repo, &src.branch).ok()?;
    (!latest.is_empty() && latest != src.sha).then_some(latest)
}

fn read_source(dir: &Path) -> Option<Source> {
    #[derive(Deserialize)]
    struct S {
        repo: String,
        branch: String,
        sha: String,
    }
    let text = std::fs::read_to_string(dir.join(".source.toml")).ok()?;
    let s: S = toml::from_str(&text).ok()?;
    Some(Source {
        repo: s.repo,
        branch: s.branch,
        sha: s.sha,
    })
}

/// Extracts a GitHub source zip into `dest`, stripping the single top-level
/// folder GitHub wraps the archive in (`{owner}-{repo}-{sha}/`).
fn extract_zip_stripped(bytes: &[u8], dest: &Path) -> Result<()> {
    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes)).context("downloaded file is not a zip")?;
    if dest.exists() {
        let _ = std::fs::remove_dir_all(dest);
    }
    std::fs::create_dir_all(dest)?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        // Strip the leading "<top>/" component.
        let rel = match name.split_once('/') {
            Some((_top, rest)) => rest,
            None => continue,
        };
        if rel.is_empty() {
            continue;
        }
        let out = dest.join(rel);
        if file.is_dir() {
            std::fs::create_dir_all(&out)?;
        } else {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p)?;
            }
            let mut buf = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut buf)?;
            std::fs::write(&out, &buf)?;
        }
    }
    Ok(())
}
