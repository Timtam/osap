//! The declarations macOS does not hand us through a crate.
//!
//! Two things live here, and both are unverifiable from a Windows machine: `cargo check`
//! type-checks an `extern` block but never links it, so every signature below has to be
//! right by construction rather than by experiment. They are transcribed from Apple's
//! headers and cross-checked against the type shims in objc2's own bindings.
//!
//! - **Carbon hotkeys.** `RegisterEventHotKey` is the proven way to hold a global shortcut
//!   on macOS: it needs no permission and cannot be silently switched off the way an event
//!   tap can. No crate in the objc2 family exposes it.
//! - **`_AXUIElementGetWindow`.** The one function that pairs an accessibility element with
//!   the window id capture needs. It is private, it is what every window manager on the
//!   platform uses, and the fallback when it is missing is matching by geometry.
