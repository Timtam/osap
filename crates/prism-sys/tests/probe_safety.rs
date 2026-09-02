//! Asking "are you there" must be safe to ask twice.
//!
//! It was not. `Context::is_available` creates a backend, reads its features and frees it,
//! which prism documents as legal without `initialize` — but OneCore's `get_features` calls
//! WinRT's `ApiInformation::IsTypePresent`, and those statics do not survive the
//! `CoUninitialize` that closing a context performs. The first probe was harmless and the
//! second killed the process with STATUS_ACCESS_VIOLATION.
//!
//! Found by measuring what a "what can speak" query would cost, which is the good luck in it:
//! an API that is safe once and fatal twice would have shipped and then crashed somebody's
//! session, on the machine of a person who cannot see the crash.
#![cfg(windows)]

use prism_sys::{Context, BACKENDS};

#[test]
fn every_backend_can_be_asked_from_a_second_context() {
    for round in 1..=3 {
        std::thread::spawn(move || {
            let ctx = Context::open().expect("context");
            for name in BACKENDS {
                let _ = ctx.is_available(name);
            }
        })
        .join()
        .unwrap_or_else(|_| panic!("round {round} died — a backend cannot survive being asked twice"));
    }
}
