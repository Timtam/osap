//! The pure half of `host`'s `ocr` folder, borrowed file by file.
//!
//! The macOS backend names these types — the capture plan, the pieces a read is made of — so
//! they have to exist at `crate::ocr` here exactly as they do in `host`. Only the files that
//! need nothing but the standard library and `fluent-langneg` are borrowed; the threads
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
