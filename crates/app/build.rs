//! Embeds the Windows application manifest into the executable, and delay-loads the screen
//! reader DLLs prism imports.
//!
//! wxWidgets requires the app to declare a dependency on Common Controls v6 (otherwise it
//! warns at startup and falls back to legacy, less accessible controls).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let manifest_path = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows")
            .join("automation-platform.exe.manifest");
        println!("cargo:rerun-if-changed={}", manifest_path.display());
        println!("cargo:rustc-link-arg-bin=automation-platform=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=automation-platform=/MANIFESTINPUT:{}",
            manifest_path.display()
        );
        delay_load_prism_imports();
    }
}

/// Delay-loads the vendor DLLs prism's ZDSR, PC-Talker and Boy PC Reader backends import.
///
/// Those backends call their vendors' functions directly, resolved through import libraries
/// generated from .def files, so without this the executable **does not start** on a machine
/// that does not have those screen readers installed: `0xC0000135`, before `main`, for
/// everybody, for the sake of three readers almost nobody here runs.
///
/// It has to be said here rather than in `prism-sys` because cargo does not propagate a link
/// argument to a dependent's executable the way it propagates a native library. The DLL
/// names are not repeated, though: `prism-sys` publishes them, and cargo hands them to a
/// **direct** dependent as `DEP_PRISM_DELAYLOAD` — which is the only reason this crate names
/// `prism-sys` in its manifest at all.
fn delay_load_prism_imports() {
    println!("cargo:rerun-if-env-changed=DEP_PRISM_DELAYLOAD");
    let Ok(dlls) = std::env::var("DEP_PRISM_DELAYLOAD") else {
        return;
    };
    for dll in dlls.split(';').filter(|d| !d.is_empty()) {
        println!("cargo:rustc-link-arg-bin=automation-platform=/delayload:{dll}");
    }
}
