//! Embeds the Windows application manifest into the executable. wxWidgets
//! requires the app to declare a dependency on Common Controls v6 (otherwise it
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
    }
}
