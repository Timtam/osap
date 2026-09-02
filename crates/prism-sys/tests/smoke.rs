//! What the compiler cannot check about a C++ library compiled from source.
//!
//! **Every test here also proves something before its first line runs.** This is a real
//! executable linking prism statically, so if the delay-load flags for the vendor-DLL
//! backends were ever dropped, it would not start at all — `0xC0000135`, before `main` —
//! and the whole file would fail rather than one assertion.
//!
//! These run wherever somebody runs them, which for now is a development machine. That is
//! worth saying out loud: none of this is guarded by CI yet.
#![cfg(windows)]

use prism_sys::{feature, Context, Error, BACKENDS};

/// The one that catches a silent mute.
///
/// prism's backends register from static initialisers in per-backend object files, so an
/// ordinary static link keeps none of them: the library then compiles, links, runs, exits 0
/// and offers **zero** backends. `build.rs` links it `+whole-archive` to prevent that.
///
/// Names, not a count — a count of nine passes in any configuration that happens to build
/// nine things, including the wrong nine.
#[test]
fn every_configured_backend_is_present() {
    let ctx = Context::open().expect("prism did not initialise");
    let mut found = ctx.backend_names();
    found.sort();
    let mut wanted: Vec<String> = BACKENDS.iter().map(|s| s.to_string()).collect();
    wanted.sort();
    assert_eq!(
        found, wanted,
        "the linked library does not offer the backends this build asked for. Zero here \
         means the +whole-archive modifier was lost."
    );
}

/// Nothing to say is not an error, and must never be treated as one.
///
/// Measured: `prism_backend_speak(b, "", false)` returns `INVALID_UTF8`. An empty string
/// reaches `host.speech.output` today from `modules/daw-hosts` whenever a plug-in window has
/// no title, and the speech layer demotes the screen-reader path on the first failure — so
/// without this, one untitled window would silence NVDA for the rest of the session.
#[test]
fn empty_text_never_reaches_prism() {
    let ctx = Context::open().expect("prism did not initialise");
    let Some(backend) = any_working_backend(&ctx) else {
        eprintln!("no backend initialises on this machine; nothing to say to");
        return;
    };
    for nothing in ["", " ", "\t\r\n", "\0"] {
        assert_eq!(
            backend.speak(nothing, false),
            Ok(()),
            "asked to say nothing, and reported a failure instead of doing nothing"
        );
    }
}

/// `is_speaking` is usually unavailable, and that is not a failure.
///
/// Measured: NVDA leaves the `IS_SPEAKING` bit clear and answers `NOT_IMPLEMENTED` unless
/// the running copy exposes a newer RPC interface. A speech layer that called it in its
/// wait loop and demoted on error would drop a perfectly healthy screen reader on the first
/// iteration, which is why the layer counts its own outstanding utterances instead.
#[test]
fn unsupported_is_speaking_is_distinguishable_from_a_real_failure() {
    let ctx = Context::open().expect("prism did not initialise");
    let Some(backend) = any_working_backend(&ctx) else {
        return;
    };
    match backend.is_speaking() {
        Ok(_) => assert!(
            backend.supports(feature::IS_SPEAKING),
            "answered is_speaking without advertising it"
        ),
        Err(e) => assert!(
            e.is_unsupported(),
            "is_speaking failed for a reason other than not being implemented: {e}"
        ),
    }
}

/// A backend that opens must claim it is supported right now.
///
/// The whole automatic return to the screen reader rests on this: prism refuses a backend
/// whose reader is not running rather than handing back a dead one. Measured on a machine
/// with NVDA up — JAWS, ZoomText, ZDSR, PC-Talker, Boy PC Reader and Sense Reader all came
/// back `BACKEND_NOT_AVAILABLE` (16), NVDA opened with `features=0xb5`. If that ever stopped
/// being true, retrying would adopt a dead backend and the user would hear nothing.
#[test]
fn a_backend_that_opens_says_it_is_available() {
    let ctx = Context::open().expect("prism did not initialise");
    for name in BACKENDS {
        if let Ok(b) = ctx.open_backend(name) {
            assert!(
                b.supports(feature::IS_SUPPORTED_AT_RUNTIME),
                "{name} opened without claiming to be supported at runtime, so the fact that \
                 it opened is no longer evidence that its screen reader is there"
            );
        }
    }
}

/// A rejected string and a dead screen reader must not look alike.
#[test]
fn caller_faults_are_told_apart_from_backend_faults() {
    assert!(Error::INVALID_UTF8.is_our_fault());
    assert!(!Error::BACKEND_NOT_AVAILABLE.is_our_fault());
    assert!(Error::NOT_IMPLEMENTED.is_unsupported());
    assert!(!Error::INVALID_UTF8.is_unsupported());
}

/// Opening the library must not cost the user a visible pause.
///
/// This is the regression test for `create_best` creeping back in. Initialising OneCore was
/// measured at 2.9 s, and `create_best` reaches it whenever no screen reader is running —
/// which would be a three-second freeze during start-up, before any window exists.
#[test]
fn opening_the_library_is_immediate() {
    let start = std::time::Instant::now();
    let ctx = Context::open().expect("prism did not initialise");
    let _ = ctx.backend_names();
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "opening prism took {elapsed:?}; something is initialising a speech engine that \
         should only be initialised when there is something to say"
    );
}

/// Whichever backend this machine can actually initialise, if any.
fn any_working_backend(ctx: &Context) -> Option<prism_sys::Backend> {
    BACKENDS
        .iter()
        .find_map(|name| ctx.open_backend(name).ok())
        .filter(|b| b.supports(feature::SPEAK))
}
