//! The pure half of `host`'s `ocr` folder, borrowed file by file.
//!
//! The macOS backend names these types — the capture plan, the pieces a read is made of — so
//! they have to exist at `crate::ocr` here exactly as they do in `host`. Only the files that
//! need nothing but the standard library, `fluent-langneg`, `toml` (the bench's picture
//! manifest) and `image` (the neural recogniser's preprocessing) are borrowed; the threads
//! (`service.rs`) and the Luau side (`lua.rs`) stay in `host`.
//!
//! A file of its own rather than an inline module in `lib.rs`: a `#[path]` inside an inline
//! module is resolved below a folder named after it, and on a Mac that folder has to exist.

#[path = "../../host/src/ocr/types.rs"]
pub mod types;

#[path = "../../host/src/ocr/policy.rs"]
pub mod policy;

#[path = "../../host/src/ocr/lang.rs"]
pub mod lang;

#[path = "../../host/src/ocr/plan.rs"]
pub mod plan;

#[path = "../../host/src/ocr/sched.rs"]
pub mod sched;

#[path = "../../host/src/ocr/pipeline.rs"]
pub mod pipeline;

/// The change wait and the snapshot lane that share the capture thread with the text reads:
/// pure, apart from the frame type, which on this target carries the macOS backing image.
#[path = "../../host/src/ocr/change.rs"]
pub mod change;

#[path = "../../host/src/ocr/snap_queue.rs"]
pub mod snap_queue;

/// `ocr-bench`'s pure half — the pictures (its `include_bytes!` paths are the file's own, so they
/// resolve here too), the statistics and what it prints — which the macOS half names.
#[path = "../../host/src/ocr/bench.rs"]
pub mod bench;

/// The neural recogniser's arithmetic around its model, and when two readings are the same;
/// `ocr-bench` judges readings with it. Std and `image`, which this crate declares for it.
#[path = "../../host/src/ocr/paddle_pre.rs"]
pub mod paddle_pre;

/// The shape of a small region's retry ladder — what the macOS recogniser is told and what
/// `ocr-bench`'s strategies vary.
#[path = "../../host/src/ocr/ladder.rs"]
pub mod ladder;

/// The neural recogniser's shadow — its readings beside the macOS ladder's, compared and counted —
/// which `backend/macos/ocr.rs` keeps.
#[path = "../../host/src/ocr/shadow.rs"]
pub mod shadow;

/// What a recognition cost, as the log says it — the wording and the arithmetic of the macOS
/// recogniser's cost lines, which `backend/macos/ocr.rs` names.
#[path = "../../host/src/ocr/cost.rs"]
pub mod cost;
