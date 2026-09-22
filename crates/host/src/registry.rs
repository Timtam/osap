//! Module registry: discover, install and update modules published as GitHub
//! repositories tagged with [`MODULE_TOPIC`] — no central registry, devs just
//! tag their repo (HFS-style). Unauthenticated GitHub REST; installs a pinned
//! commit as a ZIP into a portable modules directory next to the exe.
//!
//! Installing is two steps with a person in between. [`resolve_tree`] works out every module
//! an install would ADD — the one asked for and each dependency not installed yet — pinned to
//! the commit whose `module.toml` it read, and writes nothing. The caller shows that list with
//! each module's capabilities; [`install_resolved`] then installs exactly those commits. An
//! update goes the same way through [`resolve_update`], which also says what the new version
//! asks for that the installed one did not.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context, Result};
use module_manifest::{ArchiveLayout, LoadedModule, ModuleManifest};
use serde::{Deserialize, Serialize};

/// GitHub topic that module repositories tag themselves with. Central + a
/// working-title placeholder — change this one line to rebrand.
pub const MODULE_TOPIC: &str = "osap-module";

const USER_AGENT: &str = "automation-platform";
const API: &str = "https://api.github.com";
const RAW: &str = "https://raw.githubusercontent.com";

/// How long to wait for a name lookup, a connection (with its TLS handshake) and for a request
/// to go out. Without these a stalled connection held the install — and the manager's buttons
/// with it — for as long as the operating system cared to keep the socket.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long to wait for GitHub to start answering.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);
/// How long a whole API answer or `module.toml` may take to arrive. ureq has no idle timeout,
/// only one for the whole body, so this bounds the total.
const BODY_TIMEOUT: Duration = Duration::from_secs(60);
/// The same for a module's ZIP, which can be megabytes on a slow line.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// What is percent-encoded when a branch goes into a URL path: the control characters, the
/// space, and each ASCII character a URL path does not carry as itself (the list below).
/// Everything outside ASCII is encoded as well, by `utf8_percent_encode` itself. `/` is not in
/// the set.
const REF_IN_PATH: &percent_encoding::AsciiSet = &percent_encoding::CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'\\')
    .add(b'^')
    .add(b'`')
    .add(b'{')
    .add(b'|')
    .add(b'}')
    .add(b'[')
    .add(b']');

/// Results per search page: GitHub's maximum.
const PER_PAGE: usize = 100;
/// Pages followed per search. GitHub's search returns at most 1000 results however many pages
/// are asked for, and the unauthenticated search limit is ten requests a minute, so ten pages
/// is both the end of what exists and the most one search can afford.
const MAX_PAGES: usize = 10;

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
    /// Ids it can use when they are present — what an update is compared against.
    pub optional_dependencies: Vec<String>,
    /// What it declares under `[capabilities] require` — what an update is compared against.
    pub capabilities: Vec<String>,
    /// (repo, branch, commit sha) it was installed from, if installed remotely.
    pub source: Option<Source>,
    /// What the manifest claims about operating systems, verbatim and possibly empty.
    ///
    /// Carried so the manager can SHOW a module it did not load, with the reason. A module
    /// that silently disappears from the list is a support question; one that is listed as
    /// "declares windows, this is macos" answers itself.
    pub supported_os: Vec<String>,
}

/// Where a module was installed from, as `.source.toml` in its folder records it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub repo: String,
    pub branch: String,
    pub sha: String,
}

/// The search for the module topic, narrowed by free text.
fn topic_query(query: &str) -> String {
    if query.trim().is_empty() {
        format!("topic:{MODULE_TOPIC}")
    } else {
        format!("topic:{MODULE_TOPIC} {}", query.trim())
    }
}

/// The portable modules directory, in the application's own folder.
pub fn modules_dir() -> PathBuf {
    crate::portable::base_dir().join("modules")
}

/// The folder a repository installs into: `modules/<repository name>/`.
pub fn install_dir(full_name: &str) -> PathBuf {
    modules_dir().join(repo_name(full_name))
}

fn repo_name(full_name: &str) -> &str {
    full_name.rsplit('/').next().unwrap_or(full_name)
}

/// Refuses anything but `owner/repo` in GitHub's own spelling.
///
/// The repository name becomes the install folder and both halves go into URL paths, so a
/// name is held to what GitHub itself allows — letters, digits, `-` for an owner, and `.` and
/// `_` as well for a repository — before either happens. `owner/..` would otherwise have
/// installed over the modules folder's parent.
fn check_repo(full_name: &str) -> Result<()> {
    let ok = full_name.split_once('/').is_some_and(|(owner, repo)| {
        let owner_ok = !owner.is_empty()
            && owner.len() <= 39
            && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
        let repo_ok = !repo.is_empty()
            && repo.len() <= 100
            && !repo.starts_with('.')
            && repo.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        owner_ok && repo_ok
    });
    if ok {
        Ok(())
    } else {
        anyhow::bail!("\u{201c}{full_name}\u{201d} is not a GitHub repository name (owner/repo)")
    }
}

/// A commit id as GitHub prints it: forty hexadecimal digits. Checked because it goes into
/// the URLs a module is downloaded from.
fn check_sha(sha: &str) -> Result<()> {
    if sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        anyhow::bail!("GitHub answered {sha:?} where a commit id was expected")
    }
}

// --- HTTP (ureq 3.3; 4xx/5xx surface as Err(StatusCode), redirects automatic) --

/// The agent every request of the live application goes through: one, so connections are
/// reused across the requests a search or an install makes, and with the timeouts above.
fn live_agent() -> ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT
        .get_or_init(|| {
            ureq::Agent::config_builder()
                .user_agent(USER_AGENT)
                .timeout_resolve(Some(CONNECT_TIMEOUT))
                .timeout_connect(Some(CONNECT_TIMEOUT))
                .timeout_send_request(Some(CONNECT_TIMEOUT))
                .timeout_recv_response(Some(RESPONSE_TIMEOUT))
                .timeout_recv_body(Some(BODY_TIMEOUT))
                .build()
                .into()
        })
        .clone()
}

/// GitHub, reached through one agent. The live one is [`GitHub::live`]; the tests hand in an
/// agent whose middleware answers from a table, so every URL this module builds can be checked
/// without a network.
struct GitHub {
    agent: ureq::Agent,
}

type Get = ureq::RequestBuilder<ureq::typestate::WithoutBody>;

fn describe_err(what: &str, e: ureq::Error) -> anyhow::Error {
    match e {
        ureq::Error::StatusCode(code) => anyhow::anyhow!("{what}: HTTP {code}"),
        other => anyhow::anyhow!(other).context(what.to_string()),
    }
}

impl GitHub {
    fn live() -> Self {
        GitHub { agent: live_agent() }
    }

    fn get(&self, url: &str, accept: &str) -> Get {
        self.agent.get(url).header("Accept", accept)
    }

    fn read_string(req: Get, what: &str) -> Result<String> {
        let mut resp = req.call().map_err(|e| describe_err(what, e))?;
        resp.body_mut()
            .with_config()
            .limit(16 * 1024 * 1024)
            .read_to_string()
            .with_context(|| format!("reading the answer to {what}"))
    }

    /// One page (1-based) of the search for `q`, most stars first, and whether another page
    /// may follow it.
    fn search_page(&self, q: &str, page: usize) -> Result<(Vec<RemoteModule>, bool)> {
        #[derive(Deserialize)]
        struct SearchResponse {
            total_count: u64,
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
        // `.query` percent-encodes, so "c++" is searched for as written rather than as
        // "c  ", and "a & b" does not end the parameter at the ampersand.
        let req = self
            .get(&format!("{API}/search/repositories"), "application/vnd.github+json")
            .query("q", q)
            .query("sort", "stars")
            .query("order", "desc")
            .query("per_page", PER_PAGE.to_string())
            .query("page", page.to_string());
        let json = Self::read_string(req, "the GitHub search")?;
        let parsed: SearchResponse =
            serde_json::from_str(&json).context("parsing the GitHub search answer")?;
        let more = parsed.items.len() == PER_PAGE
            && ((page * PER_PAGE) as u64) < parsed.total_count
            && page < MAX_PAGES;
        let items = parsed
            .items
            .into_iter()
            .map(|r| RemoteModule {
                full_name: r.full_name,
                description: r.description.unwrap_or_default(),
                stars: r.stargazers_count,
                default_branch: r.default_branch,
                updated_at: r.updated_at,
            })
            .collect();
        Ok((items, more))
    }

    /// Searches for modules (repos tagged [`MODULE_TOPIC`]), narrowed by free text `query`,
    /// following pages until GitHub has no more or [`MAX_PAGES`] is reached.
    fn search(&self, query: &str) -> Result<Vec<RemoteModule>> {
        let q = topic_query(query);
        let mut out: Vec<RemoteModule> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for page in 1..=MAX_PAGES {
            let (items, more) = self.search_page(&q, page)?;
            // A repository can move between pages while they are fetched, as stars change;
            // it is listed once.
            out.extend(items.into_iter().filter(|r| seen.insert(r.full_name.clone())));
            if !more {
                break;
            }
        }
        Ok(out)
    }

    fn default_branch(&self, full_name: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct R {
            default_branch: String,
        }
        check_repo(full_name)?;
        let req = self.get(&format!("{API}/repos/{full_name}"), "application/vnd.github+json");
        let json = Self::read_string(req, &format!("looking up {full_name}"))?;
        Ok(serde_json::from_str::<R>(&json).context("parsing repo info")?.default_branch)
    }

    /// The commit a branch points at now. The branch goes in as a query parameter, so a name
    /// with `#` or `%` in it is encoded rather than cutting the URL short.
    fn head_sha(&self, full_name: &str, branch: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct Commit {
            sha: String,
        }
        check_repo(full_name)?;
        let req = self
            .get(&format!("{API}/repos/{full_name}/commits"), "application/vnd.github+json")
            .query("sha", branch)
            .query("per_page", "1");
        let json = Self::read_string(req, &format!("the latest commit of {full_name}"))?;
        let commits: Vec<Commit> =
            serde_json::from_str(&json).context("parsing the commit list")?;
        let sha = commits
            .into_iter()
            .next()
            .map(|c| c.sha)
            .ok_or_else(|| anyhow::anyhow!("{full_name} has no commits on {branch:?}"))?;
        check_sha(&sha)?;
        Ok(sha)
    }

    /// A repository's `module.toml` at `git_ref` — a commit id, or a branch — read without
    /// downloading the repository, and checked like every other manifest.
    fn fetch_manifest(&self, full_name: &str, git_ref: &str) -> Result<ModuleManifest> {
        check_repo(full_name)?;
        // A branch is part of the PATH here, so it is percent-encoded — `#`, `?`, `%` and
        // anything else that would end the path, start a query or read as an escape — with
        // `/` kept, because a branch like `feature/x` is read from the path as it stands. It
        // used to be refused instead, and a module installed from a branch named `fix#12`
        // never saw an update. `..` stays refused: git allows it in no branch name, and in a
        // path it means something else. A commit id is forty hex digits and passes unchanged.
        if git_ref.is_empty() || git_ref.contains("..") {
            anyhow::bail!("{full_name}: the branch {git_ref:?} cannot be read over raw.githubusercontent.com");
        }
        let encoded = percent_encoding::utf8_percent_encode(git_ref, REF_IN_PATH);
        let req = self.get(&format!("{RAW}/{full_name}/{encoded}/module.toml"), "text/plain");
        let text = Self::read_string(req, &format!("module.toml of {full_name}"))?;
        ModuleManifest::parse(&text).with_context(|| format!("module.toml of {full_name}"))
    }

    /// The repository at one commit, as GitHub zips it.
    fn download(&self, full_name: &str, sha: &str) -> Result<Vec<u8>> {
        check_repo(full_name)?;
        check_sha(sha)?;
        let what = format!("downloading {full_name}");
        // No JSON Accept header: this answers with a redirect to codeload.github.com and then
        // the archive itself, as it did before commits were pinned.
        let resp = self
            .agent
            .get(&format!("{API}/repos/{full_name}/zipball/{sha}"))
            .config()
            .timeout_recv_body(Some(DOWNLOAD_TIMEOUT))
            .build()
            .call()
            .map_err(|e| describe_err(&what, e))?;
        let (_parts, body) = resp.into_parts();
        body.into_with_config()
            .limit(200 * 1024 * 1024)
            .read_to_vec()
            .with_context(|| what.clone())
    }

    /// Reads a repository's manifest at the commit its branch points at now, and remembers
    /// that commit, so what is installed later is the commit that was reviewed.
    fn pin(
        &self,
        full_name: &str,
        branch: &str,
        optional: bool,
        needed_by: Option<String>,
    ) -> Result<PlannedModule> {
        let sha = self.head_sha(full_name, branch)?;
        let manifest = self.fetch_manifest(full_name, &sha)?;
        Ok(PlannedModule {
            repo: full_name.to_string(),
            branch: branch.to_string(),
            sha,
            manifest,
            optional,
            needed_by,
            via_optional: false,
        })
    }
}

// --- Public requests ------------------------------------------------------------

/// Searches GitHub for modules (repos tagged [`MODULE_TOPIC`]), optionally
/// narrowed by free-text `query`. Follows the result pages, up to GitHub's own
/// ceiling of 1000 results.
pub fn search(query: &str) -> Result<Vec<RemoteModule>> {
    GitHub::live().search(query)
}

/// The default branch of `owner/repo` (for installing by name).
pub fn default_branch(full_name: &str) -> Result<String> {
    GitHub::live().default_branch(full_name)
}

/// Fetches + parses a repo's `module.toml` at a branch or commit, without
/// downloading the repository.
pub fn fetch_manifest(full_name: &str, git_ref: &str) -> Result<ModuleManifest> {
    GitHub::live().fetch_manifest(full_name, git_ref)
}

/// Latest commit SHA of a branch, for update checks.
pub fn latest_sha(full_name: &str, branch: &str) -> Result<String> {
    GitHub::live().head_sha(full_name, branch)
}

// --- Install and update plans ---------------------------------------------------

/// One module an install would write, pinned to the commit whose manifest was read.
#[derive(Debug)]
pub struct PlannedModule {
    pub repo: String,
    pub branch: String,
    pub sha: String,
    pub manifest: ModuleManifest,
    /// Here only for an OPTIONAL dependency of the module asked for: installed only when the
    /// user accepts the optional modules.
    pub optional: bool,
    /// The name of the module that brought this one in, or `None` for the module asked for.
    pub needed_by: Option<String>,
    /// Whether `needed_by` declares this one as an OPTIONAL dependency — true only for the
    /// requested module's optional dependencies themselves, not for what they need.
    pub via_optional: bool,
}

/// Every module an install would ADD, and nothing else. Built without writing anything.
#[derive(Debug)]
pub struct InstallPlan {
    /// The module asked for first, then every dependency that is not installed yet — each
    /// once, in the order they were found.
    pub modules: Vec<PlannedModule>,
    /// Optional dependencies of the module asked for that cannot be installed, each with
    /// the reason. They are only reported: an optional extra never fails an install.
    pub unavailable_optional: Vec<(String, String)>,
}

impl InstallPlan {
    /// Whether any module in the plan is there only for an optional dependency.
    pub fn has_optional(&self) -> bool {
        self.modules.iter().any(|m| m.optional)
    }
}

/// What an update would change that the user has not agreed to yet.
#[derive(Debug)]
pub struct UpdatePlan {
    pub id: String,
    pub name: String,
    pub from_version: String,
    /// The new version first, then every module it now needs that is not installed — the
    /// same shape as an install, and installed the same way.
    pub install: InstallPlan,
    /// Capabilities the new version asks for that the installed one did not.
    pub added_capabilities: Vec<String>,
    /// Dependencies the new version declares that the installed one did not and that are
    /// ALREADY installed, as (id, optional). Such a module is not downloaded, but its code
    /// now runs for this one, so it is shown too.
    pub added_installed_dependencies: Vec<(String, bool)>,
}

impl UpdatePlan {
    /// Whether the update asks for anything new — a capability, or a module it did not use
    /// before — and must therefore be shown before it is applied.
    pub fn needs_review(&self) -> bool {
        !self.added_capabilities.is_empty()
            || self.install.modules.len() > 1
            || !self.added_installed_dependencies.is_empty()
    }
}

/// Resolves module ids to repositories through the topic search, lazily.
///
/// Nothing is searched until the first unknown dependency appears. Candidates' manifests are
/// then read in order of stars, and a further page of the search is fetched only when the
/// ones already fetched are used up — so the first (highest-starred) repository claiming an
/// id wins, as before, without reading every manifest under the topic, or every page of the
/// search, for a tree that needs one dependency. That matters beyond speed: GitHub allows an
/// unauthenticated client ten searches a minute.
struct TopicIndex {
    candidates: Vec<RemoteModule>,
    seen: HashSet<String>,
    /// The next candidate whose manifest has not been read.
    next: usize,
    /// Search pages fetched so far, and whether GitHub may have another.
    pages: usize,
    more: bool,
    found: HashMap<String, (String, String)>,
}

impl TopicIndex {
    fn new() -> Self {
        TopicIndex {
            candidates: Vec::new(),
            seen: HashSet::new(),
            next: 0,
            pages: 0,
            more: true,
            found: HashMap::new(),
        }
    }

    /// (repo, branch) of the module with `id`, or `None` when no module under the topic has it.
    fn find(&mut self, gh: &GitHub, id: &str) -> Result<Option<(String, String)>> {
        if let Some(hit) = self.found.get(id) {
            return Ok(Some(hit.clone()));
        }
        loop {
            while self.next < self.candidates.len() {
                let c = &self.candidates[self.next];
                self.next += 1;
                // A candidate whose manifest cannot be read is not a candidate; the dependency
                // may still be found further down.
                if let Ok(m) = gh.fetch_manifest(&c.full_name, &c.default_branch) {
                    let hit = (c.full_name.clone(), c.default_branch.clone());
                    self.found.entry(m.id.clone()).or_insert_with(|| hit.clone());
                    if m.id == id {
                        return Ok(Some(hit));
                    }
                }
            }
            if !self.more {
                return Ok(None);
            }
            self.pages += 1;
            let (items, more) = gh.search_page(&topic_query(""), self.pages)?;
            self.more = more;
            let seen = &mut self.seen;
            self.candidates.extend(items.into_iter().filter(|r| seen.insert(r.full_name.clone())));
        }
    }
}

impl GitHub {
    /// Adds to `modules` every REQUIRED dependency of `modules[from..]` that is not in
    /// `known`, transitively, marking each `optional` as given.
    fn add_required(
        &self,
        modules: &mut Vec<PlannedModule>,
        from: usize,
        known: &mut HashSet<String>,
        index: &mut TopicIndex,
        optional: bool,
    ) -> Result<()> {
        let mut i = from;
        while i < modules.len() {
            let (id, name) = (modules[i].manifest.id.clone(), modules[i].manifest.name.clone());
            let deps: Vec<String> = modules[i]
                .manifest
                .dependencies
                .iter()
                .map(|s| module_manifest::dep_id(s).to_string())
                .collect();
            for dep in deps {
                if known.contains(&dep) {
                    continue;
                }
                let Some((repo, branch)) = index.find(self, &dep)? else {
                    anyhow::bail!(
                        "dependency '{dep}' of '{id}' was not found among '{MODULE_TOPIC}' modules"
                    );
                };
                let planned = self.pin(&repo, &branch, optional, Some(name.clone()))?;
                // The index read the branch a moment ago; the pinned commit is what will be
                // installed, and it has to still be the module that was asked for.
                if planned.manifest.id != dep {
                    anyhow::bail!(
                        "{repo} was found for dependency '{dep}' but its module.toml now says \
                         id = '{}'",
                        planned.manifest.id
                    );
                }
                known.insert(dep);
                modules.push(planned);
            }
            i += 1;
        }
        Ok(())
    }

    /// See [`resolve_tree`]. `installed` is everything installed.
    fn resolve_tree(
        &self,
        full_name: &str,
        branch: Option<&str>,
        installed: &[InstalledModule],
    ) -> Result<InstallPlan> {
        check_repo(full_name)?;
        let branch = match branch {
            Some(b) => b.to_string(),
            None => self.default_branch(full_name)?,
        };
        let root = self.pin(full_name, &branch, false, None)?;
        let root_name = root.manifest.name.clone();
        let optional_ids: Vec<String> = root
            .manifest
            .optional_dependencies
            .iter()
            .map(|s| module_manifest::dep_id(s).to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let mut known: HashSet<String> = installed.iter().map(|m| m.id.clone()).collect();
        known.insert(root.manifest.id.clone());
        let mut modules = vec![root];
        let mut index = TopicIndex::new();
        self.add_required(&mut modules, 0, &mut known, &mut index, false)?;
        check_folders(&modules, &[], installed)?;

        // Each optional dependency with everything IT needs, or not at all: one whose own
        // dependency cannot be found is reported and left out, rather than half-installed.
        let mut unavailable_optional = Vec::new();
        for opt in optional_ids {
            if known.contains(&opt) {
                continue;
            }
            let mut trial_known = known.clone();
            let mut trial: Vec<PlannedModule> = Vec::new();
            let attempt = (|| -> Result<()> {
                let Some((repo, branch)) = index.find(self, &opt)? else {
                    anyhow::bail!("not found among '{MODULE_TOPIC}' modules");
                };
                let mut planned = self.pin(&repo, &branch, true, Some(root_name.clone()))?;
                planned.via_optional = true;
                if planned.manifest.id != opt {
                    anyhow::bail!("{repo} now says id = '{}'", planned.manifest.id);
                }
                trial_known.insert(opt.clone());
                trial.push(planned);
                self.add_required(&mut trial, 0, &mut trial_known, &mut index, true)?;
                // An optional module whose folder is taken is left out like any other that
                // cannot be installed, rather than failing the install.
                check_folders(&trial, &modules, installed)
            })();
            match attempt {
                Ok(()) => {
                    known = trial_known;
                    modules.extend(trial);
                }
                Err(e) => {
                    crate::logging::line(
                        "manager",
                        &format!("optional dependency '{opt}' of '{full_name}' left out: {e:#}"),
                    );
                    unavailable_optional.push((opt, format!("{e:#}")));
                }
            }
        }
        Ok(InstallPlan { modules, unavailable_optional })
    }

    /// See [`resolve_update`]. `installed` is everything installed, `m` among it.
    fn resolve_update(&self, m: &InstalledModule, installed: &[InstalledModule]) -> Result<UpdatePlan> {
        let src = m.source.as_ref().ok_or_else(|| {
            anyhow::anyhow!("\u{201c}{}\u{201d} was not installed from GitHub", m.id)
        })?;
        let target = self.pin(&src.repo, &src.branch, false, None)?;
        if target.manifest.id != m.id {
            anyhow::bail!(
                "the new version of '{}' in {} calls itself '{}'; uninstall it and install the \
                 new one instead",
                m.id,
                src.repo,
                target.manifest.id
            );
        }
        let have: HashSet<String> = installed.iter().map(|i| i.id.clone()).collect();
        let added_capabilities: Vec<String> = target
            .manifest
            .capabilities
            .require
            .iter()
            .filter(|c| !m.capabilities.contains(c))
            .cloned()
            .collect();
        let before: HashSet<&str> = m
            .dependencies
            .iter()
            .chain(m.optional_dependencies.iter())
            .map(String::as_str)
            .collect();
        let mut added_installed_dependencies = Vec::new();
        for (spec, optional) in target
            .manifest
            .dependencies
            .iter()
            .map(|s| (s, false))
            .chain(target.manifest.optional_dependencies.iter().map(|s| (s, true)))
        {
            let dep = module_manifest::dep_id(spec);
            if !dep.is_empty() && !before.contains(dep) && have.contains(dep) {
                added_installed_dependencies.push((dep.to_string(), optional));
            }
        }
        let (id, name) = (m.id.clone(), target.manifest.name.clone());
        let mut known = have;
        let mut modules = vec![target];
        self.add_required(&mut modules, 0, &mut known, &mut TopicIndex::new(), false)?;
        check_folders(&modules, &[], installed)?;
        Ok(UpdatePlan {
            id,
            name,
            from_version: m.version.clone(),
            install: InstallPlan { modules, unavailable_optional: Vec::new() },
            added_capabilities,
            added_installed_dependencies,
        })
    }
}

/// Works out everything installing `full_name` would add, WITHOUT writing anything: the
/// module itself and, recursively, every dependency it declares that isn't already installed —
/// each dependency id resolved to a repository through the module-topic search — plus the
/// module's OPTIONAL dependencies and what they need, marked `optional`.
///
/// Every module is pinned to the commit its manifest was read at, and [`install_resolved`]
/// downloads exactly that commit: what was reviewed is what gets installed, even if the
/// repository moves on in between.
///
/// A required dependency that cannot be resolved fails the plan; an optional one that cannot
/// be is listed in `unavailable_optional` and left out. `branch` is the branch to install
/// from, or `None` to ask GitHub for the default one.
pub fn resolve_tree(full_name: &str, branch: Option<&str>) -> Result<InstallPlan> {
    GitHub::live().resolve_tree(full_name, branch, &installed())
}

/// Installs what [`resolve_tree`] planned: every module in `plan`, the optional ones only
/// when `with_optional`. Dependencies go in before the modules that need them, so a download
/// that fails part-way leaves nothing installed that cannot load. Each module is unpacked and
/// checked in a staging folder before it replaces anything — see `install_one`. Returns the
/// installed modules in the order they were written.
pub fn install_resolved(plan: &InstallPlan, with_optional: bool) -> Result<Vec<LoadedModule>> {
    install_plan_into(&GitHub::live(), plan, with_optional, &modules_dir())
}

/// What updating the installed module `m` to its branch's latest commit would do: the new
/// version pinned, the capabilities it adds, and the dependencies it adds — those not
/// installed yet with their own trees, those already installed by id. Writes nothing.
pub fn resolve_update(m: &InstalledModule) -> Result<UpdatePlan> {
    let all = installed();
    GitHub::live().resolve_update(m, &all)
}

/// Applies an update [`resolve_update`] planned: the new version and every module it now needs.
/// Returns what was written, in the order it was written.
pub fn install_update(plan: &UpdatePlan) -> Result<Vec<LoadedModule>> {
    install_plan_into(&GitHub::live(), &plan.install, false, &modules_dir())
}

/// The one writer behind [`install_resolved`] and [`install_update`].
fn install_plan_into(
    gh: &GitHub,
    plan: &InstallPlan,
    with_optional: bool,
    modules_dir: &Path,
) -> Result<Vec<LoadedModule>> {
    let order = install_order(plan, with_optional);
    // Every folder is checked before anything is downloaded, so a plan that would overwrite
    // another module fails whole rather than after its dependencies were written. The plan
    // was checked against the installed list when it was made; this is the file system's
    // answer now, which also sees a folder that holds no module at all.
    let mut folders: HashSet<String> = HashSet::new();
    for p in &order {
        if !folders.insert(repo_name(&p.repo).to_lowercase()) {
            anyhow::bail!(
                "two modules of this install would go into modules/{}",
                repo_name(&p.repo)
            );
        }
        check_destination(&modules_dir.join(repo_name(&p.repo)), &p.manifest.id)?;
    }
    let mut done = Vec::new();
    for p in order {
        done.push(install_one(gh, p, modules_dir)?);
    }
    Ok(done)
}

/// Downloads one planned module and puts it in place of whatever version is installed.
///
/// Nothing is written to `modules/<repo>` until the new version is complete and checked. It is
/// unpacked into a staging folder beside it (`modules/.staging-…`, which module discovery
/// skips, like every folder whose name starts with a dot), given its `.source.toml`, loaded,
/// and its `module.toml` compared with the one reviewed. Only then is the installed folder
/// renamed into the staging folder and the new one renamed into its place — two renames on one
/// volume — and the staging folder, holding the old version, removed. Any failure before the
/// swap removes the staging folder alone, so an update that fails leaves the installed version
/// exactly as it was; that used to cost the user the module (unpacking removed the installed
/// folder first, and a manifest that failed the comparison was removed after it).
///
/// On Windows the first rename fails while a file in the installed folder is open, and the
/// update then fails with the installed version in place, where deleting it file by file
/// could have stopped half-way.
fn install_one(gh: &GitHub, p: &PlannedModule, modules_dir: &Path) -> Result<LoadedModule> {
    let folder = repo_name(&p.repo);
    let dest = modules_dir.join(folder);
    let bytes = gh.download(&p.repo, &p.sha)?;
    std::fs::create_dir_all(modules_dir)
        .with_context(|| format!("creating {}", modules_dir.display()))?;
    // Removed, with whatever is in it, when it goes out of scope — on every path out of here.
    let staging = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(modules_dir)
        .with_context(|| format!("creating a staging folder in {}", modules_dir.display()))?;
    let unchanged = || format!("Nothing in modules/{folder} was changed.");
    let new = staging.path().join("new");
    module_manifest::unpack_zip(&bytes, &new, ArchiveLayout::OneFolder)
        .with_context(|| format!("unpacking {}. {}", p.repo, unchanged()))?;
    write_source(
        &new,
        &Source { repo: p.repo.clone(), branch: p.branch.clone(), sha: p.sha.clone() },
    )?;
    let mut loaded = LoadedModule::load_dir(&new)
        .with_context(|| format!("{} is not a valid module. {}", p.repo, unchanged()))?;
    // The same commit, so the same file — unless something between here and GitHub
    // disagrees. What was reviewed is what may be installed.
    if let Some(what) = differs(&p.manifest, &loaded.manifest) {
        anyhow::bail!(
            "{} was not installed: its downloaded module.toml differs from the one reviewed \
             ({what}). {}",
            p.repo,
            unchanged()
        );
    }
    // Once more, right before replacing: the folder may have changed since the plan's check.
    check_destination(&dest, &p.manifest.id)?;
    let old = staging.path().join("old");
    let had_old = dest.exists();
    if had_old {
        std::fs::rename(&dest, &old).with_context(|| {
            format!(
                "could not move the installed modules/{folder} aside to replace it — is a file \
                 in it open? {}",
                unchanged()
            )
        })?;
    }
    if let Err(e) = std::fs::rename(&new, &dest) {
        if had_old && std::fs::rename(&old, &dest).is_err() {
            // Both renames failed, which leaves the old version where it can still be found:
            // `keep` keeps the staging folder rather than deleting it with the old version
            // inside.
            let kept = staging.keep();
            return Err(e).with_context(|| {
                format!(
                    "could not move {} into modules/{folder}, nor the installed version back; \
                     the installed version is in {}",
                    p.repo,
                    kept.join("old").display()
                )
            });
        }
        return Err(e).with_context(|| {
            format!("could not move {} into modules/{folder}. {}", p.repo, unchanged())
        });
    }
    // The old version goes with the staging folder. A failure to remove it leaves a dot-folder
    // that nothing loads; it is logged so that it can be found.
    let path = staging.path().to_path_buf();
    if let Err(e) = staging.close() {
        crate::logging::line(
            "manager",
            &format!("could not remove the staging folder {}: {e}", path.display()),
        );
    }
    loaded.root = dest;
    Ok(loaded)
}

/// Refuses to install `id` into `dest` when something else is there: another module, or a
/// folder with no readable `module.toml` — somebody's work in progress, say — which would
/// otherwise be deleted. The same module (an update, or a reinstall) may be replaced.
fn check_destination(dest: &Path, id: &str) -> Result<()> {
    if !dest.exists() {
        return Ok(());
    }
    match LoadedModule::load_dir(dest) {
        Ok(m) if m.manifest.id == id => Ok(()),
        Ok(m) => anyhow::bail!(
            "{id} would be installed into {}, which holds another module, {} ({}). Uninstall \
             that one first.",
            dest.display(),
            one_line(&m.manifest.name, 100),
            m.manifest.id
        ),
        Err(_) => anyhow::bail!(
            "{id} would be installed into {}, which exists and is not a module that can be read. \
             Move that folder away first; nothing was installed.",
            dest.display()
        ),
    }
}

/// Refuses modules of a plan (`new`) that would be written into a folder another module holds
/// or will hold: an installed module with another id, a module already in the plan
/// (`planned`), or another of `new`. The folder is the repository's name, compared without
/// case, as Windows and macOS compare it — and the repository name is chosen by whoever
/// publishes it, so `someone/kontakt` would otherwise have replaced the installed Kontakt
/// module while the review said it adds a module.
fn check_folders(
    new: &[PlannedModule],
    planned: &[PlannedModule],
    installed: &[InstalledModule],
) -> Result<()> {
    let folder = |repo: &str| repo_name(repo).to_lowercase();
    let mut taken: HashMap<String, &str> =
        planned.iter().map(|p| (folder(&p.repo), p.repo.as_str())).collect();
    for p in new {
        let f = folder(&p.repo);
        if let Some(other) = taken.insert(f.clone(), p.repo.as_str()) {
            anyhow::bail!(
                "{other} and {} would both be installed into modules/{}",
                p.repo,
                repo_name(&p.repo)
            );
        }
        let holder = installed.iter().find(|m| {
            m.id != p.manifest.id
                && m.dir.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == f)
        });
        if let Some(m) = holder {
            anyhow::bail!(
                "{} ({}) would be installed into modules/{}, which holds another module, {} \
                 ({}). Uninstall that one first.",
                one_line(&p.manifest.name, 100),
                p.repo,
                repo_name(&p.repo),
                one_line(&m.name, 100),
                m.id
            );
        }
    }
    Ok(())
}

/// The modules of `plan` to write, each after every module of the plan it depends on, so that
/// a download failing part-way never leaves a module installed without a dependency it
/// needs. Discovery order already puts most dependencies after their dependents; a module
/// reached twice (`root → a, b` and `b → a`) is what needs the sort. A cycle cannot be
/// ordered, and its members follow in discovery order rather than being dropped.
fn install_order(plan: &InstallPlan, with_optional: bool) -> Vec<&PlannedModule> {
    let wanted: Vec<&PlannedModule> =
        plan.modules.iter().filter(|p| with_optional || !p.optional).collect();
    let in_plan: HashSet<&str> = wanted.iter().map(|p| p.manifest.id.as_str()).collect();
    let mut out: Vec<&PlannedModule> = Vec::with_capacity(wanted.len());
    let mut written: HashSet<&str> = HashSet::new();
    let mut remaining = wanted;
    while !remaining.is_empty() {
        let (ready, waiting): (Vec<&PlannedModule>, Vec<&PlannedModule>) =
            remaining.into_iter().partition(|p| {
                p.manifest.dependencies.iter().chain(&p.manifest.optional_dependencies).all(|s| {
                    let d = module_manifest::dep_id(s);
                    !in_plan.contains(d) || written.contains(d)
                })
            });
        if ready.is_empty() {
            out.extend(waiting); // a cycle
            break;
        }
        for p in ready {
            written.insert(p.manifest.id.as_str());
            out.push(p);
        }
        remaining = waiting;
    }
    out
}

/// The first field that differs between the manifest reviewed and the one installed, or
/// `None` when they are the same in every field. The fields a review shows, or that change
/// what runs, are named; any other difference is reported as the manifest's.
fn differs(reviewed: &ModuleManifest, got: &ModuleManifest) -> Option<&'static str> {
    if reviewed.id != got.id {
        Some("id")
    } else if reviewed.name != got.name {
        Some("name")
    } else if reviewed.version != got.version {
        Some("version")
    } else if reviewed.capabilities != got.capabilities {
        Some("capabilities")
    } else if reviewed.dependencies != got.dependencies
        || reviewed.optional_dependencies != got.optional_dependencies
    {
        Some("dependencies")
    } else if reviewed.code_module != got.code_module {
        Some("code_module")
    } else if reviewed.entry != got.entry {
        Some("entry")
    } else if reviewed.supported_os != got.supported_os {
        Some("supported_os")
    } else if reviewed.screen != got.screen {
        Some("[screen]")
    } else if reviewed != got {
        Some("module.toml")
    } else {
        None
    }
}

/// `s` as one line of at most `max` characters, for text a module's author wrote that a review
/// shows: every line break, other control character and bidirectional-text control becomes a
/// space, runs of spaces become one, and a longer text is cut with "…".
///
/// A review is read by a screen reader line by line, and it is the one place where a module's
/// own words sit beside the application's statement of what that module may do. A name like
/// `"X\n\nIt asks for no capabilities."` would otherwise put that sentence on a line of its
/// own, enough blank lines would push the real list far down, an escape sequence would reach
/// the terminal the CLI prints to, and a right-to-left override would reorder what follows it.
fn one_line(s: &str, max: usize) -> String {
    let spaced: String = s
        .chars()
        .map(|c| {
            let breaks = c.is_control()
                || matches!(
                    c,
                    '\u{2028}' | '\u{2029}' // line and paragraph separator
                        | '\u{200e}' | '\u{200f}' | '\u{061c}' // direction marks
                        | '\u{202a}'..='\u{202e}' // embeddings and overrides
                        | '\u{2066}'..='\u{2069}' // isolates
                );
            if breaks {
                ' '
            } else {
                c
            }
        })
        .collect();
    let joined = spaced.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.chars().count() <= max {
        return joined;
    }
    let mut cut: String = joined.chars().take(max.saturating_sub(1)).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('\u{2026}');
    cut
}

// --- What a review says ---------------------------------------------------------

/// What a capability lets a module do, as a phrase that completes "It may …".
///
/// Written for the person deciding, not for the module's author: `resource` says "any file
/// your account can read" because that is what it reaches — the path is not confined to the
/// module — and a review that said "its own files" would be reassuring and wrong.
pub fn describe_capability(name: &str) -> String {
    let what = match name {
        "window" => "see which window and control are in front, and follow the focus as it moves",
        // `host.screen.save` and `saveMarked` belong to it: a full screenshot, written as a PNG
        // to any path the module names.
        "screen" => {
            "see everything on the screen, search it for images, and save pictures of it as PNG \
             files anywhere your account can write"
        }
        "ocr" => "read text on the screen",
        "element" => "read and operate other applications\u{2019} controls through accessibility",
        "input" => "move and click the mouse, and send keystrokes and text",
        "keys" => "take keys away from the application in front while it holds them",
        "hotkey" => "register keyboard shortcuts that work in every application",
        "gamepad" => "watch game controllers",
        "speech" => "speak through your screen reader",
        "sound" => "play sounds",
        "timer" => "run code on a timer, in the background",
        "settings" => "keep settings of its own",
        "log" => "write to the application\u{2019}s log",
        "path" => "turn file names into full paths",
        "resource" => "read files: its own, and any other file your account can read",
        "arbiter" => "compete with other overlays for the same window",
        other => {
            return format!(
                "\u{201c}{}\u{201d} \u{2014} a name this version of the application does not \
                 know, which unlocks nothing",
                one_line(other, 60)
            )
        }
    };
    format!("{what} ({name})")
}

/// One module's paragraph in a review: who it is, why it is there, and what it may do. Every
/// word the module's author chose goes through [`one_line`].
fn describe_planned(n: usize, p: &PlannedModule, os: &str) -> String {
    let m = &p.manifest;
    let why = match (&p.needed_by, p.via_optional) {
        (None, _) => "the module you chose".to_string(),
        (Some(by), false) => format!("needed by {}", one_line(by, 100)),
        (Some(by), true) => format!("optional for {}", one_line(by, 100)),
    };
    let mut s = format!(
        "{n}. {}, version {} ({}), from {} \u{2014} {why}.\n",
        one_line(&m.name, 100),
        m.version,
        m.id,
        p.repo
    );
    s.push_str(&describe_capabilities(&m.capabilities.require));
    if !m.runs_on(os) {
        let claimed: Vec<String> = m.supported_os.iter().map(|o| one_line(o, 30)).collect();
        s.push_str(&format!(
            "\nNot for this system: it declares support for {}, and this machine is {os}. It can \
             be installed, but it will not be loaded here.\n",
            claimed.join(", ")
        ));
    }
    s
}

fn describe_capabilities(caps: &[String]) -> String {
    if caps.is_empty() {
        return "It asks for no capabilities.\n".to_string();
    }
    let mut s = String::from("It may:\n");
    for c in caps {
        s.push_str(&format!("\u{2022} {}\n", describe_capability(c)));
    }
    s
}

/// The text of the install review: every module the install adds, each with what it may do.
/// `os` is the machine's `std::env::consts::OS`.
pub fn install_review_text(plan: &InstallPlan, os: &str) -> String {
    let required: Vec<&PlannedModule> = plan.modules.iter().filter(|m| !m.optional).collect();
    let optional: Vec<&PlannedModule> = plan.modules.iter().filter(|m| m.optional).collect();
    let chosen = one_line(plan.modules.first().map(|m| m.manifest.name.as_str()).unwrap_or(""), 100);
    let mut s = if required.len() == 1 {
        format!("Installing \u{201c}{chosen}\u{201d} adds this module.")
    } else {
        format!(
            "Installing \u{201c}{chosen}\u{201d} adds {} modules: the one you chose and the \
             modules it needs.",
            required.len()
        )
    };
    s.push_str(
        " A module runs its own code; the capabilities below are what that code may reach. \
         Nothing is installed until you choose to install.\n\n",
    );
    let mut n = 0;
    for p in &required {
        n += 1;
        s.push_str(&describe_planned(n, p, os));
        s.push('\n');
    }
    if !optional.is_empty() {
        s.push_str(&format!(
            "Optional modules \u{2014} extra features \u{201c}{chosen}\u{201d} can use, not \
             required. They are installed only if you choose to install with the optional \
             modules:\n\n"
        ));
        for p in &optional {
            n += 1;
            s.push_str(&describe_planned(n, p, os));
            s.push('\n');
        }
    }
    for (id, why) in &plan.unavailable_optional {
        // The reason can quote a manifest — a TOML error shows the line it failed on.
        s.push_str(&format!(
            "The optional module {} cannot be installed: {}.\n",
            one_line(id, 130),
            one_line(why, 300)
        ));
    }
    s.trim_end().to_string()
}

/// The text of the update review. `installed` is everything installed, for the capabilities
/// of dependencies the new version starts using that are already there.
pub fn update_review_text(plan: &UpdatePlan, installed: &[InstalledModule], os: &str) -> String {
    let Some(target) = plan.install.modules.first() else {
        return String::new();
    };
    let mut s = format!(
        "Updating \u{201c}{}\u{201d} from version {} to version {} changes what it may do. \
         Nothing is changed until you choose to update.\n\n",
        one_line(&plan.name, 100),
        plan.from_version,
        target.manifest.version
    );
    if !plan.added_capabilities.is_empty() {
        s.push_str("The new version also asks to:\n");
        for c in &plan.added_capabilities {
            s.push_str(&format!("\u{2022} {}\n", describe_capability(c)));
        }
        s.push('\n');
    }
    if !plan.added_installed_dependencies.is_empty() {
        s.push_str("It starts using these modules you already have; their code will run for it:\n\n");
        for (id, optional) in &plan.added_installed_dependencies {
            let m = installed.iter().find(|m| &m.id == id);
            let name = one_line(m.map(|m| m.name.as_str()).unwrap_or(id.as_str()), 100);
            s.push_str(&format!(
                "{name} ({id}){}.\n",
                if *optional { ", when it is enabled" } else { "" }
            ));
            s.push_str(&describe_capabilities(m.map(|m| m.capabilities.as_slice()).unwrap_or(&[])));
            s.push('\n');
        }
    }
    let new_modules = &plan.install.modules[1..];
    if !new_modules.is_empty() {
        s.push_str(&format!(
            "It needs {} module{} you do not have yet, which will be installed with it:\n\n",
            new_modules.len(),
            if new_modules.len() == 1 { "" } else { "s" }
        ));
        for (i, p) in new_modules.iter().enumerate() {
            s.push_str(&describe_planned(i + 1, p, os));
            s.push('\n');
        }
    }
    s.trim_end().to_string()
}

// --- Installed modules ------------------------------------------------------------

/// Lists modules installed in the portable modules directory.
pub fn installed() -> Vec<InstalledModule> {
    installed_in(&modules_dir())
}

/// Names in the log every folder under `modules/` that is skipped because its manifest fails,
/// with the reason. Called by `host::run` once the log is open: the launcher reads the
/// installed list before that, when a log line still goes nowhere, so without this call the
/// start-up — where a module vanishing matters most — would never say why.
pub fn log_unloadable() {
    let _ = installed_in_with(&modules_dir(), true);
}

fn installed_in(dir: &Path) -> Vec<InstalledModule> {
    installed_in_with(dir, false)
}

/// `force_log`: write each skip reason even when this run has written it already.
fn installed_in_with(dir: &Path, force_log: bool) -> Vec<InstalledModule> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    let ids = |specs: &[String]| -> Vec<String> {
        specs.iter().map(|s| module_manifest::dep_id(s).to_string()).collect()
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        // A folder whose name starts with a dot is not a module: an install's staging folder
        // (`install_one`), or a system's hidden folder.
        if !dir.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let m = match LoadedModule::load_dir(&dir) {
            Ok(m) => m,
            Err(e) => {
                log_skipped(&dir, &e, force_log);
                continue;
            }
        };
        out.push(InstalledModule {
            dependencies: ids(&m.manifest.dependencies),
            optional_dependencies: ids(&m.manifest.optional_dependencies),
            capabilities: m.manifest.capabilities.require,
            id: m.manifest.id,
            name: m.manifest.name,
            version: m.manifest.version,
            source: read_source(&dir),
            supported_os: m.manifest.supported_os,
            dir,
        });
    }
    out
}

/// Says in the log why the folder `dir` under `modules/` is not loaded — once per folder and
/// reason in a run (unless `force`), because the list is read again every time the manager
/// looks.
///
/// A folder whose manifest fails used to vanish without a word, and the name rules
/// (`ModuleManifest::validate`) are newer than some modules: one with a space in its id, say,
/// would have disappeared from one start to the next with nothing to say why.
fn log_skipped(dir: &Path, e: &anyhow::Error, force: bool) {
    static SAID: OnceLock<std::sync::Mutex<HashSet<String>>> = OnceLock::new();
    let line = format!("{} is not loaded: {}", dir.display(), one_line(&format!("{e:#}"), 400));
    let said = SAID.get_or_init(Default::default);
    let fresh = said.lock().map(|mut s| s.insert(line.clone())).unwrap_or(false);
    if fresh || force {
        crate::logging::line("modules", &line);
    }
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
    GitHub::live().update_available(m)
}

impl GitHub {
    /// See [`update_available`].
    fn update_available(&self, m: &InstalledModule) -> Option<String> {
        let src = m.source.as_ref()?;
        let upstream = self.fetch_manifest(&src.repo, &src.branch).ok()?;
        match (semver::Version::parse(&upstream.version), semver::Version::parse(&m.version)) {
            (Ok(up), Ok(cur)) => (up > cur).then_some(upstream.version),
            _ => {
                let latest = self.head_sha(&src.repo, &src.branch).ok()?;
                (!latest.is_empty() && latest != src.sha).then_some(latest)
            }
        }
    }
}

/// Reads `.source.toml`, or `None` for a module that was not installed from GitHub.
fn read_source(dir: &Path) -> Option<Source> {
    let text = std::fs::read_to_string(dir.join(".source.toml")).ok()?;
    toml::from_str(&text).ok()
}

/// Writes `.source.toml` with the TOML serializer. It was a format string, and a `"` or a `\`
/// in a branch name made the file unreadable — which [`read_source`] reports as "not installed
/// from GitHub", so the module silently lost its updates.
fn write_source(dir: &Path, source: &Source) -> Result<()> {
    let text = toml::to_string(source).context("writing .source.toml")?;
    std::fs::write(dir.join(".source.toml"), text)
        .with_context(|| format!("writing {}", dir.join(".source.toml").display()))
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

/// The transitive dependents of `id`, ordered so that a module always comes AFTER
/// every module it depends on that is also in the list — the order they must be
/// rebuilt in when `id` changes.
///
/// A code module's source is copied into each dependent's VM when that dependent is
/// built, so rebuilding them in the wrong order would hand a module its dependency's
/// OLD code and then rebuild the dependency underneath it. `transitive_dependents`
/// answers "who is affected" in discovery order, which is not that.
///
/// A dependency cycle cannot be ordered; the remainder is appended as-is rather than
/// dropped, so a cycle degrades to "possibly stale" instead of "silently missing".
pub fn reload_order(id: &str, graph: &[(String, Vec<String>)]) -> Vec<String> {
    use std::collections::HashSet;
    let affected = transitive_dependents(id, graph);
    let in_list: HashSet<String> = affected.iter().cloned().collect();
    let mut out: Vec<String> = Vec::new();
    let mut done: HashSet<String> = HashSet::new();
    let mut remaining = affected;
    while !remaining.is_empty() {
        let mut still: Vec<String> = Vec::new();
        let mut progressed = false;
        for m in remaining {
            let ready = graph
                .iter()
                .find(|(mid, _)| *mid == m)
                .map(|(_, deps)| {
                    deps.iter().all(|d| !in_list.contains(d) || done.contains(d))
                })
                .unwrap_or(true);
            if ready {
                done.insert(m.clone());
                out.push(m);
                progressed = true;
            } else {
                still.push(m);
            }
        }
        if !progressed {
            out.extend(still); // cycle — no valid order exists
            break;
        }
        remaining = still;
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
    fn reload_order_puts_a_dependency_before_its_dependent() {
        let graph = sample();
        let order = reload_order("overlay", &graph);
        // Everyone affected is there, and kontakt is rebuilt before css, which holds a
        // copy of kontakt's code.
        assert_eq!(order.len(), 3);
        let pos = |id: &str| order.iter().position(|m| m == id).unwrap();
        assert!(pos("kontakt") < pos("css"));
        assert!(order.contains(&"sforzando".to_string()));
        // A leaf has no dependents at all.
        assert!(reload_order("css", &graph).is_empty());
    }

    #[test]
    fn reload_order_survives_a_cycle() {
        // a <-> b, both depending on the changed module: no valid order exists, but
        // neither may be dropped from the list.
        let graph = g(&[("a", &["x", "b"]), ("b", &["x", "a"]), ("x", &[])]);
        let mut order = reload_order("x", &graph);
        order.sort();
        assert_eq!(order, ["a", "b"]);
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

/// The requests this module makes, checked against a GitHub that answers from a table: the
/// middleware sees each request with its final URL — query encoded — and nothing reaches the
/// network.
#[cfg(test)]
mod github_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use ureq::http;
    use ureq::middleware::{Middleware, MiddlewareNext};

    #[derive(Clone, Default)]
    struct Fake {
        routes: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        asked: Arc<Mutex<Vec<String>>>,
    }

    impl Middleware for Fake {
        fn handle(
            &self,
            req: http::Request<ureq::SendBody>,
            _next: MiddlewareNext,
        ) -> Result<http::Response<ureq::Body>, ureq::Error> {
            let url = req.uri().to_string();
            self.asked.lock().unwrap().push(url.clone());
            match self.routes.lock().unwrap().get(&url) {
                Some(body) => Ok(http::Response::builder()
                    .status(200)
                    .body(ureq::Body::builder().data(body.clone()))
                    .unwrap()),
                None => Err(ureq::Error::StatusCode(404)),
            }
        }
    }

    impl Fake {
        fn github(&self) -> GitHub {
            GitHub { agent: ureq::Agent::config_builder().middleware(self.clone()).build().into() }
        }
        fn route(&self, url: impl Into<String>, body: impl Into<Vec<u8>>) {
            self.routes.lock().unwrap().insert(url.into(), body.into());
        }
        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    fn search_url(q_encoded: &str, page: usize) -> String {
        format!(
            "{API}/search/repositories?q={q_encoded}&sort=stars&order=desc&per_page=100&page={page}"
        )
    }

    fn repo_json(full_name: &str) -> String {
        format!(
            r#"{{"full_name":"{full_name}","description":null,"stargazers_count":1,"default_branch":"main","updated_at":"2026-01-01T00:00:00Z"}}"#
        )
    }

    fn search_page(total: usize, names: &[String]) -> String {
        let items: Vec<String> = names.iter().map(|n| repo_json(n)).collect();
        format!(r#"{{"total_count":{total},"items":[{}]}}"#, items.join(","))
    }

    #[test]
    fn search_terms_are_encoded_not_pasted() {
        let fake = Fake::default();
        let gh = fake.github();
        // "+" would have been read by GitHub as a space, and "&" would have ended `q`.
        let cpp = search_url("topic%3Aosap-module%20c%2B%2B", 1);
        fake.route(cpp.clone(), search_page(1, &["o/cpp".to_string()]));
        let found = gh.search("  c++ ").expect("search");
        assert_eq!(found.len(), 1);
        assert_eq!(fake.asked(), vec![cpp]);

        let amp = search_url("topic%3Aosap-module%20a%20%26%20b", 1);
        fake.route(amp.clone(), search_page(0, &[]));
        assert!(gh.search("a & b").expect("search").is_empty());
        assert_eq!(fake.asked().last(), Some(&amp));

        // No free text: the topic alone.
        let all = search_url("topic%3Aosap-module", 1);
        fake.route(all.clone(), search_page(0, &[]));
        gh.search("").expect("search");
        assert_eq!(fake.asked().last(), Some(&all));
    }

    #[test]
    fn search_follows_pages_until_a_short_one() {
        let fake = Fake::default();
        let first: Vec<String> = (0..100).map(|i| format!("o/m{i}")).collect();
        // The second page repeats one from the first, as a star moving mid-search would.
        let mut second: Vec<String> = (100..149).map(|i| format!("o/m{i}")).collect();
        second.push("o/m7".to_string());
        fake.route(search_url("topic%3Aosap-module", 1), search_page(150, &first));
        fake.route(search_url("topic%3Aosap-module", 2), search_page(150, &second));
        let found = fake.github().search("").expect("search");
        assert_eq!(found.len(), 149, "every module once");
        assert_eq!(fake.asked().len(), 2, "no third page after a short one");
        assert_eq!(found[120].full_name, "o/m120");
    }

    #[test]
    fn search_stops_at_the_page_cap() {
        let fake = Fake::default();
        for page in 1..=MAX_PAGES + 2 {
            let names: Vec<String> =
                (0..PER_PAGE).map(|i| format!("o/p{page}-{i}")).collect();
            fake.route(search_url("topic%3Aosap-module", page), search_page(5000, &names));
        }
        let found = fake.github().search("").expect("search");
        assert_eq!(fake.asked().len(), MAX_PAGES);
        assert_eq!(found.len(), MAX_PAGES * PER_PAGE);
    }

    #[test]
    fn a_branch_goes_in_as_a_query_and_a_commit_into_the_path() {
        let fake = Fake::default();
        let gh = fake.github();
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let commits = format!("{API}/repos/o/r/commits?sha=feat%231&per_page=1");
        fake.route(commits.clone(), format!(r#"[{{"sha":"{sha}"}}]"#));
        assert_eq!(gh.head_sha("o/r", "feat#1").expect("sha"), sha);
        let manifest = format!("{RAW}/o/r/{sha}/module.toml");
        fake.route(manifest.clone(), "id = \"com.t.r\"\nname = \"R\"\nversion = \"1.0.0\"\n");
        assert_eq!(gh.fetch_manifest("o/r", sha).expect("manifest").id, "com.t.r");
        let zip = format!("{API}/repos/o/r/zipball/{sha}");
        fake.route(zip.clone(), b"PK".to_vec());
        gh.download("o/r", sha).expect("download");
        assert_eq!(fake.asked(), vec![commits, manifest, zip]);
    }

    #[test]
    fn nothing_is_requested_for_a_name_that_is_not_owner_slash_repo() {
        let fake = Fake::default();
        let gh = fake.github();
        for bad in ["owner/..", "owner/.hidden", "../x", "a/b/c", "a", "a b/c", "o/r?x=1", "o/r#x", ""] {
            assert!(check_repo(bad).is_err(), "{bad:?} must be refused");
            assert!(gh.default_branch(bad).is_err());
            assert!(gh.head_sha(bad, "main").is_err());
        }
        assert!(fake.asked().is_empty());
        for good in ["o/r", "Timtam/osap-daw-hosts", "a-b/c.d_e"] {
            check_repo(good).unwrap_or_else(|e| panic!("{good:?}: {e:#}"));
        }
        // A commit id is forty hex digits, and nothing else reaches a download URL.
        assert!(gh.download("o/r", "main").is_err());
        assert!(gh.download("o/r", "../../x").is_err());
        assert!(fake.asked().is_empty());
    }

    /// A small published ecosystem: each repository `o/<name>` on branch `main` at one commit.
    struct World {
        fake: Fake,
    }

    fn sha_of(name: &str) -> String {
        // Forty hex digits derived from the name, so each repository has its own.
        let mut s: String = name.bytes().map(|b| format!("{b:02x}")).collect();
        s.push_str(&"0".repeat(40));
        s.truncate(40);
        s
    }

    impl World {
        fn new(published: &[(&str, String)]) -> Self {
            let fake = Fake::default();
            let names: Vec<String> = published.iter().map(|(n, _)| format!("o/{n}")).collect();
            fake.route(search_url("topic%3Aosap-module", 1), search_page(names.len(), &names));
            for (name, manifest) in published {
                let sha = sha_of(name);
                fake.route(
                    format!("{API}/repos/o/{name}/commits?sha=main&per_page=1"),
                    format!(r#"[{{"sha":"{sha}"}}]"#),
                );
                fake.route(format!("{RAW}/o/{name}/main/module.toml"), manifest.clone());
                fake.route(format!("{RAW}/o/{name}/{sha}/module.toml"), manifest.clone());
            }
            World { fake }
        }
    }

    fn manifest(id: &str, caps: &[&str], deps: &[&str], optional: &[&str]) -> String {
        let list = |v: &[&str]| v.iter().map(|s| format!("{s:?}")).collect::<Vec<_>>().join(", ");
        format!(
            "id = {id:?}\nname = {:?}\nversion = \"1.0.0\"\ndependencies = [{}]\n\
             optional_dependencies = [{}]\n\n[capabilities]\nrequire = [{}]\n",
            id.rsplit('.').next().unwrap().to_uppercase(),
            list(deps),
            list(optional),
            list(caps)
        )
    }

    fn sample_world() -> World {
        World::new(&[
            ("root", manifest("com.t.root", &["speech"], &["com.t.a", "com.t.b"], &["com.t.opt", "com.t.gone"])),
            ("a", manifest("com.t.a", &["screen", "ocr"], &["com.t.c"], &[])),
            ("b", manifest("com.t.b", &[], &[], &[])),
            ("c", manifest("com.t.c", &["input"], &["com.t.root"], &[])),
            ("opt", manifest("com.t.opt", &["keys"], &["com.t.d"], &[])),
            ("d", manifest("com.t.d", &[], &[], &[])),
        ])
    }

    #[test]
    fn a_plan_lists_every_module_the_install_adds_and_nothing_installed() {
        let world = sample_world();
        let have = vec![installed_row("com.t.b", &[], &[])];
        let plan = world.fake.github().resolve_tree("o/root", Some("main"), &have).expect("plan");
        let got: Vec<(&str, bool, Option<&str>)> = plan
            .modules
            .iter()
            .map(|m| (m.manifest.id.as_str(), m.optional, m.needed_by.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("com.t.root", false, None),
                ("com.t.a", false, Some("ROOT")),
                ("com.t.c", false, Some("A")), // c's dependency on root is not a second root
                ("com.t.opt", true, Some("ROOT")),
                ("com.t.d", true, Some("OPT")),
            ]
        );
        assert_eq!(plan.unavailable_optional.len(), 1);
        assert_eq!(plan.unavailable_optional[0].0, "com.t.gone");
        // Pinned: each module carries the commit its manifest was read at.
        assert_eq!(plan.modules[1].sha, sha_of("a"));
        // Resolving wrote nothing and downloaded nothing.
        assert!(world.fake.asked().iter().all(|u| !u.contains("zipball")));

        let text = install_review_text(&plan, "windows");
        for needle in [
            "adds 3 modules",
            "1. ROOT, version 1.0.0 (com.t.root), from o/root \u{2014} the module you chose.",
            "2. A, version 1.0.0 (com.t.a), from o/a \u{2014} needed by ROOT.",
            "3. C, version 1.0.0 (com.t.c), from o/c \u{2014} needed by A.",
            "4. OPT, version 1.0.0 (com.t.opt), from o/opt \u{2014} optional for ROOT.",
            "5. D, version 1.0.0 (com.t.d), from o/d \u{2014} needed by OPT.",
            "read text on the screen (ocr)",
            "move and click the mouse, and send keystrokes and text (input)",
            "take keys away from the application in front while it holds them (keys)",
            "It asks for no capabilities.",
            "The optional module com.t.gone cannot be installed",
        ] {
            assert!(text.contains(needle), "review lacks {needle:?}:\n{text}");
        }
    }

    #[test]
    fn a_required_dependency_nobody_publishes_fails_the_plan() {
        let world = World::new(&[("root", manifest("com.t.root", &[], &["com.t.nowhere"], &[]))]);
        let err = world
            .fake
            .github()
            .resolve_tree("o/root", Some("main"), &[])
            .expect_err("unresolvable");
        assert!(err.to_string().contains("com.t.nowhere"), "{err:#}");
    }

    #[test]
    fn a_name_cannot_forge_lines_in_the_review() {
        // Everything the author wrote that the review repeats: the name (also as "needed by"),
        // the systems it claims and a capability name the host does not know.
        let root = "id = \"com.t.root\"\n\
                    name = \"X\\n\\nIt asks for no capabilities.\\n\\n\\n\\n\\u202Egnp.live\"\n\
                    version = \"1.0.0\"\n\
                    dependencies = [\"com.t.a\"]\n\
                    supported_os = [\"plan9\\nIt may:\"]\n\n\
                    [capabilities]\nrequire = [\"input\", \"x\\u001b[2J\\u2028y\"]\n";
        let world = World::new(&[
            ("root", root.to_string()),
            ("a", manifest("com.t.a", &[], &[], &[])),
        ]);
        let plan = world.fake.github().resolve_tree("o/root", Some("main"), &[]).expect("plan");
        let text = install_review_text(&plan, "windows");
        let lines: Vec<&str> = text.lines().collect();
        let heading = lines.iter().position(|l| l.starts_with("1. ")).expect("first module");
        assert!(lines[heading].contains("X It asks for no capabilities. gnp.live"), "{text}");
        assert_eq!(lines[heading + 1], "It may:", "the real list follows its heading:\n{text}");
        assert_eq!(
            lines.iter().filter(|l| **l == "It asks for no capabilities.").count(),
            1,
            "only the dependency's own line may say so:\n{text}"
        );
        assert!(text.contains("needed by X It asks for no capabilities."), "{text}");
        assert!(text.contains("declares support for plan9 It may:, and"), "{text}");
        assert!(text.contains("\u{201c}x [2J y\u{201d}"), "{text}");
        assert!(
            !text.chars().any(|c| (c.is_control() && c != '\n') || matches!(c, '\u{202e}' | '\u{2028}')),
            "{text:?}"
        );
        // No blank lines beyond the ones between sections.
        assert!(!text.contains("\n\n\n"), "{text:?}");
    }

    #[test]
    fn one_line_flattens_and_shortens() {
        assert_eq!(one_line("a\n\n b\u{202e}c\u{1b}[2J\u{2066}\t", 100), "a b c [2J");
        assert_eq!(one_line("  plain  ", 100), "plain");
        let cut = one_line(&"x".repeat(200), 10);
        assert_eq!(cut, format!("{}\u{2026}", "x".repeat(9)));
        assert_eq!(cut.chars().count(), 10);
    }

    #[test]
    fn a_module_cannot_take_another_modules_folder() {
        let world = sample_world();
        let gh = world.fake.github();
        // An installed module with another id in the folder a required dependency would use:
        // the plan fails, naming both.
        let mut kontakt = installed_row("com.platform.kontakt", &[], &[]);
        kontakt.dir = PathBuf::from("modules").join("A"); // compared without case
        let err = gh
            .resolve_tree("o/root", Some("main"), std::slice::from_ref(&kontakt))
            .expect_err("folder taken");
        assert!(err.to_string().contains("holds another module"), "{err:#}");
        assert!(err.to_string().contains("com.platform.kontakt"), "{err:#}");

        // The same for an OPTIONAL module leaves that one out, with the reason, and the rest
        // of the install stands.
        kontakt.dir = PathBuf::from("modules").join("opt");
        let plan = gh
            .resolve_tree("o/root", Some("main"), std::slice::from_ref(&kontakt))
            .expect("plan without the optional module");
        assert!(plan.modules.iter().all(|m| m.manifest.id != "com.t.opt" && m.manifest.id != "com.t.d"));
        assert!(
            plan.unavailable_optional.iter().any(|(id, why)| id == "com.t.opt" && why.contains("holds another module")),
            "{:?}",
            plan.unavailable_optional
        );

        // The same module in its own folder is an update, not a clash.
        let mut same = installed_row("com.t.a", &[], &[]);
        same.dir = PathBuf::from("modules").join("a");
        check_folders(&[planned("o/a", "main", &manifest("com.t.a", &[], &[], &[]), false)], &[], &[same])
            .expect("the same module");

        // Two repositories of one name are one folder.
        let x1 = planned("alice/x", "main", &manifest("com.t.x1", &[], &[], &[]), false);
        let x2 = planned("bob/X", "main", &manifest("com.t.x2", &[], &[], &[]), false);
        let err = check_folders(&[x1, x2], &[], &[]).expect_err("one folder");
        assert!(err.to_string().contains("would both be installed"), "{err:#}");
    }

    #[test]
    fn nothing_is_downloaded_over_a_folder_that_is_not_the_module() {
        let fake = Fake::default();
        let tmp = tempfile::tempdir().unwrap();
        // Somebody's work in progress, whose module.toml does not parse.
        let wip = tmp.path().join("root");
        std::fs::create_dir_all(&wip).unwrap();
        std::fs::write(wip.join("module.toml"), b"id = \"com.me.wip\"\nname = ").unwrap();
        let root = manifest("com.t.root", &[], &["com.t.a"], &[]);
        let plan = InstallPlan {
            modules: vec![
                planned("o/root", "main", &root, false),
                planned("o/a", "main", &manifest("com.t.a", &[], &[], &[]), false),
            ],
            unavailable_optional: Vec::new(),
        };
        let err = install_plan_into(&fake.github(), &plan, false, tmp.path()).expect_err("taken");
        assert!(err.to_string().contains("not a module that can be read"), "{err:#}");
        assert!(fake.asked().is_empty(), "nothing downloaded, the dependency neither");
        assert!(!tmp.path().join("a").exists());
        assert_eq!(std::fs::read(wip.join("module.toml")).unwrap(), b"id = \"com.me.wip\"\nname = ");
    }

    fn zip_of(members: &[(&str, &[u8])]) -> Vec<u8> {
        use std::io::Write;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for (name, data) in members {
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn planned(repo: &str, branch: &str, text: &str, optional: bool) -> PlannedModule {
        PlannedModule {
            repo: repo.to_string(),
            branch: branch.to_string(),
            sha: sha_of(repo_name(repo)),
            manifest: ModuleManifest::parse(text).unwrap(),
            optional,
            needed_by: None,
            via_optional: false,
        }
    }

    #[test]
    fn installing_writes_the_reviewed_commits_and_where_they_came_from() {
        let fake = Fake::default();
        let tmp = tempfile::tempdir().unwrap();
        // A branch name the old format string could not survive.
        let branch = "we\"ird\\br'anch";
        let root = manifest("com.t.root", &["speech"], &["com.t.a"], &["com.t.opt"]);
        let dep = manifest("com.t.a", &[], &[], &[]);
        let opt = manifest("com.t.opt", &[], &[], &[]);
        for (name, text) in [("root", &root), ("a", &dep), ("opt", &opt)] {
            let top = format!("o-{name}-abc/");
            let bytes = zip_of(&[
                (&format!("{top}module.toml"), text.as_bytes()),
                (&format!("{top}src/main.luau"), b"return {}"),
            ]);
            fake.route(format!("{API}/repos/o/{name}/zipball/{}", sha_of(name)), bytes);
        }
        let plan = InstallPlan {
            modules: vec![
                planned("o/root", branch, &root, false),
                planned("o/a", "main", &dep, false),
                planned("o/opt", "main", &opt, true),
            ],
            unavailable_optional: Vec::new(),
        };
        let done = install_plan_into(&fake.github(), &plan, false, tmp.path()).expect("install");
        let ids: Vec<&str> = done.iter().map(|m| m.manifest.id.as_str()).collect();
        assert_eq!(ids, ["com.t.a", "com.t.root"], "dependency first, optional left out");
        assert!(!tmp.path().join("opt").exists());
        assert!(fake.asked().iter().all(|u| !u.contains("/o/opt/")));
        let src = read_source(&tmp.path().join("root")).expect(".source.toml reads back");
        assert_eq!(src, Source { repo: "o/root".into(), branch: branch.into(), sha: sha_of("root") });
        let listed = installed_in(tmp.path());
        let root_row = listed.iter().find(|m| m.id == "com.t.root").unwrap();
        assert_eq!(root_row.capabilities, ["speech"]);
        assert_eq!(root_row.optional_dependencies, ["com.t.opt"]);
        assert!(root_row.source.is_some(), "the module keeps its update source");

        // With the optional modules accepted, the optional one comes too.
        install_plan_into(&fake.github(), &plan, true, tmp.path()).expect("install");
        assert!(tmp.path().join("opt").join("module.toml").is_file());
    }

    #[test]
    fn a_dependency_is_written_before_every_module_that_needs_it() {
        // root -> a, b and b -> a: discovery finds a before b, and b needs a.
        let root = manifest("com.t.root", &[], &["com.t.a", "com.t.b"], &["com.t.opt"]);
        let a = manifest("com.t.a", &[], &[], &[]);
        let b = manifest("com.t.b", &[], &["com.t.a"], &[]);
        let opt = manifest("com.t.opt", &[], &["com.t.b"], &[]);
        let plan = InstallPlan {
            modules: vec![
                planned("o/root", "main", &root, false),
                planned("o/b", "main", &b, false),
                planned("o/a", "main", &a, false),
                planned("o/opt", "main", &opt, true),
            ],
            unavailable_optional: Vec::new(),
        };
        let ids = |with: bool| -> Vec<String> {
            install_order(&plan, with).iter().map(|p| p.manifest.id.clone()).collect()
        };
        assert_eq!(ids(false), ["com.t.a", "com.t.b", "com.t.root"]);
        assert_eq!(ids(true), ["com.t.a", "com.t.b", "com.t.opt", "com.t.root"]);
    }

    #[test]
    fn a_download_that_differs_from_the_review_is_not_left_installed() {
        let fake = Fake::default();
        let tmp = tempfile::tempdir().unwrap();
        let reviewed = manifest("com.t.root", &["speech"], &[], &[]);
        let served = manifest("com.t.root", &["speech", "input"], &[], &[]);
        fake.route(
            format!("{API}/repos/o/root/zipball/{}", sha_of("root")),
            zip_of(&[("o-root-abc/module.toml", served.as_bytes())]),
        );
        let plan = InstallPlan {
            modules: vec![planned("o/root", "main", &reviewed, false)],
            unavailable_optional: Vec::new(),
        };
        let err = install_plan_into(&fake.github(), &plan, false, tmp.path()).expect_err("differs");
        assert!(err.to_string().contains("capabilities"), "{err:#}");
        assert!(!tmp.path().join("root").exists());
        assert!(no_staging_left(tmp.path()));
    }

    /// Whether `dir` holds no staging folder, which every install removes however it ends.
    fn no_staging_left(dir: &Path) -> bool {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .all(|e| !e.file_name().to_string_lossy().starts_with(".staging-"))
    }

    #[test]
    fn a_failed_update_leaves_the_installed_version_in_place() {
        let fake = Fake::default();
        let gh = fake.github();
        let tmp = tempfile::tempdir().unwrap();
        let v1 = manifest("com.t.root", &["speech"], &[], &[]);
        let v2 = v1.replace("version = \"1.0.0\"", "version = \"2.0.0\"");
        let url = format!("{API}/repos/o/root/zipball/{}", sha_of("root"));
        let plan_for = |text: &str| InstallPlan {
            modules: vec![planned("o/root", "main", text, false)],
            unavailable_optional: Vec::new(),
        };
        // Version 1, with a file version 2 no longer has.
        fake.route(
            url.clone(),
            zip_of(&[
                ("o-root-1/module.toml", v1.as_bytes()),
                ("o-root-1/src/main.luau", b"return 1"),
                ("o-root-1/src/old.luau", b"return 'old'"),
            ]),
        );
        install_plan_into(&gh, &plan_for(&v1), false, tmp.path()).expect("version 1");
        let dir = tmp.path().join("root");
        let installed_v1 = || {
            assert_eq!(std::fs::read_to_string(dir.join("module.toml")).unwrap(), v1);
            assert_eq!(std::fs::read(dir.join("src/old.luau")).unwrap(), b"return 'old'");
            assert!(read_source(&dir).is_some(), "the update source is kept");
            assert!(no_staging_left(tmp.path()));
        };
        installed_v1();

        // Version 2 as reviewed, but GitHub serves a module.toml that asks for more.
        let served = v2.replace("[\"speech\"]", "[\"speech\", \"input\"]");
        fake.route(url.clone(), zip_of(&[("o-root-2/module.toml", served.as_bytes())]));
        let err = install_plan_into(&gh, &plan_for(&v2), false, tmp.path()).expect_err("differs");
        assert!(err.to_string().contains("Nothing in modules/root was changed"), "{err:#}");
        installed_v1();

        // Version 2 without a module.toml at all (an `export-ignore`, say).
        fake.route(url.clone(), zip_of(&[("o-root-2/src/main.luau", b"return 2")]));
        let err = install_plan_into(&gh, &plan_for(&v2), false, tmp.path()).expect_err("no manifest");
        assert!(format!("{err:#}").contains("not a valid module"), "{err:#}");
        installed_v1();

        // An archive that breaks a rule.
        fake.route(
            url.clone(),
            zip_of(&[("o-root-2/module.toml", v2.as_bytes()), ("o-root-2/aux.luau", b"")]),
        );
        install_plan_into(&gh, &plan_for(&v2), false, tmp.path()).expect_err("device name");
        installed_v1();

        // And the update that works replaces version 1 whole.
        fake.route(
            url.clone(),
            zip_of(&[("o-root-2/module.toml", v2.as_bytes()), ("o-root-2/src/main.luau", b"return 2")]),
        );
        let done = install_plan_into(&gh, &plan_for(&v2), false, tmp.path()).expect("version 2");
        assert_eq!(done[0].root, dir, "the loaded module points at its installed folder");
        assert_eq!(std::fs::read_to_string(dir.join("module.toml")).unwrap(), v2);
        assert!(!dir.join("src/old.luau").exists(), "nothing of version 1 is left behind");
        assert!(read_source(&dir).is_some());
        assert!(no_staging_left(tmp.path()));
        // A staging folder is never listed as a module.
        std::fs::create_dir_all(tmp.path().join(".staging-x")).unwrap();
        std::fs::write(tmp.path().join(".staging-x/module.toml"), v1.as_bytes()).unwrap();
        let listed: Vec<String> = installed_in(tmp.path()).into_iter().map(|m| m.id).collect();
        assert_eq!(listed, ["com.t.root"]);
    }

    #[test]
    fn a_branch_in_a_raw_path_is_encoded_and_updates_are_found() {
        let fake = Fake::default();
        let gh = fake.github();
        let text = "id = \"com.t.root\"\nname = \"R\"\nversion = \"1.0.0\"\n";
        let url = format!("{RAW}/o/root/fix/%2312%20x%25/module.toml");
        fake.route(url.clone(), text);
        assert_eq!(gh.fetch_manifest("o/root", "fix/#12 x%").expect("manifest").id, "com.t.root");
        assert_eq!(fake.asked(), vec![url]);
        // The update check of a module installed from that branch sees the new version.
        let mut row = installed_row("com.t.root", &[], &[]);
        row.source.as_mut().unwrap().branch = "fix/#12 x%".to_string();
        assert_eq!(gh.update_available(&row), Some("1.0.0".to_string()));
        // `..` never reaches a path.
        let before = fake.asked().len();
        assert!(gh.fetch_manifest("o/root", "a/../../x").is_err());
        assert!(gh.fetch_manifest("o/root", "").is_err());
        assert_eq!(fake.asked().len(), before);
    }

    #[test]
    fn dependencies_are_looked_up_one_search_page_at_a_time() {
        let fake = Fake::default();
        let gh = fake.github();
        let page = |n: usize| -> Vec<String> {
            (n * 100 - 100..n * 100).map(|i| format!("o/m{i}")).collect()
        };
        for n in 1..=3 {
            fake.route(search_url("topic%3Aosap-module", n), search_page(300, &page(n)));
        }
        for i in [2, 150] {
            fake.route(
                format!("{RAW}/o/m{i}/main/module.toml"),
                format!("id = \"com.t.m{i}\"\nname = \"M\"\nversion = \"1.0.0\"\n"),
            );
        }
        let searches = |asked: &[String]| -> Vec<String> {
            asked.iter().filter(|u| u.contains("/search/")).cloned().collect()
        };
        let mut index = TopicIndex::new();
        assert_eq!(index.find(&gh, "com.t.m2").unwrap(), Some(("o/m2".into(), "main".into())));
        // One page, and three manifests: m0 and m1 (unreadable here, so no candidates) and m2.
        assert_eq!(searches(&fake.asked()), vec![search_url("topic%3Aosap-module", 1)]);
        assert_eq!(fake.asked().len(), 4);
        // Found again without asking.
        index.find(&gh, "com.t.m2").unwrap();
        assert_eq!(fake.asked().len(), 4);
        // An id further down fetches the next page, and no more.
        assert_eq!(index.find(&gh, "com.t.m150").unwrap(), Some(("o/m150".into(), "main".into())));
        assert_eq!(
            searches(&fake.asked()),
            vec![search_url("topic%3Aosap-module", 1), search_url("topic%3Aosap-module", 2)]
        );
    }

    fn installed_row(id: &str, caps: &[&str], deps: &[&str]) -> InstalledModule {
        InstalledModule {
            id: id.to_string(),
            name: id.rsplit('.').next().unwrap().to_uppercase(),
            version: "0.9.0".to_string(),
            dir: PathBuf::from(id),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            optional_dependencies: Vec::new(),
            capabilities: caps.iter().map(|s| s.to_string()).collect(),
            source: Some(Source {
                repo: format!("o/{}", id.rsplit('.').next().unwrap()),
                branch: "main".to_string(),
                sha: "f".repeat(40),
            }),
            supported_os: Vec::new(),
        }
    }

    #[test]
    fn an_update_shows_what_it_adds_before_it_is_applied() {
        // The new root asks for `input` on top of `speech`, and starts depending on `b`
        // (installed) and `a` (not installed, which brings `c`).
        let world = World::new(&[
            ("root", manifest("com.t.root", &["speech", "input"], &["com.t.a", "com.t.b"], &[])),
            ("a", manifest("com.t.a", &["screen"], &["com.t.c"], &[])),
            ("c", manifest("com.t.c", &[], &[], &[])),
        ]);
        let installed = vec![
            installed_row("com.t.root", &["speech"], &[]),
            installed_row("com.t.b", &["log"], &[]),
        ];
        let plan =
            world.fake.github().resolve_update(&installed[0], &installed).expect("update plan");
        assert!(plan.needs_review());
        assert_eq!(plan.added_capabilities, ["input"]);
        assert_eq!(plan.added_installed_dependencies, [("com.t.b".to_string(), false)]);
        let ids: Vec<&str> = plan.install.modules.iter().map(|m| m.manifest.id.as_str()).collect();
        assert_eq!(ids, ["com.t.root", "com.t.a", "com.t.c"]);
        let text = update_review_text(&plan, &installed, "windows");
        for needle in [
            "from version 0.9.0 to version 1.0.0",
            "The new version also asks to:\n\u{2022} move and click the mouse",
            "B (com.t.b).\nIt may:\n\u{2022} write to the application\u{2019}s log (log)",
            "It needs 2 modules you do not have yet",
            "1. A, version 1.0.0 (com.t.a), from o/a \u{2014} needed by ROOT.",
        ] {
            assert!(text.contains(needle), "review lacks {needle:?}:\n{text}");
        }
    }

    #[test]
    fn an_update_that_asks_for_nothing_new_needs_no_review() {
        let world = World::new(&[("root", manifest("com.t.root", &["speech"], &["com.t.b"], &[]))]);
        let installed = vec![
            installed_row("com.t.root", &["speech", "log"], &["com.t.b"]),
            installed_row("com.t.b", &[], &[]),
        ];
        let plan =
            world.fake.github().resolve_update(&installed[0], &installed).expect("update plan");
        // Dropping a capability or keeping a dependency is nothing to agree to.
        assert!(!plan.needs_review(), "{plan:?}");
        assert_eq!(plan.install.modules.len(), 1);
    }

    #[test]
    fn source_toml_survives_any_branch_name() {
        let tmp = tempfile::tempdir().unwrap();
        for branch in ["main", "a\"b", "back\\slash", "it's", "feature/x", "tab\there"] {
            let s = Source { repo: "o/r".into(), branch: branch.into(), sha: "a".repeat(40) };
            write_source(tmp.path(), &s).unwrap();
            assert_eq!(read_source(tmp.path()), Some(s), "branch {branch:?}");
        }
    }

    #[test]
    fn every_capability_name_reads_as_a_sentence() {
        // Every name the gate knows has its own phrase — read from the gate's own table, so a
        // capability added there without a phrase here fails — and an unknown one says so.
        for (_, cap) in crate::GATED {
            let s = describe_capability(cap);
            assert!(s.ends_with(&format!("({cap})")) && !s.contains("does not know"), "{cap}: {s}");
        }
        assert!(describe_capability("screen.imagesearch").contains("does not know"));
    }
}
