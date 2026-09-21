//! Nothing of its own — it borrows two files out of `host` and asks the compiler whether
//! they are true for macOS. See this crate's Cargo.toml for why that cannot be done by
//! checking `host` itself.
//!
//! The borrowed modules are declared at the CRATE ROOT on purpose: the backend writes to
//! `crate::logging`, and that path has to resolve to the same thing here as it does in
//! `host`, or the code would compile in one crate and not the other.
#![cfg(target_os = "macos")]

#[path = "../../host/src/appcfg.rs"]
pub mod appcfg;

#[path = "../../host/src/portable.rs"]
pub mod portable;

#[path = "../../host/src/logging.rs"]
pub mod logging;

/// Borrowed because `logging` writes the build into its header.
#[path = "../../host/src/build_info.rs"]
pub mod build_info;

#[path = "../../host/src/backend/mod.rs"]
pub mod backend;

/// The VoiceOver speech path, borrowed for the same reason — it talks to an application
/// that does not exist on this machine, through a binary that does not either.
/// The WHOLE speech module now, not just the two macOS files inside it.
///
/// It could not be borrowed before: it pulled in `tts`, which does not build for this
/// target from here. Removing `tts` in favour of `avspeech.rs` took that away — so the part
/// that was never checked at home, the WIRING (which transport a line goes to, what
/// `engines()` answers, what `use` accepts), is checked now. That wiring is where the
/// mistakes are; the two leaf files were only ever the easy half.
#[path = "../../host/src/speech/mod.rs"]
pub mod speech;

/// Names the backend so the check cannot pass by leaving it out of the build: without a
/// use, an unreferenced `mod` still compiles, but a typo'd `platform()` would not be caught.
pub fn checked() -> std::rc::Rc<dyn backend::Backend> {
    backend::platform()
}

/// Names the two application-level calls for the same reason `checked` names the backend.
///
/// They are called from `gui.rs`, which this crate cannot borrow (wxdragon), so without
/// this the re-export is unreferenced here — and an unreferenced re-export warns instead of
/// proving that the path resolves and the signatures are what the caller expects.
/// The permissions page's three calls, named for the same reason as `app_calls` below.
///
/// They live in `gui.rs`, which this crate cannot borrow (wxdragon), so without naming them
/// here the compiler would only tell us they are unused — and a signature that no longer
/// matches its caller would reach the macOS CI job as a link error, or a tester as a page
/// that does not build.
pub fn permission_calls() -> (
    fn() -> Vec<backend::Permission>,
    fn(&str) -> bool,
    fn(&str) -> bool,
) {
    (backend::permissions, backend::open_pane, backend::ask_for)
}

pub fn app_calls() -> (fn(), fn(bool, &str) -> bool, fn(), fn(), fn()) {
    (
        backend::activate_self,
        backend::set_regular,
        backend::note_frontmost_before_gui,
        backend::restore_frontmost_after_gui_start,
        // Called from the settings switch in `gui.rs`, which this crate cannot borrow, so
        // naming it here is the only thing that proves the path and the signature.
        backend::request_voiceover_automation,
    )
}
