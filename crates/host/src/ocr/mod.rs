//! `host.ocr.read`: text recognition off the event loop.
//!
//! The pure parts — what a read is (`types`), its limits (`policy`), which language it reads
//! (`lang`), how its regions are photographed (`plan`), who goes next (`sched`) and what a
//! module is handed (`pipeline`) — depend on nothing but the standard library and
//! `fluent-langneg`, and `crates/macos-check` borrows them. So are the snapshots that share the
//! capture thread: the change wait (`change`) and their lane beside the text reads
//! (`snap_queue`), which read the frame type besides. The threads live in `service`, the Luau
//! side in `lua` (and `crate::snapshot` for the snapshots). `bench` is no part of a read: it is
//! the pure half of the `ocr-bench` subcommand, which measures Vision on a Mac, and is borrowed
//! as well; so is `cost`, what the macOS recogniser's cost lines say.

/// `automation-platform ocr-bench`, its pure half: the pictures, the statistics, what it prints.
pub mod bench;
pub mod change;
/// What a recognition cost, as the log says it: the pure half of the macOS recogniser's cost lines.
pub mod cost;
pub mod lang;
pub mod lua;
pub mod pipeline;
pub mod plan;
pub mod policy;
pub mod sched;
pub mod service;
pub mod snap_queue;
pub mod types;
