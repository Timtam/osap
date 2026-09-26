//! `host.ocr.read`: text recognition off the event loop.
//!
//! The pure parts — what a read is (`types`), its limits (`policy`), which language it reads
//! (`lang`), how its regions are photographed (`plan`), who goes next (`sched`) and what a
//! module is handed (`pipeline`) — depend on nothing but the standard library and
//! `fluent-langneg`, and `crates/macos-check` borrows them. So are the snapshots that share the
//! capture thread: the change wait (`change`) and their lane beside the text reads
//! (`snap_queue`), which read the frame type besides. The threads live in `service`, the Luau
//! side in `lua` (and `crate::snapshot` for the snapshots).

pub mod change;
pub mod lang;
pub mod lua;
pub mod pipeline;
pub mod plan;
pub mod policy;
pub mod sched;
pub mod service;
pub mod snap_queue;
pub mod types;
