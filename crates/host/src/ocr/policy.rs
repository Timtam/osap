//! Every limit and timing of `host.ocr.read`, and of the snapshots and change waits taken on its
//! capture thread, in one place, so the documentation, the tests and the code quote the same
//! numbers. Std only; borrowed by `crates/macos-check`.

use std::time::Duration;

/// A region up to this size gets the small-text treatment on both platforms: cropped to its
/// content, upscaled, the blank guard, and on Windows the second recogniser. Shared so the two
/// platforms draw the line in the same place — it is observable from Luau.
pub const SMALL_W: i32 = 400;
pub const SMALL_H: i32 = 200;

/// Regions in one call.
pub const MAX_REGIONS: usize = 64;

/// Pixels in one call, all regions together: about ten 4K screens. Corners past it are a
/// mistake in the call (a corner mistyped into the millions) and raise; a window region that
/// would take the call past it, at the window's size then, is answered `"failed"` instead.
pub const MAX_CALL_PIXELS: i64 = 40_000_000;

/// Reads of one module that may be waiting or running at once. The 17th evicts that module's
/// oldest unkeyed read, or is refused when every waiting read has a key.
pub const PER_OWNER: usize = 16;

/// Jobs waiting or running in the whole application.
pub const TOTAL: usize = 64;

/// A background read that has waited this long goes before interactive ones — to be
/// photographed, and to be recognised.
pub const AGING: Duration = Duration::from_millis(500);

/// A recognition that has answered no region for this long counts as a hang: new reads fail
/// at once with a reason until it answers one. Counted per region, not per job, so that 64
/// small regions at a quarter of a second each are not a hang.
pub const HANG: Duration = Duration::from_secs(5);

/// One capture of the regions' bounding box instead of one per region, when the box is no
/// larger than this...
pub const BBOX_MAX: i64 = 8_000_000;

/// ...and no more than this many times the regions' own area (or `BBOX_FLOOR`, whichever is
/// larger — tiny read-outs a few pixels apart are one capture whatever the ratio says).
pub const BBOX_WASTE: i64 = 4;
pub const BBOX_FLOOR: i64 = 65_536;

/// Pixels captured and not yet recognised, in bytes, above which background captures wait.
/// Interactive captures always proceed.
pub const CAPTURED_BUDGET: usize = 256 * 1024 * 1024;

/// How long `host.input.*` and `host.window.focus` wait for the calling module's pending
/// capture to be taken.
pub const BARRIER: Duration = Duration::from_millis(50);

/// How long `languages()`, `resolveLanguage()` and a legacy `lang` wait for the language list
/// the recognise thread publishes first thing.
pub const LANG_WAIT: Duration = Duration::from_millis(50);

/// How often, at most, the language list is read again after a language did not resolve — a
/// language pack installed while the application runs is picked up without a restart.
pub const LANG_REREAD: Duration = Duration::from_secs(30);

/// How long the exit waits for the two OCR threads.
pub const SHUTDOWN: Duration = Duration::from_secs(1);

/// A job slower than this, from the call to the answer, gets a log line naming where the time
/// went — at most one per module per `SLOW_LOG_EVERY`.
pub const SLOW_JOB: Duration = Duration::from_millis(100);
pub const SLOW_LOG_EVERY: Duration = Duration::from_secs(10);

// ── host.screen.snapshotAsync, on the same capture thread ────────────────────────────────────

/// Snapshot requests of one module VM waiting at once — plain, timed and change waits together.
/// The 17th ends that module's oldest one: the newest always proceeds, since the newest is the
/// press somebody is waiting for.
pub const SNAP_PER_OWNER: usize = 16;

/// Change waits of one module VM at once; the 5th ends that module's oldest wait.
pub const SNAP_WAITS_PER_OWNER: usize = 4;

/// Change waits in the whole application; the 17th is not started.
pub const SNAP_WAITS: usize = 16;

/// Snapshot requests in the whole application; the 65th is not taken.
pub const SNAP_TOTAL: usize = 64;

/// The region of a change wait may cover at most this many pixels: one 4K screen. A wait holds
/// three pictures of it at a time and compares one every round.
pub const MAX_WAIT_PIXELS: i64 = 8_294_400;

/// Rectangles one change wait may watch.
pub const MAX_WATCH: usize = 16;

/// The longest a change wait may run, and how far ahead of `host.now()` a timed snapshot may be
/// asked for.
pub const WAIT_MAX: Duration = Duration::from_secs(2);
pub const AT_MAX: Duration = Duration::from_secs(2);

/// A change wait's defaults: how long it runs, how far a channel may differ without the pixel
/// counting as changed, how many pixels must differ, and how long a change must stand still.
pub const WAIT_TIMEOUT: Duration = Duration::from_millis(500);
pub const WAIT_TOLERANCE: u8 = 16;
pub const WAIT_MIN_PIXELS: u32 = 4;
pub const WAIT_SETTLE: Duration = Duration::ZERO;

/// The requests of one round are taken in one capture of their union when it covers at most
/// this many pixels, and one capture each otherwise (the standard path; desktop duplication takes
/// them all in one request anyway).
pub const UNION_MAX: i64 = 2_000_000;

/// The least time from the start of one polling round of a change wait to the next, per way of
/// capturing. The standard Windows path costs a compositor frame per capture anyway; desktop
/// duplication hands out at most one new frame per display refresh; macOS captures through
/// ScreenCaptureKit, whose cost per call in a loop is not measured yet.
pub const MIN_ROUND_STANDARD: Duration = Duration::from_millis(8);
pub const MIN_ROUND_DUPLICATION: Duration = Duration::from_millis(16);
pub const MIN_ROUND_MACOS: Duration = Duration::from_millis(33);

/// A snapshot round slower than this, capture and comparison together, is logged — the first,
/// and then every hundredth.
pub const ROUND_SLOW: Duration = Duration::from_millis(50);

/// Whether `w` x `h` is a small region.
pub fn is_small(w: i32, h: i32) -> bool {
    w <= SMALL_W && h <= SMALL_H
}
