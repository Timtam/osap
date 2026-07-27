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
        // Best effort: if run() has not opened the log yet, `line` is a no-op, so make sure
        // there is a file to write to.
        host::logging::init();
        host::logging::line("panic", &info.to_string());
        previous(info);
    }));
}

fn main() -> Result<()> {
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
                    vec!["examples/hello".to_string()]
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

    // Review the requested capabilities before installing (the GUI gets a proper
    // dialog in a later phase; this is the console equivalent).
    let branch = registry::default_branch(full_name)?;
    let manifest = registry::fetch_manifest(full_name, &branch)?;
    println!("Module: {} v{} ({})", manifest.name, manifest.version, manifest.id);
    let caps = &manifest.capabilities.require;
    println!(
        "Requested capabilities: {}",
        if caps.is_empty() { "(none)".to_string() } else { caps.join(", ") }
    );
    if !manifest.dependencies.is_empty() {
        println!("Dependencies (fetched too if missing): {}", manifest.dependencies.join(", "));
    }
    // Optional dependencies are extra features, not required — listed + offered separately.
    let optional_ids: Vec<String> = manifest
        .optional_dependencies
        .iter()
        .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if !optional_ids.is_empty() {
        println!("Optional dependencies (extra features, not required): {}", optional_ids.join(", "));
    }
    print!(
        "Install this module{}? [y/N] ",
        if manifest.dependencies.is_empty() { "" } else { " and its dependencies" }
    );
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        println!("Cancelled.");
        return Ok(());
    }

    // Offer the optional dependencies (opt-in): declining still installs the module + its
    // required deps.
    let mut accepted_optional: std::collections::HashSet<String> = std::collections::HashSet::new();
    if !optional_ids.is_empty() {
        print!("Also install its optional dependencies ({})? [y/N] ", optional_ids.join(", "));
        std::io::stdout().flush().ok();
        let mut opt_answer = String::new();
        std::io::stdin().read_line(&mut opt_answer)?;
        if matches!(opt_answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            accepted_optional.extend(optional_ids);
        }
    }

    // Resolve + fetch the whole dependency tree, not just this repo.
    let installed = registry::install_tree(full_name, &accepted_optional)?;
    println!("Installed {} module(s) into {}:", installed.len(), registry::modules_dir().display());
    for m in &installed {
        println!("  {} v{} ({})", m.name, m.version, m.id);
    }
    Ok(())
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
                println!("Updating {} ({}): v{} -> v{new_version}...", m.id, src.repo, m.version);
                registry::install(&src.repo)?;
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
