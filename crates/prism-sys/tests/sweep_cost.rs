//! What it costs to look for a screen reader that is not there.
//!
//! The speech layer does this on a timer after losing one, so the interval has to be chosen
//! against a measured number rather than a guess. The backends that are absent on this
//! machine are the honest sample: every one of them does the full "are you there" work and
//! answers no, which is exactly the case being timed.
#![cfg(windows)]

use std::time::Instant;

use prism_sys::{Context, SCREEN_READERS};

#[test]
fn looking_for_an_absent_screen_reader_is_cheap() {
    let start = Instant::now();
    let ctx = Context::open().expect("prism did not initialise");
    let opened = Instant::now();

    let mut absent = 0;
    let mut worst = std::time::Duration::ZERO;
    let mut worst_name = "";
    for name in SCREEN_READERS {
        let each = Instant::now();
        let found = ctx.open_backend(name).is_ok();
        let took = each.elapsed();
        if !found {
            absent += 1;
            if took > worst {
                worst = took;
                worst_name = name;
            }
        }
        println!("  {name:<12} {:>7.2?}  {}", took, if found { "OPEN" } else { "absent" });
    }

    println!(
        "prism_init {:.2?}, {absent} absent backend(s), slowest absent: {worst_name} {worst:.2?}, \
         whole sweep {:.2?}",
        opened - start,
        start.elapsed()
    );

    // The number the retry interval is chosen against. Generous, because this is a shared
    // machine measurement and not a benchmark — it is here to catch a backend that starts
    // doing something expensive, not to police milliseconds.
    assert!(
        start.elapsed() < std::time::Duration::from_millis(250),
        "looking for a screen reader took {:?}, which is too much to do on a timer",
        start.elapsed()
    );
}
