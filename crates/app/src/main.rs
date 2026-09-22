//! Entry point. Without a subcommand, loads one or more module directories
//! (default `examples/hello`) and runs them under the manager. The management
//! subcommands discover/install/update modules from GitHub (registry).

// No console window. This is a tray application with a wxWidgets GUI; the black box that
// used to open behind it served nothing, since diagnostics go to a FILE and always have —
// deliberately, because a screen reader reads the focused terminal and console output would
// be spoken aloud on top of the overlay's own speech.
//
// Ignored on non-Windows targets, so this stays correct for the macOS build.
#![windows_subsystem = "windows"]

use std::io::Write;

use anyhow::Result;
use host::registry;

/// The commit this executable was compiled from: seven characters of it (more only where
/// seven would be ambiguous), with `-modified` after them when the working tree had
/// uncommitted changes at the time (a local build), and `unknown` where there was no git to
/// ask (a source archive). `host::build_info` puts it into the log header.
///
/// `--exclude=*` keeps it a bare commit even once the repository has tags, where
/// `git describe` would otherwise answer `v0.2.0-5-g6c95c8b`. A shallow clone, which is what
/// the Windows CI job checks out, answers the same as a full one.
///
/// Refreshed without a Rust change: the macro makes this crate depend on the reflog and the
/// index, so a commit, a checkout or a reset recompiles it. And since this crate depends on
/// `host`, any change there recompiles it too and asks again whether the tree is modified.
/// Only an edit outside the Rust (a module, a document) leaves the marker as it was, which
/// is right: the executable is the same either way.
///
/// Two ways it goes stale on a development machine, both cured by touching this file: when
/// git could not be asked at all, the macro records no dependency, so `unknown` stays until
/// this crate compiles again for another reason; and with a target folder shared between
/// checkouts (worktrees), the dependency points into the checkout that built it last, so
/// another checkout's build can reuse it and name that one's commit. CI builds from a fresh
/// checkout and is affected by neither.
const BUILT_FROM: &str = git_version::git_version!(
    args = ["--always", "--abbrev=7", "--dirty=-modified", "--exclude=*"],
    fallback = "unknown"
);

/// Re-attach to the terminal that launched us, if there was one.
///
/// The subsystem above means the process starts with no console at all — which is the point
/// for the tray app, and would silently break the management subcommands below, whose entire
/// output is `println!`. `install` even asks a question and reads the answer.
///
/// So: no console is ever CREATED, but one that already exists is joined. Double-clicked or
/// started from a script, nothing appears; typed into a terminal, the subcommands print
/// there as before. Rust's Windows stdio calls `GetStdHandle` on every write, so pointing
/// the handles at the attached console is enough — as long as it happens before the first
/// write, which is why this is the first thing `main` does.
#[cfg(windows)]
fn attach_parent_console() {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    const STREAMS: [(&str, u32); 3] = [
        ("CONOUT$", STD_OUTPUT_HANDLE),
        ("CONOUT$", STD_ERROR_HANDLE),
        ("CONIN$", STD_INPUT_HANDLE),
    ];

    unsafe {
        // Read the inherited handles FIRST. AttachConsole resets the standard handles to the
        // console's own, which silently discards a redirection the caller asked for: with
        // `> out.txt` the output went to the screen and the file stayed empty (measured —
        // this was written the other way round first, and the empty file is what caught it).
        let inherited: [_; 3] = std::array::from_fn(|i| GetStdHandle(STREAMS[i].1));

        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return; // launched without a terminal — nothing to attach to, and none is made
        }

        for (i, (name, id)) in STREAMS.iter().enumerate() {
            let pre = inherited[i];
            if !pre.is_null() && pre != INVALID_HANDLE_VALUE {
                SetStdHandle(*id, pre); // a pipe or a file: put back what the caller chose
                continue;
            }
            // Nothing was inherited, so this stream is ours to point at the console.
            // `CONIN$`/`CONOUT$` name the attached console, and a plain `File` opens them —
            // no Win32 file API needed. The `File` is deliberately LEAKED: the handle has to
            // outlive this function, being the process's stdout from here on.
            if let Ok(f) = std::fs::OpenOptions::new().read(true).write(true).open(name) {
                SetStdHandle(*id, f.as_raw_handle());
                std::mem::forget(f);
            }
        }
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}

/// Send panics to the log file.
///
/// Without a console there is nowhere else for one to go: the default hook writes to stderr,
/// which for a windowed process is discarded. A panic before the GUI is up would then be an
/// application that simply never appears — no window, no message, nothing to report — and
/// the person it happens to cannot see that a window failed to open.
fn log_panics() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // A panic the host catches and reports itself (the image worker's, which answers it
        // and carries on) is left to that report, which is throttled. Written here as well, a
        // template that panics on every poll would cost a line per poll for as long as it did.
        if host::logging::hold_contained_panic(|| info.to_string()) {
            return;
        }
        // Opens the log if run() has not got that far — except in a start that may still turn
        // out to be a second copy, which appends one line to the running copy's log instead
        // of starting a session in the middle of it (see logging::panic_line).
        host::logging::panic_line(&info.to_string());
        previous(info);
    }));
}

fn main() -> Result<()> {
    // First, so that no log header can be written without it: the panic hook below opens the
    // log too, and may do so before `run` does.
    host::build_info::set_binary_commit(BUILT_FROM);
    attach_parent_console();
    log_panics();
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("search") => cmd_search(args.get(1).map(String::as_str).unwrap_or("")),
        Some("install") => cmd_install(args.get(1).map(String::as_str)),
        Some("list") => {
            cmd_list();
            Ok(())
        }
        Some("update") => cmd_update(),
        Some("uninstall") => cmd_uninstall(args.get(1).map(String::as_str)),
        _ => {
            let dirs = if args.is_empty() {
                // No args: run every installed module (portable modules dir next
                // to the exe). Fall back to the dev example if nothing's installed.
                // (The repo keeps its own modules/ for real ones, examples/ for the
                // demos and tools/ for dev tools — see run-dev.ps1.)
                let installed: Vec<String> = registry::installed()
                    .into_iter()
                    .map(|m| m.dir.to_string_lossy().into_owned())
                    .collect();
                if installed.is_empty() {
                    // The demo, resolved against the application's own folder rather than
                    // the working directory — launched from a Finder or a shortcut, the
                    // working directory is somewhere unrelated (on macOS it is "/"), and a
                    // relative path there fails for a reason nobody could guess from the
                    // outside.
                    //
                    // And if it is not there either, that is not an error: start with
                    // nothing loaded. The manager runs perfectly well empty — that is how
                    // someone installs their first module — whereas exiting leaves a user
                    // with an application that did not appear and never said why.
                    let demo = host::app_dir().join("examples").join("hello");
                    if demo.is_dir() {
                        vec![demo.to_string_lossy().into_owned()]
                    } else {
                        Vec::new()
                    }
                } else {
                    installed
                }
            } else {
                args
            };
            host::run(&dirs)
        }
    }
}

fn cmd_search(query: &str) -> Result<()> {
    let results = registry::search(query)?;
    if results.is_empty() {
        println!(
            "No modules found (topic '{}'{}).",
            registry::MODULE_TOPIC,
            if query.is_empty() {
                String::new()
            } else {
                format!(", query '{query}'")
            }
        );
        return Ok(());
    }
    println!("Found {} module(s):", results.len());
    for r in results {
        println!("  {}  *{}  [{}]", r.full_name, r.stars, r.default_branch);
        if !r.description.is_empty() {
            println!("      {}", r.description);
        }
    }
    Ok(())
}

fn cmd_install(full_name: Option<&str>) -> Result<()> {
    let full_name = full_name.ok_or_else(|| anyhow::anyhow!("usage: install <owner/repo>"))?;

    // Every module the install would add, each with its capabilities, before anything is
    // downloaded — the same review the manager's Browse tab shows.
    let plan = registry::resolve_tree(full_name, None)?;
    println!("{}\n", registry::install_review_text(&plan, std::env::consts::OS));
    let deps = plan.modules.iter().filter(|m| !m.optional).count() > 1;
    if !ask(&format!("Install this module{}?", if deps { " and the modules it needs" } else { "" }))? {
        println!("Cancelled. Nothing was installed.");
        return Ok(());
    }
    // Optional modules are extra features: declining still installs the module and what it
    // needs.
    let with_optional = plan.has_optional() && ask("Also install the optional modules?")?;

    let installed = registry::install_resolved(&plan, with_optional)?;
    println!("Installed {} module(s) into {}:", installed.len(), registry::modules_dir().display());
    for m in &installed {
        println!("  {} v{} ({})", m.manifest.name, m.manifest.version, m.manifest.id);
    }
    Ok(())
}

/// Asks a yes/no question on the console; anything but "y" or "yes" is no.
fn ask(question: &str) -> Result<bool> {
    print!("{question} [y/N] ");
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn cmd_list() {
    let mods = registry::installed();
    if mods.is_empty() {
        println!("No installed modules in {}", registry::modules_dir().display());
        return;
    }
    println!("Installed modules:");
    for m in mods {
        let src = m
            .source
            .as_ref()
            .map(|s| format!(" <- {}", s.repo))
            .unwrap_or_default();
        println!("  {} v{} ({}){}", m.name, m.version, m.id, src);
    }
}

fn cmd_update() -> Result<()> {
    let mods = registry::installed();
    let mut updated = 0;
    for m in &mods {
        if let Some(new_version) = registry::update_available(m) {
            if let Some(src) = &m.source {
                println!("Update for {} ({}): v{} -> v{new_version}", m.id, src.repo, m.version);
                let plan = registry::resolve_update(m)?;
                // A new capability or a new dependency is shown and asked about; an update
                // that asks for nothing new goes ahead, as it does in the manager.
                if plan.needs_review() {
                    println!("{}\n", registry::update_review_text(&plan, &mods, std::env::consts::OS));
                    if !ask(&format!("Update {}?", m.id))? {
                        println!("Skipped {}.", m.id);
                        continue;
                    }
                }
                registry::install_update(&plan)?;
                updated += 1;
            }
        }
    }
    println!("{updated} module(s) updated.");
    Ok(())
}

fn cmd_uninstall(id: Option<&str>) -> Result<()> {
    let id = id.ok_or_else(|| anyhow::anyhow!("usage: uninstall <module-id>"))?;
    let installed = registry::installed();
    if !installed.iter().any(|m| m.id == id) {
        println!("No installed module with id '{id}'.");
        return Ok(());
    }
    let graph: Vec<(String, Vec<String>)> =
        installed.iter().map(|m| (m.id.clone(), m.dependencies.clone())).collect();

    // Block if another installed module transitively depends on it.
    let needed_by = registry::transitive_dependents(id, &graph);
    if !needed_by.is_empty() {
        println!("Can't uninstall '{id}': required by {}. Remove those first.", needed_by.join(", "));
        return Ok(());
    }

    registry::uninstall(id)?;
    println!("Uninstalled {id}.");

    // Offer to remove dependencies it pulled in that nothing else needs (cascades).
    let orphans = registry::orphaned_by(std::slice::from_ref(&id.to_string()), &graph);
    if !orphans.is_empty() {
        print!("Also remove now-unused dependencies ({})? [y/N] ", orphans.join(", "));
        std::io::stdout().flush().ok();
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            for oid in &orphans {
                if registry::uninstall(oid)? {
                    println!("Uninstalled {oid}.");
                }
            }
        }
    }
    Ok(())
}
