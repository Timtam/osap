//! Delay-loads the screen reader DLLs prism imports, for this crate's own test binaries.
//!
//! **Every executable that links prism has to say this for itself.** Cargo propagates a
//! native library to a dependent, but not a link argument — so `prism-sys` covers its own
//! tests, `crates/app/build.rs` covers the application, and this covers `cargo test -p host`.
//! Forgetting one is not subtle: that executable does not start at all, `0xC0000135` before
//! `main`, which is how this file came to exist.
//!
//! The DLL names are not repeated anywhere. `prism-sys` publishes them, and cargo hands them
//! to a **direct** dependent as `DEP_PRISM_DELAYLOAD`. See `docs/prism-speech-design.md`.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DEP_PRISM_DELAYLOAD");

    let Ok(dlls) = std::env::var("DEP_PRISM_DELAYLOAD") else {
        return;
    };
    for dll in dlls.split(';').filter(|d| !d.is_empty()) {
        // Unqualified: `-tests` means a `tests/` directory, which this crate does not have,
        // and cargo rejects the instruction outright rather than ignoring it. The plain form
        // covers every executable target this package does have — which here is the lib's
        // own unit-test binary, the one that was failing to start.
        println!("cargo:rustc-link-arg=/delayload:{dll}");
    }
}
