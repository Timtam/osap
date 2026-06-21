//! Walking-skeleton entry: loads a module (default `modules/hello`) and runs it.

use anyhow::Result;

fn main() -> Result<()> {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "modules/hello".to_string());

    println!("== Automation Platform — Walking Skeleton ==");
    host::run_module(&dir)?;
    println!("== done ==");
    Ok(())
}
