//! Permissions, and saying out loud which ones are missing.
//!
//! Three switches decide whether this application can do anything at all, none of them
//! grantable programmatically, and two of them fail in ways that look like a bug rather
//! than a permission: without Accessibility every element read comes back empty, and
//! without Screen Recording a capture does not fail — it returns a picture of the
//! wallpaper. For a blind tester on a machine nobody here owns, a silent wrong answer is
//! the worst possible outcome, so everything here ends up in the log by name.

/// Asks for Accessibility once, with the system prompt.
pub fn request_accessibility_once() {}

/// The startup environment block: displays and their scale, the three permissions, the
/// macOS version, whether VoiceOver is running.
pub fn environment_report() -> Vec<(String, String)> {
    Vec::new()
}
