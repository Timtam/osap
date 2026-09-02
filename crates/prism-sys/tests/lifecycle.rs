//! Which library lifecycles are survivable — asked one at a time, because a crash answers
//! only the question it was asked.
//!
//! Found the hard way: opening a context, dropping it, and opening another on a fresh thread
//! killed the process with STATUS_ACCESS_VIOLATION. The speech layer does exactly that every
//! time a search for a returning screen reader comes up empty, so this is not academic.
//!
//! Each case is its own test, so the harness runs it in its own thread and one crash does not
//! hide the others.
#![cfg(windows)]

use prism_sys::Context;

/// The application's normal state: the screen-reader worker and the plain voice, side by side.
#[test]
fn two_at_once_on_two_threads() {
    let a = std::thread::spawn(|| {
        let ctx = Context::open().expect("first context");
        std::thread::sleep(std::time::Duration::from_millis(200));
        ctx.backend_names().len()
    });
    let b = std::thread::spawn(|| {
        let ctx = Context::open().expect("second context");
        std::thread::sleep(std::time::Duration::from_millis(200));
        ctx.backend_names().len()
    });
    assert_eq!(a.join().unwrap(), b.join().unwrap());
}

/// Open, close, open — on the SAME thread.
#[test]
fn one_after_another_on_one_thread() {
    for round in 1..=3 {
        let ctx = Context::open().unwrap_or_else(|e| panic!("round {round}: {e}"));
        let n = ctx.backend_names().len();
        assert!(n > 0, "round {round} saw no backends");
        drop(ctx);
    }
}

/// Open, close, open — each on a DIFFERENT thread. This is the one that crashed.
#[test]
fn one_after_another_on_fresh_threads() {
    for round in 1..=3 {
        let n = std::thread::spawn(|| {
            let ctx = Context::open().ok()?;
            Some(ctx.backend_names().len())
        })
        .join()
        .expect("the thread died");
        assert_eq!(n, Some(9), "round {round}");
    }
}
