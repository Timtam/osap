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

#[path = "../../host/src/backend/mod.rs"]
pub mod backend;

/// The VoiceOver speech path, borrowed for the same reason — it talks to an application
/// that does not exist on this machine, through a binary that does not either.
#[path = "../../host/src/speech/voiceover.rs"]
pub mod voiceover;

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
