//! Entry point. Without a subcommand, loads one or more module directories
//! (default `modules/hello`) and runs them under the manager. The management
//! subcommands discover/install/update modules from GitHub (registry).

use std::io::Write;

use anyhow::Result;
use host::registry;

fn main() -> Result<()> {
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
                let installed: Vec<String> = registry::installed()
                    .into_iter()
                    .map(|m| m.dir.to_string_lossy().into_owned())
                    .collect();
                if installed.is_empty() {
                    vec!["modules/hello".to_string()]
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

    // Resolve + fetch the whole dependency tree, not just this repo.
    let installed = registry::install_tree(full_name)?;
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
