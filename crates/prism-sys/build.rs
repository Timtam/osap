//! Compiles prism from `vendor/` and installs it into `OUT_DIR`.
//!
//! Everything unusual in here is a guard against a failure that is silent. Three of them
//! were found by building the library and running the result rather than by reading it:
//!
//!   * a wrong `CMAKE_MSVC_RUNTIME_LIBRARY` gives two C runtimes in one process, announced
//!     only by an `LNK4098` buried in cargo's output;
//!   * a mistyped or upstream-renamed option is a *warning* in CMake and exit 0, so the
//!     backend you asked for is simply not built and nothing says so;
//!   * and the delay-load flags for the vendor-DLL backends are not optional — without them
//!     the application does not start at all.
//!
//! So this script asserts its own outcome afterwards instead of trusting that the arguments
//! it passed had the effect it wanted.

use std::path::{Path, PathBuf};

/// What gets compiled in, and what deliberately does not.
///
/// `uia` is off because it "speaks" by raising a UI Automation notification: it initialises
/// on any window this process owns, returns success, and makes no sound when nothing is
/// listening. A success code for silence is the one thing this application cannot have.
///
/// `system_access` and `window_eyes` are off because they are the only two backends prism
/// marks `LEGACY`, and dropping them lets the legacy path go with them. Losing System Access
/// is a real regression — `package.ps1` ships `SAAPI64.dll` today precisely for it.
///
/// The four readers nobody here can test — `zdsr`, `pc_talker`, `boy_pc_reader`,
/// `sense_reader` — are on. Not being able to test them is a reason for the delay-load
/// guard below, not a reason to withhold them from the people who use them.
const BACKENDS: &[(&str, bool)] = &[
    ("nvda", true),
    ("jaws", true),
    ("zoom_text", true),
    ("sapi", true),
    ("onecore", true),
    ("zdsr", true),
    ("pc_talker", true),
    ("boy_pc_reader", true),
    ("sense_reader", true),
    ("uia", false),
    ("system_access", false),
    ("window_eyes", false),
];

fn main() {
    // Emitting any rerun line at all stops cargo defaulting to "re-run if anything in the
    // package changed", which is what keeps a no-op build instant despite `vendor/`.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=vendor");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        // Before anything reads `vendor/`, which on a Mac runner is an empty directory.
        return;
    }
    if !cfg!(windows) {
        panic!(
            "prism-sys targets Windows but is being built from a {} host. There is no \
             cross-compilation path: prism needs MSVC, the Windows SDK and midl.exe.",
            std::env::consts::OS
        );
    }

    #[cfg(windows)]
    windows::build();
}

#[cfg(windows)]
mod windows {
    use super::*;

    pub fn build() {
        let vendor = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("vendor");
        if !vendor.join("CMakeLists.txt").is_file() {
            panic!(
                "crates/prism-sys/vendor is empty. It is a git submodule:\n\n    \
                 git submodule update --init crates/prism-sys/vendor\n"
            );
        }

        let midl = find_midl();
        let mut cfg = cmake::Config::new(&vendor);
        let mut defined: Vec<(String, String)> = Vec::new();
        let mut define = |cfg: &mut cmake::Config, key: &str, value: &str| {
            cfg.define(key, value);
            defined.push((key.to_string(), value.to_string()));
        };

        // Release regardless of cargo's profile. prism is a third-party library nobody here
        // steps through, a debug build of it is far slower to compile and to run, and its
        // own default runtime-library setting is a generator expression that resolves to the
        // *static debug* CRT under Debug — which is the two-CRT trap, with mismatched
        // iterator debug levels on top.
        cfg.profile("Release");

        // Explicitly, and not through the `cmake` crate's `static_crt` helper: that one sets
        // a bare compiler flag which prism's own cache variable then overrides.
        define(&mut cfg, "CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDLL");
        // Upstream defaults this to ON, which would produce a prism.dll to ship beside the
        // executable. The whole point here is that there is nothing to ship.
        define(&mut cfg, "BUILD_SHARED_LIBS", "OFF");

        // prism's own CMake requires MIDL unconditionally on Windows and, under the Ninja
        // generator, never finds it — its search keys off a variable only the Visual Studio
        // generator defines, so it works solely because a developer shell put the SDK on
        // PATH. Pre-seeding the `find_program` cache variable turns somebody else's
        // FATAL_ERROR into the message above.
        define(&mut cfg, "PRISM_MIDL_COMPILER", &midl.to_string_lossy());

        for (name, on) in BACKENDS {
            let key = format!("PRISM_ENABLE_{}_BACKEND", name.to_uppercase());
            define(&mut cfg, &key, if *on { "ON" } else { "OFF" });
        }

        // Everything that is not a backend and not the library itself.
        for key in [
            "PRISM_ENABLE_TESTS",     // also the only place CPM is included, so this keeps
            "PRISM_ENABLE_DEMOS",     // the configure step offline
            "PRISM_ENABLE_GDEXTENSION",
            "PRISM_ENABLE_SHIMS",
            "PRISM_ENABLE_LINTING",
            "PRISM_ENABLE_LEGACY_BACKENDS",
        ] {
            define(&mut cfg, key, "OFF");
        }

        if let Some(ninja) = which("ninja.exe") {
            // Measured at 26 s for the full library. Not forced: a machine without ninja
            // should fall back to whatever generator CMake picks rather than fail.
            let _ = ninja;
            cfg.generator("Ninja");
        }

        // CMake reconfigures in place quite happily, and leaves the targets it no longer
        // builds behind as directories. That turns the check below into a reading of
        // everything ever built here rather than of what this configuration produced — which
        // is both a false alarm when the list shrinks and, worse, a place for a backend that
        // silently dropped out to hide. So a changed configuration starts from nothing.
        let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        discard_stale_build(&out, &defined);

        let dst = cfg.build();
        let build_dir = dst.join("build");

        assert_options_were_understood(&build_dir, &defined);
        assert_backends_built(&build_dir);
        assert_dynamic_crt(&dst.join("lib").join("prism.lib"));

        let header = dst.join("include").join("prism.h");
        assert!(
            header.is_file(),
            "prism installed without its header: {}",
            header.display()
        );

        generate_bindings(&dst);
        emit_link_directives(&dst);

        accept_build(&out);
    }

    /// Declares prism's C API from the header that was just compiled.
    ///
    /// Allowlisted, because `prism.h` includes `<windows.h>` on `_WIN32` and would otherwise
    /// bring the whole Win32 API along. `PRISM_STATIC` matters: without it every declaration
    /// comes back marked `__declspec(dllimport)`, which is the opposite of how this is linked.
    fn generate_bindings(dst: &Path) {
        ensure_libclang();
        let header = dst.join("include").join("prism.h");
        let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("prism.rs");

        bindgen::Builder::default()
            .header(header.to_string_lossy())
            // `prism.h` includes the generated `<prism_version.h>` with angle brackets, so
            // its own directory has to be on the search path even though it is right there.
            .clang_arg(format!("-I{}", dst.join("include").display()))
            .clang_arg("-DPRISM_STATIC")
            .allowlist_function("prism_.*")
            .allowlist_type("Prism.*")
            .allowlist_var("PRISM_.*")
            // Its members are 64-bit, its underlying type is not: `sizeof` is 4, and the
            // last member — `1 << 63` — silently evaluates to 0. `prism_backend_get_features`
            // returns a `uint64_t`, so the bits this crate uses are declared by hand as u64
            // in lib.rs and the enum itself is left out rather than half-imported.
            .blocklist_type("PrismBackendFeature")
            .derive_default(true)
            .layout_tests(true)
            .generate()
            .expect("bindgen could not read prism.h")
            .write_to_file(&out)
            .expect("cannot write the generated bindings");
    }

    /// Points bindgen at Visual Studio's own LLVM if the shell did not.
    ///
    /// `run-dev.ps1` and `package.ps1` already do exactly this for wxdragon, so a build from
    /// a plain shell works there and would otherwise stop here. There is deliberately no
    /// fallback to a checked-in bindings file: a missing libclang must be a loud failure,
    /// because the quiet alternative is declarations that no longer match the library.
    fn ensure_libclang() {
        println!("cargo:rerun-if-env-changed=LIBCLANG_PATH");
        if std::env::var_os("LIBCLANG_PATH").is_some() {
            return;
        }
        let Some(root) = std::env::var_os("ProgramFiles") else {
            return;
        };
        let vs = Path::new(&root).join("Microsoft Visual Studio");
        // <year>/<edition>/VC/Tools/Llvm/x64/bin, e.g. 18/Community.
        let Ok(years) = std::fs::read_dir(&vs) else {
            return;
        };
        let mut found = Vec::new();
        for year in years.filter_map(|e| e.ok()) {
            let Ok(editions) = std::fs::read_dir(year.path()) else {
                continue;
            };
            for edition in editions.filter_map(|e| e.ok()) {
                let bin = edition
                    .path()
                    .join("VC")
                    .join("Tools")
                    .join("Llvm")
                    .join("x64")
                    .join("bin");
                if bin.join("libclang.dll").is_file() {
                    found.push(bin);
                }
            }
        }
        found.sort();
        if let Some(bin) = found.pop() {
            std::env::set_var("LIBCLANG_PATH", bin);
        }
    }

    /// What the final executable has to link, and how.
    fn emit_link_directives(dst: &Path) {
        println!("cargo:rustc-link-search=native={}", dst.join("lib").display());

        // `+whole-archive`, and this is the single most important line in the file. prism's
        // backends register themselves from static initialisers in per-backend object files,
        // and an ordinary static link keeps only the objects that resolve a symbol — which
        // none of those do. The result compiles, links, runs, exits 0 and reports ZERO
        // backends. prism's own CMake hides this behind a WHOLE_ARCHIVE generator expression
        // on its interface target, which nothing here ever reads.
        println!("cargo:rustc-link-lib=static:+whole-archive=prism");

        // Declared by prism only as CMake interface properties, which a cargo link never
        // sees. Everything else prism needs is already claimed by directives inside the
        // object files themselves.
        for lib in ["rpcrt4", "delayimp", "onecore", "PowrProf"] {
            println!("cargo:rustc-link-lib=dylib={lib}");
        }

        // The three readers that live behind a vendor DLL. Their functions are called
        // directly — `Speak`, `GetSpeakState` — and resolved through an import library
        // generated from a .def file, so without delay-loading the executable does not start
        // at all on a machine that does not have them: 0xC0000135, before `main`, for
        // everybody, for the sake of three screen readers almost nobody here runs.
        //
        // prism links these itself under `$<BUILD_INTERFACE:>`, which means an installed
        // prism does not carry them: they have to be named again on this side.
        for stub in ["ZDSR", "byctrl", "PCTalker"] {
            println!("cargo:rustc-link-lib=dylib={stub}");
        }
        let dlls = ["ZDSRAPI_x64.dll", "byctrl-x64.dll", "PCTKUSR.dll"];
        for dll in dlls {
            // Unqualified rather than `-tests`, which covers only integration tests and
            // leaves the lib's own unit-test binary to fail at start-up — which is exactly
            // what it did. This covers every executable target of THIS crate; cargo does
            // not propagate a link argument to a dependent's executable the way it
            // propagates a native library, so the crate that produces the application
            // binary reads the list back out of the metadata below and repeats it there.
            println!("cargo:rustc-link-arg=/delayload:{dll}");
        }
        println!("cargo:delayload={}", dlls.join(";"));
    }

    /// Wipes the build and install trees when the option set differs from last time.
    ///
    /// One full rebuild costs about half a minute. Reasoning about which leftovers are still
    /// true costs more than that the first time it is wrong.
    fn discard_stale_build(out: &Path, defined: &[(String, String)]) {
        let mut lines: Vec<String> = defined.iter().map(|(k, v)| format!("{k}={v}")).collect();
        lines.sort();
        let fingerprint = lines.join("
");
        let stamp = out.join("prism-config.fingerprint");

        if std::fs::read_to_string(&stamp).ok().as_deref() == Some(fingerprint.as_str()) {
            return;
        }
        for dir in ["build", "lib", "include", "share"] {
            let _ = std::fs::remove_dir_all(out.join(dir));
        }
        // Written only after the build succeeds, so an interrupted one starts clean again.
        std::fs::write(out.join("prism-config.fingerprint.pending"), &fingerprint)
            .expect("cannot write the configuration fingerprint");
        let _ = std::fs::remove_file(&stamp);
    }

    /// Records the configuration that produced the tree now on disk.
    fn accept_build(out: &Path) {
        let pending = out.join("prism-config.fingerprint.pending");
        if pending.is_file() {
            std::fs::rename(&pending, out.join("prism-config.fingerprint"))
                .expect("cannot record the configuration fingerprint");
        }
    }

    /// Fails if CMake ignored any option this script passed.
    ///
    /// An unknown `-D` is a warning CMake prints and then exits 0 on, leaving the variable in
    /// the cache with the type `UNINITIALIZED`. Upstream renames these: configuring v0.18.2
    /// with two option names that were real at v0.17.0 succeeded and silently built a
    /// different library. So every name is checked for a *typed* cache entry.
    fn assert_options_were_understood(build_dir: &Path, defined: &[(String, String)]) {
        let cache_path = build_dir.join("CMakeCache.txt");
        let cache = std::fs::read_to_string(&cache_path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", cache_path.display()));

        let mut ignored = Vec::new();
        for (key, _) in defined {
            // Entries look like `NAME:BOOL=ON`; an ignored one is `NAME:UNINITIALIZED=ON`.
            let typed = cache.lines().find_map(|l| {
                l.strip_prefix(key.as_str())
                    .and_then(|rest| rest.strip_prefix(':'))
                    .and_then(|rest| rest.split('=').next())
            });
            match typed {
                Some("UNINITIALIZED") | None => ignored.push(key.clone()),
                Some(_) => {}
            }
        }
        assert!(
            ignored.is_empty(),
            "prism ignored {} option(s) this build script set, so the library built is not \
             the one it asked for: {}.\nThe upstream option names have probably changed — \
             re-derive them from vendor/cmake/ and update BACKENDS or the list beside it.",
            ignored.len(),
            ignored.join(", ")
        );
    }

    /// Fails unless exactly the wanted backends were built.
    ///
    /// A backend can also drop out without any option being wrong: prism skips one whose
    /// platform or architecture does not match, and says so only in a summary line. This
    /// reads what was actually generated.
    fn assert_backends_built(build_dir: &Path) {
        let dir = build_dir.join("CMakeFiles");
        let mut built: Vec<String> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.strip_prefix("prism_backend_")
                    .and_then(|n| n.strip_suffix(".dir"))
                    .map(|n| n.to_string())
            })
            .collect();
        built.sort();

        let mut wanted: Vec<String> = BACKENDS
            .iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| n.to_string())
            .collect();
        wanted.sort();

        assert_eq!(
            built,
            wanted,
            "prism built a different set of backends than this script asked for.\n  built:  \
             {}\n  wanted: {}",
            built.join(", "),
            wanted.join(", ")
        );
    }

    /// Fails if prism was compiled against the static C runtime.
    ///
    /// rustc's MSVC target always links the dynamic CRT, and two runtimes in one process
    /// means two heaps: an allocation made on one side and freed on the other corrupts the
    /// process, usually much later and somewhere unrelated. The linker says `LNK4098` and
    /// carries on, so this is asserted instead of hoped for.
    ///
    /// The archive carries its linker directives as plain text, which is why this needs no
    /// dumpbin and no extra dependency. They are quoted — `/DEFAULTLIB:"MSVCRT"` — and the
    /// quotes are what tells `MSVCRT` apart from the debug `MSVCRTD`, so they are matched
    /// rather than skipped over.
    fn assert_dynamic_crt(lib: &Path) {
        let bytes = std::fs::read(lib)
            .unwrap_or_else(|e| panic!("prism.lib missing at {}: {e}", lib.display()));
        let claims = |name: &str| {
            let needle = format!("DEFAULTLIB:\"{name}\"");
            bytes
                .windows(needle.len())
                .any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))
        };
        for runtime in ["LIBCMT", "LIBCMTD"] {
            assert!(
                !claims(runtime),
                "prism was built against the STATIC C runtime ({runtime}); rustc links the \
                 dynamic one. Two CRTs in one process is two heaps: memory allocated on one \
                 side and freed on the other corrupts the process, later and somewhere else. \
                 Check CMAKE_MSVC_RUNTIME_LIBRARY."
            );
        }
        assert!(
            claims("MSVCRT"),
            "prism.lib claims no dynamic C runtime at all, so it was built in a way this \
             script does not understand. Compare `dumpbin -directives` against what \
             CMAKE_MSVC_RUNTIME_LIBRARY was set to."
        );
    }

    /// `midl.exe`, from PATH if a developer shell provided one, otherwise from the newest
    /// Windows SDK on disk.
    fn find_midl() -> PathBuf {
        if let Some(p) = which("midl.exe") {
            return p;
        }
        // The SDK lays its tools out as bin/<10.0.x.y>/<arch>/midl.exe. There is a
        // registry key for the root, but reading it would cost a dependency for something
        // that has been in the same two places for a decade.
        let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
        let roots = [
            std::env::var("ProgramFiles(x86)").ok(),
            std::env::var("ProgramFiles").ok(),
        ];
        let mut candidates: Vec<PathBuf> = Vec::new();
        for root in roots.into_iter().flatten() {
            let bin = Path::new(&root).join("Windows Kits").join("10").join("bin");
            let Ok(entries) = std::fs::read_dir(&bin) else {
                continue;
            };
            for e in entries.filter_map(|e| e.ok()) {
                let midl = e.path().join(arch).join("midl.exe");
                if midl.is_file() {
                    candidates.push(midl);
                }
            }
        }
        // Directory names sort as versions well enough here: 10.0.22621.0 < 10.0.26100.0.
        candidates.sort();
        candidates.pop().unwrap_or_else(|| {
            panic!(
                "midl.exe not found. prism generates NVDA's RPC client from an IDL file, so \
                 the Windows SDK is required, as is the Visual Studio 'C++ ATL' component \
                 (prism.lib asks the linker for atls.lib). Both come from the Visual Studio \
                 installer."
            )
        })
    }

    /// The first `name` on PATH.
    fn which(name: &str) -> Option<PathBuf> {
        std::env::var_os("PATH").and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(name))
                .find(|p| p.is_file())
        })
    }
}
