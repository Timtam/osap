//! Entry: loads one or more modules (default `modules/hello`) into one process
//! and runs them together under the manager.

use anyhow::Result;

fn main() -> Result<()> {
    let dirs: Vec<String> = std::env::args().skip(1).collect();
    let dirs = if dirs.is_empty() {
        vec!["modules/hello".to_string()]
    } else {
        dirs
    };

    host::run(&dirs)?;
    Ok(())
}
