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
    print!("Install this module? [y/N] ");
    std::io::stdout().flush().ok();
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        println!("Cancelled.");
        return Ok(());
    }

    let installed = registry::install(full_name)?;
    println!("Installed {} into {}", installed.id, registry::modules_dir().display());
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
        if registry::update_available(m).is_some() {
            if let Some(src) = &m.source {
                println!("Updating {} ({})...", m.id, src.repo);
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
    if registry::uninstall(id)? {
        println!("Uninstalled {id}.");
    } else {
        println!("No installed module with id '{id}'.");
    }
    Ok(())
}
