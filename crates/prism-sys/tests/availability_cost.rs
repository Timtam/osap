//! What an honest "what can speak" list costs.
#![cfg(windows)]

use std::time::Instant;

use prism_sys::{Context, BACKENDS};

#[test]
fn asking_what_is_available_is_cheap() {
    let ctx = Context::open().expect("prism did not initialise");
    let start = Instant::now();
    for name in BACKENDS {
        let each = Instant::now();
        let there = ctx.is_available(name);
        println!("  {name:<12} {:>9.2?}  {}", each.elapsed(), if there { "available" } else { "no" });
    }
    let whole = start.elapsed();
    println!("the whole list: {whole:.2?}");
    assert!(
        whole < std::time::Duration::from_millis(500),
        "asking what can speak took {whole:?} — too much for a question a module may ask"
    );
}
