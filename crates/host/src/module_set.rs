//! The module set as a start reads it, read again by the reload key (`reload_everything`,
//! lib.rs).
//!
//! A start is given folders — named on the command line, or every module installed beside the
//! application — loads each, and before each the modules it depends on, found beside it. Its
//! manifests are read then, and everything derived from them was derived then: which modules are
//! loaded at all, and in which order. The reload key used to rebuild the modules that were
//! loaded, each from its own manifest read again but in the order the OLD manifests gave, and
//! nothing else: a dependency named since was never loaded, so the module that named it failed
//! with "code dependency … not in the module table" — after its old VM had been taken down, and
//! with a spoken report that said only its name.
//!
//! So a reload-all plans first and builds after. [`plan`] reads every manifest of the set as it
//! is on disk, follows each module's dependencies to the modules they name, and orders the
//! modules to build on that graph, dependencies first, which is what a start would do with the
//! same folders. What it cannot build it says, with why: a manifest that does not parse (which
//! entry, which line), a dependency that is not found, does not run on this system or is at the
//! wrong version, a module it needs that cannot be built itself, a cycle. Such a module is left
//! as it runs — not torn down for a reason a reload cannot fix, the rule a single module's
//! Reload already keeps for a broken manifest — and the next reload, after the fix, builds it.
//! It runs with the code of what it depended on when it was built, so that stays too: for a
//! module kept for its manifest, the modules it depended on are followed still, as the session
//! loaded them, and built where they are found — a typo in one module does not take the library
//! under it down — and whatever else a module left as it runs ran the code of is left as it runs
//! as well. A loaded module the set no longer holds otherwise — its folder gone, its manifest now
//! for another system, or nothing in the set naming it any more — is dropped, as a start would
//! not load it.
//!
//! Nothing here touches a VM: it reads folders and answers, so it is tested against folders.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use module_manifest::{dep_constraint, dep_id, LoadedModule, ModuleManifest, Problem};

/// Where a session's modules come from, and so where a reload-all reads them again.
#[derive(Debug, Clone)]
pub(crate) enum Sources {
    /// The folders named on the command line, each one module (or a `.zip` package), with those
    /// installed from the module window since.
    Folders(Vec<PathBuf>),
    /// Every module folder in `dir`, the modules installed beside the application — `demo` when
    /// it holds none. One installed from the module window is in `dir` already.
    Installed { dir: PathBuf, demo: PathBuf },
}

impl Sources {
    /// The installed modules, or the demo beside the application when none is installed: what a
    /// start without folders on its command line runs.
    pub(crate) fn installed() -> Self {
        Sources::Installed {
            dir: crate::registry::modules_dir(),
            demo: crate::app_dir().join("examples").join("hello"),
        }
    }

    /// A module installed from the module window: part of the set from now on.
    pub(crate) fn add(&mut self, dir: &Path) {
        if let Sources::Folders(folders) = self {
            if !folders.iter().any(|f| f == dir) {
                folders.push(dir.to_path_buf());
            }
        }
    }

    /// The folders a start loads. The installed ones are those whose manifest reads
    /// (`registry::installed_in`, which logs the others): at start a broken folder under
    /// `modules/` is skipped with its reason in the log, as module-package-format.md says.
    pub(crate) fn start_folders(&self) -> Vec<PathBuf> {
        match self {
            Sources::Folders(folders) => folders.clone(),
            Sources::Installed { dir, demo } => {
                let installed: Vec<PathBuf> =
                    crate::registry::installed_in(dir).into_iter().map(|m| m.dir).collect();
                or_demo(installed, demo)
            }
        }
    }

    /// The folders a reload-all reads: a start's, but every installed folder with a
    /// `module.toml`, whether it reads or not — one that does not is what the reload's report
    /// has to name. In the order the file system lists them, as at start: of two folders with
    /// one id the first is the module, and a reload takes the one the start took.
    fn folders(&self) -> Vec<PathBuf> {
        match self {
            Sources::Folders(folders) => folders.clone(),
            Sources::Installed { dir, demo } => {
                let found: Vec<PathBuf> =
                    subfolders(dir).into_iter().filter(|f| f.join("module.toml").is_file()).collect();
                or_demo(found, demo)
            }
        }
    }
}

/// `folders`, or the demo when there are none and it is there.
fn or_demo(folders: Vec<PathBuf>, demo: &Path) -> Vec<PathBuf> {
    if folders.is_empty() && demo.is_dir() {
        vec![demo.to_path_buf()]
    } else {
        folders
    }
}

/// The folders in `dir` that can hold a module, in the order the file system lists them: not a
/// dot-folder, which is an install's staging folder or a system's hidden one — the rule of the
/// installed list and of the start's search for a dependency (`find_module_dir`).
fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect()
}

/// A loaded module, as the plan needs to know it.
pub(crate) struct Loaded {
    pub id: String,
    pub name: String,
    /// The folder it was built from.
    pub root: PathBuf,
    /// What the session read it from: its folder, or the `.zip` package `root` was unpacked from.
    pub source: PathBuf,
    /// The ids it depended on at its last build, required and optional: the modules whose code it
    /// runs.
    pub deps: Vec<String>,
}

/// One module to build: its manifest as read now, and the folder its dependencies are looked for
/// beside — the folder of the path it was read from, as for a start.
pub(crate) struct Step {
    pub module: LoadedModule,
    pub parent: PathBuf,
    /// What it was read from: its folder, or its `.zip` package.
    pub source: PathBuf,
    /// The modules of the set it needs, by id: its required dependencies, and the optional ones
    /// that are there and run here. Each is built before it; one whose build fails is a reason
    /// not to build it (`reload_everything`).
    pub needs: Vec<String>,
}

/// A module of the set that is not built, and why. Loaded, it goes on running as it is; not
/// loaded, it stays unloaded.
#[derive(Debug)]
pub(crate) struct Kept {
    /// Its id: its manifest's, or the loaded module's when the manifest does not read.
    pub id: Option<String>,
    /// Its name: its manifest's, the loaded module's, or else its folder's.
    pub name: String,
    /// Where its own manifest goes wrong, when that is why.
    pub manifest: Option<Problem>,
    /// Why, as a clause after its name: "its module.toml is wrong at …", "it needs …".
    pub why: String,
}

/// What a reload-all does, decided before anything is torn down.
pub(crate) struct Plan {
    /// The modules to build, every one after the modules it depends on.
    pub build: Vec<Step>,
    /// Loaded modules the set no longer holds, by id: to drop.
    pub gone: Vec<String>,
    /// Modules of the set that are not built.
    pub kept: Vec<Kept>,
    /// Modules of the set whose manifest says they do not run on this system.
    pub excluded: Vec<LoadedModule>,
}

/// A module of the set while it is planned.
struct Node {
    module: LoadedModule,
    /// The folder its dependencies are looked for beside.
    parent: PathBuf,
    /// What it was read from.
    source: PathBuf,
    /// Whether it runs on this system.
    runs: bool,
    /// Why it cannot be built, once that is known.
    blocked: Option<String>,
}

/// A `module.toml` that does not load: where it goes wrong, and the id it names all the same when
/// a line of it does.
#[derive(Clone)]
struct Unread {
    problem: Problem,
    id: Option<String>,
}

/// Every folder the plan has read, read once.
#[derive(Default)]
struct Disk {
    /// A folder or package → its module, or what is wrong with its manifest.
    read: HashMap<PathBuf, Result<LoadedModule, Unread>>,
    /// A folder → its subfolders, as `subfolders` lists them.
    listed: HashMap<PathBuf, Vec<PathBuf>>,
}

impl Disk {
    fn read(&mut self, path: &Path) -> Result<LoadedModule, Unread> {
        self.read.entry(path.to_path_buf()).or_insert_with(|| read_module(path)).clone()
    }

    /// The folder of module `id` beside `near` — in it, then in a `modules` folder next to it —
    /// as the start's search finds it (`find_module_dir` in lib.rs). When there is none, the
    /// folders there whose manifest does not load and that may be it: those that name the id all
    /// the same, or no id at all. The start's search passes over them without a word.
    fn find(&mut self, near: &Path, id: &str) -> Result<PathBuf, Vec<PathBuf>> {
        let mut places = vec![near.to_path_buf()];
        if let Some(up) = near.parent() {
            let beside = up.join("modules");
            if beside != near {
                places.push(beside);
            }
        }
        let mut unread = Vec::new();
        for place in places {
            let folders = self.listed.entry(place.clone()).or_insert_with(|| subfolders(&place)).clone();
            for folder in folders {
                match self.read(&folder) {
                    Ok(m) if m.manifest.id == id => return Ok(folder),
                    Ok(_) => {}
                    Err(u) if u.id.as_deref().is_none_or(|named| named == id) => unread.push(folder),
                    Err(_) => {}
                }
            }
        }
        Err(unread)
    }
}

/// The module at `path` — an unpacked folder, or a `.zip` package — or what is wrong with its
/// manifest.
fn read_module(path: &Path) -> Result<LoadedModule, Unread> {
    let plain = |what: String| Unread { problem: Problem { entry: None, line: None, what }, id: None };
    if !path.is_dir() {
        return LoadedModule::load(path).map_err(|e| plain(one_line(&format!("{e:#}"))));
    }
    let text = std::fs::read_to_string(path.join("module.toml"))
        .map_err(|e| plain(format!("module.toml cannot be read: {e}")))?;
    match ModuleManifest::parse(&text) {
        Ok(manifest) => Ok(LoadedModule { root: path.to_path_buf(), manifest }),
        Err(e) => Err(Unread {
            problem: ModuleManifest::problem(&text)
                .unwrap_or_else(|| Problem { entry: None, line: None, what: one_line(&format!("{e:#}")) }),
            id: id_named(&text),
        }),
    }
}

/// The id a `module.toml` that does not load names all the same: its `id` when the file is TOML
/// at all, or else an `id = "…"` line above its first table. So a folder whose manifest has a
/// typo is still known as the module a dependency looks for, and the report names what is wrong
/// with it rather than "not found".
fn id_named(text: &str) -> Option<String> {
    if let Ok(table) = toml::from_str::<toml::Table>(text) {
        return table.get("id").and_then(|v| v.as_str()).map(str::to_string);
    }
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            break;
        }
        let Some(value) = line.strip_prefix("id").and_then(|r| r.trim_start().strip_prefix('=')).map(str::trim) else {
            continue;
        };
        for quote in ['"', '\''] {
            if let Some(id) = value.strip_prefix(quote).and_then(|r| r.find(quote).map(|end| &r[..end])) {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// `text` on one line.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The name a folder goes by when nothing better is known.
fn folder_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// The folder the dependencies of the module read from `path` are looked for beside, as for a
/// start.
fn parent_of(path: &Path) -> PathBuf {
    path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf()
}

/// Where a dependency is, as far as the plan can tell.
enum Found {
    /// In the plan.
    Planned,
    /// Its folder is there, but its manifest does not load: kept, and named.
    Unread(Problem),
    /// Not found; the folders looked in whose manifest does not load and that may be it.
    Missing(Vec<(PathBuf, Problem)>),
}

/// The plan while it is made.
struct Planner<'a> {
    loaded: &'a [Loaded],
    runs_here: &'a dyn Fn(&ModuleManifest) -> bool,
    disk: Disk,
    nodes: Vec<Node>,
    by_id: HashMap<String, usize>,
    kept: Vec<Kept>,
    /// The folders `kept` names for their manifest: each is named once.
    unread: HashSet<PathBuf>,
    /// For a loaded module kept for its manifest, the modules whose code it goes on running: the
    /// folder they are looked for beside, and their ids.
    follow: Vec<(PathBuf, Vec<String>)>,
}

impl<'a> Planner<'a> {
    /// The loaded module read from `path`: its folder, or the package it was unpacked from.
    fn session(&self, path: &Path) -> Option<&'a Loaded> {
        self.loaded.iter().find(|l| l.root == path || l.source == path)
    }

    fn add(&mut self, module: LoadedModule, read_from: &Path) {
        if self.by_id.contains_key(&module.manifest.id) {
            return; // the first folder with an id is the one, as for a start
        }
        self.by_id.insert(module.manifest.id.clone(), self.nodes.len());
        let runs = (self.runs_here)(&module.manifest);
        let (parent, source) = (parent_of(read_from), read_from.to_path_buf());
        self.nodes.push(Node { module, parent, source, runs, blocked: None });
    }

    /// The manifest at `path` does not load: named once, by what the session knows of its
    /// folder. A loaded module goes on running as it is, with the code of the modules it
    /// depended on, so those are followed still — built, or kept themselves — rather than
    /// dropped as needed by nothing.
    fn unreadable(&mut self, path: &Path, problem: &Problem) {
        if !self.unread.insert(path.to_path_buf()) {
            return;
        }
        let known = self.session(path);
        if let Some(l) = known {
            if self.kept.iter().any(|k| k.id.as_deref() == Some(l.id.as_str())) {
                return;
            }
            let deps = l.deps.iter().filter(|d| self.loaded.iter().any(|o| &o.id == *d)).cloned().collect();
            self.follow.push((parent_of(path), deps));
        }
        self.kept.push(Kept {
            id: known.map(|l| l.id.clone()),
            name: known.map_or_else(|| folder_name(path), |l| l.name.clone()),
            manifest: Some(problem.clone()),
            why: format!("its module.toml is wrong at {problem}"),
        });
    }

    /// Module `dep`, for a module whose dependencies are looked for beside `near`: found as a
    /// start finds it — beside the module, or in a `modules` folder next to that — or, last,
    /// where this session loaded it from, and planned.
    fn resolve(&mut self, near: &Path, dep: &str) -> Found {
        if self.by_id.contains_key(dep) {
            return Found::Planned;
        }
        let maybe = match self.disk.find(near, dep) {
            Ok(path) => {
                if let Ok(module) = self.disk.read(&path) {
                    self.add(module, &path);
                }
                return Found::Planned;
            }
            Err(maybe) => maybe,
        };
        if let Some(at) = self.loaded.iter().find(|l| l.id == dep) {
            match self.disk.read(&at.root) {
                Ok(module) if module.manifest.id == dep => {
                    self.add(module, &at.root);
                    return Found::Planned;
                }
                // A module this session loaded whose manifest no longer reads.
                Err(unread) if at.root.exists() => {
                    self.unreadable(&at.root, &unread.problem);
                    return Found::Unread(unread.problem);
                }
                _ => {}
            }
        }
        // A folder there whose manifest does not load but names the id: the module, with a typo.
        let mut others = Vec::new();
        for path in maybe {
            let Err(unread) = self.disk.read(&path) else { continue };
            if unread.id.as_deref() == Some(dep) {
                self.unreadable(&path, &unread.problem);
                return Found::Unread(unread.problem);
            }
            others.push((path, unread.problem));
        }
        Found::Missing(others)
    }

    /// The dependencies of node `i`, planned; a required one that cannot be is why it is not
    /// built. Not followed from a module that does not run here: a start loads neither it nor
    /// what it needs.
    fn follow_node(&mut self, i: usize) {
        if !self.nodes[i].runs {
            return;
        }
        let near = self.nodes[i].parent.clone();
        let manifest = &self.nodes[i].module.manifest;
        let specs: Vec<(String, bool)> = manifest
            .dependencies
            .iter()
            .map(|s| (dep_id(s).to_string(), true))
            .chain(manifest.optional_dependencies.iter().map(|s| (dep_id(s).to_string(), false)))
            .collect();
        for (dep, required) in specs {
            // Every one looked for, an optional one too: a manifest that does not load is named
            // whoever needs it.
            let found = self.resolve(&near, &dep);
            if !required || self.nodes[i].blocked.is_some() {
                continue;
            }
            self.nodes[i].blocked = match found {
                Found::Planned => None,
                Found::Unread(p) => Some(format!("it needs '{dep}', whose module.toml is wrong at {p}")),
                Found::Missing(maybe) => {
                    let mut why =
                        format!("it needs '{dep}', which is not found beside it or in a modules folder next to that");
                    if !maybe.is_empty() {
                        let names: Vec<String> =
                            maybe.iter().map(|(path, _)| format!("\u{201c}{}\u{201d}", folder_name(path))).collect();
                        why.push_str(&format!(
                            "; it may be {} there, whose module.toml does not load",
                            names.join(" or ")
                        ));
                    }
                    for (path, problem) in &maybe {
                        self.unreadable(path, problem);
                    }
                    Some(why)
                }
            };
        }
    }
}

/// Plans a reload-all of the modules `sources` holds: `loaded` is what runs now, in its build
/// order; `runs_here` the rule for a manifest's `supported_os` (lib.rs, `runs_here`); `os` this
/// system's name, as a reason says it.
pub(crate) fn plan(
    sources: &Sources,
    loaded: &[Loaded],
    runs_here: &dyn Fn(&ModuleManifest) -> bool,
    os: &str,
) -> Plan {
    let mut planner = Planner {
        loaded,
        runs_here,
        disk: Disk::default(),
        nodes: Vec::new(),
        by_id: HashMap::new(),
        kept: Vec::new(),
        unread: HashSet::new(),
        follow: Vec::new(),
    };

    // The set's own folders first, every one, so that a dependency one of them names is the
    // folder the set holds rather than a copy found beside another module.
    for path in sources.folders() {
        if !path.exists() {
            continue; // gone, and its module with it
        }
        match planner.disk.read(&path) {
            Ok(module) => planner.add(module, &path),
            Err(unread) => planner.unreadable(&path, &unread.problem),
        }
    }

    // Then what they depend on, as a start finds it, and what a module kept for its manifest
    // went on running the code of, until nothing new turns up.
    let mut i = 0;
    loop {
        if i < planner.nodes.len() {
            planner.follow_node(i);
            i += 1;
        } else if let Some((near, deps)) = planner.follow.pop() {
            for dep in deps {
                planner.resolve(&near, &dep);
            }
        } else {
            break;
        }
    }
    let Planner { mut nodes, by_id, mut kept, .. } = planner;

    // What each module needs in the set: its required dependencies, and the optional ones that
    // are there and run here.
    let edges: Vec<Vec<usize>> = nodes
        .iter()
        .map(|n| {
            let m = &n.module.manifest;
            let required = m.dependencies.iter().filter_map(|s| by_id.get(dep_id(s)).copied());
            let optional = m.optional_dependencies.iter().filter_map(|s| by_id.get(dep_id(s)).copied());
            required.chain(optional.filter(|&d| nodes[d].runs)).collect()
        })
        .collect();
    // A required dependency that does not run here stops a module whose build has to evaluate
    // its code (a data dependency that is not there only answers nil), and so does one at a
    // version the module does not accept.
    for idx in 0..nodes.len() {
        if !nodes[idx].runs || nodes[idx].blocked.is_some() {
            continue;
        }
        let m = &nodes[idx].module.manifest;
        let mut why = None;
        for spec in &m.dependencies {
            let Some(&d) = by_id.get(dep_id(spec)) else { continue };
            let dep = &nodes[d].module.manifest;
            if !nodes[d].runs && dep.code_module {
                why = Some(format!("it needs '{}', which says it does not run on {os}", dep.id));
                break;
            }
            if let Some(req) = dep_constraint(spec) {
                if let Err(e) = crate::check_dep_version(&m.id, &dep.id, req, &dep.version) {
                    why = Some(format!("{e}"));
                    break;
                }
            }
        }
        nodes[idx].blocked = why;
    }
    // And a module that needs one that cannot be built cannot be built either.
    loop {
        let mut changed = false;
        for idx in 0..nodes.len() {
            if !nodes[idx].runs || nodes[idx].blocked.is_some() {
                continue;
            }
            if let Some(&d) = edges[idx].iter().find(|&&d| nodes[d].runs && nodes[d].blocked.is_some()) {
                let dep = nodes[d].module.manifest.id.clone();
                nodes[idx].blocked = Some(format!("it depends on '{dep}', which is not built either"));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // The order: dependencies first. Among the modules ready at a step, the one loaded first goes
    // first, and a new one after every loaded one, in the order it was found — so a reload that
    // changes nothing builds in the order the session built.
    let rank = |idx: usize| {
        let id = &nodes[idx].module.manifest.id;
        loaded.iter().position(|l| &l.id == id).unwrap_or(loaded.len() + idx)
    };
    let buildable: HashSet<usize> =
        (0..nodes.len()).filter(|&i| nodes[i].runs && nodes[i].blocked.is_none()).collect();
    let mut placed: HashSet<usize> = HashSet::new();
    let mut order: Vec<usize> = Vec::new();
    while let Some(next) = buildable
        .iter()
        .copied()
        .filter(|i| !placed.contains(i))
        .filter(|&i| edges[i].iter().all(|d| !buildable.contains(d) || placed.contains(d)))
        .min_by_key(|&i| rank(i))
    {
        placed.insert(next);
        order.push(next);
    }
    // What is left waits on itself: a cycle, or a module that needs one.
    let mut circle: Vec<usize> = buildable.iter().copied().filter(|i| !placed.contains(i)).collect();
    circle.sort_by_key(|&i| rank(i));
    let ids: Vec<String> = circle.iter().map(|&i| nodes[i].module.manifest.id.clone()).collect();
    for &i in &circle {
        nodes[i].blocked = Some(format!(
            "its dependencies go round in a circle, or it needs a module whose dependencies do: {}",
            ids.join(", ")
        ));
    }

    let build: Vec<Step> = order
        .iter()
        .map(|&i| Step {
            module: nodes[i].module.clone(),
            parent: nodes[i].parent.clone(),
            source: nodes[i].source.clone(),
            needs: edges[i].iter().map(|&d| nodes[d].module.manifest.id.clone()).collect(),
        })
        .collect();
    let mut excluded = Vec::new();
    for n in nodes.into_iter().filter(|n| !n.runs || n.blocked.is_some()) {
        match n.blocked {
            Some(why) if n.runs => kept.push(Kept {
                id: Some(n.module.manifest.id.clone()),
                name: n.module.manifest.name.clone(),
                manifest: None,
                why,
            }),
            _ => excluded.push(n.module),
        }
    }
    // A loaded module left as it runs goes on running the code of the modules it depended on when
    // it was built, copied into its VM, and so does each of those: they stay as they run too, even
    // when nothing in the set names them now or their folder is gone, until a reload builds what
    // needs them. (One kept for its manifest had them followed above, and built where they are
    // found; this is what is left: a module that needs one no longer found, or no longer names
    // one it ran.) Not one now for another system: that is dropped, as a start would not load it.
    let mut staying: HashSet<String> = build
        .iter()
        .map(|s| s.module.manifest.id.clone())
        .chain(kept.iter().filter_map(|k| k.id.clone()))
        .collect();
    let elsewhere: HashSet<&str> = excluded.iter().map(|m| m.manifest.id.as_str()).collect();
    let mut queue: Vec<String> = kept.iter().filter_map(|k| k.id.clone()).collect();
    while let Some(id) = queue.pop() {
        let Some(l) = loaded.iter().find(|l| l.id == id) else { continue };
        for d in l.deps.iter().filter_map(|dep| loaded.iter().find(|o| &o.id == dep)) {
            if !elsewhere.contains(d.id.as_str()) && staying.insert(d.id.clone()) {
                kept.push(Kept {
                    id: Some(d.id.clone()),
                    name: d.name.clone(),
                    manifest: None,
                    why: format!("'{}' runs its code and is not rebuilt either", l.id),
                });
                queue.push(d.id.clone());
            }
        }
    }
    let gone = loaded.iter().filter(|l| !staying.contains(&l.id)).map(|l| l.id.clone()).collect();
    Plan { build, gone, kept, excluded }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A module in the folder `parent/folder`: its id, its dependencies, required and optional, as
    /// spec strings, and the system it claims.
    fn module(parent: &Path, folder: &str, id: &str, deps: &[&str], optional: &[&str], os: &str) -> PathBuf {
        let dir = parent.join(folder);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let list = |v: &[&str]| v.iter().map(|d| format!("{d:?}")).collect::<Vec<_>>().join(", ");
        std::fs::write(
            dir.join("module.toml"),
            format!(
                "id = \"{id}\"\nname = \"{}\"\nversion = \"1.0.0\"\ncode_module = true\n\
                 dependencies = [{}]\noptional_dependencies = [{}]\nsupported_os = [\"{os}\"]\n",
                folder.to_uppercase(),
                list(deps),
                list(optional)
            ),
        )
        .unwrap();
        std::fs::write(dir.join("src").join("main.luau"), "return {}\n").unwrap();
        dir
    }

    /// What runs now: each folder's module, as it was built.
    fn loaded(dirs: &[&PathBuf]) -> Vec<Loaded> {
        dirs.iter()
            .map(|d| {
                let m = LoadedModule::load_dir(d).unwrap();
                let deps = m.manifest.dependencies.iter().chain(&m.manifest.optional_dependencies);
                let deps = deps.map(|s| dep_id(s).to_string()).collect();
                Loaded { id: m.manifest.id, name: m.manifest.name, root: d.to_path_buf(), source: d.to_path_buf(), deps }
            })
            .collect()
    }

    fn plan_of(sources: &Sources, loaded: &[Loaded]) -> Plan {
        plan(sources, loaded, &|m: &ModuleManifest| m.runs_on("here"), "here")
    }

    fn built(p: &Plan) -> Vec<&str> {
        p.build.iter().map(|s| s.module.manifest.id.as_str()).collect()
    }

    fn kept_ids(p: &Plan) -> Vec<&str> {
        p.kept.iter().map(|k| k.id.as_deref().unwrap_or("?")).collect()
    }

    /// A module that names a dependency since the start is built after it, on the graph the
    /// manifests give now — loaded in the other order, it would copy the old code of what it now
    /// depends on — and a dependency at a version it no longer accepts keeps it as it runs.
    #[test]
    fn a_changed_dependency_is_built_after_what_it_names_now() {
        let tmp = tempfile::tempdir().unwrap();
        let a = module(tmp.path(), "a", "com.t.a", &[], &[], "here");
        let b = module(tmp.path(), "b", "com.t.b", &[], &[], "here");
        let c = module(tmp.path(), "c", "com.t.c", &[], &[], "here");
        let now = loaded(&[&a, &b, &c]);
        let sources = Sources::Folders(vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(built(&plan_of(&sources, &now)), ["com.t.a", "com.t.b", "com.t.c"], "nothing changed, the session's order");

        module(tmp.path(), "a", "com.t.a", &["com.t.c"], &[], "here");
        module(tmp.path(), "b", "com.t.b", &[], &["com.t.a"], "here");
        let p = plan_of(&sources, &now);
        assert_eq!(built(&p), ["com.t.c", "com.t.a", "com.t.b"], "C before A, A before B");
        assert!(p.gone.is_empty() && p.kept.is_empty() && p.excluded.is_empty());
        // What each step needs, which a failed build of it holds back (`reload_everything`).
        let needs: Vec<&[String]> = p.build.iter().map(|s| s.needs.as_slice()).collect();
        assert_eq!(needs, [&[][..], &["com.t.c".to_string()][..], &["com.t.a".to_string()][..]]);

        module(tmp.path(), "a", "com.t.a", &["com.t.c >= 2"], &[], "here");
        let p = plan_of(&sources, &now);
        assert_eq!(built(&p), ["com.t.c"], "A stays as it runs, and B, which uses it, with it");
        assert_eq!(kept_ids(&p), ["com.t.a", "com.t.b"]);
        assert!(p.kept[0].why.contains("requires 'com.t.c' >= 2, but the loaded version is v1.0.0"), "{}", p.kept[0].why);
        assert_eq!(p.kept[1].why, "it depends on 'com.t.a', which is not built either");
        assert!(p.gone.is_empty(), "nothing kept is dropped");
    }

    /// A module folder that is new to the session joins it: a dependency named since, found beside
    /// the module that names it, before that module — a required one and an optional one — and
    /// a folder added to the installed modules. One that does not run here is excluded, and a
    /// code module that requires it is kept.
    #[test]
    fn a_new_module_folder_is_loaded() {
        let tmp = tempfile::tempdir().unwrap();
        let a = module(tmp.path(), "a", "com.t.a", &[], &[], "here");
        let now = loaded(&[&a]);
        let sources = Sources::Folders(vec![a.clone()]);
        module(tmp.path(), "lib", "com.t.lib", &[], &[], "here");
        module(tmp.path(), "extra", "com.t.extra", &[], &[], "here");
        module(tmp.path(), "a", "com.t.a", &["com.t.lib"], &["com.t.extra", "com.t.absent"], "here");
        let p = plan_of(&sources, &now);
        assert_eq!(built(&p), ["com.t.lib", "com.t.extra", "com.t.a"]);
        assert_eq!(p.build[0].parent, tmp.path(), "looked for beside it, as a start does");

        // The installed modules: a folder copied in since the start is read with the rest.
        let installed = tmp.path().join("installed");
        let one = module(&installed, "one", "com.i.one", &[], &[], "here");
        let sources = Sources::Installed { dir: installed.clone(), demo: tmp.path().join("no-demo") };
        let now = loaded(&[&one]);
        module(&installed, "two", "com.i.two", &[], &[], "here");
        assert_eq!(built(&plan_of(&sources, &now)), ["com.i.one", "com.i.two"]);
        // A staging folder of an install is not a module.
        std::fs::create_dir_all(installed.join(".two.partial")).unwrap();
        std::fs::copy(installed.join("two").join("module.toml"), installed.join(".two.partial").join("module.toml")).unwrap();
        assert_eq!(built(&plan_of(&sources, &now)), ["com.i.one", "com.i.two"]);

        // One for another system: not built, listed; the code module that requires it is kept.
        module(&installed, "three", "com.i.three", &[], &[], "elsewhere");
        module(&installed, "two", "com.i.two", &["com.i.three"], &[], "here");
        let p = plan_of(&sources, &now);
        assert_eq!(built(&p), ["com.i.one"]);
        assert_eq!(p.excluded.iter().map(|m| m.manifest.id.as_str()).collect::<Vec<_>>(), ["com.i.three"]);
        assert_eq!(kept_ids(&p), ["com.i.two"]);
        assert_eq!(p.kept[0].why, "it needs 'com.i.three', which says it does not run on here");
    }

    /// A module whose folder is gone is dropped, and so is a dependency nothing names any more;
    /// a module that still requires one that is gone is kept as it runs, with the reason, and the
    /// one it requires with it: it runs that one's code.
    #[test]
    fn a_removed_module_is_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = module(tmp.path(), "lib", "com.t.lib", &[], &[], "here");
        let base = module(tmp.path(), "base", "com.t.base", &[], &[], "here");
        let a = module(tmp.path(), "a", "com.t.a", &["com.t.lib"], &[], "here");
        let b = module(tmp.path(), "b", "com.t.b", &["com.t.base"], &[], "here");
        let c = module(tmp.path(), "c", "com.t.c", &[], &[], "here");
        // `lib` and `base` were loaded as dependencies, not named on the command line.
        let now = loaded(&[&lib, &a, &base, &b, &c]);
        let sources = Sources::Folders(vec![a.clone(), b.clone(), c.clone()]);
        assert!(plan_of(&sources, &now).gone.is_empty());

        std::fs::remove_dir_all(&c).unwrap();
        module(tmp.path(), "a", "com.t.a", &[], &[], "here");
        std::fs::remove_dir_all(&base).unwrap();
        let p = plan_of(&sources, &now);
        assert_eq!(p.gone, ["com.t.lib", "com.t.c"], "in the session's order");
        assert_eq!(built(&p), ["com.t.a"]);
        assert_eq!(kept_ids(&p), ["com.t.b", "com.t.base"], "B still requires what is gone, and runs its code");
        assert_eq!(p.kept[0].why, "it needs 'com.t.base', which is not found beside it or in a modules folder next to that");
        assert_eq!(p.kept[1].why, "'com.t.b' runs its code and is not rebuilt either");

        // Once B no longer needs it, it goes.
        module(tmp.path(), "b", "com.t.b", &[], &[], "here");
        let p = plan_of(&sources, &now);
        assert_eq!(p.gone, ["com.t.lib", "com.t.base", "com.t.c"]);
        assert!(p.kept.is_empty());
    }

    /// A manifest that does not read is named — the module, the entry, the line — and kept as it
    /// runs, with every module that needs it; the rest is built.
    #[test]
    fn a_broken_manifest_names_its_module_and_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = module(tmp.path(), "lib", "com.t.lib", &[], &[], "here");
        let a = module(tmp.path(), "a", "com.t.a", &["com.t.lib"], &[], "here");
        let b = module(tmp.path(), "b", "com.t.b", &[], &[], "here");
        let now = loaded(&[&lib, &a, &b]);
        let manifest = lib.join("module.toml");
        let text = std::fs::read_to_string(&manifest).unwrap();
        std::fs::write(&manifest, text.replacen("dependencies = []", "dependencies = \"com.t.b\"", 1)).unwrap();
        for sources in [Sources::Folders(vec![lib.clone(), a.clone(), b.clone()]), Sources::Folders(vec![a.clone(), b.clone()])] {
            let p = plan_of(&sources, &now);
            assert_eq!(built(&p), ["com.t.b"]);
            assert!(p.gone.is_empty(), "nothing that runs is dropped for a typo");
            assert_eq!(kept_ids(&p), ["com.t.lib", "com.t.a"]);
            let wrong = p.kept[0].manifest.as_ref().expect("its manifest is why");
            assert_eq!(wrong.entry.as_deref(), Some("dependencies"));
            assert_eq!(wrong.line, Some(5));
            assert_eq!(p.kept[0].name, "LIB", "named as the session knows it");
            assert_eq!(
                p.kept[0].why,
                "its module.toml is wrong at `dependencies`, line 5: invalid type: string \"com.t.b\", expected a sequence"
            );
            assert!(p.kept[1].why.starts_with("it needs 'com.t.lib', "), "{}", p.kept[1].why);
        }
        // A new folder whose manifest does not read is named by its folder.
        let fresh = tmp.path().join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        std::fs::write(fresh.join("module.toml"), "id = \"com.t.fresh\"\nname = \"Fresh\"\n").unwrap();
        let p = plan_of(&Sources::Folders(vec![b.clone(), fresh.clone()]), &loaded(&[&b]));
        assert_eq!(built(&p), ["com.t.b"]);
        assert_eq!((p.kept[0].id.as_deref(), p.kept[0].name.as_str()), (None, "fresh"));
        assert_eq!(p.kept[0].manifest.as_ref().and_then(|m| m.entry.as_deref()), Some("version"));
    }

    /// Modules whose dependencies go round in a circle are kept, with the modules in it named.
    #[test]
    fn a_cycle_is_kept_and_named() {
        let tmp = tempfile::tempdir().unwrap();
        let a = module(tmp.path(), "a", "com.t.a", &[], &[], "here");
        let b = module(tmp.path(), "b", "com.t.b", &[], &[], "here");
        let now = loaded(&[&a, &b]);
        module(tmp.path(), "a", "com.t.a", &["com.t.b"], &[], "here");
        module(tmp.path(), "b", "com.t.b", &["com.t.a"], &[], "here");
        let p = plan_of(&Sources::Folders(vec![a, b]), &now);
        assert!(p.build.is_empty() && p.gone.is_empty());
        assert_eq!(kept_ids(&p), ["com.t.a", "com.t.b"]);
        assert!(p.kept[0].why.ends_with(": com.t.a, com.t.b"), "{}", p.kept[0].why);
    }

    /// A loaded module kept for a typo in its manifest goes on running with the code of what it
    /// depended on, so that is not dropped as needed by nothing: built, as a library it reached
    /// only through it, the library's own dependency too. A loaded dependency whose manifest no
    /// longer reads is kept and named whoever needs it — reached only as an optional dependency,
    /// or after another of the module's dependencies already kept it — and a package given on
    /// the command line is known by the package, not by the folder it was unpacked into.
    #[test]
    fn a_typo_does_not_drop_what_runs() {
        let tmp = tempfile::tempdir().unwrap();
        let base = module(tmp.path(), "base", "com.t.base", &[], &[], "here");
        let lib = module(tmp.path(), "lib", "com.t.lib", &["com.t.base"], &[], "here");
        let a = module(tmp.path(), "a", "com.t.a", &["com.t.lib"], &[], "here");
        let now = loaded(&[&base, &lib, &a]);
        let sources = Sources::Folders(vec![a.clone()]);
        std::fs::write(a.join("module.toml"), "id = \"com.t.a\"\nname = \"A\"\n").unwrap();
        let p = plan_of(&sources, &now);
        assert!(p.gone.is_empty(), "the library under it stays: {:?}", p.gone);
        assert_eq!(built(&p), ["com.t.base", "com.t.lib"]);
        assert_eq!(kept_ids(&p), ["com.t.a"]);

        // Reached only as an optional dependency: kept, and named with its entry and line.
        let x = module(tmp.path(), "x", "com.t.x", &[], &[], "here");
        let c = module(tmp.path(), "c", "com.t.c", &[], &["com.t.x"], "here");
        let now = loaded(&[&x, &c]);
        let text = std::fs::read_to_string(x.join("module.toml")).unwrap();
        std::fs::write(x.join("module.toml"), text.replacen("dependencies = []", "dependencies = 1", 1)).unwrap();
        let p = plan_of(&Sources::Folders(vec![c.clone()]), &now);
        assert!(p.gone.is_empty(), "{:?}", p.gone);
        assert_eq!(built(&p), ["com.t.c"], "an optional dependency holds nothing back");
        assert_eq!(kept_ids(&p), ["com.t.x"]);
        assert_eq!(p.kept[0].manifest.as_ref().map(|m| (m.entry.as_deref(), m.line)), Some((Some("dependencies"), Some(5))));

        // Two required ones that no longer read: both named, not only the first.
        module(tmp.path(), "x", "com.t.x", &[], &[], "here");
        let y = module(tmp.path(), "y", "com.t.y", &[], &[], "here");
        let d = module(tmp.path(), "d", "com.t.d", &["com.t.x", "com.t.y"], &[], "here");
        let now = loaded(&[&x, &y, &d]);
        std::fs::write(x.join("module.toml"), "id = \"com.t.x\"\n").unwrap();
        std::fs::write(y.join("module.toml"), "id = \"com.t.y\"\n").unwrap();
        let p = plan_of(&Sources::Folders(vec![d.clone()]), &now);
        assert!(p.gone.is_empty(), "{:?}", p.gone);
        assert_eq!(kept_ids(&p), ["com.t.x", "com.t.y", "com.t.d"]);

        // A package: what the session read it from, while it runs from the folder it was
        // unpacked into.
        let pkg = tmp.path().join("pkg.zip");
        std::fs::write(&pkg, b"not a package").unwrap();
        let unpacked = module(&tmp.path().join("cache"), "pkg", "com.t.pkg", &[], &[], "here");
        let now = vec![Loaded {
            id: "com.t.pkg".into(),
            name: "Package".into(),
            root: unpacked,
            source: pkg.clone(),
            deps: Vec::new(),
        }];
        let p = plan_of(&Sources::Folders(vec![pkg]), &now);
        assert!(p.gone.is_empty(), "{:?}", p.gone);
        assert_eq!((kept_ids(&p), p.kept[0].name.as_str()), (vec!["com.t.pkg"], "Package"));
    }

    /// A dependency named since whose folder is there, but whose module.toml does not load, is
    /// named with what is wrong in it — not "not found" — whether the file is TOML or not; and a
    /// folder whose manifest names no id at all is said to be what it may be.
    #[test]
    fn a_new_dependency_with_a_typo_is_named() {
        let tmp = tempfile::tempdir().unwrap();
        let a = module(tmp.path(), "a", "com.t.a", &[], &[], "here");
        let now = loaded(&[&a]);
        let sources = Sources::Folders(vec![a.clone()]);
        module(tmp.path(), "a", "com.t.a", &["com.t.newlib"], &[], "here");
        let newlib = tmp.path().join("newlib");
        std::fs::create_dir_all(&newlib).unwrap();
        std::fs::write(newlib.join("module.toml"), "id = \"com.t.newlib\"\nname = \"New\"\n").unwrap();
        let p = plan_of(&sources, &now);
        assert!(p.build.is_empty() && p.gone.is_empty());
        assert_eq!((p.kept[0].id.as_deref(), p.kept[0].name.as_str()), (None, "newlib"));
        assert_eq!(p.kept[0].manifest.as_ref().and_then(|m| m.entry.as_deref()), Some("version"));
        assert_eq!(p.kept[1].why, "it needs 'com.t.newlib', whose module.toml is wrong at `version`: missing field `version`");

        // Not TOML at all: the id line above the first table still says whose it is.
        std::fs::write(newlib.join("module.toml"), "id = \"com.t.newlib\"\nname = \"New\nversion = \"1.0.0\"\n").unwrap();
        let p = plan_of(&sources, &now);
        assert_eq!(kept_ids(&p), ["?", "com.t.a"]);
        assert!(p.kept[1].why.starts_with("it needs 'com.t.newlib', whose module.toml is wrong at "), "{}", p.kept[1].why);
        assert_eq!(p.kept[0].manifest.as_ref().and_then(|m| m.line), Some(2));

        // No id to go by: not found, and the folder that may be it named.
        std::fs::write(newlib.join("module.toml"), "name = \"New\n").unwrap();
        let p = plan_of(&sources, &now);
        assert_eq!(kept_ids(&p), ["?", "com.t.a"]);
        assert_eq!(
            p.kept[1].why,
            "it needs 'com.t.newlib', which is not found beside it or in a modules folder next to that; it may be \
             \u{201c}newlib\u{201d} there, whose module.toml does not load"
        );
        // One that names another id is somebody else's typo, and not this module's business.
        std::fs::write(newlib.join("module.toml"), "id = \"com.t.other\"\nname = \"Other\"\n").unwrap();
        let p = plan_of(&sources, &now);
        assert_eq!(kept_ids(&p), ["com.t.a"]);
        assert!(p.kept[0].why.ends_with("in a modules folder next to that"), "{}", p.kept[0].why);
    }

    /// The id of a manifest that does not load, as far as a line of it says.
    #[test]
    fn the_id_of_a_manifest_that_does_not_load() {
        assert_eq!(id_named("id = \"com.t.a\"\nname = 3\n").as_deref(), Some("com.t.a"));
        assert_eq!(id_named("name = \"x\n  id   =   'com.t.b'  \n").as_deref(), Some("com.t.b"));
        assert_eq!(id_named("[limits\nid = \"com.t.c\"\n"), None, "below a table it is not the module's");
        assert_eq!(id_named("identity = \"com.t.d\"\nname = \"x\n"), None);
        assert_eq!(id_named("id = 4\n"), None);
    }
}
