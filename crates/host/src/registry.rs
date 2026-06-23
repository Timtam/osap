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
    /// Ids this module declares as dependencies (for the install/uninstall graph).
    pub dependencies: Vec<String>,
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

/// Installs `full_name` and, recursively, every dependency it declares that isn't
/// already installed — resolving each dependency id to a repo via the module-topic
/// search. Returns every newly-installed manifest (the caller hot-loads them). The
/// capability review is the caller's responsibility (as with [`install`]).
pub fn install_tree(full_name: &str) -> Result<Vec<ModuleManifest>> {
    use std::collections::{HashMap, HashSet};
    let mut have: HashSet<String> = installed().into_iter().map(|m| m.id).collect();
    let mut installed_now: Vec<ModuleManifest> = Vec::new();
    let mut queue: Vec<String> = vec![full_name.to_string()];
    let mut seen_repo: HashSet<String> = HashSet::new();
    // id -> repo, built once from the topic, only if an unresolved dependency appears.
    let mut index: Option<HashMap<String, String>> = None;
    while let Some(repo) = queue.pop() {
        if !seen_repo.insert(repo.clone()) {
            continue;
        }
        let m = install(&repo)?;
        let (id, deps) = (
            m.id.clone(),
            m.dependencies
                .iter()
                .map(|s| module_manifest::dep_id(s).to_string())
                .collect::<Vec<String>>(),
        );
        have.insert(id.clone());
        installed_now.push(m);
        let missing: Vec<String> = deps.into_iter().filter(|d| !have.contains(d)).collect();
        if missing.is_empty() {
            continue;
        }
        if index.is_none() {
            index = Some(topic_index()?);
        }
        let idx = index.as_ref().unwrap();
        for dep in missing {
            match idx.get(&dep) {
                Some(dep_repo) => queue.push(dep_repo.clone()),
                None => anyhow::bail!(
                    "dependency '{dep}' of '{id}' was not found among '{MODULE_TOPIC}' modules"
                ),
            }
        }
    }
    Ok(installed_now)
}

/// Maps every module id published under the topic to its repo (owner/repo), by
/// fetching each candidate's `module.toml`. Used to resolve dependency ids; the
/// first (highest-starred) repo claiming an id wins.
fn topic_index() -> Result<std::collections::HashMap<String, String>> {
    let mut map = std::collections::HashMap::new();
    for c in search("")? {
        if let Ok(m) = fetch_manifest(&c.full_name, &c.default_branch) {
            map.entry(m.id).or_insert(c.full_name);
        }
    }
    Ok(map)
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
                dependencies: m
                    .manifest
                    .dependencies
                    .iter()
                    .map(|s| module_manifest::dep_id(s).to_string())
                    .collect(),
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

/// If a remotely-installed module has a newer **version** upstream (its
/// `module.toml` `version` parses as semver and is greater than the installed
/// one), returns that new version; otherwise `None`. Commits *between* releases no
/// longer trigger an update — only a version bump does. Falls back to the legacy
/// "any newer commit" signal when either version isn't valid semver.
pub fn update_available(m: &InstalledModule) -> Option<String> {
    let src = m.source.as_ref()?;
    let upstream = fetch_manifest(&src.repo, &src.branch).ok()?;
    match (semver::Version::parse(&upstream.version), semver::Version::parse(&m.version)) {
        (Ok(up), Ok(cur)) => (up > cur).then_some(upstream.version),
        _ => {
            let latest = latest_sha(&src.repo, &src.branch).ok()?;
            (!latest.is_empty() && latest != src.sha).then_some(latest)
        }
    }
}

// --- Dependency graph (install/uninstall) -------------------------------------
// `graph` is a slice of (module id, its declared dependency ids) — build it from
// `installed()` (id + dependencies). Pure functions, so they're unit-tested.

/// Module ids that **transitively** depend on `id` — removing `id` would break
/// them, so they must be removed first (or block the uninstall).
pub fn transitive_dependents(id: &str, graph: &[(String, Vec<String>)]) -> Vec<String> {
    use std::collections::HashSet;
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = vec![id.to_string()];
    while let Some(cur) = stack.pop() {
        for (mid, deps) in graph {
            if deps.iter().any(|d| d == &cur) && seen.insert(mid.clone()) {
                out.push(mid.clone());
                stack.push(mid.clone());
            }
        }
    }
    out
}

/// Installed dependency modules left **orphaned** — needed by nothing that
/// survives — when every id in `removing` is uninstalled. Cascades: an orphan's
/// own now-unneeded dependencies are orphaned too. Use to offer cleanup after a
/// removal.
pub fn orphaned_by(removing: &[String], graph: &[(String, Vec<String>)]) -> Vec<String> {
    use std::collections::HashSet;
    let installed: HashSet<&str> = graph.iter().map(|(m, _)| m.as_str()).collect();
    let mut gone: HashSet<String> = removing.iter().cloned().collect();
    let mut orphans: Vec<String> = Vec::new();
    loop {
        let mut next: Option<String> = None;
        'scan: for (mid, deps) in graph {
            if !gone.contains(mid) {
                continue; // only the deps that a removed module pulled in
            }
            for d in deps {
                if !installed.contains(d.as_str()) || gone.contains(d) {
                    continue;
                }
                let still_needed = graph
                    .iter()
                    .any(|(m, ds)| !gone.contains(m) && ds.iter().any(|x| x == d));
                if !still_needed {
                    next = Some(d.clone());
                    break 'scan;
                }
            }
        }
        match next {
            Some(d) => {
                gone.insert(d.clone());
                orphans.push(d);
            }
            None => break,
        }
    }
    orphans
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    fn g(pairs: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        pairs
            .iter()
            .map(|(id, deps)| (id.to_string(), deps.iter().map(|s| s.to_string()).collect()))
            .collect()
    }
    // css -> kontakt -> {overlay, daw}; sforzando -> {overlay, daw}.
    fn sample() -> Vec<(String, Vec<String>)> {
        g(&[
            ("css", &["kontakt"]),
            ("kontakt", &["overlay", "daw"]),
            ("sforzando", &["overlay", "daw"]),
            ("overlay", &[]),
            ("daw", &[]),
        ])
    }
    #[test]
    fn dependents_walk_the_whole_chain() {
        let graph = sample();
        let mut d = transitive_dependents("overlay", &graph);
        d.sort();
        assert_eq!(d, ["css", "kontakt", "sforzando"]);
        let mut k = transitive_dependents("kontakt", &graph);
        k.sort();
        assert_eq!(k, ["css"]);
        assert!(transitive_dependents("css", &graph).is_empty());
    }
    #[test]
    fn orphans_cascade_but_spare_still_needed() {
        let graph = sample();
        // Removing css orphans only kontakt — overlay+daw are still needed by sforzando.
        let mut o = orphaned_by(&["css".into()], &graph);
        o.sort();
        assert_eq!(o, ["kontakt"]);
        // Removing css AND sforzando cascades: kontakt, then overlay, then daw.
        let mut o2 = orphaned_by(&["css".into(), "sforzando".into()], &graph);
        o2.sort();
        assert_eq!(o2, ["daw", "kontakt", "overlay"]);
    }
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
